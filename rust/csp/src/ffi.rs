//! Southstar — the C ABI of the CSP parser and checks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint};
use core::ptr;

use southstar_glib::{
    FALSE, GBoolean, GChecksumType, TRUE, boolean, bytes, g_base64_encode, g_checksum_free,
    g_checksum_get_digest, g_checksum_new, g_checksum_update, g_free, slice,
};

use crate::Csp;

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

unsafe fn owned(p: *const c_char) -> Option<Vec<u8>> {
    unsafe { bytes(p) }.map(<[u8]>::to_vec)
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
