//! Southstar — the browser executable a cargo install builds: installs the bundled data beside it, then runs the GTK shell.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use std::ffi::{CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const BUNDLE: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/bundle.rs"));
const STAMP: &str = ".southstar-version";

fn share_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let prefix = exe.parent()?.parent()?;
    Some(prefix.join("share").join("southstar"))
}

fn install_bundle(share: &Path) {
    let stamp = share.join(STAMP);
    let version = env!("CARGO_PKG_VERSION");
    if fs::read_to_string(&stamp).is_ok_and(|installed| installed == version) {
        return;
    }
    for (relative, contents) in BUNDLE {
        let path = share.join(relative);
        let written = path
            .parent()
            .is_some_and(|dir| fs::create_dir_all(dir).is_ok())
            && fs::write(&path, contents).is_ok();
        if !written {
            return;
        }
    }
    let _ = fs::write(stamp, version);
}

fn arguments() -> Vec<&'static CStr> {
    std::env::args_os()
        .filter_map(|arg| CString::new(arg.to_string_lossy().into_owned()).ok())
        .map(|arg| &*Box::leak(arg.into_boxed_c_str()))
        .collect()
}

fn main() -> ExitCode {
    if let Some(share) = share_dir() {
        install_bundle(&share);
    }
    let status = southstar_shell::run(&arguments());
    ExitCode::from(u8::try_from(status & 0xff).unwrap_or(1))
}
