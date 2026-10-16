//! Southstar — image fetches for a laid-out page: the wanted-URL set, the blocking fetch on a nested loop and the incremental sessions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::collections::HashSet;
use std::ffi::CString;
use std::rc::Rc;

use southstar_glib::{self as glib, GBoolean, GHashTable};

use super::net::{self, Dest, NestedLoop, Response};

#[repr(C)]
struct NsImagePrefix {
    _url: *mut c_char,
    final_url: *mut c_char,
    cors_allow_origin: *mut c_char,
}

unsafe extern "C" {
    fn ns_image_cache_peek(cache: *mut c_void, url: *const c_char) -> *mut c_void;
    fn ns_image_cache_insert_encoded(
        cache: *mut c_void,
        url: *const c_char,
        data: *const u8,
        len: usize,
    ) -> *mut c_void;
    fn g_hash_table_new_full(
        hash: unsafe extern "C" fn(*const c_void) -> c_uint,
        equal: unsafe extern "C" fn(*const c_void, *const c_void) -> GBoolean,
        key_destroy: glib::GDestroyNotify,
        value_destroy: glib::GDestroyNotify,
    ) -> *mut GHashTable;
}

pub use southstar_layout_query::collect_images;

#[derive(Clone, Copy)]
pub struct ImageCache(NonNull<c_void>);

impl ImageCache {
    pub fn of(cache: southstar_image::ImageCache) -> ImageCache {
        ImageCache(NonNull::new(cache.raw()).expect("image cache handle"))
    }

    pub fn peek(self, url: &CStr) -> bool {
        !unsafe { ns_image_cache_peek(self.0.as_ptr(), url.as_ptr()) }.is_null()
    }

    fn store(self, url: *const c_char, resp: &Response) {
        let Some(body) = resp.body().filter(|b| !b.is_empty()) else {
            return;
        };
        if resp.error().is_some() {
            return;
        }
        let img = unsafe {
            ns_image_cache_insert_encoded(self.0.as_ptr(), url, body.as_ptr(), body.len())
        };
        let Some(img) = (unsafe { img.cast::<NsImagePrefix>().as_mut() }) else {
            return;
        };
        if img.final_url.is_null() {
            let text = |value: Option<&CStr>| value.map_or(ptr::null(), CStr::as_ptr);
            img.final_url = unsafe { glib::g_strdup(text(resp.final_url())) };
            img.cors_allow_origin = unsafe { glib::g_strdup(text(resp.cors_allow_origin())) };
        }
    }
}

pub struct Wanted(NonNull<GHashTable>);

impl Wanted {
    pub fn new() -> Wanted {
        let table = unsafe {
            g_hash_table_new_full(
                glib::g_str_hash,
                glib::g_str_equal,
                Some(glib::g_free),
                None,
            )
        };
        Wanted(NonNull::new(table).expect("g_hash_table_new_full"))
    }

    pub fn contains(&self, url: &CStr) -> bool {
        unsafe { glib::g_hash_table_contains(self.0.as_ptr(), url.as_ptr().cast()) != 0 }
    }

    pub fn add(&self, url: &CStr) {
        unsafe { glib::g_hash_table_add(self.0.as_ptr(), glib::g_strdup(url.as_ptr()).cast()) };
    }

    fn keys(&self) -> Vec<*const c_char> {
        let mut len: c_uint = 0;
        let keys = unsafe { glib::g_hash_table_get_keys_as_array(self.0.as_ptr(), &mut len) };
        let out = (0..len as usize)
            .map(|i| unsafe { *keys.add(i) }.cast_const().cast())
            .collect();
        unsafe { glib::g_free(keys.cast()) };
        out
    }

    pub fn len(&self) -> usize {
        unsafe { glib::g_hash_table_size(self.0.as_ptr()) as usize }
    }

    pub fn exclude_and_record(&self, requested: &mut HashSet<CString>) {
        for key in self.keys() {
            // SAFETY: the key is a NUL-terminated string the table owns until it is removed.
            if requested.contains(unsafe { CStr::from_ptr(key) }) {
                // SAFETY: the key is still in this live table.
                unsafe { glib::g_hash_table_remove(self.0.as_ptr(), key.cast()) };
            }
        }
        for key in self.keys() {
            // SAFETY: the key is a NUL-terminated string the table owns.
            requested.insert(unsafe { CStr::from_ptr(key) }.to_owned());
        }
    }
}

impl Drop for Wanted {
    fn drop(&mut self) {
        unsafe { glib::g_hash_table_destroy(self.0.as_ptr()) };
    }
}

struct BlockingFetch {
    main_loop: *mut c_void,
    pending: Cell<usize>,
    cache: ImageCache,
}

struct BlockingItem {
    state: *const BlockingFetch,
    abs: *mut c_char,
}

unsafe extern "C" fn on_image_fetched(_src: *mut c_void, result: *mut c_void, ud: *mut c_void) {
    let item = unsafe { Box::from_raw(ud.cast::<BlockingItem>()) };
    let st = unsafe { &*item.state };
    if let Some(resp) = unsafe { net::finish(result) } {
        st.cache.store(item.abs, &resp);
    }
    st.pending.set(st.pending.get() - 1);
    if st.pending.get() == 0 {
        unsafe { net::quit_loop(st.main_loop) };
    }
    unsafe { glib::g_free(item.abs.cast()) };
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
            abs: unsafe { glib::g_strdup(key) },
        });
        let abs = item.abs;
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
    abs: *mut c_char,
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
        s.cache.store(item.abs, &resp);
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
    unsafe { glib::g_free(item.abs.cast()) };
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
        let item = Box::new(SessionItem {
            session,
            abs: unsafe { glib::g_strdup(key) },
        });
        let s = unsafe { &*session };
        s.refs.set(s.refs.get() + 1);
        let abs = item.abs;
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
