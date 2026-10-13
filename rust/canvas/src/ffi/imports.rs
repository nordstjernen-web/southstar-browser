//! Southstar — the Rust-ABI entry points of the crates the canvas calls back into, resolved at link time because those crates sit above this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_webgl_get_context(
        scope: &mut Scope<'_>,
        js: usize,
        canvas_obj: &Value,
        canvas: usize,
        version: i32,
        attrs: &Value,
    ) -> Result<Value, Value>;
}

#[cfg(feature = "webgpu")]
unsafe extern "Rust" {
    pub(crate) safe fn nsr_webgpu_get_context(
        scope: &mut Scope<'_>,
        canvas_obj: &Value,
        canvas: usize,
    ) -> Value;
}
