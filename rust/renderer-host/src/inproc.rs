//! Southstar — single-process mode, as declared in src/rproc_inproc.h: every tab's renderer session served on the window's main context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use southstar_glib::{GBoolean, GPtrArray};
use southstar_ipc::{Conn, Head, until_nul};

use crate::engine::{self, PrintSetup};
use crate::ffi::{RendererSession, new_session};

#[repr(C)]
struct GSource {
    _private: [u8; 0],
}

type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_idle_source_new() -> *mut GSource;
    fn g_source_set_priority(source: *mut GSource, priority: c_int);
    fn g_source_set_callback(
        source: *mut GSource,
        func: SourceFn,
        data: *mut c_void,
        notify: *mut c_void,
    );
    fn g_source_attach(source: *mut GSource, context: *mut c_void) -> u32;
    fn g_source_unref(source: *mut GSource);
    fn g_timeout_add(interval: u32, function: SourceFn, data: *mut c_void) -> u32;
    fn g_atomic_rc_box_acquire(mem_block: *mut c_void) -> *mut c_void;
    fn g_atomic_rc_box_release(mem_block: *mut c_void);
    #[cfg(unix)]
    fn close(fd: c_int) -> c_int;
    #[cfg(windows)]
    fn _close(fd: c_int) -> c_int;
}

struct Connection {
    ctrl_r: c_int,
    ctrl_w: c_int,
    fb: *mut u8,
    session: RendererSession,
}

struct PrintJob {
    setup: usize,
    pages: Mutex<Option<usize>>,
    done: Condvar,
}

enum Work {
    Request { path: Vec<u8>, body: Vec<u8> },
    Close,
    Print(Arc<PrintJob>),
}

struct Item {
    conn: *mut Connection,
    work: Work,
}

unsafe impl Send for Item {}

struct ConnPtr(*mut Connection);

unsafe impl Send for ConnPtr {}

static ENABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static PENDING: RefCell<VecDeque<Box<Item>>> = const { RefCell::new(VecDeque::new()) };
    static HANDLING: Cell<bool> = const { Cell::new(false) };
    static RETRY_SCHEDULED: Cell<bool> = const { Cell::new(false) };
    static ENGINE_INITED: Cell<bool> = const { Cell::new(false) };
}

fn close_fd(fd: c_int) {
    #[cfg(unix)]
    unsafe {
        close(fd)
    };
    #[cfg(windows)]
    unsafe {
        _close(fd)
    };
}

fn close_connection(conn: *mut Connection) {
    let conn = unsafe { Box::from_raw(conn) };
    let Connection {
        ctrl_r,
        ctrl_w,
        fb,
        session,
    } = *conn;
    drop(session);
    unsafe { g_atomic_rc_box_release(fb.cast()) };
    close_fd(ctrl_r);
    if cfg!(windows) {
        close_fd(ctrl_w);
    }
}

fn run(item: Item) {
    let Item { conn, work } = item;
    match work {
        Work::Print(job) => {
            let session = unsafe { &mut (*conn).session };
            let setup = unsafe { &mut *(job.setup as *mut PrintSetup) };
            let pages = session.print(setup).map_or(0, |p| p.into_raw() as usize);
            *job.pages.lock().unwrap_or_else(|e| e.into_inner()) = Some(pages);
            job.done.notify_one();
        }
        Work::Close => close_connection(conn),
        Work::Request { path, body } => {
            let session = unsafe { &mut (*conn).session };
            session.handle(&path, until_nul(&body));
        }
    }
}

fn busy(item: &Item) -> bool {
    !item.conn.is_null() && unsafe { &mut (*item.conn).session }.busy()
}

fn head_busy() -> Option<bool> {
    PENDING.with(|p| p.borrow().front().map(|item| busy(item)))
}

fn schedule_retry() {
    if RETRY_SCHEDULED.get() {
        return;
    }
    RETRY_SCHEDULED.set(true);
    unsafe { g_timeout_add(5, retry_tick, ptr::null_mut()) };
}

fn run_queue() {
    HANDLING.set(true);
    if !ENGINE_INITED.get() {
        ENGINE_INITED.set(true);
        engine::init();
    }
    while head_busy() == Some(false) {
        let Some(item) = PENDING.with(|p| p.borrow_mut().pop_front()) else {
            break;
        };
        run(*item);
    }
    HANDLING.set(false);
    if PENDING.with(|p| !p.borrow().is_empty()) {
        schedule_retry();
    }
}

unsafe extern "C" fn retry_tick(_data: *mut c_void) -> GBoolean {
    if HANDLING.get() {
        return 1;
    }
    let head = head_busy();
    if head == Some(true) {
        return 1;
    }
    RETRY_SCHEDULED.set(false);
    if head.is_some() {
        run_queue();
    }
    0
}

unsafe extern "C" fn dispatch(data: *mut c_void) -> GBoolean {
    let item = unsafe { Box::from_raw(data.cast::<Item>()) };
    PENDING.with(|p| p.borrow_mut().push_back(item));
    if HANDLING.get() {
        return 0;
    }
    if head_busy() == Some(true) {
        schedule_retry();
        return 0;
    }
    run_queue();
    0
}

fn post(item: Item) {
    let data = Box::into_raw(Box::new(item)).cast::<c_void>();
    unsafe {
        let source = g_idle_source_new();
        g_source_set_priority(source, 0);
        g_source_set_callback(source, dispatch, data, ptr::null_mut());
        g_source_attach(source, ptr::null_mut());
        g_source_unref(source);
    }
}

fn read_requests(conn: ConnPtr, ctrl_r: c_int) {
    let conn = conn.0;
    let mut channel = Conn::boxed(ctrl_r);
    let mut head = Head::boxed();
    loop {
        if !channel.read_head(&mut head) {
            break;
        }
        let mut body = vec![0u8; head.content_length as usize];
        if !body.is_empty() && !channel.read_body(&mut body) {
            break;
        }
        let path = until_nul(&head.path).to_vec();
        post(Item {
            conn,
            work: Work::Request { path, body },
        });
    }
    post(Item {
        conn,
        work: Work::Close,
    });
}

unsafe extern "C" fn attach(
    ctrl_r: c_int,
    ctrl_w: c_int,
    fb: *mut u8,
    max_w: c_int,
    max_h: c_int,
) -> *mut c_void {
    let Some(session) = new_session(ctrl_w, fb, max_w, max_h, true) else {
        return ptr::null_mut();
    };
    let conn = Box::into_raw(Box::new(Connection {
        ctrl_r,
        ctrl_w,
        fb: unsafe { g_atomic_rc_box_acquire(fb.cast()) }.cast(),
        session,
    }));
    let shared = ConnPtr(conn);
    let spawned = std::thread::Builder::new()
        .name("ns-inproc-read".to_owned())
        .spawn(move || read_requests(shared, ctrl_r));
    if spawned.is_err() {
        let conn = unsafe { Box::from_raw(conn) };
        unsafe { g_atomic_rc_box_release(conn.fb.cast()) };
        return ptr::null_mut();
    }
    conn.cast()
}

unsafe extern "C" fn print(conn: *mut c_void, out_setup: *mut c_void) -> *mut GPtrArray {
    let job = Arc::new(PrintJob {
        setup: out_setup as usize,
        pages: Mutex::new(None),
        done: Condvar::new(),
    });
    post(Item {
        conn: conn.cast(),
        work: Work::Print(Arc::clone(&job)),
    });
    let mut pages = job.pages.lock().unwrap_or_else(|e| e.into_inner());
    while pages.is_none() {
        pages = job.done.wait(pages).unwrap_or_else(|e| e.into_inner());
    }
    pages.unwrap_or(0) as *mut GPtrArray
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_single_process_enable() {
    ENABLED.store(true, Ordering::SeqCst);
    southstar_renderer_client::set_inproc(Some(attach));
    southstar_renderer_client::set_inproc_print(Some(print));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rproc_single_process_enabled() -> c_int {
    c_int::from(ENABLED.load(Ordering::SeqCst))
}
