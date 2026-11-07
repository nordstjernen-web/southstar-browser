//! Southstar — the custom-property cascade: an element's matched custom-property declarations put in cascade order, revert and the other CSS-wide keywords rolled back, var() and if() references resolved on demand with dependency cycles made invalid, and registered @property values checked against their syntax, into the variables its style carries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::cmp::Ordering;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use southstar_css_prop_syntax::{self as prop_syntax, SyntaxDef};
use southstar_dom::Node;

use crate::cascade::{ordered, same_revert_origin};
use crate::scan::has_substitution;
use crate::vars::{self, Lookup, Registration, Wide};

const LAYER_NONE: i32 = i32::MAX;
const GUARANTEED_INVALID: &[u8] = b"initial";
const RESOLVE_DEPTH_MAX: usize = 256;

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
    pub(crate) syntax: Option<&'a SyntaxDef>,
}

pub(crate) trait Registry {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>>;
    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>));
    fn rejects(&self, name: &[u8], value: &[u8]) -> bool;

    fn compute(&self, syntax: &SyntaxDef, value: &[u8]) -> Option<Vec<u8>> {
        prop_syntax::compute(syntax, value, None)
    }
}

pub(crate) trait Inherited {
    fn lookup(&self, name: &[u8]) -> Option<&[u8]>;
}

fn is_revert(kind: Wide) -> bool {
    matches!(kind, Wide::Revert | Wide::RevertLayer | Wide::RevertRule)
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
        let plain = !has_substitution(vm.text)
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
    node: Option<Node<'a>>,
    own: RefCell<Own>,
    todo: RefCell<HashMap<&'m [u8], usize>>,
    stack: RefCell<Vec<&'m [u8]>>,
    cyclic: RefCell<HashSet<&'m [u8]>>,
}

struct Value<'m> {
    text: &'m [u8],
    expanded: Option<Vec<u8>>,
    kind: Wide,
    failed: bool,
}

impl Lookup for Cascade<'_, '_> {
    fn value(&self, name: &[u8]) -> Option<Cow<'_, [u8]>> {
        {
            let stack = self.stack.borrow();
            if let Some(at) = stack.iter().position(|n| *n == name) {
                self.cyclic.borrow_mut().extend(&stack[at..]);
                return None;
            }
            if stack.len() >= RESOLVE_DEPTH_MAX && self.todo.borrow().contains_key(name) {
                return None;
            }
        }
        let pending = self.todo.borrow_mut().remove_entry(name);
        if let Some((name, index)) = pending {
            self.resolve(name, index);
        }
        if let Some(value) = self.own.borrow().get(name) {
            return Some(Cow::Owned(value.clone()));
        }
        self.inherited(name)
    }

    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>> {
        self.registry?.get(name)?.initial.map(Some)
    }

    fn inherited(&self, name: &[u8]) -> Option<Cow<'_, [u8]>> {
        self.parent?.lookup(name).map(Cow::Borrowed)
    }

    fn registration(&self, name: &[u8]) -> Option<Registration<'_>> {
        let registered = self.registry?.get(name)?;
        Some(Registration {
            syntax: registered.syntax,
            inherits: registered.inherits,
            initial: registered.initial,
        })
    }

    fn compute(&self, syntax: &SyntaxDef, value: &[u8]) -> Option<Vec<u8>> {
        match self.registry {
            Some(registry) => registry.compute(syntax, value),
            None => prop_syntax::compute(syntax, value, None),
        }
    }

    fn node(&self) -> Option<Node<'_>> {
        self.node
    }

    fn defers_attr(&self) -> bool {
        true
    }
}

impl<'a, 'm> Cascade<'a, 'm> {
    fn new(
        matches: &'a [VarMatch<'m>],
        parent: Option<&'a dyn Inherited>,
        registry: Option<&'a dyn Registry>,
        node: Option<Node<'a>>,
    ) -> Self {
        Cascade {
            matches,
            parent,
            registry,
            node,
            own: RefCell::new(Own::new()),
            todo: RefCell::new(HashMap::new()),
            stack: RefCell::new(Vec::new()),
            cyclic: RefCell::new(HashSet::new()),
        }
    }

    fn parent_value(&self, name: &[u8]) -> Option<&'a [u8]> {
        self.parent.and_then(|p| p.lookup(name))
    }

    fn insert(&self, name: &[u8], value: &[u8]) {
        self.own.borrow_mut().insert(name.to_vec(), value.to_vec());
    }

    fn remove(&self, name: &[u8]) {
        self.own.borrow_mut().remove(name);
    }

    fn value_of(&self, index: usize, resolved_at: usize) -> Value<'m> {
        let text = self.matches[resolved_at].text;
        let mut value = Value {
            text,
            expanded: None,
            kind: vars::wide_kind(text),
            failed: false,
        };
        if value.kind == Wide::None && has_substitution(text) {
            value.expanded = vars::substitute(text, Some(self), 0);
            value.failed = value.expanded.is_none();
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
            value.expanded = None;
            value.failed = false;
        }
        value
    }

    fn apply_unregistered(&self, index: usize) {
        let name = self.matches[index].name;
        let Some(at) = resolved(self.matches, index) else {
            self.remove(name);
            return;
        };
        let value = self.value_of(index, at);
        match value.kind {
            Wide::Inherit | Wide::Unset | Wide::Revert | Wide::RevertLayer => self.remove(name),
            Wide::Initial => self.insert(name, GUARANTEED_INVALID),
            _ if value.failed => self.insert(name, GUARANTEED_INVALID),
            _ => self.insert(name, value.expanded.as_deref().unwrap_or(value.text)),
        }
    }

    fn restore_default(&self, name: &[u8], registered: Option<&Registered<'_>>, inherit: bool) {
        let parent_value = if inherit {
            self.parent_value(name)
        } else {
            None
        };
        let fallback = match (parent_value, registered.and_then(|r| r.initial)) {
            (Some(value), _) | (None, Some(value)) => Some(value),
            (None, None) if inherit || self.parent_value(name).is_some() => {
                Some(GUARANTEED_INVALID)
            }
            (None, None) => None,
        };
        match fallback {
            Some(value) => self.insert(name, value),
            None => self.remove(name),
        }
    }

    fn apply_registered(&self, index: usize) {
        let name = self.matches[index].name;
        let registered = self.registry.and_then(|r| r.get(name));
        let inherits = registered.as_ref().is_none_or(|r| r.inherits);
        let Some(at) = resolved(self.matches, index) else {
            self.restore_default(name, registered.as_ref(), inherits);
            return;
        };
        let value = self.value_of(index, at);
        match value.kind {
            Wide::Inherit => self.restore_default(name, registered.as_ref(), true),
            Wide::Unset | Wide::Revert | Wide::RevertLayer => {
                self.restore_default(name, registered.as_ref(), inherits)
            }
            Wide::Initial => {
                self.restore_default(name, registered.as_ref(), false);
                if registered.as_ref().is_none_or(|r| r.initial.is_none()) {
                    self.insert(name, GUARANTEED_INVALID);
                }
            }
            _ if value.failed && registered.is_none() => self.insert(name, GUARANTEED_INVALID),
            _ if value.failed => self.restore_default(name, registered.as_ref(), inherits),
            _ => {
                let checked = value.expanded.as_deref().unwrap_or(value.text);
                let rejected =
                    registered.is_some() && self.registry.is_some_and(|r| r.rejects(name, checked));
                if rejected {
                    let inherits = registered.as_ref().is_some_and(|r| r.inherits);
                    self.restore_default(name, registered.as_ref(), inherits);
                } else {
                    self.insert(name, checked);
                }
            }
        }
    }

    fn invalidate(&self, name: &[u8]) {
        let registered = self.registry.and_then(|r| r.get(name));
        match registered {
            Some(registered) => {
                let inherits = registered.inherits;
                self.restore_default(name, Some(&registered), inherits);
            }
            None => self.insert(name, GUARANTEED_INVALID),
        }
    }

    fn resolve(&self, name: &'m [u8], index: usize) {
        self.stack.borrow_mut().push(name);
        if self.registry.is_some() {
            self.apply_registered(index);
        } else {
            self.apply_unregistered(index);
        }
        self.stack.borrow_mut().pop();
        if self.cyclic.borrow().contains(name) {
            self.invalidate(name);
        }
    }

    fn apply_all(self) -> Own {
        let prefilled =
            prefill_plain_values(&mut self.own.borrow_mut(), self.matches, self.registry);
        {
            let mut todo = self.todo.borrow_mut();
            for (i, vm) in self.matches.iter().enumerate() {
                if !prefilled.contains(vm.name) {
                    todo.insert(vm.name, i);
                }
            }
        }
        for vm in self.matches {
            let pending = self.todo.borrow_mut().remove_entry(vm.name);
            if let Some((name, index)) = pending {
                self.resolve(name, index);
            }
        }
        self.own.into_inner()
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
                if inherited.is_some_and(|v| !v.eq_ignore_ascii_case(GUARANTEED_INVALID)) {
                    own.insert(name.to_vec(), GUARANTEED_INVALID.to_vec());
                }
            }
        }
    });
}

pub(crate) fn sort(matches: &mut [VarMatch<'_>]) {
    matches.sort_by(cascade_order);
}

pub(crate) fn unregistered(
    matches: &[VarMatch<'_>],
    parent: Option<&dyn Inherited>,
    node: Option<Node<'_>>,
) -> Own {
    Cascade::new(matches, parent, None, node).apply_all()
}

pub(crate) fn registered(
    matches: &[VarMatch<'_>],
    parent: Option<&dyn Inherited>,
    registry: &dyn Registry,
    node: Option<Node<'_>>,
) -> Own {
    let cascade = Cascade::new(matches, parent, Some(registry), node);
    reset_registered(&mut cascade.own.borrow_mut(), parent, registry);
    cascade.apply_all()
}
