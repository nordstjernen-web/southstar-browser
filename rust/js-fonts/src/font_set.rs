//! Southstar — document.fonts: a FontFaceSet whose ready and load() promises wait for the pending web fonts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi, ready};

pub(crate) fn document_fonts(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let set = scope.new_object();
    let (ready_promise, resolve, _) = scope.new_promise()?;
    let loading = !js.is_null() && ffi::pending_font_count() > 0;
    ready::resolve_when_fonts_loaded(scope, js, resolve, set.clone());
    crate::set(scope, &set, "ready", ready_promise);
    crate::set_str(
        scope,
        &set,
        "status",
        if loading { "loading" } else { "loaded" },
    );
    crate::bind(scope, &set, "check", 1, crate::always_true);
    crate::bind(scope, &set, "load", 2, load);
    crate::bind(scope, &set, "add", 1, crate::undefined);
    crate::bind(scope, &set, "delete", 1, crate::undefined);
    crate::bind(scope, &set, "clear", 0, crate::undefined);
    crate::bind(scope, &set, "forEach", 1, crate::undefined);
    crate::set(scope, &set, "size", Value::int(0));
    Ok(set)
}

fn load(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let (promise, resolve, _) = scope.new_promise()?;
    let faces = scope.new_array();
    ready::resolve_when_fonts_loaded(scope, js, resolve, faces);
    Ok(promise)
}
