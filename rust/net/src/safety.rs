//! Southstar — the hooks through which the browser's safe-browsing checks reach networking: vetting each navigation redirect and the rows about:southstar shows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

pub struct SafetyHooks {
    pub blocks_navigation: fn(&[u8]) -> bool,
    pub status_rows: fn() -> Vec<(&'static str, String)>,
}

static HOOKS: OnceLock<SafetyHooks> = OnceLock::new();

pub fn set_safety_hooks(hooks: SafetyHooks) {
    let _ = HOOKS.set(hooks);
}

pub(crate) fn blocks_navigation(url: &[u8]) -> bool {
    HOOKS
        .get()
        .is_some_and(|hooks| (hooks.blocks_navigation)(url))
}

pub(crate) fn status_rows() -> Option<Vec<(&'static str, String)>> {
    HOOKS.get().map(|hooks| (hooks.status_rows)())
}
