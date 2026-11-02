//! Southstar — the element-internals state of custom elements: form association, submission entries, validity, the last form owner and disabled state, and the custom states :state() matches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;

use crate::{Node, WeakNode, ancestors, controls};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: Option<Vec<u8>>,
    pub value: Vec<u8>,
}

#[derive(Default)]
struct Record {
    form_associated: bool,
    submission: Option<Vec<Entry>>,
    invalid: bool,
    owner: Option<WeakNode>,
    disabled: bool,
    states: Vec<String>,
}

#[derive(Default)]
struct Registry {
    records: HashMap<WeakNode, Record>,
    prune_at: usize,
}

const FIRST_PRUNE: usize = 64;

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
}

impl Registry {
    fn entry(&mut self, node: Node<'_>) -> &mut Record {
        if self.records.len() >= self.prune_at.max(FIRST_PRUNE) {
            self.records.retain(|weak, _| weak.get().is_some());
            self.prune_at = self.records.len() * 2;
        }
        self.records.entry(node.downgrade()).or_default()
    }
}

fn read<R>(node: Node<'_>, f: impl FnOnce(&Record) -> R) -> Option<R> {
    REGISTRY
        .try_with(|registry| registry.borrow().records.get(&node.downgrade()).map(f))
        .ok()
        .flatten()
}

fn write<R>(node: Node<'_>, f: impl FnOnce(&mut Record) -> R) -> Option<R> {
    REGISTRY
        .try_with(|registry| f(registry.borrow_mut().entry(node)))
        .ok()
}

pub fn mark_form_associated(node: Node<'_>) {
    write(node, |record| record.form_associated = true);
}

pub fn is_form_associated(node: Node<'_>) -> bool {
    node.is_element() && read(node, |record| record.form_associated).unwrap_or(false)
}

pub fn set_submission(node: Node<'_>, entries: Option<Vec<Entry>>) {
    write(node, |record| record.submission = entries);
}

pub fn submission(node: Node<'_>) -> Option<Vec<Entry>> {
    read(node, |record| record.submission.clone()).flatten()
}

pub fn set_invalid(node: Node<'_>, invalid: bool) {
    write(node, |record| record.invalid = invalid);
}

pub fn is_invalid(node: Node<'_>) -> bool {
    read(node, |record| record.invalid).unwrap_or(false)
}

pub fn will_validate(node: Node<'_>) -> bool {
    is_form_associated(node)
        && !controls::effectively_disabled(node)
        && node.attr(c"readonly").is_none()
        && !ancestors(node).any(|p| p.element_name() == Some(b"datalist".as_slice()))
}

pub fn fails_validation(node: Node<'_>) -> bool {
    is_invalid(node) && will_validate(node)
}

pub fn swap_owner(node: Node<'_>, owner: Option<Node<'_>>) -> Option<WeakNode> {
    write(node, |record| {
        core::mem::replace(&mut record.owner, owner.map(Node::downgrade))
    })
    .flatten()
}

pub fn swap_disabled(node: Node<'_>, disabled: bool) -> bool {
    write(node, |record| {
        core::mem::replace(&mut record.disabled, disabled)
    })
    .unwrap_or(false)
}

pub fn has_state(node: Node<'_>, state: &[u8]) -> bool {
    read(node, |record| {
        record.states.iter().any(|s| s.as_bytes() == state)
    })
    .unwrap_or(false)
}

pub fn states(node: Node<'_>) -> Vec<String> {
    read(node, |record| record.states.clone()).unwrap_or_default()
}

pub fn add_state(node: Node<'_>, state: &str) -> bool {
    write(node, |record| {
        let added = !record.states.iter().any(|s| s == state);
        if added {
            record.states.push(state.to_owned());
        }
        added
    })
    .unwrap_or(false)
}

pub fn delete_state(node: Node<'_>, state: &str) -> bool {
    write(node, |record| {
        let before = record.states.len();
        record.states.retain(|s| s != state);
        record.states.len() != before
    })
    .unwrap_or(false)
}

pub fn clear_states(node: Node<'_>) -> bool {
    write(node, |record| {
        let had = !record.states.is_empty();
        record.states.clear();
        had
    })
    .unwrap_or(false)
}
