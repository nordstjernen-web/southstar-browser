//! Southstar — editing the focused field: its selection, copy and cut, replacing text with beforeinput and input events, editing keys and paste.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::{Node, ancestors_and_self};

use crate::ffi::{self, NsBrowser};
use crate::{build, hit, page, query};

const EDIT_FIELD: c_int = 1;
const EDIT_WRITABLE: c_int = 2;
const EDIT_SELECTION: c_int = 4;

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn value_bytes(n: Node<'_>) -> &[u8] {
    ffi::editable_value(n).map_or(&[][..], CStr::to_bytes)
}

pub fn focused_field(b: &NsBrowser) -> Option<Node<'_>> {
    let focused = b.js().and_then(|js| js.focused_node());
    focused.filter(|_| ffi::is_editable(focused))
}

fn field_writable(field: Option<Node<'_>>) -> bool {
    let Some(field) = field.filter(|_| ffi::is_editable(field)) else {
        return false;
    };
    ffi::is_contenteditable_host(field)
        || (field.attr(c"readonly").is_none() && field.attr(c"disabled").is_none())
}

fn field_copyable(field: Node<'_>) -> bool {
    if !ffi::is_named(field, c"input") {
        return true;
    }
    field
        .attr(c"type")
        .is_none_or(|t| !t.to_bytes().eq_ignore_ascii_case(b"password"))
}

fn field_selection(b: &NsBrowser, field: Node<'_>) -> (usize, usize) {
    let cur = value_bytes(field);
    let caret = query::utf8_boundary(cur, b.caret_byte.get());
    let anchor = query::utf8_boundary(cur, b.sel_anchor_byte.get());
    (caret.min(anchor), caret.max(anchor))
}

fn field_selected_text(b: &NsBrowser, field: Node<'_>) -> Option<Vec<u8>> {
    let (lo, hi) = field_selection(b, field);
    if lo >= hi || !field_copyable(field) {
        return None;
    }
    Some(value_bytes(field)[lo..hi].to_vec())
}

fn field_edit_state(b: &NsBrowser, field: Node<'_>) -> c_int {
    let mut state = EDIT_FIELD;
    if field_writable(Some(field)) {
        state |= EDIT_WRITABLE;
    }
    let (lo, hi) = field_selection(b, field);
    if lo < hi && field_copyable(field) {
        state |= EDIT_SELECTION;
    }
    state
}

pub fn field_select_all(b: &NsBrowser, field: Node<'_>) {
    b.sel_anchor_byte.set(0);
    b.caret_byte.set(value_bytes(field).len());
    page::relayout(b);
    b.dirty.set(false);
}

pub fn field_cut(b: &NsBrowser, field: Node<'_>) -> Option<Vec<u8>> {
    let text = if field_writable(Some(field)) {
        field_selected_text(b, field)
    } else {
        None
    }?;
    let (lo, hi) = field_selection(b, field);
    input_replace(b, field, lo, hi, None, c"deleteByCut");
    page::relayout(b);
    b.dirty.set(false);
    Some(text)
}

pub fn copy_text(b: &NsBrowser) -> Option<Vec<u8>> {
    if let Some(field) = focused_field(b) {
        let (lo, hi) = field_selection(b, field);
        if lo < hi {
            return field_selected_text(b, field);
        }
    }
    b.selection_text().map(|t| t.to_bytes().to_vec())
}

pub fn input_replace(
    b: &NsBrowser,
    node: Node<'_>,
    del_start: usize,
    del_end: usize,
    insert: Option<&CStr>,
    input_type: &CStr,
) {
    let Some(cur) = ffi::editable_value(node) else {
        return;
    };
    let cur = cur.to_bytes();
    let cur_len = cur.len();
    let del_start = del_start.min(cur_len);
    let del_end = del_end.min(cur_len).max(del_start);
    let ins = insert.map_or(&b""[..], CStr::to_bytes);
    let mut next = Vec::with_capacity(cur_len - (del_end - del_start) + ins.len());
    next.extend_from_slice(&cur[..del_start]);
    next.extend_from_slice(ins);
    next.extend_from_slice(&cur[del_end..]);
    let next = c_string(&next);
    let data = if input_type.to_bytes() == b"insertLineBreak" {
        None
    } else {
        insert
    };
    if let Some(js) = b.js() {
        let prevented = js.dispatch_input(node, c"beforeinput", input_type, data, true);
        let focused = js.focused_node();
        if prevented || !focused.is_some_and(|f| core::ptr::eq(f.as_ptr(), node.as_ptr())) {
            return;
        }
        js.note_user_edit(node, ffi::editable_value(node));
    }
    ffi::set_editable_value(node, &next);
    b.caret_byte.set(del_start + ins.len());
    b.sel_anchor_byte.set(b.caret_byte.get());
    if let Some(js) = b.js() {
        js.dispatch_input(node, c"input", input_type, data, false);
        js.consume_mutated();
    }
}

fn is_focused(b: &NsBrowser, node: Node<'_>) -> bool {
    b.js()
        .and_then(|js| js.focused_node())
        .is_some_and(|f| core::ptr::eq(f.as_ptr(), node.as_ptr()))
}

pub fn edit_key(b: &NsBrowser, node: Node<'_>, key: Option<&CStr>, mods: c_int) -> bool {
    let shift = mods & 1 != 0;
    let ctrl = mods & 2 != 0;
    if mods & (4 | 8) != 0 {
        return false;
    }
    let (Some(cur), Some(key)) = (ffi::editable_value(node), key.filter(|k| !k.is_empty())) else {
        return false;
    };
    let bytes = cur.to_bytes();
    let cur_len = bytes.len();
    if b.caret_byte.get() > cur_len {
        b.caret_byte.set(cur_len);
    }
    if b.sel_anchor_byte.get() > cur_len {
        b.sel_anchor_byte.set(cur_len);
    }
    let (caret, anchor) = (b.caret_byte.get(), b.sel_anchor_byte.get());
    let (sel_lo, sel_hi) = (anchor.min(caret), anchor.max(caret));
    let has_sel = sel_lo != sel_hi;
    let multiline = node.name().is_some_and(|n| n.to_bytes() == b"textarea")
        || ffi::is_contenteditable_host(node);
    let k = key.to_bytes();
    if ctrl {
        if k.len() == 1 && (k[0] == b'a' || k[0] == b'A') {
            b.sel_anchor_byte.set(0);
            b.caret_byte.set(cur_len);
            return true;
        }
        return false;
    }
    match k {
        b"Backspace" => {
            if has_sel {
                input_replace(b, node, sel_lo, sel_hi, None, c"deleteContentBackward");
            } else if caret > 0 {
                let prev = ffi::utf8_prev(cur, caret);
                input_replace(b, node, prev, caret, None, c"deleteContentBackward");
            }
            true
        }
        b"Delete" => {
            if has_sel {
                input_replace(b, node, sel_lo, sel_hi, None, c"deleteContentForward");
            } else if caret < cur_len {
                let next = ffi::utf8_next(bytes, caret);
                input_replace(b, node, caret, next, None, c"deleteContentForward");
            }
            true
        }
        b"ArrowLeft" => {
            if has_sel && !shift {
                b.caret_byte.set(sel_lo);
            } else if caret > 0 {
                b.caret_byte.set(ffi::utf8_prev(cur, caret));
            }
            if !shift {
                b.sel_anchor_byte.set(b.caret_byte.get());
            }
            true
        }
        b"ArrowRight" => {
            if has_sel && !shift {
                b.caret_byte.set(sel_hi);
            } else if caret < cur_len {
                b.caret_byte.set(ffi::utf8_next(bytes, caret));
            }
            if !shift {
                b.sel_anchor_byte.set(b.caret_byte.get());
            }
            true
        }
        b"Home" => {
            b.caret_byte.set(0);
            if !shift {
                b.sel_anchor_byte.set(0);
            }
            true
        }
        b"End" => {
            b.caret_byte.set(cur_len);
            if !shift {
                b.sel_anchor_byte.set(cur_len);
            }
            true
        }
        b"Enter" => {
            if multiline {
                input_replace(b, node, sel_lo, sel_hi, Some(c"\n"), c"insertLineBreak");
                return true;
            }
            if let Some(js) = b.js() {
                js.commit_change(node);
                if !is_focused(b, node) {
                    return true;
                }
            }
            build::submit_form(b, Some(node));
            true
        }
        _ => {
            if ffi::utf8_strlen(key) == 1 && !ffi::unichar_iscntrl(ffi::utf8_get_char(key)) {
                input_replace(b, node, sel_lo, sel_hi, Some(key), c"insertText");
                return true;
            }
            false
        }
    }
}

pub fn key_char_code(key: Option<&CStr>) -> c_int {
    let Some(key) = key.filter(|k| !k.is_empty() && ffi::utf8_validate(k)) else {
        return 0;
    };
    let next = ffi::utf8_next(key.to_bytes(), 0);
    if key.to_bytes().get(next).copied().unwrap_or(0) != 0 {
        return 0;
    }
    let ch = ffi::utf8_get_char(key);
    if ffi::unichar_iscntrl(ch) {
        0
    } else {
        ch as c_int
    }
}

fn paste_text_for(field: Node<'_>, text: &CStr, lo: usize, hi: usize) -> CString {
    let multiline = ffi::is_named(field, c"textarea") || ffi::is_contenteditable_host(field);
    let text = text.to_bytes();
    let mut len = text.len();
    if !multiline {
        while len > 0 && (text[len - 1] == b'\n' || text[len - 1] == b'\r') {
            len -= 1;
        }
    }
    let mut out = Vec::with_capacity(len);
    let mut i = 0;
    while i < len {
        let mut c = text[i];
        if c == b'\r' {
            if i + 1 < len && text[i + 1] == b'\n' {
                i += 1;
            }
            c = b'\n';
        }
        out.push(if c == b'\n' && !multiline { b' ' } else { c });
        i += 1;
    }
    let mut out = c_string(&out);
    let maxlength = if ffi::is_contenteditable_host(field) {
        None
    } else {
        field.attr(c"maxlength")
    };
    let max = maxlength.and_then(ffi::parse_long).filter(|&m| m >= 0);
    if let (Some(max), Some(cur)) = (max, ffi::editable_value(field)) {
        let kept = ffi::utf8_strlen_bytes(cur, lo) + ffi::utf8_strlen_from(cur, hi);
        let room = if max > kept { max - kept } else { 0 };
        if ffi::utf8_strlen(&out) > room {
            let cut = ffi::utf8_offset(&out, room);
            out = c_string(&out.as_bytes()[..cut]);
        }
    }
    out
}

pub fn paste(b: &NsBrowser, target: Node<'_>, text: &CStr) {
    let Some(js) = b.js() else {
        return;
    };
    let prevented = js.dispatch_clipboard(target, c"paste", text);
    if js.consume_mutated() {
        b.dirty.set(true);
    }
    let field = focused_field(b);
    if prevented || !field_writable(field) {
        return;
    }
    let Some(field) = field else {
        return;
    };
    let (lo, hi) = field_selection(b, field);
    let insert = paste_text_for(field, text, lo, hi);
    if !insert.is_empty() {
        input_replace(b, field, lo, hi, Some(&insert), c"insertFromPaste");
        b.datalist_suppressed.set(false);
        b.dirty.set(true);
    }
}

pub fn context_field(b: &NsBrowser, x: c_int, y: c_int) -> c_int {
    let Some(layout) = b.layout() else {
        return 0;
    };
    let form = ffi::hit_form_dom(layout, f64::from(x), f64::from(y));
    let mut field = form.filter(|_| ffi::is_editable(form));
    if field.is_none() {
        field = hit::hit_node(b, x, y)
            .and_then(|n| ancestors_and_self(n).find(|&a| ffi::is_contenteditable_host(a)));
    }
    let (Some(field), Some(js)) = (field, b.js()) else {
        return 0;
    };
    if !is_focused(b, field) {
        js.set_focus(field);
        b.dirty.set(true);
        if !is_focused(b, field) {
            return 0;
        }
        b.caret_byte.set(value_bytes(field).len());
        b.sel_anchor_byte.set(b.caret_byte.get());
    }
    field_edit_state(b, field)
}
