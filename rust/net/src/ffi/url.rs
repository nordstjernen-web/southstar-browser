//! Southstar — the C ABI of the URL helpers and user agent strings declared in src/net.h, plus the origin, site and ASCII forms the fetch path in net.c uses.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::url;

#[repr(C)]
pub struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

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
    text(date).and_then(crate::http_date::parse).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean {
    glib::boolean(text(url).is_some_and(url::is_http_or_https))
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
pub unsafe extern "C" fn ns_url_resolve_len(
    base: *const c_char,
    href: *const c_char,
    href_len: usize,
) -> *mut c_char {
    if href.is_null() {
        return ptr::null_mut();
    }
    let href = unsafe { glib::slice(href.cast(), href_len) };
    dup(url::resolve(text(base), href))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char {
    let Some(href) = text(href) else {
        return ptr::null_mut();
    };
    dup(url::resolve(text(base), href))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_set_component_len(
    href: *const c_char,
    component: *const c_char,
    value: *const c_char,
    value_len: usize,
) -> *mut c_char {
    let (Some(href), Some(component)) = (text(href), text(component)) else {
        return ptr::null_mut();
    };
    if value.is_null() {
        return ptr::null_mut();
    }
    let value = unsafe { glib::slice(value.cast(), value_len) };
    dup(url::set_component(href, component, value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_to_ascii(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(url::to_ascii))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_origin_from(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(url::origin_from))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_origin_from_any(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(url::origin_from_any))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_host_from(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(url::host_from))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> GBoolean {
    let same = match (text(a), text(b)) {
        (Some(a), Some(b)) => url::same_origin(a, b),
        _ => false,
    };
    glib::boolean(same)
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
pub unsafe extern "C" fn ns_url_site_from(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(url::site_from))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_is_same_site(a: *const c_char, b: *const c_char) -> GBoolean {
    let same = match (text(a), text(b)) {
        (Some(a), Some(b)) => url::is_same_site(a, b),
        _ => false,
    };
    glib::boolean(same)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_parts_free(parts: *mut NsUrlParts) {
    let Some(p) = (unsafe { parts.as_mut() }) else {
        return;
    };
    for field in [
        p.href, p.protocol, p.origin, p.host, p.hostname, p.port, p.pathname, p.search, p.hash,
        p.username, p.password,
    ] {
        unsafe { glib::g_free(field.cast()) };
    }
    unsafe { glib::g_free(parts.cast()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts {
    let Some(p) = text(url).and_then(url::parts) else {
        return ptr::null_mut();
    };
    let out = unsafe { glib::g_malloc0(core::mem::size_of::<NsUrlParts>()) }.cast::<NsUrlParts>();
    unsafe {
        out.write(NsUrlParts {
            href: glib::strdup(&p.href),
            protocol: glib::strdup(&p.protocol),
            origin: glib::strdup(&p.origin),
            host: glib::strdup(&p.host),
            hostname: glib::strdup(&p.hostname),
            port: glib::strdup(&p.port),
            pathname: glib::strdup(&p.pathname),
            search: glib::strdup(&p.search),
            hash: glib::strdup(&p.hash),
            username: glib::strdup(&p.username),
            password: glib::strdup(&p.password),
        });
    }
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean {
    glib::boolean(text(url).is_some_and(url::is_valid_absolute))
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
