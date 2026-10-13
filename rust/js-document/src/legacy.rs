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

methods! {
    ns_document_open => ffi::nsr_document_open,
    ns_document_close => ffi::nsr_document_close,
    ns_document_write => ffi::nsr_document_write,
    ns_document_writeln => ffi::nsr_document_writeln,
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
