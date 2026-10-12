//! Southstar — confining the helper with the renderer's Landlock and seccomp profile on Linux once its device and network are up.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;

use southstar_helper_ffi::southstar_sandbox as _;

unsafe extern "C" {
    fn ns_security_add_writable_dir(dir: *const c_char);
    fn ns_security_sandbox_init(self_exe: *const c_char);
    fn ns_security_seccomp_init();
}

fn add_writable_dir(dir: &[u8]) {
    if let Ok(dir) = CString::new(dir) {
        unsafe { ns_security_add_writable_dir(dir.as_ptr()) };
    }
}

pub fn sandbox_self(tmp_dir: &[u8]) {
    add_writable_dir(tmp_dir);
    add_writable_dir(b"/dev/shm");
    let exe = std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|p| CString::new(p.as_os_str().as_bytes()).ok());
    unsafe {
        ns_security_sandbox_init(exe.as_ref().map_or(ptr::null(), |e| e.as_ptr()));
        ns_security_seccomp_init();
    }
}
