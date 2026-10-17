//! Southstar — the C ABI of cookies, HSTS and the network layer's storage paths: the network calls plus the paths the transport code in net.c uses and the teardown of the Rust side's network state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_glib::{self as glib, GBoolean};

use crate::hsts;
use crate::storage::{self};

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
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
pub unsafe extern "C" fn ns_net_hsts_should_upgrade(host: *const c_char) -> GBoolean {
    glib::boolean(text(host).is_some_and(hsts::should_upgrade))
}
