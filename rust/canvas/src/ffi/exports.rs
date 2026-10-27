//! Southstar — the Rust-ABI entry points other binding crates call into the canvas through: install, structured-clone support and DOMMatrix results.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

#[unsafe(no_mangle)]
pub fn nsr_canvas_install(scope: &mut Scope<'_>, global: &Value, window: bool) {
    crate::api::install(scope, global, window);
}

pub fn nsr_canvas_clone_object(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
    crate::api::clone_object(scope, value)
}

pub fn nsr_image_bitmap_is(value: &Value) -> bool {
    super::is_image_bitmap(value)
}

pub fn nsr_canvas_hidden_is(value: &Value, kind: i32) -> bool {
    crate::hidden::is(value, kind)
}

pub fn nsr_canvas_hidden_get(
    scope: &mut Scope<'_>,
    value: &Value,
    key: &str,
) -> Result<Value, Value> {
    crate::hidden::get(scope, value, key)
}

pub fn nsr_dommatrix_make(scope: &mut Scope<'_>, matrix: [f64; 6]) -> Value {
    crate::context::dommatrix(scope, matrix)
}
