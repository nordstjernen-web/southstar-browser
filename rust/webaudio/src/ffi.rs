//! Southstar — the C ABI of offline Web Audio rendering, as declared in src/js_internal.h, over a QuickJS context the bindings lend.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webaudio_render_offline(
    ctx: *mut JSContext,
    destination: JSValue,
    frames: u32,
    rate: f64,
    out: *mut f32,
) -> c_int {
    if ctx.is_null() || out.is_null() || frames == 0 || !crate::positive(rate) {
        return 0;
    }
    let out = unsafe { core::slice::from_raw_parts_mut(out, frames as usize) };
    let rendered = unsafe {
        quickjs::with_context(ctx, |scope| {
            let destination = quickjs::borrow_value(scope, destination);
            crate::render_offline(scope, &destination, rate, out)
        })
    };
    c_int::from(rendered)
}
