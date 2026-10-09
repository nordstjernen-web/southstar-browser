//! Southstar — the C ABI of proxy settings: the command-line override and the configured HTTP, HTTPS and no-proxy values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_config::NsConfig;
use southstar_glib as glib;

use crate::storage::Slot;

static OVERRIDE: Slot = Slot::new();

fn config() -> Option<&'static NsConfig> {
    southstar_config::get()
}

fn non_empty(p: *mut c_char) -> Option<*const c_char> {
    unsafe { glib::bytes(p) }
        .filter(|v| !v.is_empty())
        .map(|_| p.cast_const())
}

fn override_ptr() -> *const c_char {
    OVERRIDE.with_ptr(|value| value.map_or(ptr::null(), |v| v.as_ptr()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_set_proxy_override(proxy_url: *const c_char) {
    OVERRIDE.set(
        unsafe { glib::bytes(proxy_url) }
            .filter(|p| !p.is_empty())
            .map(<[u8]>::to_vec),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_proxy_override() -> *const c_char {
    override_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_pick_configured_proxy(url: *const c_char) -> *const c_char {
    let forced = override_ptr();
    if unsafe { glib::bytes(forced) }.is_some_and(|f| !f.is_empty()) {
        return forced;
    }
    let Some(cfg) = config() else {
        return ptr::null();
    };
    let url = unsafe { glib::bytes(url) }.unwrap_or_default();
    let https = url.starts_with(b"https://") || url.starts_with(b"wss://");
    let secure = if https {
        non_empty(cfg.https_proxy)
    } else {
        None
    };
    secure
        .or_else(|| non_empty(cfg.http_proxy))
        .unwrap_or(ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_configured_no_proxy() -> *const c_char {
    config()
        .and_then(|cfg| non_empty(cfg.no_proxy))
        .unwrap_or(ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_http_proxy() -> *const c_char {
    config().map_or(ptr::null(), |cfg| cfg.http_proxy.cast_const())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_https_proxy() -> *const c_char {
    config().map_or(ptr::null(), |cfg| cfg.https_proxy.cast_const())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_no_proxy() -> *const c_char {
    config().map_or(ptr::null(), |cfg| cfg.no_proxy.cast_const())
}

pub fn shutdown() {
    OVERRIDE.set(None);
}
