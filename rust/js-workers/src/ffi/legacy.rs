//! Southstar — the messaging bindings' C-ABI shims over raw QuickJS, kept for the crates that still call them that way.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};

use southstar_js_engine::ContextHandle;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[repr(C)]
pub struct EventAtTarget {
    phase: JSValue,
    current: JSValue,
    nested: c_int,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_at_target_begin(
    ctx: *mut JSContext,
    ev: JSValue,
    target: JSValue,
    st: *mut EventAtTarget,
) {
    let Some(st) = (unsafe { st.as_mut() }) else {
        return;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let ev = quickjs::borrow_value(scope, ev);
            let target = quickjs::borrow_value(scope, target);
            let saved = super::event_at_target_begin(scope, &ev, target);
            st.nested = c_int::from(saved.nested);
            st.phase = quickjs::into_raw(saved.phase);
            st.current = quickjs::into_raw(saved.current);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_at_target_end(
    ctx: *mut JSContext,
    ev: JSValue,
    st: *mut EventAtTarget,
) {
    let Some(st) = (unsafe { st.as_mut() }) else {
        return;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let ev = quickjs::borrow_value(scope, ev);
            let saved = super::AtTarget {
                phase: quickjs::take_value(scope, st.phase),
                current: quickjs::take_value(scope, st.current),
                nested: st.nested != 0,
            };
            st.phase = quickjs::UNDEFINED;
            st.current = quickjs::UNDEFINED;
            super::event_at_target_end(scope, &ev, saved);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_transfer_prepare(
    ctx: *mut JSContext,
    transfer: JSValue,
    source_port: JSValue,
    realm: *mut JSContext,
    old_ports: *mut JSValue,
    new_ports: *mut JSValue,
) -> c_int {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let transfer = quickjs::borrow_value(scope, transfer);
            let source = quickjs::borrow_value(scope, source_port);
            let realm = ContextHandle::from_ptr(realm.cast());
            match super::nsr_port_transfer_prepare(scope, &transfer, &source, realm) {
                Ok((old, new)) => {
                    *old_ports = quickjs::into_raw(old);
                    *new_ports = quickjs::into_raw(new);
                    0
                }
                Err(error) => {
                    *old_ports = quickjs::UNDEFINED;
                    *new_ports = quickjs::UNDEFINED;
                    quickjs::result_raw(scope, Err(error));
                    -1
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_transfer_commit(
    ctx: *mut JSContext,
    old_ports: JSValue,
    new_ports: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let old = quickjs::borrow_value(scope, old_ports);
            let new = quickjs::borrow_value(scope, new_ports);
            super::nsr_port_transfer_commit(scope, &old, &new);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_add_event_listener(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(
            ctx,
            this_val,
            argc,
            argv,
            super::nsr_port_add_event_listener,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_port_remove_event_listener(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(
            ctx,
            this_val,
            argc,
            argv,
            super::nsr_port_remove_event_listener,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_ports(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            super::nsr_net_install_ports(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_transfer_is_port(_ctx: *mut JSContext, v: JSValue) -> c_int {
    let is_port = unsafe { quickjs::with_raw(v, super::nsr_worker_transfer_is_port) };
    c_int::from(is_port)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_report_exception(js: *mut c_void, ex: JSValue) -> c_int {
    let js_ref = super::Js(js as usize);
    let Some(handled) = super::with_js(js_ref, |scope| {
        let ex = unsafe { quickjs::borrow_value(scope, ex) };
        super::nsr_worker_report_exception(js, &ex)
    }) else {
        return 0;
    };
    c_int::from(handled)
}
