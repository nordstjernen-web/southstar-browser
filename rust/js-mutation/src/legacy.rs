//! Southstar — the C-ABI QuickJS shims of the NamedNodeMap cache, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use crate::ffi::NsJs;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attribute_map_lookup(js: *mut NsJs, owner: *const NsNode) -> JSValue {
    quickjs::into_raw(unsafe { crate::ffi::nsr_attribute_map_lookup(js, owner) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attribute_map_remember(
    ctx: *mut JSContext,
    js: *mut NsJs,
    owner: *const NsNode,
    map: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let map = quickjs::borrow_value(scope, map);
            crate::ffi::nsr_attribute_map_remember(js, owner, &map);
        });
    }
}
