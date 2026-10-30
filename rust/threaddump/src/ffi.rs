//! Southstar — installing the thread-dump signal, and the platform calls behind thread dumps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(target_os = "linux")]
pub(crate) fn list_threads(pid: i32, out: &mut Vec<u8>) {
    use core::ffi::c_int;
    use std::io::Write;

    unsafe extern "C" {
        fn sysconf(name: c_int) -> core::ffi::c_long;
    }
    const SC_CLK_TCK: c_int = 2;
    let Ok(dir) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        let _ = writeln!(out, "  (no /proc/{pid}/task)");
        return;
    };
    // SAFETY: sysconf takes an integer name and touches no caller memory.
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
    use core::ffi::{c_int, c_void};
    use std::io::Write;

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

    struct Owned(Handle);

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: Owned only wraps a valid handle this function opened, closed exactly once here.
            unsafe { CloseHandle(self.0) };
        }
    }

    // SAFETY: CreateToolhelp32Snapshot takes plain flags and a process id and touches no caller memory.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        out.extend_from_slice(b"  (thread snapshot failed)\n");
        return;
    }
    let snapshot = Owned(snapshot);
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
    // SAFETY: snapshot is a live thread snapshot and entry is a writable THREADENTRY32 whose size field is set.
    let mut more = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
    while more {
        if entry.owner_process_id == pid as u32 {
            let mut cpu = -1.0;
            // SAFETY: OpenThread takes plain access flags and a thread id and touches no caller memory.
            let thread =
                unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.thread_id) };
            if !thread.is_null() {
                let thread = Owned(thread);
                let [mut creation, mut exit, mut kernel, mut user] = <[FileTime; 4]>::default();
                // SAFETY: thread is a live handle opened with query access and the four FILETIMEs are writable locals.
                if unsafe {
                    GetThreadTimes(thread.0, &mut creation, &mut exit, &mut kernel, &mut user)
                } != 0
                {
                    cpu = (kernel.ticks() + user.ticks()) as f64 / 1e7;
                }
            }
            let _ = writeln!(
                out,
                "  thread {:<6}  cpu {cpu:8.3}s  base-pri {}",
                entry.thread_id, entry.base_priority
            );
            count += 1;
        }
        // SAFETY: snapshot is a live thread snapshot and entry is a writable THREADENTRY32 whose size field is set.
        more = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
    }
    drop(snapshot);
    let _ = writeln!(out, "  ({count} threads)");
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(crate) fn list_threads(_pid: i32, out: &mut Vec<u8>) {
    out.extend_from_slice(b"  (thread dump not supported on this platform)\n");
}

#[cfg(unix)]
mod signal {
    use core::ffi::{c_int, c_void};
    use std::io::{ErrorKind, PipeReader, Read, Write};
    use std::os::fd::{AsRawFd, IntoRawFd};
    use std::sync::atomic::{AtomicI32, Ordering};

    const F_SETFL: c_int = 4;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: c_int = 0o4000;
    #[cfg(not(target_os = "linux"))]
    const O_NONBLOCK: c_int = 0x4;
    const SIGQUIT: c_int = 3;
    const EINTR: c_int = 4;

    type Handler = extern "C" fn(c_int);

    unsafe extern "C" {
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
        // SAFETY: errno_location has no preconditions and returns this thread's errno slot.
        let errno = unsafe { errno_location() };
        // SAFETY: errno is this thread's live, aligned errno slot.
        let saved = unsafe { errno.read() };
        let byte = 1u8;
        let fd = WRITE_END.load(Ordering::Relaxed);
        loop {
            // SAFETY: write reads the one byte at &byte, a live local; it is async-signal-safe.
            let wrote = unsafe { write(fd, (&raw const byte).cast(), 1) };
            // SAFETY: errno is this thread's live, aligned errno slot.
            if wrote >= 0 || unsafe { errno.read() } != EINTR {
                break;
            }
        }
        // SAFETY: errno is this thread's live, aligned errno slot; restoring it keeps the interrupted code's errno intact.
        unsafe { errno.write(saved) };
    }

    fn to_stderr(pid: i32, label: Option<&[u8]>) {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(&crate::dump_text(pid, label));
        let _ = stderr.flush();
    }

    fn dispatch(mut pipe: PipeReader, label: Option<Vec<u8>>) {
        let mut byte = [0u8; 1];
        loop {
            match pipe.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => to_stderr(std::process::id() as i32, label.as_deref()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
    }

    pub(super) fn install(label: Option<&[u8]>) {
        if READ_END.load(Ordering::Relaxed) >= 0 {
            return;
        }
        let Ok((reader, writer)) = std::io::pipe() else {
            return;
        };
        let write_end = writer.into_raw_fd();
        // SAFETY: F_SETFL with an integer flag argument only changes the write end's status flags; it touches no memory.
        unsafe { fcntl(write_end, F_SETFL, O_NONBLOCK) };
        READ_END.store(reader.as_raw_fd(), Ordering::Relaxed);
        WRITE_END.store(write_end, Ordering::Relaxed);
        let label = label.map(<[u8]>::to_vec);
        let _ = std::thread::Builder::new()
            .name("thread-dump".into())
            .spawn(move || dispatch(reader, label));
        // SAFETY: on_sigquit only loads an atomic, calls write and saves and restores errno, all async-signal-safe.
        unsafe { signal(SIGQUIT, on_sigquit) };
    }
}

pub fn install_signal(label: Option<&[u8]>) {
    #[cfg(unix)]
    signal::install(label);
    #[cfg(not(unix))]
    let _ = label;
}
