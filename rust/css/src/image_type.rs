//! Southstar — the image MIME types the image decoders accept, which <picture> sources and presentational hints check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const BUILTIN_TYPES: [&[u8]; 8] = [
    b"image/png",
    b"image/jpeg",
    b"image/jpg",
    b"image/gif",
    b"image/bmp",
    b"image/webp",
    b"image/x-icon",
    b"image/vnd.microsoft.icon",
];

fn builtin_supports_mime(bare: &[u8]) -> bool {
    BUILTIN_TYPES.contains(&bare)
        || bare == b"image/svg+xml"
        || (cfg!(feature = "avif") && bare == b"image/avif")
}

#[cfg(windows)]
fn mime_blocked_on_platform(bare: &[u8]) -> bool {
    [
        &b"image/avif"[..],
        b"image/heif",
        b"image/heic",
        b"image/heif-sequence",
        b"image/heic-sequence",
        b"image/jxl",
    ]
    .contains(&bare)
}

#[cfg(not(windows))]
fn mime_blocked_on_platform(_bare: &[u8]) -> bool {
    false
}

pub fn image_type_supported(mime: &[u8]) -> bool {
    let start = mime
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(mime.len());
    let rest = &mime[start..];
    let end = rest
        .iter()
        .position(|&b| b == b';' || b.is_ascii_whitespace())
        .unwrap_or(rest.len());
    if end == 0 {
        return false;
    }
    let bare = rest[..end].to_ascii_lowercase();
    !mime_blocked_on_platform(&bare) && builtin_supports_mime(&bare)
}
