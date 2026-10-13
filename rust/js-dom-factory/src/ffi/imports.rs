//! Southstar — the Rust-ABI entry points of the binding crates the document factories call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{NsAttr, NsNode};
use southstar_js_engine::{Scope, Value};

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) unsafe fn nsr_attr_to_js(
        scope: &mut Scope<'_>,
        owner: &Value,
        attr: *const NsAttr,
        include_base: bool,
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_make_realm_document(
        scope: &mut Scope<'_>,
        doc: *mut NsNode,
        url: Option<&[u8]>,
        charset: Option<&[u8]>,
        content_type: Option<&[u8]>,
        is_xml: bool,
        inert: bool,
    ) -> Value;
    pub(crate) safe fn nsr_make_synth_xml_document(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_element_reflect_str_get(
        scope: &mut Scope<'_>,
        this: &Value,
        attr: &CStr,
        null_if_absent: bool,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_addEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_removeEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}
