//! Southstar — a computed style's lifetime: the pool styles come from, the counted reference the style table and other crates hold on one, and the clone style sharing hands an element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ops::{Deref, DerefMut};
use std::rc::Rc;

use super::computed_units::RawStyle;

const POOL_MAX: usize = 16384;

thread_local! {
    static POOL: RefCell<Vec<Rc<RawStyle>>> = const { RefCell::new(Vec::new()) };
}

fn recycle(mut style: Rc<RawStyle>) {
    let Some(raw) = Rc::get_mut(&mut style) else {
        return;
    };
    *raw = RawStyle::new();
    let _ = POOL.try_with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < POOL_MAX {
            pool.push(style);
        }
    });
}

pub(super) struct NewStyle(Rc<RawStyle>);

impl Deref for NewStyle {
    type Target = RawStyle;

    fn deref(&self) -> &RawStyle {
        &self.0
    }
}

impl DerefMut for NewStyle {
    fn deref_mut(&mut self) -> &mut RawStyle {
        Rc::get_mut(&mut self.0).expect("a style being built has no other reference")
    }
}

pub(super) fn style_alloc() -> NewStyle {
    let pooled = POOL.try_with(|pool| pool.borrow_mut().pop()).ok().flatten();
    NewStyle(pooled.unwrap_or_else(|| Rc::new(RawStyle::new())))
}

pub struct RetainedStyle(Option<Rc<RawStyle>>);

impl RetainedStyle {
    pub(super) fn new(style: NewStyle) -> RetainedStyle {
        RetainedStyle(Some(style.0))
    }

    pub fn get(&self) -> &RawStyle {
        self.0
            .as_deref()
            .expect("a retained style is held until it drops")
    }

    pub fn as_ptr(&self) -> *const RawStyle {
        self.0.as_ref().map_or(core::ptr::null(), Rc::as_ptr)
    }
}

impl Clone for RetainedStyle {
    fn clone(&self) -> RetainedStyle {
        RetainedStyle(self.0.clone())
    }
}

impl Drop for RetainedStyle {
    fn drop(&mut self) {
        if let Some(style) = self.0.take() {
            recycle(style);
        }
    }
}

pub(super) fn style_clone_shared(source: &RawStyle) -> NewStyle {
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
