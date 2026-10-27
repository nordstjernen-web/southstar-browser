//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsElementsLinks {
    pub element_img_natural_height: fn(&mut Scope<'_>, &Value) -> Result<Value, Value>,
    pub element_img_natural_width: fn(&mut Scope<'_>, &Value) -> Result<Value, Value>,
}

static JS_ELEMENTS_LINKS: OnceLock<JsElementsLinks> = OnceLock::new();

pub fn set_js_elements_links(links: JsElementsLinks) {
    let _ = JS_ELEMENTS_LINKS.set(links);
}

fn js_elements_links() -> &'static JsElementsLinks {
    JS_ELEMENTS_LINKS
        .get()
        .expect("southstar-js-elements installs its links before the first page")
}

pub struct JsReflectLinks {
    pub element_int_attr_getter: fn(&mut Scope<'_>, &Value, i32) -> Result<Value, Value>,
}

static JS_REFLECT_LINKS: OnceLock<JsReflectLinks> = OnceLock::new();

pub fn set_js_reflect_links(links: JsReflectLinks) {
    let _ = JS_REFLECT_LINKS.set(links);
}

fn js_reflect_links() -> &'static JsReflectLinks {
    JS_REFLECT_LINKS
        .get()
        .expect("southstar-js-reflect installs its links before the first page")
}

pub(crate) fn nsr_element_img_natural_height(
    scope: &mut Scope<'_>,
    this: &Value,
) -> Result<Value, Value> {
    (js_elements_links().element_img_natural_height)(scope, this)
}

pub(crate) fn nsr_element_img_natural_width(
    scope: &mut Scope<'_>,
    this: &Value,
) -> Result<Value, Value> {
    (js_elements_links().element_img_natural_width)(scope, this)
}

pub(crate) fn nsr_element_int_attr_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    magic: i32,
) -> Result<Value, Value> {
    (js_reflect_links().element_int_attr_getter)(scope, this, magic)
}
