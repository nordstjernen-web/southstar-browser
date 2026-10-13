//! Southstar — the forms bindings' C-ABI shims over raw QuickJS, kept for the crates that still bind them that way.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::NsNode;
use southstar_glib as glib;
use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

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
                    quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
                        $f(scope, this)
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
                let mut val = val;
                unsafe {
                    quickjs::call_native(ctx, this_val, 1, &mut val, |scope, this, args| {
                        $f(scope, this, &args[0]).map(|()| Value::undefined())
                    })
                }
            }
        )*
    };
}

methods! {
    ns_element_attachInternals => super::nsr_element_attachInternals,
    ns_element_check_validity => super::nsr_element_check_validity,
    ns_element_setCustomValidity => super::nsr_element_setCustomValidity,
    ns_element_form_requestSubmit => super::nsr_element_form_requestSubmit,
    ns_element_form_submit => super::nsr_element_form_submit,
    ns_element_form_reset => super::nsr_element_form_reset,
    ns_submit_event_ctor => super::nsr_submit_event_ctor,
    ns_input_select => super::nsr_input_select,
    ns_input_setSelectionRange => super::nsr_input_setSelectionRange,
    ns_input_setRangeText => super::nsr_input_setRangeText,
    ns_input_stepUp => super::nsr_input_stepUp,
    ns_input_stepDown => super::nsr_input_stepDown,
    ns_options_item => super::nsr_options_item,
    ns_options_namedItem => super::nsr_options_namedItem,
    ns_select_add => super::nsr_select_add,
    ns_element_show_picker => super::nsr_element_show_picker,
}

getters! {
    ns_element_get_internals => super::nsr_element_get_internals,
    ns_validity_get_valid => super::nsr_validity_get_valid,
    ns_element_get_validity => super::nsr_element_get_validity,
    ns_element_get_validation_message => super::nsr_element_get_validation_message,
    ns_element_get_will_validate => super::nsr_element_get_will_validate,
    ns_element_get_label_control => super::nsr_element_get_label_control,
    ns_element_get_selection_dir => super::nsr_element_get_selection_dir,
    ns_element_get_default_value => super::nsr_element_get_default_value,
    ns_element_get_default_checked => super::nsr_element_get_default_checked,
    ns_element_get_default_selected => super::nsr_element_get_default_selected,
    ns_element_get_selected => super::nsr_element_get_selected,
    ns_element_get_value_as_number => super::nsr_element_get_value_as_number,
    ns_element_get_value_as_date => super::nsr_element_get_value_as_date,
    ns_element_get_checked => super::nsr_element_get_checked,
    ns_element_get_indeterminate => super::nsr_element_get_indeterminate,
    ns_element_get_progress_position => super::nsr_element_get_progress_position,
    ns_element_get_value_prop => super::nsr_element_get_value_prop,
    ns_element_get_label_prop => super::nsr_element_get_label_prop,
    ns_element_get_selectedIndex => super::nsr_element_get_selectedIndex,
    ns_element_get_options => super::nsr_element_get_options,
    ns_element_get_selectedOptions => super::nsr_element_get_selectedOptions,
    ns_element_get_selection_start => super::nsr_element_get_selection_start,
    ns_element_get_selection_end => super::nsr_element_get_selection_end,
    ns_text_control_get_text_length => super::nsr_text_control_get_text_length,
    ns_input_get_files => super::nsr_input_get_files,
    ns_element_get_form_elements => super::nsr_element_get_form_elements,
}

setters! {
    ns_element_set_default_value => super::nsr_element_set_default_value,
    ns_element_set_default_checked => super::nsr_element_set_default_checked,
    ns_element_set_default_selected => super::nsr_element_set_default_selected,
    ns_element_set_selected => super::nsr_element_set_selected,
    ns_element_set_value_as_number => super::nsr_element_set_value_as_number,
    ns_element_set_value_as_date => super::nsr_element_set_value_as_date,
    ns_element_set_checked => super::nsr_element_set_checked,
    ns_element_set_indeterminate => super::nsr_element_set_indeterminate,
    ns_element_set_value_prop => super::nsr_element_set_value_prop,
    ns_element_set_label_prop => super::nsr_element_set_label_prop,
    ns_element_set_selectedIndex => super::nsr_element_set_selectedIndex,
    ns_element_set_selection_start => super::nsr_element_set_selection_start,
    ns_element_set_selection_end => super::nsr_element_set_selection_end,
    ns_element_set_selection_dir => super::nsr_element_set_selection_dir,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_range_number_getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
            super::nsr_element_range_number_getter(scope, this, magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_range_number_setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    let mut val = val;
    unsafe {
        quickjs::call_native(ctx, this_val, 1, &mut val, |scope, this, args| {
            super::nsr_element_range_number_setter(scope, this, &args[0], magic)
                .map(|()| Value::undefined())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_form_data(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            super::nsr_net_install_form_data(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_data_serialize(
    ctx: *mut JSContext,
    fd: JSValue,
    out_len: *mut usize,
    out_content_type: *mut *mut c_char,
) -> *mut c_char {
    let (body, content_type) = unsafe {
        quickjs::with_context(ctx, |scope| {
            let fd = quickjs::borrow_value(scope, fd);
            super::nsr_js_form_data_serialize(scope, &fd)
        })
    };
    if !out_content_type.is_null() {
        unsafe { *out_content_type = glib::strdup(&content_type) };
    }
    if !out_len.is_null() {
        unsafe { *out_len = body.len() };
    }
    let out = unsafe { glib::g_malloc(body.len() + 1) }.cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(body.as_ptr(), out, body.len());
        *out.add(body.len()) = 0;
    }
    out.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_submit_form(
    ctx: *mut JSContext,
    form: *const NsNode,
    submitter: *const NsNode,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            super::nsr_js_request_submit_form(scope, form, submitter);
            quickjs::into_raw(Value::undefined())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_reset_form(ctx: *mut JSContext, form: *mut NsNode) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            super::nsr_js_reset_form(scope, form);
            quickjs::into_raw(Value::undefined())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_elements_named_lookup(
    ctx: *mut JSContext,
    this_val: JSValue,
    name: *const c_char,
) -> JSValue {
    if name.is_null() {
        return quickjs::into_raw(Value::null());
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let elements = quickjs::borrow_value(scope, this_val);
            let result = super::nsr_form_elements_named_lookup(scope, &elements, name);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_controls_snapshot(ctx: *mut JSContext, form: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let form = quickjs::borrow_value(scope, form);
            let result = super::nsr_form_controls_snapshot(scope, &form);
            quickjs::result_raw(scope, result)
        })
    }
}
