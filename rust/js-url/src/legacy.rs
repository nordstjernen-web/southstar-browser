//! Southstar — the C-ABI QuickJS shims of the FileReader installer and blob byte reader, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib as glib;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_file_reader(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::ffi::nsr_net_install_file_reader(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_blob_bytes_as_string(
    ctx: *mut JSContext,
    blob: JSValue,
    out_len: *mut usize,
) -> *mut c_char {
    let bytes = unsafe {
        quickjs::with_context(ctx, |scope| {
            let blob = quickjs::borrow_value(scope, blob);
            crate::ffi::nsr_blob_bytes(scope, &blob)
        })
    };
    if !out_len.is_null() {
        unsafe { *out_len = bytes.len() };
    }
    let out = unsafe { glib::g_malloc(bytes.len() + 1) }.cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    out.cast()
}
