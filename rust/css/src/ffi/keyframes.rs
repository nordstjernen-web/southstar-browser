//! Southstar — the C ABI of resolving var() in a @keyframes rule's stops against an element's custom properties, and of a style's own var() text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::{size_of, size_of_val};
use core::ptr;

use southstar_glib as glib;

use super::computed_units::RawStyle;
use super::registry::pass_registered;
use super::sheet::{RawKeyframes, RawStop};
use super::vars::ns_css_substitute_vars;
use crate::keyframes;
use crate::transform::Transform;

unsafe fn stops<'a>(data: *mut RawStop, n_stops: c_int) -> &'a mut [RawStop] {
    let n = usize::try_from(n_stops).unwrap_or(0);
    if data.is_null() || n == 0 {
        return &mut [];
    }
    unsafe { core::slice::from_raw_parts_mut(data, n) }
}

fn resolve_stop(stop: &mut RawStop, vars: *const c_void) {
    let raw = core::mem::replace(&mut stop.raw_props, ptr::null_mut());
    if raw.is_null() {
        return;
    }
    let resolved = unsafe { ns_css_substitute_vars(raw, vars, pass_registered(), 0) };
    if resolved.is_null() {
        return;
    }
    let existing = if stop.has_transform != 0 {
        stop.transform
    } else {
        Transform::default()
    };
    let text = unsafe { CStr::from_ptr(resolved) }.to_bytes();
    if let Some(merged) = keyframes::resolved_transform(text, existing) {
        stop.transform = merged;
        stop.has_transform = glib::TRUE;
    }
    stop.raw_props = resolved;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_keyframes_resolve(
    kf: *const RawKeyframes,
    vars: *const c_void,
) -> *mut RawKeyframes {
    let Some(kf) = (unsafe { kf.as_ref() }) else {
        return ptr::null_mut();
    };
    let source = unsafe { stops(kf.stops, kf.n_stops) };
    if source.iter().all(|stop| stop.raw_props.is_null()) {
        return ptr::null_mut();
    }
    unsafe {
        let out = glib::g_malloc0(size_of::<RawKeyframes>()).cast::<RawKeyframes>();
        (*out).n_stops = kf.n_stops;
        (*out).stops = glib::g_malloc(size_of_val(source)).cast::<RawStop>();
        ptr::copy_nonoverlapping(source.as_ptr(), (*out).stops, source.len());
        for stop in stops((*out).stops, (*out).n_stops) {
            resolve_stop(stop, vars);
        }
        out
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_keyframes_resolved_free(kf: *mut RawKeyframes) {
    let Some(kf_ref) = (unsafe { kf.as_ref() }) else {
        return;
    };
    unsafe {
        glib::g_free(kf_ref.name.cast());
        for stop in stops(kf_ref.stops, kf_ref.n_stops) {
            glib::g_free(stop.raw_props.cast());
        }
        glib::g_free(kf_ref.stops.cast());
        glib::g_free(kf.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_resolve_style_vars(
    text: *const c_char,
    style: *const RawStyle,
) -> *mut c_char {
    let vars = unsafe { style.as_ref() }.map_or(ptr::null(), |s| s.vars.cast_const().cast());
    unsafe { ns_css_substitute_vars(text, vars, pass_registered(), 0) }
}
