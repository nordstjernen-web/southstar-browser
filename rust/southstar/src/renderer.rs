//! Southstar — the out-of-process renderer executable a cargo install builds.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use southstar_ffi as _;

fn main() -> std::process::ExitCode {
    let args: Vec<Vec<u8>> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned().into_bytes())
        .collect();
    let args: Vec<&[u8]> = args.iter().map(Vec::as_slice).collect();
    let status = southstar_renderer_host::run(&args);
    std::process::ExitCode::from(u8::try_from(status & 0xff).unwrap_or(1))
}
