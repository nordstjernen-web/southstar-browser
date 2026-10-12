//! Southstar — the monotonic clock the shell also reads, named shared memory, file sizes and the Linux sandbox.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

#[cfg(windows)]
mod sys {
    use super::*;

    pub const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;
    pub const PAGE_READWRITE: u32 = 0x04;
    pub const FILE_MAP_ALL_ACCESS: u32 = 0x000f_001f;

    unsafe extern "system" {
        pub fn QueryPerformanceCounter(count: *mut i64) -> c_int;
        pub fn QueryPerformanceFrequency(frequency: *mut i64) -> c_int;
        pub fn CreateFileMappingA(
            file: *mut c_void,
            attributes: *mut c_void,
            protect: u32,
            size_high: u32,
            size_low: u32,
            name: *const c_char,
        ) -> *mut c_void;
        pub fn MapViewOfFile(
            mapping: *mut c_void,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        pub fn UnmapViewOfFile(base: *const c_void) -> c_int;
        pub fn CloseHandle(handle: *mut c_void) -> c_int;
    }
}

#[cfg(unix)]
mod sys {
    use super::*;

    pub const O_RDWR: c_int = 2;
    #[cfg(target_os = "linux")]
    pub const CREATE_EXCLUSIVE: c_int = 0x40 | 0x80;
    #[cfg(not(target_os = "linux"))]
    pub const CREATE_EXCLUSIVE: c_int = 0x200 | 0x800;
    pub const PROT_READ_WRITE: c_int = 3;
    pub const MAP_SHARED: c_int = 1;
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
        pub fn shm_open(name: *const c_char, oflag: c_int, ...) -> c_int;
        pub fn shm_unlink(name: *const c_char) -> c_int;
        #[cfg(not(all(target_os = "linux", target_env = "gnu", target_pointer_width = "32")))]
        pub fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        #[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "32"))]
        #[link_name = "mmap64"]
        pub fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        pub fn munmap(addr: *mut c_void, len: usize) -> c_int;
    }
}

#[cfg(windows)]
pub fn now_us() -> i64 {
    let mut count = 0i64;
    let mut frequency = 0i64;
    unsafe {
        sys::QueryPerformanceCounter(&mut count);
        sys::QueryPerformanceFrequency(&mut frequency);
    }
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

pub struct SharedMemory {
    base: *mut u8,
    name: CString,
    #[cfg(windows)]
    handle: *mut c_void,
    #[cfg(unix)]
    bytes: usize,
    #[cfg(unix)]
    _file: std::fs::File,
}

unsafe impl Send for SharedMemory {}

impl SharedMemory {
    #[cfg(windows)]
    pub fn create(name: &str, bytes: usize) -> Option<SharedMemory> {
        let name = CString::new(name).ok()?;
        let handle = unsafe {
            sys::CreateFileMappingA(
                sys::INVALID_HANDLE_VALUE,
                ptr::null_mut(),
                sys::PAGE_READWRITE,
                (bytes as u64 >> 32) as u32,
                (bytes as u64 & 0xffff_ffff) as u32,
                name.as_ptr(),
            )
        };
        if handle.is_null() {
            return None;
        }
        let base = unsafe { sys::MapViewOfFile(handle, sys::FILE_MAP_ALL_ACCESS, 0, 0, bytes) };
        if base.is_null() {
            unsafe { sys::CloseHandle(handle) };
            return None;
        }
        Some(SharedMemory {
            base: base.cast(),
            name,
            handle,
        })
    }

    #[cfg(unix)]
    pub fn create(name: &str, bytes: usize) -> Option<SharedMemory> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let name = CString::new(name).ok()?;
        let fd = unsafe {
            sys::shm_open(
                name.as_ptr(),
                sys::CREATE_EXCLUSIVE | sys::O_RDWR,
                0o600 as core::ffi::c_uint,
            )
        };
        if fd < 0 {
            return None;
        }
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let unlink = || unsafe { sys::shm_unlink(name.as_ptr()) };
        if file.set_len(bytes as u64).is_err() {
            unlink();
            return None;
        }
        let base = unsafe {
            sys::mmap(
                ptr::null_mut(),
                bytes,
                sys::PROT_READ_WRITE,
                sys::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if base as isize == -1 {
            unlink();
            return None;
        }
        Some(SharedMemory {
            base: base.cast(),
            name,
            bytes,
            _file: file,
        })
    }

    pub fn base(&self) -> *mut u8 {
        self.base
    }

    pub fn name(&self) -> &str {
        self.name.to_str().unwrap_or_default()
    }
}

impl Drop for SharedMemory {
    #[cfg(windows)]
    fn drop(&mut self) {
        unsafe {
            sys::UnmapViewOfFile(self.base.cast());
            sys::CloseHandle(self.handle);
        }
    }

    #[cfg(unix)]
    fn drop(&mut self) {
        unsafe {
            sys::munmap(self.base.cast(), self.bytes);
            sys::shm_unlink(self.name.as_ptr());
        }
    }
}

#[cfg(target_os = "linux")]
pub fn sandbox_self() {
    use southstar_helper_ffi::southstar_sandbox as _;
    unsafe extern "C" {
        fn ns_security_add_writable_dir(dir: *const c_char);
        fn ns_security_sandbox_init(self_exe: *const c_char);
        fn ns_security_seccomp_init();
    }
    let tmp = std::env::var_os("TMPDIR")
        .filter(|dir| !dir.is_empty())
        .unwrap_or_else(|| "/tmp".into());
    let tmp = CString::new(std::os::unix::ffi::OsStrExt::as_bytes(tmp.as_os_str())).ok();
    let self_exe = std::fs::read_link("/proc/self/exe")
        .ok()
        .and_then(|path| CString::new(path.into_os_string().into_encoded_bytes()).ok());
    unsafe {
        ns_security_add_writable_dir(c"/dev/shm".as_ptr());
        ns_security_add_writable_dir(tmp.as_deref().unwrap_or(c"/tmp").as_ptr());
        ns_security_sandbox_init(self_exe.as_deref().map_or(ptr::null(), |exe| exe.as_ptr()));
        ns_security_seccomp_init();
    }
}

#[cfg(not(target_os = "linux"))]
pub fn sandbox_self() {}
