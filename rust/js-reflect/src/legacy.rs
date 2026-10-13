//! Southstar — the C-ABI accessors the raw QuickJS callers still bind, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::JsResult;
use crate::ffi::{self, c_str};

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> JsResult,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

unsafe fn setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value, &Value) -> JsResult<()>,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let val = quickjs::borrow_value(scope, val);
            let result = f(scope, &this, &val).map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
        })
    }
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { getter(ctx, this_val, $f) }
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
                unsafe { setter(ctx, this_val, val, $f) }
            }
        )*
    };
}

macro_rules! magic_accessors {
    ($($get:ident, $set:ident => $g:path, $s:path);* $(;)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $get(
                ctx: *mut JSContext,
                this_val: JSValue,
                magic: c_int,
            ) -> JSValue {
                unsafe { getter(ctx, this_val, |scope, this| $g(scope, this, magic)) }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $set(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
                magic: c_int,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, val, |scope, this, val| $s(scope, this, val, magic)) }
            }
        )*
    };
}

magic_accessors! {
    ns_element_attr_getter, ns_element_attr_setter => ffi::nsr_element_attr_getter, ffi::nsr_element_attr_setter;
    ns_element_enum_getter, ns_element_enum_setter => ffi::nsr_element_enum_getter, ffi::nsr_element_enum_setter;
    ns_element_aria_string_getter, ns_element_aria_string_setter => ffi::nsr_element_aria_string_getter, ffi::nsr_element_aria_string_setter;
    ns_element_int_attr_getter, ns_element_int_attr_setter => ffi::nsr_element_int_attr_getter, ffi::nsr_element_int_attr_setter;
    ns_element_bool_attr_getter, ns_element_bool_attr_setter => ffi::nsr_element_bool_attr_getter, ffi::nsr_element_bool_attr_setter;
    ns_element_boolattr_getter, ns_element_boolattr_setter => ffi::nsr_element_boolattr_getter, ffi::nsr_element_boolattr_setter;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_dimension_setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        setter(ctx, this_val, val, |scope, this, val| {
            ffi::nsr_element_dimension_setter(scope, this, val, magic)
        })
    }
}

getters! {
    ns_element_get_autocomplete => ffi::nsr_element_get_autocomplete,
    ns_element_get_dir => ffi::nsr_element_get_dir,
    ns_element_get_translate => ffi::nsr_element_get_translate,
    ns_element_get_type => ffi::nsr_element_get_type,
    ns_element_get_autocapitalize => ffi::nsr_element_get_autocapitalize,
    ns_element_get_spellcheck => ffi::nsr_element_get_spellcheck,
    ns_element_get_htmlFor => ffi::nsr_element_get_htmlFor,
    ns_element_get_draggable => ffi::nsr_element_get_draggable,
    ns_element_get_contentEditable => ffi::nsr_element_get_contentEditable,
    ns_element_get_id => ffi::nsr_element_get_id,
    ns_element_get_className => ffi::nsr_element_get_className,
    ns_element_get_tabIndex => ffi::nsr_element_get_tabIndex,
}

setters! {
    ns_element_set_autocomplete => ffi::nsr_element_set_autocomplete,
    ns_element_set_dir => ffi::nsr_element_set_dir,
    ns_element_set_translate => ffi::nsr_element_set_translate,
    ns_element_set_autocapitalize => ffi::nsr_element_set_autocapitalize,
    ns_element_set_spellcheck => ffi::nsr_element_set_spellcheck,
    ns_element_set_htmlFor => ffi::nsr_element_set_htmlFor,
    ns_element_set_draggable => ffi::nsr_element_set_draggable,
    ns_element_set_contentEditable => ffi::nsr_element_set_contentEditable,
    ns_element_set_id => ffi::nsr_element_set_id,
    ns_element_set_className => ffi::nsr_element_set_className,
    ns_element_attr_setter_sizes => ffi::nsr_element_attr_setter_sizes,
    ns_element_attr_setter_sandbox => ffi::nsr_element_attr_setter_sandbox,
    ns_element_set_tabIndex => ffi::nsr_element_set_tabIndex,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_reflect_str_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    attr: *const c_char,
    null_if_absent: GBoolean,
) -> JSValue {
    let attr = c_str(attr).unwrap_or(c"");
    unsafe {
        getter(ctx, this_val, |scope, this| {
            ffi::nsr_element_reflect_str_get(scope, this, attr, null_if_absent != glib::FALSE)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_reflect_str_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    attr: *const c_char,
) -> JSValue {
    let attr = c_str(attr).unwrap_or(c"");
    unsafe {
        setter(ctx, this_val, val, |scope, this, val| {
            ffi::nsr_element_reflect_str_set(scope, this, val, attr)
        })
    }
}
