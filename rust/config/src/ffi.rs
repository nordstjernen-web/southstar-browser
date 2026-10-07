//! Southstar — the C side's accessor for the runtime configuration, declared in src/config.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::NsConfig;

unsafe extern "C" {
    fn ns_config_get() -> *const NsConfig;
}

pub(crate) fn config() -> Option<&'static NsConfig> {
    unsafe { ns_config_get().as_ref() }
}
