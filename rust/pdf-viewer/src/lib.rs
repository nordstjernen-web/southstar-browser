//! Southstar — PDF documents turned into a viewer page that runs pdf.js on the bytes, and the southstar-pdf: files that page loads.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use southstar_html_util::escape_text;

pub const SCHEME: &[u8] = b"southstar-pdf:";
const ORIGIN: &[u8] = b"southstar-pdf://viewer/";

const TEMPLATE: &str = include_str!("../viewer/viewer.html");
const VIEWER_FILES: [(&str, &[u8], &str); 2] = [
    (
        "viewer/viewer.mjs",
        include_bytes!("../viewer/viewer.mjs"),
        "text/javascript; charset=utf-8",
    ),
    (
        "viewer/viewer.css",
        include_bytes!("../viewer/viewer.css"),
        "text/css; charset=utf-8",
    ),
];

pub struct Resource {
    pub body: &'static [u8],
    pub content_type: &'static str,
}

fn path_of(url: &[u8]) -> Option<&str> {
    let rest = url.strip_prefix(ORIGIN)?;
    let end = rest
        .iter()
        .position(|&c| c == b'?' || c == b'#')
        .unwrap_or(rest.len());
    core::str::from_utf8(&rest[..end]).ok()
}

pub fn resource(url: &[u8]) -> Option<Resource> {
    let path = path_of(url)?;
    if let Some(&(_, body, content_type)) = VIEWER_FILES.iter().find(|(name, ..)| *name == path) {
        return Some(Resource { body, content_type });
    }
    let file = southstar_pdfjs::file(path.strip_prefix("pdfjs/")?)?;
    Some(Resource {
        body: file.bytes,
        content_type: file.content_type,
    })
}

fn starts_ci(text: &[u8], prefix: &[u8]) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub fn is_pdf(content_type: Option<&[u8]>, body: &[u8]) -> bool {
    match content_type.map(<[u8]>::trim_ascii) {
        Some(ct) if starts_ci(ct, b"application/pdf") || starts_ci(ct, b"application/x-pdf") => {
            true
        }
        Some(ct)
            if !ct.is_empty()
                && !starts_ci(ct, b"application/octet-stream")
                && !starts_ci(ct, b"binary/octet-stream") =>
        {
            false
        }
        _ => body.starts_with(b"%PDF-"),
    }
}

fn document_name(url: &[u8]) -> Vec<u8> {
    let end = url
        .iter()
        .position(|&c| c == b'?' || c == b'#')
        .unwrap_or(url.len());
    let path = &url[..end];
    let start = path.iter().rposition(|&c| c == b'/').map_or(0, |i| i + 1);
    let name = &path[start..];
    if name.is_empty() {
        b"document.pdf".to_vec()
    } else {
        name.to_vec()
    }
}

fn fill(template: &str, fields: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(template.len() + fields.iter().map(|f| f.1.len()).sum::<usize>());
    let mut rest = template.as_bytes();
    while let Some(open) = rest.windows(2).position(|w| w == b"{{") {
        out.extend_from_slice(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.windows(2).position(|w| w == b"}}") else {
            rest = &rest[open..];
            break;
        };
        let key = &after[..close];
        match fields.iter().find(|(name, _)| name.as_bytes() == key) {
            Some((_, value)) => out.extend_from_slice(value),
            None => out.extend_from_slice(&rest[open..open + 4 + close]),
        }
        rest = &after[close + 2..];
    }
    out.extend_from_slice(rest);
    out
}

pub fn document_html(data: &[u8], url: &[u8]) -> Vec<u8> {
    let name = escape_text(&document_name(url));
    let href = escape_text(url);
    let encoded = southstar_digest::base64(data);
    let holder = [
        b"<script type=\"application/pdf;base64\" id=\"pdf-data\">".as_slice(),
        encoded.as_bytes(),
        b"</script>",
    ]
    .concat();
    fill(
        TEMPLATE,
        &[
            ("TITLE", &name),
            ("NAME", &name),
            ("URL", &href),
            ("BASE", ORIGIN),
            ("DATA", &holder),
        ],
    )
}
