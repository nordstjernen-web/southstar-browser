//! Southstar — the C ABI and Rust-ABI entry points of the font loading bindings, and the realm and font loader calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_void};

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{font_face, ready};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(usize);

impl Js {
    fn from_ptr(js: *mut NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

type IdleCallback = unsafe extern "C" fn(user_data: *mut c_void);

unsafe extern "C" {
    fn ns_font_pending_count() -> c_uint;
    fn ns_font_add_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
    fn ns_font_remove_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(scope.context_opaque().cast())
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
    unsafe { ns_font_pending_count() }
}

unsafe extern "C" fn fonts_idle(user_data: *mut c_void) {
    let js = Js::from_ptr(user_data.cast());
    if js.is_null() || !ready::has_waiting(js) {
        return;
    }
    api::main_context(js.ptr()).with(|scope| ready::fonts_idle(scope, js));
}

pub(crate) fn wait_for_fonts(js: Js) {
    unsafe { ns_font_add_idle_cb(Some(fonts_idle), js.ptr().cast()) };
}

pub(crate) fn stop_waiting_for_fonts(js: Js) {
    unsafe { ns_font_remove_idle_cb(Some(fonts_idle), js.ptr().cast()) };
}

#[unsafe(no_mangle)]
pub fn nsr_window_fontface_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    font_face::construct(scope, this, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fonts_teardown(js: *mut NsJs) {
    ready::teardown(Js::from_ptr(js));
}
