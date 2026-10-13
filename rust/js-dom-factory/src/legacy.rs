//! Southstar — the C ABI shims of the document factories for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::Value;
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

methods! {
    ns_document_createElement => ffi::nsr_document_createElement,
    ns_document_createElementNS => ffi::nsr_document_createElementNS,
    ns_document_createTextNode => ffi::nsr_document_createTextNode,
    ns_document_createComment => ffi::nsr_document_createComment,
    ns_document_createCDATASection => ffi::nsr_document_createCDATASection,
    ns_document_createProcessingInstruction => ffi::nsr_document_createProcessingInstruction,
    ns_document_createAttribute => ffi::nsr_document_createAttribute,
    ns_document_createAttributeNS => ffi::nsr_document_createAttributeNS,
    ns_document_createEvent => ffi::nsr_document_createEvent,
    ns_document_createDocumentFragment => ffi::nsr_document_createDocumentFragment,
    ns_document_import_node => ffi::nsr_document_import_node,
    ns_document_adopt_node => ffi::nsr_document_adopt_node,
    ns_element_cloneNode => ffi::nsr_element_cloneNode,
    ns_element_lookupNamespaceURI => ffi::nsr_element_lookupNamespaceURI,
    ns_element_lookupPrefix => ffi::nsr_element_lookupPrefix,
    ns_element_isDefaultNamespace => ffi::nsr_element_isDefaultNamespace,
    ns_impl_create_html_document => ffi::nsr_impl_create_html_document,
    ns_impl_create_document => ffi::nsr_impl_create_document,
    ns_impl_create_document_type => ffi::nsr_impl_create_document_type,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_implementation(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = ffi::nsr_document_implementation(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_anchor_part_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = ffi::nsr_element_anchor_part_get(scope, &this, magic);
            quickjs::result_raw(scope, result)
        })
    }
}

unsafe fn with_this_val(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
    f: fn(&mut southstar_js_engine::Scope<'_>, &Value, &Value, i32) -> Result<(), Value>,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let val = quickjs::borrow_value(scope, val);
            let result = f(scope, &this, &val, magic).map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_anchor_href_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe { with_this_val(ctx, this_val, val, magic, ffi::nsr_element_anchor_href_set) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_url_part_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe { with_this_val(ctx, this_val, val, magic, ffi::nsr_element_url_part_set) }
}
