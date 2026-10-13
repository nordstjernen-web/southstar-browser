//! Southstar — the C-ABI QuickJS shims of the geometry and scrolling natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, NativeFn, SetterFn, Value};
use southstar_layout::NsBox;

macro_rules! natives {
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

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                let f: GetterFn = $f;
                unsafe {
                    quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
                        f(scope, this)
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
                let mut argv = [val];
                unsafe {
                    quickjs::call_native(ctx, this_val, 1, argv.as_mut_ptr(), |scope, this, args| {
                        let value = args.first().cloned().unwrap_or_else(Value::undefined);
                        f(scope, this, &value).map(|()| Value::undefined())
                    })
                }
            }
        )*
    };
}

natives! {
    ns_element_getBoundingClientRect => crate::ffi::nsr_element_getBoundingClientRect,
    ns_element_getClientRects => crate::ffi::nsr_element_getClientRects,
    ns_element_scroll_to => crate::ffi::nsr_element_scroll_to,
    ns_element_scroll_by => crate::ffi::nsr_element_scroll_by,
    ns_element_scrollIntoView => crate::ffi::nsr_element_scrollIntoView,
    ns_element_getBBox => crate::ffi::nsr_element_getBBox,
    ns_element_getCTM => crate::ffi::nsr_element_getCTM,
    ns_element_getScreenCTM => crate::ffi::nsr_element_getScreenCTM,
    ns_element_getTotalLength => crate::ffi::nsr_element_getTotalLength,
    ns_element_getPointAtLength => crate::ffi::nsr_element_getPointAtLength,
    ns_element_createSVGPoint => crate::ffi::nsr_element_createSVGPoint,
    ns_element_createSVGRect => crate::ffi::nsr_element_createSVGRect,
    ns_element_createSVGMatrix => crate::ffi::nsr_element_createSVGMatrix,
    ns_element_createSVGTransform => crate::ffi::nsr_element_createSVGTransform,
    ns_element_checkVisibility => crate::ffi::nsr_element_checkVisibility,
    ns_svg_beginElement => crate::ffi::nsr_svg_beginElement,
    ns_svg_setCurrentTime => crate::ffi::nsr_svg_setCurrentTime,
    ns_document_element_from_point => crate::ffi::nsr_document_element_from_point,
    ns_document_elements_from_point => crate::ffi::nsr_document_elements_from_point,
}

getters! {
    ns_element_get_offsetWidth => crate::ffi::nsr_element_get_offsetWidth,
    ns_element_get_offsetHeight => crate::ffi::nsr_element_get_offsetHeight,
    ns_element_get_offsetTop => crate::ffi::nsr_element_get_offsetTop,
    ns_element_get_offsetLeft => crate::ffi::nsr_element_get_offsetLeft,
    ns_element_get_offsetParent => crate::ffi::nsr_element_get_offsetParent,
    ns_element_get_clientWidth => crate::ffi::nsr_element_get_clientWidth,
    ns_element_get_clientHeight => crate::ffi::nsr_element_get_clientHeight,
    ns_element_get_clientTop => crate::ffi::nsr_element_get_clientTop,
    ns_element_get_clientLeft => crate::ffi::nsr_element_get_clientLeft,
    ns_element_get_scrollTop => crate::ffi::nsr_element_get_scrollTop,
    ns_element_get_scrollLeft => crate::ffi::nsr_element_get_scrollLeft,
    ns_element_get_scrollWidth => crate::ffi::nsr_element_get_scrollWidth,
    ns_element_get_scrollHeight => crate::ffi::nsr_element_get_scrollHeight,
    ns_element_get_ownerSVGElement => crate::ffi::nsr_element_get_ownerSVGElement,
}

setters! {
    ns_element_set_scrollTop => crate::ffi::nsr_element_set_scrollTop,
    ns_element_set_scrollLeft => crate::ffi::nsr_element_set_scrollLeft,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_dimension_getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
            crate::ffi::nsr_element_dimension_getter(scope, this, magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_dom_rect(
    ctx: *mut JSContext,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::ffi::nsr_make_dom_rect(scope, x, y, w, h))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_for_this(ctx: *mut JSContext, this_val: JSValue) -> *const NsBox {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            crate::ffi::nsr_box_for_this(scope, &this)
        })
    }
}
