//! Southstar — SQLite connections and prepared statements over the system libsqlite3.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
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

fn error_message(db: Option<*mut Sqlite3>, rc: c_int) -> Vec<u8> {
    let message = match db {
        // SAFETY: db is an open connection handle (Db's, or the one a failed open allocated).
        Some(db) => unsafe { sqlite3_errmsg(db) },
        // SAFETY: sqlite3_errstr accepts any result code and returns a static string.
        None => unsafe { sqlite3_errstr(rc) },
    };
    // SAFETY: message is a NUL-terminated string libsqlite3 keeps valid until the next call on
    // the connection, and it is copied before any other call.
    unsafe { glib::bytes(message) }.unwrap_or_default().to_vec()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Row,
    Done,
    Error,
}

pub struct Db(*mut Sqlite3);

// SAFETY: a connection is not tied to the thread that opened it, and Db is not Sync, so only one
// thread at a time uses the handle.
unsafe impl Send for Db {}

impl Db {
    pub fn open(path: &CStr) -> Result<Db, Vec<u8>> {
        Db::open_with(path, SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE)
    }

    pub fn open_with(path: &CStr, flags: c_int) -> Result<Db, Vec<u8>> {
        let mut db: *mut Sqlite3 = ptr::null_mut();
        // SAFETY: path is NUL-terminated, db is a valid out-pointer and a null vfs is the default.
        let rc = unsafe { sqlite3_open_v2(path.as_ptr(), &mut db, flags, ptr::null()) };
        if rc == SQLITE_OK {
            return Ok(Db(db));
        }
        if db.is_null() {
            return Err(error_message(None, rc));
        }
        let message = error_message(Some(db), rc);
        // SAFETY: db came from the failed open, has no statements and is not used again.
        unsafe { sqlite3_close(db) };
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
            // SAFETY: self.0 is open, each of these ops takes (int, int*) varargs, and a null
            // int* skips the read-back.
            unsafe { sqlite3_db_config(self.0, op, value, ptr::null_mut::<c_int>()) };
        }
    }

    pub fn busy_timeout(&self, ms: c_int) {
        // SAFETY: self.0 is an open connection for the life of self.
        unsafe { sqlite3_busy_timeout(self.0, ms) };
    }

    pub fn exec(&self, sql: &CStr) -> Result<(), Vec<u8>> {
        let mut err: *mut c_char = ptr::null_mut();
        // SAFETY: self.0 is open, sql is NUL-terminated, there is no callback and err is a valid
        // out-pointer.
        let rc = unsafe {
            sqlite3_exec(
                self.0,
                sql.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut err,
            )
        };
        let message = if err.is_null() {
            error_message(None, rc)
        } else {
            // SAFETY: err is the NUL-terminated message sqlite3_exec allocated, copied at once.
            let message = unsafe { glib::bytes(err) }.unwrap_or_default().to_vec();
            // SAFETY: err came from sqlite3_malloc inside sqlite3_exec and is freed exactly once.
            unsafe { sqlite3_free(err.cast()) };
            message
        };
        if rc == SQLITE_OK {
            Ok(())
        } else {
            Err(message)
        }
    }

    pub fn errmsg(&self) -> Vec<u8> {
        error_message(Some(self.0), SQLITE_OK)
    }

    pub fn errcode(&self) -> c_int {
        // SAFETY: self.0 is an open connection for the life of self.
        unsafe { sqlite3_errcode(self.0) }
    }

    pub fn exec_quiet(&self, sql: &CStr) -> bool {
        self.exec(sql).is_ok()
    }

    pub fn prepare(&self, sql: &CStr) -> Option<Stmt<'_>> {
        let mut raw: *mut Sqlite3Stmt = ptr::null_mut();
        // SAFETY: self.0 is open, sql is NUL-terminated (length -1), raw is a valid out-pointer
        // and the tail is not wanted.
        let rc = unsafe { sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
        let stmt = Stmt {
            raw,
            db: PhantomData,
        };
        (rc == SQLITE_OK && !raw.is_null()).then_some(stmt)
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // SAFETY: self.0 is open and every Stmt borrows self, so all of them are finalized.
        unsafe { sqlite3_close(self.0) };
    }
}

pub struct Stmt<'db> {
    raw: *mut Sqlite3Stmt,
    db: PhantomData<&'db Db>,
}

impl Stmt<'_> {
    pub fn bind_int(&self, index: c_int, value: c_int) {
        self.bind_int64(index, i64::from(value));
    }

    pub fn bind_int64(&self, index: c_int, value: i64) {
        // SAFETY: self.raw is a live statement; a bad index is reported as SQLITE_RANGE.
        unsafe { sqlite3_bind_int64(self.raw, index, value) };
    }

    pub fn bind_text(&self, index: c_int, value: Option<&CStr>) {
        let value = value.map_or(ptr::null(), CStr::as_ptr);
        // SAFETY: value is null (binds NULL) or NUL-terminated (length -1), and SQLITE_TRANSIENT
        // makes libsqlite3 copy it before returning.
        unsafe { sqlite3_bind_text(self.raw, index, value, -1, SQLITE_TRANSIENT) };
    }

    pub fn bind_blob(&self, index: c_int, value: &[u8]) -> bool {
        let Ok(len) = c_int::try_from(value.len()) else {
            return false;
        };
        // SAFETY: value points to len readable bytes and SQLITE_TRANSIENT makes libsqlite3 copy
        // them before returning.
        let rc = unsafe {
            sqlite3_bind_blob(
                self.raw,
                index,
                value.as_ptr().cast(),
                len,
                SQLITE_TRANSIENT,
            )
        };
        rc == SQLITE_OK
    }

    fn step_code(&mut self) -> c_int {
        // SAFETY: self.raw is a live statement; &mut self means no column borrow outlives the step.
        unsafe { sqlite3_step(self.raw) }
    }

    pub fn step_result(&mut self) -> Step {
        match self.step_code() {
            SQLITE_ROW => Step::Row,
            SQLITE_DONE => Step::Done,
            _ => Step::Error,
        }
    }

    pub fn step(&mut self) -> bool {
        self.step_code() == SQLITE_ROW
    }

    pub fn column_text(&self, column: c_int) -> Option<Vec<u8>> {
        // SAFETY: self.raw is a live statement; a bad column yields null.
        let text = unsafe { sqlite3_column_text(self.raw, column) };
        // SAFETY: text is null or NUL-terminated and valid until the next step or reset; it is
        // copied now.
        unsafe { glib::bytes(text.cast()) }.map(<[u8]>::to_vec)
    }

    pub fn column_int(&self, column: c_int) -> c_int {
        self.column_int64(column) as c_int
    }

    pub fn column_int64(&self, column: c_int) -> i64 {
        // SAFETY: self.raw is a live statement; a bad column yields 0.
        unsafe { sqlite3_column_int64(self.raw, column) }
    }

    pub fn column_blob(&self, column: c_int) -> Option<&[u8]> {
        // SAFETY: self.raw is a live statement; a bad column yields null.
        let blob = unsafe { sqlite3_column_blob(self.raw, column) };
        // SAFETY: self.raw is a live statement, and the size is asked after the blob as
        // libsqlite3 requires.
        let len = unsafe { sqlite3_column_bytes(self.raw, column) };
        let len = usize::try_from(len).ok().filter(|&len| len > 0)?;
        if blob.is_null() {
            return None;
        }
        // SAFETY: blob points to len bytes that stay valid until the next step, reset or
        // finalize, each of which needs &mut self or self while this borrow of self is alive.
        Some(unsafe { glib::slice(blob.cast(), len) })
    }

    pub fn reset(&mut self) {
        // SAFETY: self.raw is a live statement; &mut self means no column borrow outlives it.
        unsafe { sqlite3_reset(self.raw) };
        // SAFETY: self.raw is a live statement.
        unsafe { sqlite3_clear_bindings(self.raw) };
    }
}

impl Drop for Stmt<'_> {
    fn drop(&mut self) {
        // SAFETY: self.raw is a live statement (or null after a failed prepare, which is a
        // no-op), finalized exactly once here.
        unsafe { sqlite3_finalize(self.raw) };
    }
}

pub struct SharedDb(Db);

// SAFETY: SharedDb always opens with SQLITE_OPEN_FULLMUTEX, so libsqlite3 serializes every call
// on the connection across threads.
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
