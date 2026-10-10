//! Southstar — the focusing steps for popovers and dialogs, and the document's autofocus candidate.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_dom::tree::effectively_inert;
use southstar_dom::{children, index};

use crate::ffi::{self, Js};
use crate::popover::{PopoverType, is_showing, type_of};
use crate::{Element, MAX_DEPTH, has_attr, in_active_document, inclusive_ancestor, is_named};

pub(crate) fn run_focusing_steps(js: Js, el: Element) {
    if !in_active_document(js, el)
        || effectively_inert(el)
        || ffi::effectively_disabled(el)
        || !ffi::is_focusable(el)
    {
        return;
    }
    js.set_focus(Some(el));
}

fn hides_focus_subtree(n: Element) -> bool {
    if has_attr(n, c"hidden") || is_named(Some(n), b"template") {
        return true;
    }
    if is_named(Some(n), b"dialog") && !has_attr(n, c"open") {
        return true;
    }
    type_of(Some(n)) != PopoverType::None && !is_showing(n)
}

pub(crate) fn autofocus_delegate(root: Element, depth: i32) -> Option<Element> {
    if depth >= MAX_DEPTH {
        return None;
    }
    for c in children(root) {
        if !c.is_element() || crate::is_shadow_root(c) || hides_focus_subtree(c) {
            continue;
        }
        if has_attr(c, c"autofocus") && ffi::is_focusable(c) && !effectively_inert(c) {
            return Some(c);
        }
        if let Some(d) = autofocus_delegate(c, depth + 1) {
            return Some(d);
        }
    }
    None
}

fn is_sequentially_focusable(n: Element) -> bool {
    ffi::is_focusable(n) && !effectively_inert(n) && !ffi::tabindex(n).is_some_and(|ti| ti < 0)
}

fn first_sequentially_focusable(root: Element, depth: i32) -> Option<Element> {
    if depth >= MAX_DEPTH {
        return None;
    }
    for c in children(root) {
        if !c.is_element() || hides_focus_subtree(c) {
            continue;
        }
        if is_sequentially_focusable(c) {
            return Some(c);
        }
        if let Some(d) = first_sequentially_focusable(c, depth + 1) {
            return Some(d);
        }
    }
    None
}

pub(crate) fn popover_focusing_steps(js: Js, el: Element) {
    if is_named(Some(el), b"dialog") {
        dialog_focusing_steps(js, el);
        return;
    }
    let control = if has_attr(el, c"autofocus") {
        Some(el)
    } else {
        autofocus_delegate(el, 0)
    };
    let Some(control) = control else {
        return;
    };
    run_focusing_steps(js, control);
    js.set_autofocus_processed();
}

pub(crate) fn dialog_focusing_steps(js: Js, dialog: Element) {
    js.set_autofocus_processed();
    let control = if has_attr(dialog, c"autofocus") {
        dialog
    } else {
        autofocus_delegate(dialog, 0)
            .or_else(|| first_sequentially_focusable(dialog, 0))
            .unwrap_or(dialog)
    };
    if control != dialog {
        run_focusing_steps(js, control);
    } else if !effectively_inert(dialog) && in_active_document(js, dialog) {
        js.set_focus(Some(dialog));
    }
    if js.focused().is_some()
        && crate::dialog::is_modal(js, dialog)
        && !inclusive_ancestor(dialog, js.focused())
    {
        js.set_focus(None);
    }
}

fn url_targets_element(js: Js, doc: Element) -> bool {
    let url = js.current_url().to_bytes();
    let Some(hash) = url.iter().position(|&b| b == b'#') else {
        return false;
    };
    let fragment = &url[hash + 1..];
    if fragment.is_empty() {
        return false;
    }
    let Some(id) = percent_decode(fragment) else {
        return false;
    };
    let Ok(id) = CString::new(id) else {
        return false;
    };
    index::find_by_id(doc, &id).is_some()
}

fn percent_decode(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hi = (hex[0] as char).to_digit(16)?;
            let lo = (hex[1] as char).to_digit(16)?;
            let byte = (hi * 16 + lo) as u8;
            if byte == 0 {
                return None;
            }
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(out)
}

pub(crate) fn flush_autofocus(js: Js) {
    let Some(doc) = js.current_document() else {
        return;
    };
    if js.autofocus_processed() || js.halted() {
        return;
    }
    if js.focused().is_some() || url_targets_element(js, doc) {
        js.set_autofocus_processed();
        return;
    }
    if let Some(target) = autofocus_delegate(doc, 0) {
        js.set_autofocus_processed();
        run_focusing_steps(js, target);
        return;
    }
    if js.ready_state() >= 2 {
        js.set_autofocus_processed();
    }
}
