//! Southstar — the network modules behind the ported C ABI, and the error pages, about:, data:, file:, FTP listing and view-source: responses synthesized without the network.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod fetch;
pub mod forms;
pub mod hop;
pub mod host;
pub mod netlog;
pub mod proxy;
pub mod queue;
pub mod sinks;
pub mod storage;
pub mod sys;
pub mod transport;
pub mod url;

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use crate::budget::{Budgeted, Sink, exhausted_error};
use crate::data_url::{self, Failure};
use crate::response::{Response, cstr, text};
use crate::{about, file, ftp, view_source};

pub use sys::utf8_collate;

struct VecSink<'a>(&'a mut Vec<u8>);

impl Sink for VecSink<'_> {
    fn append(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

pub fn decode_data_url(url: &[u8], budget: u64) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    let mut sink = VecSink(&mut body);
    let mut budgeted = Budgeted::new(&mut sink, budget);
    data_url::decode(url, &mut budgeted)?.result.ok()?;
    Some(body)
}

pub fn synthesize_data_response(url: &[u8], resp: &mut Response) -> bool {
    if !url.starts_with(b"data:") {
        return false;
    }
    let budget = crate::transport::response_budget();
    let body_start = resp.body.len();
    let mut sink = VecSink(&mut resp.body);
    let mut budgeted = Budgeted::new(&mut sink, budget);
    let Some(decoded) = data_url::decode(url, &mut budgeted) else {
        return false;
    };
    if let Err(failure) = decoded.result {
        resp.body.truncate(body_start);
        resp.error = Some(match failure {
            Failure::TooLarge => cstr(&exhausted_error(budget)),
            Failure::Malformed => cstr(b"malformed data: URL"),
        });
    }
    resp.status = 200;
    resp.final_url = Some(cstr(url));
    resp.content_type = Some(cstr(&decoded.content_type));
    true
}

pub fn set_allow_file_urls(allow: bool) {
    file::ALLOW_FILE_URLS.store(allow, Ordering::Relaxed);
}

pub fn synthesize_file_response(url: &[u8], top_url: Option<&[u8]>, resp: &mut Response) -> bool {
    let mut sink = VecSink(&mut resp.body);
    let budget = crate::transport::response_budget;
    let Some(out) = file::respond(url, top_url, &mut sink, budget) else {
        return false;
    };
    resp.final_url = Some(cstr(&out.final_url));
    resp.status = out.status;
    if let Some(error) = out.error {
        resp.error = Some(cstr(&error));
    }
    if let Some(content_type) = out.content_type {
        resp.content_type = Some(cstr(&content_type));
    }
    true
}

pub fn finish_ftp_response(resp: &mut Response) {
    if ftp::looks_like_directory(text(&resp.final_url)) {
        let html = ftp::directory_page(text(&resp.final_url), &resp.body);
        resp.body.clear();
        resp.body.extend_from_slice(&html);
        resp.content_type = Some(cstr(b"text/html; charset=utf-8"));
    }
    let Some(final_url) = text(&resp.final_url) else {
        return;
    };
    if resp.content_type.is_some()
        || !final_url.starts_with(b"ftp://")
        || ftp::looks_like_directory(Some(final_url))
    {
        return;
    }
    let data = (!resp.body.is_empty()).then_some(&resp.body[..]);
    let mime = sys::mime_type_guess(&ftp::guess_path(final_url), data);
    resp.content_type = Some(cstr(&mime));
}

pub fn synthesize_about_response(
    url: &[u8],
    top_url: Option<&[u8]>,
    method: Option<&[u8]>,
    req_body: Option<&[u8]>,
    resp: &mut Response,
) -> bool {
    let Some(page) = about::respond(url, top_url, method, req_body) else {
        return false;
    };
    resp.status = page.status;
    resp.final_url = Some(cstr(url));
    resp.content_type = Some(cstr(page.content_type));
    resp.body.extend_from_slice(&page.body);
    true
}

pub fn synthesize_view_source_response(
    url: &[u8],
    top_url: Option<&[u8]>,
    cancellable: *mut c_void,
    resp: &mut Response,
) -> bool {
    let Some(inner) = url.strip_prefix(view_source::PREFIX) else {
        return false;
    };
    resp.final_url = Some(cstr(url));
    if !view_source::allowed(top_url, inner) {
        resp.status = 403;
        resp.content_type = Some(cstr(b"text/plain; charset=utf-8"));
        resp.body
            .extend_from_slice(b"view-source: is not available here");
        return true;
    }
    let fetched = queue::blocking(
        &queue::FetchRequest {
            url: inner,
            method: Some(b"GET"),
            ..queue::FetchRequest::default()
        },
        cancellable,
    );
    let page = match &fetched {
        Ok(r) if r.error.is_none() && !(r.status >= 400 && r.body.is_empty()) => {
            let (decoded, _) = southstar_html_util::decode_body(&r.body, text(&r.content_type));
            let source = decoded.split(|&b| b == 0).next().unwrap_or_default();
            view_source::document(inner, source)
        }
        Ok(r) => {
            let message: &[u8] = match text(&r.error) {
                Some(error) => error,
                None => b"the server returned an error",
            };
            view_source::error_document(inner, message)
        }
        Err(failure) => {
            let message = cstr(failure.message());
            view_source::error_document(inner, message.to_bytes())
        }
    };
    resp.status = 200;
    resp.content_type = Some(cstr(b"text/html; charset=utf-8"));
    resp.body.extend_from_slice(&page);
    true
}
