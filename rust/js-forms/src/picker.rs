//! Southstar — showPicker() on <input> and <select>: the mutability and user-activation checks; the shell opens no picker of its own.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::controls::{effectively_disabled, type_supports_readonly};
use southstar_js_engine::{Scope, Value};

use crate::JsResult;
use crate::ffi::{self, Page};

const INVALID_STATE_ERR: i32 = 11;

pub(crate) fn show_picker(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(control) = ffi::element(this) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    let is_input = control.element_name() == Some(b"input");
    if !is_input && control.element_name() != Some(b"select") {
        return Err(scope.type_error("Illegal invocation"));
    }
    let readonly_applies = is_input
        && control.attr(c"readonly").is_some()
        && type_supports_readonly(control.attr(c"type"));
    if effectively_disabled(control) || readonly_applies {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            INVALID_STATE_ERR,
            "showPicker() cannot be used on immutable controls",
        ));
    }
    let page = Page::of(scope);
    if !page.is_some_and(Page::has_transient_activation) {
        return Err(ffi::dom_exception(
            scope,
            c"NotAllowedError",
            0,
            "showPicker() requires a user gesture",
        ));
    }
    if let Some(page) = page {
        page.consume_user_activation();
    }
    Ok(Value::undefined())
}
