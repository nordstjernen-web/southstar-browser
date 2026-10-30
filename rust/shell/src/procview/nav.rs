//! Southstar — the tab view's navigation arithmetic: renderer limits, the settle delay, the zoom ladder and the find-bar label.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const MAX_WIDTH: i32 = 6144;
pub const MAX_HEIGHT: i32 = 3456;
pub const MAX_RESTARTS: i32 = 3;
pub const MAX_JS_REDIRECTS: i32 = 20;
const SETTLE_MS: i32 = 400;
const ZOOM_MIN_PERMILLE: i32 = 250;
const ZOOM_MAX_PERMILLE: i32 = 5000;
const ZOOM_LADDER_PERCENT: [i32; 17] = [
    25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500,
];
const FIND_MARGIN: i32 = 40;

fn atoi(text: &[u8]) -> i64 {
    let mut rest = text
        .iter()
        .skip_while(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
        .peekable();
    let negative = match rest.peek() {
        Some(b'-') => {
            rest.next();
            true
        }
        Some(b'+') => {
            rest.next();
            false
        }
        _ => false,
    };
    let magnitude = rest.take_while(|d| d.is_ascii_digit()).fold(0i64, |n, d| {
        n.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    });
    if negative { -magnitude } else { magnitude }
}

pub fn settle_ms(env: Option<&[u8]>) -> i32 {
    match env {
        Some(text) if !text.is_empty() => {
            let v = atoi(text);
            if (0..=10000).contains(&v) {
                v as i32
            } else {
                SETTLE_MS
            }
        }
        _ => SETTLE_MS,
    }
}

pub fn zoom_permille(scale: f64) -> i32 {
    ((scale * 1000.0 + 0.5) as i32).clamp(ZOOM_MIN_PERMILLE, ZOOM_MAX_PERMILLE)
}

pub fn zoom_percent(scale: f64) -> i32 {
    (scale * 100.0 + 0.5) as i32
}

pub fn zoom_step(scale: f64, direction: i32) -> f64 {
    let now = zoom_percent(scale);
    let percent = if direction > 0 {
        ZOOM_LADDER_PERCENT
            .iter()
            .copied()
            .find(|&p| p > now)
            .unwrap_or(ZOOM_LADDER_PERCENT[ZOOM_LADDER_PERCENT.len() - 1])
    } else {
        ZOOM_LADDER_PERCENT
            .iter()
            .rev()
            .copied()
            .find(|&p| p < now)
            .unwrap_or(ZOOM_LADDER_PERCENT[0])
    };
    f64::from(percent) / 100.0
}

pub fn is_fullscreen_action(action: &[u8]) -> bool {
    action == b"fullscreen-enter" || action == b"fullscreen-exit"
}

pub fn find_count(current: i32, total: i32) -> Vec<u8> {
    format!("{current}/{total}").into_bytes()
}

pub fn is_local_url(url: &[u8]) -> bool {
    let scheme: Vec<u8> = url
        .iter()
        .copied()
        .skip_while(|&c| c <= b' ')
        .filter(|&c| !matches!(c, b'\t' | b'\n' | b'\r'))
        .take(17)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let scheme = scheme.strip_prefix(b"view-source:").unwrap_or(&scheme);
    scheme.starts_with(b"file:")
}

pub fn same_document_url_ok(current: Option<&[u8]>, next: &[u8], local_page: bool) -> bool {
    if local_page {
        return is_local_url(next);
    }
    if is_local_url(next) {
        return false;
    }
    match current {
        Some(current) if southstar_url::bytes::is_http_or_https(current) => {
            southstar_url::bytes::same_origin(current, next)
        }
        _ => true,
    }
}

const INSPECT_HEAD: &[u8] = br#"(function(){try{var e=document.querySelector(""#;
const INSPECT_TAIL: &[u8] = r#"");if(!e)return "";var s=e.outerHTML;return s.length>20000?s.slice(0,20000)+"\n…(truncated)":s;}catch(err){return "Error: "+err;}})()"#.as_bytes();

fn c_escape(text: &[u8], out: &mut Vec<u8>) {
    for &b in text {
        let named = match b {
            0x08 => Some(b'b'),
            0x0c => Some(b'f'),
            b'\n' => Some(b'n'),
            b'\r' => Some(b'r'),
            b'\t' => Some(b't'),
            0x0b => Some(b'v'),
            b'\\' => Some(b'\\'),
            b'"' => Some(b'"'),
            _ => None,
        };
        match named {
            Some(c) => out.extend_from_slice(&[b'\\', c]),
            None if !(b' '..0x7f).contains(&b) => out.extend_from_slice(&[
                b'\\',
                b'0' + ((b >> 6) & 7),
                b'0' + ((b >> 3) & 7),
                b'0' + (b & 7),
            ]),
            None => out.push(b),
        }
    }
}

pub fn inspect_script(selector: &[u8]) -> Vec<u8> {
    let mut src = INSPECT_HEAD.to_vec();
    c_escape(selector, &mut src);
    src.extend_from_slice(INSPECT_TAIL);
    src
}

pub fn find_scroll_target(match_y: i32) -> f64 {
    f64::from(if match_y > FIND_MARGIN {
        match_y - FIND_MARGIN
    } else {
        0
    })
}
