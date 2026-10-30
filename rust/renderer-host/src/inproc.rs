//! Southstar — single-process mode: every tab's renderer session served on the window's main context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ops::ControlFlow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use southstar_cairo::Surface;
use southstar_glib::MainContext;
use southstar_ipc::{Conn, Head, until_nul};
use southstar_renderer_client::{InprocId, SharedFrame};

use crate::engine::{self, PrintPages, PrintSetup};
use crate::ffi::{RendererSession, new_session};
use crate::frame::FrameMemory;

unsafe extern "C" {
    #[cfg(unix)]
    fn close(fd: c_int) -> c_int;
    #[cfg(windows)]
    fn _close(fd: c_int) -> c_int;
}

struct Connection {
    ctrl_r: c_int,
    ctrl_w: c_int,
    session: RendererSession,
}

struct Printed(Option<Vec<Surface>>);

// SAFETY: cairo counts surface references atomically; the main thread gives up the printed surfaces whole, and the print caller only passes them on to the window's main loop to paint.
unsafe impl Send for Printed {}

struct PrintJob {
    result: Mutex<Option<(Printed, PrintSetup)>>,
    done: Condvar,
}

enum Work {
    Open {
        ctrl_r: c_int,
        ctrl_w: c_int,
        frame: Arc<SharedFrame>,
        max_w: c_int,
        max_h: c_int,
    },
    Request {
        path: Vec<u8>,
        body: Vec<u8>,
    },
    Close,
    Forget,
    Print {
        setup: PrintSetup,
        job: Arc<PrintJob>,
    },
}

struct Item {
    conn: InprocId,
    work: Work,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static CONNECTIONS: RefCell<HashMap<InprocId, Rc<RefCell<Connection>>>> =
        RefCell::new(HashMap::new());
    static PENDING: RefCell<VecDeque<Item>> = const { RefCell::new(VecDeque::new()) };
    static HANDLING: Cell<bool> = const { Cell::new(false) };
    static RETRY_SCHEDULED: Cell<bool> = const { Cell::new(false) };
    static ENGINE_INITED: Cell<bool> = const { Cell::new(false) };
}

fn close_fd(fd: c_int) {
    #[cfg(unix)]
    // SAFETY: the connection owned this descriptor and was just removed, so nothing reads or writes it any more.
    unsafe {
        close(fd)
    };
    #[cfg(windows)]
    // SAFETY: the connection owned this CRT descriptor and was just removed, so nothing reads or writes it any more.
    unsafe {
        _close(fd)
    };
}

fn connection(id: InprocId) -> Option<Rc<RefCell<Connection>>> {
    CONNECTIONS.with(|c| c.borrow().get(&id).cloned())
}

fn remove_connection(id: InprocId) -> Option<Connection> {
    let conn = CONNECTIONS.with(|c| c.borrow_mut().remove(&id))?;
    Rc::into_inner(conn).map(RefCell::into_inner)
}

fn close_connection(id: InprocId) {
    let Some(Connection {
        ctrl_r,
        ctrl_w,
        session,
    }) = remove_connection(id)
    else {
        return;
    };
    drop(session);
    close_fd(ctrl_r);
    if cfg!(windows) {
        close_fd(ctrl_w);
    }
}

fn open_connection(
    id: InprocId,
    ctrl_r: c_int,
    ctrl_w: c_int,
    frame: Arc<SharedFrame>,
    max_w: c_int,
    max_h: c_int,
) {
    let memory = FrameMemory::shared(frame);
    if let Some(session) = new_session(ctrl_w, memory, max_w, max_h, true) {
        let conn = Rc::new(RefCell::new(Connection {
            ctrl_r,
            ctrl_w,
            session,
        }));
        CONNECTIONS.with(|c| c.borrow_mut().insert(id, conn));
    }
}

fn print_pages(id: InprocId, setup: &mut PrintSetup) -> Option<Vec<Surface>> {
    let conn = connection(id)?;
    let pages = conn.borrow_mut().session.print(setup);
    pages.map(PrintPages::into_surfaces)
}

fn run(item: Item) {
    let Item { conn: id, work } = item;
    match work {
        Work::Open {
            ctrl_r,
            ctrl_w,
            frame,
            max_w,
            max_h,
        } => open_connection(id, ctrl_r, ctrl_w, frame, max_w, max_h),
        Work::Print { mut setup, job } => {
            let pages = print_pages(id, &mut setup);
            *job.result.lock().unwrap_or_else(|e| e.into_inner()) = Some((Printed(pages), setup));
            job.done.notify_one();
        }
        Work::Close => close_connection(id),
        Work::Forget => drop(remove_connection(id)),
        Work::Request { path, body } => {
            if let Some(conn) = connection(id) {
                conn.borrow_mut().session.handle(&path, until_nul(&body));
            }
        }
    }
}

fn busy(item: &Item) -> bool {
    connection(item.conn).is_some_and(|conn| conn.borrow_mut().session.busy())
}

fn head_busy() -> Option<bool> {
    PENDING.with(|p| p.borrow().front().map(busy))
}

fn schedule_retry() {
    if RETRY_SCHEDULED.get() {
        return;
    }
    RETRY_SCHEDULED.set(true);
    southstar_glib::timeout_add_local(5, retry_tick);
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
        run(item);
    }
    HANDLING.set(false);
    if PENDING.with(|p| !p.borrow().is_empty()) {
        schedule_retry();
    }
}

fn retry_tick() -> ControlFlow<()> {
    if HANDLING.get() {
        return ControlFlow::Continue(());
    }
    let head = head_busy();
    if head == Some(true) {
        return ControlFlow::Continue(());
    }
    RETRY_SCHEDULED.set(false);
    if head.is_some() {
        run_queue();
    }
    ControlFlow::Break(())
}

fn dispatch(item: Item) {
    PENDING.with(|p| p.borrow_mut().push_back(item));
    if HANDLING.get() {
        return;
    }
    if head_busy() == Some(true) {
        schedule_retry();
        return;
    }
    run_queue();
}

fn post(conn: InprocId, work: Work) {
    let item = Item { conn, work };
    MainContext::global().defer(move || dispatch(item));
}

fn read_requests(conn: InprocId, ctrl_r: c_int) {
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
        post(conn, Work::Request { path, body });
    }
    post(conn, Work::Close);
}

fn frame_len(max_w: c_int, max_h: c_int) -> Option<usize> {
    let (w, h) = (usize::try_from(max_w).ok()?, usize::try_from(max_h).ok()?);
    if w == 0 || h == 0 {
        return None;
    }
    w.checked_mul(h)?.checked_mul(4)
}

fn attach(
    ctrl_r: c_int,
    ctrl_w: c_int,
    frame: Arc<SharedFrame>,
    max_w: c_int,
    max_h: c_int,
) -> Option<InprocId> {
    let len = frame_len(max_w, max_h)?;
    if frame.len() < len {
        return None;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    post(
        id,
        Work::Open {
            ctrl_r,
            ctrl_w,
            frame,
            max_w,
            max_h,
        },
    );
    let spawned = std::thread::Builder::new()
        .name("ns-inproc-read".to_owned())
        .spawn(move || read_requests(id, ctrl_r));
    if spawned.is_err() {
        post(id, Work::Forget);
        return None;
    }
    Some(id)
}

fn print(conn: InprocId, setup: &mut PrintSetup) -> Option<Vec<Surface>> {
    let job = Arc::new(PrintJob {
        result: Mutex::new(None),
        done: Condvar::new(),
    });
    post(
        conn,
        Work::Print {
            setup: *setup,
            job: Arc::clone(&job),
        },
    );
    let mut result = job.result.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if let Some((Printed(pages), used)) = result.take() {
            *setup = used;
            return pages;
        }
        result = job.done.wait(result).unwrap_or_else(|e| e.into_inner());
    }
}

pub fn single_process_enable() {
    ENABLED.store(true, Ordering::SeqCst);
    southstar_renderer_client::set_inproc(Some(attach));
    southstar_renderer_client::set_inproc_print(Some(print));
}

pub fn single_process_enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn single_process_shutdown() {
    PENDING.with(|p| p.borrow_mut().clear());
    let open: Vec<InprocId> = CONNECTIONS.with(|c| c.borrow().keys().copied().collect());
    for id in open {
        close_connection(id);
    }
}
