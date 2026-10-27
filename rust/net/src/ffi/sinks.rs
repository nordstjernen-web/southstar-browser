//! Southstar — the body and header sinks the transport writes into: the budgeted body append, and the header lines that capture the headers the fetch path reads and hand them over.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::response::{Response, cstr};

const RECHECK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RAW_HEADER_BYTES: usize = 1024 * 1024;

pub struct NsWriteCtx {
    body: Vec<u8>,
    total: u64,
    budget: u64,
    limit: Option<u64>,
    next_recheck: u64,
    exceeded: bool,
}

#[derive(Default)]
pub struct NsHeaderCtx {
    content_type: Option<CString>,
    content_disposition: Option<CString>,
    csp: Option<CString>,
    xframe_options: Option<CString>,
    x_content_type_options: Option<CString>,
    cors_allow_origin: Option<CString>,
    refresh: Option<CString>,
    content_language: Option<CString>,
    etag: Option<Vec<u8>>,
    last_modified: Option<Vec<u8>>,
    cache_control: Option<Vec<u8>>,
    vary: Option<Vec<u8>>,
    expires: Option<Vec<u8>>,
    location: Option<Vec<u8>>,
    raw: Option<Vec<u8>>,
    set_cookie_seen: bool,
}

impl NsWriteCtx {
    pub fn new(limit: Option<u64>) -> NsWriteCtx {
        NsWriteCtx {
            body: Vec::new(),
            total: 0,
            budget: capped(crate::transport::response_budget(), limit),
            limit,
            next_recheck: RECHECK_BYTES,
            exceeded: false,
        }
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn exceeded(&self) -> bool {
        self.exceeded
    }

    pub fn mark_exceeded(&mut self) {
        self.exceeded = true;
    }

    pub fn take_body(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.body)
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
    pub fn take_raw(&mut self) -> Option<CString> {
        self.raw.take().map(|raw| cstr(&raw))
    }
}

impl NsHeaderCtx {
    pub fn new() -> NsHeaderCtx {
        NsHeaderCtx::default()
    }

    pub fn store(&mut self, resp: &mut Response) {
        resp.content_type = self.content_type.take();
        resp.content_disposition = self.content_disposition.take();
        resp.csp_header = self.csp.take();
        resp.xframe_options = self.xframe_options.take();
        resp.x_content_type_options = self.x_content_type_options.take();
        resp.cors_allow_origin = self.cors_allow_origin.take();
        resp.refresh = self.refresh.take();
        resp.content_language = self.content_language.take();
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

fn capped(budget: u64, limit: Option<u64>) -> u64 {
    limit.map_or(budget, |limit| budget.min(limit))
}

pub fn write(ctx: &mut NsWriteCtx, data: &[u8]) -> usize {
    let bytes = data.len() as u64;
    if bytes == 0 || bytes > u64::from(u32::MAX) {
        return 0;
    }
    if ctx.total >= ctx.next_recheck {
        ctx.budget = capped(crate::transport::response_budget(), ctx.limit);
        ctx.next_recheck = ctx.total + RECHECK_BYTES;
    }
    if ctx.total + bytes > ctx.budget || ctx.total + bytes > u64::from(u32::MAX) {
        ctx.exceeded = true;
        return 0;
    }
    ctx.body.extend_from_slice(data);
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

fn capture(line: &[u8], name: &[u8], slot: &mut Option<CString>) -> bool {
    if !has_name(line, name) {
        return false;
    }
    *slot = Some(cstr(&header_value(line, name.len())));
    true
}

fn append(line: &[u8], name: &[u8], slot: &mut Option<CString>) -> bool {
    if !has_name(line, name) {
        return false;
    }
    let value = header_value(line, name.len());
    if value.is_empty() {
        return true;
    }
    let current = slot
        .as_deref()
        .map(|c| c.to_bytes())
        .filter(|c| !c.is_empty());
    *slot = Some(match current {
        Some(current) => cstr(&[current, b", ", &value].concat()),
        None => cstr(&value),
    });
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
    let captured = capture(line, b"Content-Type:", &mut hc.content_type)
        || keep(line, b"ETag:", &mut hc.etag)
        || keep(line, b"Last-Modified:", &mut hc.last_modified)
        || keep(line, b"Cache-Control:", &mut hc.cache_control)
        || keep(line, b"Vary:", &mut hc.vary)
        || keep(line, b"Expires:", &mut hc.expires)
        || append(line, b"Content-Security-Policy:", &mut hc.csp)
        || capture(line, b"X-Frame-Options:", &mut hc.xframe_options)
        || capture(
            line,
            b"X-Content-Type-Options:",
            &mut hc.x_content_type_options,
        )
        || capture(
            line,
            b"Access-Control-Allow-Origin:",
            &mut hc.cors_allow_origin,
        )
        || capture(line, b"Content-Disposition:", &mut hc.content_disposition)
        || capture(line, b"Content-Language:", &mut hc.content_language)
        || capture(line, b"Refresh:", &mut hc.refresh)
        || keep(line, b"Location:", &mut hc.location);
    if !captured && has_name(line, b"Set-Cookie:") {
        hc.set_cookie_seen = true;
    }
}
