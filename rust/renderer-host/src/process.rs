//! Southstar — the southstar-renderer process: its arguments, control channel, framebuffer and request loop around one session.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::io::Write;
use std::time::Duration;

use southstar_ipc::{Conn, Head, set_bufsize, until_nul};

use crate::engine::{self, Page};
use crate::ffi::new_session;
use crate::frame::FrameMemory;

unsafe extern "C" {
    fn g_setenv(variable: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
    fn fflush(stream: *mut c_void) -> c_int;
    fn _exit(status: c_int) -> !;
    #[cfg(feature = "sdl")]
    fn SDL_SetMainReady();
}

fn set_env(name: &CStr, value: &CStr) {
    // SAFETY: both strings are NUL-terminated, and the renderer calls this during start-up, before it starts another thread.
    unsafe { g_setenv(name.as_ptr(), value.as_ptr(), 1) };
}

fn env(name: &CStr) -> Option<Vec<u8>> {
    southstar_glib::env_bytes(name.to_str().ok()?)
}

fn selftest(url: &[u8]) -> c_int {
    engine::init();
    let Some(page) = Page::open(&engine::cstring(url), 1280, 720.0, 25000, None) else {
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

struct Channel {
    ctrl_r: c_int,
    ctrl_w: c_int,
    pixels: FrameMemory,
}

pub fn run(args: &[&[u8]]) -> c_int {
    #[cfg(feature = "sdl")]
    // SAFETY: SDL_SetMainReady only sets a flag, and runs before any other SDL call.
    unsafe {
        SDL_SetMainReady()
    };
    if let Some(url) = env(c"NS_RENDER_SELFTEST").filter(|u| !u.is_empty()) {
        return selftest(&url);
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
    let mode = args.get(3).copied();
    let shm = mode == Some(b"shm");
    let size = max_w as usize * max_h as usize * 4;
    let channel = if mode == Some(b"stdio") {
        sys::stdio_channel(size)
    } else {
        sys::control_channel(args, shm, size)
    };
    let Some(channel) = channel else {
        return 2;
    };
    let private = args[3..].iter().any(|a| *a == b"private");
    let settings = [
        (c"GIO_USE_VFS", c"local", true),
        (c"NS_PRIVATE", c"1", private),
    ];
    for (name, value, _) in settings.iter().filter(|(_, _, wanted)| *wanted) {
        set_env(name, value);
    }
    engine::init();
    if let Some(scheme) = env(c"NS_COLOR_SCHEME").filter(|s| !s.is_empty()) {
        engine::set_color_scheme(scheme == b"dark");
    }
    let self_exe = engine::cstring(args[0]);
    engine::sandbox(Some(&self_exe));
    southstar_threaddump::install_signal(Some(b"southstar-renderer"));
    let bufsz = (size + 65536).min(0x7fff_ffff) as c_int;
    set_bufsize(channel.ctrl_r, bufsz);
    set_bufsize(channel.ctrl_w, bufsz);
    let Some(mut session) = new_session(channel.ctrl_w, channel.pixels, max_w, max_h, shm) else {
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
        // SAFETY: fflush(NULL) flushes every open output stream.
        unsafe { fflush(ptr::null_mut()) };
        // SAFETY: _exit ends the process without running the atexit handlers that would wait on the busy network threads.
        unsafe { _exit(0) };
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_renderer_main(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let argc = usize::try_from(argc).unwrap_or(0);
    if argv.is_null() || argc == 0 {
        return run(&[]);
    }
    // SAFETY: the C main passes `argc` argument pointers at `argv`, live for the whole process.
    let argv = unsafe { core::slice::from_raw_parts(argv, argc) };
    let args: Vec<&[u8]> = argv
        .iter()
        .take_while(|arg| !arg.is_null())
        // SAFETY: each non-null argument is a NUL-terminated string live for the whole process.
        .map(|&arg| unsafe { CStr::from_ptr(arg) }.to_bytes())
        .collect();
    run(&args)
}

#[cfg(unix)]
mod sys {
    use core::ffi::{c_int, c_void};
    use core::ptr;

    use super::Channel;
    use crate::frame::FrameMemory;

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
        // SAFETY: PR_SET_PDEATHSIG takes the signal number as its one argument.
        unsafe {
            prctl(1, 9 as core::ffi::c_ulong);
        }
        // SAFETY: getppid takes no arguments and cannot fail.
        if cfg!(any(target_os = "linux", target_vendor = "apple")) && unsafe { getppid() } == 1 {
            return false;
        }
        #[cfg(target_vendor = "apple")]
        let _ = std::thread::Builder::new()
            .name("nd-rparent".to_owned())
            .spawn(|| {
                // SAFETY: getppid takes no arguments and cannot fail.
                while unsafe { getppid() } != 1 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
                // SAFETY: the parent is gone, so the renderer ends at once without unwinding other threads.
                unsafe { super::_exit(0) };
            });
        true
    }

    fn ignore_sigpipe() {
        // SAFETY: SIG_IGN is a valid disposition for SIGPIPE, so writes to a closed pipe return EPIPE instead.
        unsafe { signal(SIGPIPE, SIG_IGN) };
    }

    pub fn stdio_channel(size: usize) -> Option<Channel> {
        ignore_sigpipe();
        Some(Channel {
            ctrl_r: 0,
            ctrl_w: 1,
            pixels: FrameMemory::owned(size),
        })
    }

    pub fn control_channel(_args: &[&[u8]], shm: bool, size: usize) -> Option<Channel> {
        ignore_sigpipe();
        let pixels = if shm {
            let fd = southstar_ipc::recv_fd(3);
            if fd < 0 {
                return None;
            }
            // SAFETY: a fresh shared mapping of the received descriptor at an address the kernel picks touches no existing memory.
            let map = unsafe { mmap(ptr::null_mut(), size, PROT_READ_WRITE, MAP_SHARED, fd, 0) };
            // SAFETY: the received descriptor is owned here and not used again; the mapping keeps its own reference.
            unsafe { close(fd) };
            if map as isize == -1 {
                return None;
            }
            // SAFETY: the mapping spans `size` writable bytes and unmap_frame unmaps it once on drop.
            unsafe { FrameMemory::foreign(map.cast(), size, unmap_frame) }?
        } else {
            FrameMemory::owned(size)
        };
        Some(Channel {
            ctrl_r: 3,
            ctrl_w: 3,
            pixels,
        })
    }

    /// # Safety
    ///
    /// `pixels` must be a live mapping of `len` bytes from this module's mmap.
    unsafe fn unmap_frame(pixels: core::ptr::NonNull<u8>, len: usize) {
        // SAFETY: the caller passes the mapping and length mmap returned.
        unsafe { munmap(pixels.as_ptr().cast(), len) };
    }
}

#[cfg(windows)]
mod sys {
    use core::ffi::{c_char, c_int, c_void};
    use core::ptr;

    use super::Channel;
    use crate::frame::FrameMemory;

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
    const CURRENT_PROCESS: Handle = ptr::without_provenance_mut(usize::MAX);

    unsafe extern "system" {
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
        // SAFETY: the buffer holds the 4096 UTF-16 units the size argument promises.
        let n = unsafe { GetModuleFileNameW(ptr::null_mut(), wide.as_mut_ptr(), 4096) } as usize;
        let dir = (n > 0 && n < 4096)
            .then(|| String::from_utf16(&wide[..n]).ok())
            .flatten()
            .and_then(|exe| PathBuf::from(exe).parent().map(PathBuf::from));
        let set = |name: &str, value: &std::path::Path| {
            let name = CString::new(name).unwrap_or_default();
            let value = CString::new(value.to_string_lossy().into_owned()).unwrap_or_default();
            super::set_env(&name, &value);
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
            super::set_env(c"PANGOCAIRO_BACKEND", c"fc");
        }
        // SAFETY: FcInit takes no arguments and loads the configuration the variables above point at.
        unsafe { FcInit() };
    }

    #[cfg(not(feature = "fontconfig"))]
    pub fn prepare_fonts() {}

    pub fn watch_parent() -> bool {
        true
    }

    pub fn stdio_channel(size: usize) -> Option<Channel> {
        // SAFETY: descriptor 0 is the CRT's standard input, which _setmode only switches to binary mode.
        unsafe { _setmode(0, O_BINARY) };
        // SAFETY: descriptor 1 is the CRT's standard output, likewise.
        unsafe { _setmode(1, O_BINARY) };
        Some(Channel {
            ctrl_r: 0,
            ctrl_w: 1,
            pixels: FrameMemory::owned(size),
        })
    }

    fn handle_arg(arg: &[u8]) -> usize {
        let digits = arg.iter().take_while(|b| b.is_ascii_digit());
        digits.fold(0u64, |acc, &d| {
            acc.saturating_mul(10).saturating_add(u64::from(d - b'0'))
        }) as usize
    }

    fn duplicate_std(which: u32) -> Option<Handle> {
        let me = CURRENT_PROCESS;
        // SAFETY: GetStdHandle only reads the process's standard handle table.
        let source = unsafe { GetStdHandle(which) };
        let mut copy: Handle = ptr::null_mut();
        // SAFETY: both process handles are this process, `source` is null or a handle it owns, and `copy` is a local out-parameter.
        let ok = unsafe { DuplicateHandle(me, source, me, &mut copy, 0, 0, DUPLICATE_SAME_ACCESS) };
        (ok != 0).then_some(copy)
    }

    fn open_nul(access: u32) -> Handle {
        // SAFETY: the name is NUL-terminated and every pointer argument is null, which CreateFileA accepts.
        unsafe {
            CreateFileA(
                c"NUL".as_ptr(),
                access,
                FILE_SHARE_READ_WRITE,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        }
    }

    fn redirect_std(which: u32, handle: Handle) {
        if handle as isize != -1 {
            // SAFETY: `handle` is a valid handle to NUL that the process keeps for good.
            unsafe { SetStdHandle(which, handle) };
        }
    }

    fn reopen_nul(index: u32, mode: &core::ffi::CStr) {
        // SAFETY: indexes 0 and 1 name the CRT's stdin and stdout streams, which live for the whole process.
        let stream = unsafe { __acrt_iob_func(index) };
        // SAFETY: path and mode are NUL-terminated and `stream` is a live CRT stream.
        unsafe { freopen(c"NUL".as_ptr(), mode.as_ptr(), stream) };
    }

    fn crt_fd(handle: Handle) -> c_int {
        // SAFETY: the CRT descriptor takes over `handle`, a duplicate nothing else owns.
        unsafe { _open_osfhandle(handle as isize, O_BINARY) }
    }

    pub fn control_channel(args: &[&[u8]], shm: bool, size: usize) -> Option<Channel> {
        let ipc_in = duplicate_std(STD_INPUT_HANDLE)?;
        let ipc_out = duplicate_std(STD_OUTPUT_HANDLE)?;
        redirect_std(STD_INPUT_HANDLE, open_nul(GENERIC_READ));
        redirect_std(STD_OUTPUT_HANDLE, open_nul(GENERIC_WRITE));
        reopen_nul(0, c"r");
        reopen_nul(1, c"w");
        let ctrl_r = crt_fd(ipc_in);
        let ctrl_w = crt_fd(ipc_out);
        if ctrl_r < 0 || ctrl_w < 0 {
            return None;
        }
        let pixels = if shm {
            let handle = handle_arg(args.get(4)?) as Handle;
            // SAFETY: a fresh view of the inherited mapping handle at an address the system picks touches no existing memory; a bad handle yields null.
            let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ_WRITE, 0, 0, size) };
            if view.is_null() {
                return None;
            }
            // SAFETY: the view spans `size` writable bytes and unmap_frame unmaps it once on drop.
            unsafe { FrameMemory::foreign(view.cast(), size, unmap_frame) }?
        } else {
            FrameMemory::owned(size)
        };
        Some(Channel {
            ctrl_r,
            ctrl_w,
            pixels,
        })
    }

    /// # Safety
    ///
    /// `pixels` must be a live view this module's MapViewOfFile returned.
    unsafe fn unmap_frame(pixels: core::ptr::NonNull<u8>, _len: usize) {
        // SAFETY: the caller passes the view MapViewOfFile returned.
        unsafe { UnmapViewOfFile(pixels.as_ptr().cast()) };
    }
}
