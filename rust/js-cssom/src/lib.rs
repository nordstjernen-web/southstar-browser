//! Southstar — the CSSOM bindings: getComputedStyle and its resolved values, element.style, the CSS namespace the Web Animations hooks and Element.animate().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod animate;
mod animations;
pub mod class;
mod computed;
mod css;
pub mod declaration;
pub(crate) mod ffi;
pub mod links;

pub use ffi::computed_text;
pub use ffi::install_runtime_hooks;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_js_engine::RealmCache;

use crate::ffi::Js;

#[derive(Default)]
pub(crate) struct Page {
    computed_proxy: RefCell<RealmCache>,
    computed_methods: RefCell<Option<Rc<declaration::ComputedMethods>>>,
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
        let proxies = page.computed_proxy.take();
        drop(proxies);
        let methods = page.computed_methods.borrow_mut().take();
        drop(methods);
    }
}
