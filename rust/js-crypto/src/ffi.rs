//! Southstar — the C ABI of the WebCrypto bindings as declared in src/js_internal.h, and the CSPRNG they draw from.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

unsafe extern "C" {
    fn ns_security_csprng_fill(buf: *mut c_void, len: usize) -> GBoolean;
}

pub(crate) fn csprng_fill(bytes: &mut [u8]) -> bool {
    unsafe { ns_security_csprng_fill(bytes.as_mut_ptr().cast(), bytes.len()) != 0 }
}

unsafe fn with_global(ctx: *mut JSContext, global: JSValue, f: fn(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            f(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_install_window(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::install::install_window) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_install_window_subtle(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::install::install_window_subtle) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_crypto_install_worker(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_global(ctx, global, crate::install::install_worker) }
}
