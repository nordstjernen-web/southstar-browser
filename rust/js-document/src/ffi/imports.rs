//! Southstar — the Rust-ABI entry points of the binding crates the document bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

macro_rules! natives {
    ($($name:ident),* $(,)?) => {
        unsafe extern "Rust" {
            $(
                #[allow(non_snake_case)]
                pub(crate) safe fn $name(
                    scope: &mut Scope<'_>,
                    this: &Value,
                    args: &[Value],
                ) -> Result<Value, Value>;
            )*
        }
    };
}

natives! {
    nsr_document_createElement,
    nsr_document_createElementNS,
    nsr_document_createTextNode,
    nsr_document_createComment,
    nsr_document_createDocumentFragment,
    nsr_document_createCDATASection,
    nsr_document_createProcessingInstruction,
    nsr_document_createAttribute,
    nsr_document_createAttributeNS,
    nsr_document_create_range,
    nsr_document_createEvent,
    nsr_document_import_node,
    nsr_document_adopt_node,
    nsr_element_querySelector,
    nsr_element_querySelectorAll,
    nsr_element_getElementsByTagName,
    nsr_element_getElementsByClassName,
    nsr_document_create_tree_walker,
    nsr_document_create_node_iterator,
    nsr_window_get_selection,
    nsr_document_has_focus,
    nsr_window_open_method,
}

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) unsafe fn nsr_iframe_realm_window(
        scope: &mut Scope<'_>,
        element: &Value,
        node: *mut NsNode,
    ) -> Value;
    pub(crate) unsafe fn nsr_document_expose_legacy_named(
        scope: &mut Scope<'_>,
        root: *const NsNode,
        document: &Value,
    );
    pub(crate) safe fn nsr_make_live(
        scope: &mut Scope<'_>,
        owner: &Value,
        kind: c_int,
        param: Option<&[u8]>,
        param2: Option<&[u8]>,
    ) -> Value;
    pub(crate) safe fn nsr_document_implementation(
        scope: &mut Scope<'_>,
        this: &Value,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_document_install_funcs(scope: &mut Scope<'_>, doc: &Value);
}
