//! Southstar — TLS client connections over OpenSSL's libssl: the shared contexts, ALPN, certificate checks, session resumption and blocking or non-blocking reads and writes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uchar, c_uint, c_void};
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Mutex;

#[repr(C)]
struct SslCtx(c_void);
#[repr(C)]
struct SslRaw(c_void);
#[repr(C)]
struct SslSession(c_void);
#[repr(C)]
struct SslMethod(c_void);

unsafe extern "C" {
    fn TLS_client_method() -> *const SslMethod;
    fn SSL_CTX_new(method: *const SslMethod) -> *mut SslCtx;
    fn SSL_CTX_ctrl(ctx: *mut SslCtx, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    fn SSL_CTX_set_options(ctx: *mut SslCtx, options: u64) -> u64;
    fn SSL_CTX_set_cipher_list(ctx: *mut SslCtx, list: *const c_char) -> c_int;
    fn SSL_CTX_set_ciphersuites(ctx: *mut SslCtx, list: *const c_char) -> c_int;
    fn SSL_CTX_load_verify_locations(
        ctx: *mut SslCtx,
        file: *const c_char,
        path: *const c_char,
    ) -> c_int;
    fn SSL_CTX_set_default_verify_paths(ctx: *mut SslCtx) -> c_int;
    fn SSL_CTX_set_verify(ctx: *mut SslCtx, mode: c_int, callback: *const c_void);
    fn SSL_CTX_set_alpn_protos(ctx: *mut SslCtx, protos: *const c_uchar, len: c_uint) -> c_int;
    fn SSL_new(ctx: *mut SslCtx) -> *mut SslRaw;
    fn SSL_free(ssl: *mut SslRaw);
    fn SSL_set_fd(ssl: *mut SslRaw, fd: c_int) -> c_int;
    fn SSL_ctrl(ssl: *mut SslRaw, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    fn SSL_set_hostflags(ssl: *mut SslRaw, flags: c_uint);
    fn SSL_set1_host(ssl: *mut SslRaw, host: *const c_char) -> c_int;
    fn SSL_connect(ssl: *mut SslRaw) -> c_int;
    fn SSL_get_verify_result(ssl: *const SslRaw) -> c_long;
    fn SSL_get_error(ssl: *const SslRaw, ret: c_int) -> c_int;
    fn SSL_read(ssl: *mut SslRaw, buf: *mut c_void, num: c_int) -> c_int;
    fn SSL_write(ssl: *mut SslRaw, buf: *const c_void, num: c_int) -> c_int;
    fn SSL_pending(ssl: *const SslRaw) -> c_int;
    fn SSL_get0_alpn_selected(ssl: *const SslRaw, data: *mut *const c_uchar, len: *mut c_uint);
    fn SSL_get1_session(ssl: *mut SslRaw) -> *mut SslSession;
    fn SSL_set_session(ssl: *mut SslRaw, session: *mut SslSession) -> c_int;
    fn SSL_SESSION_free(session: *mut SslSession);
    fn SSL_shutdown(ssl: *mut SslRaw) -> c_int;
    fn X509_verify_cert_error_string(n: c_long) -> *const c_char;
}

const SSL_CTRL_MODE: c_int = 33;
const SSL_CTRL_SET_SESS_CACHE_MODE: c_int = 44;
const SSL_CTRL_SET_TLSEXT_HOSTNAME: c_int = 55;
const SSL_CTRL_SET_GROUPS_LIST: c_int = 92;
const SSL_CTRL_SET_MIN_PROTO_VERSION: c_int = 123;
const SSL_SESS_CACHE_CLIENT: c_long = 0x1;
const SSL_SESS_CACHE_NO_INTERNAL_STORE: c_long = 0x200;
const TLS1_2_VERSION: c_long = 0x0303;
const SSL_OP_NO_COMPRESSION: u64 = 1 << 17;
const SSL_MODE_ENABLE_PARTIAL_WRITE: c_long = 0x1;
const SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER: c_long = 0x2;
const SSL_MODE_AUTO_RETRY: c_long = 0x4;
const SSL_VERIFY_NONE: c_int = 0;
const SSL_VERIFY_PEER: c_int = 1;
const X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS: c_uint = 0x4;
const SSL_ERROR_WANT_READ: c_int = 2;
const SSL_ERROR_WANT_WRITE: c_int = 3;
const SSL_ERROR_SYSCALL: c_int = 5;
const SSL_ERROR_ZERO_RETURN: c_int = 6;
pub const X509_V_OK: c_long = 0;

const ALPN_H2: &[u8] = b"\x02h2\x08http/1.1";
const ALPN_H1: &[u8] = b"\x08http/1.1";

const CIPHERS: &CStr = c"ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:ECDHE-RSA-AES128-SHA:ECDHE-RSA-AES256-SHA:AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA";
const SUITES: &CStr = c"TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256";

pub struct Settings {
    pub ca_bundle: Option<Vec<u8>>,
    pub curves: Vec<u8>,
}

struct Ctx(*mut SslCtx);
unsafe impl Send for Ctx {}

struct Session(*mut SslSession);
unsafe impl Send for Session {}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { SSL_SESSION_free(self.0) };
    }
}

static CONTEXTS: Mutex<[Option<Ctx>; 4]> = Mutex::new([None, None, None, None]);
static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);

fn context(verify: bool, h2: bool, settings: &Settings) -> Option<*mut SslCtx> {
    let mut slots = CONTEXTS.lock().unwrap_or_else(|e| e.into_inner());
    let slot = &mut slots[usize::from(!verify) * 2 + usize::from(!h2)];
    if let Some(ctx) = slot {
        return Some(ctx.0);
    }
    let ctx = unsafe { SSL_CTX_new(TLS_client_method()) };
    if ctx.is_null() {
        return None;
    }
    unsafe {
        SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_SESS_CACHE_MODE,
            SSL_SESS_CACHE_CLIENT | SSL_SESS_CACHE_NO_INTERNAL_STORE,
            core::ptr::null_mut(),
        );
        SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_MIN_PROTO_VERSION,
            TLS1_2_VERSION,
            core::ptr::null_mut(),
        );
        SSL_CTX_set_options(ctx, SSL_OP_NO_COMPRESSION);
        SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_MODE,
            SSL_MODE_ENABLE_PARTIAL_WRITE
                | SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER
                | SSL_MODE_AUTO_RETRY,
            core::ptr::null_mut(),
        );
        SSL_CTX_set_cipher_list(ctx, CIPHERS.as_ptr());
        SSL_CTX_set_ciphersuites(ctx, SUITES.as_ptr());
        if let Ok(curves) = CString::new(settings.curves.clone()) {
            if !settings.curves.is_empty() {
                SSL_CTX_ctrl(
                    ctx,
                    SSL_CTRL_SET_GROUPS_LIST,
                    0,
                    curves.as_ptr() as *mut c_void,
                );
            }
        }
        if verify {
            match settings
                .ca_bundle
                .as_ref()
                .and_then(|c| CString::new(c.clone()).ok())
            {
                Some(ca) if !ca.as_bytes().is_empty() => {
                    SSL_CTX_load_verify_locations(ctx, ca.as_ptr(), core::ptr::null());
                }
                _ => {
                    SSL_CTX_set_default_verify_paths(ctx);
                }
            }
            SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, core::ptr::null());
        } else {
            SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, core::ptr::null());
        }
        let alpn = if h2 { ALPN_H2 } else { ALPN_H1 };
        SSL_CTX_set_alpn_protos(ctx, alpn.as_ptr(), alpn.len() as c_uint);
    }
    *slot = Some(Ctx(ctx));
    Some(ctx)
}

pub fn verify_error_text(code: c_long) -> String {
    let p = unsafe { X509_verify_cert_error_string(code) };
    if p.is_null() {
        return String::from("unknown error");
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

pub struct Tls {
    ssl: *mut SslRaw,
    key: String,
    resumable: bool,
}

unsafe impl Send for Tls {}

pub enum Io {
    Done(usize),
    WouldBlock,
    Closed,
    Failed,
}

pub enum Handshake {
    Cancelled,
    Failed { verify_result: c_long },
}

impl Tls {
    pub fn handshake(
        fd: super::socket::Raw,
        host: &str,
        port: u16,
        verify: bool,
        h2: bool,
        settings: &Settings,
        should_abort: &dyn Fn() -> bool,
    ) -> Result<Tls, Handshake> {
        let ctx = context(verify, h2, settings).ok_or(Handshake::Failed {
            verify_result: X509_V_OK,
        })?;
        let ssl = unsafe { SSL_new(ctx) };
        if ssl.is_null() {
            return Err(Handshake::Failed {
                verify_result: X509_V_OK,
            });
        }
        let key = format!("{host}:{port}");
        let tls = Tls {
            ssl,
            key,
            resumable: verify,
        };
        let host_c = CString::new(host).unwrap_or_default();
        unsafe {
            SSL_set_fd(ssl, fd as c_int);
            SSL_ctrl(
                ssl,
                SSL_CTRL_SET_TLSEXT_HOSTNAME,
                0,
                host_c.as_ptr() as *mut c_void,
            );
        }
        if let Some(s) = SESSIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|m| m.get(&tls.key))
        {
            unsafe { SSL_set_session(ssl, s.0) };
        }
        if verify {
            unsafe {
                SSL_set_hostflags(ssl, X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS);
                SSL_set1_host(ssl, host_c.as_ptr());
            }
        }
        loop {
            let r = unsafe { SSL_connect(ssl) };
            if r == 1 {
                return Ok(tls);
            }
            let err = unsafe { SSL_get_error(ssl, r) };
            let retry = err == SSL_ERROR_WANT_READ
                || err == SSL_ERROR_WANT_WRITE
                || (err == SSL_ERROR_SYSCALL && super::socket::retryable(os_error()));
            if retry {
                if should_abort() {
                    return Err(Handshake::Cancelled);
                }
                continue;
            }
            let verify_result = unsafe { SSL_get_verify_result(ssl) };
            return Err(Handshake::Failed { verify_result });
        }
    }

    pub fn alpn_h2(&self) -> bool {
        let mut data: *const c_uchar = core::ptr::null();
        let mut len: c_uint = 0;
        unsafe { SSL_get0_alpn_selected(self.ssl, &mut data, &mut len) };
        len == 2 && !data.is_null() && unsafe { core::slice::from_raw_parts(data, 2) } == b"h2"
    }

    pub fn pending(&self) -> bool {
        unsafe { SSL_pending(self.ssl) > 0 }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Io {
        let n = buf.len().min(c_int::MAX as usize) as c_int;
        let r = unsafe { SSL_read(self.ssl, buf.as_mut_ptr().cast(), n) };
        if r > 0 {
            return Io::Done(r as usize);
        }
        let err = unsafe { SSL_get_error(self.ssl, r) };
        if err == SSL_ERROR_ZERO_RETURN {
            return Io::Closed;
        }
        if err == SSL_ERROR_WANT_READ || err == SSL_ERROR_WANT_WRITE {
            return Io::WouldBlock;
        }
        if err == SSL_ERROR_SYSCALL {
            let e = os_error();
            if super::socket::retryable(e) {
                return Io::WouldBlock;
            }
            if e == 0 {
                return Io::Closed;
            }
        }
        Io::Failed
    }

    pub fn write(&mut self, buf: &[u8]) -> Io {
        let n = buf.len().min(c_int::MAX as usize) as c_int;
        let r = unsafe { SSL_write(self.ssl, buf.as_ptr().cast(), n) };
        if r > 0 {
            return Io::Done(r as usize);
        }
        let err = unsafe { SSL_get_error(self.ssl, r) };
        if err == SSL_ERROR_WANT_READ
            || err == SSL_ERROR_WANT_WRITE
            || (err == SSL_ERROR_SYSCALL && super::socket::retryable(os_error()))
        {
            return Io::WouldBlock;
        }
        Io::Failed
    }
}

impl Drop for Tls {
    fn drop(&mut self) {
        if self.resumable {
            let session = unsafe { SSL_get1_session(self.ssl) };
            if !session.is_null() {
                SESSIONS
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get_or_insert_with(HashMap::new)
                    .insert(self.key.clone(), Session(session));
            }
        }
        unsafe {
            SSL_shutdown(self.ssl);
            SSL_free(self.ssl);
        }
    }
}

pub fn shutdown() {
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).take();
}

fn os_error() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
