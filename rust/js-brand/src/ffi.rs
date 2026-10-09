//! Southstar — the C ABI of the interface brand checks, as declared in src/js_brand.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs::{self, JSContext};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_brand_node_interfaces(
    ctx: *mut JSContext,
    element_cid: u32,
    attr_cid: u32,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::brand_node_interfaces(scope, element_cid, attr_cid);
        });
    }
}
