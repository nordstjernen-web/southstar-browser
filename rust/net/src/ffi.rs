//! Southstar — the C ABI of the ported net.h calls: error pages, about:, data:, file:, FTP listing and view-source: responses written into an ns_response.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod curl;
pub mod forms;
pub mod host;
pub mod lexbor;
pub mod netlog;
pub mod proxy;
pub mod sinks;
pub mod storage;
pub mod sys;
pub mod transport;
pub mod url;

use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use core::sync::atomic::Ordering;

use southstar_glib::{self as glib, GBoolean, GError};

use crate::budget::{Budgeted, Sink, exhausted_error};
use crate::data_url::{self, Failure};
use crate::{about, error_page, file, ftp, view_source};

pub use sys::utf8_collate;

#[repr(C)]
pub struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    content_disposition: *mut c_char,
    csp_header: *mut c_char,
    xframe_options: *mut c_char,
    x_content_type_options: *mut c_char,
    cors_allow_origin: *mut c_char,
    refresh: *mut c_char,
    content_language: *mut c_char,
    raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
    tls_warning: *mut c_char,
    remote_ip: *mut c_char,
    next_hop_protocol: *mut c_char,
    request_start_us: i64,
    request_start_real_ms: f64,
    domain_lookup_ms: f64,
    connect_ms: f64,
    tls_ms: f64,
    pretransfer_ms: f64,
    response_start_ms: f64,
    response_end_ms: f64,
    security: c_int,
    redirect_count: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsResponse>() == 200
        && core::mem::offset_of!(NsResponse, body) == 88
        && core::mem::offset_of!(NsResponse, response_end_ms) == 184
        && core::mem::offset_of!(NsResponse, security) == 192
);

unsafe extern "C" {
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_byte_array_set_size(array: *mut GByteArray, length: c_uint) -> *mut GByteArray;
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_unref(array: *mut GByteArray);
    fn ns_net_request_blocking(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        extra_headers: *const *const c_char,
        cancellable: *mut c_void,
        error: *mut *mut GError,
    ) -> *mut NsResponse;
    fn ns_html_decode_body_full(
        body: *const c_char,
        len: usize,
        content_type: *const c_char,
        charset_out: *mut *mut c_char,
    ) -> *mut c_char;
}

struct ByteArraySink(*mut GByteArray);

impl Sink for ByteArraySink {
    fn append(&mut self, bytes: &[u8]) {
        unsafe { g_byte_array_append(self.0, bytes.as_ptr(), bytes.len() as c_uint) };
    }
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn body_bytes<'a>(body: *const GByteArray) -> Option<&'a [u8]> {
    let body = unsafe { body.as_ref() }?;
    Some(unsafe { glib::slice(body.data, body.len as usize) })
}

fn replace(field: &mut *mut c_char, value: &[u8]) {
    unsafe { glib::g_free((*field).cast()) };
    *field = glib::strdup(value);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_build_error_page(
    url: *const c_char,
    status: c_long,
    transport_error: *const c_char,
) -> *mut c_char {
    glib::strdup(&error_page::build(text(url), status, text(transport_error)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_data_url_decode(
    url: *const c_char,
    out: *mut GByteArray,
    budget: u64,
    out_content_type: *mut *mut c_char,
    too_large: *mut GBoolean,
) -> GBoolean {
    unsafe {
        if let Some(t) = too_large.as_mut() {
            *t = 0;
        }
        if let Some(ct) = out_content_type.as_mut() {
            *ct = ptr::null_mut();
        }
    }
    let Some(url) = text(url).filter(|_| !out.is_null()) else {
        return 0;
    };
    let mut sink = ByteArraySink(out);
    let mut budgeted = Budgeted::new(&mut sink, budget);
    let Some(decoded) = data_url::decode(url, &mut budgeted) else {
        return 0;
    };
    unsafe {
        if let Some(ct) = out_content_type.as_mut() {
            *ct = glib::strdup(&decoded.content_type);
        }
    }
    match decoded.result {
        Ok(()) => 1,
        Err(failure) => {
            if let (Failure::TooLarge, Some(t)) = (failure, unsafe { too_large.as_mut() }) {
                *t = 1;
            }
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_synthesize_data_response(
    url: *const c_char,
    resp: *mut NsResponse,
) -> GBoolean {
    let Some(url_bytes) = text(url).filter(|u| u.starts_with(b"data:")) else {
        return 0;
    };
    let resp = unsafe { &mut *resp };
    let budget = crate::transport::response_budget();
    let body_start = unsafe { (*resp.body).len };
    let mut sink = ByteArraySink(resp.body);
    let mut budgeted = Budgeted::new(&mut sink, budget);
    let Some(decoded) = data_url::decode(url_bytes, &mut budgeted) else {
        return 0;
    };
    if let Err(failure) = decoded.result {
        unsafe { g_byte_array_set_size(resp.body, body_start) };
        resp.error = match failure {
            Failure::TooLarge => glib::strdup(&exhausted_error(budget)),
            Failure::Malformed => glib::strdup(b"malformed data: URL"),
        };
    }
    resp.status = 200;
    resp.final_url = unsafe { glib::g_strdup(url) };
    resp.content_type = glib::strdup(&decoded.content_type);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_set_allow_file_urls(allow: GBoolean) {
    file::ALLOW_FILE_URLS.store(allow != 0, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_synthesize_file_response(
    url: *const c_char,
    top_url: *const c_char,
    resp: *mut NsResponse,
) -> GBoolean {
    let Some(url) = text(url) else {
        return 0;
    };
    let resp = unsafe { &mut *resp };
    let mut sink = ByteArraySink(resp.body);
    let budget = crate::transport::response_budget;
    let Some(out) = file::respond(url, text(top_url), &mut sink, budget) else {
        return 0;
    };
    resp.final_url = glib::strdup(&out.final_url);
    resp.status = out.status;
    if let Some(error) = out.error {
        resp.error = glib::strdup(&error);
    }
    if let Some(content_type) = out.content_type {
        resp.content_type = glib::strdup(&content_type);
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_finish_ftp_response(resp: *mut NsResponse) {
    let Some(resp) = (unsafe { resp.as_mut() }) else {
        return;
    };
    let final_url = text(resp.final_url);
    if !resp.body.is_null() && ftp::looks_like_directory(final_url) {
        let html = ftp::directory_page(final_url, body_bytes(resp.body).unwrap_or_default());
        unsafe { g_byte_array_set_size(resp.body, 0) };
        ByteArraySink(resp.body).append(&html);
        replace(&mut resp.content_type, b"text/html; charset=utf-8");
    }
    let Some(final_url) = text(resp.final_url) else {
        return;
    };
    if !resp.content_type.is_null()
        || !final_url.starts_with(b"ftp://")
        || ftp::looks_like_directory(Some(final_url))
    {
        return;
    }
    let data = unsafe { resp.body.as_ref() }.map(|b| (b.data, b.len as usize));
    let data = data
        .filter(|(p, _)| !p.is_null())
        .map(|(p, len)| unsafe { core::slice::from_raw_parts(p, len) });
    let mime = sys::mime_type_guess(&ftp::guess_path(final_url), data);
    resp.content_type = glib::strdup(&mime);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_synthesize_about_response(
    url: *const c_char,
    top_url: *const c_char,
    method: *const c_char,
    req_body: *const c_void,
    req_body_len: usize,
    resp: *mut NsResponse,
) -> GBoolean {
    let Some(url_bytes) = text(url) else {
        return 0;
    };
    let req_body =
        (!req_body.is_null()).then(|| unsafe { glib::slice(req_body.cast(), req_body_len) });
    let Some(page) = about::respond(url_bytes, text(top_url), text(method), req_body) else {
        return 0;
    };
    let resp = unsafe { &mut *resp };
    resp.status = page.status;
    resp.final_url = unsafe { glib::g_strdup(url) };
    resp.content_type = glib::strdup(page.content_type);
    ByteArraySink(resp.body).append(&page.body);
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_synthesize_view_source_response(
    url: *const c_char,
    top_url: *const c_char,
    cancellable: *mut c_void,
    resp: *mut NsResponse,
) -> GBoolean {
    let Some(inner) = text(url).and_then(|u| u.strip_prefix(view_source::PREFIX)) else {
        return 0;
    };
    let resp = unsafe { &mut *resp };
    resp.final_url = unsafe { glib::g_strdup(url) };
    let mut body = ByteArraySink(resp.body);
    if !view_source::allowed(text(top_url), inner) {
        resp.status = 403;
        resp.content_type = glib::strdup(b"text/plain; charset=utf-8");
        body.append(b"view-source: is not available here");
        return 1;
    }
    let inner_ptr = unsafe { url.add(view_source::PREFIX.len()) };
    let mut err: *mut GError = ptr::null_mut();
    let fetched = unsafe {
        ns_net_request_blocking(
            inner_ptr,
            ptr::null(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
            cancellable,
            &mut err,
        )
    };
    let fetched_ref = unsafe { fetched.as_ref() };
    let failed = match fetched_ref {
        None => true,
        Some(r) => {
            !r.error.is_null()
                || (r.status != 0
                    && r.status >= 400
                    && body_bytes(r.body).is_none_or(<[u8]>::is_empty))
        }
    };
    let page = if failed {
        let message: &[u8] = match fetched_ref {
            Some(r) if r.error.is_null() && r.status >= 400 => b"the server returned an error",
            Some(r) if !r.error.is_null() => text(r.error).unwrap_or_default(),
            _ => unsafe { err.as_ref() }
                .and_then(|e| text(e.message))
                .unwrap_or(b"request failed"),
        };
        view_source::error_document(inner, message)
    } else {
        let r = fetched_ref.unwrap_or_else(|| unreachable!());
        let (data, len) =
            unsafe { r.body.as_ref() }.map_or((ptr::null(), 0), |b| (b.data, b.len as usize));
        let decoded =
            unsafe { ns_html_decode_body_full(data.cast(), len, r.content_type, ptr::null_mut()) };
        let source = unsafe { glib::GStr::take(decoded) };
        view_source::document(inner, source.as_deref().map_or(&b""[..], |s| s.to_bytes()))
    };
    resp.status = 200;
    resp.content_type = glib::strdup(b"text/html; charset=utf-8");
    body.append(&page);
    if !fetched.is_null() {
        unsafe { ns_response_free(fetched) };
    }
    if !err.is_null() {
        unsafe { glib::g_error_free(err) };
    }
    1
}

fn text_fields(r: &mut NsResponse) -> [&mut *mut c_char; 14] {
    [
        &mut r.final_url,
        &mut r.content_type,
        &mut r.content_disposition,
        &mut r.csp_header,
        &mut r.xframe_options,
        &mut r.x_content_type_options,
        &mut r.cors_allow_origin,
        &mut r.refresh,
        &mut r.content_language,
        &mut r.raw_headers,
        &mut r.error,
        &mut r.tls_warning,
        &mut r.remote_ip,
        &mut r.next_hop_protocol,
    ]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_response_free(resp: *mut NsResponse) {
    let Some(r) = (unsafe { resp.as_mut() }) else {
        return;
    };
    for field in text_fields(r) {
        unsafe { glib::g_free((*field).cast()) };
    }
    if !r.body.is_null() {
        unsafe { g_byte_array_unref(r.body) };
    }
    unsafe { glib::g_free(resp.cast()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_response_copy(src: *const NsResponse) -> *mut NsResponse {
    if src.is_null() {
        return ptr::null_mut();
    }
    let copy = unsafe { glib::g_malloc0(core::mem::size_of::<NsResponse>()) }.cast::<NsResponse>();
    unsafe { ptr::copy_nonoverlapping(src, copy, 1) };
    let r = unsafe { &mut *copy };
    for field in text_fields(r) {
        *field = unsafe { glib::g_strdup(*field) };
    }
    let body = body_bytes(unsafe { (*src).body }).unwrap_or_default();
    r.body = unsafe { g_byte_array_new() };
    if !body.is_empty() {
        ByteArraySink(r.body).append(body);
    }
    copy
}
