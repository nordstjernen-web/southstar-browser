//! Southstar — the shared-memory audio clock the shell reads each player's position from, laid out as rust/media-shm declares.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use core::sync::atomic::Ordering;

use southstar_media_shm::{
    AUDIO_CLOCK_MAGIC, AUDIO_CLOCK_SLOTS, AUDIO_CLOCK_VERSION, AudioClockHdr as Header,
    MEDIA_CLOCK_PLAYING, MEDIA_CLOCK_USED,
};

const BYTES: usize = core::mem::size_of::<Header>();

pub struct Sample<'a> {
    pub playing: bool,
    pub position: f64,
    pub token: &'a [u8],
}

pub struct Clock {
    base: *mut Header,
    name: Vec<u8>,
    map: os::Map,
}

unsafe impl Send for Clock {}

impl Clock {
    pub fn create() -> Option<Clock> {
        let name = format!("/nsaud-{}", std::process::id()).into_bytes();
        let (map, base) = os::Map::create(&name, BYTES)?;
        let base = base.cast::<Header>();
        unsafe {
            ptr::write_bytes(base.cast::<u8>(), 0, BYTES);
            (*base).magic = AUDIO_CLOCK_MAGIC;
            (*base).version = AUDIO_CLOCK_VERSION;
            (*base).nslots = AUDIO_CLOCK_SLOTS;
        }
        Some(Clock { base, name, map })
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn store(&self, index: usize, sample: Option<Sample>) {
        if index >= AUDIO_CLOCK_SLOTS as usize {
            return;
        }
        let slot = unsafe { &raw mut (*self.base).slots[index] };
        let sequence = unsafe { &(*slot).sequence };
        let mut seq = sequence.load(Ordering::Relaxed);
        if seq & 1 != 0 {
            seq = seq.wrapping_add(1);
        }
        let now_us = southstar_glib::monotonic_us();
        sequence.store(seq.wrapping_add(1), Ordering::Release);
        unsafe {
            match sample {
                None => {
                    ptr::write_volatile(&raw mut (*slot).flags, 0);
                    ptr::write_volatile(&raw mut (*slot).position, 0.0);
                    ptr::write_volatile(&raw mut (*slot).token[0], 0);
                }
                Some(s) => {
                    let flags = MEDIA_CLOCK_USED | if s.playing { MEDIA_CLOCK_PLAYING } else { 0 };
                    ptr::write_volatile(&raw mut (*slot).flags, flags);
                    ptr::write_volatile(&raw mut (*slot).position, s.position);
                    let mut token = [0u8; 64];
                    let n = s.token.len().min(63);
                    token[..n].copy_from_slice(&s.token[..n]);
                    ptr::write_volatile(&raw mut (*slot).token, token);
                }
            }
            ptr::write_volatile(&raw mut (*slot).monotonic_us, now_us);
        }
        sequence.store(seq.wrapping_add(2), Ordering::Release);
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        self.map.destroy(self.base.cast(), BYTES, &self.name);
    }
}

#[cfg(unix)]
mod os {
    use core::ffi::{c_char, c_int, c_uint, c_void};
    use std::ffi::CString;
    use std::fs::File;
    use std::mem::ManuallyDrop;
    use std::os::fd::FromRawFd;

    const O_RDWR: c_int = 2;
    #[cfg(target_os = "linux")]
    const CREATE_EXCLUSIVE: c_int = 0x40 | 0x80;
    #[cfg(not(target_os = "linux"))]
    const CREATE_EXCLUSIVE: c_int = 0x200 | 0x800;
    const PROT_READ_WRITE: c_int = 3;
    const MAP_SHARED: c_int = 1;
    const MAP_FAILED: *mut c_void = !0usize as *mut c_void;

    unsafe extern "C" {
        fn shm_open(name: *const c_char, oflag: c_int, ...) -> c_int;
        fn shm_unlink(name: *const c_char) -> c_int;
        fn close(fd: c_int) -> c_int;
        #[cfg(not(all(target_os = "linux", target_env = "gnu", target_pointer_width = "32")))]
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        #[cfg(all(target_os = "linux", target_env = "gnu", target_pointer_width = "32"))]
        #[link_name = "mmap64"]
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> c_int;
    }

    pub struct Map {
        fd: c_int,
    }

    impl Map {
        pub fn create(name: &[u8], bytes: usize) -> Option<(Map, *mut c_void)> {
            let cname = CString::new(name).ok()?;
            let fd =
                unsafe { shm_open(cname.as_ptr(), CREATE_EXCLUSIVE | O_RDWR, 0o600 as c_uint) };
            if fd < 0 {
                return None;
            }
            let file = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
            let base = if file.set_len(bytes as u64).is_ok() {
                unsafe {
                    mmap(
                        core::ptr::null_mut(),
                        bytes,
                        PROT_READ_WRITE,
                        MAP_SHARED,
                        fd,
                        0,
                    )
                }
            } else {
                MAP_FAILED
            };
            if base == MAP_FAILED {
                unsafe {
                    close(fd);
                    shm_unlink(cname.as_ptr());
                }
                return None;
            }
            Some((Map { fd }, base))
        }

        pub fn destroy(&mut self, base: *mut c_void, bytes: usize, name: &[u8]) {
            unsafe {
                munmap(base, bytes);
                close(self.fd);
            }
            if let Ok(cname) = CString::new(name) {
                unsafe { shm_unlink(cname.as_ptr()) };
            }
        }
    }
}

#[cfg(windows)]
mod os {
    use core::ffi::{c_char, c_void};
    use std::ffi::CString;

    type Handle = *mut c_void;

    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PAGE_READWRITE: u32 = 4;
    const FILE_MAP_ALL_ACCESS: u32 = 0xF001F;

    unsafe extern "system" {
        fn CreateFileMappingA(
            file: Handle,
            attributes: *mut c_void,
            protect: u32,
            size_high: u32,
            size_low: u32,
            name: *const c_char,
        ) -> Handle;
        fn MapViewOfFile(
            mapping: Handle,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub struct Map {
        handle: Handle,
    }

    impl Map {
        pub fn create(name: &[u8], bytes: usize) -> Option<(Map, *mut c_void)> {
            let cname = CString::new(name).ok()?;
            let handle = unsafe {
                CreateFileMappingA(
                    INVALID_HANDLE_VALUE,
                    core::ptr::null_mut(),
                    PAGE_READWRITE,
                    0,
                    bytes as u32,
                    cname.as_ptr(),
                )
            };
            if handle.is_null() {
                return None;
            }
            let base = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, bytes) };
            if base.is_null() {
                unsafe { CloseHandle(handle) };
                return None;
            }
            Some((Map { handle }, base))
        }

        pub fn destroy(&mut self, base: *mut c_void, _bytes: usize, _name: &[u8]) {
            unsafe {
                UnmapViewOfFile(base);
                CloseHandle(self.handle);
            }
        }
    }
}
