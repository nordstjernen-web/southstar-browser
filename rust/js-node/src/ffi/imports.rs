//! Southstar — the Rust-ABI entry points of the binding crates the node bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_attr_owner(value: &Value) -> *mut NsNode;
    pub(crate) safe fn nsr_doc_wrapper_is_xml(scope: &mut Scope<'_>, doc: &Value) -> bool;
    pub(crate) safe fn nsr_element_reflect_str_get(
        scope: &mut Scope<'_>,
        this: &Value,
        attr: &CStr,
        null_if_absent: bool,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_make_live(
        scope: &mut Scope<'_>,
        owner: &Value,
        kind: c_int,
        param: Option<&[u8]>,
        param2: Option<&[u8]>,
    ) -> Value;
    pub(crate) safe fn nsr_validate_attr_ns(
        scope: &mut Scope<'_>,
        namespace: Option<&[u8]>,
        qname: &[u8],
    ) -> Result<(), Value>;
}
