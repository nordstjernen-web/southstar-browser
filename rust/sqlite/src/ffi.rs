//! Southstar — SQLite connections and prepared statements over the system libsqlite3.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_glib as glib;

#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}

#[repr(C)]
struct Sqlite3Stmt {
    _private: [u8; 0],
}

type SqliteDestructor = isize;

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
pub const SQLITE_CONSTRAINT: c_int = 19;
pub const SQLITE_OPEN_READONLY: c_int = 0x01;
pub const SQLITE_OPEN_READWRITE: c_int = 0x02;
pub const SQLITE_OPEN_CREATE: c_int = 0x04;
pub const SQLITE_OPEN_FULLMUTEX: c_int = 0x0001_0000;
pub const SQLITE_OPEN_NOFOLLOW: c_int = 0x0100_0000;
const SQLITE_TRANSIENT: SqliteDestructor = -1;
const SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION: c_int = 1005;
const SQLITE_DBCONFIG_DEFENSIVE: c_int = 1010;
const SQLITE_DBCONFIG_DQS_DML: c_int = 1013;
const SQLITE_DBCONFIG_DQS_DDL: c_int = 1014;
const SQLITE_DBCONFIG_TRUSTED_SCHEMA: c_int = 1017;

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
    fn sqlite3_column_int(stmt: *mut Sqlite3Stmt, column: c_int) -> c_int;
    fn sqlite3_column_int64(stmt: *mut Sqlite3Stmt, column: c_int) -> i64;
    fn sqlite3_column_blob(stmt: *mut Sqlite3Stmt, column: c_int) -> *const c_void;
    fn sqlite3_column_bytes(stmt: *mut Sqlite3Stmt, column: c_int) -> c_int;
    fn sqlite3_bind_blob(
        stmt: *mut Sqlite3Stmt,
        index: c_int,
        value: *const c_void,
        n: c_int,
        destructor: SqliteDestructor,
    ) -> c_int;
    fn sqlite3_reset(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_clear_bindings(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_errcode(db: *mut Sqlite3) -> c_int;
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    unsafe { glib::bytes(p) }.unwrap_or_default().to_vec()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Row,
    Done,
    Error,
}

pub struct Db(*mut Sqlite3);

unsafe impl Send for Db {}

impl Db {
    pub fn open(path: &CStr) -> Result<Db, Vec<u8>> {
        Db::open_with(path, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE)
    }

    pub fn open_with(path: &CStr, flags: c_int) -> Result<Db, Vec<u8>> {
        let mut db: *mut Sqlite3 = ptr::null_mut();
        let rc = unsafe { sqlite3_open_v2(path.as_ptr(), &mut db, flags, ptr::null()) };
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

    pub fn harden(&self) {
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

    pub fn busy_timeout(&self, ms: c_int) {
        unsafe { sqlite3_busy_timeout(self.0, ms) };
    }

    pub fn exec(&self, sql: &CStr) -> Result<(), Vec<u8>> {
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

    pub fn errmsg(&self) -> Vec<u8> {
        unsafe { borrowed(sqlite3_errmsg(self.0)) }
    }

    pub fn errcode(&self) -> c_int {
        unsafe { sqlite3_errcode(self.0) }
    }

    pub fn exec_quiet(&self, sql: &CStr) -> bool {
        let rc = unsafe {
            sqlite3_exec(
                self.0,
                sql.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        rc == SQLITE_OK
    }

    pub fn prepare(&self, sql: &CStr) -> Option<Stmt> {
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

pub struct Stmt(*mut Sqlite3Stmt);

impl Stmt {
    pub fn bind_int(&self, index: c_int, value: c_int) {
        unsafe { sqlite3_bind_int(self.0, index, value) };
    }

    pub fn bind_int64(&self, index: c_int, value: i64) {
        unsafe { sqlite3_bind_int64(self.0, index, value) };
    }

    pub fn bind_text(&self, index: c_int, value: Option<&CStr>) {
        let value = value.map_or(ptr::null(), CStr::as_ptr);
        unsafe { sqlite3_bind_text(self.0, index, value, -1, SQLITE_TRANSIENT) };
    }

    pub fn step_result(&self) -> Step {
        match unsafe { sqlite3_step(self.0) } {
            SQLITE_ROW => Step::Row,
            SQLITE_DONE => Step::Done,
            _ => Step::Error,
        }
    }

    pub fn step(&self) -> bool {
        unsafe { sqlite3_step(self.0) == SQLITE_ROW }
    }

    pub fn column_text(&self, column: c_int) -> Option<Vec<u8>> {
        unsafe { glib::bytes(sqlite3_column_text(self.0, column).cast()) }.map(<[u8]>::to_vec)
    }

    pub fn column_int(&self, column: c_int) -> c_int {
        unsafe { sqlite3_column_int(self.0, column) }
    }

    pub fn column_int64(&self, column: c_int) -> i64 {
        unsafe { sqlite3_column_int64(self.0, column) }
    }

    pub fn column_blob(&self, column: c_int) -> Option<&[u8]> {
        unsafe {
            let blob = sqlite3_column_blob(self.0, column);
            let len = sqlite3_column_bytes(self.0, column);
            (!blob.is_null() && len > 0)
                .then(|| core::slice::from_raw_parts(blob.cast::<u8>(), len as usize))
        }
    }

    pub fn bind_blob(&self, index: c_int, value: &[u8]) -> bool {
        let Ok(len) = c_int::try_from(value.len()) else {
            return false;
        };
        unsafe {
            sqlite3_bind_blob(self.0, index, value.as_ptr().cast(), len, SQLITE_TRANSIENT)
                == SQLITE_OK
        }
    }

    pub fn reset(&self) {
        unsafe {
            sqlite3_reset(self.0);
            sqlite3_clear_bindings(self.0);
        }
    }
}

pub struct SharedDb(Db);

unsafe impl Sync for SharedDb {}

impl SharedDb {
    pub fn open(path: &CStr, flags: c_int) -> Result<SharedDb, Vec<u8>> {
        Db::open_with(path, flags | SQLITE_OPEN_FULLMUTEX).map(SharedDb)
    }
}

impl core::ops::Deref for SharedDb {
    type Target = Db;

    fn deref(&self) -> &Db {
        &self.0
    }
}

impl Drop for Stmt {
    fn drop(&mut self) {
        unsafe { sqlite3_finalize(self.0) };
    }
}
