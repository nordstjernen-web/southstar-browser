//! Southstar — the southstar-video helper: reads the shell's commands from stdin and decodes each video into a shared-memory frame ring.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

macro_rules! emit {
    ($($arg:tt)*) => {
        $crate::protocol::emit(format_args!($($arg)*))
    };
}

mod ffi;
mod player;
mod protocol;

pub use protocol::run;
