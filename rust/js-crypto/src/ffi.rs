//! Southstar — the Rust-ABI installers of the WebCrypto bindings, and the CSPRNG they draw from.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_glib::GBoolean;
use southstar_js_engine::{Scope, Value};

unsafe extern "C" {
    fn ns_security_csprng_fill(buf: *mut c_void, len: usize) -> GBoolean;
}

pub(crate) fn csprng_fill(bytes: &mut [u8]) -> bool {
    unsafe { ns_security_csprng_fill(bytes.as_mut_ptr().cast(), bytes.len()) != 0 }
}

#[unsafe(no_mangle)]
pub fn nsr_crypto_install_window(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window(scope, global)
}

#[unsafe(no_mangle)]
pub fn nsr_crypto_install_window_subtle(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window_subtle(scope, global)
}

#[unsafe(no_mangle)]
pub fn nsr_crypto_install_worker(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_worker(scope, global)
}
