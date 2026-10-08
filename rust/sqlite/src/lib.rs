//! Southstar — the SQLite connection and statement wrappers the ported stores share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{Db, Step, Stmt};
