//! Southstar — the C ABI of the user's colour-scheme and reduced-motion preferences, which drop the cached style sheets when they change.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::sync::atomic::{AtomicI32, Ordering};

use super::sheet_cache::ns_css_stylesheet_cache_drop;

pub(super) const COLOR_SCHEME_LIGHT: c_int = 0;
pub(super) const COLOR_SCHEME_DARK: c_int = 1;
const REDUCED_MOTION_NO_PREFERENCE: c_int = 0;
const REDUCED_MOTION_REDUCE: c_int = 1;

static COLOR_SCHEME: AtomicI32 = AtomicI32::new(COLOR_SCHEME_LIGHT);
static REDUCED_MOTION: AtomicI32 = AtomicI32::new(REDUCED_MOTION_NO_PREFERENCE);

fn set(preference: &AtomicI32, value: c_int) {
    if preference.swap(value, Ordering::Relaxed) != value {
        ns_css_stylesheet_cache_drop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_color_scheme(scheme: c_int) {
    let scheme = if scheme == COLOR_SCHEME_DARK {
        COLOR_SCHEME_DARK
    } else {
        COLOR_SCHEME_LIGHT
    };
    set(&COLOR_SCHEME, scheme);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_get_color_scheme() -> c_int {
    COLOR_SCHEME.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_reduced_motion(motion: c_int) {
    let motion = if motion == REDUCED_MOTION_REDUCE {
        REDUCED_MOTION_REDUCE
    } else {
        REDUCED_MOTION_NO_PREFERENCE
    };
    set(&REDUCED_MOTION, motion);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_get_reduced_motion() -> c_int {
    REDUCED_MOTION.load(Ordering::Relaxed)
}
