//! Southstar — the C ABI shims of the attribute bindings for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::{NsAttr, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi;

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn with_value<R>(
    ctx: *mut JSContext,
    value: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> R,
) -> R {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value)
        })
    }
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
                unsafe { native(ctx, this_val, argc, argv, $f) }
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
                    with_value(ctx, this_val, |scope, this| {
                        let result = $f(scope, this);
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
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        let this = quickjs::borrow_value(scope, this_val);
                        let value = quickjs::borrow_value(scope, val);
                        let result = $f(scope, &this, &value).map(|()| Value::undefined());
                        quickjs::result_raw(scope, result)
                    })
                }
            }
        )*
    };
}

methods! {
    ns_namedmap_getNamedItem => ffi::nsr_namedmap_getNamedItem,
    ns_namedmap_getNamedItemNS => ffi::nsr_namedmap_getNamedItemNS,
    ns_namedmap_setNamedItem => ffi::nsr_namedmap_setNamedItem,
    ns_namedmap_removeNamedItem => ffi::nsr_namedmap_removeNamedItem,
    ns_namedmap_removeNamedItemNS => ffi::nsr_namedmap_removeNamedItemNS,
    ns_namedmap_item => ffi::nsr_namedmap_item,
    ns_element_getAttributeNode => ffi::nsr_element_getAttributeNode,
    ns_element_getAttributeNodeNS => ffi::nsr_element_getAttributeNodeNS,
    ns_element_setAttributeNode => ffi::nsr_element_setAttributeNode,
    ns_element_removeAttributeNode => ffi::nsr_element_removeAttributeNode,
}

getters! {
    ns_namedmap_get_length => ffi::nsr_namedmap_get_length,
    ns_element_get_classList => ffi::nsr_element_get_classList,
    ns_element_get_relList => ffi::nsr_element_get_relList,
    ns_element_get_sandbox_list => ffi::nsr_element_get_sandbox_list,
    ns_element_get_sizes_list => ffi::nsr_element_get_sizes_list,
    ns_element_get_attributes => ffi::nsr_element_get_attributes,
    ns_element_get_dataset => ffi::nsr_element_get_dataset,
}

setters! {
    ns_element_set_style => ffi::nsr_element_set_style,
    ns_element_set_classList => ffi::nsr_element_set_classList,
    ns_element_set_relList => ffi::nsr_element_set_relList,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_token_list(
    ctx: *mut JSContext,
    element: JSValue,
    attr: *const c_char,
) -> JSValue {
    if attr.is_null() {
        return quickjs::into_raw(Value::null());
    }
    let attr = unsafe { CStr::from_ptr(attr) };
    unsafe {
        with_value(ctx, element, |scope, element| {
            let result = crate::element::token_list(scope, element, attr);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_token_list_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            ffi::nsr_token_list_register_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_token_list_class_proto(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_token_list_class_proto(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            ffi::nsr_dataset_register_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_class_proto(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_dataset_class_proto(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            ffi::nsr_attr_register_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_to_js(
    ctx: *mut JSContext,
    owner: JSValue,
    attr: *const NsAttr,
    include_base: GBoolean,
) -> JSValue {
    if attr.is_null() {
        return quickjs::UNDEFINED;
    }
    unsafe {
        with_value(ctx, owner, |scope, owner| {
            let result = ffi::nsr_attr_to_js(scope, owner, attr, include_base != 0);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_owner(value: JSValue) -> *mut NsNode {
    unsafe { quickjs::with_raw(value, ffi::attr_owner_node) }
}
