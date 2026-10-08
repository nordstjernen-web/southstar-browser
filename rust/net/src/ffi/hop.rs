//! Southstar — one HTTP or FTP hop over a libcurl easy handle: the request and result structs net_backend.h declares, the curl transport, and the backend seam when the nghttp2 backend is not built.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::curl::{self, Code};
use super::fetch::widen;
use super::sinks::{NsHeaderCtx, NsWriteCtx, ns_header_cb, ns_write_cb};
use super::storage::{ns_net_altsvc_path, ns_net_hsts_curl_path};
use super::transport::{ns_net_apply_curl_tls, ns_net_multi_perform, ns_net_share, ns_xferinfo_cb};
use crate::{hsts, url};

const REFERER_NO_REFERRER: c_int = 0;
const REFERER_UNSAFE_URL: c_int = 3;

#[repr(C)]
pub struct HopReq {
    pub url: *const c_char,
    pub method: *const c_char,
    pub body: *const c_void,
    pub body_len: usize,
    pub headers: *mut curl::Slist,
    pub user_agent: *const c_char,
    pub referer: *const c_char,
    pub referer_policy: c_int,
    pub accept_encoding: *const c_char,
    pub timeout_s: c_long,
    pub connect_timeout_s: c_long,
    pub proxy: *const c_char,
    pub no_proxy: *const c_char,
    pub cookie_jar_path: *const c_char,
    pub cookie_js_path: *const c_char,
    pub follow_redirects: GBoolean,
    pub max_redirs: c_long,
    pub is_navigation: GBoolean,
    pub request_ftp: GBoolean,
    pub initial_https: GBoolean,
    pub http_version_pref: c_long,
}

#[repr(C)]
pub struct HopOut {
    pub status: c_long,
    pub effective_url: *mut c_char,
    pub remote_ip: *mut c_char,
    pub http_version: c_long,
    pub num_connects: c_long,
    pub t_namelookup_ms: f64,
    pub t_connect_ms: f64,
    pub t_appconnect_ms: f64,
    pub t_pretransfer_ms: f64,
    pub t_starttransfer_ms: f64,
    pub t_total_ms: f64,
    pub ok: GBoolean,
    pub cancelled: GBoolean,
    pub tls_verify_failed: GBoolean,
    pub connect_failed: GBoolean,
    pub tls_warning: *mut c_char,
    pub error_message: *mut c_char,
}

#[cfg(all(target_pointer_width = "64", not(windows)))]
const _: () = assert!(
    core::mem::size_of::<HopReq>() == 160
        && core::mem::offset_of!(HopReq, referer_policy) == 56
        && core::mem::offset_of!(HopReq, follow_redirects) == 120
        && core::mem::offset_of!(HopReq, http_version_pref) == 152
        && core::mem::size_of::<HopOut>() == 120
        && core::mem::offset_of!(HopOut, ok) == 88
        && core::mem::offset_of!(HopOut, error_message) == 112
);

impl HopOut {
    pub fn new() -> HopOut {
        HopOut {
            status: 0,
            effective_url: ptr::null_mut(),
            remote_ip: ptr::null_mut(),
            http_version: 0,
            num_connects: 0,
            t_namelookup_ms: 0.0,
            t_connect_ms: 0.0,
            t_appconnect_ms: 0.0,
            t_pretransfer_ms: 0.0,
            t_starttransfer_ms: 0.0,
            t_total_ms: 0.0,
            ok: 0,
            cancelled: 0,
            tls_verify_failed: 0,
            connect_failed: 0,
            tls_warning: ptr::null_mut(),
            error_message: ptr::null_mut(),
        }
    }
}

#[cfg(feature = "http-nghttp2")]
unsafe extern "C" {
    pub fn ns_hop_transport(
        req: *const HopReq,
        wctx: *mut NsWriteCtx,
        hctx: *mut NsHeaderCtx,
        out: *mut HopOut,
        cancellable: *mut c_void,
    ) -> GBoolean;
    fn ns_net_backend_shutdown();
}

#[cfg(feature = "http-nghttp2")]
pub fn backend_shutdown() {
    unsafe { ns_net_backend_shutdown() };
}

#[cfg(not(feature = "http-nghttp2"))]
pub fn backend_shutdown() {}

unsafe extern "C" {
    fn g_cancellable_is_cancelled(cancellable: *mut c_void) -> GBoolean;
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn non_empty(p: *const c_char) -> bool {
    text(p).is_some_and(|t| !t.is_empty())
}

pub struct Easy(*mut c_void);

impl Easy {
    pub fn new() -> Option<Easy> {
        let handle = unsafe { curl::curl_easy_init() };
        (!handle.is_null()).then_some(Easy(handle))
    }

    pub fn handle(&self) -> *mut c_void {
        self.0
    }

    pub fn long(&self, option: c_int, value: c_long) {
        unsafe { curl::curl_easy_setopt(self.0, option, value) };
    }

    pub fn ptr(&self, option: c_int, value: *const c_void) {
        unsafe { curl::curl_easy_setopt(self.0, option, value) };
    }

    pub fn text(&self, option: c_int, value: *const c_char) {
        self.ptr(option, value.cast());
    }

    pub fn off(&self, option: c_int, value: i64) {
        unsafe { curl::curl_easy_setopt(self.0, option, value) };
    }

    pub fn perform(&self) -> Code {
        unsafe { curl::curl_easy_perform(self.0) }
    }

    fn info_long(&self, info: c_int) -> c_long {
        let mut value: c_long = 0;
        unsafe { curl::curl_easy_getinfo(self.0, info, &mut value as *mut c_long) };
        value
    }

    fn info_ms(&self, info: c_int) -> f64 {
        let mut value: i64 = 0;
        unsafe { curl::curl_easy_getinfo(self.0, info, &mut value as *mut i64) };
        value as f64 / 1000.0
    }

    fn info_text(&self, info: c_int) -> *const c_char {
        let mut value: *const c_char = ptr::null();
        unsafe { curl::curl_easy_getinfo(self.0, info, &mut value as *mut *const c_char) };
        value
    }
}

impl Drop for Easy {
    fn drop(&mut self) {
        unsafe { curl::curl_easy_cleanup(self.0) };
    }
}

fn buffered(errbuf: &[c_char; curl::ERROR_SIZE]) -> &[u8] {
    unsafe { CStr::from_ptr(errbuf.as_ptr()) }.to_bytes()
}

fn error_text(errbuf: &[c_char; curl::ERROR_SIZE], rc: Code) -> Vec<u8> {
    let buffered = buffered(errbuf);
    if buffered.is_empty() {
        text(unsafe { curl::curl_easy_strerror(rc) })
            .unwrap_or_default()
            .to_vec()
    } else {
        buffered.to_vec()
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn is_cancelled(cancellable: *mut c_void) -> bool {
    !cancellable.is_null() && unsafe { g_cancellable_is_cancelled(cancellable) } != 0
}

fn configure(
    easy: &Easy,
    req: &HopReq,
    errbuf: &mut [c_char; curl::ERROR_SIZE],
    wctx: *mut NsWriteCtx,
    hctx: *mut NsHeaderCtx,
    cancellable: *mut c_void,
) {
    if std::env::var_os("NS_NET_TRACE").is_some() {
        easy.long(curl::OPT_VERBOSE, 1);
    }
    let share = ns_net_share();
    if !share.is_null() {
        easy.ptr(curl::OPT_SHARE, share);
    }
    easy.text(curl::OPT_URL, req.url);
    if non_empty(req.proxy) {
        easy.text(curl::OPT_PROXY, req.proxy);
    }
    if non_empty(req.no_proxy) {
        easy.text(curl::OPT_NOPROXY, req.no_proxy);
    }
    easy.long(
        curl::OPT_FOLLOWLOCATION,
        c_long::from(req.follow_redirects != 0),
    );
    easy.long(curl::OPT_UNRESTRICTED_AUTH, 0);
    easy.long(curl::OPT_MAXREDIRS, req.max_redirs);
    easy.long(curl::OPT_TIMEOUT, req.timeout_s);
    easy.long(curl::OPT_CONNECTTIMEOUT, req.connect_timeout_s);
    easy.text(curl::OPT_USERAGENT, req.user_agent);
    easy.text(
        curl::OPT_ACCEPT_ENCODING,
        if req.accept_encoding.is_null() {
            c"".as_ptr()
        } else {
            req.accept_encoding
        },
    );
    match req.referer_policy {
        REFERER_NO_REFERRER => {
            easy.long(curl::OPT_AUTOREFERER, 0);
            easy.text(curl::OPT_REFERER, c"".as_ptr());
        }
        REFERER_UNSAFE_URL => easy.long(curl::OPT_AUTOREFERER, 1),
        _ => easy.long(curl::OPT_AUTOREFERER, 0),
    }
    if non_empty(req.referer) {
        easy.text(curl::OPT_REFERER, req.referer);
    }

    let method = text(req.method);
    let is_post = method.is_some_and(|m| m.eq_ignore_ascii_case(b"POST"));
    let is_get = method.is_none_or(|m| m.is_empty() || m.eq_ignore_ascii_case(b"GET"));
    let has_body = !req.body.is_null() && req.body_len > 0;
    if is_post {
        easy.long(curl::OPT_POST, 1);
    }
    if has_body {
        easy.ptr(curl::OPT_POSTFIELDS, req.body);
        easy.long(curl::OPT_POSTFIELDSIZE, req.body_len as c_long);
    } else if is_post {
        easy.text(curl::OPT_POSTFIELDS, c"".as_ptr());
        easy.long(curl::OPT_POSTFIELDSIZE, 0);
    }
    let single_line = method.is_some_and(|m| !m.contains(&b'\r') && !m.contains(&b'\n'));
    if method.is_some_and(|m| m.eq_ignore_ascii_case(b"HEAD")) {
        easy.long(curl::OPT_NOBODY, 1);
    } else if !is_post && !is_get && single_line {
        easy.text(curl::OPT_CUSTOMREQUEST, req.method);
    }

    easy.ptr(curl::OPT_HTTPHEADER, req.headers.cast());
    easy.long(curl::OPT_NOSIGNAL, 1);
    easy.ptr(curl::OPT_ERRORBUFFER, errbuf.as_mut_ptr().cast());
    unsafe { ns_net_apply_curl_tls(easy.handle()) };

    if !req.cookie_jar_path.is_null() {
        easy.text(curl::OPT_COOKIEFILE, req.cookie_jar_path);
        easy.text(curl::OPT_COOKIEJAR, req.cookie_jar_path);
        if !req.cookie_js_path.is_null() {
            easy.text(curl::OPT_COOKIEFILE, req.cookie_js_path);
        }
    }

    easy.ptr(curl::OPT_WRITEFUNCTION, ns_write_cb as *const c_void);
    easy.ptr(curl::OPT_WRITEDATA, wctx.cast());
    easy.off(curl::OPT_MAXFILESIZE_LARGE, unsafe { (*wctx).budget() }
        as i64);
    easy.ptr(curl::OPT_HEADERFUNCTION, ns_header_cb as *const c_void);
    easy.ptr(curl::OPT_HEADERDATA, hctx.cast());

    easy.text(curl::OPT_PROTOCOLS_STR, c"http,https,ftp".as_ptr());
    let redirect_protocols = if req.initial_https != 0 {
        c"https"
    } else if req.request_ftp != 0 {
        c"ftp"
    } else {
        c"http,https"
    };
    easy.text(curl::OPT_REDIR_PROTOCOLS_STR, redirect_protocols.as_ptr());

    let hsts_path = ns_net_hsts_curl_path();
    if !hsts_path.is_null() {
        easy.long(curl::OPT_HSTS_CTRL, curl::HSTS_ENABLE);
        easy.text(curl::OPT_HSTS, hsts_path);
    }
    easy.long(curl::OPT_HTTP_VERSION, req.http_version_pref);
    let altsvc = ns_net_altsvc_path();
    if !altsvc.is_null() {
        easy.text(curl::OPT_ALTSVC, altsvc);
    }

    easy.long(curl::OPT_NOPROGRESS, 0);
    easy.ptr(curl::OPT_XFERINFOFUNCTION, ns_xferinfo_cb as *const c_void);
    easy.ptr(curl::OPT_XFERINFODATA, cancellable.cast_const());
}

fn retry_untrusted(
    easy: &Easy,
    req: &HopReq,
    errbuf: &mut [c_char; curl::ERROR_SIZE],
    wctx: &mut NsWriteCtx,
    out: &mut HopOut,
    rc: Code,
    cancellable: *mut c_void,
) -> Code {
    let opt_in = southstar_config::get().is_some_and(|c| c.tls_allow_insecure_override != 0);
    let host = text(req.url).and_then(url::host_from);
    let hsts_pinned = host.as_deref().is_some_and(hsts::should_upgrade);
    if !opt_in || hsts_pinned {
        return rc;
    }
    let warning = [
        &b"Insecure: TLS certificate not trusted ("[..],
        &error_text(errbuf, rc),
        b")",
    ]
    .concat();
    wctx.restart();
    errbuf[0] = 0;
    easy.long(curl::OPT_SSL_VERIFYPEER, 0);
    easy.long(curl::OPT_SSL_VERIFYHOST, 0);
    easy.text(curl::OPT_COOKIEFILE, c"".as_ptr());
    easy.text(curl::OPT_COOKIEJAR, ptr::null());
    easy.text(curl::OPT_COOKIELIST, c"ALL".as_ptr());
    let rc = unsafe { ns_net_multi_perform(easy.handle(), cancellable) };
    if rc == curl::E_OK {
        out.tls_warning = glib::strdup(&warning);
    }
    rc
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hop_out_clear(out: *mut HopOut) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    for field in [
        &mut out.effective_url,
        &mut out.remote_ip,
        &mut out.tls_warning,
        &mut out.error_message,
    ] {
        unsafe { glib::g_free((*field).cast()) };
        *field = ptr::null_mut();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hop_transport_curl(
    req: *const HopReq,
    wctx: *mut NsWriteCtx,
    hctx: *mut NsHeaderCtx,
    out: *mut HopOut,
    cancellable: *mut c_void,
) -> GBoolean {
    let (req, out) = unsafe { (&*req, &mut *out) };
    let Some(easy) = Easy::new() else {
        out.error_message = glib::strdup(b"curl_easy_init failed");
        return 0;
    };
    let mut errbuf = [0 as c_char; curl::ERROR_SIZE];
    configure(&easy, req, &mut errbuf, wctx, hctx, cancellable);

    let mut rc = unsafe { ns_net_multi_perform(easy.handle(), cancellable) };

    if rc == curl::E_RECV_ERROR
        && unsafe { (*hctx).has_location() }
        && contains(buffered(&errbuf), b"unexpected eof")
        && crate::request::is_redirect(widen(easy.info_long(curl::INFO_RESPONSE_CODE)))
    {
        rc = curl::E_OK;
    }

    let wctx = unsafe { &mut *wctx };
    if (rc == curl::E_PEER_FAILED_VERIFICATION || rc == curl::E_SSL_CACERT_BADFILE)
        && text(req.url).is_some_and(|u| u.starts_with(b"https://"))
    {
        rc = retry_untrusted(&easy, req, &mut errbuf, wctx, out, rc, cancellable);
    }

    if rc == curl::E_ABORTED_BY_CALLBACK && is_cancelled(cancellable) {
        out.cancelled = 1;
        return 0;
    }

    out.status = easy.info_long(curl::INFO_RESPONSE_CODE);
    let effective = easy.info_text(curl::INFO_EFFECTIVE_URL);
    out.effective_url = unsafe {
        glib::g_strdup(if effective.is_null() {
            req.url
        } else {
            effective
        })
    };
    out.t_namelookup_ms = easy.info_ms(curl::INFO_NAMELOOKUP_TIME_T);
    out.t_connect_ms = easy.info_ms(curl::INFO_CONNECT_TIME_T);
    out.t_appconnect_ms = easy.info_ms(curl::INFO_APPCONNECT_TIME_T);
    out.t_pretransfer_ms = easy.info_ms(curl::INFO_PRETRANSFER_TIME_T);
    out.t_starttransfer_ms = easy.info_ms(curl::INFO_STARTTRANSFER_TIME_T);
    out.t_total_ms = easy.info_ms(curl::INFO_TOTAL_TIME_T);
    let ip = easy.info_text(curl::INFO_PRIMARY_IP);
    if non_empty(ip) {
        out.remote_ip = unsafe { glib::g_strdup(ip) };
    }
    out.http_version = easy.info_long(curl::INFO_HTTP_VERSION);
    out.num_connects = easy.info_long(curl::INFO_NUM_CONNECTS);
    out.tls_verify_failed = glib::boolean(matches!(
        rc,
        curl::E_PEER_FAILED_VERIFICATION | curl::E_SSL_CACERT_BADFILE | curl::E_SSL_ISSUER_ERROR
    ));
    out.connect_failed = glib::boolean(matches!(
        rc,
        curl::E_COULDNT_CONNECT | curl::E_OPERATION_TIMEDOUT | curl::E_COULDNT_RESOLVE_HOST
    ));
    if rc == curl::E_FILESIZE_EXCEEDED {
        wctx.mark_exceeded();
    }
    out.ok = glib::boolean(rc == curl::E_OK);
    if rc != curl::E_OK && !wctx.exceeded() {
        out.error_message = glib::strdup(&error_text(&errbuf, rc));
    }
    1
}

#[cfg(not(feature = "http-nghttp2"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hop_transport(
    req: *const HopReq,
    wctx: *mut NsWriteCtx,
    hctx: *mut NsHeaderCtx,
    out: *mut HopOut,
    cancellable: *mut c_void,
) -> GBoolean {
    unsafe { ns_hop_transport_curl(req, wctx, hctx, out, cancellable) }
}

#[cfg(not(feature = "http-nghttp2"))]
#[unsafe(no_mangle)]
pub extern "C" fn ns_net_backend_shutdown() {}
