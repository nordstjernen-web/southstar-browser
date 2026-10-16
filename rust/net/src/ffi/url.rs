//! Southstar — the C ABI of the network helpers still called through C: tracking-parameter stripping, refresh and HTTP-date parsing, referers, header values, HTTPS-First and the user agent strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::url;

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn dup(value: Option<Vec<u8>>) -> *mut c_char {
    value.map_or(ptr::null_mut(), |v| glib::strdup(&v))
}

fn static_str(s: &'static str) -> *const c_char {
    s.as_ptr().cast()
}

fn config_flag(flag: impl Fn(&southstar_config::NsConfig) -> GBoolean) -> bool {
    southstar_config::get().is_some_and(|c| flag(c) != 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_http_date(date: *const c_char) -> i64 {
    text(date)
        .and_then(southstar_netstore::http_date::parse)
        .unwrap_or(-1)
}

pub fn strip_tracking_params(url: &[u8]) -> Option<Vec<u8>> {
    if !url::is_http_or_https(url) {
        return None;
    }
    url::strip_tracking_params(url, config_flag(|c| c.strip_tracking_params))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_referer_for(
    url: *const c_char,
    top_url: *const c_char,
    policy: c_int,
) -> *mut c_char {
    dup(url::referer_for(text(url), text(top_url), policy))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_raw_header_values(
    raw: *const c_char,
    name: *const c_char,
) -> *mut c_char {
    let (Some(raw), Some(name)) = (text(raw), text(name)) else {
        return ptr::null_mut();
    };
    dup(url::raw_header_values(raw, name))
}

pub fn https_first_upgrade(url: &[u8]) -> Option<Vec<u8>> {
    if !config_flag(|c| c.https_first) {
        return None;
    }
    url::https_first_upgrade(url, true)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_header_is_nosniff(value: *const c_char) -> GBoolean {
    glib::boolean(text(value).is_some_and(url::is_nosniff))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_is_mobile_mode() -> GBoolean {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_navigator_platform() -> *const c_char {
    static_str(url::NAVIGATOR_PLATFORM)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_ua_hint_platform() -> *const c_char {
    static_str(url::HINT_PLATFORM)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char {
    static_str(url::user_agent_for_mode(text(compat_mode)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_user_agent_has_client_hints(user_agent: *const c_char) -> GBoolean {
    glib::boolean(text(user_agent).is_some_and(url::has_client_hints))
}
