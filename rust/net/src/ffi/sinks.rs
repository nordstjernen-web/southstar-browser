//! Southstar — the body and header sinks the transport writes into: the budgeted body append, and the header lines that capture the headers the fetch path reads and hand them over.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint};

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

pub struct NsHeaderCtx {
    content_type_out: *mut *mut c_char,
    content_disposition_out: *mut *mut c_char,
    csp_out: *mut *mut c_char,
    xframe_options_out: *mut *mut c_char,
    x_content_type_options_out: *mut *mut c_char,
    cors_allow_origin_out: *mut *mut c_char,
    refresh_out: *mut *mut c_char,
    content_language_out: *mut *mut c_char,
    etag: Option<Vec<u8>>,
    last_modified: Option<Vec<u8>>,
    cache_control: Option<Vec<u8>>,
    vary: Option<Vec<u8>>,
    expires: Option<Vec<u8>>,
    location: Option<Vec<u8>>,
    raw: Option<Vec<u8>>,
    set_cookie_seen: bool,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<NsWriteCtx>() == 40);

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
    raw: Option<Vec<u8>>,
}

impl Captured {
    pub fn take_raw(&mut self) -> Option<*mut c_char> {
        self.raw.take().map(|raw| glib::strdup(&raw))
    }
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
            etag: None,
            last_modified: None,
            cache_control: None,
            vary: None,
            expires: None,
            location: None,
            raw: None,
            set_cookie_seen: false,
        }
    }

    pub fn take(&mut self) -> Captured {
        Captured {
            etag: self.etag.take(),
            last_modified: self.last_modified.take(),
            cache_control: self.cache_control.take(),
            vary: self.vary.take(),
            expires: self.expires.take(),
            location: self.location.take(),
            set_cookie_seen: self.set_cookie_seen,
            raw: self.raw.take(),
        }
    }
}

unsafe extern "C" {
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
}

pub fn write(ctx: &mut NsWriteCtx, data: &[u8]) -> usize {
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

fn keep(line: &[u8], name: &[u8], slot: &mut Option<Vec<u8>>) -> bool {
    if !has_name(line, name) {
        return false;
    }
    *slot = Some(header_value(line, name.len()));
    true
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
        if let Some(raw) = hc.raw.as_mut() {
            raw.clear();
        }
        return;
    }
    if line.len() <= 2 || has_name(line, b"Set-Cookie:") || has_name(line, b"Set-Cookie2:") {
        return;
    }
    let raw = hc.raw.get_or_insert_with(Vec::new);
    if raw.len() + line.len() <= MAX_RAW_HEADER_BYTES {
        raw.extend_from_slice(line);
    }
}

pub fn feed(hc: &mut NsHeaderCtx, line: &[u8]) {
    keep_raw(hc, line);
    let captured = unsafe {
        capture(line, b"Content-Type:", hc.content_type_out)
            || keep(line, b"ETag:", &mut hc.etag)
            || keep(line, b"Last-Modified:", &mut hc.last_modified)
            || keep(line, b"Cache-Control:", &mut hc.cache_control)
            || keep(line, b"Vary:", &mut hc.vary)
            || keep(line, b"Expires:", &mut hc.expires)
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
            || keep(line, b"Location:", &mut hc.location)
    };
    if !captured && has_name(line, b"Set-Cookie:") {
        hc.set_cookie_seen = true;
    }
}
