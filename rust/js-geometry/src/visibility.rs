//! Southstar — Element.checkVisibility(): whether the element generated a box, optionally also its computed visibility and opacity.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult, ffi};

fn option_set(scope: &mut Scope<'_>, options: &Value, key: &str) -> bool {
    match scope.get(options, key) {
        Ok(value) if !value.is_undefined() => scope.to_bool(&value),
        _ => false,
    }
}

fn either_option_set(scope: &mut Scope<'_>, options: &Value, keys: [&str; 2]) -> bool {
    option_set(scope, options, keys[0]) || option_set(scope, options, keys[1])
}

fn visibility_hidden(scope: &mut Scope<'_>, el: Element) -> bool {
    ffi::computed_text(scope, el, c"visibility")
        .is_some_and(|v| v.as_bytes() == b"hidden" || v.as_bytes() == b"collapse")
}

fn fully_transparent(scope: &mut Scope<'_>, el: Element) -> bool {
    ffi::computed_text(scope, el, c"opacity").is_some_and(|o| ffi::ascii_strtod_prefix(&o).0 == 0.0)
}

pub(crate) fn check_visibility(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(el) = ffi::unwrap_node(this).filter(|_| !js.is_null()) else {
        return Ok(Value::boolean(false));
    };
    ffi::flush_layout(js);
    let mut visible = ffi::layout_root(js).is_some_and(|root| ffi::find_by_dom(root, el).is_some());
    if let Some(options) = args.first().filter(|o| o.is_object()) {
        if visible
            && either_option_set(scope, options, ["checkVisibilityCSS", "visibilityProperty"])
            && visibility_hidden(scope, el)
        {
            visible = false;
        }
        if visible
            && either_option_set(scope, options, ["checkOpacity", "opacityProperty"])
            && fully_transparent(scope, el)
        {
            visible = false;
        }
    }
    Ok(Value::boolean(visible))
}
