//! Southstar — the C ABI shims of the selector queries and live collections for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi;

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn with_value<R>(
    ctx: *mut JSContext,
    value: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> R,
) -> R {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value)
        })
    }
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

natives! {
    ns_element_querySelector => ffi::nsr_element_querySelector,
    ns_element_querySelectorAll => ffi::nsr_element_querySelectorAll,
    ns_document_querySelector => ffi::nsr_document_querySelector,
    ns_document_querySelectorAll => ffi::nsr_document_querySelectorAll,
    ns_element_matches => ffi::nsr_element_matches,
    ns_element_closest => ffi::nsr_element_closest,
    ns_element_getElementById => ffi::nsr_element_getElementById,
    ns_document_getElementById => ffi::nsr_document_getElementById,
    ns_element_getElementsByTagName => ffi::nsr_element_getElementsByTagName,
    ns_document_getElementsByTagName => ffi::nsr_document_getElementsByTagName,
    ns_element_getElementsByTagNameNS => ffi::nsr_element_getElementsByTagNameNS,
    ns_document_getElementsByTagNameNS => ffi::nsr_document_getElementsByTagNameNS,
    ns_element_getElementsByClassName => ffi::nsr_element_getElementsByClassName,
    ns_document_getElementsByClassName => ffi::nsr_document_getElementsByClassName,
    ns_document_getElementsByName => ffi::nsr_document_getElementsByName,
    ns_live_length_get => ffi::nsr_live_length_get,
}

fn opt_bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_nodelist_from_array(ctx: *mut JSContext, array: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let array = quickjs::take_value(scope, array);
            quickjs::into_raw(ffi::nsr_nodelist_from_array(scope, array))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_live(
    ctx: *mut JSContext,
    owner: JSValue,
    kind: c_int,
    param: *const c_char,
) -> JSValue {
    unsafe {
        with_value(ctx, owner, |scope, owner| {
            quickjs::into_raw(ffi::nsr_make_live(
                scope,
                owner,
                kind,
                opt_bytes(param),
                None,
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_collection_kind(obj: JSValue) -> c_int {
    unsafe { quickjs::with_raw(obj, ffi::nsr_live_collection_kind) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_wire_constructors(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, ffi::nsr_live_wire_constructors) }
}
