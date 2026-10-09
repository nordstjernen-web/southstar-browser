//! Southstar — which elements a DOM change can restyle: the ids, classes, tags and attributes the style sheets' structural, sibling and :has() selectors depend on, and the elements marked dirty when one of them changes, so the next style pass recomputes only those.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::{Mutex, MutexGuard};

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Node, attrs};

use crate::ffi::{AttrRef, CompoundRef, GroupRef, SelectorRef, SheetRef};
use crate::hints::is_presentational_attr;
use crate::matcher::{AddrSet, attr_value_matches};

const DEPTH_MAX: i32 = 6;
const SIBLING_SCAN_MAX: u32 = 256;
const FLAG_XML_DOC: u32 = 1 << 14;

const COMB_ADJACENT: u32 = 3;
const COMB_SIBLING: u32 = 4;

const ATTR_PRESENT: u32 = 0;
const ATTR_HYPHEN: u32 = 6;

const PC_EMPTY: u32 = 6;
const PC_ROOT: u32 = 7;
const PC_CHECKED: u32 = 8;
const PC_DISABLED: u32 = 9;
const PC_ENABLED: u32 = 10;
const PC_REQUIRED: u32 = 11;
const PC_OPTIONAL: u32 = 12;
const PC_NTH_CHILD: u32 = 19;
const PC_NTH_LAST_OF_TYPE: u32 = 22;
const PC_TARGET: u32 = 31;
const PC_TARGET_WITHIN: u32 = 32;
const PC_SCOPE: u32 = 34;
const PC_READ_ONLY: u32 = 36;
const PC_READ_WRITE: u32 = 37;
const PC_LANG: u32 = 39;
const PC_DIR: u32 = 40;
const PC_OPEN: u32 = 41;
const PC_POPOVER_OPEN: u32 = 42;
const PC_MODAL: u32 = 43;

const INTRINSIC_ATTRS: [&[u8]; 44] = [
    b"class",
    b"id",
    b"style",
    b"hidden",
    b"lang",
    b"xml:lang",
    b"dir",
    b"width",
    b"height",
    b"src",
    b"srcset",
    b"sizes",
    b"href",
    b"type",
    b"value",
    b"checked",
    b"selected",
    b"open",
    b"disabled",
    b"readonly",
    b"required",
    b"placeholder",
    b"multiple",
    b"size",
    b"rows",
    b"cols",
    b"rowspan",
    b"colspan",
    b"span",
    b"start",
    b"reversed",
    b"wrap",
    b"contenteditable",
    b"inert",
    b"popover",
    b"popovertarget",
    b"slot",
    b"name",
    b"form",
    b"list",
    b"min",
    b"max",
    b"step",
    b"media",
];

struct AttrDep {
    name: CString,
    op: u32,
    value: Option<Vec<u8>>,
    case_insensitive: bool,
    case_sensitive: bool,
    html_ci: bool,
}

#[derive(Default)]
struct Names {
    ids: HashSet<Vec<u8>>,
    classes: HashSet<Vec<u8>>,
    tags: HashSet<Vec<u8>>,
    attrs: Vec<AttrDep>,
}

struct Anchor {
    tag: Option<Vec<u8>>,
    id: Option<Vec<u8>>,
    classes: Vec<Vec<u8>>,
    attrs: Vec<AttrDep>,
}

#[derive(Default)]
struct Keys {
    sig: u64,
    structural: Names,
    structural_ancestors: Names,
    sibling: Names,
    sibling_attrs: HashSet<Vec<u8>>,
    sibling_value_attrs: HashSet<Vec<u8>>,
    attrs: HashSet<Vec<u8>>,
    class_names: HashSet<Vec<u8>>,
    id_names: HashSet<Vec<u8>>,
    class_names_loose: bool,
    id_names_loose: bool,
    structural_loose: bool,
    sibling_loose: bool,
}

struct HasDeps {
    sig: u64,
    anchors: Vec<Anchor>,
    loose: bool,
}

#[derive(Default)]
struct State {
    keys: Option<Keys>,
    has: Option<HasDeps>,
    dirty: AddrSet<usize>,
}

struct HasCtx<'a> {
    sel: SelectorRef<'a>,
    idx: usize,
    outer: Option<&'a HasCtx<'a>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn state() -> MutexGuard<'static, Option<State>> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(state().get_or_insert_with(State::default))
}

fn addr(node: Node<'_>) -> usize {
    node.as_ptr() as usize
}

fn attr<'a>(el: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    attrs::get(el, name).map(CStr::to_bytes)
}

fn lower(text: &[u8]) -> Vec<u8> {
    text.to_ascii_lowercase()
}

fn until_nul(text: &[u8]) -> &[u8] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

fn tokens(value: &[u8]) -> impl Iterator<Item = &[u8]> {
    value
        .split(u8::is_ascii_whitespace)
        .filter(|t| !t.is_empty())
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

fn self_and_ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(Some(node), |n| n.parent())
}

fn compounds(sel: SelectorRef<'_>) -> impl Iterator<Item = CompoundRef<'_>> {
    (0..sel.len()).filter_map(move |i| sel.compound(i))
}

fn subject(sel: SelectorRef<'_>) -> Option<CompoundRef<'_>> {
    sel.len().checked_sub(1).and_then(|i| sel.compound(i))
}

fn logical_groups(c: CompoundRef<'_>) -> impl Iterator<Item = GroupRef<'_>> {
    c.matches_any().chain(c.matches_none())
}

fn nested_groups(c: CompoundRef<'_>) -> impl Iterator<Item = GroupRef<'_>> {
    logical_groups(c).chain(c.has_groups())
}

fn compound_type(c: CompoundRef<'_>) -> Option<&[u8]> {
    c.type_name().filter(|t| !t.is_empty() && *t != b"*")
}

fn compound_id(c: CompoundRef<'_>) -> Option<&[u8]> {
    c.id().filter(|id| !id.is_empty())
}

fn compound_classes(c: CompoundRef<'_>) -> impl Iterator<Item = &[u8]> {
    c.classes().map(until_nul).filter(|cls| !cls.is_empty())
}

fn attr_name(a: &AttrRef) -> Option<&[u8]> {
    a.name().map(CStr::to_bytes).filter(|n| !n.is_empty())
}

fn attr_dep(a: &AttrRef) -> Option<AttrDep> {
    Some(AttrDep {
        name: a.name()?.to_owned(),
        op: a.op(),
        value: a.value().map(<[u8]>::to_vec),
        case_insensitive: a.case_insensitive(),
        case_sensitive: a.case_sensitive(),
        html_ci: a.html_ci(),
    })
}

fn pc_is_structural(kind: u32) -> bool {
    kind <= PC_EMPTY || (PC_NTH_CHILD..=PC_NTH_LAST_OF_TYPE).contains(&kind)
}

fn state_pseudo_attr(kind: u32) -> Option<&'static [u8]> {
    match kind {
        PC_DISABLED | PC_ENABLED => Some(b"disabled"),
        PC_CHECKED => Some(b"data-nd-checked"),
        PC_REQUIRED | PC_OPTIONAL => Some(b"required"),
        PC_READ_ONLY | PC_READ_WRITE => Some(b"readonly"),
        0..=PC_ROOT | PC_NTH_CHILD..=PC_SCOPE => Some(b""),
        _ => None,
    }
}

fn pseudo_attr_extras(kind: u32) -> &'static [&'static [u8]] {
    match kind {
        PC_LANG => &[b"lang", b"xml:lang"],
        PC_DIR => &[b"dir"],
        PC_OPEN => &[b"open"],
        PC_POPOVER_OPEN => &[b"data-nd-popover-open"],
        PC_MODAL => &[b"data-nd-modal"],
        _ => &[],
    }
}

fn simple_has_structural(c: CompoundRef<'_>, depth: i32) -> bool {
    if depth > DEPTH_MAX {
        return true;
    }
    c.pseudos().iter().any(|p| {
        pc_is_structural(p.kind())
            || p.of_group()
                .is_some_and(|g| g.selectors().any(|s| selector_has_structural(s, depth + 1)))
    }) || nested_groups(c).any(|g| g.selectors().any(|s| selector_has_structural(s, depth + 1)))
}

fn selector_has_structural(sel: Option<SelectorRef<'_>>, depth: i32) -> bool {
    let Some(sel) = sel else {
        return false;
    };
    depth > DEPTH_MAX || compounds(sel).any(|c| simple_has_structural(c, depth))
}

fn simple_uses_has(c: CompoundRef<'_>, depth: i32) -> bool {
    if depth > DEPTH_MAX || c.has_group_count() > 0 {
        return true;
    }
    c.pseudos()
        .iter()
        .filter_map(|p| p.of_group())
        .chain(logical_groups(c))
        .any(|g| g.selectors().any(|s| selector_uses_has(s, depth + 1)))
}

fn selector_uses_has(sel: Option<SelectorRef<'_>>, depth: i32) -> bool {
    sel.is_some_and(|sel| compounds(sel).any(|c| simple_uses_has(c, depth)))
}

impl Names {
    fn add_compound(&mut self, c: CompoundRef<'_>) {
        if let Some(id) = compound_id(c) {
            self.ids.insert(id.to_vec());
        }
        for cls in compound_classes(c) {
            self.classes.insert(cls.to_vec());
        }
        if let Some(tag) = compound_type(c) {
            self.tags.insert(lower(tag));
        }
    }

    fn add_positive(&mut self, c: CompoundRef<'_>, depth: i32) -> bool {
        if depth > DEPTH_MAX {
            return false;
        }
        if let Some(id) = compound_id(c) {
            self.ids.insert(id.to_vec());
            return true;
        }
        if let Some(first) = c.classes().next().map(until_nul) {
            if !first.is_empty() {
                self.classes.insert(first.to_vec());
                return true;
            }
        }
        if let Some(first) = c.attrs().first() {
            self.attrs.extend(attr_dep(first));
            return true;
        }
        if let Some(tag) = compound_type(c) {
            self.tags.insert(lower(tag));
            return true;
        }
        for group in c.matches_any() {
            if group.selectors().next().is_none() {
                continue;
            }
            let mut alternatives = Names::default();
            let complete = group.selectors().all(|sel| {
                sel.and_then(subject)
                    .is_some_and(|s| alternatives.add_positive(s, depth + 1))
            });
            if complete {
                self.ids.extend(alternatives.ids);
                self.classes.extend(alternatives.classes);
                self.tags.extend(alternatives.tags);
                self.attrs.extend(alternatives.attrs);
                return true;
            }
        }
        false
    }

    fn matches_name(&self, n: Node<'_>) -> bool {
        if !n.is_element() {
            return false;
        }
        if !self.tags.is_empty()
            && n.name()
                .is_some_and(|name| self.tags.contains(&lower(name.to_bytes())))
        {
            return true;
        }
        if !self.ids.is_empty()
            && attr(n, c"id").is_some_and(|id| !id.is_empty() && self.ids.contains(id))
        {
            return true;
        }
        !self.classes.is_empty()
            && attr(n, c"class").is_some_and(|cls| tokens(cls).any(|t| self.classes.contains(t)))
    }

    fn matches(&self, n: Node<'_>) -> bool {
        self.matches_name(n) || (n.is_element() && self.attrs.iter().any(|dep| dep.matches(n)))
    }
}

impl AttrDep {
    fn matches(&self, n: Node<'_>) -> bool {
        let value = attr(n, &self.name);
        if self.op == ATTR_PRESENT {
            return value.is_some();
        }
        let (Some(value), Some(want)) = (value, self.value.as_deref()) else {
            return false;
        };
        let html_doc =
            n.flags() & FLAG_XML_DOC == 0 && n.flags() & (FLAG_FOREIGN_NS | FLAG_SVG_NS) == 0;
        let ci = self.case_insensitive || (!self.case_sensitive && html_doc && self.html_ci);
        self.op <= ATTR_HYPHEN && attr_value_matches(self.op, value, want, ci)
    }
}

impl Anchor {
    fn from_compound(c: CompoundRef<'_>) -> Option<Anchor> {
        let anchor = Anchor {
            tag: compound_type(c).map(lower),
            id: compound_id(c).map(<[u8]>::to_vec),
            classes: compound_classes(c).map(<[u8]>::to_vec).collect(),
            attrs: c.attrs().iter().filter_map(attr_dep).collect(),
        };
        let keyed = anchor.tag.is_some()
            || anchor.id.is_some()
            || !anchor.classes.is_empty()
            || !anchor.attrs.is_empty();
        keyed.then_some(anchor)
    }

    fn matches(&self, n: Node<'_>) -> bool {
        if let Some(tag) = &self.tag {
            if !n
                .name()
                .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag))
            {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if attr(n, c"id") != Some(id.as_slice()) {
                return false;
            }
        }
        self.classes.iter().all(|cls| attrs::has_class(n, cls))
            && self.attrs.iter().all(|dep| dep.matches(n))
    }
}

impl Keys {
    fn collect(sheets: &[SheetRef<'_>], sig: u64) -> Keys {
        let mut keys = Keys {
            sig,
            ..Keys::default()
        };
        for sheet in sheets {
            keys.collect_structural(*sheet);
            keys.collect_names(*sheet);
        }
        keys
    }

    fn collect_sibling_left(&mut self, c: CompoundRef<'_>) {
        self.sibling.add_compound(c);
        for name in c.attrs().iter().filter_map(attr_name) {
            self.sibling_attrs.insert(lower(name));
        }
        for a in c.attrs() {
            if let Some(name) = attr_name(a).filter(|_| a.op() != ATTR_PRESENT) {
                self.sibling_value_attrs.insert(lower(name));
            }
        }
        for p in c.pseudos() {
            match state_pseudo_attr(p.kind()) {
                None => self.sibling_loose = true,
                Some(name) if !name.is_empty() => {
                    self.sibling_attrs.insert(name.to_vec());
                    self.sibling_value_attrs.insert(name.to_vec());
                }
                Some(_) => {}
            }
        }
        if c.has_group_arrays() {
            self.sibling_loose = true;
        }
    }

    fn collect_attrs_simple(&mut self, c: CompoundRef<'_>, depth: i32) {
        if depth > DEPTH_MAX {
            return;
        }
        for name in c.attrs().iter().filter_map(attr_name) {
            self.attrs.insert(lower(name));
        }
        for p in c.pseudos() {
            if let Some(name) = state_pseudo_attr(p.kind()).filter(|a| !a.is_empty()) {
                self.attrs.insert(name.to_vec());
            }
            for name in pseudo_attr_extras(p.kind()) {
                self.attrs.insert(name.to_vec());
            }
            if let Some(group) = p.of_group() {
                for sel in group.selectors() {
                    self.collect_attrs_selector(sel, depth + 1);
                }
            }
        }
        for group in nested_groups(c) {
            for sel in group.selectors() {
                self.collect_attrs_selector(sel, depth + 1);
            }
        }
    }

    fn collect_attrs_selector(&mut self, sel: Option<SelectorRef<'_>>, depth: i32) {
        if let Some(sel) = sel.filter(|_| depth <= DEPTH_MAX) {
            for c in compounds(sel) {
                self.collect_attrs_simple(c, depth);
            }
        }
    }

    fn collect_structural(&mut self, sheet: SheetRef<'_>) {
        for sel in sheet.rules().flat_map(|r| r.selectors()).flatten() {
            self.collect_attrs_selector(Some(sel), 0);
            for ci in 0..sel.len() {
                let Some(c) = sel.compound(ci) else {
                    continue;
                };
                let sibling_comb =
                    |i: usize| matches!(sel.combinator(i), COMB_ADJACENT | COMB_SIBLING);
                if sibling_comb(ci + 1) {
                    self.collect_sibling_left(c);
                }
                if !simple_has_structural(c, 0) && !sibling_comb(ci) {
                    continue;
                }
                if self.structural.add_positive(c, 0) {
                    continue;
                }
                let mut in_siblings = true;
                let mut found = false;
                for j in (0..ci).rev() {
                    in_siblings = in_siblings && sibling_comb(j + 1);
                    let names = if in_siblings {
                        &mut self.structural
                    } else {
                        &mut self.structural_ancestors
                    };
                    if sel.compound(j).is_some_and(|jc| names.add_positive(jc, 0)) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.structural_loose = true;
                }
            }
        }
    }

    fn collect_names_simple(&mut self, c: CompoundRef<'_>, depth: i32) {
        if depth > DEPTH_MAX {
            self.class_names_loose = true;
            self.id_names_loose = true;
            return;
        }
        if let Some(id) = compound_id(c) {
            self.id_names.insert(lower(id));
        }
        for cls in compound_classes(c) {
            self.class_names.insert(lower(cls));
        }
        for name in c.attrs().iter().filter_map(|a| a.name()) {
            if name.to_bytes().eq_ignore_ascii_case(b"class") {
                self.class_names_loose = true;
            } else if name.to_bytes().eq_ignore_ascii_case(b"id") {
                self.id_names_loose = true;
            }
        }
        for p in c.pseudos() {
            if p.kind() == PC_TARGET || p.kind() == PC_TARGET_WITHIN {
                self.id_names_loose = true;
            }
            if let Some(group) = p.of_group() {
                self.collect_names_group(group, depth + 1);
            }
        }
        for group in nested_groups(c) {
            self.collect_names_group(group, depth + 1);
        }
    }

    fn collect_names_group(&mut self, group: GroupRef<'_>, depth: i32) {
        for sel in group.selectors().flatten() {
            for c in compounds(sel) {
                self.collect_names_simple(c, depth);
            }
        }
    }

    fn collect_names(&mut self, sheet: SheetRef<'_>) {
        for rule in sheet.rules() {
            if let Some(selectors) = rule.selector_group() {
                self.collect_names_group(selectors, 0);
            }
            for scope in rule.scopes() {
                for group in [scope.roots(), scope.limits()].into_iter().flatten() {
                    self.collect_names_group(group, 0);
                }
            }
        }
    }

    fn name_change_unused(&self, target: Node<'_>, name: &[u8], old: Option<&[u8]>) -> bool {
        if name.eq_ignore_ascii_case(b"class") {
            let used = |value: Option<&[u8]>| {
                value.is_some_and(|v| tokens(v).any(|t| self.class_names.contains(&lower(t))))
            };
            return !self.class_names_loose && !used(old) && !used(attr(target, c"class"));
        }
        if name.eq_ignore_ascii_case(b"id") {
            let used =
                |value: Option<&[u8]>| value.is_some_and(|v| self.id_names.contains(&lower(v)));
            return !self.id_names_loose && !used(old) && !used(attr(target, c"id"));
        }
        false
    }

    fn attr_may_affect_style(&self, name: &[u8]) -> bool {
        if is_presentational_attr(name) {
            return true;
        }
        let low = lower(name);
        self.attrs.contains(&low) || INTRINSIC_ATTRS.contains(&low.as_slice())
    }

    fn childlist_needs_flood(&self, parent: Node<'_>) -> bool {
        self.structural_loose
            || self.structural.matches(parent)
            || children(parent).any(|c| c.is_element() && self.structural.matches(c))
            || self_and_ancestors(parent).any(|a| self.structural_ancestors.matches(a))
    }

    fn key_change_is_sibling(&self, target: Node<'_>, name: &[u8], old: Option<&[u8]>) -> bool {
        if name.eq_ignore_ascii_case(b"id") {
            let hit =
                |v: Option<&[u8]>| v.is_some_and(|v| !v.is_empty() && self.sibling.ids.contains(v));
            return hit(attr(target, c"id")) || hit(old);
        }
        name.eq_ignore_ascii_case(b"class")
            && (self.sibling.matches_name(target)
                || old.is_some_and(|old| tokens(old).any(|t| self.sibling.classes.contains(t))))
    }

    fn attr_change_is_sibling(&self, target: Node<'_>, name: &[u8], old: Option<&[u8]>) -> bool {
        if self.sibling_loose || self.key_change_is_sibling(target, name, old) {
            return true;
        }
        let low = lower(name);
        if !self.sibling_attrs.contains(&low) {
            return false;
        }
        let present = CString::new(name).is_ok_and(|name| attrs::get(target, &name).is_some());
        self.sibling_value_attrs.contains(&low) || old.is_some() != present
    }
}

impl HasDeps {
    fn collect(sheets: &[SheetRef<'_>], sig: u64) -> HasDeps {
        let mut has = HasDeps {
            sig,
            anchors: Vec::new(),
            loose: false,
        };
        for sel in sheets
            .iter()
            .flat_map(|sheet| sheet.rules())
            .flat_map(|rule| rule.selectors())
            .flatten()
        {
            if sel.len() == 0 || !selector_uses_has(Some(sel), 0) {
                continue;
            }
            if !has.collect_selector(Some(sel), None, 0) {
                has.loose = true;
            }
        }
        has
    }

    fn collect_selector(
        &mut self,
        sel: Option<SelectorRef<'_>>,
        outer: Option<&HasCtx<'_>>,
        depth: i32,
    ) -> bool {
        let Some(sel) = sel.filter(|_| depth <= DEPTH_MAX) else {
            return false;
        };
        let mut found = false;
        for idx in 0..sel.len() {
            found |= self.collect_simple(&HasCtx { sel, idx, outer }, depth);
        }
        found
    }

    fn collect_simple(&mut self, at: &HasCtx<'_>, depth: i32) -> bool {
        let Some(c) = at.sel.compound(at.idx).filter(|_| depth <= DEPTH_MAX) else {
            return false;
        };
        let mut found = false;
        if c.has_group_count() > 0 {
            found = true;
            if !self.add_deps(at, depth) {
                self.loose = true;
            }
        }
        let nested = c
            .pseudos()
            .iter()
            .filter_map(|p| p.of_group())
            .chain(logical_groups(c));
        for group in nested {
            for sel in group.selectors() {
                found |= self.collect_selector(sel, Some(at), depth + 1);
            }
        }
        found
    }

    fn add_deps(&mut self, at: &HasCtx<'_>, depth: i32) -> bool {
        let mut cx = Some(at);
        while let Some(ctx) = cx {
            for i in (0..=ctx.idx).rev() {
                if ctx
                    .sel
                    .compound(i)
                    .is_some_and(|c| self.add_compound(c, depth))
                {
                    return true;
                }
            }
            if ctx.idx + 1 != ctx.sel.len() {
                return false;
            }
            cx = ctx.outer;
        }
        false
    }

    fn add_compound(&mut self, c: CompoundRef<'_>, depth: i32) -> bool {
        if depth > DEPTH_MAX {
            return false;
        }
        if let Some(anchor) = Anchor::from_compound(c) {
            self.anchors.push(anchor);
            return true;
        }
        c.matches_any()
            .any(|group| group.selectors().next().is_some() && self.add_group(group, depth))
    }

    fn add_group(&mut self, group: GroupRef<'_>, depth: i32) -> bool {
        let mark = self.anchors.len();
        for alt in group.selectors() {
            if !alt
                .and_then(subject)
                .is_some_and(|s| self.add_compound(s, depth + 1))
            {
                self.anchors.truncate(mark);
                return false;
            }
        }
        true
    }

    fn anchored(&self, n: Node<'_>) -> bool {
        n.is_element() && self.anchors.iter().any(|a| a.matches(n))
    }
}

impl State {
    fn prepare(&mut self, sheets: &[SheetRef<'_>], sig: u64) -> bool {
        if self.has.as_ref().is_none_or(|has| has.sig != sig) {
            self.has = Some(HasDeps::collect(sheets, sig));
        }
        if self.keys.as_ref().is_none_or(|keys| keys.sig != sig) {
            self.keys = Some(Keys::collect(sheets, sig));
        }
        self.has.as_ref().is_some_and(|has| !has.loose)
    }

    fn mark_region(dirty: &mut AddrSet<usize>, anchor: Node<'_>) {
        let siblings = core::iter::successors(Some(anchor), |n| n.next_sibling());
        dirty.extend(siblings.filter(|n| n.is_element()).map(addr));
    }

    fn mark_has_subjects(&mut self, changed: Node<'_>) {
        let Some(has) = self.has.as_ref() else {
            return;
        };
        if has.loose || has.anchors.is_empty() {
            return;
        }
        let dirty = &mut self.dirty;
        for node in self_and_ancestors(changed) {
            if has.anchored(node) {
                Self::mark_region(dirty, node);
            }
            let mut scanned = 0;
            let earlier = core::iter::successors(node.prev_sibling(), |s| s.prev_sibling());
            for sib in earlier.filter(|s| s.is_element()) {
                scanned += 1;
                if scanned > SIBLING_SCAN_MAX {
                    if let Some(parent) = node.parent() {
                        dirty.insert(addr(parent));
                    }
                    break;
                }
                if has.anchored(sib) {
                    Self::mark_region(dirty, sib);
                }
            }
        }
    }

    fn mark(&mut self, node: Node<'_>) {
        self.dirty.insert(addr(node));
        self.mark_has_subjects(node);
    }

    fn mark_following_siblings(&mut self, from: Node<'_>) -> bool {
        let following = core::iter::successors(Some(from), |n| n.next_sibling());
        for (marked, sib) in following.filter(|n| n.is_element()).enumerate() {
            if marked as u32 >= SIBLING_SCAN_MAX {
                return false;
            }
            self.mark(sib);
        }
        true
    }

    fn mark_childlist(&mut self, parent: Node<'_>, added: Option<Node<'_>>) {
        let flood = self.dirty.contains(&addr(parent))
            || self
                .keys
                .as_ref()
                .is_none_or(|keys| keys.childlist_needs_flood(parent));
        match added {
            _ if flood => self.mark(parent),
            Some(added) => self.mark(added),
            None => self.mark_has_subjects(parent),
        }
    }

    fn attr_may_affect_style(&self, name: Option<&[u8]>) -> bool {
        match (name, &self.keys) {
            (Some(name), Some(keys)) if !name.is_empty() => keys.attr_may_affect_style(name),
            _ => true,
        }
    }

    fn mark_attr(&mut self, target: Node<'_>, name: Option<&[u8]>, old: Option<&[u8]>) {
        if let (Some(name), Some(old)) = (name, old) {
            if CString::new(name).is_ok_and(|name| attr(target, &name) == Some(old)) {
                return;
            }
        }
        if !self.attr_may_affect_style(name) {
            return;
        }
        let fallback = target.parent().unwrap_or(target);
        let sibling = match (&self.keys, name) {
            (None, _) => {
                self.mark(fallback);
                return;
            }
            (Some(keys), Some(name)) => {
                if keys.name_change_unused(target, name, old) {
                    return;
                }
                keys.attr_change_is_sibling(target, name, old)
            }
            (Some(keys), None) => keys.sibling_loose,
        };
        self.mark(target);
        if sibling {
            if let Some(next) = target.next_sibling() {
                if !self.mark_following_siblings(next) {
                    self.mark(fallback);
                }
            }
        }
    }
}

pub(crate) fn prepare(sheets: &[SheetRef<'_>], sig: u64) -> bool {
    with_state(|s| s.prepare(sheets, sig))
}

pub(crate) fn is_dirty(node: Node<'_>) -> bool {
    with_state(|s| s.dirty.contains(&addr(node)))
}

pub(crate) fn clear_dirty() {
    with_state(|s| s.dirty.clear());
}

pub(crate) fn mark_restyle_dirty(node: Node<'_>) {
    with_state(|s| s.mark(node));
}

pub(crate) fn mark_childlist_dirty(parent: Node<'_>, added: Option<Node<'_>>) {
    with_state(|s| s.mark_childlist(parent, added));
}

pub(crate) fn mark_attr_dirty(target: Node<'_>, name: Option<&[u8]>, old: Option<&[u8]>) {
    with_state(|s| s.mark_attr(target, name, old));
}

pub(crate) fn attr_may_affect_style(name: Option<&[u8]>) -> bool {
    with_state(|s| s.attr_may_affect_style(name))
}
