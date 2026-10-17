//! Southstar — the C ABI of cookies, HSTS and the network layer's storage paths: the network calls plus the paths the transport code in net.c uses and the teardown of the Rust side's network state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::hsts;
use crate::storage::{self};

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

pub fn hsts_should_upgrade(host: &[u8]) -> bool {
    hsts::should_upgrade(host)
}
