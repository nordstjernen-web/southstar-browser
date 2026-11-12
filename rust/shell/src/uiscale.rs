//! Southstar — UI scale for displays GTK does not scale itself: GTK on Windows scales widgets by whole factors only, so at 125%, 150% or 175% it scales text and leaves every pixel length at 100%.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

const MAX_SCALE: f64 = 4.0;

static SCALE: OnceLock<f64> = OnceLock::new();

pub fn fraction_gtk_leaves(display_scale: f64) -> f64 {
    if !(display_scale > 1.0 && display_scale <= 8.0) {
        return 1.0;
    }
    let ui = display_scale / display_scale.floor();
    if ui > 1.01 && ui <= MAX_SCALE {
        ui
    } else {
        1.0
    }
}

#[cfg(windows)]
fn platform_scale() -> f64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForSystem() -> u32;
    }
    // SAFETY: GetDpiForSystem takes no arguments and only reads the process's DPI setting.
    let dpi = unsafe { GetDpiForSystem() };
    fraction_gtk_leaves(f64::from(dpi) / 96.0)
}

#[cfg(not(windows))]
fn platform_scale() -> f64 {
    1.0
}

fn detect() -> f64 {
    let forced = std::env::var("NS_UI_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok());
    match forced {
        Some(s) if (1.0..=MAX_SCALE).contains(&s) => s,
        _ => platform_scale(),
    }
}

pub fn scale() -> f64 {
    *SCALE.get_or_init(detect)
}

pub fn scale_px(px: i32, scale: f64) -> i32 {
    if (-1..=1).contains(&px) {
        return px;
    }
    (f64::from(px) * scale).round() as i32
}

pub fn px(px: i32) -> i32 {
    scale_px(px, scale())
}

fn starts_token(prev: Option<u8>) -> bool {
    !prev.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'#' | b'-' | b'_'))
}

pub fn scale_css(css: &str, scale: f64) -> String {
    if scale == 1.0 {
        return css.to_owned();
    }
    let bytes = css.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 64);
    let mut i = 0;
    while i < bytes.len() {
        let prev = i.checked_sub(1).map(|p| bytes[p]);
        if !bytes[i].is_ascii_digit() || !starts_token(prev) {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let mut end = i;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let is_px = bytes[end..].starts_with(b"px")
            && !bytes.get(end + 2).is_some_and(u8::is_ascii_alphanumeric);
        let value = css[i..end].parse::<i32>().ok().filter(|_| is_px);
        match value {
            Some(n) => out.extend_from_slice(scale_px(n, scale).to_string().as_bytes()),
            None => out.extend_from_slice(&bytes[i..end]),
        }
        i = end;
    }
    String::from_utf8(out).unwrap_or_else(|_| css.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{fraction_gtk_leaves, scale_css};

    #[test]
    fn whole_display_scales_leave_nothing() {
        assert_eq!(fraction_gtk_leaves(1.0), 1.0);
        assert_eq!(fraction_gtk_leaves(2.0), 1.0);
        assert_eq!(fraction_gtk_leaves(1.5), 1.5);
        assert_eq!(fraction_gtk_leaves(2.5), 1.25);
    }

    #[test]
    fn scales_pixel_lengths_only() {
        assert_eq!(
            scale_css(
                "a { margin: 0 1px 12px; color: #12px; flex: 2; width: 50%; }",
                1.5
            ),
            "a { margin: 0 1px 18px; color: #12px; flex: 2; width: 50%; }"
        );
        assert_eq!(
            scale_css("b { min-height: 42px; }", 1.0),
            "b { min-height: 42px; }"
        );
    }
}
