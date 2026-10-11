//! Southstar — the Windows bundle launcher, linked by meson into southstar-launcher.exe; empty elsewhere.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(windows)]
mod ffi;
#[cfg(windows)]
mod launch;
