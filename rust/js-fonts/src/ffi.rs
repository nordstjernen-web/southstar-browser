//! Southstar — the C ABI and Rust-ABI entry points of the font loading bindings, and the realm and font loader calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::{font_face, ready};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(Option<Page>);

impl Js {
    fn from_ptr(js: Option<Page>) -> Js {
        Js(js)
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }

    fn key(self) -> usize {
        self.0.map_or(0, Page::addr)
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
}

pub(crate) fn flush_layout(js: Js) {
    api::flush_layout(js.ptr());
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.ptr());
}

pub(crate) fn drain_microtasks(js: Js) {
    api::drain_microtasks(js.ptr());
}

pub(crate) fn pending_font_count() -> u32 {
    southstar_font::pending_count()
}

fn fonts_idle(data: usize) {
    let js = Js(Page::from_addr(data));
    if js.is_null() || !ready::has_waiting(js) {
        return;
    }
    api::main_context(js.ptr()).with(|scope| ready::fonts_idle(scope, js));
}

pub(crate) fn wait_for_fonts(js: Js) {
    southstar_font::add_idle_waiter(fonts_idle, js.key());
}

pub(crate) fn stop_waiting_for_fonts(js: Js) {
    southstar_font::remove_idle_waiter(fonts_idle, js.key());
}

pub fn nsr_window_fontface_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    font_face::construct(scope, this, args)
}

fn teardown(js: Option<Page>) {
    ready::teardown(Js::from_ptr(js));
}

pub fn install_runtime_hooks() {
    api::set_fonts_hooks(api::PageStateHooks {
        init: |_| {},
        teardown: |js| teardown(Some(js)),
    });
}
