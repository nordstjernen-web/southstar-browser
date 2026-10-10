//! Southstar — the C ABI of the WebAssembly JS API and the raw context and caller hand-off that lets wasm and JavaScript call each other.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::c_void;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};
use wasmi::{AsContextMut, Caller, StoreContextMut};

use crate::realm::HostState;

pub(crate) struct Entry {
    context: Cell<*mut JSContext>,
    caller: Cell<*mut c_void>,
}

impl Entry {
    pub fn new() -> Entry {
        Entry {
            context: Cell::new(ptr::null_mut()),
            caller: Cell::new(ptr::null_mut()),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_wasm_install(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            drop(crate::install(scope, &global));
        })
    }
}

pub(crate) fn entered<R>(scope: &mut Scope<'_>, entry: &Entry, f: impl FnOnce() -> R) -> R {
    let previous = entry.context.replace(quickjs::raw_context(scope));
    let result = f();
    entry.context.set(previous);
    result
}

pub(crate) fn in_import<R>(
    entry: &Entry,
    caller: &mut Caller<'_, HostState>,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> Option<R> {
    let ctx = entry.context.get();
    if ctx.is_null() {
        return None;
    }
    let previous = entry
        .caller
        .replace((caller as *mut Caller<'_, HostState>).cast());
    let result = unsafe { quickjs::with_context(ctx, f) };
    entry.caller.set(previous);
    Some(result)
}

pub(crate) fn with_active_caller<R, F: FnOnce(StoreContextMut<'_, HostState>) -> R>(
    entry: &Entry,
    f: F,
) -> Result<R, F> {
    let caller = entry.caller.get().cast::<Caller<'_, HostState>>();
    match unsafe { caller.as_mut() } {
        Some(caller) => Ok(f(caller.as_context_mut())),
        None => Err(f),
    }
}

pub(crate) fn memory_buffer(
    scope: &mut Scope<'_>,
    data: *mut u8,
    len: usize,
) -> Result<Value, Value> {
    unsafe { scope.external_array_buffer(data, len) }
}
