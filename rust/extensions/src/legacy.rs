//! Southstar — the C-ABI QuickJS shim of the content script lookup, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ext_content_scripts_for_url(
    ctx: *mut JSContext,
    global: JSValue,
    url: *const c_char,
    at_start: GBoolean,
) -> *mut c_char {
    let url = unsafe { glib::bytes(url) };
    let script = unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::ffi::nsr_ext_content_scripts_for_url(scope, &global, url, at_start != 0)
        })
    };
    script.map_or(ptr::null_mut(), |script| glib::strdup(&script))
}
