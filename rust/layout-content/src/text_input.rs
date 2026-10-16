//! Southstar — the columns a one-line text input shows: its size attribute, the fitted column counts of a refitting build, alignment and the code points scrolled out of view.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_long};
use std::collections::HashMap;

use southstar_dom::{Node, ancestors_and_self, controls::parse_int, controls::used_value};
use southstar_layout::{BoxRef, children};
use southstar_style::{PropId, StyleRef, box_style};

use crate::controls::{WIDTH_PROPS, dim_px_clamped, keyword_is, length_or, pad_spaces};
use crate::ffi;

thread_local! {
    static COLUMNS: RefCell<Option<HashMap<usize, c_int>>> = const { RefCell::new(None) };
}

pub use southstar_layout_geom::utf8_skip;

pub fn utf8_len(s: &[u8]) -> c_long {
    let mut i = 0;
    let mut n: c_long = 0;
    while i < s.len() {
        i += utf8_skip(s[i]);
        n += 1;
    }
    n
}

fn size_attr(n: Node<'_>) -> c_int {
    match n.attr(c"size") {
        Some(size) => parse_int(Some(size), 20, 4, 80),
        None => 20,
    }
}

pub fn columns(n: Option<Node<'_>>) -> c_int {
    let size = n.map_or(20, size_attr);
    COLUMNS.with_borrow(|cols| {
        let Some(cols) = cols else {
            return size;
        };
        match n.and_then(|n| cols.get(&(n.as_ptr() as usize))) {
            Some(&fitted) if fitted != 0 => fitted.min(4096),
            _ => size,
        }
    })
}

fn align_rtl(s: Option<StyleRef<'_>>, n: Option<Node<'_>>) -> bool {
    if let Some(n) = n {
        for p in ancestors_and_self(n) {
            if !p.is_element() {
                continue;
            }
            let Some(dir) = p.attr(c"dir").map(CStr::to_bytes) else {
                continue;
            };
            if dir.eq_ignore_ascii_case(b"rtl") {
                return true;
            }
            if dir.eq_ignore_ascii_case(b"ltr") {
                return false;
            }
        }
    }
    keyword_is(s.and_then(|s| s.get(PropId::Direction)), c"rtl")
}

pub fn align_offset(s: Option<StyleRef<'_>>, n: Option<Node<'_>>, span: c_long) -> c_long {
    if span <= 0 {
        return 0;
    }
    let ta = s.and_then(|s| s.get(PropId::TextAlign));
    let rtl = align_rtl(s, n);
    if keyword_is(ta, c"center") {
        return span / 2;
    }
    if keyword_is(ta, c"right")
        || (keyword_is(ta, c"end") && !rtl)
        || (keyword_is(ta, c"start") && rtl)
        || (ta.is_none() && rtl)
    {
        return span;
    }
    0
}

pub fn align_skip_cps(
    s: Option<StyleRef<'_>>,
    n: Option<Node<'_>>,
    cps: c_long,
    size: c_long,
) -> c_long {
    if cps <= size {
        return 0;
    }
    align_offset(s, n, cps - size)
}

pub fn focused_skip_cps(
    value: Option<&CStr>,
    caret_byte: usize,
    cps: c_long,
    size: c_long,
) -> c_long {
    let Some(value) = value else {
        return 0;
    };
    if cps <= size {
        return 0;
    }
    let max_skip = cps - size;
    let caret_cp = pointer_to_offset(value.to_bytes(), caret_byte);
    let skip = if caret_cp > size { caret_cp - size } else { 0 };
    skip.min(max_skip).max(0)
}

pub fn pointer_to_offset(bytes: &[u8], pos: usize) -> c_long {
    let mut i = 0;
    let mut offset: c_long = 0;
    while i < pos {
        i += utf8_skip(bytes.get(i).copied().unwrap_or(0));
        offset += 1;
    }
    offset
}

pub fn advance_cps(s: &[u8], cps: c_long) -> usize {
    let mut at = 0;
    let mut i = 0;
    while at < s.len() && i < cps {
        at = (at + utf8_skip(s[at])).min(s.len());
        i += 1;
    }
    at
}

fn is_windowed_text_input(n: Option<Node<'_>>) -> bool {
    let Some(n) = n.filter(|n| n.element_name() == Some(b"input")) else {
        return false;
    };
    let Some(ty) = n
        .attr(c"type")
        .map(CStr::to_bytes)
        .filter(|t| !t.is_empty())
    else {
        return true;
    };
    [
        &b"text"[..],
        b"search",
        b"email",
        b"url",
        b"tel",
        b"number",
        b"password",
    ]
    .iter()
    .any(|t| ty.eq_ignore_ascii_case(t))
}

fn display_cps(n: Node<'_>) -> c_long {
    let v = used_value(n)
        .filter(|v| !v.is_empty())
        .or_else(|| n.attr(c"placeholder"));
    v.filter(|v| !v.is_empty())
        .map_or(0, |v| utf8_len(v.to_bytes()))
}

fn width_is_definite(s: StyleRef<'_>) -> bool {
    let fs = length_or(s.get(PropId::FontSize), 16.0);
    dim_px_clamped(Some(s), WIDTH_PROPS, fs, 0.0) > 0.0
}

fn collect(b: BoxRef<'_>, cols: &mut HashMap<usize, c_int>) -> bool {
    let mut changed = false;
    if let (Some(s), Some(dom)) = (box_style(b), b.dom().filter(|_| b.content_width() > 0.0))
        && is_windowed_text_input(Some(dom))
    {
        let have = size_attr(dom);
        let shown = display_cps(dom);
        let cell = if shown > 0 {
            ffi::char_cell_px(s.as_ptr())
        } else {
            0.0
        };
        if cell > 0.0 {
            let overhead = 2 + pad_spaces(Some(s), false);
            let cells = (b.content_width() / cell).floor() - f64::from(overhead);
            let fit = if cells < 1.0 {
                1
            } else if cells > f64::from(i16::MAX) {
                c_int::from(i16::MAX)
            } else {
                cells as c_int
            };
            let grow = fit > have && shown > c_long::from(have);
            let shrink = fit < have && shown > c_long::from(fit) && width_is_definite(s);
            if grow || shrink {
                cols.insert(dom.as_ptr() as usize, fit);
                changed = true;
            }
        }
    }
    for c in children(b) {
        if collect(c, cols) {
            changed = true;
        }
    }
    for a in b.inline_atomic_boxes() {
        if collect(a, cols) {
            changed = true;
        }
    }
    changed
}

pub fn columns_active() -> bool {
    COLUMNS.with_borrow(Option::is_some)
}

pub fn fit_columns(root: BoxRef<'_>) -> bool {
    let mut cols = HashMap::new();
    if !collect(root, &mut cols) {
        return false;
    }
    COLUMNS.set(Some(cols));
    true
}

pub fn clear_columns() {
    COLUMNS.set(None);
}
