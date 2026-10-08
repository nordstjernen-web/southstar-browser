//! Southstar — the GLib pieces the page lifecycle uses: GString buffers, the main context, timeouts, logging and allocation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_uint, c_ulong, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean, GHashTable, GPtrArray, GStr};

#[repr(C)]
pub struct GString {
    str_: *mut c_char,
    len: usize,
    _allocated_len: usize,
}

pub type SourceFunc = unsafe extern "C" fn(ud: *mut c_void) -> GBoolean;

const LOG_LEVEL_MESSAGE: c_int = 1 << 5;
const DLOG_JS: c_uint = 5;

unsafe extern "C" {
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_append_len(s: *mut GString, val: *const c_char, len: isize) -> *mut GString;
    fn g_string_erase(s: *mut GString, pos: isize, len: isize) -> *mut GString;
    fn g_string_truncate(s: *mut GString, len: usize) -> *mut GString;
    fn g_string_free(s: *mut GString, free_segment: GBoolean) -> *mut c_char;
    fn g_string_append_printf(s: *mut GString, format: *const c_char, ...);
    fn g_get_monotonic_time() -> i64;
    fn g_main_context_pending(context: *mut c_void) -> GBoolean;
    fn g_main_context_iteration(context: *mut c_void, may_block: GBoolean) -> GBoolean;
    fn g_usleep(microseconds: c_ulong);
    fn g_source_remove(tag: c_uint) -> GBoolean;
    pub fn g_timeout_add(interval: c_uint, function: SourceFunc, data: *mut c_void) -> c_uint;
    pub fn g_main_loop_new(context: *mut c_void, is_running: GBoolean) -> *mut c_void;
    pub fn g_main_loop_run(main_loop: *mut c_void);
    pub fn g_main_loop_quit(main_loop: *mut c_void);
    pub fn g_main_loop_unref(main_loop: *mut c_void);
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_printerr(format: *const c_char, ...);
    fn g_uri_unescape_string(escaped: *const c_char, illegal: *const c_char) -> *mut c_char;
    pub fn g_ptr_array_remove_index_fast(array: *mut GPtrArray, index: c_uint) -> *mut c_void;
    fn g_bytes_unref(bytes: *mut c_void);
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
    fn malloc(size: usize) -> *mut c_void;
    fn g_dir_open(path: *const c_char, flags: c_uint, error: *mut *mut glib::GError)
    -> *mut c_void;
    fn g_dir_read_name(dir: *mut c_void) -> *const c_char;
    fn g_dir_close(dir: *mut c_void);
    fn g_strchomp(s: *mut c_char) -> *mut c_char;
}

fn c_input(bytes: &[u8]) -> Vec<u8> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut v = bytes[..end].to_vec();
    v.push(0);
    v
}

#[repr(transparent)]
pub struct StrBuf(Cell<*mut GString>);

impl StrBuf {
    fn ensure(&self) -> *mut GString {
        if self.0.get().is_null() {
            self.0.set(unsafe { g_string_new(ptr::null()) });
        }
        self.0.get()
    }

    pub fn len(&self) -> usize {
        unsafe { self.0.get().as_ref() }.map_or(0, |s| s.len)
    }

    pub fn create(&self) {
        self.ensure();
    }

    pub fn append(&self, bytes: &[u8]) {
        let s = self.ensure();
        unsafe { g_string_append_len(s, bytes.as_ptr().cast(), bytes.len() as isize) };
    }

    pub fn erase_front(&self, n: usize) {
        unsafe { g_string_erase(self.ensure(), 0, n as isize) };
    }

    pub fn take_text(&self) -> *mut c_char {
        let s = self.0.get();
        if s.is_null() || self.len() == 0 {
            return ptr::null_mut();
        }
        let out = unsafe { glib::g_strdup((*s).str_) };
        unsafe { g_string_truncate(s, 0) };
        out
    }

    pub fn free(&self) {
        let s = self.0.replace(ptr::null_mut());
        if !s.is_null() {
            unsafe { g_string_free(s, glib::TRUE) };
        }
    }
}

pub struct StrBufOwned(*mut GString);

impl StrBufOwned {
    pub fn new() -> StrBufOwned {
        StrBufOwned(unsafe { g_string_new(ptr::null()) })
    }

    pub fn raw(&self) -> *mut GString {
        self.0
    }

    pub fn bytes(&self) -> &[u8] {
        let s = unsafe { &*self.0 };
        unsafe { glib::slice(s.str_.cast(), s.len) }
    }

    pub fn len(&self) -> usize {
        unsafe { (*self.0).len }
    }

    pub fn append(&self, bytes: &[u8]) {
        unsafe { g_string_append_len(self.0, bytes.as_ptr().cast(), bytes.len() as isize) };
    }

    pub fn append_double(&self, format: &CStr, value: f64) {
        unsafe { g_string_append_printf(self.0, format.as_ptr(), value) };
    }

    pub fn into_text(self) -> *mut c_char {
        let s = self.0;
        core::mem::forget(self);
        unsafe { g_string_free(s, glib::FALSE) }
    }

    pub fn into_raw(self) -> (*mut c_char, usize) {
        let len = self.len();
        (self.into_text(), len)
    }
}

impl Drop for StrBufOwned {
    fn drop(&mut self) {
        unsafe { g_string_free(self.0, glib::TRUE) };
    }
}

pub unsafe fn append_to(s: *mut GString, bytes: &[u8]) {
    unsafe { g_string_append_len(s, bytes.as_ptr().cast(), bytes.len() as isize) };
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn main_context_pending() -> bool {
    unsafe { g_main_context_pending(ptr::null_mut()) != 0 }
}

pub fn main_context_iteration() {
    unsafe { g_main_context_iteration(ptr::null_mut(), glib::FALSE) };
}

pub fn usleep(us: u64) {
    unsafe { g_usleep(us as c_ulong) };
}

pub fn source_remove(tag: c_uint) {
    unsafe { g_source_remove(tag) };
}

pub fn log_message(text: &CStr) {
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_MESSAGE,
            c"%s".as_ptr(),
            text.as_ptr(),
        )
    };
}

pub fn printerr(text: &[u8]) {
    let text = c_input(text);
    unsafe { g_printerr(c"%s".as_ptr(), text.as_ptr()) };
}

pub fn printerr_double(format: &CStr, value: f64) {
    unsafe { g_printerr(format.as_ptr(), value) };
}

pub fn env_value(name: &CStr) -> Option<&'static CStr> {
    let v = unsafe { glib::g_getenv(name.as_ptr()) };
    (!v.is_null()).then(|| unsafe { CStr::from_ptr(v) })
}

pub fn printerr_paint_profile(ms: f64, width: c_int, height: c_int) {
    unsafe {
        g_printerr(
            c"[profile] paint %6.1fms %dx%d\n".as_ptr(),
            ms,
            width,
            height,
        )
    };
}

pub fn env_set(name: &CStr) -> bool {
    !unsafe { glib::g_getenv(name.as_ptr()) }.is_null()
}

pub fn uri_unescape(escaped: &CStr) -> Option<GStr> {
    unsafe { GStr::take(g_uri_unescape_string(escaped.as_ptr(), ptr::null())) }
}

pub fn dup_text(text: &CStr) -> Option<GStr> {
    unsafe { GStr::take(glib::g_strdup(text.as_ptr())) }
}

pub fn gstr_from(bytes: &[u8]) -> Option<GStr> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    unsafe { GStr::take(glib::strdup(&bytes[..end])) }
}

pub fn debug_log_console(line: &CStr) {
    unsafe { ns_debug_log_emit_take(DLOG_JS, c"console".as_ptr(), glib::g_strdup(line.as_ptr())) };
}

pub fn malloc_dup(bytes: &[u8]) -> *mut c_char {
    let out = unsafe { malloc(bytes.len() + 1) }.cast::<u8>();
    if out.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    out.cast()
}

pub fn new_string_set() -> *mut GHashTable {
    unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            None,
        )
    }
}

pub fn new_css_cache() -> *mut GHashTable {
    unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            Some(g_bytes_unref),
        )
    }
}

pub enum Threads {
    Unavailable(Vec<u8>),
    Listed(Vec<(Vec<u8>, Vec<u8>)>),
}

pub fn list_threads() -> Threads {
    let mut error: *mut glib::GError = ptr::null_mut();
    let dir = unsafe { g_dir_open(c"/proc/self/task".as_ptr(), 0, &mut error) };
    if dir.is_null() {
        let message = unsafe { error.as_ref() }
            .and_then(|e| unsafe { glib::bytes(e.message) })
            .map_or_else(|| b"?".to_vec(), <[u8]>::to_vec);
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return Threads::Unavailable(message);
    }
    let mut threads = Vec::new();
    loop {
        let tid = unsafe { g_dir_read_name(dir) };
        if tid.is_null() {
            break;
        }
        let tid = unsafe { CStr::from_ptr(tid) }.to_bytes().to_vec();
        let mut path = b"/proc/self/task/".to_vec();
        path.extend_from_slice(&tid);
        path.extend_from_slice(b"/comm\0");
        let mut comm: *mut c_char = ptr::null_mut();
        let ok = unsafe {
            glib::g_file_get_contents(
                path.as_ptr().cast(),
                &mut comm,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if ok != 0 && !comm.is_null() {
            let name = unsafe { CStr::from_ptr(g_strchomp(comm)) }
                .to_bytes()
                .to_vec();
            threads.push((tid, name));
        }
        unsafe { glib::g_free(comm.cast()) };
    }
    unsafe { g_dir_close(dir) };
    Threads::Listed(threads)
}
