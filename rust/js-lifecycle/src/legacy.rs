//! Southstar — the C-ABI QuickJS shims of the animation-frame natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::NativeFn;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
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
    ns_window_requestAnimationFrame => crate::ffi::nsr_window_requestAnimationFrame,
    ns_window_cancelAnimationFrame => crate::ffi::nsr_window_cancelAnimationFrame,
    ns_media_request_video_frame_callback => crate::ffi::nsr_media_request_video_frame_callback,
}
