//! Southstar — one style pass's result: every element's computed style keyed by its node, holding a reference on each.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use super::computed_units::RawStyle;
use super::style_alloc::RetainedStyle;

#[derive(Default)]
pub struct StyleMap(HashMap<usize, RetainedStyle>);

impl StyleMap {
    pub(super) fn insert(&mut self, node: usize, style: RetainedStyle) {
        self.0.insert(node, style);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains<T>(&self, node: *const T) -> bool {
        self.0.contains_key(&(node as usize))
    }

    pub fn get<T>(&self, node: *const T) -> Option<&RawStyle> {
        self.0.get(&(node as usize)).map(RetainedStyle::get)
    }

    pub fn retained<T>(&self, node: *const T) -> Option<RetainedStyle> {
        self.0.get(&(node as usize)).cloned()
    }

    pub fn lookup<T>(&self, node: *const T) -> *mut RawStyle {
        self.0
            .get(&(node as usize))
            .map_or(core::ptr::null_mut(), |style| style.as_ptr().cast_mut())
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &RawStyle)> + '_ {
        self.0.iter().map(|(&node, style)| (node, style.get()))
    }

    pub(super) fn styles(&self) -> impl Iterator<Item = (usize, &RetainedStyle)> + '_ {
        self.0.iter().map(|(&node, style)| (node, style))
    }

    pub fn entries(&self) -> impl Iterator<Item = (usize, *mut RawStyle)> + '_ {
        self.0
            .iter()
            .map(|(&node, style)| (node, style.as_ptr().cast_mut()))
    }
}
