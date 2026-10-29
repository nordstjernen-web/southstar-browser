//! Southstar — the C ABI of element state: the pseudo-class test css.c's matcher delegates, the target fragment, visited links and the document language, with the Unicode, pattern and URL checks they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

const RTL_SCRIPTS: [c_int; 7] = [19, 2, 34, 37, 66, 90, 95];

fn nul_free_or_empty(text: &[u8]) -> &[u8] {
    if text.contains(&0) { b"" } else { text }
}

pub(crate) fn unichar_is_rtl_script(c: u32) -> bool {
    RTL_SCRIPTS.contains(&c_int::from(southstar_unicode::script(c)))
}

pub(crate) fn unichar_is_alpha(c: u32) -> bool {
    southstar_unicode::is_alpha(c)
}

pub(crate) fn url_resolve(base: &[u8], href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(nul_free_or_empty(base)), nul_free_or_empty(href))
}

pub(crate) fn url_is_valid_absolute(url: &CStr) -> bool {
    southstar_url::bytes::is_valid_absolute(url.to_bytes())
}

pub(crate) fn regex_matches_whole(pattern: Option<&[u8]>, value: &[u8]) -> bool {
    let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    crate::pattern_matches(pattern, value).unwrap_or(true)
}
