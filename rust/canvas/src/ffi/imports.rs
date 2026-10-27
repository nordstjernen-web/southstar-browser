//! Southstar — the Rust-ABI entry points of the crates the canvas calls back into, resolved at link time because those crates sit above this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_webgl_get_context;
use southstar_js_engine::{Scope, Value};

#[cfg(feature = "webgpu")]
unsafe extern "Rust" {
    pub(crate) safe fn nsr_webgpu_get_context(
        scope: &mut Scope<'_>,
        canvas_obj: &Value,
        canvas: usize,
    ) -> Value;
}
