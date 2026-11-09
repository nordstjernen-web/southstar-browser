//! Southstar — captures an embedder asks the renderer for: a PNG of a region of the document, and the page printed to a paged PDF.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::sync::atomic::{AtomicU64, Ordering};
use std::ffi::CString;

use southstar_cairo::Format;
use southstar_ipc::{atol, json_double, json_string, json_value};

use crate::engine::{self, MarginSlot, Page, PdfDocument, PdfLayout};

const MAX_SIDE: f64 = 30000.0;
const MAX_PIXELS: i64 = 1 << 26;

fn number(body: &[u8], key: &str) -> Option<f64> {
    json_value(body, key.as_bytes())
        .map(json_double)
        .filter(|v| v.is_finite())
}

fn flag(body: &[u8], key: &str) -> bool {
    json_value(body, key.as_bytes()).is_some_and(|v| atol(v) != 0)
}

fn text(body: &[u8], key: &str) -> Option<Vec<u8>> {
    json_value(body, key.as_bytes())
        .and_then(json_string)
        .map(|mut value| {
            value.retain(|&b| b != 0);
            value
        })
        .filter(|value| !value.is_empty())
}

fn device_extent(css: f64, scale: f64) -> c_int {
    (css * scale).ceil().clamp(1.0, MAX_SIDE) as c_int
}

pub fn screenshot(cur: &Page, body: &[u8]) -> Option<(c_int, c_int, Vec<u8>)> {
    let scale = number(body, "scale")
        .filter(|s| (0.1..=8.0).contains(s))
        .unwrap_or(1.0);
    cur.wait_images();
    let (page_w, page_h) = cur.page_size()?;
    let x = number(body, "x").unwrap_or(0.0).max(0.0);
    let y = number(body, "y").unwrap_or(0.0).max(0.0);
    let width = number(body, "width")
        .filter(|w| *w > 0.0)
        .unwrap_or(f64::from(page_w) - x);
    let height = number(body, "height")
        .filter(|h| *h > 0.0)
        .unwrap_or(f64::from(page_h) - y);
    let w = device_extent(width, scale);
    let h = device_extent(height, scale).min((MAX_PIXELS / i64::from(w)) as c_int);
    let stride = w.checked_mul(4)?;
    let mut pixels = vec![0u8; usize::try_from(stride).ok()? * usize::try_from(h).ok()?];
    if cur.render_argb32(x as c_int, y as c_int, w, h, scale, &mut pixels) != 0 {
        return None;
    }
    let png = southstar_cairo::encode_png(&pixels, Format::Argb32, w, h, stride)?;
    Some((w, h, png))
}

fn page_number(s: &[u8]) -> Option<usize> {
    core::str::from_utf8(s.trim_ascii()).ok()?.parse().ok()
}

fn page_selected(ranges: Option<&[u8]>, page: usize) -> bool {
    let Some(ranges) = ranges else {
        return true;
    };
    ranges.split(|&b| b == b',').any(|range| {
        let range = range.trim_ascii();
        let bounds = match range.iter().position(|&b| b == b'-') {
            Some(dash) => Some((
                page_number(&range[..dash]).unwrap_or(1),
                page_number(&range[dash + 1..]).unwrap_or(usize::MAX),
            )),
            None => page_number(range).map(|n| (n, n)),
        };
        bounds.is_some_and(|(from, to)| (from..=to).contains(&page))
    })
}

struct Fields<'a> {
    title: &'a [u8],
    url: &'a [u8],
    date: &'a [u8],
}

fn expand(template: &[u8], slot: &MarginSlot, fields: &Fields) -> Vec<u8> {
    let page = slot.page.to_string();
    let pages = slot.pages.to_string();
    let substitutions: [(&[u8], &[u8]); 5] = [
        (b"{page}", page.as_bytes()),
        (b"{pages}", pages.as_bytes()),
        (b"{title}", fields.title),
        (b"{url}", fields.url),
        (b"{date}", fields.date),
    ];
    let mut out = Vec::with_capacity(template.len());
    let mut rest = template;
    while let Some(&byte) = rest.first() {
        match substitutions.iter().find(|(key, _)| rest.starts_with(key)) {
            Some((key, value)) => {
                out.extend_from_slice(value);
                rest = &rest[key.len()..];
            }
            None => {
                out.push(byte);
                rest = &rest[1..];
            }
        }
    }
    out.retain(|&b| b != 0);
    out
}

fn today() -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut date = engine::iso8601_utc(now).into_bytes();
    date.truncate(10);
    date
}

fn temporary_pdf() -> Option<(std::path::PathBuf, CString)> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let cache = southstar_glib::path_from_bytes(&southstar_glib::user_cache_dir())?;
    let dir = cache.join("southstar");
    std::fs::create_dir_all(&dir).ok()?;
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("print-{}-{n}.pdf", std::process::id()));
    let c_path = CString::new(path.to_string_lossy().into_owned()).ok()?;
    Some((path, c_path))
}

fn sheet(body: &[u8]) -> engine::PrintSetup {
    let mut setup = engine::print_setup_default();
    let sides = [
        ("width", &mut setup.width, 1.0),
        ("height", &mut setup.height, 1.0),
        ("margin_top", &mut setup.margin_top, 0.0),
        ("margin_right", &mut setup.margin_right, 0.0),
        ("margin_bottom", &mut setup.margin_bottom, 0.0),
        ("margin_left", &mut setup.margin_left, 0.0),
    ];
    for (key, side, least) in sides {
        if let Some(value) = number(body, key).filter(|v| *v >= least && *v < 100_000.0) {
            *side = value;
        }
    }
    if flag(body, "landscape") && setup.width < setup.height {
        core::mem::swap(&mut setup.width, &mut setup.height);
    }
    setup
}

pub fn pdf(cur: &Page, body: &[u8]) -> Option<(usize, Vec<u8>)> {
    let scale = number(body, "scale")
        .filter(|s| (0.1..=10.0).contains(s))
        .unwrap_or(1.0);
    let ranges = text(body, "page_ranges");
    let selected = |page: usize| page_selected(ranges.as_deref(), page);
    let to_c = |value: Option<Vec<u8>>| value.and_then(|v| CString::new(v).ok());
    let title = to_c(text(body, "title"));
    let author = to_c(text(body, "author"));
    let creation_date = to_c(text(body, "creation_date"));
    let date = creation_date
        .as_ref()
        .map(|d| d.to_bytes())
        .filter(|d| d.len() >= 10)
        .map_or_else(today, |d| d[..10].to_vec());
    let labels = [
        ["header_left", "header_center", "header_right"].map(|key| text(body, key)),
        ["footer_left", "footer_center", "footer_right"].map(|key| text(body, key)),
    ];
    let page_title = cur.title().unwrap_or_default();
    let page_url = cur.url().unwrap_or_default();
    let fields = Fields {
        title: title.as_ref().map_or(&page_title[..], |t| t.to_bytes()),
        url: &page_url,
        date: &date,
    };
    let margin_text = |slot: &MarginSlot| {
        let template = labels[usize::from(slot.footer)][slot.column].as_deref()?;
        CString::new(expand(template, slot, &fields)).ok()
    };
    let doc = PdfDocument {
        scale,
        selected: &selected,
        title: title.as_deref(),
        author: author.as_deref(),
        creation_date: creation_date.as_deref(),
        margin_text: &margin_text,
    };
    let layout = PdfLayout {
        setup: sheet(body),
        page_rule: !flag(body, "ignore_css_page_size"),
        print_media: !flag(body, "screen_media"),
    };
    let (path, c_path) = temporary_pdf()?;
    let pages = cur.write_pdf(&c_path, &layout, &doc);
    let bytes = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    pages.filter(|&n| n > 0).zip(bytes)
}
