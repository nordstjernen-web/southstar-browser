//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

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

pub struct JsShadowLinks {
    pub element_get_shadow_root: fn(&mut Scope<'_>, &Value) -> Result<Value, Value>,
}

static JS_SHADOW_LINKS: OnceLock<JsShadowLinks> = OnceLock::new();

pub fn set_js_shadow_links(links: JsShadowLinks) {
    let _ = JS_SHADOW_LINKS.set(links);
}

fn js_shadow_links() -> &'static JsShadowLinks {
    JS_SHADOW_LINKS
        .get()
        .expect("southstar-js-shadow installs its links before the first page")
}

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_element_get_shadowRoot(
    scope: &mut Scope<'_>,
    this: &Value,
) -> Result<Value, Value> {
    (js_shadow_links().element_get_shadow_root)(scope, this)
}
