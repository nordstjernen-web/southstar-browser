//! Southstar — one style pass's result: every element's computed style keyed by its node, holding a reference on each.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use southstar_dom::{Node, WeakNode};

use super::computed_units::RawStyle;
use super::style_alloc::RetainedStyle;

#[derive(Clone, Default)]
pub struct StyleMap(HashMap<usize, (WeakNode, RetainedStyle)>);

impl StyleMap {
    pub(super) fn insert(&mut self, node: Node<'_>, style: RetainedStyle) {
        self.0
            .insert(node.as_ptr() as usize, (node.downgrade(), style));
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
        self.0.get(&(node as usize)).map(|(_, style)| style.get())
    }

    pub fn retained<T>(&self, node: *const T) -> Option<RetainedStyle> {
        self.0.get(&(node as usize)).map(|(_, style)| style.clone())
    }

    pub fn modify<T, R>(
        &mut self,
        node: *const T,
        f: impl FnOnce(&mut RawStyle) -> R,
    ) -> Option<R> {
        let key = node as usize;
        let (_, style) = self.0.get_mut(&key)?;
        Some(super::compute::with_previous_released(
            key,
            style,
            |style| f(style.make_mut()),
        ))
    }

    pub fn modify_all(&mut self, mut f: impl FnMut(&mut RawStyle)) {
        for (_, style) in self.0.values_mut() {
            f(style.make_mut());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &RawStyle)> + '_ {
        self.0.iter().map(|(&node, (_, style))| (node, style.get()))
    }

    pub fn nodes(&self) -> impl Iterator<Item = (Node<'static>, &RawStyle)> + '_ {
        self.0
            .values()
            .filter_map(|(node, style)| Some((node.get()?, style.get())))
    }

    pub(super) fn styles(&self) -> impl Iterator<Item = (usize, &RetainedStyle)> + '_ {
        self.0.iter().map(|(&node, (_, style))| (node, style))
    }
}
