//! Southstar — the C ABI of the Temporal API for the QuickJS engine, as declared in src/js_date.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_temporal_install(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let _ = crate::install(scope, &global);
        });
    }
}
