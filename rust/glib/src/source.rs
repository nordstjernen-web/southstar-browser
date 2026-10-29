//! Southstar — timeout and idle sources on GLib's default main context that run a Rust closure, and removing them by id.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::num::NonZeroU32;
use core::ops::ControlFlow;
use core::ptr;
use std::thread::{self, ThreadId};

use crate::GDestroyNotify;

type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> c_int;

const PRIORITY_DEFAULT: c_int = 0;
const PRIORITY_DEFAULT_IDLE: c_int = 200;

unsafe extern "C" {
    fn g_timeout_add_full(
        priority: c_int,
        interval: c_uint,
        function: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;
    fn g_timeout_add_seconds_full(
        priority: c_int,
        interval: c_uint,
        function: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;
    fn g_idle_add_full(
        priority: c_int,
        function: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;
    fn g_source_remove(tag: c_uint) -> c_int;
    fn g_main_context_find_source_by_id(context: *mut c_void, source_id: c_uint) -> *mut c_void;
    fn g_source_destroy(source: *mut c_void);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceId(NonZeroU32);

impl SourceId {
    pub fn remove(self) {
        // SAFETY: g_source_remove takes any id and only warns when no such source exists.
        unsafe { g_source_remove(self.0.get()) };
    }

    pub fn raw(self) -> u32 {
        self.0.get()
    }

    fn remove_if_attached(self) {
        // SAFETY: a null context is the default one; the lookup only reads GLib's id table.
        let source = unsafe { g_main_context_find_source_by_id(ptr::null_mut(), self.0.get()) };
        if !source.is_null() {
            // SAFETY: the source was found attached to the default context, which holds a reference to it.
            unsafe { g_source_destroy(source) };
        }
    }
}

type Func = Box<dyn FnMut() -> ControlFlow<()>>;

struct Callback {
    owner: Option<ThreadId>,
    func: Func,
}

impl Callback {
    fn require_owner(&self) {
        if self
            .owner
            .is_some_and(|owner| owner != thread::current().id())
        {
            std::process::abort();
        }
    }
}

impl Drop for Callback {
    fn drop(&mut self) {
        self.require_owner();
    }
}

unsafe extern "C" fn dispatch(data: *mut c_void) -> c_int {
    // SAFETY: data is the Callback the source was added with; GLib keeps it alive through the dispatch and never dispatches one source re-entrantly.
    let callback = unsafe { &mut *data.cast::<Callback>() };
    callback.require_owner();
    c_int::from((callback.func)().is_continue())
}

unsafe extern "C" fn destroy(data: *mut c_void) {
    // SAFETY: GLib hands back the Callback the source was added with, exactly once, when the source is destroyed.
    drop(unsafe { Box::from_raw(data.cast::<Callback>()) });
}

enum Kind {
    Timeout(u32),
    TimeoutSeconds(u32),
    Idle,
}

fn add(kind: Kind, owner: Option<ThreadId>, func: Func) -> SourceId {
    let data = Box::into_raw(Box::new(Callback { owner, func })).cast::<c_void>();
    let id = match kind {
        // SAFETY: the source owns data and frees it with destroy; dispatch reads it as the Callback it is.
        Kind::Timeout(ms) => unsafe {
            g_timeout_add_full(PRIORITY_DEFAULT, ms, dispatch, data, Some(destroy))
        },
        // SAFETY: as above.
        Kind::TimeoutSeconds(s) => unsafe {
            g_timeout_add_seconds_full(PRIORITY_DEFAULT, s, dispatch, data, Some(destroy))
        },
        // SAFETY: as above.
        Kind::Idle => unsafe {
            g_idle_add_full(PRIORITY_DEFAULT_IDLE, dispatch, data, Some(destroy))
        },
    };
    SourceId(NonZeroU32::new(id).expect("GLib source ids are never 0"))
}

fn once(func: impl FnOnce() + 'static) -> Func {
    let mut func = Some(func);
    Box::new(move || {
        if let Some(func) = func.take() {
            func();
        }
        ControlFlow::Break(())
    })
}

pub fn timeout_add_local(ms: u32, func: impl FnMut() -> ControlFlow<()> + 'static) -> SourceId {
    add(
        Kind::Timeout(ms),
        Some(thread::current().id()),
        Box::new(func),
    )
}

pub fn timeout_add_local_once(ms: u32, func: impl FnOnce() + 'static) -> SourceId {
    add(Kind::Timeout(ms), Some(thread::current().id()), once(func))
}

pub fn timeout_add_seconds_local(
    seconds: u32,
    func: impl FnMut() -> ControlFlow<()> + 'static,
) -> SourceId {
    add(
        Kind::TimeoutSeconds(seconds),
        Some(thread::current().id()),
        Box::new(func),
    )
}

pub fn idle_add_local(func: impl FnMut() -> ControlFlow<()> + 'static) -> SourceId {
    add(Kind::Idle, Some(thread::current().id()), Box::new(func))
}

pub fn idle_add_local_once(func: impl FnOnce() + 'static) -> SourceId {
    add(Kind::Idle, Some(thread::current().id()), once(func))
}

pub fn timeout_add_once(ms: u32, func: impl FnOnce() + Send + 'static) -> SourceId {
    add(Kind::Timeout(ms), None, once(func))
}

pub fn idle_add_once(func: impl FnOnce() + Send + 'static) -> SourceId {
    add(Kind::Idle, None, once(func))
}

type ScopedFunc<'env> = Box<dyn FnMut() -> ControlFlow<()> + 'env>;

pub struct Scope<'env> {
    sources: RefCell<Vec<SourceId>>,
    env: PhantomData<*mut &'env ()>,
}

impl<'env> Scope<'env> {
    fn track(&self, func: ScopedFunc<'env>, kind: Kind) -> SourceId {
        // SAFETY: only the lifetime is erased; scope() destroys every source this Scope added, and with it the closure, before 'env ends.
        let func: Func = unsafe { core::mem::transmute::<ScopedFunc<'env>, Func>(func) };
        let id = add(kind, Some(thread::current().id()), func);
        self.sources.borrow_mut().push(id);
        id
    }

    pub fn timeout_add(&self, ms: u32, func: impl FnMut() -> ControlFlow<()> + 'env) -> SourceId {
        self.track(Box::new(func), Kind::Timeout(ms))
    }

    pub fn timeout_add_once(&self, ms: u32, func: impl FnOnce() + 'env) -> SourceId {
        let mut func = Some(func);
        self.timeout_add(ms, move || {
            if let Some(func) = func.take() {
                func();
            }
            ControlFlow::Break(())
        })
    }
}

struct ScopeEnd<'a, 'env>(&'a Scope<'env>);

impl Drop for ScopeEnd<'_, '_> {
    fn drop(&mut self) {
        let sources = core::mem::take(&mut *self.0.sources.borrow_mut());
        for source in sources {
            source.remove_if_attached();
        }
    }
}

pub fn scope<'env, R>(body: impl FnOnce(&Scope<'env>) -> R) -> R {
    let scope = Scope {
        sources: RefCell::new(Vec::new()),
        env: PhantomData,
    };
    let _end = ScopeEnd(&scope);
    body(&scope)
}
