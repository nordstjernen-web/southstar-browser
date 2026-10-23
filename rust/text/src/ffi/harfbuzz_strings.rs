//! Southstar text — HarfBuzz's parsers and printers for OpenType feature and variation strings, used to normalize font descriptions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{c_char, c_int, c_uint};

#[repr(C)]
#[derive(Default)]
struct HbFeature {
    tag: u32,
    value: u32,
    start: c_uint,
    end: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct HbVariation {
    tag: u32,
    value: f32,
}

#[link(name = "harfbuzz")]
unsafe extern "C" {
    fn hb_feature_from_string(s: *const c_char, len: c_int, feature: *mut HbFeature) -> c_int;
    fn hb_feature_to_string(feature: *mut HbFeature, buf: *mut c_char, size: c_uint);
    fn hb_variation_from_string(s: *const c_char, len: c_int, variation: *mut HbVariation)
    -> c_int;
    fn hb_variation_to_string(variation: *mut HbVariation, buf: *mut c_char, size: c_uint);
}

fn buffer_text(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

pub(crate) fn normalize_feature(text: &[u8]) -> Option<String> {
    let mut feature = HbFeature::default();
    let len = c_int::try_from(text.len()).ok()?;
    // SAFETY: HarfBuzz reads at most len bytes of text and writes one hb_feature_t into the local.
    let parsed = unsafe { hb_feature_from_string(text.as_ptr().cast(), len, &mut feature) };
    if parsed == 0 {
        return None;
    }
    let mut buf = [0u8; 128];
    // SAFETY: HarfBuzz writes at most buf.len() bytes, NUL included, into the local buffer.
    unsafe { hb_feature_to_string(&mut feature, buf.as_mut_ptr().cast(), buf.len() as c_uint) };
    Some(buffer_text(&buf))
}

pub(crate) fn normalize_variation(text: &[u8]) -> Option<String> {
    let mut variation = HbVariation::default();
    let len = c_int::try_from(text.len()).ok()?;
    // SAFETY: HarfBuzz reads at most len bytes of text and writes one hb_variation_t into the local.
    let parsed = unsafe { hb_variation_from_string(text.as_ptr().cast(), len, &mut variation) };
    if parsed == 0 {
        return None;
    }
    let mut buf = [0u8; 128];
    // SAFETY: HarfBuzz writes at most buf.len() bytes, NUL included, into the local buffer.
    unsafe { hb_variation_to_string(&mut variation, buf.as_mut_ptr().cast(), buf.len() as c_uint) };
    Some(buffer_text(&buf))
}
