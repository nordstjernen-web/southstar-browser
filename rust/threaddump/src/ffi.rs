//! Southstar — the C ABI of thread dumps, as declared in src/threaddump.h, and the platform calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::io::Write;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

#[cfg(target_os = "linux")]
pub(crate) fn list_threads(pid: i32, out: &mut Vec<u8>) {
    unsafe extern "C" {
        fn sysconf(name: c_int) -> core::ffi::c_long;
    }
    const SC_CLK_TCK: c_int = 2;
    let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        let _ = writeln!(out, "  (no /proc/{pid}/task)");
        return;
    };
    let tick = match unsafe { sysconf(SC_CLK_TCK) } {
        tick if tick > 0 => tick as f64,
        _ => 100.0,
    };
    let mut count = 0;
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let stat = std::fs::read(format!("/proc/{pid}/task/{name}/stat"))
            .map(|bytes| crate::parse_stat(crate::first_line(&bytes), tick))
            .unwrap_or(crate::ThreadStat {
                state: b'?',
                cpu: -1.0,
                comm: Vec::new(),
            });
        let _ = write!(
            out,
            "  thread {name:<6}  state {}  cpu {:8.3}s  ",
            char::from(stat.state),
            stat.cpu
        );
        out.extend_from_slice(&stat.comm);
        out.push(b'\n');
        count += 1;
    }
    let _ = writeln!(out, "  ({count} threads)");
}

#[cfg(windows)]
pub(crate) fn list_threads(pid: i32, out: &mut Vec<u8>) {
    #[repr(C)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_process_id: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }

    type Handle = *mut c_void;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const TH32CS_SNAPTHREAD: u32 = 0x4;
    const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
        fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry32) -> c_int;
        fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry32) -> c_int;
        fn OpenThread(access: u32, inherit: c_int, thread_id: u32) -> Handle;
        fn GetThreadTimes(
            thread: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> c_int;
        fn CloseHandle(handle: Handle) -> c_int;
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        out.extend_from_slice(b"  (thread snapshot failed)\n");
        return;
    }
    let mut entry = ThreadEntry32 {
        size: core::mem::size_of::<ThreadEntry32>() as u32,
        usage: 0,
        thread_id: 0,
        owner_process_id: 0,
        base_priority: 0,
        delta_priority: 0,
        flags: 0,
    };
    let mut count = 0;
    let mut more = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while more {
        if entry.owner_process_id == pid as u32 {
            let mut cpu = -1.0;
            let thread =
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.thread_id) };
            if !thread.is_null() {
                let [mut creation, mut exit, mut kernel, mut user] = <[FileTime; 4]>::default();
                if unsafe {
                    GetThreadTimes(thread, &mut creation, &mut exit, &mut kernel, &mut user)
                } != 0
                {
                    cpu = (kernel.ticks() + user.ticks()) as f64 / 1e7;
                }
                unsafe { CloseHandle(thread) };
            }
            let _ = writeln!(
                out,
                "  thread {:<6}  cpu {cpu:8.3}s  base-pri {}",
                entry.thread_id, entry.base_priority
            );
            count += 1;
        }
        more = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    let _ = writeln!(out, "  ({count} threads)");
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn list_threads(_pid: i32, out: &mut Vec<u8>) {
    out.extend_from_slice(b"  (thread dump not supported on this platform)\n");
}

fn to_stderr(pid: i32, label: Option<&[u8]>) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(&crate::dump_text(pid, label));
    let _ = stderr.flush();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_thread_dump_text(pid: c_int, label: *const c_char) -> *mut c_char {
    let label = (!label.is_null()).then(|| unsafe { CStr::from_ptr(label) }.to_bytes());
    let text = crate::dump_text(pid, label);
    let out = unsafe { malloc(text.len() + 1) }.cast::<u8>();
    if out.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
        *out.add(text.len()) = 0;
    }
    out.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_thread_dump_to_stderr(pid: c_int, label: *const c_char) {
    let label = (!label.is_null()).then(|| unsafe { CStr::from_ptr(label) }.to_bytes());
    to_stderr(pid, label);
}

#[cfg(unix)]
mod signal {
    use core::ffi::{c_int, c_void};
    use std::fs::File;
    use std::io::{ErrorKind, Read};
    use std::os::fd::FromRawFd;
    use std::sync::atomic::{AtomicI32, Ordering};

    const F_SETFD: c_int = 2;
    const F_SETFL: c_int = 4;
    const FD_CLOEXEC: c_int = 1;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: c_int = 0o4000;
    #[cfg(not(target_os = "linux"))]
    const O_NONBLOCK: c_int = 0x4;
    const SIGQUIT: c_int = 3;
    const EINTR: c_int = 4;

    type Handler = extern "C" fn(c_int);

    unsafe extern "C" {
        fn pipe(fds: *mut c_int) -> c_int;
        fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
        fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        fn signal(signum: c_int, handler: Handler) -> usize;
        #[cfg_attr(
            any(target_os = "linux", target_os = "android"),
            link_name = "__errno_location"
        )]
        #[cfg_attr(
            any(
                target_os = "macos",
                target_os = "ios",
                target_os = "freebsd",
                target_os = "dragonfly"
            ),
            link_name = "__error"
        )]
        #[cfg_attr(
            any(target_os = "netbsd", target_os = "openbsd"),
            link_name = "__errno"
        )]
        fn errno_location() -> *mut c_int;
    }

    static READ_END: AtomicI32 = AtomicI32::new(-1);
    static WRITE_END: AtomicI32 = AtomicI32::new(-1);

    extern "C" fn on_sigquit(_signum: c_int) {
        let errno = unsafe { errno_location() };
        let saved = unsafe { *errno };
        let byte = 1u8;
        let fd = WRITE_END.load(Ordering::Relaxed);
        while unsafe { write(fd, (&raw const byte).cast(), 1) } < 0 && unsafe { *errno } == EINTR {}
        unsafe { *errno = saved };
    }

    fn dispatch(mut pipe: File, label: Option<Vec<u8>>) {
        let mut byte = [0u8; 1];
        loop {
            match pipe.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => super::to_stderr(std::process::id() as i32, label.as_deref()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    }

    pub(super) fn install(label: Option<&[u8]>) {
        if READ_END.load(Ordering::Relaxed) >= 0 {
            return;
        }
        let mut fds = [-1; 2];
        if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
            return;
        }
        unsafe {
            fcntl(fds[0], F_SETFD, FD_CLOEXEC);
            fcntl(fds[1], F_SETFD, FD_CLOEXEC);
            fcntl(fds[1], F_SETFL, O_NONBLOCK);
        }
        READ_END.store(fds[0], Ordering::Relaxed);
        WRITE_END.store(fds[1], Ordering::Relaxed);
        let reader = unsafe { File::from_raw_fd(fds[0]) };
        let label = label.map(<[u8]>::to_vec);
        let _ = std::thread::Builder::new()
            .name("thread-dump".into())
            .spawn(move || dispatch(reader, label));
        unsafe { signal(SIGQUIT, on_sigquit) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_thread_dump_install_signal(label: *const c_char) {
    #[cfg(unix)]
    {
        let label = (!label.is_null()).then(|| unsafe { CStr::from_ptr(label) }.to_bytes());
        signal::install(label);
    }
    #[cfg(not(unix))]
    let _ = label;
}
