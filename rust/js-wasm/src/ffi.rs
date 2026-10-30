//! Southstar — the Rust-ABI installer of the WebAssembly JS API and the context and caller hand-off that lets wasm and JavaScript call each other.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::cell::Cell;
use core::ffi::c_void;
use core::ptr;
use std::rc::Rc;

use southstar_js_engine::{ContextHandle, Scope, Value};
use wasmi::{AsContextMut, Caller, StoreContextMut};

use crate::realm::HostState;

pub(crate) struct Entry {
    context: Cell<ContextHandle>,
    caller: Cell<*mut c_void>,
}

impl Entry {
    pub fn new() -> Entry {
        Entry {
            context: Cell::new(ContextHandle::NULL),
            caller: Cell::new(ptr::null_mut()),
        }
    }
}

pub(crate) fn entered<R>(scope: &mut Scope<'_>, entry: &Entry, f: impl FnOnce() -> R) -> R {
    let previous = entry.context.replace(scope.handle());
    let result = f();
    entry.context.set(previous);
    result
}

pub(crate) fn in_import<R>(
    entry: &Entry,
    caller: &mut Caller<'_, HostState>,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> Option<R> {
    let handle = entry.context.get().non_null()?;
    let previous = entry
        .caller
        .replace((caller as *mut Caller<'_, HostState>).cast());
    let result = handle.with(f);
    entry.caller.set(previous);
    result
}

pub(crate) fn with_active_caller<R, F: FnOnce(StoreContextMut<'_, HostState>) -> R>(
    entry: &Entry,
    f: F,
) -> Result<R, F> {
    let caller = entry.caller.get().cast::<Caller<'_, HostState>>();
    // SAFETY: `caller` is null or the `Caller` `in_import` stored for exactly the duration of
    // the import call it is running, which is still on the stack while `f` runs on it.
    match unsafe { caller.as_mut() } {
        Some(caller) => Ok(f(caller.as_context_mut())),
        None => Err(f),
    }
}

pub(crate) fn memory_buffer(
    scope: &mut Scope<'_>,
    data: *mut u8,
    len: usize,
    store: Rc<dyn Any>,
) -> Result<Value, Value> {
    // SAFETY: `data..data + len` is the wasm memory owned by the store `store` keeps alive, and
    // `Realm::sync_views` detaches the buffer before a grow can move or shrink that memory.
    unsafe { scope.external_array_buffer(data, len, Some(store)) }
}
