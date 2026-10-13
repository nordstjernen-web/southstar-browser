//! Southstar — the C ABI and Rust-ABI entry points of the font loading bindings, and the realm and font loader calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_void};

use southstar_js_engine::{ContextHandle, Scope, Value};

use crate::{font_face, font_set, ready};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

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
    fn ns_js_flush_layout(js: *mut NsJs);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_main_context(js: *const NsJs) -> ContextHandle;
    fn ns_drain_microtasks(js: *mut NsJs);
    fn ns_font_pending_count() -> c_uint;
    fn ns_font_add_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
    fn ns_font_remove_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(scope.context_opaque().cast())
}

pub(crate) fn flush_layout(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_flush_layout(js.ptr()) };
    }
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.ptr()) };
}

pub(crate) fn drain_microtasks(js: Js) {
    unsafe { ns_drain_microtasks(js.ptr()) };
}

pub(crate) fn pending_font_count() -> u32 {
    unsafe { ns_font_pending_count() }
}

unsafe extern "C" fn fonts_idle(user_data: *mut c_void) {
    let js = Js::from_ptr(user_data.cast());
    if js.is_null() || !ready::has_waiting(js) {
        return;
    }
    unsafe { ns_js_main_context(js.ptr()) }.with(|scope| ready::fonts_idle(scope, js));
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

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub fn nsr_document_get_fonts(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    font_set::document_fonts(scope, this, &[])
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fonts_teardown(js: *mut NsJs) {
    ready::teardown(Js::from_ptr(js));
}
