//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsDomFactoryLinks {
    pub document_implementation: fn(&mut Scope<'_>, &Value) -> Result<Value, Value>,
}

static JS_DOM_FACTORY_LINKS: OnceLock<JsDomFactoryLinks> = OnceLock::new();

pub fn set_js_dom_factory_links(links: JsDomFactoryLinks) {
    let _ = JS_DOM_FACTORY_LINKS.set(links);
}

fn js_dom_factory_links() -> &'static JsDomFactoryLinks {
    JS_DOM_FACTORY_LINKS
        .get()
        .expect("southstar-js-dom-factory installs its links before the first page")
}

pub struct JsInterfacesLinks {
    pub document_install_funcs: fn(&mut Scope<'_>, &Value),
}

static JS_INTERFACES_LINKS: OnceLock<JsInterfacesLinks> = OnceLock::new();

pub fn set_js_interfaces_links(links: JsInterfacesLinks) {
    let _ = JS_INTERFACES_LINKS.set(links);
}

fn js_interfaces_links() -> &'static JsInterfacesLinks {
    JS_INTERFACES_LINKS
        .get()
        .expect("southstar-js-interfaces installs its links before the first page")
}

pub(crate) fn nsr_document_implementation(
    scope: &mut Scope<'_>,
    this: &Value,
) -> Result<Value, Value> {
    (js_dom_factory_links().document_implementation)(scope, this)
}

pub(crate) fn nsr_document_install_funcs(scope: &mut Scope<'_>, doc: &Value) {
    (js_interfaces_links().document_install_funcs)(scope, doc)
}
