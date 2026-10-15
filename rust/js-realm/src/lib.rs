//! Southstar — hides SharedArrayBuffer from window, frame and worker globals, since no page is cross-origin isolated.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

use southstar_js_engine::{Scope, Value};

pub fn hide_shared_array_buffer(scope: &mut Scope<'_>, global: &Value) {
    let _ = scope.delete(global, "SharedArrayBuffer");
}
