//! Southstar — a computed style's lifetime: the pool ns_style structs come from, freeing one with its values, pseudo-element styles and variables, the reference other crates hold on one, and the clone style sharing hands an element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::mem::size_of;
use core::ptr::{self, NonNull};
use std::sync::{Mutex, PoisonError};

use southstar_glib as glib;

use super::computed_units::{RawStyle, style_retain};
use super::value::value_free;
use super::vars::{map_ref, var_map_unref};

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

pub(super) fn style_alloc() -> *mut RawStyle {
    match pooled() {
        Some(style) => {
            unsafe { ptr::write_bytes(style, 0, 1) };
            style
        }
        None => unsafe { glib::g_malloc0(size_of::<RawStyle>()) }.cast(),
    }
}

/// # Safety
///
/// `s` is null or a computed style made by this crate, and the caller gives up the reference it holds on it.
pub(super) unsafe fn style_free(s: *mut RawStyle) {
    let Some(style) = (unsafe { s.as_mut() }) else {
        return;
    };
    if !style.release() {
        return;
    }
    for &value in &style.values {
        if !value.is_null() {
            unsafe { value_free(value) };
        }
    }
    for &pseudo in &style.pseudo_styles {
        unsafe { style_free(pseudo) };
    }
    if !style.vars.is_null() {
        unsafe { var_map_unref(style.vars) };
    }
    release(s);
}

pub struct RetainedStyle(NonNull<RawStyle>);

impl RetainedStyle {
    /// # Safety
    ///
    /// `style` is null or a live computed style made by this crate.
    pub unsafe fn retain<T>(style: *const T) -> Option<RetainedStyle> {
        let style = NonNull::new(style.cast::<RawStyle>().cast_mut())?;
        // SAFETY: the caller passes a live computed style.
        unsafe { style_retain(style.as_ptr()) };
        Some(RetainedStyle(style))
    }

    pub fn get(&self) -> &RawStyle {
        // SAFETY: a RetainedStyle holds a reference on a live style, which keeps it allocated.
        unsafe { self.0.as_ref() }
    }

    pub fn as_ptr(&self) -> *const RawStyle {
        self.0.as_ptr()
    }
}

impl Drop for RetainedStyle {
    fn drop(&mut self) {
        // SAFETY: a RetainedStyle gives up the one reference it took on the style.
        unsafe { style_free(self.0.as_ptr()) };
    }
}

pub(super) fn style_clone_shared(source: &RawStyle) -> *mut RawStyle {
    let out = style_alloc();
    let clone = unsafe { &mut *out };
    clone.share_id = source.share_id;
    clone.display = source.display;
    for (slot, &value) in clone.values.iter_mut().zip(&source.values) {
        if let Some(v) = unsafe { value.as_ref() } {
            v.add_ref();
        }
        *slot = value;
    }
    for (slot, &pseudo) in clone.pseudo_styles.iter_mut().zip(&source.pseudo_styles) {
        *slot = unsafe { pseudo.as_ref() }.map_or(ptr::null_mut(), style_clone_shared);
    }
    clone.vars = unsafe { map_ref(source.vars) };
    out
}
