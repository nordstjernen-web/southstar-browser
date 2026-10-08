//! Southstar — the body and header sinks transports write into: the budgeted body append behind curl's write callback and ns_body_sink_write, and the header callback that captures the headers the fetch path reads and hands them over.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::GByteArray;

const RECHECK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RAW_HEADER_BYTES: usize = 1024 * 1024;

#[repr(C)]
pub struct NsWriteCtx {
    body: *mut GByteArray,
    total: u64,
    budget: u64,
    next_recheck: u64,
    exceeded: GBoolean,
}

#[repr(C)]
pub struct GString {
    str_: *mut c_char,
    len: usize,
    allocated_len: usize,
}

#[repr(C)]
pub struct NsHeaderCtx {
    content_type_out: *mut *mut c_char,
    content_disposition_out: *mut *mut c_char,
    csp_out: *mut *mut c_char,
    xframe_options_out: *mut *mut c_char,
    x_content_type_options_out: *mut *mut c_char,
    cors_allow_origin_out: *mut *mut c_char,
    refresh_out: *mut *mut c_char,
    content_language_out: *mut *mut c_char,
    etag: *mut c_char,
    last_modified: *mut c_char,
    cache_control: *mut c_char,
    vary: *mut c_char,
    expires: *mut c_char,
    location: *mut c_char,
    raw: *mut GString,
    set_cookie_seen: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsWriteCtx>() == 40
        && core::mem::size_of::<NsHeaderCtx>() == 128
        && core::mem::offset_of!(NsHeaderCtx, set_cookie_seen) == 120
);

impl NsWriteCtx {
    pub fn new(body: *mut GByteArray) -> NsWriteCtx {
        NsWriteCtx {
            body,
            total: 0,
            budget: crate::transport::response_budget(),
            next_recheck: RECHECK_BYTES,
            exceeded: 0,
        }
    }

    pub fn restart(&mut self) {
        unsafe { g_byte_array_set_size(self.body, 0) };
        self.total = 0;
        self.next_recheck = RECHECK_BYTES;
        self.exceeded = 0;
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn exceeded(&self) -> bool {
        self.exceeded != 0
    }

    pub fn mark_exceeded(&mut self) {
        self.exceeded = 1;
    }
}

pub struct Captured {
    pub etag: Option<Vec<u8>>,
    pub last_modified: Option<Vec<u8>>,
    pub cache_control: Option<Vec<u8>>,
    pub vary: Option<Vec<u8>>,
    pub expires: Option<Vec<u8>>,
    pub location: Option<Vec<u8>>,
    pub set_cookie_seen: bool,
    raw: *mut GString,
}

impl Captured {
    pub fn take_raw(&mut self) -> Option<*mut c_char> {
        let raw = core::mem::replace(&mut self.raw, ptr::null_mut());
        (!raw.is_null()).then(|| unsafe { g_string_free(raw, 0) })
    }
}

impl Drop for Captured {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { g_string_free(self.raw, 1) };
        }
    }
}

fn take_text(slot: &mut *mut c_char) -> Option<Vec<u8>> {
    let value = unsafe { glib::bytes(*slot) }.map(<[u8]>::to_vec);
    unsafe { glib::g_free((*slot).cast()) };
    *slot = ptr::null_mut();
    value
}

impl NsHeaderCtx {
    pub fn new(outputs: [*mut *mut c_char; 8]) -> NsHeaderCtx {
        let [
            content_type_out,
            content_disposition_out,
            csp_out,
            xframe_options_out,
            x_content_type_options_out,
            cors_allow_origin_out,
            refresh_out,
            content_language_out,
        ] = outputs;
        NsHeaderCtx {
            content_type_out,
            content_disposition_out,
            csp_out,
            xframe_options_out,
            x_content_type_options_out,
            cors_allow_origin_out,
            refresh_out,
            content_language_out,
            etag: ptr::null_mut(),
            last_modified: ptr::null_mut(),
            cache_control: ptr::null_mut(),
            vary: ptr::null_mut(),
            expires: ptr::null_mut(),
            location: ptr::null_mut(),
            raw: ptr::null_mut(),
            set_cookie_seen: 0,
        }
    }

    pub fn has_location(&self) -> bool {
        unsafe { glib::bytes(self.location) }.is_some_and(|l| !l.is_empty())
    }

    pub fn take(&mut self) -> Captured {
        Captured {
            etag: take_text(&mut self.etag),
            last_modified: take_text(&mut self.last_modified),
            cache_control: take_text(&mut self.cache_control),
            vary: take_text(&mut self.vary),
            expires: take_text(&mut self.expires),
            location: take_text(&mut self.location),
            set_cookie_seen: self.set_cookie_seen != 0,
            raw: core::mem::replace(&mut self.raw, ptr::null_mut()),
        }
    }
}

unsafe extern "C" {
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_byte_array_set_size(array: *mut GByteArray, length: c_uint) -> *mut GByteArray;
    fn g_string_free(string: *mut GString, free_segment: GBoolean) -> *mut c_char;
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_set_size(string: *mut GString, len: usize) -> *mut GString;
    fn g_string_append_len(string: *mut GString, val: *const c_char, len: isize) -> *mut GString;
}

pub fn gstring_append(out: *mut GString, bytes: &[u8]) {
    unsafe { g_string_append_len(out, bytes.as_ptr().cast(), bytes.len() as isize) };
}

fn write(ctx: &mut NsWriteCtx, data: &[u8]) -> usize {
    let bytes = data.len() as u64;
    if bytes == 0 || bytes > u64::from(u32::MAX) {
        return 0;
    }
    if ctx.total >= ctx.next_recheck {
        ctx.budget = crate::transport::response_budget();
        ctx.next_recheck = ctx.total + RECHECK_BYTES;
    }
    if ctx.total + bytes > ctx.budget || ctx.total + bytes > u64::from(u32::MAX) {
        ctx.exceeded = 1;
        return 0;
    }
    unsafe { g_byte_array_append(ctx.body, data.as_ptr(), bytes as c_uint) };
    ctx.total += bytes;
    data.len()
}

fn total_size(size: usize, count: usize) -> Option<usize> {
    size.checked_mul(count)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_write_cb(
    data: *mut c_char,
    size: usize,
    nmemb: usize,
    userdata: *mut c_void,
) -> usize {
    let Some(bytes) = total_size(size, nmemb) else {
        return 0;
    };
    let ctx = unsafe { &mut *userdata.cast::<NsWriteCtx>() };
    write(ctx, unsafe { glib::slice(data.cast(), bytes) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_body_sink_init(ctx: *mut NsWriteCtx, body: *mut GByteArray) {
    unsafe { ctx.write(NsWriteCtx::new(body)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_body_sink_write(
    ctx: *mut NsWriteCtx,
    data: *const c_void,
    len: usize,
) -> GBoolean {
    if len == 0 {
        return 1;
    }
    let ctx = unsafe { &mut *ctx };
    glib::boolean(write(ctx, unsafe { glib::slice(data.cast(), len) }) == len)
}

fn header_value(line: &[u8], prefix_len: usize) -> Vec<u8> {
    let mut v = &line[prefix_len..];
    while let [b' ' | b'\t', rest @ ..] = v {
        v = rest;
    }
    while let [rest @ .., b'\r' | b'\n' | b' ' | b'\t'] = v {
        v = rest;
    }
    v[..v.iter().position(|&b| b == 0).unwrap_or(v.len())].to_vec()
}

fn has_name(line: &[u8], name: &[u8]) -> bool {
    line.len() >= name.len() && line[..name.len()].eq_ignore_ascii_case(name)
}

unsafe fn capture(line: &[u8], name: &[u8], slot: *mut *mut c_char) -> bool {
    if !has_name(line, name) {
        return false;
    }
    if let Some(slot) = unsafe { slot.as_mut() } {
        unsafe { glib::g_free((*slot).cast()) };
        *slot = glib::strdup(&header_value(line, name.len()));
    }
    true
}

unsafe fn append(line: &[u8], name: &[u8], slot: *mut *mut c_char) -> bool {
    if !has_name(line, name) {
        return false;
    }
    if let Some(slot) = unsafe { slot.as_mut() } {
        let value = header_value(line, name.len());
        let current = unsafe { glib::bytes(*slot) }.filter(|c| !c.is_empty());
        match current {
            Some(current) if !value.is_empty() => {
                let joined = [current, b", ", &value].concat();
                unsafe { glib::g_free((*slot).cast()) };
                *slot = glib::strdup(&joined);
            }
            _ if !value.is_empty() => {
                unsafe { glib::g_free((*slot).cast()) };
                *slot = glib::strdup(&value);
            }
            _ => {}
        }
    }
    true
}

fn keep_raw(hc: &mut NsHeaderCtx, line: &[u8]) {
    if has_name(line, b"HTTP/") {
        if !hc.raw.is_null() {
            unsafe { g_string_set_size(hc.raw, 0) };
        }
        return;
    }
    if line.len() <= 2 || has_name(line, b"Set-Cookie:") || has_name(line, b"Set-Cookie2:") {
        return;
    }
    if hc.raw.is_null() {
        hc.raw = unsafe { g_string_new(ptr::null()) };
    }
    if unsafe { (*hc.raw).len } + line.len() <= MAX_RAW_HEADER_BYTES {
        gstring_append(hc.raw, line);
    }
}

fn feed(hc: &mut NsHeaderCtx, line: &[u8]) {
    keep_raw(hc, line);
    let captured = unsafe {
        capture(line, b"Content-Type:", hc.content_type_out)
            || capture(line, b"ETag:", &mut hc.etag)
            || capture(line, b"Last-Modified:", &mut hc.last_modified)
            || capture(line, b"Cache-Control:", &mut hc.cache_control)
            || capture(line, b"Vary:", &mut hc.vary)
            || capture(line, b"Expires:", &mut hc.expires)
            || append(line, b"Content-Security-Policy:", hc.csp_out)
            || capture(line, b"X-Frame-Options:", hc.xframe_options_out)
            || capture(
                line,
                b"X-Content-Type-Options:",
                hc.x_content_type_options_out,
            )
            || capture(
                line,
                b"Access-Control-Allow-Origin:",
                hc.cors_allow_origin_out,
            )
            || capture(line, b"Content-Disposition:", hc.content_disposition_out)
            || capture(line, b"Content-Language:", hc.content_language_out)
            || capture(line, b"Refresh:", hc.refresh_out)
            || capture(line, b"Location:", &mut hc.location)
    };
    if !captured && has_name(line, b"Set-Cookie:") {
        hc.set_cookie_seen = 1;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_header_cb(
    buffer: *mut c_char,
    size: usize,
    nitems: usize,
    userdata: *mut c_void,
) -> usize {
    let Some(bytes) = total_size(size, nitems) else {
        return 0;
    };
    let hc = unsafe { &mut *userdata.cast::<NsHeaderCtx>() };
    feed(hc, unsafe { glib::slice(buffer.cast(), bytes) });
    bytes
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_header_sink_feed(
    ctx: *mut NsHeaderCtx,
    line: *const c_char,
    len: usize,
) {
    if line.is_null() || len == 0 {
        return;
    }
    feed(unsafe { &mut *ctx }, unsafe {
        glib::slice(line.cast(), len)
    });
}
