//! Southstar — the C ABI of the ported net.h calls: ns_build_error_page, ns_data_url_decode and the data: response the fetch path synthesizes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_long, c_uint};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::data_url::{self, Budgeted, Failure, Sink};
use crate::error_page;

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
    _headers: [*mut c_char; 8],
    body: *mut GByteArray,
    error: *mut c_char,
    _tail: [u8; 96],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsResponse>() == 200
        && core::mem::offset_of!(NsResponse, body) == 88
        && core::mem::offset_of!(NsResponse, error) == 96
);

unsafe extern "C" {
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_byte_array_set_size(array: *mut GByteArray, length: c_uint) -> *mut GByteArray;
    fn ns_net_response_budget() -> u64;
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
    let budget = unsafe { ns_net_response_budget() };
    let body_start = unsafe { (*resp.body).len };
    let mut sink = ByteArraySink(resp.body);
    let mut budgeted = Budgeted::new(&mut sink, budget);
    let Some(decoded) = data_url::decode(url_bytes, &mut budgeted) else {
        return 0;
    };
    if let Err(failure) = decoded.result {
        unsafe { g_byte_array_set_size(resp.body, body_start) };
        resp.error = match failure {
            Failure::TooLarge => glib::strdup(&data_url::budget_error(budget)),
            Failure::Malformed => glib::strdup(b"malformed data: URL"),
        };
    }
    resp.status = 200;
    resp.final_url = unsafe { glib::g_strdup(url) };
    resp.content_type = glib::strdup(&decoded.content_type);
    1
}
