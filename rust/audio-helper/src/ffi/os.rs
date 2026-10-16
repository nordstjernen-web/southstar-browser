//! Southstar — confining the helper with the renderer's Landlock and seccomp profile on Linux once its device and network are up.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;

use southstar_helper_ffi::southstar_sandbox as sandbox;

pub fn sandbox_self(tmp_dir: &[u8]) {
    sandbox::add_writable_dir(tmp_dir);
    sandbox::add_writable_dir(b"/dev/shm");
    let exe = std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|p| CString::new(p.as_os_str().as_bytes()).ok());
    sandbox::sandbox_init(exe.as_deref());
    sandbox::seccomp_init();
}
