//! Southstar — the "Index of" page listing a local folder or an FTP directory, folders first.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use southstar_html_util::escape_text;

pub struct Entry {
    pub display: Vec<u8>,
    pub uri: Vec<u8>,
    pub size: Vec<u8>,
    pub date: Vec<u8>,
    pub is_dir: bool,
}

const HEAD: &str = concat!(
    "</title><style>",
    "body{font-family:serif;margin:8px;color:#000;background:#fff}",
    "h1{font-size:2em;margin:.4em 0}",
    "table{border-collapse:collapse}",
    "th{text-align:left;font-weight:bold;padding:0 3em .25em 0}",
    "td{padding:0 3em 0 0;white-space:nowrap}",
    "td.name{min-width:22em}",
    "td.size{text-align:right}",
    ".ico{display:inline-block;width:1.35em;margin-right:.35em;",
    "text-align:center;font-family:\"Segoe UI Emoji\",\"Apple Color Emoji\",",
    "\"Noto Color Emoji\",sans-serif}",
    "a{color:#00e;text-decoration:underline}",
    "hr{border:0;border-top:1px solid #bbb;margin:.6em 0}",
    "</style></head><body><h1>Index of "
);

const TABLE: &str = concat!(
    "</h1><hr><table><thead><tr>",
    "<th>Name</th><th>Size</th><th>Date modified</th>",
    "</tr></thead><tbody>"
);

const FOLDER_ICON: &str = "&#128193;";
const FILE_ICON: &str = "&#128196;";

pub fn size_label(size: u64) -> Vec<u8> {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let label = if unit == 0 {
        format!("{size} B")
    } else if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    };
    label.into_bytes()
}

pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => crate::ffi::utf8_collate(&a.display, &b.display),
    });
}

pub fn page(title: &[u8], parent: Option<&[u8]>, entries: &[Entry]) -> Vec<u8> {
    let title = escape_text(title);
    let mut out = Vec::new();
    out.extend_from_slice(b"<!doctype html><html><head><meta charset=\"utf-8\"><title>Index of ");
    out.extend_from_slice(&title);
    out.extend_from_slice(HEAD.as_bytes());
    out.extend_from_slice(&title);
    out.extend_from_slice(TABLE.as_bytes());
    if let Some(parent) = parent {
        out.extend_from_slice(
            b"<tr><td class=\"name\"><span class=\"ico dir\" aria-hidden=\"true\">",
        );
        out.extend_from_slice(FOLDER_ICON.as_bytes());
        out.extend_from_slice(b"</span><a href=\"");
        out.extend_from_slice(&escape_text(parent));
        out.extend_from_slice(b"\">../</a></td><td class=\"size\"></td><td></td></tr>");
    }
    for e in entries {
        out.extend_from_slice(b"<tr><td class=\"name\"><span class=\"ico ");
        out.extend_from_slice(if e.is_dir { b"dir" } else { b"file" });
        out.extend_from_slice(b"\" aria-hidden=\"true\">");
        out.extend_from_slice(if e.is_dir { FOLDER_ICON } else { FILE_ICON }.as_bytes());
        out.extend_from_slice(b"</span><a href=\"");
        out.extend_from_slice(&escape_text(&e.uri));
        out.extend_from_slice(b"\">");
        out.extend_from_slice(&escape_text(&e.display));
        if e.is_dir {
            out.push(b'/');
        }
        out.extend_from_slice(b"</a></td><td class=\"size\">");
        out.extend_from_slice(&escape_text(&e.size));
        out.extend_from_slice(b"</td><td>");
        out.extend_from_slice(&escape_text(&e.date));
        out.extend_from_slice(b"</td></tr>");
    }
    out.extend_from_slice(b"</tbody></table><hr></body></html>");
    out
}
