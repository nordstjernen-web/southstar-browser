//! Southstar — the details element: summary activation, the toggle events and exclusive `<details name>` groups.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::collections::VecDeque;
use std::ffi::CString;

use southstar_dom::children;

use crate::ffi::{self, Js};
use crate::popover::open_state;
use crate::{Element, has_attr, is_named};

fn close_others_in_group(js: Js, opened: Element, name: &CStr) {
    if name.is_empty() {
        return;
    }
    let Some(doc) = js.current_document() else {
        return;
    };
    let mut queue = VecDeque::from([doc]);
    while let Some(n) = queue.pop_front() {
        let mut child = n.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            queue.push_back(c);
            if c == opened
                || !c.is_element()
                || c.name().map(CStr::to_bytes) != Some(b"details")
                || c.attr(c"name") != Some(name)
                || !has_attr(c, c"open")
            {
                continue;
            }
            js.fire_toggle(c, c"beforetoggle", c"open", c"closed", false, None);
            ffi::remove_attr(c, c"open");
            js.mark_mutated();
            js.fire_toggle(c, c"toggle", c"open", c"closed", false, None);
            child = c.next_sibling();
        }
    }
}

pub(crate) fn toggle_open(js: Js, details: Element, open: bool) {
    let old_state = open_state(!open);
    let new_state = open_state(open);
    js.fire_toggle(details, c"beforetoggle", old_state, new_state, false, None);
    if open {
        let name: Option<CString> = details.attr(c"name").map(CStr::to_owned);
        if let Some(name) = name {
            close_others_in_group(js, details, &name);
        }
    }
    js.fire_toggle(details, c"toggle", old_state, new_state, false, None);
}

pub(crate) fn summary_toggle_target(el: Option<Element>) -> Option<Element> {
    let summary =
        core::iter::successors(el, |c| c.parent()).find(|c| is_named(Some(*c), b"summary"))?;
    let details = summary
        .parent()
        .filter(|d| is_named(Some(*d), b"details"))?;
    let first = children(details).find(|c| is_named(Some(*c), b"summary"))?;
    (first == summary).then_some(details)
}

pub(crate) fn activate_summary(js: Js, el: Option<Element>) -> bool {
    let Some(details) = summary_toggle_target(el) else {
        return false;
    };
    let open = has_attr(details, c"open");
    if open {
        js.remove_attr_recorded(details, c"open");
    } else {
        js.set_attr_recorded(details, c"open", b"");
    }
    toggle_open(js, details, !open);
    true
}
