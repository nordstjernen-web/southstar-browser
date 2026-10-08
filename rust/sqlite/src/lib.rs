//! Southstar — the SQLite connection and statement wrappers the ported stores share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{
    Db, SQLITE_CONSTRAINT, SQLITE_OPEN_CREATE, SQLITE_OPEN_FULLMUTEX, SQLITE_OPEN_NOFOLLOW,
    SQLITE_OPEN_READONLY, SQLITE_OPEN_READWRITE, SharedDb, Step, Stmt,
};
