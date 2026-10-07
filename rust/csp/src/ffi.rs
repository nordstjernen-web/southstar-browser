//! Southstar — the C ABI of the CSP parser and checks, as declared in src/csp.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint};
use core::ptr;

use southstar_glib::{
    FALSE, GBoolean, GChecksumType, TRUE, boolean, bytes, g_base64_encode, g_checksum_free,
    g_checksum_get_digest, g_checksum_new, g_checksum_update, g_free, slice,
};

use crate::{Csp, UrlParts};

#[repr(C)]
struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

unsafe extern "C" {
    fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> GBoolean;
    fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts;
    fn ns_url_parts_free(parts: *mut NsUrlParts);
}

fn optional_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

unsafe fn owned(p: *const c_char) -> Option<Vec<u8>> {
    unsafe { bytes(p) }.map(<[u8]>::to_vec)
}

pub(crate) fn same_origin(a: &CStr, b: Option<&CStr>) -> bool {
    unsafe { ns_url_same_origin(a.as_ptr(), optional_ptr(b)) != FALSE }
}

pub(crate) fn url_parts(url: &CStr) -> Option<UrlParts> {
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    if parts.is_null() {
        return None;
    }
    let copy = unsafe {
        UrlParts {
            protocol: owned((*parts).protocol).unwrap_or_default(),
            hostname: owned((*parts).hostname),
            port: owned((*parts).port).unwrap_or_default(),
            pathname: owned((*parts).pathname).unwrap_or_default(),
        }
    };
    unsafe { ns_url_parts_free(parts) };
    Some(copy)
}

pub(crate) fn base64_digest(checksum_type: GChecksumType, data: &[u8]) -> Vec<u8> {
    let mut digest = [0u8; 64];
    let mut digest_len = digest.len();
    unsafe {
        let checksum = g_checksum_new(checksum_type);
        g_checksum_update(checksum, data.as_ptr(), data.len() as isize);
        g_checksum_get_digest(checksum, digest.as_mut_ptr(), &mut digest_len);
        let encoded = g_base64_encode(digest.as_ptr(), digest_len);
        let out = owned(encoded).unwrap_or_default();
        g_free(encoded.cast());
        g_checksum_free(checksum);
        out
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_parse(header_value: *const c_char) -> *mut Csp {
    match unsafe { bytes(header_value) }.and_then(Csp::parse) {
        Some(csp) => Box::into_raw(Box::new(csp)),
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_free(csp: *mut Csp) {
    if !csp.is_null() {
        drop(unsafe { Box::from_raw(csp) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_merge(dst: *mut Csp, src: *mut Csp) {
    if dst.is_null() || src.is_null() || dst == src {
        return;
    }
    unsafe { (*dst).merge(&mut *src) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_allows(
    csp: *const Csp,
    kind: c_uint,
    resource_url: *const c_char,
    document_url: *const c_char,
) -> GBoolean {
    unsafe { ns_csp_allows_with_nonce(csp, kind, resource_url, document_url, ptr::null(), TRUE) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_allows_with_nonce(
    csp: *const Csp,
    kind: c_uint,
    resource_url: *const c_char,
    document_url: *const c_char,
    nonce: *const c_char,
    parser_inserted: GBoolean,
) -> GBoolean {
    let (Some(csp), Some(resource_url)) = (unsafe { csp.as_ref() }, unsafe { cstr(resource_url) })
    else {
        return TRUE;
    };
    boolean(csp.allows(
        kind as usize,
        resource_url,
        unsafe { cstr(document_url) },
        unsafe { bytes(nonce) },
        parser_inserted != FALSE,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_inline_script_allowed(
    csp: *const Csp,
    body: *const c_char,
    body_len: usize,
    nonce: *const c_char,
) -> GBoolean {
    let Some(csp) = (unsafe { csp.as_ref() }) else {
        return TRUE;
    };
    let body = unsafe { slice(body.cast(), body_len) };
    boolean(csp.inline_script_allowed(body, unsafe { bytes(nonce) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_csp_inline_event_handler_allowed(csp: *const Csp) -> GBoolean {
    match unsafe { csp.as_ref() } {
        Some(csp) => boolean(csp.inline_event_handler_allowed()),
        None => TRUE,
    }
}
