//! Southstar — the C ABI shims of the window bindings for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::NsNode;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use super::{Js, NsJs, current_realm};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_bind_post_message(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            super::nsr_window_bind_post_message(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_make_post_message(
    ctx: *mut JSContext,
    window: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            quickjs::into_raw(super::nsr_window_make_post_message(scope, &window))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_link_outward(
    js: *const NsJs,
    outward: JSValue,
    realm_window: JSValue,
) {
    let Some(realm) = current_realm(Js::of(js)) else {
        return;
    };
    realm.enter(|scope| {
        let outward = unsafe { quickjs::borrow_value(scope, outward) };
        let realm_window = unsafe { quickjs::borrow_value(scope, realm_window) };
        unsafe { super::nsr_window_link_outward(js, &outward, &realm_window) };
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_window_event(ctx: *mut JSContext, kind: *const c_char) -> JSValue {
    let kind = if kind.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(kind) }
            .to_string_lossy()
            .into_owned()
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(super::nsr_make_window_event(scope, &kind))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_make_location(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(super::nsr_window_make_location(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_open_method(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, super::nsr_window_open_method) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_current_document_for(
    ctx: *mut JSContext,
    window: JSValue,
) -> *mut NsNode {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            super::nsr_window_current_document_for(scope, &window)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_document_for(
    ctx: *mut JSContext,
    window: JSValue,
) -> *mut NsNode {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            super::nsr_window_document_for(scope, &window)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_expose_legacy_named(
    ctx: *mut JSContext,
    root: *const NsNode,
    document: JSValue,
) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let document = quickjs::borrow_value(scope, document);
            super::nsr_document_expose_legacy_named(scope, root, &document);
        })
    }
}
