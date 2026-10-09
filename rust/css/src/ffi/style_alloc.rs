//! Southstar — the C ABI of a computed style's lifetime: the pool ns_style structs come from, freeing one with its values, pseudo-element styles and variables, and the clone style sharing hands an element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::mem::size_of;
use core::ptr;
use std::sync::{Mutex, PoisonError};

use southstar_glib as glib;

use super::computed_units::RawStyle;
use super::value::ns_css_value_free;
use super::vars::{map_ref, ns_var_map_unref};

const POOL_MAX: usize = 16384;

struct Pool(Vec<*mut RawStyle>);

unsafe impl Send for Pool {}

static POOL: Mutex<Pool> = Mutex::new(Pool(Vec::new()));

fn pooled() -> Option<*mut RawStyle> {
    POOL.lock().unwrap_or_else(PoisonError::into_inner).0.pop()
}

fn release(style: *mut RawStyle) {
    let mut pool = POOL.lock().unwrap_or_else(PoisonError::into_inner);
    if pool.0.len() < POOL_MAX {
        pool.0.push(style);
    } else {
        drop(pool);
        unsafe { glib::g_free(style.cast()) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_style_alloc() -> *mut RawStyle {
    match pooled() {
        Some(style) => {
            unsafe { ptr::write_bytes(style, 0, 1) };
            style
        }
        None => unsafe { glib::g_malloc0(size_of::<RawStyle>()) }.cast(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_free(s: *mut RawStyle) {
    let Some(style) = (unsafe { s.as_mut() }) else {
        return;
    };
    if style.ref_count > 0 {
        style.ref_count -= 1;
        return;
    }
    for &value in &style.values {
        if !value.is_null() {
            unsafe { ns_css_value_free(value) };
        }
    }
    for &pseudo in &style.pseudo_styles {
        unsafe { ns_style_free(pseudo) };
    }
    if !style.vars.is_null() {
        unsafe { ns_var_map_unref(style.vars) };
    }
    release(s);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_clone_shared(s: *const RawStyle) -> *mut RawStyle {
    let Some(source) = (unsafe { s.as_ref() }) else {
        return ptr::null_mut();
    };
    let out = ns_style_alloc();
    let clone = unsafe { &mut *out };
    clone.share_id = source.share_id;
    clone.display = source.display;
    for (slot, &value) in clone.values.iter_mut().zip(&source.values) {
        if let Some(v) = unsafe { value.as_mut() } {
            v.ref_count += 1;
        }
        *slot = value;
    }
    for (slot, &pseudo) in clone.pseudo_styles.iter_mut().zip(&source.pseudo_styles) {
        *slot = unsafe { ns_style_clone_shared(pseudo) };
    }
    clone.vars = unsafe { map_ref(source.vars) };
    out
}
