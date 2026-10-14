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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_strip_tracking_params(url: *const c_char) -> *mut c_char {
    let Some(url) = text(url).filter(|u| url::is_http_or_https(u)) else {
        return ptr::null_mut();
    };
    dup(url::strip_tracking_params(
        url,
        config_flag(|c| c.strip_tracking_params),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_parse_refresh(
    input: *const c_char,
    time_out: *mut f64,
    url_out: *mut *mut c_char,
) -> GBoolean {
    unsafe {
        if let Some(t) = time_out.as_mut() {
            *t = 0.0;
        }
        if let Some(u) = url_out.as_mut() {
            *u = ptr::null_mut();
        }
    }
    let Some((seconds, target)) = text(input).and_then(url::parse_refresh) else {
        return 0;
    };
    unsafe {
        if let Some(t) = time_out.as_mut() {
            *t = seconds;
        }
        if let Some(u) = url_out.as_mut() {
            *u = dup(target);
        }
    }
    1
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_https_first_upgrade(url: *const c_char) -> *mut c_char {
    if !config_flag(|c| c.https_first) {
        return ptr::null_mut();
    }
    dup(text(url).and_then(|u| url::https_first_upgrade(u, true)))
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
