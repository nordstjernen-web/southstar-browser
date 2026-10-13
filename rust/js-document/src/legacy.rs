//! Southstar — the C ABI shims of the document bindings for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::NsNode;
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::ffi;

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
    ns_document_open => ffi::nsr_document_open,
    ns_document_close => ffi::nsr_document_close,
    ns_document_write => ffi::nsr_document_write,
    ns_document_writeln => ffi::nsr_document_writeln,
}

getters! {
    ns_document_get_documentElement => ffi::nsr_document_get_documentElement,
    ns_document_get_body => ffi::nsr_document_get_body,
    ns_document_get_head => ffi::nsr_document_get_head,
    ns_document_get_scrollingElement => ffi::nsr_document_get_scrollingElement,
    ns_document_get_activeElement => ffi::nsr_document_get_activeElement,
    ns_document_get_currentScript => ffi::nsr_document_get_currentScript,
    ns_document_get_scripts => ffi::nsr_document_get_scripts,
    ns_document_get_anchors => ffi::nsr_document_get_anchors,
    ns_document_get_embeds => ffi::nsr_document_get_embeds,
    ns_document_get_plugins => ffi::nsr_document_get_plugins,
    ns_document_get_applets => ffi::nsr_document_get_applets,
    ns_document_get_title => ffi::nsr_document_get_title,
    ns_document_get_dir => ffi::nsr_document_get_dir,
    ns_document_get_cookie => ffi::nsr_document_get_cookie,
    ns_document_get_referrer => ffi::nsr_document_get_referrer,
    ns_document_get_readyState => ffi::nsr_document_get_readyState,
    ns_document_get_designMode => ffi::nsr_document_get_designMode,
    ns_document_get_lastModified => ffi::nsr_document_get_lastModified,
    ns_document_get_xmlVersion => ffi::nsr_document_get_xmlVersion,
    ns_document_get_hidden => ffi::nsr_document_get_hidden,
    ns_document_get_visibilityState => ffi::nsr_document_get_visibilityState,
    ns_document_get_compatMode => ffi::nsr_document_get_compatMode,
    ns_document_get_defaultView => ffi::nsr_document_get_defaultView,
}

setters! {
    ns_document_set_body => ffi::nsr_document_set_body,
    ns_document_set_title => ffi::nsr_document_set_title,
    ns_document_set_dir => ffi::nsr_document_set_dir,
    ns_document_set_cookie => ffi::nsr_document_set_cookie,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_color(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        with_value(ctx, this_val, |scope, this| {
            let result = ffi::nsr_document_get_color(scope, this, magic);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_set_color(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let value = quickjs::borrow_value(scope, val);
            let result = ffi::nsr_document_set_color(scope, &this, &value, magic)
                .map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
        })
    }
}

fn text_arg<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_realm_document(
    ctx: *mut JSContext,
    doc_node: *mut NsNode,
    url: *const c_char,
    charset: *const c_char,
    content_type: *const c_char,
    is_xml: GBoolean,
    inert: GBoolean,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_make_realm_document(
                scope,
                doc_node,
                text_arg(url),
                text_arg(charset),
                text_arg(content_type),
                is_xml != 0,
                inert != 0,
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_realmdoc_deny_cookie(ctx: *mut JSContext, doc: JSValue) {
    unsafe { with_value(ctx, doc, ffi::nsr_realmdoc_deny_cookie) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_lift_methods_to_proto(ctx: *mut JSContext, document: JSValue) {
    unsafe { with_value(ctx, document, ffi::nsr_document_lift_methods_to_proto) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_define_doctype_getter(ctx: *mut JSContext, document: JSValue) {
    unsafe { with_value(ctx, document, ffi::nsr_document_define_doctype_getter) }
}
