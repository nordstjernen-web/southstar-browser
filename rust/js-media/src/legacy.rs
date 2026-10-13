//! Southstar — the C-ABI QuickJS shims of the media natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, NativeFn, SetterFn, Value};

use crate::ffi;

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
                value: JSValue,
            ) -> JSValue {
                let f: SetterFn = $f;
                let mut args = [value];
                unsafe {
                    quickjs::call_native(ctx, this_val, 1, args.as_mut_ptr(), |scope, this, args| {
                        let value = args.first().cloned().unwrap_or_else(Value::undefined);
                        f(scope, this, &value).map(|()| Value::undefined())
                    })
                }
            }
        )*
    };
}

natives! {
    ns_eme_request_access => ffi::nsr_eme_request_access,
    ns_media_set_media_keys => ffi::nsr_media_set_media_keys,
    ns_media_canPlayType => ffi::nsr_media_canPlayType,
    ns_media_capabilities_info => ffi::nsr_media_capabilities_info,
    ns_media_play => ffi::nsr_media_play,
    ns_media_pause => ffi::nsr_media_pause,
    ns_media_load => ffi::nsr_media_load,
    ns_media_fast_seek => ffi::nsr_media_fast_seek,
    ns_media_get_video_playback_quality => ffi::nsr_media_get_video_playback_quality,
    ns_media_addTextTrack => ffi::nsr_media_addTextTrack,
}

getters! {
    ns_media_get_paused => ffi::nsr_media_get_paused,
    ns_media_get_ended => ffi::nsr_media_get_ended,
    ns_media_get_seeking => ffi::nsr_media_get_seeking,
    ns_media_get_readyState => ffi::nsr_media_get_readyState,
    ns_media_get_networkState => ffi::nsr_media_get_networkState,
    ns_media_get_current_time => ffi::nsr_media_get_current_time,
    ns_media_get_duration => ffi::nsr_media_get_duration,
    ns_media_get_error => ffi::nsr_media_get_error,
    ns_media_get_seekable_ranges => ffi::nsr_media_get_seekable_ranges,
    ns_media_get_buffered_ranges => ffi::nsr_media_get_buffered_ranges,
    ns_media_get_played_ranges => ffi::nsr_media_get_played_ranges,
    ns_media_get_playbackRate => ffi::nsr_media_get_playbackRate,
    ns_media_get_defaultPlaybackRate => ffi::nsr_media_get_defaultPlaybackRate,
    ns_media_get_volume => ffi::nsr_media_get_volume,
    ns_media_get_muted => ffi::nsr_media_get_muted,
    ns_media_get_srcObject => ffi::nsr_media_get_srcObject,
    ns_media_get_textTracks => ffi::nsr_media_get_textTracks,
}

setters! {
    ns_media_set_current_time => ffi::nsr_media_set_current_time,
    ns_media_set_playbackRate => ffi::nsr_media_set_playbackRate,
    ns_media_set_defaultPlaybackRate => ffi::nsr_media_set_defaultPlaybackRate,
    ns_media_set_volume => ffi::nsr_media_set_volume,
    ns_media_set_muted => ffi::nsr_media_set_muted,
    ns_media_set_srcObject => ffi::nsr_media_set_srcObject,
}
