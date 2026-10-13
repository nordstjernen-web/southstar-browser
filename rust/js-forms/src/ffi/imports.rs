//! Southstar — the Rust-ABI entry points of the engine crates the forms bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

use super::NsJs;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_make_element(
        scope: &mut Scope<'_>,
        node: *const NsNode,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_unwrap_element(value: &Value) -> *const NsNode;
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
    pub(crate) safe fn nsr_blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8>;
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
    pub(crate) safe fn nsr_make_live(
        scope: &mut Scope<'_>,
        owner: &Value,
        kind: c_int,
        param: Option<&[u8]>,
        param2: Option<&[u8]>,
    ) -> Value;
    pub(crate) safe fn nsr_live_snapshot(scope: &mut Scope<'_>, obj: &Value) -> Value;
    pub(crate) safe fn nsr_live_owner(obj: &Value) -> Value;
    pub(crate) safe fn nsr_qcache_get(
        scope: &mut Scope<'_>,
        root: *const c_void,
        kind: u8,
        key: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_qcache_put(
        scope: &mut Scope<'_>,
        root: *const c_void,
        kind: u8,
        key: &[u8],
        value: &Value,
    );
}
