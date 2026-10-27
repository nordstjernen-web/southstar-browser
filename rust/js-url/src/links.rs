//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsDispatchLinks {
    pub xhr_fire_progress_event: fn(&mut Scope<'_>, &Value, &str, f64, f64, bool),
}

static JS_DISPATCH_LINKS: OnceLock<JsDispatchLinks> = OnceLock::new();

pub fn set_js_dispatch_links(links: JsDispatchLinks) {
    let _ = JS_DISPATCH_LINKS.set(links);
}

fn js_dispatch_links() -> &'static JsDispatchLinks {
    JS_DISPATCH_LINKS
        .get()
        .expect("southstar-js-dispatch installs its links before the first page")
}

pub(crate) fn nsr_xhr_fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    loaded: f64,
    total: f64,
    length_computable: bool,
) {
    (js_dispatch_links().xhr_fire_progress_event)(
        scope,
        target,
        kind,
        loaded,
        total,
        length_computable,
    )
}
