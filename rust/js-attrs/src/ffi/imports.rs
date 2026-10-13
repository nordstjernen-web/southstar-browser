//! Southstar — the Rust-ABI entry points of the binding crates the attribute bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

use super::NsJs;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_live_owner_node(obj: &Value) -> *mut NsNode;
    pub(crate) safe fn nsr_element_reflect_str_get(
        scope: &mut Scope<'_>,
        this: &Value,
        attr: &CStr,
        null_if_absent: bool,
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_attribute_map_lookup(
        js: *mut NsJs,
        owner: *const NsNode,
    ) -> Option<Value>;
    pub(crate) unsafe fn nsr_attribute_map_remember(
        js: *mut NsJs,
        owner: *const NsNode,
        map: &Value,
    );
    pub(crate) safe fn nsr_make_live(
        scope: &mut Scope<'_>,
        owner: &Value,
        kind: c_int,
        param: Option<&[u8]>,
        param2: Option<&[u8]>,
    ) -> Value;
}
