//! Southstar — the Windows bundle launcher executable, which starts the browser beside it with the console and stdio handed through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

#[cfg(windows)]
fn main() {
    std::process::exit(southstar_launcher::start());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("southstar-launcher is the Windows bundle launcher");
}
