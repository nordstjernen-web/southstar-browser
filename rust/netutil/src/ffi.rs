//! Southstar — the configuration, file system and locale lookups behind the shared network helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_config::util;
use southstar_glib as glib;

use crate::file_uri;

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn configured_search_engine() -> Option<Vec<u8>> {
    let config = southstar_config::get()?;
    config.search_engine.to_vec()
}

pub(crate) fn uri_escape(text: &[u8]) -> Vec<u8> {
    file_uri::uri_escape(text)
}

pub(crate) fn file_exists(path: &[u8]) -> bool {
    util::os_path(path).is_some_and(|path| path.exists())
}

pub(crate) fn is_dir(path: &[u8]) -> bool {
    util::os_path(path).is_some_and(|path| path.is_dir())
}

pub(crate) fn canonicalize_filename(path: &[u8]) -> Option<Vec<u8>> {
    Some(file_uri::canonicalize_filename(path))
}

pub(crate) fn filename_to_uri(path: &[u8]) -> Option<Vec<u8>> {
    file_uri::filename_to_uri(path)
}

#[cfg(windows)]
fn windows_locale() -> Option<Vec<u8>> {
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(name: *mut u16, length: core::ffi::c_int) -> core::ffi::c_int;
    }
    let mut name = [0u16; LOCALE_NAME_MAX_LENGTH];
    if unsafe {
        GetUserDefaultLocaleName(
            name.as_mut_ptr(),
            LOCALE_NAME_MAX_LENGTH as core::ffi::c_int,
        )
    } <= 0
    {
        return None;
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    String::from_utf16(&name[..end])
        .ok()
        .map(String::into_bytes)
        .filter(|locale| !locale.is_empty())
}

#[cfg(not(windows))]
fn windows_locale() -> Option<Vec<u8>> {
    None
}

fn language_names() -> Vec<Vec<u8>> {
    glib::language_names()
}

fn default_accept_language_c() -> &'static CStr {
    static CACHED: OnceLock<CString> = OnceLock::new();
    CACHED.get_or_init(|| {
        let value = match windows_locale() {
            Some(locale) => crate::accept_language_for_windows_locale(&locale),
            None => crate::accept_language_from_language_names(&language_names())
                .unwrap_or_else(|| b"en-US,en;q=0.9".to_vec()),
        };
        cstring(&value)
    })
}

pub fn effective_accept_language() -> Vec<u8> {
    southstar_config::get()
        .and_then(|c| c.accept_language.non_empty())
        .unwrap_or_else(default_accept_language)
        .to_vec()
}

pub fn effective_navigator_languages() -> Vec<Vec<u8>> {
    crate::navigator_languages(&effective_accept_language())
}

pub fn default_accept_language() -> &'static [u8] {
    default_accept_language_c().to_bytes()
}
