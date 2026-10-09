//! Southstar — TCP sockets the standard library cannot open: a non-blocking connect that polls for cancellation, poll() over several sockets, and the wake pair an I/O thread sleeps on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_short};
use std::io;
use std::net::{SocketAddr, TcpStream};

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, RawSocket};

#[cfg(unix)]
pub type Raw = RawFd;
#[cfg(windows)]
pub type Raw = RawSocket;

#[cfg(target_os = "linux")]
const AF_INET6: c_int = 10;
#[cfg(windows)]
const AF_INET6: c_int = 23;
#[cfg(target_os = "macos")]
const AF_INET6: c_int = 30;
#[cfg(target_os = "freebsd")]
const AF_INET6: c_int = 28;
#[cfg(target_os = "netbsd")]
const AF_INET6: c_int = 24;
const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const IPPROTO_TCP: c_int = 6;

#[cfg(target_os = "linux")]
const SOCK_CLOEXEC: c_int = 0o2_000_000;
#[cfg(any(target_os = "freebsd", target_os = "netbsd"))]
const SOCK_CLOEXEC: c_int = 0x1000_0000;
#[cfg(any(target_os = "macos", windows))]
const SOCK_CLOEXEC: c_int = 0;

#[cfg(unix)]
pub const POLLIN: c_short = 0x1;
#[cfg(unix)]
pub const POLLOUT: c_short = 0x4;
#[cfg(unix)]
pub const POLLERR: c_short = 0x8;
#[cfg(unix)]
pub const POLLHUP: c_short = 0x10;
#[cfg(windows)]
pub const POLLIN: c_short = 0x100;
#[cfg(windows)]
pub const POLLOUT: c_short = 0x10;
#[cfg(windows)]
pub const POLLERR: c_short = 0x1;
#[cfg(windows)]
pub const POLLHUP: c_short = 0x2;

#[cfg(target_os = "linux")]
const EINPROGRESS: i32 = 115;
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
const EINPROGRESS: i32 = 36;
#[cfg(target_os = "linux")]
const EAGAIN: i32 = 11;
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
const EAGAIN: i32 = 35;
#[cfg(unix)]
const EINTR: i32 = 4;
#[cfg(windows)]
const WSAEINTR: i32 = 10004;
#[cfg(windows)]
const WSAEWOULDBLOCK: i32 = 10035;
#[cfg(windows)]
const WSAEINPROGRESS: i32 = 10036;
#[cfg(windows)]
const WSAEALREADY: i32 = 10037;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    pub fd: Raw,
    pub events: c_short,
    pub revents: c_short,
}

#[cfg(unix)]
#[repr(C)]
struct PollFdC {
    fd: c_int,
    events: c_short,
    revents: c_short,
}

#[cfg(target_os = "linux")]
type Nfds = core::ffi::c_ulong;
#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
type Nfds = core::ffi::c_uint;

#[cfg(unix)]
unsafe extern "C" {
    fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
    fn connect(fd: c_int, addr: *const u8, len: u32) -> c_int;
    fn poll(fds: *mut PollFdC, nfds: Nfds, timeout: c_int) -> c_int;
}

#[cfg(windows)]
#[link(name = "ws2_32")]
unsafe extern "system" {
    fn WSASocketW(
        af: c_int,
        kind: c_int,
        protocol: c_int,
        info: *mut u8,
        group: u32,
        flags: u32,
    ) -> usize;
    fn connect(s: usize, addr: *const u8, len: c_int) -> c_int;
    fn WSAPoll(fds: *mut PollFd, nfds: core::ffi::c_ulong, timeout: c_int) -> c_int;
    fn WSAStartup(version: u16, data: *mut u8) -> c_int;
}

#[cfg(windows)]
pub fn init() -> bool {
    use std::sync::OnceLock;
    static READY: OnceLock<bool> = OnceLock::new();
    *READY.get_or_init(|| {
        let mut data = [0u8; 512];
        unsafe { WSAStartup(0x0202, data.as_mut_ptr()) == 0 }
    })
}

#[cfg(unix)]
pub fn init() -> bool {
    true
}

fn last_error() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(unix)]
pub fn retryable(e: i32) -> bool {
    e == EAGAIN || e == EINTR
}

#[cfg(windows)]
pub fn retryable(e: i32) -> bool {
    e == WSAEWOULDBLOCK || e == WSAEINTR || e == WSAEINPROGRESS
}

#[cfg(unix)]
pub fn interrupted(e: i32) -> bool {
    e == EINTR
}

#[cfg(windows)]
pub fn interrupted(e: i32) -> bool {
    e == WSAEINTR
}

#[cfg(unix)]
fn connect_pending(e: i32) -> bool {
    e == EINPROGRESS
}

#[cfg(windows)]
fn connect_pending(e: i32) -> bool {
    e == WSAEINPROGRESS || e == WSAEWOULDBLOCK || e == WSAEALREADY
}

#[cfg(any(target_os = "linux", windows))]
fn family_bytes(family: c_int, len: usize) -> [u8; 2] {
    let _ = len;
    (family as u16).to_ne_bytes()
}

#[cfg(any(target_os = "macos", target_os = "freebsd", target_os = "netbsd"))]
fn family_bytes(family: c_int, len: usize) -> [u8; 2] {
    [len as u8, family as u8]
}

pub fn encode_addr(addr: &SocketAddr) -> (Vec<u8>, c_int) {
    match addr {
        SocketAddr::V4(a) => {
            let mut out = family_bytes(AF_INET, 16).to_vec();
            out.extend_from_slice(&a.port().to_be_bytes());
            out.extend_from_slice(&a.ip().octets());
            out.extend_from_slice(&[0; 8]);
            (out, AF_INET)
        }
        SocketAddr::V6(a) => {
            let mut out = family_bytes(AF_INET6, 28).to_vec();
            out.extend_from_slice(&a.port().to_be_bytes());
            out.extend_from_slice(&a.flowinfo().to_ne_bytes());
            out.extend_from_slice(&a.ip().octets());
            out.extend_from_slice(&a.scope_id().to_ne_bytes());
            (out, AF_INET6)
        }
    }
}

#[cfg(unix)]
fn new_socket(family: c_int) -> Option<TcpStream> {
    let fd = unsafe { socket(family, SOCK_STREAM | SOCK_CLOEXEC, IPPROTO_TCP) };
    (fd >= 0).then(|| unsafe { TcpStream::from_raw_fd(fd) })
}

#[cfg(windows)]
fn new_socket(family: c_int) -> Option<TcpStream> {
    let _ = SOCK_CLOEXEC;
    let s = unsafe {
        WSASocketW(
            family,
            SOCK_STREAM,
            IPPROTO_TCP,
            core::ptr::null_mut(),
            0,
            0x81,
        )
    };
    (s != usize::MAX).then(|| unsafe { TcpStream::from_raw_socket(s as RawSocket) })
}

#[cfg(unix)]
fn raw_connect(stream: &TcpStream, addr: &[u8]) -> c_int {
    unsafe { connect(stream.as_raw_fd(), addr.as_ptr(), addr.len() as u32) }
}

#[cfg(windows)]
fn raw_connect(stream: &TcpStream, addr: &[u8]) -> c_int {
    unsafe {
        connect(
            stream.as_raw_socket() as usize,
            addr.as_ptr(),
            addr.len() as c_int,
        )
    }
}

pub fn raw(stream: &TcpStream) -> Raw {
    #[cfg(unix)]
    {
        stream.as_raw_fd()
    }
    #[cfg(windows)]
    {
        stream.as_raw_socket()
    }
}

#[cfg(unix)]
pub fn poll_fds(fds: &mut [PollFd], timeout_ms: i32) -> Result<usize, i32> {
    let mut c: Vec<PollFdC> = fds
        .iter()
        .map(|f| PollFdC {
            fd: f.fd,
            events: f.events,
            revents: 0,
        })
        .collect();
    let r = unsafe { poll(c.as_mut_ptr(), c.len() as Nfds, timeout_ms) };
    if r < 0 {
        return Err(last_error());
    }
    for (f, p) in fds.iter_mut().zip(&c) {
        f.revents = p.revents;
    }
    Ok(r as usize)
}

#[cfg(windows)]
pub fn poll_fds(fds: &mut [PollFd], timeout_ms: i32) -> Result<usize, i32> {
    let r = unsafe {
        WSAPoll(
            fds.as_mut_ptr(),
            fds.len() as core::ffi::c_ulong,
            timeout_ms,
        )
    };
    if r < 0 {
        return Err(last_error());
    }
    Ok(r as usize)
}

pub enum ConnectError {
    Cancelled,
    TimedOut,
    Failed,
}

pub fn connect_addr(
    addr: &SocketAddr,
    remaining_ms: &dyn Fn() -> i64,
    cancelled: &dyn Fn() -> bool,
) -> Result<TcpStream, ConnectError> {
    let (sockaddr, family) = encode_addr(addr);
    let stream = new_socket(family).ok_or(ConnectError::Failed)?;
    stream
        .set_nonblocking(true)
        .map_err(|_| ConnectError::Failed)?;
    let _ = stream.set_nodelay(true);
    if raw_connect(&stream, &sockaddr) != 0 {
        if !connect_pending(last_error()) {
            return Err(ConnectError::Failed);
        }
        loop {
            if cancelled() {
                return Err(ConnectError::Cancelled);
            }
            let remain = remaining_ms();
            if remain <= 0 {
                return Err(ConnectError::TimedOut);
            }
            let mut fds = [PollFd {
                fd: raw(&stream),
                events: POLLOUT,
                revents: 0,
            }];
            match poll_fds(&mut fds, remain.min(500) as i32) {
                Err(e) if interrupted(e) => continue,
                Err(_) => return Err(ConnectError::Failed),
                Ok(0) => continue,
                Ok(_) => {}
            }
            match stream.take_error() {
                Ok(None) => break,
                _ => return Err(ConnectError::Failed),
            }
        }
    }
    Ok(stream)
}

pub fn wait(fd: Raw, events: c_short, timeout_ms: i32) -> bool {
    let mut fds = [PollFd {
        fd,
        events,
        revents: 0,
    }];
    matches!(poll_fds(&mut fds, timeout_ms), Ok(n) if n > 0)
}

pub struct Wake {
    #[cfg(unix)]
    reader: std::os::unix::net::UnixStream,
    #[cfg(unix)]
    writer: std::os::unix::net::UnixStream,
    #[cfg(windows)]
    reader: std::net::UdpSocket,
    #[cfg(windows)]
    writer: std::net::UdpSocket,
}

impl Wake {
    #[cfg(unix)]
    pub fn new() -> Option<Wake> {
        let (reader, writer) = std::os::unix::net::UnixStream::pair().ok()?;
        reader.set_nonblocking(true).ok()?;
        writer.set_nonblocking(true).ok()?;
        Some(Wake { reader, writer })
    }

    #[cfg(windows)]
    pub fn new() -> Option<Wake> {
        let reader = std::net::UdpSocket::bind("127.0.0.1:0").ok()?;
        let writer = std::net::UdpSocket::bind("127.0.0.1:0").ok()?;
        writer.connect(reader.local_addr().ok()?).ok()?;
        reader.set_nonblocking(true).ok()?;
        writer.set_nonblocking(true).ok()?;
        Some(Wake { reader, writer })
    }

    pub fn raw(&self) -> Raw {
        #[cfg(unix)]
        {
            self.reader.as_raw_fd()
        }
        #[cfg(windows)]
        {
            self.reader.as_raw_socket()
        }
    }

    pub fn signal(&self) {
        #[cfg(unix)]
        {
            use std::io::Write;
            let _ = (&self.writer).write(b"x");
        }
        #[cfg(windows)]
        {
            let _ = self.writer.send(b"x");
        }
    }

    pub fn drain(&self) {
        let mut buf = [0u8; 256];
        #[cfg(unix)]
        {
            use std::io::Read;
            while matches!((&self.reader).read(&mut buf), Ok(n) if n > 0) {}
        }
        #[cfg(windows)]
        {
            while matches!(self.reader.recv(&mut buf), Ok(n) if n > 0) {}
        }
    }
}
