//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_dom::AttrView;
use southstar_js_engine::{Scope, Value};

pub struct JsAttrsLinks {
    pub attr_to_js: fn(&mut Scope<'_>, &Value, AttrView<'_>, bool) -> Result<Value, Value>,
}

static JS_ATTRS_LINKS: OnceLock<JsAttrsLinks> = OnceLock::new();

pub fn set_js_attrs_links(links: JsAttrsLinks) {
    let _ = JS_ATTRS_LINKS.set(links);
}

fn js_attrs_links() -> &'static JsAttrsLinks {
    JS_ATTRS_LINKS
        .get()
        .expect("southstar-js-attrs installs its links before the first page")
}

pub(crate) fn nsr_attr_to_js(
    scope: &mut Scope<'_>,
    owner: &Value,
    attr: AttrView<'_>,
    include_base: bool,
) -> Result<Value, Value> {
    (js_attrs_links().attr_to_js)(scope, owner, attr, include_base)
}
