//! Southstar — the Rust-ABI entry points of the engine crates the forms bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

use super::NsJs;

pub(crate) use southstar_js_collections::nsr_live_owner;
pub(crate) use southstar_js_collections::nsr_live_snapshot;
pub(crate) use southstar_js_collections::nsr_make_live;
pub(crate) use southstar_js_collections::nsr_qcache_get;
pub(crate) use southstar_js_collections::nsr_qcache_put;
pub(crate) use southstar_js_url::nsr_blob_bytes;

unsafe extern "Rust" {
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_element_get_shadowRoot(
        scope: &mut Scope<'_>,
        this: &Value,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) unsafe fn nsr_make_event(
        scope: &mut Scope<'_>,
        kind: &str,
        target: *const NsNode,
    ) -> Value;
    pub(crate) unsafe fn nsr_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: &str,
        event: Value,
    ) -> (bool, bool);
    pub(crate) safe fn nsr_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_namedmap_item(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}
