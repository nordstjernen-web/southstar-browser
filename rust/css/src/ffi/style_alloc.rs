//! Southstar — a computed style's lifetime: the pool styles come from, the counted reference the style table and other crates hold on one, and the clone style sharing hands an element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr::NonNull;
use std::sync::{Mutex, PoisonError};

use super::computed_units::RawStyle;

const POOL_MAX: usize = 16384;

#[allow(clippy::vec_box)]
struct Pool(Vec<Box<RawStyle>>);

// SAFETY: a pooled style is empty: it holds no value, variable map or style another thread could reach.
unsafe impl Send for Pool {}

static POOL: Mutex<Pool> = Mutex::new(Pool(Vec::new()));

fn release(mut style: Box<RawStyle>) {
    *style = RawStyle::new();
    let mut pool = POOL.lock().unwrap_or_else(PoisonError::into_inner);
    if pool.0.len() < POOL_MAX {
        pool.0.push(style);
    }
}

pub(super) fn style_alloc() -> Box<RawStyle> {
    let pooled = POOL.lock().unwrap_or_else(PoisonError::into_inner).0.pop();
    pooled.unwrap_or_else(|| Box::new(RawStyle::new()))
}

pub struct RetainedStyle(NonNull<RawStyle>);

impl RetainedStyle {
    pub(super) fn new(style: Box<RawStyle>) -> RetainedStyle {
        RetainedStyle(NonNull::from(Box::leak(style)))
    }

    /// # Safety
    ///
    /// `style` is null or a live computed style made by this crate.
    pub unsafe fn retain<T>(style: *const T) -> Option<RetainedStyle> {
        let style = NonNull::new(style.cast::<RawStyle>().cast_mut())?;
        // SAFETY: the caller passes a live computed style.
        unsafe { style.as_ref() }.retain();
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

impl Clone for RetainedStyle {
    fn clone(&self) -> RetainedStyle {
        self.get().retain();
        RetainedStyle(self.0)
    }
}

impl Drop for RetainedStyle {
    fn drop(&mut self) {
        if !self.get().release() {
            return;
        }
        // SAFETY: the style came from Box::leak in RetainedStyle::new, and this was its last reference.
        release(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

pub(super) fn style_clone_shared(source: &RawStyle) -> Box<RawStyle> {
    let mut clone = style_alloc();
    clone.share_id = source.share_id;
    clone.display = source.display;
    for (slot, value) in clone.values.iter_mut().zip(&source.values) {
        slot.clone_from(value);
    }
    for (slot, pseudo) in clone.pseudo_styles.iter_mut().zip(&source.pseudo_styles) {
        *slot = pseudo
            .as_ref()
            .map(|pseudo| RetainedStyle::new(style_clone_shared(pseudo.get())));
    }
    clone.vars.clone_from(&source.vars);
    clone
}
