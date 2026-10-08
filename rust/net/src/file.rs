//! Southstar — file: URLs: the file's bytes under the response budget, or an "Index of" page for a folder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_long;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::budget::{Budgeted, Sink, exhausted_error};
use crate::error_page;
use crate::ffi::sys;
use crate::listing::{self, Entry};

pub static ALLOW_FILE_URLS: AtomicBool = AtomicBool::new(false);

const SEPARATOR: &[u8] = if cfg!(windows) { b"\\" } else { b"/" };
const READ_CHUNK: usize = 65536;

pub struct Response {
    pub final_url: Vec<u8>,
    pub status: c_long,
    pub error: Option<Vec<u8>>,
    pub content_type: Option<Vec<u8>>,
}

fn access_allowed(top_url: Option<&[u8]>) -> bool {
    ALLOW_FILE_URLS.load(Ordering::Relaxed)
        && top_url.is_none_or(|top| top.is_empty() || top.starts_with(b"file:"))
}

fn uri_for_path(path: &[u8], is_dir: bool) -> Option<Vec<u8>> {
    if path.is_empty() {
        return None;
    }
    let mut path = path.to_vec();
    if is_dir && !path.ends_with(SEPARATOR) {
        path.extend_from_slice(SEPARATOR);
    }
    sys::filename_to_uri(&path)
}

fn parent_uri(path: &[u8]) -> Option<Vec<u8>> {
    if path.is_empty() {
        return None;
    }
    let parent = sys::path_dirname(path);
    if parent.is_empty() || parent == b"." || parent == path {
        return None;
    }
    uri_for_path(&parent, true)
}

fn directory_listing(path: &[u8], url: &[u8]) -> (Vec<u8>, c_long) {
    let names = match sys::read_dir(path) {
        Ok(names) => names,
        Err(err) => {
            let status = if err.denied { 403 } else { 404 };
            let message = err
                .message
                .unwrap_or_else(|| b"cannot read folder".to_vec());
            return (error_page::build(Some(url), status, Some(&message)), status);
        }
    };
    let mut entries = Vec::new();
    for name in names {
        let full = sys::build_filename(path, &name);
        let display = sys::filename_display_name(&name);
        let is_dir = sys::is_dir(&full);
        let stat = sys::stat(&full);
        let Some(uri) = uri_for_path(&full, is_dir) else {
            continue;
        };
        let size = if is_dir {
            Vec::new()
        } else {
            listing::size_label(stat.map_or(0, |s| s.size.max(0) as u64))
        };
        let date = stat
            .filter(|s| s.mtime > 0)
            .and_then(|s| sys::local_time_label(s.mtime))
            .unwrap_or_default();
        entries.push(Entry {
            display,
            uri,
            size,
            date,
            is_dir,
        });
    }
    listing::sort(&mut entries);
    let title = sys::filename_display_name(path);
    let parent = parent_uri(path);
    (listing::page(&title, parent.as_deref(), &entries), 200)
}

fn read_file<S: Sink>(path: &[u8], body: &mut S, budget: u64) -> Result<(), Vec<u8>> {
    if let Some(stat) = sys::stat(path) {
        let size = stat.size.max(0) as u64;
        if size > 0 && (size > budget || size > u64::from(u32::MAX)) {
            return Err(exhausted_error(size));
        }
    }
    let mut file = sys::CFile::open(path)?;
    let mut buf = vec![0u8; READ_CHUNK];
    let mut out = Budgeted::new(body, budget);
    loop {
        let n = file.read(&mut buf);
        if n > 0 && !out.append(&buf[..n]) {
            return Err(exhausted_error(out.total()));
        }
        if n < buf.len() {
            return match file.error() {
                Some(err) => Err(err),
                None => Ok(()),
            };
        }
    }
}

pub fn respond<S: Sink>(
    url: &[u8],
    top_url: Option<&[u8]>,
    body: &mut S,
    budget: impl FnOnce() -> u64,
) -> Option<Response> {
    if !url.starts_with(b"file:") {
        return None;
    }
    if !access_allowed(top_url) {
        return Some(Response {
            final_url: url.to_vec(),
            status: 0,
            error: Some(b"local file access is not allowed from a remote page".to_vec()),
            content_type: None,
        });
    }
    let Some(path) = sys::filename_from_uri(url).map(|p| sys::canonicalize_filename(&p)) else {
        return Some(Response {
            final_url: url.to_vec(),
            status: 400,
            error: Some(b"invalid file URL".to_vec()),
            content_type: None,
        });
    };
    if sys::is_dir(&path) {
        let final_url = uri_for_path(&path, true).unwrap_or_else(|| url.to_vec());
        let (html, status) = directory_listing(&path, &final_url);
        body.append(&html);
        return Some(Response {
            final_url,
            status,
            error: None,
            content_type: Some(b"text/html; charset=utf-8".to_vec()),
        });
    }
    let (status, error) = match read_file(&path, body, budget()) {
        Ok(()) => (200, None),
        Err(err) => (
            if err.starts_with(b"response would exhaust") {
                0
            } else {
                404
            },
            Some(err),
        ),
    };
    Some(Response {
        final_url: url.to_vec(),
        status,
        error,
        content_type: Some(sys::mime_type_guess(&path, None)),
    })
}
