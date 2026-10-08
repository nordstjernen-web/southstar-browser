//! Southstar — the southstar-renderer process: its arguments, control channel, framebuffer and request loop around one session.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::io::Write;
use std::time::Duration;

use southstar_ipc::{Conn, Head, set_bufsize, until_nul};

use crate::engine::{self, Browser};
use crate::ffi::new_session;

unsafe extern "C" {
    fn g_getenv(variable: *const c_char) -> *const c_char;
    fn g_setenv(variable: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
    fn ns_thread_dump_install_signal(label: *const c_char);
    fn fflush(stream: *mut c_void) -> c_int;
    fn _exit(status: c_int) -> !;
    #[cfg(feature = "sdl")]
    fn SDL_SetMainReady();
}

fn env(name: &CStr) -> Option<&'static [u8]> {
    let value = unsafe { g_getenv(name.as_ptr()) };
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) }.to_bytes())
}

fn selftest(url: &[u8]) -> c_int {
    if engine::init() != 0 {
        return 1;
    }
    let Some(mut page) = Browser::open_viewport(&engine::cstring(url), 1280, 720.0, 25000) else {
        return 1;
    };
    let mut fb = vec![0u8; 1280 * 4 * 720];
    for frame in 0..600 {
        page.tick(8);
        let rc = page.render_argb32(0, 0, 1280, 720, 1.0, &mut fb);
        if frame == 0 {
            let _ = writeln!(std::io::stderr(), "[selftest] first render rc={rc}");
        }
        std::thread::sleep(Duration::from_micros(16000));
    }
    0
}

fn dimension(arg: &[u8]) -> Option<c_int> {
    let value = southstar_ipc::atoi(arg);
    (1..=32768).contains(&value).then_some(value)
}

enum Pixels {
    Owned(Vec<u8>),
    Shared(*mut u8),
}

impl Pixels {
    fn pointer(&mut self) -> *mut u8 {
        match self {
            Pixels::Owned(v) => v.as_mut_ptr(),
            Pixels::Shared(p) => *p,
        }
    }
}

struct Channel {
    ctrl_r: c_int,
    ctrl_w: c_int,
    pixels: Pixels,
}

pub fn run(args: &[&[u8]]) -> c_int {
    #[cfg(feature = "sdl")]
    unsafe {
        SDL_SetMainReady()
    };
    if let Some(url) = env(c"NS_RENDER_SELFTEST").filter(|u| !u.is_empty()) {
        return selftest(url);
    }
    if args.len() < 3 {
        return 2;
    }
    sys::prepare_fonts();
    let (Some(max_w), Some(max_h)) = (dimension(args[1]), dimension(args[2])) else {
        return 2;
    };
    if !sys::watch_parent() {
        return 0;
    }
    unsafe { ns_thread_dump_install_signal(c"southstar-renderer".as_ptr()) };
    let mode = args.get(3).copied();
    let shm = mode == Some(b"shm");
    let size = max_w as usize * max_h as usize * 4;
    let channel = if mode == Some(b"stdio") {
        sys::stdio_channel(size)
    } else {
        sys::control_channel(args, shm, size)
    };
    let Some(mut channel) = channel else {
        return 2;
    };
    if args[3..].iter().any(|a| *a == b"private") {
        unsafe { g_setenv(c"NS_PRIVATE".as_ptr(), c"1".as_ptr(), 1) };
    }
    if engine::init() != 0 {
        sys::release(channel.pixels, size);
        return 2;
    }
    if let Some(scheme) = env(c"NS_COLOR_SCHEME").filter(|s| !s.is_empty()) {
        engine::set_color_scheme(c_int::from(scheme == b"dark"));
    }
    let self_exe = engine::cstring(args[0]);
    engine::sandbox(Some(&self_exe));
    let bufsz = (size + 65536).min(0x7fff_ffff) as c_int;
    set_bufsize(channel.ctrl_r, bufsz);
    set_bufsize(channel.ctrl_w, bufsz);
    let Some(mut session) =
        new_session(channel.ctrl_w, channel.pixels.pointer(), max_w, max_h, shm)
    else {
        return 2;
    };
    let mut conn = Conn::boxed(channel.ctrl_r);
    let mut head = Head::boxed();
    while conn.read_head(&mut head) {
        let mut body = vec![0u8; head.content_length.max(0) as usize];
        if !body.is_empty() && !conn.read_body(&mut body) {
            break;
        }
        let path = until_nul(&head.path).to_vec();
        if session.handle(&path, until_nul(&body)) {
            break;
        }
    }
    drop(session);
    engine::shutdown();
    if !engine::net_idle() {
        unsafe {
            fflush(ptr::null_mut());
            _exit(0)
        };
    }
    sys::release(channel.pixels, size);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let args: Vec<&[u8]> = (0..usize::try_from(argc).unwrap_or(0))
        .map(|i| unsafe { *argv.add(i) })
        .take_while(|arg| !arg.is_null())
        .map(|arg| unsafe { CStr::from_ptr(arg) }.to_bytes())
        .collect();
    run(&args)
}

#[cfg(unix)]
mod sys {
    use core::ffi::{c_int, c_void};
    use core::ptr;

    use super::{Channel, Pixels};

    const SIGPIPE: c_int = 13;
    const SIG_IGN: usize = 1;
    const PROT_READ_WRITE: c_int = 3;
    const MAP_SHARED: c_int = 1;

    unsafe extern "C" {
        fn signal(signum: c_int, handler: usize) -> usize;
        fn close(fd: c_int) -> c_int;
        fn getppid() -> c_int;
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
        #[cfg(target_os = "linux")]
        fn prctl(option: c_int, ...) -> c_int;
    }

    pub fn prepare_fonts() {}

    pub fn watch_parent() -> bool {
        #[cfg(target_os = "linux")]
        unsafe {
            prctl(1, 9 as core::ffi::c_ulong);
        }
        if cfg!(any(target_os = "linux", target_vendor = "apple")) && unsafe { getppid() } == 1 {
            return false;
        }
        #[cfg(target_vendor = "apple")]
        let _ = std::thread::Builder::new()
            .name("nd-rparent".to_owned())
            .spawn(|| {
                while unsafe { getppid() } != 1 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                unsafe { super::_exit(0) };
            });
        true
    }

    pub fn stdio_channel(size: usize) -> Option<Channel> {
        unsafe { signal(SIGPIPE, SIG_IGN) };
        Some(Channel {
            ctrl_r: 0,
            ctrl_w: 1,
            pixels: Pixels::Owned(vec![0u8; size]),
        })
    }

    pub fn control_channel(_args: &[&[u8]], shm: bool, size: usize) -> Option<Channel> {
        unsafe { signal(SIGPIPE, SIG_IGN) };
        let pixels = if shm {
            let fd = southstar_ipc::recv_fd(3);
            if fd < 0 {
                return None;
            }
            let map = unsafe { mmap(ptr::null_mut(), size, PROT_READ_WRITE, MAP_SHARED, fd, 0) };
            unsafe { close(fd) };
            if map as isize == -1 {
                return None;
            }
            Pixels::Shared(map.cast())
        } else {
            Pixels::Owned(vec![0u8; size])
        };
        Some(Channel {
            ctrl_r: 3,
            ctrl_w: 3,
            pixels,
        })
    }

    pub fn release(pixels: Pixels, size: usize) {
        if let Pixels::Shared(map) = pixels {
            unsafe { munmap(map.cast(), size) };
        }
    }
}

#[cfg(windows)]
mod sys {
    use core::ffi::{c_char, c_int, c_void};
    use core::ptr;

    use super::{Channel, Pixels};

    type Handle = *mut c_void;

    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const DUPLICATE_SAME_ACCESS: u32 = 2;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ_WRITE: u32 = 1 | 2;
    const OPEN_EXISTING: u32 = 3;
    const FILE_MAP_READ_WRITE: u32 = 4 | 2;
    const O_BINARY: c_int = 0x8000;

    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn GetStdHandle(which: u32) -> Handle;
        fn SetStdHandle(which: u32, handle: Handle) -> i32;
        fn DuplicateHandle(
            source_process: Handle,
            source: Handle,
            target_process: Handle,
            target: *mut Handle,
            access: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn CreateFileA(
            name: *const c_char,
            access: u32,
            share: u32,
            security: *mut c_void,
            disposition: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn MapViewOfFile(
            mapping: Handle,
            access: u32,
            offset_high: u32,
            offset_low: u32,
            bytes: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(base: *const c_void) -> i32;
        #[cfg(feature = "fontconfig")]
        fn GetModuleFileNameW(module: Handle, name: *mut u16, size: u32) -> u32;
    }

    unsafe extern "C" {
        fn _open_osfhandle(handle: isize, flags: c_int) -> c_int;
        fn _setmode(fd: c_int, mode: c_int) -> c_int;
        fn freopen(path: *const c_char, mode: *const c_char, stream: *mut c_void) -> *mut c_void;
        fn __acrt_iob_func(index: u32) -> *mut c_void;
        #[cfg(feature = "fontconfig")]
        fn FcInit() -> c_int;
    }

    #[cfg(feature = "fontconfig")]
    pub fn prepare_fonts() {
        use std::ffi::CString;
        use std::path::PathBuf;

        let mut wide = [0u16; 4096];
        let n = unsafe { GetModuleFileNameW(ptr::null_mut(), wide.as_mut_ptr(), 4096) } as usize;
        let dir = (n > 0 && n < 4096)
            .then(|| String::from_utf16(&wide[..n]).ok())
            .flatten()
            .and_then(|exe| PathBuf::from(exe).parent().map(PathBuf::from));
        let set = |name: &str, value: &std::path::Path| {
            let name = CString::new(name).unwrap_or_default();
            let value = CString::new(value.to_string_lossy().into_owned()).unwrap_or_default();
            unsafe { super::g_setenv(name.as_ptr(), value.as_ptr(), 1) };
        };
        if let Some(dir) = dir {
            let fonts = dir.join("etc").join("fonts");
            let conf = fonts.join("fonts.conf");
            if super::env(c"FONTCONFIG_FILE").is_none() && conf.exists() {
                set("FONTCONFIG_FILE", &conf);
            }
            if super::env(c"FONTCONFIG_PATH").is_none() && fonts.is_dir() {
                set("FONTCONFIG_PATH", &fonts);
            }
        }
        if super::env(c"PANGOCAIRO_BACKEND").is_none() {
            unsafe { super::g_setenv(c"PANGOCAIRO_BACKEND".as_ptr(), c"fc".as_ptr(), 1) };
        }
        unsafe { FcInit() };
    }

    #[cfg(not(feature = "fontconfig"))]
    pub fn prepare_fonts() {}

    pub fn watch_parent() -> bool {
        true
    }

    pub fn stdio_channel(size: usize) -> Option<Channel> {
        unsafe {
            _setmode(0, O_BINARY);
            _setmode(1, O_BINARY);
        }
        Some(Channel {
            ctrl_r: 0,
            ctrl_w: 1,
            pixels: Pixels::Owned(vec![0u8; size]),
        })
    }

    fn handle_arg(arg: &[u8]) -> usize {
        let digits = arg.iter().take_while(|b| b.is_ascii_digit());
        digits.fold(0u64, |acc, &d| {
            acc.saturating_mul(10).saturating_add(u64::from(d - b'0'))
        }) as usize
    }

    pub fn control_channel(args: &[&[u8]], shm: bool, size: usize) -> Option<Channel> {
        let me = unsafe { GetCurrentProcess() };
        let (mut ipc_in, mut ipc_out): (Handle, Handle) = (ptr::null_mut(), ptr::null_mut());
        let duplicated = unsafe {
            DuplicateHandle(
                me,
                GetStdHandle(STD_INPUT_HANDLE),
                me,
                &mut ipc_in,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) != 0
                && DuplicateHandle(
                    me,
                    GetStdHandle(STD_OUTPUT_HANDLE),
                    me,
                    &mut ipc_out,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                ) != 0
        };
        if !duplicated {
            return None;
        }
        let open_nul = |access: u32| unsafe {
            CreateFileA(
                c"NUL".as_ptr(),
                access,
                FILE_SHARE_READ_WRITE,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        let nul_in = open_nul(GENERIC_READ);
        let nul_out = open_nul(GENERIC_WRITE);
        unsafe {
            if nul_in as isize != -1 {
                SetStdHandle(STD_INPUT_HANDLE, nul_in);
            }
            if nul_out as isize != -1 {
                SetStdHandle(STD_OUTPUT_HANDLE, nul_out);
            }
            freopen(c"NUL".as_ptr(), c"r".as_ptr(), __acrt_iob_func(0));
            freopen(c"NUL".as_ptr(), c"w".as_ptr(), __acrt_iob_func(1));
        }
        let ctrl_r = unsafe { _open_osfhandle(ipc_in as isize, O_BINARY) };
        let ctrl_w = unsafe { _open_osfhandle(ipc_out as isize, O_BINARY) };
        if ctrl_r < 0 || ctrl_w < 0 {
            return None;
        }
        let pixels = if shm {
            let handle = handle_arg(args.get(4)?) as Handle;
            let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ_WRITE, 0, 0, size) };
            if view.is_null() {
                return None;
            }
            Pixels::Shared(view.cast())
        } else {
            Pixels::Owned(vec![0u8; size])
        };
        Some(Channel {
            ctrl_r,
            ctrl_w,
            pixels,
        })
    }

    pub fn release(pixels: Pixels, _size: usize) {
        if let Pixels::Shared(view) = pixels {
            unsafe { UnmapViewOfFile(view.cast()) };
        }
    }
}
