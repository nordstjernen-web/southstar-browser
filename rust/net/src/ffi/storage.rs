//! Southstar — the teardown of the network layer's state and the HSTS upgrade check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::hsts;
use crate::storage;

pub fn ns_net_state_shutdown() {
    crate::netlog::shutdown();
    super::proxy::shutdown();
    storage::shutdown();
    hsts::shutdown();
}

pub fn hsts_should_upgrade(host: &[u8]) -> bool {
    hsts::should_upgrade(host)
}
