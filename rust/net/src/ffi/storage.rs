//! Southstar — the C ABI of cookies, HSTS and the network layer's storage paths: the net.h calls plus the paths the transport code in net.c uses and the teardown of the Rust side's network state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use crate::storage::{self, Slot};
use crate::{cookies, hsts};

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn dup(value: Option<Vec<u8>>) -> *mut c_char {
    value.map_or(ptr::null_mut(), |v| glib::strdup(&v))
}

fn slot_ptr(slot: &Slot, initialized: Option<Vec<u8>>) -> *const c_char {
    if initialized.is_none() {
        return ptr::null();
    }
    slot.with_ptr(|path| path.map_or(ptr::null(), |p| p.as_ptr()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_cookie_dir() -> *const c_char {
    slot_ptr(&storage::COOKIE_DIR, storage::cookie_dir())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_hsts_curl_path() -> *const c_char {
    slot_ptr(&storage::HSTS_PATH, storage::hsts_path())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_altsvc_path() -> *const c_char {
    slot_ptr(&storage::ALTSVC_PATH, storage::altsvc_path())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_cookie_path_for_partition(
    top_origin: *const c_char,
) -> *mut c_char {
    dup(storage::cookie_jar_path(text(top_origin), false))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_cookie_js_path_for_partition(
    top_origin: *const c_char,
) -> *mut c_char {
    dup(storage::cookie_jar_path(text(top_origin), true))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_cookies_clear() {
    storage::clear_cookies();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_site_storage_clear() {
    storage::clear_site_storage();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_state_shutdown() {
    crate::netlog::shutdown();
    super::proxy::shutdown();
    storage::shutdown();
    hsts::shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_cookies_for_js(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(|u| cookies::collect(u, false)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_cookies_for_request(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(|u| cookies::collect(u, true)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_cookie_store_from_js(url: *const c_char, cookie: *const c_char) {
    if let (Some(url), Some(cookie)) = (text(url), text(cookie)) {
        cookies::store(url, cookie, false);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_store_set_cookie(
    url: *const c_char,
    set_cookie_value: *const c_char,
) {
    if let (Some(url), Some(cookie)) = (text(url), text(set_cookie_value)) {
        cookies::store(url, cookie, true);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_hsts_should_upgrade(host: *const c_char) -> GBoolean {
    glib::boolean(text(host).is_some_and(hsts::should_upgrade))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_hsts_upgrade(url: *const c_char) -> *mut c_char {
    dup(text(url).and_then(hsts::upgrade))
}
