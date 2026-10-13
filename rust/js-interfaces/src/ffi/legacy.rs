//! Southstar — the C-ABI interface installers raw-QuickJS callers still declare; each goes when its last caller moves to the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

unsafe fn with_object(ctx: *mut JSContext, raw: JSValue, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, raw);
            f(scope, &value);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_hasinstance(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_has_instance) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_tostringtag(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_to_string_tag) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_dom_hierarchy(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_dom_hierarchy) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_install_funcs(ctx: *mut JSContext, doc: JSValue) {
    unsafe { with_object(ctx, doc, super::document_install_funcs) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_install_proto_table(
    ctx: *mut JSContext,
    proto: JSValue,
    table: c_int,
) {
    unsafe {
        with_object(ctx, proto, |scope, proto| {
            super::install_proto_table(scope, proto, table)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_host_class_proto(ctx: *mut JSContext, which: c_int) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(super::host_class_proto(scope, which))
        })
    }
}
