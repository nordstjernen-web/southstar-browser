//! Southstar — the C ABI of browsing history, as declared in src/history.h, and the SQLite and GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}

#[repr(C)]
struct Sqlite3Stmt {
    _private: [u8; 0],
}

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct GUri {
    _private: [u8; 0],
}

type SqliteDestructor = isize;

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READWRITE: c_int = 0x02;
const SQLITE_OPEN_CREATE: c_int = 0x04;
const SQLITE_TRANSIENT: SqliteDestructor = -1;
const SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION: c_int = 1005;
const SQLITE_DBCONFIG_DEFENSIVE: c_int = 1010;
const SQLITE_DBCONFIG_DQS_DML: c_int = 1013;
const SQLITE_DBCONFIG_DQS_DDL: c_int = 1014;
const SQLITE_DBCONFIG_TRUSTED_SCHEMA: c_int = 1017;
const LOG_LEVEL_WARNING: c_int = 1 << 4;
const URI_FLAGS_NONE: c_int = 0;

unsafe extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        db: *mut *mut Sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    fn sqlite3_db_config(db: *mut Sqlite3, op: c_int, ...) -> c_int;
    fn sqlite3_busy_timeout(db: *mut Sqlite3, ms: c_int) -> c_int;
    fn sqlite3_exec(
        db: *mut Sqlite3,
        sql: *const c_char,
        callback: *mut c_void,
        arg: *mut c_void,
        errmsg: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_errstr(rc: c_int) -> *const c_char;
    fn sqlite3_free(p: *mut c_void);
    fn sqlite3_prepare_v2(
        db: *mut Sqlite3,
        sql: *const c_char,
        n_byte: c_int,
        stmt: *mut *mut Sqlite3Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_bind_int(stmt: *mut Sqlite3Stmt, index: c_int, value: c_int) -> c_int;
    fn sqlite3_bind_int64(stmt: *mut Sqlite3Stmt, index: c_int, value: i64) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut Sqlite3Stmt,
        index: c_int,
        value: *const c_char,
        n: c_int,
        destructor: SqliteDestructor,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_finalize(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_column_text(stmt: *mut Sqlite3Stmt, column: c_int) -> *const u8;
    fn sqlite3_column_int64(stmt: *mut Sqlite3Stmt, column: c_int) -> i64;

    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
    fn g_markup_escape_text(text: *const c_char, length: isize) -> *mut c_char;
    fn g_utf8_make_valid(text: *const c_char, length: isize) -> *mut c_char;
    fn g_utf8_get_char_validated(p: *const c_char, max_len: isize) -> u32;
    fn g_unichar_isalnum(c: u32) -> GBoolean;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
    fn g_uri_parse(uri_string: *const c_char, flags: c_int, error: *mut *mut c_void) -> *mut GUri;
    fn g_uri_get_host(uri: *mut GUri) -> *const c_char;
    fn g_uri_unref(uri: *mut GUri);
    fn g_date_time_new_now_local() -> *mut GDateTime;
    fn g_date_time_new_from_unix_local(t: i64) -> *mut GDateTime;
    fn g_date_time_get_day_of_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_get_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_format(datetime: *mut GDateTime, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut GDateTime);
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Option<Vec<u8>> {
    let bytes = unsafe { glib::bytes(p) }?.to_vec();
    unsafe { glib::g_free(p.cast()) };
    Some(bytes)
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    unsafe { glib::bytes(p) }.unwrap_or_default().to_vec()
}

pub(crate) fn warn(message: &[u8]) {
    let message = c_string(message);
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
}

pub(crate) fn build_filename(dir: &CStr, name: &CStr) -> CString {
    unsafe {
        let path = g_build_filename(dir.as_ptr(), name.as_ptr(), ptr::null::<c_char>());
        c_string(&take(path).unwrap_or_default())
    }
}

pub(crate) fn user_data_dir() -> CString {
    c_string(&unsafe { borrowed(glib::g_get_user_data_dir()) })
}

pub(crate) fn mkdir_with_parents(path: &CStr, mode: c_int) {
    unsafe { g_mkdir_with_parents(path.as_ptr(), mode) };
}

pub(crate) fn chmod(path: &CStr, mode: c_int) {
    unsafe { g_chmod(path.as_ptr(), mode) };
}

pub(crate) fn markup_escape(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    unsafe { take(g_markup_escape_text(text.as_ptr(), -1)) }.unwrap_or_default()
}

pub(crate) fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    unsafe { take(g_utf8_make_valid(text.as_ptr(), -1)) }.unwrap_or_default()
}

pub(crate) fn first_char(text: &[u8]) -> Option<u32> {
    let text = c_string(text);
    let c = unsafe { g_utf8_get_char_validated(text.as_ptr(), -1) };
    (c != u32::MAX && c != u32::MAX - 1).then_some(c)
}

pub(crate) fn unichar_isalnum(c: u32) -> bool {
    unsafe { g_unichar_isalnum(c) != glib::FALSE }
}

pub(crate) fn unichar_to_utf8(c: u32) -> Vec<u8> {
    let mut out = [0u8; 8];
    let n = unsafe { g_unichar_to_utf8(c, out.as_mut_ptr().cast()) };
    out[..n.clamp(0, 6) as usize].to_vec()
}

pub(crate) fn uri_host(url: &[u8]) -> Option<Vec<u8>> {
    let url = c_string(url);
    unsafe {
        let uri = g_uri_parse(url.as_ptr(), URI_FLAGS_NONE, ptr::null_mut());
        if uri.is_null() {
            return None;
        }
        let host = glib::bytes(g_uri_get_host(uri)).map(<[u8]>::to_vec);
        g_uri_unref(uri);
        host
    }
}

pub(crate) struct LocalTime(*mut GDateTime);

impl LocalTime {
    pub(crate) fn now() -> LocalTime {
        LocalTime(unsafe { g_date_time_new_now_local() })
    }

    pub(crate) fn from_unix(t: i64) -> Option<LocalTime> {
        let dt = unsafe { g_date_time_new_from_unix_local(t) };
        (!dt.is_null()).then_some(LocalTime(dt))
    }

    pub(crate) fn day_of_year(&self) -> i32 {
        unsafe { g_date_time_get_day_of_year(self.0) }
    }

    pub(crate) fn year(&self) -> i32 {
        unsafe { g_date_time_get_year(self.0) }
    }

    pub(crate) fn format(&self, format: &CStr) -> Option<Vec<u8>> {
        unsafe { take(g_date_time_format(self.0, format.as_ptr())) }
    }
}

impl Drop for LocalTime {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { g_date_time_unref(self.0) };
        }
    }
}

pub(crate) struct Db(*mut Sqlite3);

unsafe impl Send for Db {}

impl Db {
    pub(crate) fn open(path: &CStr) -> Result<Db, Vec<u8>> {
        let mut db: *mut Sqlite3 = ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                path.as_ptr(),
                &mut db,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
                ptr::null(),
            )
        };
        if rc == SQLITE_OK {
            return Ok(Db(db));
        }
        let message = unsafe {
            if db.is_null() {
                borrowed(sqlite3_errstr(rc))
            } else {
                let message = borrowed(sqlite3_errmsg(db));
                sqlite3_close(db);
                message
            }
        };
        Err(message)
    }

    pub(crate) fn harden(&self) {
        for op in [
            SQLITE_DBCONFIG_DEFENSIVE,
            SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION,
            SQLITE_DBCONFIG_TRUSTED_SCHEMA,
            SQLITE_DBCONFIG_DQS_DDL,
            SQLITE_DBCONFIG_DQS_DML,
        ] {
            let value: c_int = c_int::from(op == SQLITE_DBCONFIG_DEFENSIVE);
            unsafe { sqlite3_db_config(self.0, op, value, ptr::null_mut::<c_int>()) };
        }
    }

    pub(crate) fn busy_timeout(&self, ms: c_int) {
        unsafe { sqlite3_busy_timeout(self.0, ms) };
    }

    pub(crate) fn exec(&self, sql: &CStr) -> Result<(), Vec<u8>> {
        let mut err: *mut c_char = ptr::null_mut();
        let rc = unsafe {
            sqlite3_exec(
                self.0,
                sql.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut err,
            )
        };
        let message = unsafe {
            let message = if err.is_null() {
                borrowed(sqlite3_errstr(rc))
            } else {
                borrowed(err)
            };
            if !err.is_null() {
                sqlite3_free(err.cast());
            }
            message
        };
        if rc == SQLITE_OK {
            Ok(())
        } else {
            Err(message)
        }
    }

    pub(crate) fn prepare(&self, sql: &CStr) -> Option<Stmt> {
        let mut st: *mut Sqlite3Stmt = ptr::null_mut();
        let rc = unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut st, ptr::null_mut()) };
        if rc == SQLITE_OK {
            Some(Stmt(st))
        } else {
            unsafe { sqlite3_finalize(st) };
            None
        }
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        unsafe { sqlite3_close(self.0) };
    }
}

pub(crate) struct Stmt(*mut Sqlite3Stmt);

impl Stmt {
    pub(crate) fn bind_int(&self, index: c_int, value: c_int) {
        unsafe { sqlite3_bind_int(self.0, index, value) };
    }

    pub(crate) fn bind_int64(&self, index: c_int, value: i64) {
        unsafe { sqlite3_bind_int64(self.0, index, value) };
    }

    pub(crate) fn bind_text(&self, index: c_int, value: Option<&CStr>) {
        let value = value.map_or(ptr::null(), CStr::as_ptr);
        unsafe { sqlite3_bind_text(self.0, index, value, -1, SQLITE_TRANSIENT) };
    }

    pub(crate) fn step(&self) -> bool {
        unsafe { sqlite3_step(self.0) == SQLITE_ROW }
    }

    pub(crate) fn column_text(&self, column: c_int) -> Option<Vec<u8>> {
        unsafe { glib::bytes(sqlite3_column_text(self.0, column).cast()) }.map(<[u8]>::to_vec)
    }

    pub(crate) fn column_int64(&self, column: c_int) -> i64 {
        unsafe { sqlite3_column_int64(self.0, column) }
    }
}

impl Drop for Stmt {
    fn drop(&mut self) {
        unsafe { sqlite3_finalize(self.0) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_init() {
    crate::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_shutdown() {
    crate::shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_history_record(url: *const c_char, title: *const c_char) {
    let url = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) });
    let title = (!title.is_null()).then(|| unsafe { CStr::from_ptr(title) });
    crate::record(url, title);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_clear() {
    crate::clear();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_html_page() -> *mut c_char {
    glib::strdup(&crate::html_page())
}
