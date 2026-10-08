//! Southstar — what an embedder reads from a page: the performance report, captures and print sheets, the focused field and caret blink, the title, links and favicon.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::collections::HashSet;

use southstar_dom::{Node, children};
use southstar_glib::GPtrArray;
use southstar_layout::BoxRef;

use crate::ffi::{self, NsBrowser, PrintSetup, StrBufOwned};
use crate::{images, page};

const MAX_DEPTH: c_int = 1024;
const CARET_BLINK_US: i64 = 530 * 1000;

fn node_count(n: Option<Node<'_>>, depth: c_int) -> c_int {
    if depth > MAX_DEPTH {
        return 0;
    }
    let mut count = 0;
    let mut cur = n;
    while let Some(node) = cur {
        count += 1 + node_count(node.first_child(), depth + 1);
        cur = node.next_sibling();
    }
    count
}

fn box_count(b: Option<BoxRef<'_>>) -> c_int {
    let mut count = 0;
    let mut cur = b;
    while let Some(b) = cur {
        count += 1 + box_count(b.first_child());
        cur = b.next_sibling();
    }
    count
}

fn dump_threads(out: &StrBufOwned) {
    out.append(b"Threads\n");
    if cfg!(target_os = "linux") {
        match ffi::list_threads() {
            ffi::Threads::Unavailable(message) => {
                out.append(b"  (unavailable: ");
                out.append(&message);
                out.append(b")\n");
            }
            ffi::Threads::Listed(threads) => {
                for (tid, name) in &threads {
                    out.append(b"  [");
                    out.append(tid);
                    out.append(b"] ");
                    out.append(name);
                    out.append(b"\n");
                }
                let n = threads.len();
                out.append(
                    format!("  {n} thread{} total\n", if n == 1 { "" } else { "s" }).as_bytes(),
                );
            }
        }
    } else {
        out.append(b"  (thread enumeration is Linux-only)\n");
    }
}

pub fn dump_performance(b: &NsBrowser) -> StrBufOwned {
    let out = StrBufOwned::new();
    let (pw, ph) = page::page_size(b).unwrap_or((0, 0));
    let text = |t: Option<&CStr>| t.map_or(&b""[..], CStr::to_bytes).to_vec();
    out.append(b"Document\n");
    out.append(b"  url         ");
    out.append(&text(b.base_url.get()));
    out.append(b"\n  charset     ");
    out.append(&text(b.doc_charset.get()));
    out.append(b"\n");
    out.append(format!("  DOM nodes   {}\n", node_count(b.doc(), 0)).as_bytes());
    out.append(format!("  layout boxes {}\n", box_count(b.layout())).as_bytes());
    out.append(format!("  page size   {pw} x {ph} px\n").as_bytes());
    out.append(b"\nLayout\n");
    out.append(format!("  viewport    {} x ", b.vw.get()).as_bytes());
    out.append_double(c"%.0f px\n", b.vh.get());
    out.append_double(
        c"  last reflow %.2f ms\n",
        b.relayout_cost_us.get() as f64 / 1000.0,
    );
    out.append(format!("  oscillation {}\n", b.layout_osc.get()).as_bytes());
    out.append(b"\n");
    if let Some(js) = b.js() {
        js.dump_stats(&out);
    }
    out.append(b"\n");
    dump_threads(&out);
    out
}

pub fn render_image(b: &NsBrowser, path: &CStr) -> c_int {
    images::wait_images(b);
    ffi::paint_set_js(b.js());
    ffi::paint_set_anim(b.anim());
    let path_bytes = path.to_bytes();
    let pdf =
        path_bytes.len() >= 4 && path_bytes[path_bytes.len() - 4..].eq_ignore_ascii_case(b".pdf");
    let rc = match b.layout() {
        Some(layout) => ffi::write_capture(layout, path, pdf),
        None => -1,
    };
    ffi::paint_set_anim(None);
    ffi::paint_set_js(None);
    rc
}

fn content_box(setup: &PrintSetup) -> (c_int, f64) {
    (
        (setup.width - setup.margin_left - setup.margin_right) as c_int,
        setup.height - setup.margin_top - setup.margin_bottom,
    )
}

pub fn print_pages(b: &NsBrowser) -> (*mut GPtrArray, PrintSetup) {
    images::wait_images(b);
    let mut setup = ffi::print_setup_default();
    let saved = (b.vw.get(), b.vh.get());
    ffi::css_set_print_media(true);
    let (w, h) = content_box(&setup);
    b.vw.set(w);
    b.vh.set(h);
    page::relayout(b);
    if ffi::apply_page_rule(&mut setup) {
        let (w, h) = content_box(&setup);
        if w > 0 && w != b.vw.get() {
            b.vw.set(w);
            b.vh.set(h);
            page::relayout(b);
        }
    }
    ffi::paint_set_js(b.js());
    ffi::paint_set_anim(b.anim());
    let pages = b.print_recordings(&setup);
    ffi::paint_set_anim(None);
    ffi::paint_set_js(None);
    ffi::css_set_print_media(false);
    b.vw.set(saved.0);
    b.vh.set(saved.1);
    page::relayout(b);
    (pages, setup)
}

pub fn utf8_boundary(s: &[u8], off: usize) -> usize {
    if off >= s.len() {
        return s.len();
    }
    let mut off = off;
    while off > 0 && s[off] & 0xc0 == 0x80 {
        off -= 1;
    }
    off
}

fn focused_value(b: &NsBrowser) -> Option<(Node<'_>, &CStr)> {
    let focused = b.js()?.focused_node()?;
    Some((focused, ffi::editable_value(focused)?))
}

pub fn focused_editable(b: &NsBrowser) -> bool {
    focused_value(b).is_some()
}

pub fn set_caret_blink_active(b: &NsBrowser, active: bool) -> bool {
    let focused = if active && focused_editable(b) {
        b.js().and_then(|js| js.focused_node())
    } else {
        None
    };
    let caret = if focused.is_some() {
        b.caret_byte.get()
    } else {
        0
    };
    let anchor = if focused.is_some() {
        b.sel_anchor_byte.get()
    } else {
        0
    };
    let now = ffi::monotonic_us();
    if !b.caret_blink_node.is(focused)
        || caret != b.caret_blink_byte.get()
        || anchor != b.caret_blink_anchor.get()
    {
        b.caret_blink_epoch_us.set(now);
        b.caret_blink_node.set(focused);
        b.caret_blink_byte.set(caret);
        b.caret_blink_anchor.set(anchor);
    }
    let visible =
        focused.is_some() && ((now - b.caret_blink_epoch_us.get()) / CARET_BLINK_US) % 2 == 0;
    let changed =
        b.caret_blink_active.get() != focused.is_some() || b.caret_paint_visible.get() != visible;
    b.caret_blink_active.set(focused.is_some());
    b.caret_paint_visible.set(visible);
    if focused.is_none() {
        b.caret_blink_epoch_us.set(0);
    }
    changed
}

pub fn focused_editable_value(b: &NsBrowser) -> Option<(Vec<u8>, usize, usize)> {
    let (_, value) = focused_value(b)?;
    let value = value.to_bytes();
    let caret = utf8_boundary(value, b.caret_byte.get().min(value.len()));
    let anchor = utf8_boundary(value, b.sel_anchor_byte.get().min(value.len()));
    Some((value.to_vec(), caret, anchor))
}

pub fn set_focused_editable_selection(b: &NsBrowser, caret: usize, anchor: usize) -> bool {
    let Some((_, value)) = focused_value(b) else {
        return false;
    };
    let value = value.to_bytes();
    b.caret_byte.set(utf8_boundary(value, caret));
    b.sel_anchor_byte.set(utf8_boundary(value, anchor));
    b.dirty.set(true);
    page::relayout(b);
    b.dirty.set(false);
    true
}

pub fn title(b: &NsBrowser) -> Option<Vec<u8>> {
    let title = ffi::find_first_element(b.doc()?, c"title")?;
    let raw = ffi::collect_text(title)?;
    let mut out = Vec::new();
    let mut prev_ws = true;
    for &c in raw.to_bytes() {
        if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
            if !prev_ws {
                out.push(b' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    if out.last() == Some(&b' ') {
        out.pop();
    }
    (!out.is_empty()).then_some(out)
}

fn collect_links(
    node: Node<'_>,
    base: Option<&CStr>,
    out: &mut Vec<u8>,
    seen: &mut HashSet<Vec<u8>>,
    depth: c_int,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for c in children(node) {
        if ffi::is_named(c, c"a") {
            if let Some(href) = c.attr(c"href") {
                let h = href.to_bytes();
                if !h.is_empty() && h[0] != b'#' && !h.starts_with(b"javascript:") {
                    if let Some(abs) = ffi::url_resolve(base, href).filter(|a| !a.is_empty()) {
                        if seen.insert(abs.to_bytes().to_vec()) {
                            if !out.is_empty() {
                                out.push(b'\n');
                            }
                            out.extend_from_slice(abs.to_bytes());
                        }
                    }
                }
            }
        }
        collect_links(c, base, out, seen, depth + 1);
    }
}

pub fn links(b: &NsBrowser) -> Option<Vec<u8>> {
    let doc = b.doc()?;
    let mut out = Vec::new();
    collect_links(doc, b.base_url.get(), &mut out, &mut HashSet::new(), 0);
    (!out.is_empty()).then_some(out)
}

fn rel_token_is_icon(rel: Option<&CStr>) -> bool {
    rel.is_some_and(|rel| {
        rel.to_bytes()
            .split(|c| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
            .any(|token| token.eq_ignore_ascii_case(b"icon"))
    })
}

fn find_icon_href(node: Node<'_>, depth: c_int) -> Option<&CStr> {
    if depth > MAX_DEPTH {
        return None;
    }
    for c in children(node) {
        if ffi::is_named(c, c"link") {
            if let Some(href) = c.attr(c"href").filter(|h| !h.is_empty()) {
                if rel_token_is_icon(c.attr(c"rel")) {
                    return Some(href);
                }
            }
        }
        if let Some(found) = find_icon_href(c, depth + 1) {
            return Some(found);
        }
    }
    None
}

pub fn favicon_url(b: &NsBrowser) -> Option<Vec<u8>> {
    let doc = b.doc()?;
    let abs = find_icon_href(doc, 0).and_then(|href| ffi::url_resolve(b.base_url.get(), href));
    if let Some(abs) = abs.filter(|a| !a.is_empty()) {
        return Some(abs.to_bytes().to_vec());
    }
    let origin = ffi::url_origin_from(b.base_url.get()).filter(|o| !o.is_empty())?;
    let mut out = origin.to_bytes().to_vec();
    out.extend_from_slice(b"/favicon.ico");
    Some(out)
}
