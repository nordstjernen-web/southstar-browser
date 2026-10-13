//! Southstar — the C ABI shims of the node bindings for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use crate::ffi;

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        let this = quickjs::borrow_value(scope, this_val);
                        let result = $f(scope, &this);
                        quickjs::result_raw(scope, result)
                    })
                }
            }
        )*
    };
}

methods! {
    ns_element_appendChild => ffi::nsr_element_appendChild,
    ns_element_removeChild => ffi::nsr_element_removeChild,
    ns_element_insertBefore => ffi::nsr_element_insertBefore,
    ns_element_replaceChild => ffi::nsr_element_replaceChild,
    ns_element_moveBefore => ffi::nsr_element_moveBefore,
    ns_element_before => ffi::nsr_element_before,
    ns_element_after => ffi::nsr_element_after,
    ns_element_replaceWith => ffi::nsr_element_replaceWith,
    ns_element_remove_self => ffi::nsr_element_remove_self,
    ns_element_append => ffi::nsr_element_append,
    ns_element_prepend => ffi::nsr_element_prepend,
    ns_element_replaceChildren => ffi::nsr_element_replaceChildren,
    ns_element_setAttribute => ffi::nsr_element_setAttribute,
    ns_element_removeAttribute => ffi::nsr_element_removeAttribute,
    ns_element_toggleAttribute => ffi::nsr_element_toggleAttribute,
    ns_element_getAttributeNS => ffi::nsr_element_getAttributeNS,
    ns_element_hasAttributeNS => ffi::nsr_element_hasAttributeNS,
    ns_element_setAttributeNS => ffi::nsr_element_setAttributeNS,
    ns_element_removeAttributeNS => ffi::nsr_element_removeAttributeNS,
    ns_element_getAttributeNames => ffi::nsr_element_getAttributeNames,
    ns_element_hasAttributes => ffi::nsr_element_hasAttributes,
    ns_element_isSameNode => ffi::nsr_element_isSameNode,
    ns_element_getAttribute => ffi::nsr_element_getAttribute,
    ns_element_hasAttribute => ffi::nsr_element_hasAttribute,
    ns_element_contains => ffi::nsr_element_contains,
    ns_element_hasChildNodes => ffi::nsr_element_hasChildNodes,
    ns_element_isEqualNode => ffi::nsr_element_isEqualNode,
    ns_element_compareDocumentPosition => ffi::nsr_element_compareDocumentPosition,
}

getters! {
    ns_element_get_parentElement => ffi::nsr_element_get_parentElement,
    ns_element_get_parentNode => ffi::nsr_element_get_parentNode,
    ns_element_get_firstChild => ffi::nsr_element_get_firstChild,
    ns_element_get_lastChild => ffi::nsr_element_get_lastChild,
    ns_element_get_nextSibling => ffi::nsr_element_get_nextSibling,
    ns_element_get_previousSibling => ffi::nsr_element_get_previousSibling,
    ns_element_get_firstElementChild => ffi::nsr_element_get_firstElementChild,
    ns_element_get_lastElementChild => ffi::nsr_element_get_lastElementChild,
    ns_element_get_nextElementSibling => ffi::nsr_element_get_nextElementSibling,
    ns_element_get_previousElementSibling => ffi::nsr_element_get_previousElementSibling,
    ns_element_get_nodeName => ffi::nsr_element_get_nodeName,
    ns_element_get_localName => ffi::nsr_element_get_localName,
    ns_element_get_prefix => ffi::nsr_element_get_prefix,
    ns_element_get_namespaceURI => ffi::nsr_element_get_namespaceURI,
    ns_element_get_tagName => ffi::nsr_element_get_tagName,
    ns_element_get_ownerDocument => ffi::nsr_element_get_ownerDocument,
    ns_element_get_baseURI => ffi::nsr_element_get_baseURI,
    ns_element_template_content => ffi::nsr_element_template_content,
    ns_element_get_children => ffi::nsr_element_get_children,
    ns_element_get_childNodes => ffi::nsr_element_get_childNodes,
    ns_element_get_nodeType => ffi::nsr_element_get_nodeType,
    ns_element_get_childElementCount => ffi::nsr_element_get_childElementCount,
    ns_element_get_isConnected => ffi::nsr_element_get_isConnected,
}
