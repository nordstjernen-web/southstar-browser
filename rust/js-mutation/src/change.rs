//! Southstar — the pending change event: the value a text control held when the user started editing it, compared when the edit is committed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::ffi::{self, Js};
use crate::{Element, key, with_page};

fn pending_is(js: Js, el: Element) -> bool {
    with_page(js, |page| {
        page.change.as_ref().is_some_and(|(n, _)| *n == key(el))
    })
}

pub(crate) fn forget(js: Js) {
    let change = with_page(js, |page| page.change.take());
    drop(change);
}

pub(crate) fn forget_node(js: Js, el: Element) {
    if pending_is(js, el) {
        forget(js);
    }
}

pub(crate) fn note_user_edit(js: Js, el: Element, before: &CStr) {
    if pending_is(js, el) {
        return;
    }
    if !matches!(el.element_name(), Some(b"input" | b"textarea")) {
        return;
    }
    forget(js);
    ffi::arm_js_invalidate(el);
    with_page(js, |page| page.change = Some((key(el), before.to_owned())));
}

pub(crate) fn commit(js: Js, el: Element) {
    let Some(baseline) = with_page(js, |page| {
        page.change
            .as_ref()
            .filter(|(n, _)| *n == key(el))
            .map(|(_, baseline)| baseline.clone())
    }) else {
        return;
    };
    let changed = ffi::editable_value(el) != Some(baseline.as_c_str());
    forget(js);
    if changed {
        ffi::dispatch_event(js, el, c"change");
    }
}
