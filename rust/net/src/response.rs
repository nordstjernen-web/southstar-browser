//! Southstar — a fetched response: its status, final URL, the headers the browser reads, the body, the timings and the connection's security.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};
use std::ffi::CString;

use southstar_http_cache as http_cache;

pub struct Timing {
    pub request_start_us: i64,
    pub request_start_real_ms: f64,
    pub domain_lookup_ms: f64,
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub pretransfer_ms: f64,
    pub response_start_ms: f64,
    pub response_end_ms: f64,
}

#[derive(Clone, Default)]
pub struct Response {
    pub(crate) status: c_long,
    pub(crate) final_url: Option<CString>,
    pub(crate) content_type: Option<CString>,
    pub(crate) content_disposition: Option<CString>,
    pub(crate) csp_header: Option<CString>,
    pub(crate) xframe_options: Option<CString>,
    pub(crate) x_content_type_options: Option<CString>,
    pub(crate) cors_allow_origin: Option<CString>,
    pub(crate) refresh: Option<CString>,
    pub(crate) content_language: Option<CString>,
    pub(crate) raw_headers: Option<CString>,
    pub(crate) body: Vec<u8>,
    pub(crate) error: Option<CString>,
    pub(crate) tls_warning: Option<CString>,
    pub(crate) remote_ip: Option<CString>,
    pub(crate) next_hop_protocol: Option<CString>,
    pub(crate) request_start_us: i64,
    pub(crate) request_start_real_ms: f64,
    pub(crate) domain_lookup_ms: f64,
    pub(crate) connect_ms: f64,
    pub(crate) tls_ms: f64,
    pub(crate) pretransfer_ms: f64,
    pub(crate) response_start_ms: f64,
    pub(crate) response_end_ms: f64,
    pub(crate) security: c_int,
    pub(crate) redirect_count: c_int,
}

pub fn cstr(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn text(value: &Option<CString>) -> Option<&[u8]> {
    value.as_deref().map(CStr::to_bytes)
}

impl Response {
    pub fn new() -> Response {
        Response::default()
    }

    pub fn status(&self) -> c_long {
        self.status
    }

    pub fn final_url(&self) -> Option<&CStr> {
        self.final_url.as_deref()
    }

    pub fn content_type(&self) -> Option<&CStr> {
        self.content_type.as_deref()
    }

    pub fn cors_allow_origin(&self) -> Option<&CStr> {
        self.cors_allow_origin.as_deref()
    }

    pub fn x_content_type_options(&self) -> Option<&CStr> {
        self.x_content_type_options.as_deref()
    }

    pub fn error(&self) -> Option<&CStr> {
        self.error.as_deref()
    }

    pub fn body(&self) -> Option<&[u8]> {
        Some(&self.body)
    }

    pub fn body_len(&self) -> usize {
        self.body.len()
    }

    pub fn refresh(&self) -> Option<&CStr> {
        self.refresh.as_deref()
    }

    pub fn content_language(&self) -> Option<&CStr> {
        self.content_language.as_deref()
    }

    pub fn csp_header(&self) -> Option<&CStr> {
        self.csp_header.as_deref()
    }

    pub fn raw_headers(&self) -> Option<&CStr> {
        self.raw_headers.as_deref()
    }

    pub fn next_hop_protocol(&self) -> Option<&CStr> {
        self.next_hop_protocol.as_deref()
    }

    pub fn content_disposition(&self) -> Option<&CStr> {
        self.content_disposition.as_deref()
    }

    pub fn xframe_options(&self) -> Option<&CStr> {
        self.xframe_options.as_deref()
    }

    pub fn redirect_count(&self) -> c_int {
        self.redirect_count
    }

    pub fn remote_ip(&self) -> Option<&CStr> {
        self.remote_ip.as_deref()
    }

    pub fn security(&self) -> c_int {
        self.security
    }

    pub fn timing(&self) -> Timing {
        Timing {
            request_start_us: self.request_start_us,
            request_start_real_ms: self.request_start_real_ms,
            domain_lookup_ms: self.domain_lookup_ms,
            connect_ms: self.connect_ms,
            tls_ms: self.tls_ms,
            pretransfer_ms: self.pretransfer_ms,
            response_start_ms: self.response_start_ms,
            response_end_ms: self.response_end_ms,
        }
    }

    pub fn append(&mut self, bytes: &[u8]) {
        self.body.extend_from_slice(bytes);
    }

    pub fn set_status(&mut self, status: c_long) {
        self.status = status;
    }

    pub fn set_error(&mut self, message: &[u8]) {
        self.error = Some(cstr(message));
    }

    pub fn set_cors_allow_origin(&mut self, value: &[u8]) {
        self.cors_allow_origin = Some(cstr(value));
    }

    pub fn set_raw_headers(&mut self, value: &[u8]) {
        self.raw_headers = Some(cstr(value));
    }

    pub fn set_body(&mut self, bytes: &[u8]) {
        self.body.clear();
        self.body.extend_from_slice(bytes);
    }

    pub fn set_content_type(&mut self, value: &CStr) {
        self.content_type = Some(value.to_owned());
    }

    pub fn set_final_url(&mut self, value: &CStr) {
        self.final_url = Some(value.to_owned());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn set_security(&mut self, security: c_int) {
        self.security = security;
    }

    pub(crate) fn with_error(url: &[u8], status: c_long, error: &[u8]) -> Response {
        Response {
            final_url: Some(cstr(url)),
            status,
            error: Some(cstr(error)),
            ..Response::default()
        }
    }

    pub(crate) fn from_cache(entry: &http_cache::Entry) -> Response {
        Response {
            status: entry.status as c_long,
            final_url: entry.final_url.as_deref().map(cstr),
            content_type: entry.content_type.as_deref().map(cstr),
            cors_allow_origin: entry.cors_allow_origin.as_deref().map(cstr),
            body: entry.body.to_vec(),
            ..Response::default()
        }
    }
}
