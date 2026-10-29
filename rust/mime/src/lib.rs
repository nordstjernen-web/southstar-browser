//! Southstar — the media type of a local file or an untyped download, from its file name extension and, failing that, its first bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

const SNIFF_LEN: usize = 4096;

const BY_EXTENSION: &[(&str, &str)] = &[
    ("7z", "application/x-7z-compressed"),
    ("aac", "audio/aac"),
    ("apng", "image/apng"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("bz2", "application/x-bzip2"),
    ("c", "text/x-csrc"),
    ("cjs", "text/javascript"),
    ("css", "text/css"),
    ("csv", "text/csv"),
    ("doc", "application/msword"),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ("epub", "application/epub+zip"),
    ("flac", "audio/flac"),
    ("gif", "image/gif"),
    ("gz", "application/gzip"),
    ("h", "text/x-chdr"),
    ("htm", "text/html"),
    ("html", "text/html"),
    ("ico", "image/vnd.microsoft.icon"),
    ("ics", "text/calendar"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("js", "text/javascript"),
    ("json", "application/json"),
    ("jxl", "image/jxl"),
    ("m1v", "video/mpeg"),
    ("m4a", "audio/mp4"),
    ("m4v", "video/mp4"),
    ("md", "text/markdown"),
    ("mjs", "text/javascript"),
    ("mkv", "video/x-matroska"),
    ("mov", "video/quicktime"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("mpeg", "video/mpeg"),
    ("mpg", "video/mpeg"),
    ("oga", "audio/ogg"),
    ("ogg", "audio/ogg"),
    ("ogv", "video/ogg"),
    ("opus", "audio/ogg"),
    ("otf", "font/otf"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("rs", "text/rust"),
    ("rtf", "application/rtf"),
    ("svg", "image/svg+xml"),
    ("svgz", "image/svg+xml"),
    ("tar", "application/x-tar"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("ttf", "font/ttf"),
    ("txt", "text/plain"),
    ("wasm", "application/wasm"),
    ("wav", "audio/wav"),
    ("weba", "audio/webm"),
    ("webm", "video/webm"),
    ("webmanifest", "application/manifest+json"),
    ("webp", "image/webp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("xhtml", "application/xhtml+xml"),
    ("xht", "application/xhtml+xml"),
    ("xls", "application/vnd.ms-excel"),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    ("xml", "application/xml"),
    ("xsl", "application/xml"),
    ("xz", "application/x-xz"),
    ("zip", "application/zip"),
    ("zst", "application/zstd"),
];

const BY_SIGNATURE: &[(&[u8], &str)] = &[
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
    (b"BM", "image/bmp"),
    (b"\x00\x00\x01\x00", "image/vnd.microsoft.icon"),
    (b"%PDF-", "application/pdf"),
    (b"PK\x03\x04", "application/zip"),
    (b"\x1f\x8b\x08", "application/gzip"),
    (b"\x1a\x45\xdf\xa3", "video/webm"),
    (b"OggS\x00", "audio/ogg"),
    (b"ID3", "audio/mpeg"),
    (b"fLaC", "audio/flac"),
    (b"\x00asm", "application/wasm"),
    (b"wOFF", "font/woff"),
    (b"wOF2", "font/woff2"),
    (b"OTTO", "font/otf"),
    (b"\x00\x01\x00\x00\x00", "font/ttf"),
    (b"\x00\x00\x01\xba", "video/mpeg"),
    (b"\x00\x00\x01\xb3", "video/mpeg"),
];

fn extension(name: &[u8]) -> Option<&[u8]> {
    let file = name
        .rsplit(|&b| b == b'/' || b == b'\\')
        .next()
        .unwrap_or(name);
    let dot = file.iter().rposition(|&b| b == b'.')?;
    (dot > 0 && dot + 1 < file.len()).then(|| &file[dot + 1..])
}

fn by_extension(name: &[u8]) -> Option<&'static str> {
    let ext = extension(name)?;
    BY_EXTENSION
        .iter()
        .find(|(known, _)| known.as_bytes().eq_ignore_ascii_case(ext))
        .map(|&(_, mime)| mime)
}

fn riff(data: &[u8], form: &[u8; 4]) -> bool {
    data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == form
}

fn starts_with_tag(text: &[u8], tag: &[u8]) -> bool {
    text.len() >= tag.len() && text[..tag.len()].eq_ignore_ascii_case(tag)
}

fn markup(data: &[u8]) -> Option<&'static str> {
    let text = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let start = text.iter().position(|b| !b.is_ascii_whitespace())?;
    let text = &text[start..];
    if starts_with_tag(text, b"<!doctype html") || starts_with_tag(text, b"<html") {
        return Some("text/html");
    }
    if starts_with_tag(text, b"<svg") {
        return Some("image/svg+xml");
    }
    if text.starts_with(b"<?xml") {
        let head = &text[..text.len().min(512)];
        let svg = head.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg"));
        return Some(if svg {
            "image/svg+xml"
        } else {
            "application/xml"
        });
    }
    None
}

fn by_signature(data: &[u8]) -> Option<&'static str> {
    let data = &data[..data.len().min(SNIFF_LEN)];
    if let Some(&(_, mime)) = BY_SIGNATURE
        .iter()
        .find(|(magic, _)| data.starts_with(magic))
    {
        return Some(mime);
    }
    if riff(data, b"WEBP") {
        return Some("image/webp");
    }
    if riff(data, b"WAVE") {
        return Some("audio/wav");
    }
    if riff(data, b"AVI ") {
        return Some("video/x-msvideo");
    }
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let brand = &data[8..12];
        return Some(match brand {
            b"avif" | b"avis" => "image/avif",
            b"M4A " => "audio/mp4",
            b"qt  " => "video/quicktime",
            _ => "video/mp4",
        });
    }
    markup(data)
}

fn looks_like_text(data: &[u8]) -> bool {
    let data = &data[..data.len().min(SNIFF_LEN)];
    !data.contains(&0)
        && match core::str::from_utf8(data) {
            Ok(_) => true,
            Err(error) => error.error_len().is_none(),
        }
}

pub fn guess(name: &[u8], data: Option<&[u8]>) -> Option<&'static str> {
    if let Some(mime) = by_extension(name) {
        return Some(mime);
    }
    let data = data.filter(|data| !data.is_empty())?;
    by_signature(data).or_else(|| looks_like_text(data).then_some("text/plain"))
}
