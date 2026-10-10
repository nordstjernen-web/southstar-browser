//! Southstar — the CSSOM bindings: getComputedStyle and its resolved values, element.style, the CSS namespace and the Web Animations hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animations;
mod computed;
mod css;
mod declaration;
mod ffi;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_js_engine::Value;

use crate::ffi::Js;

#[derive(Default)]
pub(crate) struct Page {
    computed_proxy: RefCell<Option<Value>>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page_or_new(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js).or_default().clone())
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js))
        .ok()
        .flatten();
    if let Some(page) = page {
        let proxy = page.computed_proxy.borrow_mut().take();
        drop(proxy);
    }
}
