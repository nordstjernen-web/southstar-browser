//! Southstar — the southstar-audio helper process, whose executable's `main` calls `run`.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod decode;
mod ffi;
mod helper;
mod mixer;
mod mp3;

pub use helper::run;
