//! Southstar — the custom-property cascade: an element's matched custom-property declarations put in cascade order, revert and the other CSS-wide keywords rolled back, var() references expanded, and registered @property values checked against their syntax, into the variables its style carries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::vars::{self, Lookup, Wide};

const ORIGIN_PRESENTATIONAL: i32 = 1;
const ORIGIN_AUTHOR: i32 = 2;
const LAYER_NONE: i32 = i32::MAX;

pub(crate) type Own = HashMap<Vec<u8>, Vec<u8>>;

pub(crate) struct VarMatch<'a> {
    pub(crate) origin: i32,
    pub(crate) specificity: (i32, i32, i32),
    pub(crate) sheet_index: i32,
    pub(crate) layer_order: i32,
    pub(crate) scope_order: i32,
    pub(crate) source_order: i32,
    pub(crate) decl_order: i32,
    pub(crate) important: bool,
    pub(crate) inline_style: bool,
    pub(crate) rule: usize,
    pub(crate) name: &'a [u8],
    pub(crate) text: &'a [u8],
}

pub(crate) struct Registered<'a> {
    pub(crate) inherits: bool,
    pub(crate) initial: Option<&'a [u8]>,
}

pub(crate) trait Registry {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>>;
    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>));
    fn rejects(&self, name: &[u8], value: &[u8]) -> bool;
}

pub(crate) trait Inherited {
    fn lookup(&self, name: &[u8]) -> Option<&[u8]>;
}

struct Scope<'a> {
    own: &'a Own,
    parent: Option<&'a dyn Inherited>,
    registry: Option<&'a dyn Registry>,
}

impl Lookup for Scope<'_> {
    fn value(&self, name: &[u8]) -> Option<&[u8]> {
        self.own
            .get(name)
            .map(Vec::as_slice)
            .or_else(|| self.parent.and_then(|p| p.lookup(name)))
    }

    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>> {
        self.registry?.get(name)?.initial.map(Some)
    }
}

fn is_revert(kind: Wide) -> bool {
    matches!(kind, Wide::Revert | Wide::RevertLayer | Wide::RevertRule)
}

fn has_var(text: &[u8]) -> bool {
    text.windows(4).any(|w| w == b"var(")
}

fn ordered(a: i32, b: i32, reversed: bool) -> Ordering {
    if reversed { b.cmp(&a) } else { a.cmp(&b) }
}

fn cascade_order(a: &VarMatch<'_>, b: &VarMatch<'_>) -> Ordering {
    a.important
        .cmp(&b.important)
        .then_with(|| ordered(a.origin, b.origin, a.important))
        .then(a.inline_style.cmp(&b.inline_style))
        .then_with(|| ordered(a.layer_order, b.layer_order, a.important))
        .then(a.specificity.cmp(&b.specificity))
        .then(a.scope_order.cmp(&b.scope_order))
        .then(a.sheet_index.cmp(&b.sheet_index))
        .then(a.source_order.cmp(&b.source_order))
        .then(a.decl_order.cmp(&b.decl_order))
}

fn same_revert_origin(rollback: i32, candidate: i32) -> bool {
    if rollback == ORIGIN_AUTHOR {
        candidate == ORIGIN_AUTHOR || candidate == ORIGIN_PRESENTATIONAL
    } else {
        rollback == candidate
    }
}

fn rolled_back_past(rollback: &VarMatch<'_>, prev: &VarMatch<'_>, kind: Wide) -> bool {
    match kind {
        Wide::RevertRule => prev.rule == rollback.rule,
        Wide::RevertLayer => {
            prev.origin == rollback.origin
                && if rollback.inline_style {
                    prev.inline_style
                } else if rollback.layer_order == LAYER_NONE {
                    prev.layer_order == LAYER_NONE
                } else {
                    prev.layer_order >= rollback.layer_order
                }
        }
        _ => same_revert_origin(rollback.origin, prev.origin),
    }
}

fn rollback(matches: &[VarMatch<'_>], before: usize, from: usize, kind: Wide) -> Option<usize> {
    let current = &matches[from];
    for j in (0..before).rev() {
        let prev = &matches[j];
        if prev.name != current.name || rolled_back_past(current, prev, kind) {
            continue;
        }
        let prev_kind = vars::wide_kind(prev.text);
        if is_revert(prev_kind) {
            return rollback(matches, j, j, prev_kind);
        }
        return Some(j);
    }
    None
}

fn resolved(matches: &[VarMatch<'_>], index: usize) -> Option<usize> {
    let kind = vars::wide_kind(matches[index].text);
    if is_revert(kind) {
        rollback(matches, index, index, kind)
    } else {
        Some(index)
    }
}

fn prefill_plain_values<'a>(
    own: &mut Own,
    matches: &[VarMatch<'a>],
    registry: Option<&dyn Registry>,
) -> HashSet<&'a [u8]> {
    let mut last: HashMap<&[u8], Option<usize>> = HashMap::new();
    for (i, vm) in matches.iter().enumerate() {
        let plain = !has_var(vm.text)
            && vars::wide_kind(vm.text) == Wide::None
            && registry.is_none_or(|r| r.get(vm.name).is_none());
        let chain_plain = last.get(vm.name).is_none_or(Option::is_some);
        last.insert(vm.name, (plain && chain_plain).then_some(i));
    }
    let mut prefilled = HashSet::new();
    for (name, plain_at) in last {
        if let Some(i) = plain_at {
            own.insert(name.to_vec(), matches[i].text.to_vec());
            prefilled.insert(name);
        }
    }
    prefilled
}

struct Cascade<'a, 'm> {
    matches: &'a [VarMatch<'m>],
    parent: Option<&'a dyn Inherited>,
    registry: Option<&'a dyn Registry>,
}

struct Value<'m> {
    text: &'m [u8],
    expanded: Option<Vec<u8>>,
    kind: Wide,
}

impl<'m> Cascade<'_, 'm> {
    fn inherited(&self, name: &[u8]) -> Option<&[u8]> {
        self.parent.and_then(|p| p.lookup(name))
    }

    fn value(&self, own: &Own, index: usize, resolved_at: usize) -> Value<'m> {
        let text = self.matches[resolved_at].text;
        let mut value = Value {
            text,
            expanded: None,
            kind: vars::wide_kind(text),
        };
        if value.kind == Wide::None && has_var(text) {
            let scope = Scope {
                own,
                parent: self.parent,
                registry: self.registry,
            };
            value.expanded = vars::substitute(text, Some(&scope), 0);
            value.kind = value
                .expanded
                .as_deref()
                .map_or(Wide::None, vars::wide_kind);
        }
        if matches!(value.kind, Wide::Revert | Wide::RevertLayer)
            && let Some(back) = rollback(self.matches, index, index, value.kind)
        {
            value.text = self.matches[back].text;
            value.kind = vars::wide_kind(value.text);
        }
        value
    }

    fn apply_unregistered(&self, own: &mut Own, index: usize) {
        let name = self.matches[index].name;
        let Some(at) = resolved(self.matches, index) else {
            own.remove(name);
            return;
        };
        let value = self.value(own, index, at);
        match value.kind {
            Wide::Inherit | Wide::Unset | Wide::Revert | Wide::RevertLayer => {
                own.remove(name);
            }
            Wide::Initial => {
                own.insert(name.to_vec(), b"initial".to_vec());
            }
            _ => {
                own.insert(name.to_vec(), value.text.to_vec());
            }
        }
    }

    fn restore_default(
        &self,
        own: &mut Own,
        name: &[u8],
        registered: Option<&Registered<'_>>,
        inherit: bool,
    ) {
        let parent_value = if inherit { self.inherited(name) } else { None };
        let fallback = match (parent_value, registered.and_then(|r| r.initial)) {
            (Some(value), _) | (None, Some(value)) => Some(value),
            (None, None) if inherit || self.inherited(name).is_some() => Some(&b"initial"[..]),
            (None, None) => None,
        };
        match fallback {
            Some(value) => {
                own.insert(name.to_vec(), value.to_vec());
            }
            None => {
                own.remove(name);
            }
        }
    }

    fn apply_registered(&self, own: &mut Own, index: usize) {
        let name = self.matches[index].name;
        let registered = self.registry.and_then(|r| r.get(name));
        let inherits = registered.as_ref().is_none_or(|r| r.inherits);
        let Some(at) = resolved(self.matches, index) else {
            self.restore_default(own, name, registered.as_ref(), inherits);
            return;
        };
        let value = self.value(own, index, at);
        match value.kind {
            Wide::Inherit => self.restore_default(own, name, registered.as_ref(), true),
            Wide::Unset | Wide::Revert | Wide::RevertLayer => {
                self.restore_default(own, name, registered.as_ref(), inherits)
            }
            Wide::Initial => {
                self.restore_default(own, name, registered.as_ref(), false);
                if registered.as_ref().is_none_or(|r| r.initial.is_none()) {
                    own.insert(name.to_vec(), b"initial".to_vec());
                }
            }
            _ => {
                let checked = value.expanded.as_deref().unwrap_or(value.text);
                let rejected =
                    registered.is_some() && self.registry.is_some_and(|r| r.rejects(name, checked));
                if rejected {
                    let inherits = registered.as_ref().is_some_and(|r| r.inherits);
                    self.restore_default(own, name, registered.as_ref(), inherits);
                } else {
                    own.insert(name.to_vec(), value.text.to_vec());
                }
            }
        }
    }

    fn apply_all(&self, own: &mut Own, registered: bool) {
        let prefilled = prefill_plain_values(own, self.matches, self.registry);
        for (i, vm) in self.matches.iter().enumerate() {
            if prefilled.contains(vm.name) {
                continue;
            }
            if registered {
                self.apply_registered(own, i);
            } else {
                self.apply_unregistered(own, i);
            }
        }
    }
}

fn reset_registered(own: &mut Own, parent: Option<&dyn Inherited>, registry: &dyn Registry) {
    registry.each(&mut |name, rule| {
        let inherited = parent.and_then(|p| p.lookup(name));
        let start = match inherited {
            Some(value) if rule.inherits => Some(value),
            _ => rule.initial,
        };
        match start {
            Some(start) if inherited != Some(start) => {
                own.insert(name.to_vec(), start.to_vec());
            }
            Some(_) => {}
            None => {
                if inherited.is_some_and(|v| !v.eq_ignore_ascii_case(b"initial")) {
                    own.insert(name.to_vec(), b"initial".to_vec());
                }
            }
        }
    });
}

pub(crate) fn sort(matches: &mut [VarMatch<'_>]) {
    matches.sort_by(cascade_order);
}

pub(crate) fn unregistered(matches: &[VarMatch<'_>], parent: Option<&dyn Inherited>) -> Own {
    let mut own = Own::new();
    Cascade {
        matches,
        parent,
        registry: None,
    }
    .apply_all(&mut own, false);
    own
}

pub(crate) fn registered(
    matches: &[VarMatch<'_>],
    parent: Option<&dyn Inherited>,
    registry: &dyn Registry,
) -> Own {
    let mut own = Own::new();
    reset_registered(&mut own, parent, registry);
    Cascade {
        matches,
        parent,
        registry: Some(registry),
    }
    .apply_all(&mut own, true);
    own
}
