//! Southstar — timeout and idle sources that run a Rust closure on GLib's default main context or a given one, and removing them by id.
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

#[cfg(unix)]
pub type RawPid = c_int;
#[cfg(windows)]
pub type RawPid = *mut c_void;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Pid(RawPid);

impl Pid {
    pub fn from_raw(raw: RawPid) -> Pid {
        Pid(raw)
    }

    pub fn raw(self) -> RawPid {
        self.0
    }
}

type ChildWatchFunc = unsafe extern "C" fn(pid: RawPid, status: c_int, data: *mut c_void);

const PRIORITY_DEFAULT: c_int = 0;

unsafe extern "C" {
    fn g_timeout_source_new(interval: c_uint) -> *mut c_void;
    fn g_timeout_source_new_seconds(interval: c_uint) -> *mut c_void;
    fn g_idle_source_new() -> *mut c_void;
    fn g_source_set_callback(
        source: *mut c_void,
        func: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    );
    fn g_source_attach(source: *mut c_void, context: *mut c_void) -> c_uint;
    fn g_source_unref(source: *mut c_void);
    fn g_source_remove(tag: c_uint) -> c_int;
    fn g_main_context_find_source_by_id(context: *mut c_void, source_id: c_uint) -> *mut c_void;
    fn g_source_destroy(source: *mut c_void);
    fn g_child_watch_add_full(
        priority: c_int,
        pid: RawPid,
        function: ChildWatchFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;
    #[cfg(unix)]
    fn g_unix_signal_add_full(
        priority: c_int,
        signum: c_int,
        handler: SourceFunc,
        data: *mut c_void,
        notify: GDestroyNotify,
    ) -> c_uint;
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
        remove_from(ptr::null_mut(), self);
    }
}

pub(crate) type Func = Box<dyn FnMut() -> ControlFlow<()>>;

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

pub(crate) enum Kind {
    Timeout(u32),
    TimeoutSeconds(u32),
    Idle,
}

pub(crate) fn remove_from(context: *mut c_void, id: SourceId) {
    // SAFETY: context is null (the default context) or one the caller holds a reference to; the lookup only reads its id table.
    let source = unsafe { g_main_context_find_source_by_id(context, id.0.get()) };
    if !source.is_null() {
        // SAFETY: the source was found attached to the context, which holds a reference to it.
        unsafe { g_source_destroy(source) };
    }
}

pub(crate) fn attach(
    context: *mut c_void,
    kind: Kind,
    owner: Option<ThreadId>,
    func: Func,
) -> SourceId {
    let source = match kind {
        // SAFETY: creating a source has no preconditions.
        Kind::Timeout(ms) => unsafe { g_timeout_source_new(ms) },
        // SAFETY: as above.
        Kind::TimeoutSeconds(s) => unsafe { g_timeout_source_new_seconds(s) },
        // SAFETY: as above.
        Kind::Idle => unsafe { g_idle_source_new() },
    };
    let data = Box::into_raw(Box::new(Callback { owner, func })).cast::<c_void>();
    // SAFETY: the source takes ownership of data and frees it with destroy; dispatch reads it as the Callback it is.
    unsafe { g_source_set_callback(source, dispatch, data, Some(destroy)) };
    // SAFETY: context is null (the default context) or one the caller holds a reference to; attaching adds the context's own reference to the source.
    let id = unsafe { g_source_attach(source, context) };
    // SAFETY: this gives up the creating reference; the context keeps the source until it is destroyed.
    unsafe { g_source_unref(source) };
    SourceId(NonZeroU32::new(id).expect("GLib source ids are never 0"))
}

fn add(kind: Kind, owner: Option<ThreadId>, func: Func) -> SourceId {
    attach(ptr::null_mut(), kind, owner, func)
}

pub(crate) fn once(func: impl FnOnce() + 'static) -> Func {
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

struct ChildCallback {
    owner: ThreadId,
    func: Option<Box<dyn FnOnce(i32)>>,
}

impl Drop for ChildCallback {
    fn drop(&mut self) {
        if self.owner != thread::current().id() {
            std::process::abort();
        }
    }
}

unsafe extern "C" fn child_exited(_pid: RawPid, status: c_int, data: *mut c_void) {
    // SAFETY: data is the ChildCallback the watch was added with; GLib keeps it alive through the dispatch.
    let callback = unsafe { &mut *data.cast::<ChildCallback>() };
    if callback.owner != thread::current().id() {
        std::process::abort();
    }
    if let Some(func) = callback.func.take() {
        func(status);
    }
}

unsafe extern "C" fn drop_child_callback(data: *mut c_void) {
    // SAFETY: GLib hands back the ChildCallback the watch was added with, exactly once, when the watch is destroyed.
    drop(unsafe { Box::from_raw(data.cast::<ChildCallback>()) });
}

pub fn child_watch_add_local(pid: Pid, func: impl FnOnce(i32) + 'static) -> SourceId {
    let callback = ChildCallback {
        owner: thread::current().id(),
        func: Some(Box::new(func)),
    };
    let data = Box::into_raw(Box::new(callback)).cast::<c_void>();
    // SAFETY: the watch owns data and frees it with drop_child_callback; child_exited reads it as the ChildCallback it is.
    let id = unsafe {
        g_child_watch_add_full(
            PRIORITY_DEFAULT,
            pid.0,
            child_exited,
            data,
            Some(drop_child_callback),
        )
    };
    SourceId(NonZeroU32::new(id).expect("GLib source ids are never 0"))
}

#[cfg(unix)]
pub fn unix_signal_add_local(
    signal: i32,
    func: impl FnMut() -> ControlFlow<()> + 'static,
) -> SourceId {
    let callback = Callback {
        owner: Some(thread::current().id()),
        func: Box::new(func),
    };
    let data = Box::into_raw(Box::new(callback)).cast::<c_void>();
    // SAFETY: the source owns data and frees it with destroy; dispatch reads it as the Callback it is.
    let id =
        unsafe { g_unix_signal_add_full(PRIORITY_DEFAULT, signal, dispatch, data, Some(destroy)) };
    SourceId(NonZeroU32::new(id).expect("GLib source ids are never 0"))
}
