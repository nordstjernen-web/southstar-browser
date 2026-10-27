//! Southstar — the Rust-ABI installers of the WebCrypto bindings, and the CSPRNG they draw from.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

pub(crate) fn csprng_fill(bytes: &mut [u8]) -> bool {
    southstar_sandbox::csprng_fill(bytes)
}

pub fn nsr_crypto_install_window(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window(scope, global)
}

pub fn nsr_crypto_install_window_subtle(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window_subtle(scope, global)
}

pub fn nsr_crypto_install_worker(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_worker(scope, global)
}
