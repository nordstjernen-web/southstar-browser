//! Southstar — the C ABI of the renderer protocol's framing, as declared in src/ipc_http.h, and the descriptor calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_void};
use core::ptr;
use std::io;

use crate::{Conn, Head};

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

#[cfg(unix)]
mod sys {
    use core::ffi::{c_int, c_long, c_void};
    use core::mem::{ManuallyDrop, size_of};
    use core::ptr;
    use std::io;
    use std::os::unix::io::FromRawFd;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    unsafe extern "C" {
        fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
        fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        fn setsockopt(
            fd: c_int,
            level: c_int,
            name: c_int,
            value: *const c_void,
            len: u32,
        ) -> c_int;
        fn sendmsg(fd: c_int, msg: *const Msghdr, flags: c_int) -> isize;
        fn recvmsg(fd: c_int, msg: *mut Msghdr, flags: c_int) -> isize;
        fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    }

    #[repr(C)]
    struct Iovec {
        base: *mut c_void,
        len: usize,
    }

    #[cfg(target_os = "linux")]
    #[repr(C)]
    struct Msghdr {
        name: *mut c_void,
        namelen: u32,
        iov: *mut Iovec,
        iovlen: usize,
        control: *mut c_void,
        controllen: usize,
        flags: c_int,
    }

    #[cfg(not(target_os = "linux"))]
    #[repr(C)]
    struct Msghdr {
        name: *mut c_void,
        namelen: u32,
        iov: *mut Iovec,
        iovlen: c_int,
        control: *mut c_void,
        controllen: u32,
        flags: c_int,
    }

    #[cfg(target_os = "linux")]
    type CmsgLen = usize;
    #[cfg(not(target_os = "linux"))]
    type CmsgLen = u32;

    #[cfg(target_os = "linux")]
    const SOCKET_LEVEL: c_int = 1;
    #[cfg(target_os = "linux")]
    const SEND_BUFFER: c_int = 7;
    #[cfg(target_os = "linux")]
    const RECEIVE_BUFFER: c_int = 8;
    #[cfg(target_os = "linux")]
    const FORCED_BUFFERS: (c_int, c_int) = (32, 33);
    #[cfg(target_os = "linux")]
    const RECEIVE_CLOEXEC: c_int = 0x4000_0000;

    #[cfg(not(target_os = "linux"))]
    const SOCKET_LEVEL: c_int = 0xffff;
    #[cfg(not(target_os = "linux"))]
    const SEND_BUFFER: c_int = 0x1001;
    #[cfg(not(target_os = "linux"))]
    const RECEIVE_BUFFER: c_int = 0x1002;
    #[cfg(not(target_os = "linux"))]
    const FORCED_BUFFERS: (c_int, c_int) = (SEND_BUFFER, RECEIVE_BUFFER);
    #[cfg(target_os = "freebsd")]
    const RECEIVE_CLOEXEC: c_int = 0x0004_0000;
    #[cfg(any(target_os = "netbsd", target_os = "openbsd"))]
    const RECEIVE_CLOEXEC: c_int = 0x0800;
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    const RECEIVE_CLOEXEC: c_int = 0;

    const RIGHTS: c_int = 1;
    const SET_FD_FLAGS: c_int = 2;
    const CLOSE_ON_EXEC: c_int = 1;

    const fn cmsg_align(n: usize) -> usize {
        let align = if cfg!(target_vendor = "apple") {
            4
        } else if cfg!(target_os = "linux") {
            size_of::<usize>()
        } else {
            size_of::<c_long>()
        };
        (n + align - 1) & !(align - 1)
    }

    const CMSG_HEADER: usize = size_of::<CmsgLen>() + 2 * size_of::<c_int>();
    const CMSG_DATA: usize = cmsg_align(CMSG_HEADER);
    const CMSG_LEN: usize = CMSG_DATA + size_of::<c_int>();
    const CMSG_SPACE: usize = CMSG_DATA + cmsg_align(size_of::<c_int>());

    #[repr(C, align(8))]
    struct Control([u8; CMSG_SPACE]);

    fn last_error<T>(result: isize, ok: impl FnOnce(usize) -> T) -> io::Result<T> {
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ok(result as usize))
        }
    }

    pub(super) fn read_fd(fd: c_int, buf: &mut [u8]) -> io::Result<usize> {
        let n = unsafe { read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        last_error(n, |n| n)
    }

    pub(super) fn write_fd(fd: c_int, buf: &[u8]) -> io::Result<usize> {
        let n = unsafe { write(fd, buf.as_ptr().cast(), buf.len()) };
        last_error(n, |n| n)
    }

    fn set_int_option(fd: c_int, name: c_int, value: c_int) -> bool {
        let value_ptr = (&raw const value).cast();
        unsafe { setsockopt(fd, SOCKET_LEVEL, name, value_ptr, size_of::<c_int>() as u32) == 0 }
    }

    pub(super) fn set_bufsize(fd: c_int, bytes: c_int) {
        if !set_int_option(fd, FORCED_BUFFERS.0, bytes) {
            set_int_option(fd, SEND_BUFFER, bytes);
        }
        if !set_int_option(fd, FORCED_BUFFERS.1, bytes) {
            set_int_option(fd, RECEIVE_BUFFER, bytes);
        }
    }

    pub(super) fn set_read_timeout(fd: c_int, seconds: c_int) {
        if fd < 0 || (seconds < 0 && cfg!(not(target_os = "linux"))) {
            return;
        }
        let socket = ManuallyDrop::new(unsafe { UnixStream::from_raw_fd(fd) });
        let timeout = (seconds > 0).then(|| Duration::from_secs(seconds as u64));
        let _ = socket.set_read_timeout(timeout);
    }

    fn message(iov: &mut Iovec, control: &mut Control) -> Msghdr {
        Msghdr {
            name: ptr::null_mut(),
            namelen: 0,
            iov,
            iovlen: 1,
            control: control.0.as_mut_ptr().cast(),
            controllen: CMSG_SPACE as _,
            flags: 0,
        }
    }

    pub(super) fn send_fd(sock: c_int, fd: c_int) -> bool {
        let mut dummy = b'F';
        let mut iov = Iovec {
            base: (&raw mut dummy).cast(),
            len: 1,
        };
        let mut control = Control([0; CMSG_SPACE]);
        let len = CMSG_LEN as CmsgLen;
        let level_at = size_of::<CmsgLen>();
        let kind_at = level_at + size_of::<c_int>();
        control.0[..level_at].copy_from_slice(&len.to_ne_bytes());
        control.0[level_at..kind_at].copy_from_slice(&SOCKET_LEVEL.to_ne_bytes());
        control.0[kind_at..CMSG_HEADER].copy_from_slice(&RIGHTS.to_ne_bytes());
        control.0[CMSG_DATA..CMSG_LEN].copy_from_slice(&fd.to_ne_bytes());
        let msg = message(&mut iov, &mut control);
        loop {
            if unsafe { sendmsg(sock, &msg, 0) } >= 0 {
                return true;
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return false;
            }
        }
    }

    pub(super) fn recv_fd(sock: c_int) -> c_int {
        let mut dummy = 0u8;
        let mut iov = Iovec {
            base: (&raw mut dummy).cast(),
            len: 1,
        };
        let mut control = Control([0; CMSG_SPACE]);
        let mut msg = message(&mut iov, &mut control);
        let received = loop {
            let r = unsafe { recvmsg(sock, &mut msg, RECEIVE_CLOEXEC) };
            if r >= 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break r;
            }
        };
        if received <= 0 || (msg.controllen as usize) < CMSG_HEADER {
            return -1;
        }
        let int_at = |at: usize| {
            c_int::from_ne_bytes([
                control.0[at],
                control.0[at + 1],
                control.0[at + 2],
                control.0[at + 3],
            ])
        };
        let level_at = size_of::<CmsgLen>();
        if int_at(level_at) != SOCKET_LEVEL || int_at(level_at + size_of::<c_int>()) != RIGHTS {
            return -1;
        }
        let fd = int_at(CMSG_DATA);
        unsafe { fcntl(fd, SET_FD_FLAGS, CLOSE_ON_EXEC) };
        fd
    }
}

#[cfg(windows)]
mod sys {
    use core::ffi::{c_int, c_uint, c_void};
    use std::io;

    unsafe extern "C" {
        fn _read(fd: c_int, buf: *mut c_void, count: c_uint) -> c_int;
        fn _write(fd: c_int, buf: *const c_void, count: c_uint) -> c_int;
    }

    fn chunk(len: usize) -> c_uint {
        c_uint::try_from(len).unwrap_or(c_uint::MAX)
    }

    fn result(n: c_int) -> io::Result<usize> {
        usize::try_from(n).map_err(|_| io::Error::other("CRT descriptor call failed"))
    }

    pub(super) fn read_fd(fd: c_int, buf: &mut [u8]) -> io::Result<usize> {
        result(unsafe { _read(fd, buf.as_mut_ptr().cast(), chunk(buf.len())) })
    }

    pub(super) fn write_fd(fd: c_int, buf: &[u8]) -> io::Result<usize> {
        result(unsafe { _write(fd, buf.as_ptr().cast(), chunk(buf.len())) })
    }

    pub(super) fn set_bufsize(_fd: c_int, _bytes: c_int) {}

    pub(super) fn set_read_timeout(_fd: c_int, _seconds: c_int) {}
}

pub(crate) fn set_bufsize(fd: c_int, bytes: c_int) {
    sys::set_bufsize(fd, bytes);
}

pub(crate) fn set_read_timeout(fd: c_int, seconds: c_int) {
    sys::set_read_timeout(fd, seconds);
}

#[cfg(unix)]
pub(crate) fn send_fd(sock: c_int, fd: c_int) -> bool {
    sys::send_fd(sock, fd)
}

#[cfg(unix)]
pub(crate) fn recv_fd(sock: c_int) -> c_int {
    sys::recv_fd(sock)
}

pub(crate) fn read(fd: c_int, buf: &mut [u8]) -> io::Result<usize> {
    sys::read_fd(fd, buf)
}

pub(crate) fn write(fd: c_int, buf: &[u8]) -> io::Result<usize> {
    sys::write_fd(fd, buf)
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn body<'a>(data: *const c_void, n: usize) -> &'a [u8] {
    if data.is_null() || n == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(data.cast(), n) }
    }
}

fn malloc_string(text: &[u8]) -> *mut c_char {
    let out = unsafe { malloc(text.len() + 1) }.cast::<u8>();
    if !out.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(text.as_ptr(), out, text.len());
            *out.add(text.len()) = 0;
        }
    }
    out.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_conn_init(c: *mut Conn, fd: c_int) {
    if c.is_null() {
        return;
    }
    unsafe {
        (&raw mut (*c).fd).write(fd);
        (&raw mut (*c).start).write(0);
        (&raw mut (*c).len).write(0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn http_set_bufsize(fd: c_int, bytes: c_int) {
    set_bufsize(fd, bytes);
}

#[unsafe(no_mangle)]
pub extern "C" fn http_set_read_timeout(fd: c_int, seconds: c_int) {
    set_read_timeout(fd, seconds);
}

#[cfg(unix)]
#[unsafe(no_mangle)]
pub extern "C" fn http_send_fd(sock: c_int, fd: c_int) -> c_int {
    if send_fd(sock, fd) { 0 } else { -1 }
}

#[cfg(unix)]
#[unsafe(no_mangle)]
pub extern "C" fn http_recv_fd(sock: c_int) -> c_int {
    recv_fd(sock)
}

unsafe fn conn<'a>(c: *mut Conn) -> Option<&'a mut Conn> {
    unsafe { c.as_mut() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_read_head(c: *mut Conn, out: *mut Head) -> c_int {
    let Some(c) = (unsafe { conn(c) }) else {
        return -1;
    };
    if out.is_null() {
        return -1;
    }
    unsafe { ptr::write_bytes(out, 0, 1) };
    let out = unsafe { &mut *out };
    if c.read_head(out) { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_read_body(c: *mut Conn, n: c_long, dst: *mut c_void) -> c_int {
    let Some(c) = (unsafe { conn(c) }) else {
        return -1;
    };
    if n < 0 || (n > 0 && dst.is_null()) {
        return -1;
    }
    if n == 0 {
        return 0;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(dst.cast::<u8>(), n as usize) };
    if c.read_body(dst) { 0 } else { -1 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_skip_body(c: *mut Conn, n: c_long) -> c_int {
    let Some(c) = (unsafe { conn(c) }) else {
        return -1;
    };
    match u64::try_from(n) {
        Ok(n) if c.skip_body(n) => 0,
        _ => -1,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write_request(
    fd: c_int,
    method: *const c_char,
    path: *const c_char,
    content_type: *const c_char,
    data: *const c_void,
    n: usize,
) -> c_int {
    let method = unsafe { bytes(method) }.unwrap_or_default();
    let path = unsafe { bytes(path) }.unwrap_or_default();
    let content_type = unsafe { bytes(content_type) }.unwrap_or(b"text/plain");
    let Some(head) = crate::request_head(method, path, content_type, n) else {
        return -1;
    };
    if crate::write_message(fd, &head, unsafe { body(data, n) }) {
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn http_write_response(
    fd: c_int,
    status: c_int,
    content_type: *const c_char,
    extra_headers: *const c_char,
    data: *const c_void,
    n: usize,
) -> c_int {
    let content_type = unsafe { bytes(content_type) }.unwrap_or(b"text/plain");
    let extra_headers = unsafe { bytes(extra_headers) }.unwrap_or_default();
    let head = crate::response_head(status, content_type, extra_headers, n);
    if crate::write_message(fd, &head, unsafe { body(data, n) }) {
        0
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_escape(s: *const c_char) -> *mut c_char {
    let s = unsafe { bytes(s) }.unwrap_or_default();
    if s.len() > (usize::MAX - 1) / 6 {
        return ptr::null_mut();
    }
    malloc_string(&crate::json_escape(s))
}

unsafe fn json_value<'a>(body: *const c_char, key: *const c_char) -> Option<&'a [u8]> {
    let body = unsafe { bytes(body) }?;
    let key = unsafe { bytes(key) }?;
    crate::json_value(body, key)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_get_long(
    body: *const c_char,
    key: *const c_char,
    out: *mut c_long,
) -> c_int {
    let Some(value) = (unsafe { json_value(body, key) }) else {
        return -1;
    };
    if let Some(out) = unsafe { out.as_mut() } {
        *out = crate::atol(value);
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_get_double(
    body: *const c_char,
    key: *const c_char,
    out: *mut f64,
) -> c_int {
    let Some(value) = (unsafe { json_value(body, key) }) else {
        return -1;
    };
    if let Some(out) = unsafe { out.as_mut() } {
        *out = crate::json_double(value);
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn json_get_str(body: *const c_char, key: *const c_char) -> *mut c_char {
    unsafe { json_value(body, key) }
        .and_then(crate::json_string)
        .map_or(ptr::null_mut(), |text| malloc_string(&text))
}
