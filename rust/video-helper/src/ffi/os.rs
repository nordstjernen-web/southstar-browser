//! Southstar — the monotonic clock the shell also reads, file sizes and the Linux sandbox.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(target_os = "linux")]
use std::ffi::CString;

#[cfg(windows)]
mod sys {
    use core::ffi::c_int;

    unsafe extern "system" {
        pub fn QueryPerformanceCounter(count: *mut i64) -> c_int;
        pub fn QueryPerformanceFrequency(frequency: *mut i64) -> c_int;
    }
}

#[cfg(unix)]
mod sys {
    use core::ffi::c_int;

    #[cfg(target_os = "linux")]
    pub const CLOCK_MONOTONIC: c_int = 1;
    #[cfg(target_vendor = "apple")]
    pub const CLOCK_MONOTONIC: c_int = 6;
    #[cfg(target_os = "freebsd")]
    pub const CLOCK_MONOTONIC: c_int = 4;
    #[cfg(not(any(target_os = "linux", target_vendor = "apple", target_os = "freebsd")))]
    pub const CLOCK_MONOTONIC: c_int = 3;

    #[repr(C)]
    pub struct Timespec {
        pub tv_sec: i64,
        pub tv_nsec: i64,
    }

    unsafe extern "C" {
        pub fn clock_gettime(clock: c_int, ts: *mut Timespec) -> c_int;
    }
}

#[cfg(windows)]
pub fn now_us() -> i64 {
    let mut count = 0i64;
    let mut frequency = 0i64;
    // SAFETY: count is a writable i64 the call only fills.
    unsafe { sys::QueryPerformanceCounter(&mut count) };
    // SAFETY: frequency is a writable i64 the call only fills.
    unsafe { sys::QueryPerformanceFrequency(&mut frequency) };
    if frequency <= 0 {
        return 0;
    }
    count / frequency * 1_000_000 + count % frequency * 1_000_000 / frequency
}

#[cfg(unix)]
pub fn now_us() -> i64 {
    let mut ts = sys::Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts is a writable timespec the call only fills.
    unsafe { sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut ts) };
    ts.tv_sec * 1_000_000 + ts.tv_nsec / 1000
}

pub fn file_size(path: &[u8]) -> i64 {
    #[cfg(unix)]
    let path = <std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(path);
    #[cfg(not(unix))]
    let path = String::from_utf8_lossy(path).into_owned();
    std::fs::metadata(path).map_or(-1, |meta| meta.len() as i64)
}

#[cfg(target_os = "linux")]
pub fn sandbox_self() {
    use southstar_helper_ffi::southstar_sandbox as sandbox;
    let tmp = std::env::var_os("TMPDIR")
        .filter(|dir| !dir.is_empty())
        .unwrap_or_else(|| "/tmp".into());
    let tmp = CString::new(std::os::unix::ffi::OsStrExt::as_bytes(tmp.as_os_str())).ok();
    let self_exe = std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|path| CString::new(path.into_os_string().into_encoded_bytes()).ok());
    sandbox::add_writable_dir(b"/dev/shm");
    sandbox::add_writable_dir(tmp.as_deref().unwrap_or(c"/tmp").to_bytes());
    sandbox::sandbox_init(self_exe.as_deref());
    sandbox::seccomp_init();
}

#[cfg(not(target_os = "linux"))]
pub fn sandbox_self() {}
