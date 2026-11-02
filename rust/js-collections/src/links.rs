//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsFormsLinks {
    pub form_elements_named_lookup: fn(&mut Scope<'_>, &Value, &[u8]) -> Result<Value, Value>,
}

static JS_FORMS_LINKS: OnceLock<JsFormsLinks> = OnceLock::new();

pub fn set_js_forms_links(links: JsFormsLinks) {
    let _ = JS_FORMS_LINKS.set(links);
}

fn js_forms_links() -> &'static JsFormsLinks {
    JS_FORMS_LINKS
        .get()
        .expect("southstar-js-forms installs its links before the first page")
}

pub(crate) fn nsr_form_elements_named_lookup(
    scope: &mut Scope<'_>,
    elements: &Value,
    name: &[u8],
) -> Result<Value, Value> {
    (js_forms_links().form_elements_named_lookup)(scope, elements, name)
}
