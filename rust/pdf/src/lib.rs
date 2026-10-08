//! Southstar — PDF documents rendered to an inline HTML page of page images.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

const STYLE: &str = "<style>\
html,body{margin:0;background:#1c1d1e}\
.doc{max-width:900px;margin:0 auto;padding:24px 16px}\
.page{background:#fff;margin:0 auto 24px;\
box-shadow:0 4px 12px rgba(0,0,0,.45);border:1px solid #444}\
.page:last-child{margin-bottom:0}\
.page img{display:block;width:100%;height:auto}\
.note{color:#bbb;font:14px/1.6 system-ui,sans-serif;\
text-align:center;padding:48px 16px}\
.note a{color:#6ab7ff}\
</style>";

#[cfg_attr(not(feature = "poppler"), allow(dead_code))]
const TARGET_W: f64 = 1240.0;
#[cfg_attr(not(feature = "poppler"), allow(dead_code))]
const MAX_PAGES: i32 = 300;

pub fn notice_html(url: Option<&[u8]>, message: &[u8]) -> Vec<u8> {
    let url = ffi::html_escape(url.unwrap_or_default());
    let message = ffi::html_escape(message);
    [
        b"<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>PDF</title>".as_slice(),
        STYLE.as_bytes(),
        b"</head><body><div class=\"doc\"><p class=\"note\">",
        &message,
        b"<br><a href=\"",
        &url,
        b"\">",
        &url,
        b"</a></p></div></body></html>",
    ]
    .concat()
}

fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x > hi {
        hi
    } else if x < lo {
        lo
    } else {
        x
    }
}

#[cfg_attr(not(feature = "poppler"), allow(dead_code))]
pub fn render_size(pw: f64, ph: f64) -> Option<(f64, i32, i32)> {
    if !pw.is_finite() || !ph.is_finite() || pw <= 0.0 || ph <= 0.0 {
        return None;
    }
    let scale = clamp(TARGET_W / pw, 0.5, 4.0);
    let w = clamp(pw * scale + 0.5, 1.0, 4000.0) as i32;
    let h = clamp(ph * scale + 0.5, 1.0, 4000.0) as i32;
    Some((scale, w, h))
}

#[cfg_attr(not(feature = "poppler"), allow(dead_code))]
fn document_name(url: Option<&[u8]>) -> Vec<u8> {
    let name = url.map(ffi::path_basename).map(|mut name| {
        if let Some(query) = name.iter().position(|&c| c == b'?') {
            name.truncate(query);
        }
        name
    });
    match name {
        Some(name) if !name.is_empty() => ffi::html_escape(&name),
        _ => ffi::html_escape(b"PDF"),
    }
}

#[cfg(feature = "poppler")]
pub fn document_html(data: &[u8], url: Option<&[u8]>) -> Vec<u8> {
    if data.is_empty() {
        return notice_html(url, b"This document could not be opened.");
    }
    let doc = match ffi::Document::open(data) {
        Ok(doc) => doc,
        Err(error) => {
            let message = match error {
                Some(error) => [
                    b"This PDF could not be displayed: ".as_slice(),
                    &error,
                    b".",
                ]
                .concat(),
                None => b"This PDF could not be displayed.".to_vec(),
            };
            return notice_html(url, &message);
        }
    };
    let n = doc.pages();
    if n <= 0 {
        return notice_html(url, b"This PDF has no pages.");
    }
    let mut out = [
        b"<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>".as_slice(),
        &document_name(url),
        b"</title>",
        STYLE.as_bytes(),
        b"</head><body><div class=\"doc\">",
    ]
    .concat();
    let limit = n.min(MAX_PAGES);
    for i in 0..limit {
        let Some(uri) = doc.page_data_uri(i) else {
            continue;
        };
        out.extend_from_slice(b"<div class=\"page\"><img src=\"");
        out.extend_from_slice(&uri);
        out.extend_from_slice(format!("\" alt=\"Page {}\"></div>", i + 1).as_bytes());
    }
    if limit < n {
        out.extend_from_slice(
            format!("<p class=\"note\">Showing the first {limit} of {n} pages.</p>").as_bytes(),
        );
    }
    out.extend_from_slice(b"</div></body></html>");
    out
}

#[cfg(not(feature = "poppler"))]
pub fn document_html(_data: &[u8], url: Option<&[u8]>) -> Vec<u8> {
    notice_html(url, b"Inline PDF viewing is not available in this build.")
}
