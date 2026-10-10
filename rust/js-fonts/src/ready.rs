//! Southstar — per-page resolvers parked until the font loader reports no pending web fonts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};

type Waiting = Vec<(Value, Value)>;

thread_local! {
    static PAGES: RefCell<Vec<(Js, Waiting)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn resolve_when_fonts_loaded(
    scope: &mut Scope<'_>,
    js: Js,
    resolve: Value,
    value: Value,
) {
    if !js.is_null() && ffi::pending_font_count() > 0 {
        PAGES.with(|pages| {
            let mut pages = pages.borrow_mut();
            match pages.iter_mut().find(|(owner, _)| *owner == js) {
                Some((_, waiting)) => waiting.push((resolve, value)),
                None => pages.push((js, vec![(resolve, value)])),
            }
        });
        ffi::wait_for_fonts(js);
        return;
    }
    let _ = scope.call(&resolve, &Value::undefined(), &[value]);
}

fn take(js: Js) -> Option<Waiting> {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = pages.iter().position(|(owner, _)| *owner == js)?;
        Some(pages.swap_remove(index).1)
    })
}

pub(crate) fn fonts_idle(scope: &mut Scope<'_>, js: Js) {
    let Some(waiting) = take(js) else {
        return;
    };
    ffi::mark_mutated(js);
    for (resolve, value) in waiting {
        let _ = scope.call(&resolve, &Value::undefined(), &[value]);
    }
    ffi::drain_microtasks(js);
}

pub(crate) fn has_waiting(js: Js) -> bool {
    PAGES.with(|pages| pages.borrow().iter().any(|(owner, _)| *owner == js))
}

pub(crate) fn teardown(js: Js) {
    ffi::stop_waiting_for_fonts(js);
    drop(take(js));
}
