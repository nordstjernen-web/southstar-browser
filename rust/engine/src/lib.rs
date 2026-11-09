//! Southstar — the synchronous page pipeline shared by the drivers: fetches, style sheets, relayout, captures and dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod fetch;
mod ffi;
mod styles;
mod timings;

pub use fetch::{linked_css_known, linked_css_redirected_away, linked_css_text};
pub use styles::layout_perf;
pub use timings::{ResourceTiming, take_resource_timings};

use core::ffi::CStr;
use std::ffi::CString;

use southstar_cairo::PdfMetadata;
use southstar_layout::{BoxKind, BoxRef};

use crate::ffi::{Capture, Out};

pub use crate::fetch::speculative_preload;
pub use crate::ffi::{
    CssCache, ImageFetch, ImageSession, Page, Post, PrintSetup, Relayout, Sheet, anim_observe,
    compute_cascade, dump_layout, dump_text, fetch_blocking, fetch_images, load_keyframes,
    navigate_blocking, print_sheets, profile_enabled, relayout, restyle, start_image_session,
};
pub use southstar_net::in_blocking_fetch;
pub use southstar_render::page_rule;

const CAIRO_MAX: i32 = 30000;
const PT_PER_PX: f64 = 72.0 / 96.0;

pub fn png_size(root: BoxRef) -> (i32, i32) {
    let mut cw = root.content_width();
    let positive = cw > 0.0;
    if !positive {
        cw = 1024.0;
    }
    if cw > CAIRO_MAX as f64 {
        cw = CAIRO_MAX as f64;
    }
    let w = cw as i32;
    let mut max_bottom = root.max_bottom(root.content_height());
    let positive = max_bottom > 0.0;
    if !positive {
        max_bottom = 0.0;
    }
    if max_bottom > CAIRO_MAX as f64 {
        max_bottom = CAIRO_MAX as f64;
    }
    let viewport_h = southstar_css::viewport().1.min(CAIRO_MAX as f64);
    let mut h = if max_bottom > viewport_h + 0.5 {
        (max_bottom as i32).wrapping_add(32)
    } else {
        viewport_h as i32
    };
    if h <= 0 {
        h = 768;
    }
    (w.min(CAIRO_MAX), h)
}

pub fn write_png(root: BoxRef, path: &core::ffi::CStr) -> i32 {
    let (w, mut h) = png_size(root);
    if h > CAIRO_MAX {
        ffi::err(
            format!("engine: page is {h} px tall; PNG capped at {CAIRO_MAX} (cairo limit)\n")
                .as_bytes(),
        );
        h = CAIRO_MAX;
    }
    let Some(capture) = Capture::image(w, h) else {
        ffi::err(b"engine: failed to create PNG surface\n");
        return 2;
    };
    capture.paint(root);
    match capture.write_png(path) {
        Ok(()) => 0,
        Err(reason) => {
            ffi::err(&[b"engine: PNG write failed: ", reason.as_slice(), b"\n"].concat());
            2
        }
    }
}

pub fn write_pdf(root: BoxRef, path: &core::ffi::CStr) -> i32 {
    let w = if root.content_width() > 0.0 {
        root.content_width()
    } else {
        595.0
    };
    let h = if root.content_height() > 0.0 {
        root.content_height() + 32.0
    } else {
        842.0
    };
    let Some(capture) = Capture::pdf(path, w, h) else {
        ffi::err(b"engine: failed to create PDF surface\n");
        return 2;
    };
    capture.paint(root);
    capture.show_page();
    0
}

pub fn write_pdf_paged(root: BoxRef, path: &core::ffi::CStr, setup: &ffi::PrintSetup) -> i32 {
    let page_h = setup.height - setup.margin_top - setup.margin_bottom;
    let offsets = ffi::page_offsets(root, page_h);
    let Some(capture) = Capture::pdf(path, setup.width * PT_PER_PX, setup.height * PT_PER_PX)
    else {
        ffi::err(b"engine: failed to create PDF surface\n");
        return 2;
    };
    for i in 0..offsets.len() {
        capture.draw_sheet(
            root,
            setup,
            PT_PER_PX,
            offsets.top(i),
            offsets.bottom(i, page_h),
        );
        capture.show_page();
    }
    0
}

pub struct MarginSlot {
    pub page: usize,
    pub pages: usize,
    pub column: usize,
    pub footer: bool,
}

pub struct PdfDocument<'a> {
    pub scale: f64,
    pub selected: &'a dyn Fn(usize) -> bool,
    pub title: Option<&'a CStr>,
    pub author: Option<&'a CStr>,
    pub creation_date: Option<&'a CStr>,
    pub margin_text: &'a dyn Fn(&MarginSlot) -> Option<CString>,
}

fn draw_margin_texts(
    capture: &Capture,
    setup: &ffi::PrintSetup,
    points: f64,
    slot: MarginSlot,
    slot_text: &dyn Fn(&MarginSlot) -> Option<CString>,
) {
    let width = setup.width * points;
    let columns = [
        (setup.margin_left * points, 0.0),
        (width / 2.0, 0.5),
        (width - setup.margin_right * points, 1.0),
    ];
    let header = setup.margin_top * points / 2.0 + ffi::LABEL_POINTS / 3.0;
    let footer =
        setup.height * points - setup.margin_bottom * points / 2.0 + ffi::LABEL_POINTS / 3.0;
    for footer_row in [false, true] {
        for (column, (x, align)) in columns.into_iter().enumerate() {
            let at = MarginSlot {
                column,
                footer: footer_row,
                ..slot
            };
            if let Some(text) = slot_text(&at) {
                capture.draw_label(x, if footer_row { footer } else { header }, align, &text);
            }
        }
    }
}

pub fn write_pdf_document(
    root: BoxRef,
    path: &CStr,
    setup: &ffi::PrintSetup,
    doc: &PdfDocument,
) -> Option<usize> {
    let page_h = setup.height - setup.margin_top - setup.margin_bottom;
    if page_h <= 1.0 || page_h.is_nan() || doc.scale <= 0.0 || doc.scale.is_nan() {
        return None;
    }
    let offsets = ffi::page_offsets(root, page_h);
    let points = PT_PER_PX * doc.scale;
    let capture = Capture::pdf(path, setup.width * points, setup.height * points)?;
    let metadata = [
        (PdfMetadata::Title, doc.title),
        (PdfMetadata::Author, doc.author),
        (PdfMetadata::CreateDate, doc.creation_date),
    ];
    for (key, value) in metadata {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            capture.set_metadata(key, value);
        }
    }
    let pages = offsets.len();
    let mut written = 0;
    for i in (0..pages).filter(|i| (doc.selected)(i + 1)) {
        capture.draw_sheet(
            root,
            setup,
            points,
            offsets.top(i),
            offsets.bottom(i, page_h),
        );
        let slot = MarginSlot {
            page: i + 1,
            pages,
            column: 0,
            footer: false,
        };
        draw_margin_texts(&capture, setup, points, slot, doc.margin_text);
        capture.show_page();
        written += 1;
    }
    Some(written)
}

pub fn print_recordings(root: BoxRef, setup: &ffi::PrintSetup) -> Vec<Capture> {
    let page_h = setup.height - setup.margin_top - setup.margin_bottom;
    let offsets = ffi::page_offsets(root, page_h);
    (0..offsets.len())
        .map(|i| {
            let sheet = Capture::recording(setup.width, setup.height);
            sheet.draw_sheet(root, setup, 1.0, offsets.top(i), offsets.bottom(i, page_h));
            sheet
        })
        .collect()
}

fn write_text(b: BoxRef, out: &mut Out) {
    match b.kind() {
        BoxKind::Inline => {
            if let Some(text) = b.text().filter(|t| !t.is_empty()) {
                out.append(text.to_bytes());
                out.append(b"\n");
            }
        }
        BoxKind::Image => {
            if let Some(dom) = ffi::box_dom(b) {
                let alt = dom.attr(c"alt").filter(|a| !a.is_empty());
                let src = b.media().and_then(|m| m.image_src());
                match alt.or(src.as_deref()) {
                    Some(label) => out.append(&[b"[image: ", label.to_bytes(), b"]\n"].concat()),
                    None => out.append(b"[image]\n"),
                }
            }
        }
        _ => {}
    }
    for c in southstar_layout::children(b) {
        write_text(c, out);
    }
    for atomic in b.inline_atomic_boxes() {
        write_text(atomic, out);
    }
}

fn write_layout(b: BoxRef, indent: i32, out: &mut Out) {
    for _ in 0..indent {
        out.append(b" ");
    }
    out.box_line(b);
    if let Some(dom) = ffi::box_dom(b)
        && let Some(name) = dom.name()
    {
        let id = dom
            .is_element()
            .then(|| dom.attr(c"id"))
            .flatten()
            .filter(|id| !id.is_empty());
        match id {
            Some(id) => out.append(&[b" <", name.to_bytes(), b"#", id.to_bytes(), b">"].concat()),
            None => out.append(&[b" <", name.to_bytes(), b">"].concat()),
        }
    }
    if let Some(src) = b.media().and_then(|m| m.image_src()) {
        out.append(&[b" img=", src.to_bytes()].concat());
    }
    if let Some(text) = b.text().filter(|t| !t.is_empty()) {
        let text = text.to_bytes();
        if text.len() > 40 {
            out.append(&[b" text=\"", &text[..40], "…\"".as_bytes()].concat());
        } else {
            out.append(&[b" text=\"", text, b"\""].concat());
        }
    }
    out.append(b"\n");
    for c in southstar_layout::children(b) {
        write_layout(c, indent + 2, out);
    }
    for atomic in b.inline_atomic_boxes() {
        write_layout(atomic, indent + 2, out);
    }
}

pub fn suffix_before_ext(path: &[u8], suffix: &[u8]) -> Vec<u8> {
    let slash = path.iter().rposition(|&b| b == b'/' || b == b'\\');
    match path.iter().rposition(|&b| b == b'.') {
        Some(dot) if slash.is_none_or(|s| dot > s) => [&path[..dot], suffix, &path[dot..]].concat(),
        _ => [path, suffix].concat(),
    }
}

pub fn iso8601_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86400);
    let secs = unix_secs.rem_euclid(86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}
