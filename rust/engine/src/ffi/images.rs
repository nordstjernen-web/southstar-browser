//! Southstar — image fetches for a laid-out page: the wanted-URL set, the blocking fetch on a nested loop and the incremental sessions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr::{self, NonNull};

use southstar_glib::{self as glib, GBoolean, GHashTable, GPtrArray};
use southstar_layout::{BoxRef, NsBox};

use super::net::{self, Dest, NestedLoop, Response};

#[repr(C)]
struct NsImagePrefix {
    _url: *mut c_char,
    final_url: *mut c_char,
    cors_allow_origin: *mut c_char,
}

unsafe extern "C" {
    fn ns_layout_collect_images(root: *const NsBox, out: *mut GPtrArray);
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

pub fn collect_images(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let array = unsafe { glib::g_ptr_array_new() };
    unsafe { ns_layout_collect_images(root.as_ptr(), array) };
    let a = unsafe { &*array };
    let boxes = (0..a.len as usize)
        .filter_map(|i| unsafe { BoxRef::from_ptr((*a.pdata.add(i)).cast()) })
        .collect();
    unsafe { glib::g_ptr_array_free(array, glib::TRUE) };
    boxes
}

#[derive(Clone, Copy)]
pub struct ImageCache(NonNull<c_void>);

impl ImageCache {
    pub unsafe fn from_ptr(cache: *mut c_void) -> Option<ImageCache> {
        NonNull::new(cache).map(ImageCache)
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
            img.final_url = unsafe { glib::g_strdup(resp.final_url()) };
            img.cors_allow_origin = unsafe { glib::g_strdup(resp.cors_allow_origin()) };
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

    pub fn exclude_and_record(&self, requested: *mut GHashTable) {
        if requested.is_null() {
            return;
        }
        for key in self.keys() {
            if unsafe { glib::g_hash_table_contains(requested, key.cast()) } != 0 {
                unsafe { glib::g_hash_table_remove(self.0.as_ptr(), key.cast()) };
            }
        }
        for key in self.keys() {
            unsafe { glib::g_hash_table_add(requested, glib::g_strdup(key).cast()) };
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

pub type ArrivedCb = Option<unsafe extern "C" fn(user_data: *mut c_void)>;

pub struct Session {
    refs: Cell<i32>,
    dead: Cell<bool>,
    outstanding: Cell<i32>,
    cache: ImageCache,
    arrived_cb: Cell<ArrivedCb>,
    user_data: *mut c_void,
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
    if let Some(resp) = unsafe { net::finish(result) } {
        if !s.dead.get() {
            s.cache.store(item.abs, &resp);
        }
    }
    if s.outstanding.get() > 0 {
        s.outstanding.set(s.outstanding.get() - 1);
    }
    if !s.dead.get() {
        if let Some(cb) = s.arrived_cb.get() {
            unsafe { cb(s.user_data) };
        }
    }
    unsafe { session_unref(item.session) };
    unsafe { glib::g_free(item.abs.cast()) };
}

pub fn start_session(
    wanted: &Wanted,
    base: &CStr,
    cache: ImageCache,
    arrived_cb: ArrivedCb,
    user_data: *mut c_void,
) -> *mut Session {
    let session = Box::into_raw(Box::new(Session {
        refs: Cell::new(1),
        dead: Cell::new(false),
        outstanding: Cell::new(wanted.len() as i32),
        cache,
        arrived_cb: Cell::new(arrived_cb),
        user_data,
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
    session
}

pub unsafe fn session_outstanding(s: *const Session) -> i32 {
    unsafe { s.as_ref() }.map_or(0, |s| s.outstanding.get())
}

pub unsafe fn session_close(s: *mut Session) {
    let Some(session) = (unsafe { s.as_ref() }) else {
        return;
    };
    session.dead.set(true);
    session.arrived_cb.set(None);
    unsafe { session_unref(s) };
}

pub fn null_session() -> *mut Session {
    ptr::null_mut()
}
