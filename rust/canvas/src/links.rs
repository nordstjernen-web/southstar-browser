//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub type WebglGetContextFn =
    fn(&mut Scope<'_>, usize, &Value, usize, i32, &Value) -> Result<Value, Value>;

pub struct JsEventsLinks {
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
}

static JS_EVENTS_LINKS: OnceLock<JsEventsLinks> = OnceLock::new();

pub fn set_js_events_links(links: JsEventsLinks) {
    let _ = JS_EVENTS_LINKS.set(links);
}

fn js_events_links() -> &'static JsEventsLinks {
    JS_EVENTS_LINKS
        .get()
        .expect("southstar-js-events installs its links before the first page")
}

pub struct WebglLinks {
    pub webgl_get_context: WebglGetContextFn,
}

static WEBGL_LINKS: OnceLock<WebglLinks> = OnceLock::new();

pub fn set_webgl_links(links: WebglLinks) {
    let _ = WEBGL_LINKS.set(links);
}

fn webgl_links() -> &'static WebglLinks {
    WEBGL_LINKS
        .get()
        .expect("southstar-webgl installs its links before the first page")
}

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

pub(crate) fn nsr_webgl_get_context(
    scope: &mut Scope<'_>,
    js: usize,
    canvas_obj: &Value,
    canvas: usize,
    version: i32,
    attrs: &Value,
) -> Result<Value, Value> {
    (webgl_links().webgl_get_context)(scope, js, canvas_obj, canvas, version, attrs)
}
