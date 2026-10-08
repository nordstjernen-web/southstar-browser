//! Southstar — the image decode chain and the per-page image cache: MIME support, retry back-off, animation frames and the memory budget.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::c_long;

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
const SECOND_US: i64 = 1_000_000;
const RETRYABLE_STATUS: [c_long; 7] = [408, 425, 429, 500, 502, 503, 504];

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

#[cfg(windows)]
pub(crate) fn bytes_blocked_on_platform(data: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 12] = [
        b"avif", b"avis", b"heic", b"heix", b"hevc", b"hevx", b"mif1", b"msf1", b"heim", b"heis",
        b"hevm", b"hevs",
    ];
    if data.len() < 12 {
        return false;
    }
    if data.starts_with(b"\xFF\x0A") {
        return true;
    }
    if data.starts_with(b"\x00\x00\x00") && &data[4..8] == b"JXL " {
        return true;
    }
    &data[4..8] == b"ftyp" && BRANDS.iter().any(|brand| &data[8..12] == *brand)
}

#[cfg(not(windows))]
pub(crate) fn bytes_blocked_on_platform(_data: &[u8]) -> bool {
    false
}

pub fn supports_mime(mime: &[u8]) -> bool {
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

pub struct RetryState {
    pub failed: bool,
    pub attempts: i32,
    pub http_status: c_long,
    pub failed_at_us: i64,
}

pub fn should_retry(state: &RetryState, now_us: i64) -> bool {
    if !state.failed || state.attempts >= 3 {
        return false;
    }
    if state.http_status > 0 && !RETRYABLE_STATUS.contains(&state.http_status) {
        return false;
    }
    let early = state.attempts <= 1;
    let wait_us = match (state.http_status == 429, early) {
        (true, true) => 15 * SECOND_US,
        (true, false) => 45 * SECOND_US,
        (false, true) => 2 * SECOND_US,
        (false, false) => 10 * SECOND_US,
    };
    state.failed_at_us == 0 || now_us.wrapping_sub(state.failed_at_us) >= wait_us
}

pub fn animation_frame(delays: &[i32], total_ms: i32, start_us: i64, now_us: i64) -> usize {
    let elapsed_ms = (now_us.wrapping_sub(start_us) / 1000).max(0);
    let phase = elapsed_ms % i64::from(total_ms);
    let mut acc: i64 = 0;
    for (index, delay) in delays.iter().enumerate() {
        acc += i64::from(*delay);
        if phase < acc {
            return index;
        }
    }
    delays.len().saturating_sub(1)
}

pub fn total_delay_ms(delays: impl Iterator<Item = i64>) -> i32 {
    delays.sum::<i64>().clamp(1, i64::from(i32::MAX)) as i32
}

pub fn decode_error(content_type: Option<&[u8]>, body_len: usize) -> Vec<u8> {
    let kind = content_type
        .filter(|t| !t.is_empty())
        .unwrap_or(b"unknown type");
    let mut text = b"could not decode image (".to_vec();
    text.extend_from_slice(kind);
    text.extend_from_slice(format!(", {} bytes)", body_len as u32).as_bytes());
    text
}
