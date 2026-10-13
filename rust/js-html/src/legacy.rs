//! Southstar — the C-ABI QuickJS shims of the markup, text and microdata natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, NativeFn, SetterFn, Value};

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                let f: GetterFn = $f;
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        let this = quickjs::borrow_value(scope, this_val);
                        let result = f(scope, &this);
                        quickjs::result_raw(scope, result)
                    })
                }
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
            ) -> JSValue {
                let f: SetterFn = $f;
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        let this = quickjs::borrow_value(scope, this_val);
                        let val = quickjs::borrow_value(scope, val);
                        let result = f(scope, &this, &val).map(|()| Value::undefined());
                        quickjs::result_raw(scope, result)
                    })
                }
            }
        )*
    };
}

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
                let f: NativeFn = $f;
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
            }
        )*
    };
}

getters! {
    ns_element_get_textContent => crate::ffi::nsr_element_get_textContent,
    ns_element_get_text => crate::ffi::nsr_element_get_text,
    ns_element_get_innerHTML => crate::ffi::nsr_element_get_innerHTML,
    ns_element_get_outerHTML => crate::ffi::nsr_element_get_outerHTML,
    ns_element_get_itemScope => crate::ffi::nsr_element_get_itemScope,
    ns_element_get_itemId => crate::ffi::nsr_element_get_itemId,
    ns_element_get_itemType => crate::ffi::nsr_element_get_itemType,
    ns_element_get_itemProp => crate::ffi::nsr_element_get_itemProp,
    ns_element_get_itemRef => crate::ffi::nsr_element_get_itemRef,
    ns_element_get_itemValue => crate::ffi::nsr_element_get_itemValue,
    ns_element_get_properties => crate::ffi::nsr_element_get_properties,
}

setters! {
    ns_element_set_textContent => crate::ffi::nsr_element_set_textContent,
    ns_element_set_text => crate::ffi::nsr_element_set_text,
    ns_element_set_innerHTML => crate::ffi::nsr_element_set_innerHTML,
    ns_element_set_outerHTML => crate::ffi::nsr_element_set_outerHTML,
    ns_element_set_itemScope => crate::ffi::nsr_element_set_itemScope,
    ns_element_set_itemId => crate::ffi::nsr_element_set_itemId,
    ns_element_set_itemValue => crate::ffi::nsr_element_set_itemValue,
}

methods! {
    ns_element_getHTML => crate::ffi::nsr_element_getHTML,
    ns_element_setHTMLUnsafe => crate::ffi::nsr_element_setHTMLUnsafe,
    ns_element_insertAdjacentHTML => crate::ffi::nsr_element_insertAdjacentHTML,
    ns_element_insertAdjacentElement => crate::ffi::nsr_element_insertAdjacentElement,
    ns_element_insertAdjacentText => crate::ffi::nsr_element_insertAdjacentText,
    ns_document_getItems => crate::ffi::nsr_document_getItems,
    ns_xml_serializer_ctor => crate::ffi::nsr_xml_serializer_ctor,
}
