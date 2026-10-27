//! Southstar — image fetches for a laid-out page: the wanted-URL set, the blocking fetch on a nested loop and the incremental sessions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_void};
use core::ptr::NonNull;
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;

use super::net::{self, Dest, NestedLoop, Response};

pub use southstar_layout_query::collect_images;

#[derive(Clone, Copy)]
pub struct ImageCache(southstar_image::ImageCache);

impl ImageCache {
    pub fn of(cache: southstar_image::ImageCache) -> ImageCache {
        ImageCache(cache)
    }

    pub fn peek(self, url: &CStr) -> bool {
        self.0.peek(url).is_some()
    }

    fn store(self, url: &CStr, resp: &Response) {
        let Some(body) = resp.body().filter(|b| !b.is_empty()) else {
            return;
        };
        if resp.error().is_some() {
            return;
        }
        if let Some(image) = self.0.insert_encoded(url, body) {
            image.adopt_origin(resp.final_url(), resp.cors_allow_origin());
        }
    }
}

#[derive(Default)]
pub struct Wanted {
    order: RefCell<Vec<CString>>,
    set: RefCell<HashSet<CString>>,
}

impl Wanted {
    pub fn new() -> Wanted {
        Wanted::default()
    }

    pub fn contains(&self, url: &CStr) -> bool {
        self.set.borrow().contains(url)
    }

    pub fn add(&self, url: &CStr) {
        if self.set.borrow_mut().insert(url.to_owned()) {
            self.order.borrow_mut().push(url.to_owned());
        }
    }

    fn keys(&self) -> Vec<CString> {
        self.order.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.order.borrow().len()
    }

    pub fn exclude_and_record(&self, requested: &mut HashSet<CString>) {
        self.order
            .borrow_mut()
            .retain(|key| !requested.contains(key));
        self.set.borrow_mut().retain(|key| !requested.contains(key));
        requested.extend(self.order.borrow().iter().cloned());
    }
}

struct BlockingFetch {
    main_loop: *mut c_void,
    pending: Cell<usize>,
    cache: ImageCache,
}

struct BlockingItem {
    state: *const BlockingFetch,
    abs: CString,
}

unsafe extern "C" fn on_image_fetched(_src: *mut c_void, result: *mut c_void, ud: *mut c_void) {
    let item = unsafe { Box::from_raw(ud.cast::<BlockingItem>()) };
    let st = unsafe { &*item.state };
    if let Some(resp) = unsafe { net::finish(result) } {
        st.cache.store(&item.abs, &resp);
    }
    st.pending.set(st.pending.get() - 1);
    if st.pending.get() == 0 {
        unsafe { net::quit_loop(st.main_loop) };
    }
}

pub fn fetch_images_blocking(wanted: &Wanted, base: &CStr, cache: ImageCache) {
    let main_loop = NestedLoop::new();
    let state = BlockingFetch {
        main_loop: main_loop.raw(),
        pending: Cell::new(wanted.len()),
        cache,
    };
    for key in wanted.keys() {
        let item = Box::new(BlockingItem {
            state: &state,
            abs: key,
        });
        let abs = item.abs.as_ptr();
        net::request_async(
            abs,
            base,
            Dest::Image,
            on_image_fetched,
            Box::into_raw(item).cast(),
        );
    }
    main_loop.run_blocking();
}

pub type Arrived = Rc<dyn Fn()>;

pub struct Session {
    refs: Cell<i32>,
    dead: Cell<bool>,
    outstanding: Cell<i32>,
    cache: ImageCache,
    arrived: RefCell<Option<Arrived>>,
}

struct SessionItem {
    session: *const Session,
    abs: CString,
}

unsafe fn session_unref(s: *const Session) {
    let session = unsafe { &*s };
    session.refs.set(session.refs.get() - 1);
    if session.refs.get() == 0 {
        drop(unsafe { Box::from_raw(s.cast_mut()) });
    }
}

unsafe extern "C" fn on_session_image_fetched(
    _src: *mut c_void,
    result: *mut c_void,
    ud: *mut c_void,
) {
    let item = unsafe { Box::from_raw(ud.cast::<SessionItem>()) };
    let s = unsafe { &*item.session };
    if let Some(resp) = unsafe { net::finish(result) }
        && !s.dead.get()
    {
        s.cache.store(&item.abs, &resp);
    }
    if s.outstanding.get() > 0 {
        s.outstanding.set(s.outstanding.get() - 1);
    }
    if !s.dead.get() {
        let arrived = s.arrived.borrow().clone();
        if let Some(arrived) = arrived {
            arrived();
        }
    }
    unsafe { session_unref(item.session) };
}

pub fn start_session(
    wanted: &Wanted,
    base: &CStr,
    cache: ImageCache,
    arrived: Arrived,
) -> ImageSession {
    let session = Box::into_raw(Box::new(Session {
        refs: Cell::new(1),
        dead: Cell::new(false),
        outstanding: Cell::new(wanted.len() as i32),
        cache,
        arrived: RefCell::new(Some(arrived)),
    }));
    for key in wanted.keys() {
        let item = Box::new(SessionItem { session, abs: key });
        let s = unsafe { &*session };
        s.refs.set(s.refs.get() + 1);
        let abs = item.abs.as_ptr();
        net::request_async(
            abs,
            base,
            Dest::Image,
            on_session_image_fetched,
            Box::into_raw(item).cast(),
        );
    }
    ImageSession(NonNull::new(session).expect("Box::into_raw"))
}

pub struct ImageSession(NonNull<Session>);

impl ImageSession {
    fn session(&self) -> &Session {
        // SAFETY: this handle holds one of the session's references, so the session is alive.
        unsafe { self.0.as_ref() }
    }

    pub fn outstanding(&self) -> i32 {
        self.session().outstanding.get()
    }

    pub fn close(self) {
        drop(self);
    }
}

impl Drop for ImageSession {
    fn drop(&mut self) {
        let session = self.session();
        session.dead.set(true);
        drop(session.arrived.replace(None));
        // SAFETY: this handle's reference is released exactly once, here.
        unsafe { session_unref(self.0.as_ptr()) };
    }
}
