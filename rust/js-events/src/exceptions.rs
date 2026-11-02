//! Southstar — the DOMException values the bindings throw or reject with, built on the realm's DOMException interface.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

const ABORT_MESSAGE: &str = "The operation was aborted.";

pub(crate) fn abort_error(scope: &mut Scope<'_>) -> Value {
    scope.dom_exception("AbortError", ABORT_MESSAGE)
}
