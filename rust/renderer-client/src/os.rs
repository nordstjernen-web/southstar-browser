//! Southstar — starting, stopping and inspecting renderer processes, and mapping the framebuffer they share with the window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};
use core::ptr;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use southstar_ipc::{Conn, Head};

use crate::client::Renderer;

pub type AttachFn = fn(
    ctrl_r: c_int,
    ctrl_w: c_int,
    fb: Arc<SharedFrame>,
    max_w: c_int,
    max_h: c_int,
) -> Option<InprocId>;

pub type InprocId = u64;

pub struct SharedFrame(Box<[AtomicU8]>);

impl SharedFrame {
    fn zeroed(len: usize) -> SharedFrame {
        let bytes = Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut [AtomicU8];
        // SAFETY: the pointer comes from Box::into_raw of a byte slice, and AtomicU8 has the size, alignment and bit validity of u8.
        SharedFrame(unsafe { Box::from_raw(bytes) })
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.0.as_ptr().cast::<u8>().cast_mut()
    }
}

static ATTACH: Mutex<Option<AttachFn>> = Mutex::new(None);

pub fn set_attach(attach: Option<AttachFn>) {
    *ATTACH.lock().unwrap_or_else(|e| e.into_inner()) = attach;
}

fn attach_hook() -> Option<AttachFn> {
    *ATTACH.lock().unwrap_or_else(|e| e.into_inner())
}

pub enum Child {
    Process(sys::Process),
    InProcess,
}

pub enum Framebuffer {
    None,
    Mapped(sys::Mapping),
    Shared(Arc<SharedFrame>),
}

impl Framebuffer {
    pub fn pixels(&self) -> *const u8 {
        match self {
            Framebuffer::None => ptr::null(),
            Framebuffer::Mapped(map) => map.pixels(),
            Framebuffer::Shared(fb) => fb.as_ptr(),
        }
    }
}

fn dimensions_ok(max_w: c_int, max_h: c_int) -> bool {
    (1..=32768).contains(&max_w) && (1..=32768).contains(&max_h)
}

fn renderer(
    child: Child,
    sock: c_int,
    wfd: c_int,
    fb: Framebuffer,
    size: usize,
    max_w: c_int,
    max_h: c_int,
) -> Renderer {
    let rx = matches!(fb, Framebuffer::None).then(|| vec![0u8; size]);
    let rxcap = if matches!(fb, Framebuffer::Shared(_)) {
        0
    } else {
        size
    };
    Renderer {
        child,
        sock,
        wfd,
        conn: Conn::boxed(sock),
        rx,
        rxcap,
        fb,
        max_w,
        max_h,
        dpr_milli: 0,
        inproc_conn: None,
        linear: (0, 0, 0),
        head: Head::boxed(),
    }
}

pub fn spawn(
    path: Option<&[u8]>,
    max_w: c_int,
    max_h: c_int,
    shm: bool,
    private: bool,
) -> Option<Renderer> {
    if attach_hook().is_some() {
        return spawn_inproc(max_w, max_h);
    }
    let path = path.filter(|_| dimensions_ok(max_w, max_h))?;
    sys::spawn(path, max_w, max_h, shm, private)
}

fn spawn_inproc(max_w: c_int, max_h: c_int) -> Option<Renderer> {
    let attach = attach_hook()?;
    if !dimensions_ok(max_w, max_h) {
        return None;
    }
    let size = max_w as usize * max_h as usize * 4;
    let fb = Arc::new(SharedFrame::zeroed(size));
    let (server_r, server_w, client_r, client_w) = sys::inproc_channel()?;
    let Some(conn) = attach(server_r, server_w, Arc::clone(&fb), max_w, max_h) else {
        sys::close_channel(server_r, server_w, client_r, client_w);
        return None;
    };
    let mut r = renderer(
        Child::InProcess,
        client_r,
        client_w,
        Framebuffer::Shared(fb),
        size,
        max_w,
        max_h,
    );
    r.inproc_conn = Some(conn);
    Some(r)
}

impl Drop for Renderer {
    fn drop(&mut self) {
        close(self);
    }
}

fn close(r: &mut Renderer) {
    r.quit();
    match core::mem::replace(&mut r.child, Child::InProcess) {
        Child::InProcess => {
            sys::close_inproc(r.sock, r.wfd);
            r.fb = Framebuffer::None;
        }
        Child::Process(process) => {
            let map = match core::mem::replace(&mut r.fb, Framebuffer::None) {
                Framebuffer::Mapped(map) => Some(map),
                _ => None,
            };
            sys::close_process(process, r.sock, r.wfd, map);
        }
    }
}

#[derive(Clone, Copy)]
pub struct Remote {
    sock: c_int,
    process: Option<sys::Process>,
}

impl Remote {
    pub fn pid(&self) -> c_int {
        self.process.as_ref().map_or(-1, sys::pid)
    }

    pub fn interrupt(&self) {
        sys::interrupt(self.sock);
    }

    pub fn terminate(&self) {
        match &self.process {
            Some(process) => sys::terminate(process),
            None => self.interrupt(),
        }
    }
}

impl Renderer {
    pub fn remote(&self) -> Remote {
        Remote {
            sock: self.sock,
            process: match &self.child {
                Child::Process(process) => Some(*process),
                Child::InProcess => None,
            },
        }
    }

    pub(crate) fn framebuffer(&self) -> &[u8] {
        let pixels = self.fb.pixels();
        if pixels.is_null() {
            return &[];
        }
        // SAFETY: every framebuffer holds max_w * max_h * 4 bytes, the map_size, and stays mapped until the renderer that owns it is dropped.
        unsafe { core::slice::from_raw_parts(pixels, self.map_size()) }
    }
}

pub fn self_pid() -> c_int {
    std::process::id() as c_int
}

static PRINT_COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn next_print_counter() -> u32 {
    PRINT_COUNTER.fetch_add(1, Ordering::SeqCst) + 1
}

pub struct ProcInfo {
    pub alive: bool,
    pub state: Option<&'static str>,
    pub rss_kb: Option<c_long>,
}

#[cfg(target_os = "linux")]
fn stat_line(pid: c_int, cap: usize) -> Option<Vec<u8>> {
    let bytes = std::fs::read(format!("/proc/{pid}/stat")).ok()?;
    let end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |i| i + 1);
    Some(bytes[..end.min(cap - 1)].to_vec())
}

#[cfg(target_os = "linux")]
fn stat_fields(line: &[u8]) -> Option<Vec<&[u8]>> {
    let close = line.iter().rposition(|&b| b == b')')?;
    if close + 1 >= line.len() {
        return None;
    }
    let rest = line.get(close + 2..).unwrap_or_default();
    Some(
        rest.split(|&b| b == b' ')
            .filter(|t| !t.is_empty())
            .collect(),
    )
}

#[cfg(target_os = "linux")]
fn atol(token: &[u8]) -> c_long {
    southstar_ipc::atol(token)
}

pub fn proc_info(pid: c_int) -> ProcInfo {
    let mut info = ProcInfo {
        alive: false,
        state: Some("running"),
        rss_kb: None,
    };
    if pid <= 0 {
        return info;
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(bytes) = std::fs::read(format!("/proc/{pid}/stat")) else {
            info.state = Some("terminated");
            return info;
        };
        let line = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
        let line = &line[..line.len().min(511)];
        if let Some(close) = line.iter().rposition(|&b| b == b')')
            && line.get(close + 1) == Some(&b' ')
            && let Some(&code) = line.get(close + 2)
        {
            info.state = Some(match code {
                b'S' | b'D' => "sleeping",
                b'T' | b't' => "stopped",
                b'Z' | b'X' => "terminated",
                _ => "running",
            });
        }
        if let Ok(statm) = std::fs::read_to_string(format!("/proc/{pid}/statm")) {
            let mut fields = statm.split_ascii_whitespace();
            if let (Some(_), Some(resident)) = (fields.next(), fields.next())
                && let Ok(resident) = resident.parse::<c_long>()
            {
                let page = sys::page_size();
                info.rss_kb = Some(resident * if page > 0 { page / 1024 } else { 4 });
            }
        }
        info.alive = true;
        info
    }
    #[cfg(not(target_os = "linux"))]
    {
        sys::proc_info(pid, &mut info);
        info
    }
}

#[cfg(target_os = "linux")]
pub fn clock_ticks() -> c_long {
    sys::clock_ticks()
}

pub fn proc_cpu(pid: c_int) -> f64 {
    if pid <= 0 {
        return -1.0;
    }
    #[cfg(target_os = "linux")]
    {
        let Some(line) = stat_line(pid, 1024) else {
            return -1.0;
        };
        let Some(fields) = stat_fields(&line) else {
            return -1.0;
        };
        let tick = sys::clock_ticks();
        let tick = if tick <= 0 { 100 } else { tick };
        let utime = fields.get(11).map_or(0, |t| atol(t));
        let stime = fields.get(12).map_or(0, |t| atol(t));
        (utime + stime) as f64 / tick as f64
    }
    #[cfg(not(target_os = "linux"))]
    {
        sys::proc_cpu(pid)
    }
}

pub fn proc_threads(pid: c_int) -> c_int {
    if pid <= 0 {
        return -1;
    }
    #[cfg(target_os = "linux")]
    {
        let Some(line) = stat_line(pid, 1024) else {
            return -1;
        };
        stat_fields(&line)
            .and_then(|fields| fields.get(17).map(|t| atol(t) as c_int))
            .unwrap_or(-1)
    }
    #[cfg(not(target_os = "linux"))]
    {
        sys::proc_threads(pid)
    }
}

#[cfg(unix)]
mod sys {
    use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
    use core::ptr;
    use std::ffi::{CStr, CString};
    use std::fs::File;
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::{Child, Framebuffer};
    use crate::client::Renderer;

    #[derive(Clone, Copy)]
    pub struct Process(c_int);

    pub struct Mapping {
        pixels: *mut u8,
        len: usize,
    }

    impl Mapping {
        fn new(fd: &OwnedFd, len: usize) -> Option<Mapping> {
            // SAFETY: a new shared mapping of len bytes at an address the kernel picks, over a
            // descriptor at least len bytes long; it overlaps no Rust-owned memory.
            let pixels = unsafe {
                mmap(
                    ptr::null_mut(),
                    len,
                    PROT_READ_WRITE,
                    MAP_SHARED,
                    fd.as_raw_fd(),
                    0,
                )
            };
            (pixels as isize != -1).then_some(Mapping {
                pixels: pixels.cast(),
                len,
            })
        }

        pub fn pixels(&self) -> *const u8 {
            self.pixels
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: pixels and len are exactly the mapping Mapping::new made, unmapped once
            // here; the Renderer that lends it out as a slice is being torn down.
            unsafe { munmap(self.pixels.cast(), self.len) };
        }
    }

    const SIGPIPE: c_int = 13;
    const SIGKILL: c_int = 9;
    const SIG_IGN: usize = 1;
    const AF_UNIX: c_int = 1;
    const SOCK_STREAM: c_int = 1;
    const F_SETFD: c_int = 2;
    const FD_CLOEXEC: c_int = 1;
    const PROT_READ_WRITE: c_int = 3;
    const MAP_SHARED: c_int = 1;
    const WNOHANG: c_int = 1;
    const SHUT_WR: c_int = 1;
    const SHUT_RDWR: c_int = 2;
    const O_RDWR: c_int = 2;
    const EINTR: i32 = 4;
    #[cfg(target_os = "linux")]
    const SC_PAGESIZE: c_int = 30;
    #[cfg(target_os = "linux")]
    const SC_CLK_TCK: c_int = 2;
    #[cfg(target_os = "linux")]
    const OPEN_MAX_NAME: c_int = 4;
    #[cfg(not(target_os = "linux"))]
    const OPEN_MAX_NAME: c_int = 5;
    #[cfg(target_os = "linux")]
    const CREATE_EXCLUSIVE: c_int = 0x40 | 0x80;
    #[cfg(not(target_os = "linux"))]
    const CREATE_EXCLUSIVE: c_int = 0x200 | 0x800;
    #[cfg(target_os = "linux")]
    const O_CLOEXEC: c_int = 0x80000;
    #[cfg(target_vendor = "apple")]
    const O_CLOEXEC: c_int = 0x100_0000;
    #[cfg(target_os = "freebsd")]
    const O_CLOEXEC: c_int = 0x10_0000;
    #[cfg(target_os = "netbsd")]
    const O_CLOEXEC: c_int = 0x40_0000;
    #[cfg(target_os = "openbsd")]
    const O_CLOEXEC: c_int = 0x1_0000;
    #[cfg(not(any(
        target_os = "linux",
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    const O_CLOEXEC: c_int = 0;
    const EINVAL: i32 = 22;
    #[cfg(target_os = "linux")]
    const MFD_CLOEXEC: c_uint = 1;
    #[cfg(target_os = "linux")]
    const MFD_ALLOW_SEALING: c_uint = 2;
    #[cfg(target_os = "linux")]
    const F_ADD_SEALS: c_int = 1033;
    #[cfg(target_os = "linux")]
    const F_SEAL_SEAL: c_int = 1;
    #[cfg(target_os = "linux")]
    const F_SEAL_SHRINK: c_int = 2;
    #[cfg(target_os = "linux")]
    const F_SEAL_GROW: c_int = 4;
    #[cfg(target_os = "linux")]
    const SYS_CLOSE_RANGE: c_long = 436;
    #[cfg(target_os = "freebsd")]
    const SYS_CLOSE_RANGE: c_long = 575;

    unsafe extern "C" {
        fn fork() -> c_int;
        fn execv(path: *const c_char, argv: *const *const c_char) -> c_int;
        fn _exit(status: c_int) -> !;
        fn dup2(old: c_int, new: c_int) -> c_int;
        fn close(fd: c_int) -> c_int;
        fn socketpair(domain: c_int, kind: c_int, protocol: c_int, sv: *mut c_int) -> c_int;
        fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
        fn signal(signum: c_int, handler: usize) -> usize;
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
        fn memfd_create(name: *const c_char, flags: c_uint) -> c_int;
        fn shm_open(name: *const c_char, oflag: c_int, ...) -> c_int;
        fn shm_unlink(name: *const c_char) -> c_int;
        fn kill(pid: c_int, signal: c_int) -> c_int;
        fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
        fn shutdown(fd: c_int, how: c_int) -> c_int;
        fn sysconf(name: c_int) -> c_long;
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        fn syscall(number: c_long, ...) -> c_long;
    }

    fn sysconf_value(name: c_int) -> c_long {
        // SAFETY: sysconf takes a plain integer and touches no memory.
        unsafe { sysconf(name) }
    }

    #[cfg(target_os = "linux")]
    pub fn page_size() -> c_long {
        sysconf_value(SC_PAGESIZE)
    }

    #[cfg(target_os = "linux")]
    pub fn clock_ticks() -> c_long {
        sysconf_value(SC_CLK_TCK)
    }

    fn errno() -> i32 {
        io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }

    fn close_fd(fd: c_int) {
        // SAFETY: every caller passes a descriptor this module opened and owns, and never uses
        // it again; close touches no memory.
        unsafe { close(fd) };
    }

    fn set_cloexec(fd: c_int) {
        // SAFETY: F_SETFD takes an int argument and touches no memory.
        unsafe { fcntl(fd, F_SETFD, FD_CLOEXEC) };
    }

    fn shutdown_socket(fd: c_int, how: c_int) {
        // SAFETY: shutdown takes plain integers and touches no memory.
        unsafe { shutdown(fd, how) };
    }

    fn send_signal(pid: c_int, signal: c_int) -> c_int {
        // SAFETY: kill takes plain integers and touches no memory.
        unsafe { kill(pid, signal) }
    }

    fn ignore_sigpipe() {
        // SAFETY: SIG_IGN is a valid disposition and installs no handler code.
        unsafe { signal(SIGPIPE, SIG_IGN) };
    }

    fn socket_pair() -> Option<[c_int; 2]> {
        let mut sv = [0 as c_int; 2];
        // SAFETY: sv has room for the two descriptors socketpair writes.
        let rc = unsafe { socketpair(AF_UNIX, SOCK_STREAM, 0, sv.as_mut_ptr()) };
        (rc == 0).then_some(sv)
    }

    #[cfg(target_os = "linux")]
    fn memfd() -> Option<OwnedFd> {
        // SAFETY: the name is a NUL-terminated literal and the flags are valid memfd flags.
        let fd = unsafe { memfd_create(c"nshttp-fb".as_ptr(), MFD_CLOEXEC | MFD_ALLOW_SEALING) };
        // SAFETY: a non-negative fd was just created here and has no other owner.
        (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
    }

    #[cfg(not(target_os = "linux"))]
    fn memfd() -> Option<OwnedFd> {
        None
    }

    fn shm_create(name: &CStr, flags: c_int) -> Option<OwnedFd> {
        // SAFETY: name is NUL-terminated and the mode is the unsigned int shm_open reads when
        // O_CREAT is set.
        let fd = unsafe { shm_open(name.as_ptr(), flags, 0o600 as c_uint) };
        // SAFETY: a non-negative fd was just opened here and has no other owner.
        (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn shm_fd() -> Option<OwnedFd> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let name =
            CString::new(format!("/nshttp-{}-{counter}", std::process::id())).unwrap_or_default();
        let mut fd = shm_create(&name, CREATE_EXCLUSIVE | O_RDWR | O_CLOEXEC);
        if fd.is_none() && errno() == EINVAL {
            fd = shm_create(&name, CREATE_EXCLUSIVE | O_RDWR);
            if let Some(fd) = &fd {
                set_cloexec(fd.as_raw_fd());
            }
        }
        if fd.is_some() {
            // SAFETY: name is NUL-terminated; unlinking only drops the name, the fd stays open.
            unsafe { shm_unlink(name.as_ptr()) };
        }
        fd
    }

    fn open_fb_fd(size: usize) -> Option<OwnedFd> {
        let file = File::from(memfd().or_else(shm_fd)?);
        file.set_len(size as u64).ok()?;
        #[cfg(target_os = "linux")]
        seal_size(&file);
        Some(OwnedFd::from(file))
    }

    #[cfg(target_os = "linux")]
    fn seal_size(file: &File) {
        let seals = F_SEAL_SEAL | F_SEAL_SHRINK | F_SEAL_GROW;
        // SAFETY: file is the open memfd just created and sized; F_ADD_SEALS takes an int argument and touches no memory.
        unsafe { fcntl(file.as_raw_fd(), F_ADD_SEALS, seals) };
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn close_range_from_4() -> bool {
        // SAFETY: close_range takes three unsigned ints and touches no memory; in the forked
        // child every descriptor from 4 up is unwanted.
        let rc = unsafe { syscall(SYS_CLOSE_RANGE, 4 as c_uint, !0 as c_uint, 0 as c_uint) };
        rc == 0
    }

    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    fn close_range_from_4() -> bool {
        false
    }

    fn close_inherited_fds(max_fd: c_long) {
        if close_range_from_4() {
            return;
        }
        for fd in 4..max_fd {
            close_fd(fd as c_int);
        }
    }

    fn exec_child(sv: [c_int; 2], program: &CStr, argv: &[*const c_char], max_fd: c_long) -> ! {
        close_fd(sv[0]);
        if sv[1] != 3 {
            // SAFETY: dup2 takes two descriptors and touches no memory.
            unsafe { dup2(sv[1], 3) };
            close_fd(sv[1]);
        }
        close_inherited_fds(max_fd);
        // SAFETY: program and every argv entry are NUL-terminated strings the parent's frame
        // keeps alive, and argv ends with a null pointer.
        unsafe { execv(program.as_ptr(), argv.as_ptr()) };
        // SAFETY: _exit ends the forked child without running destructors or atexit handlers
        // that belong to the parent.
        unsafe { _exit(127) }
    }

    fn reap(pid: c_int, options: c_int) -> c_int {
        let mut status = 0;
        // SAFETY: status is a valid place for waitpid to store the exit status.
        unsafe { waitpid(pid, &mut status, options) }
    }

    fn wait_child(pid: c_int) {
        while reap(pid, 0) < 0 && errno() == EINTR {}
    }

    pub fn spawn(
        path: &[u8],
        max_w: c_int,
        max_h: c_int,
        shm: bool,
        private: bool,
    ) -> Option<Renderer> {
        ignore_sigpipe();
        let size = max_w as usize * max_h as usize * 4;
        let shared = if shm {
            let fd = open_fb_fd(size)?;
            let map = Mapping::new(&fd, size)?;
            Some((fd, map))
        } else {
            None
        };
        let max_fd = sysconf_value(OPEN_MAX_NAME);
        let max_fd = if !(0..=65536).contains(&max_fd) {
            65536
        } else {
            max_fd
        };
        let sv = socket_pair()?;
        set_cloexec(sv[0]);
        let program = CString::new(path).unwrap_or_default();
        let mut args = vec![
            program.clone(),
            CString::new(max_w.to_string()).unwrap_or_default(),
            CString::new(max_h.to_string()).unwrap_or_default(),
        ];
        if shm {
            args.push(c"shm".to_owned());
        }
        if private {
            args.push(c"private".to_owned());
        }
        let mut argv: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        argv.push(ptr::null());
        // SAFETY: before exec the child only runs exec_child, which calls async-signal-safe
        // functions (close, dup2, syscall, execv, _exit) and allocates nothing.
        let pid = unsafe { fork() };
        if pid < 0 {
            close_fd(sv[0]);
            close_fd(sv[1]);
            return None;
        }
        if pid == 0 {
            exec_child(sv, &program, &argv, max_fd);
        }
        close_fd(sv[1]);
        let fb = match shared {
            Some((fd, map)) => {
                southstar_ipc::send_fd(sv[0], fd.as_raw_fd());
                drop(fd);
                Framebuffer::Mapped(map)
            }
            None => Framebuffer::None,
        };
        let bufsz = (size + 65536).min(0x7fff_ffff);
        southstar_ipc::set_bufsize(sv[0], bufsz as c_int);
        southstar_ipc::set_read_timeout(sv[0], 30);
        Some(super::renderer(
            Child::Process(Process(pid)),
            sv[0],
            sv[0],
            fb,
            size,
            max_w,
            max_h,
        ))
    }

    pub fn inproc_channel() -> Option<(c_int, c_int, c_int, c_int)> {
        ignore_sigpipe();
        let sv = socket_pair()?;
        southstar_ipc::set_bufsize(sv[0], 1 << 20);
        southstar_ipc::set_bufsize(sv[1], 1 << 20);
        Some((sv[1], sv[1], sv[0], sv[0]))
    }

    pub fn close_channel(server_r: c_int, _server_w: c_int, client_r: c_int, _client_w: c_int) {
        close_fd(client_r);
        close_fd(server_r);
    }

    pub fn close_inproc(sock: c_int, _wfd: c_int) {
        shutdown_socket(sock, SHUT_RDWR);
        close_fd(sock);
    }

    pub fn close_process(process: Process, sock: c_int, _wfd: c_int, map: Option<Mapping>) {
        shutdown_socket(sock, SHUT_WR);
        let pid = process.0;
        let mut reaped = false;
        for _ in 0..200 {
            let w = reap(pid, WNOHANG);
            if w == pid || (w < 0 && errno() != EINTR) {
                reaped = true;
                break;
            }
            if w == 0 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if !reaped {
            send_signal(pid, SIGKILL);
            wait_child(pid);
        }
        close_fd(sock);
        drop(map);
    }

    pub fn interrupt(sock: c_int) {
        shutdown_socket(sock, SHUT_RDWR);
    }

    pub fn pid(process: &Process) -> c_int {
        process.0
    }

    pub fn terminate(process: &Process) {
        if process.0 > 0 {
            send_signal(process.0, SIGKILL);
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn proc_info(pid: c_int, info: &mut super::ProcInfo) {
        if send_signal(pid, 0) != 0 {
            info.state = Some("terminated");
        } else {
            info.alive = true;
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn proc_cpu(_pid: c_int) -> f64 {
        -1.0
    }

    #[cfg(not(target_os = "linux"))]
    pub fn proc_threads(_pid: c_int) -> c_int {
        -1
    }
}

#[cfg(windows)]
mod sys {
    use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
    use core::{mem, ptr};

    use super::{Child, Framebuffer};
    use crate::client::Renderer;

    type Handle = *mut c_void;

    #[derive(Clone, Copy)]
    pub struct Process(Handle);

    pub struct Mapping {
        pixels: *mut u8,
        mapping: Handle,
    }

    impl Mapping {
        fn new(size: usize) -> Option<Mapping> {
            let mut sa = SecurityAttributes::inheritable();
            // SAFETY: an anonymous, pagefile-backed mapping of size bytes; sa lives for the call
            // and a null name creates an unnamed object.
            let mapping = unsafe {
                CreateFileMappingA(
                    INVALID_HANDLE_VALUE,
                    &mut sa,
                    PAGE_READWRITE,
                    ((size as u64) >> 32) as u32,
                    (size & 0xffff_ffff) as u32,
                    ptr::null(),
                )
            };
            if mapping.is_null() {
                return None;
            }
            // SAFETY: mapping is the section just created, at least size bytes long; the view
            // lands at an address the system picks, overlapping no Rust-owned memory.
            let pixels = unsafe { MapViewOfFile(mapping, FILE_MAP_READ_WRITE, 0, 0, size) };
            if pixels.is_null() {
                close_all(&[mapping]);
                return None;
            }
            Some(Mapping {
                pixels: pixels.cast(),
                mapping,
            })
        }

        pub fn pixels(&self) -> *const u8 {
            self.pixels
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: pixels is the view Mapping::new mapped, unmapped once here; the Renderer
            // that lends it out as a slice is being torn down.
            unsafe { UnmapViewOfFile(self.pixels.cast()) };
            close_all(&[self.mapping]);
        }
    }

    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }

    impl SecurityAttributes {
        fn inheritable() -> SecurityAttributes {
            SecurityAttributes {
                length: mem::size_of::<SecurityAttributes>() as u32,
                descriptor: ptr::null_mut(),
                inherit: 1,
            }
        }
    }

    #[repr(C)]
    struct StartupInfo {
        cb: u32,
        reserved: *mut c_char,
        desktop: *mut c_char,
        title: *mut c_char,
        x: u32,
        y: u32,
        x_size: u32,
        y_size: u32,
        x_count_chars: u32,
        y_count_chars: u32,
        fill_attribute: u32,
        flags: u32,
        show_window: u16,
        reserved2_len: u16,
        reserved2: *mut u8,
        std_input: Handle,
        std_output: Handle,
        std_error: Handle,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: Handle,
        thread: Handle,
        process_id: u32,
        thread_id: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct MemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ThreadEntry {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_process_id: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }

    #[repr(C)]
    struct StartupInfoEx {
        startup_info: StartupInfo,
        attribute_list: *mut c_void,
    }

    const PAGE_READWRITE: u32 = 4;
    const FILE_MAP_READ_WRITE: u32 = 4 | 2;
    const HANDLE_FLAG_INHERIT: u32 = 1;
    const STARTF_USESTDHANDLES: u32 = 0x100;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
    const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x20002;
    const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const TH32CS_SNAPTHREAD: u32 = 0x4;
    const O_BINARY: c_int = 0x8000;

    unsafe extern "system" {
        fn CreateFileMappingA(
            file: Handle,
            attributes: *mut SecurityAttributes,
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
        fn CreatePipe(
            read: *mut Handle,
            write: *mut Handle,
            attributes: *mut SecurityAttributes,
            size: u32,
        ) -> i32;
        fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
        fn GetStdHandle(which: u32) -> Handle;
        fn CreateProcessA(
            application: *const c_char,
            command_line: *mut c_char,
            process_attributes: *mut c_void,
            thread_attributes: *mut c_void,
            inherit_handles: i32,
            creation_flags: u32,
            environment: *mut c_void,
            current_directory: *const c_char,
            startup_info: *mut StartupInfo,
            process_information: *mut ProcessInformation,
        ) -> i32;
        fn InitializeProcThreadAttributeList(
            list: *mut c_void,
            attribute_count: u32,
            flags: u32,
            size: *mut usize,
        ) -> i32;
        fn UpdateProcThreadAttribute(
            list: *mut c_void,
            flags: u32,
            attribute: usize,
            value: *const c_void,
            size: usize,
            previous_value: *mut c_void,
            return_size: *mut usize,
        ) -> i32;
        fn DeleteProcThreadAttributeList(list: *mut c_void);
        fn TerminateProcess(process: Handle, exit_code: u32) -> i32;
        fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
        fn GetProcessId(process: Handle) -> u32;
        fn CancelIoEx(handle: Handle, overlapped: *mut c_void) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn K32GetProcessMemoryInfo(process: Handle, counters: *mut MemoryCounters, cb: u32) -> i32;
        fn GetProcessTimes(
            process: Handle,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
        fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    }

    unsafe extern "C" {
        fn _open_osfhandle(handle: isize, flags: c_int) -> c_int;
        fn _get_osfhandle(fd: c_int) -> isize;
        fn _close(fd: c_int) -> c_int;
        fn _pipe(fds: *mut c_int, size: c_uint, mode: c_int) -> c_int;
    }

    fn dirname(path: &[u8]) -> Option<Vec<u8>> {
        let slash = path.iter().rposition(|&b| b == b'\\' || b == b'/')?;
        let n = match slash {
            0 => 1,
            2 if path.get(1) == Some(&b':') => 3,
            n => n,
        };
        Some(path[..n].to_vec())
    }

    fn close_all(handles: &[Handle]) {
        for &handle in handles {
            if !handle.is_null() {
                // SAFETY: every caller passes handles this module opened and owns, and never
                // uses them again.
                unsafe { CloseHandle(handle) };
            }
        }
    }

    fn close_fd(fd: c_int) {
        // SAFETY: every caller passes a CRT descriptor this module opened and owns, and never
        // uses it again; _close rejects an invalid one with an error.
        unsafe { _close(fd) };
    }

    fn crt_fd(handle: Handle) -> c_int {
        // SAFETY: handle is a pipe end this process owns; the returned descriptor takes it over.
        unsafe { _open_osfhandle(handle as isize, O_BINARY) }
    }

    fn crt_pipe() -> Option<[c_int; 2]> {
        let mut fds = [-1 as c_int; 2];
        // SAFETY: fds has room for the two descriptors _pipe writes.
        let rc = unsafe { _pipe(fds.as_mut_ptr(), 1 << 20, O_BINARY) };
        (rc == 0).then_some(fds)
    }

    fn create_pipe(read: &mut Handle, write: &mut Handle) -> bool {
        let mut sa = SecurityAttributes::inheritable();
        // SAFETY: read, write and sa are valid for the call; CreatePipe stores the two ends.
        unsafe { CreatePipe(read, write, &mut sa, 0) != 0 }
    }

    fn set_inherit(handle: Handle, inherit: bool) -> bool {
        let flags = if inherit { HANDLE_FLAG_INHERIT } else { 0 };
        // SAFETY: SetHandleInformation takes a handle and two flag words and touches no memory;
        // an invalid handle fails with an error.
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, flags) != 0 }
    }

    fn terminate_handle(process: Handle, code: u32) {
        // SAFETY: process is the process handle CreateProcessA returned; TerminateProcess
        // touches no memory and fails with an error on a stale handle.
        unsafe { TerminateProcess(process, code) };
    }

    fn open_process(pid: c_int) -> Option<Handle> {
        // SAFETY: OpenProcess takes plain values and returns a new handle the caller closes.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
        (!handle.is_null()).then_some(handle)
    }

    struct InheritedHandles {
        list: Vec<u8>,
        handles: Vec<Handle>,
    }

    impl InheritedHandles {
        fn new(handles: Vec<Handle>) -> Option<Self> {
            let mut size = 0usize;
            // SAFETY: a null list with a size out-parameter only asks for the buffer size.
            unsafe { InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size) };
            if size == 0 {
                return None;
            }
            let mut inherited = Self {
                list: vec![0u8; size],
                handles,
            };
            let list = inherited.list.as_mut_ptr().cast::<c_void>();
            // SAFETY: the buffer holds the size the first call reported for one attribute.
            if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
                return None;
            }
            let bytes = inherited.handles.len() * mem::size_of::<Handle>();
            // SAFETY: the list was initialized above and the handle array outlives it.
            let updated = unsafe {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                    inherited.handles.as_ptr().cast(),
                    bytes,
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if updated == 0 {
                // SAFETY: the list was initialized above and is deleted once here.
                unsafe { DeleteProcThreadAttributeList(list) };
                inherited.list.clear();
                return None;
            }
            Some(inherited)
        }

        fn list(&mut self) -> *mut c_void {
            self.list.as_mut_ptr().cast()
        }
    }

    impl Drop for InheritedHandles {
        fn drop(&mut self) {
            if self.list.is_empty() {
                return;
            }
            // SAFETY: the list was initialized by new and is deleted once.
            unsafe { DeleteProcThreadAttributeList(self.list()) };
        }
    }

    fn inheritable_stderr() -> Handle {
        // SAFETY: GetStdHandle has no preconditions.
        let handle = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return ptr::null_mut();
        }
        if !set_inherit(handle, true) {
            return ptr::null_mut();
        }
        handle
    }

    fn startup_info(
        stdin: Handle,
        stdout: Handle,
        stderr: Handle,
        attribute_list: *mut c_void,
    ) -> StartupInfoEx {
        StartupInfoEx {
            startup_info: StartupInfo {
                cb: mem::size_of::<StartupInfoEx>() as u32,
                reserved: ptr::null_mut(),
                desktop: ptr::null_mut(),
                title: ptr::null_mut(),
                x: 0,
                y: 0,
                x_size: 0,
                y_size: 0,
                x_count_chars: 0,
                y_count_chars: 0,
                fill_attribute: 0,
                flags: STARTF_USESTDHANDLES,
                show_window: 0,
                reserved2_len: 0,
                reserved2: ptr::null_mut(),
                std_input: stdin,
                std_output: stdout,
                std_error: stderr,
            },
            attribute_list,
        }
    }

    pub fn spawn(
        path: &[u8],
        max_w: c_int,
        max_h: c_int,
        shm: bool,
        private: bool,
    ) -> Option<Renderer> {
        let size = max_w as usize * max_h as usize * 4;
        let mapping = if shm { Some(Mapping::new(size)?) } else { None };
        let mut pipes: [Handle; 4] = [ptr::null_mut(); 4];
        let [child_in, parent_in, parent_out, child_out] = &mut pipes;
        let piped = create_pipe(child_in, parent_in)
            && set_inherit(*parent_in, false)
            && create_pipe(parent_out, child_out)
            && set_inherit(*parent_out, false);
        let [child_in, parent_in, parent_out, child_out] = pipes;
        if !piped {
            close_all(&pipes);
            return None;
        }
        let private_arg = if private { " private" } else { "" };
        let mut command = vec![b'"'];
        command.extend_from_slice(path);
        command.extend_from_slice(b"\"");
        let tail = match &mapping {
            Some(map) => format!(
                " {max_w} {max_h} shm {}{private_arg}",
                map.mapping as usize as u64
            ),
            None => format!(" {max_w} {max_h}{private_arg}"),
        };
        command.extend_from_slice(tail.as_bytes());
        if command.len() >= 32767 || command.contains(&0) {
            close_all(&pipes);
            return None;
        }
        command.push(0);
        let mut program = path.to_vec();
        program.push(0);
        let workdir = dirname(path).map(|mut d| {
            d.push(0);
            d
        });
        let stderr = inheritable_stderr();
        let mut handles = vec![child_in, child_out];
        if let Some(map) = &mapping {
            handles.push(map.mapping);
        }
        if !stderr.is_null() {
            handles.push(stderr);
        }
        let Some(mut inherited) = InheritedHandles::new(handles) else {
            close_all(&pipes);
            return None;
        };
        let mut startup = startup_info(child_in, child_out, stderr, inherited.list());
        let mut info = ProcessInformation {
            process: ptr::null_mut(),
            thread: ptr::null_mut(),
            process_id: 0,
            thread_id: 0,
        };
        // SAFETY: program, command and workdir are NUL-terminated buffers that outlive the call,
        // command is writable as CreateProcessA requires, startup carries an initialized
        // attribute list whose handle array is alive, and info is a valid out-parameter.
        let ok = unsafe {
            CreateProcessA(
                program.as_ptr().cast(),
                command.as_mut_ptr().cast(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                ptr::null_mut(),
                workdir.as_ref().map_or(ptr::null(), |d| d.as_ptr().cast()),
                (&raw mut startup).cast(),
                &mut info,
            )
        };
        drop(inherited);
        close_all(&[child_in, child_out]);
        if ok == 0 {
            close_all(&[parent_in, parent_out]);
            return None;
        }
        close_all(&[info.thread]);
        let wfd = crt_fd(parent_in);
        let sock = crt_fd(parent_out);
        let fb = mapping.map_or(Framebuffer::None, Framebuffer::Mapped);
        Some(super::renderer(
            Child::Process(Process(info.process)),
            sock,
            wfd,
            fb,
            size,
            max_w,
            max_h,
        ))
    }

    pub fn inproc_channel() -> Option<(c_int, c_int, c_int, c_int)> {
        let req = crt_pipe()?;
        let Some(resp) = crt_pipe() else {
            close_fd(req[0]);
            close_fd(req[1]);
            return None;
        };
        Some((req[0], resp[1], resp[0], req[1]))
    }

    pub fn close_channel(server_r: c_int, server_w: c_int, client_r: c_int, client_w: c_int) {
        close_fd(server_r);
        close_fd(client_w);
        close_fd(client_r);
        close_fd(server_w);
    }

    pub fn close_inproc(sock: c_int, wfd: c_int) {
        close_fd(wfd);
        close_fd(sock);
    }

    pub fn close_process(process: Process, sock: c_int, wfd: c_int, map: Option<Mapping>) {
        close_fd(wfd);
        // SAFETY: process.0 is the process handle CreateProcessA returned, still open here;
        // waiting touches no memory.
        unsafe { WaitForSingleObject(process.0, 2000) };
        terminate_handle(process.0, 0);
        close_all(&[process.0]);
        close_fd(sock);
        drop(map);
    }

    pub fn interrupt(sock: c_int) {
        // SAFETY: _get_osfhandle takes a descriptor and reports -1 for an invalid one.
        let handle = unsafe { _get_osfhandle(sock) };
        if handle != -1 {
            // SAFETY: handle is the live pipe handle behind sock; a null OVERLAPPED cancels all
            // of its pending I/O.
            unsafe { CancelIoEx(handle as Handle, ptr::null_mut()) };
        }
    }

    pub fn pid(process: &Process) -> c_int {
        if process.0.is_null() {
            -1
        } else {
            // SAFETY: GetProcessId takes a handle and touches no memory; it returns 0 on failure.
            unsafe { GetProcessId(process.0) as c_int }
        }
    }

    pub fn terminate(process: &Process) {
        if !process.0.is_null() {
            terminate_handle(process.0, 1);
        }
    }

    pub fn proc_info(pid: c_int, info: &mut super::ProcInfo) {
        let Some(h) = open_process(pid) else {
            info.state = Some("terminated");
            return;
        };
        let mut code = 0u32;
        // SAFETY: h is open and code is a valid out-parameter.
        let alive = unsafe { GetExitCodeProcess(h, &mut code) } != 0 && code == STILL_ACTIVE;
        let mut counters = MemoryCounters::default();
        let cb = mem::size_of::<MemoryCounters>() as u32;
        // SAFETY: h is open and counters is a writable PROCESS_MEMORY_COUNTERS of cb bytes.
        if unsafe { K32GetProcessMemoryInfo(h, &mut counters, cb) } != 0 {
            info.rss_kb = Some((counters.working_set_size / 1024) as c_long);
        }
        close_all(&[h]);
        if !alive {
            info.state = Some("terminated");
        }
        info.alive = alive;
    }

    pub fn proc_cpu(pid: c_int) -> f64 {
        let Some(h) = open_process(pid) else {
            return -1.0;
        };
        let (mut created, mut exited, mut kernel, mut user) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        let mut seconds = -1.0;
        // SAFETY: h is open and the four FILETIMEs are valid out-parameters.
        if unsafe { GetProcessTimes(h, &mut created, &mut exited, &mut kernel, &mut user) } != 0 {
            let k = (u64::from(kernel.high) << 32) | u64::from(kernel.low);
            let u = (u64::from(user.high) << 32) | u64::from(user.low);
            seconds = (k + u) as f64 / 1e7;
        }
        close_all(&[h]);
        seconds
    }

    fn next_thread(snapshot: Handle, entry: &mut ThreadEntry, first: bool) -> bool {
        let found = if first {
            // SAFETY: snapshot is an open thread snapshot and entry a THREADENTRY32 whose size
            // field is set.
            unsafe { Thread32First(snapshot, entry) }
        } else {
            // SAFETY: snapshot is an open thread snapshot and entry a THREADENTRY32 whose size
            // field is set.
            unsafe { Thread32Next(snapshot, entry) }
        };
        found != 0
    }

    pub fn proc_threads(pid: c_int) -> c_int {
        // SAFETY: CreateToolhelp32Snapshot takes plain values and returns a new handle or
        // INVALID_HANDLE_VALUE.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return -1;
        }
        let mut entry = ThreadEntry {
            size: mem::size_of::<ThreadEntry>() as u32,
            ..ThreadEntry::default()
        };
        let mut count = 0;
        let mut more = next_thread(snapshot, &mut entry, true);
        while more {
            if entry.owner_process_id == pid as u32 {
                count += 1;
            }
            more = next_thread(snapshot, &mut entry, false);
        }
        close_all(&[snapshot]);
        count
    }
}
