//! Southstar — the runtime configuration struct of src/config.h, read by ported modules while config.c is still C.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{c_char, c_int};
use southstar_glib::{FALSE, GBoolean};

#[repr(C)]
pub struct NsConfig {
    pub home_url: *mut c_char,
    pub user_agent: *mut c_char,
    pub compat_mode: *mut c_char,
    pub accept_language: *mut c_char,
    pub search_engine: *mut c_char,
    pub ai_model_mirror: *mut c_char,
    pub http_proxy: *mut c_char,
    pub https_proxy: *mut c_char,
    pub no_proxy: *mut c_char,
    pub doh_url: *mut c_char,
    pub gsk_renderer: *mut c_char,
    pub referer_policy: c_int,
    pub cookie_policy: c_int,
    pub color_scheme: c_int,
    pub reduced_motion: c_int,
    pub do_not_track: GBoolean,
    pub global_privacy_control: GBoolean,
    pub strip_tracking_params: GBoolean,
    pub https_first: GBoolean,
    pub harden_allocator: GBoolean,
    pub speculative_preload: GBoolean,
    pub async_image_decode: GBoolean,
    pub images_enabled: GBoolean,
    pub javascript_enabled: GBoolean,
    pub webgl_enabled: GBoolean,
    pub camera_enabled: GBoolean,
    pub microphone_enabled: GBoolean,
    pub local_storage_enabled: GBoolean,
    pub cache_enabled: GBoolean,
    pub tls_allow_insecure_override: GBoolean,
    pub watchdog_enabled: GBoolean,
    pub private_mode: GBoolean,
    pub cache_cap_mb: c_int,
    pub js_eval_budget_ms: c_int,
    pub js_memory_cap_mb: c_int,
    pub max_redirects: c_int,
    pub window_width_px: c_int,
    pub window_height_px: c_int,
    pub layout_viewport_px: c_int,
}

pub fn get() -> Option<&'static NsConfig> {
    ffi::config()
}

pub fn cache_enabled() -> bool {
    get().is_none_or(|config| config.cache_enabled != FALSE)
}

pub fn private_mode() -> bool {
    get().is_some_and(|config| config.private_mode != FALSE)
}
