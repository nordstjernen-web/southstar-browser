//! Southstar — the option rows of an open or list-box select and the datalist suggestions under a focused input.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{MAX_DEPTH, Node, children, controls::used_value, select::option_label};
use southstar_layout::inline_kind;
use southstar_style::StyleTable;

use crate::controls::emit_form_attr_sized;
use crate::ffi::{self, Attrs, Out};

const LINE_SEPARATOR: &[u8] = "\u{2028}".as_bytes();
const TWO_NBSP: &[u8] = "\u{a0}\u{a0}".as_bytes();

fn emit_row(out: &mut Out, attrs: Attrs, styles: StyleTable, dom: Node<'_>, label: &[u8]) {
    let start = out.len();
    out.push(TWO_NBSP);
    out.push(label);
    out.push(TWO_NBSP);
    emit_form_attr_sized(
        attrs,
        inline_kind::INPUT_FIELD,
        start,
        out.len(),
        Some(dom),
        styles,
    );
}

pub fn emit_option(
    out: &mut Out,
    attrs: Attrs,
    styles: StyleTable,
    option: Node<'_>,
    separate: bool,
) {
    if separate {
        out.push(LINE_SEPARATOR);
    }
    emit_row(out, attrs, styles, option, &option_label(option));
}

fn find_datalist<'a>(node: Node<'a>, id: &CStr, depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if node.element_name() == Some(b"datalist") && node.attr(c"id") == Some(id) {
        return Some(node);
    }
    children(node).find_map(|c| find_datalist(c, id, depth + 1))
}

fn option_value(option: Node<'_>) -> Vec<u8> {
    match option.attr(c"value").filter(|v| !v.is_empty()) {
        Some(v) => v.to_bytes().to_vec(),
        None => option_label(option),
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

pub fn emit_datalist_suggestions(out: &mut Out, attrs: Attrs, styles: StyleTable, input: Node<'_>) {
    let Some(list_id) = input.attr(c"list").filter(|id| !id.is_empty()) else {
        return;
    };
    let mut root = input;
    while let Some(p) = root.parent() {
        root = p;
    }
    let Some(dl) = find_datalist(root, list_id, 0) else {
        return;
    };
    let needle = used_value(input)
        .filter(|v| !v.is_empty())
        .map(|v| ffi::utf8_casefold(v.to_bytes()));
    let mut shown = 0;
    for o in children(dl) {
        if shown >= 8 {
            break;
        }
        if o.element_name() != Some(b"option") {
            continue;
        }
        let val = option_value(o);
        if val.is_empty() {
            continue;
        }
        let matched = needle.as_ref().is_none_or(|needle| {
            let vl = ffi::utf8_casefold(&val);
            contains(&vl, needle) && !vl.eq_ignore_ascii_case(needle)
        });
        if matched {
            out.push(LINE_SEPARATOR);
            emit_row(out, attrs, styles, o, &val);
            shown += 1;
        }
    }
}
