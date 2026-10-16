//! Southstar — the 2D context's drawing state lookup and the page-mutation mark its drawing calls set.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use super::state::CanvasState;

pub(crate) fn ctx_state(scope: &Scope<'_>, this: &Value) -> Option<&'static mut CanvasState> {
    let js = super::state::js_of(scope);
    if js.is_null() || !crate::hidden::is_ctx2d(this) {
        return None;
    }
    let el = super::state::Node::from_addr(crate::hidden::ptr(this));
    let st = crate::state::state_for(js, el)?;
    Some(unsafe { &mut *st })
}

pub(crate) fn mark_mutated(scope: &Scope<'_>) {
    southstar_js_runtime::api::mark_mutated(super::state::js_of(scope).ptr());
}
