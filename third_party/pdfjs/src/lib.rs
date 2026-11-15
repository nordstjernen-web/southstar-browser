//! Southstar — the vendored pdf.js files by their path in the package, with their media types.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

mod files;

pub const VERSION: &str = "6.4.299";

pub struct File {
    pub bytes: &'static [u8],
    pub content_type: &'static str,
}

fn content_type(path: &str) -> &'static str {
    let extension = path.rsplit_once('.').map_or("", |(_, extension)| extension);
    match extension {
        "mjs" | "js" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "ttf" => "font/ttf",
        "icc" => "application/vnd.iccprofile",
        _ => "application/octet-stream",
    }
}

pub fn file(path: &str) -> Option<File> {
    files::FILES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|&(name, bytes)| File {
            bytes,
            content_type: content_type(name),
        })
}
