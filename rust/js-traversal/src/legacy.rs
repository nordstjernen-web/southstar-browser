//! Southstar — the C-ABI QuickJS shims of the Range, Selection, TreeWalker and NodeIterator natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::NativeFn;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

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

natives! {
    ns_window_get_selection => crate::ffi::nsr_window_get_selection,
    ns_document_create_range => crate::ffi::nsr_document_create_range,
    ns_native_range => crate::ffi::nsr_native_range,
    ns_document_create_tree_walker => crate::ffi::nsr_document_create_tree_walker,
    ns_document_create_node_iterator => crate::ffi::nsr_document_create_node_iterator,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tree_walker_install_proto(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            crate::ffi::nsr_tree_walker_install_proto(scope, &proto);
        })
    }
}
