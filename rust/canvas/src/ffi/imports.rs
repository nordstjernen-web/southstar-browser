//! Southstar — the entry points of the crates above the canvas that it calls back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_webgl_get_context;
#[cfg(feature = "webgpu")]
use southstar_js_engine::{Scope, Value};

#[cfg(feature = "webgpu")]
unsafe extern "Rust" {
    pub(crate) safe fn nsr_webgpu_get_context(
        scope: &mut Scope<'_>,
        canvas_obj: &Value,
        canvas: usize,
    ) -> Value;
}
