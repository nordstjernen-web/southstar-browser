//! Southstar — matching selectors against elements: compounds with their type, id, class, attribute and pseudo-class tests, :is()/:where()/:not() and memoised relative :has(), complex chains right to left under an operation budget, nth positions cached per batch, and @scope roots and limits with their proximity.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::hash::{BuildHasherDefault, Hasher};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH, Node, attrs};

use crate::element_state;
use crate::ffi::{self, CompoundRef, GroupRef, PseudoRef, RuleRef, ScopeRef, SelectorRef};
use crate::scan::is_ws;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_XML_DOC: u32 = 1 << 14;

const COMB_NONE: u32 = 0;
const COMB_DESCENDANT: u32 = 1;
const COMB_CHILD: u32 = 2;
const COMB_ADJACENT: u32 = 3;
const COMB_SIBLING: u32 = 4;

const PE_NONE: u32 = 0;

const ATTR_PRESENT: u32 = 0;
const ATTR_EQ: u32 = 1;
const ATTR_PREFIX: u32 = 2;
const ATTR_SUFFIX: u32 = 3;
const ATTR_SUBSTR: u32 = 4;
const ATTR_WORD: u32 = 5;
const ATTR_HYPHEN: u32 = 6;

const PC_FIRST_CHILD: u32 = 0;
const PC_LAST_CHILD: u32 = 1;
const PC_ONLY_CHILD: u32 = 2;
const PC_ONLY_OF_TYPE: u32 = 3;
const PC_FIRST_OF_TYPE: u32 = 4;
const PC_LAST_OF_TYPE: u32 = 5;
const PC_ROOT: u32 = 7;
const PC_NTH_CHILD: u32 = 19;
const PC_NTH_LAST_CHILD: u32 = 20;
const PC_NTH_OF_TYPE: u32 = 21;
const PC_NTH_LAST_OF_TYPE: u32 = 22;
const PC_HOVER: u32 = 26;
const PC_ACTIVE: u32 = 27;
const PC_FOCUS: u32 = 28;
const PC_FOCUS_VISIBLE: u32 = 29;
const PC_FOCUS_WITHIN: u32 = 30;
const PC_DEFINED: u32 = 33;
const PC_SCOPE: u32 = 34;
const PC_FULLSCREEN: u32 = 44;
const STATE_KINDS: [u32; 36] = [
    6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 23, 24, 25, 31, 32, 35, 36, 37, 38, 39, 40, 41,
    42, 43, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54,
];

const MATCH_BUDGET: u64 = 8_000_000;
const MAX_CHAIN: i32 = 1024;

#[derive(Default)]
pub(crate) struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }

    fn write_usize(&mut self, n: usize) {
        self.0 = (self.0.rotate_left(5) ^ n as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

type AddrMap<K, V> = HashMap<K, V, BuildHasherDefault<AddrHasher>>;
pub(crate) type AddrSet<K> = HashSet<K, BuildHasherDefault<AddrHasher>>;

#[derive(Clone, Copy)]
struct Position {
    child: i32,
    last_child: i32,
    of_type: i32,
    last_of_type: i32,
}

#[derive(PartialEq, Eq)]
enum Chain {
    Matches,
    FailsLocally,
    FailsAllSiblings,
    FailsCompletely,
}

thread_local! {
    static BATCH_DEPTH: Cell<u32> = const { Cell::new(0) };
    static POSITIONS: RefCell<Option<AddrMap<usize, Position>>> = const { RefCell::new(None) };
    static HAS_MEMO: RefCell<Option<AddrMap<(usize, usize), bool>>> = const { RefCell::new(None) };
    static MATCH_OPS: Cell<u64> = const { Cell::new(0) };
    static MATCH_DEPTH: Cell<i32> = const { Cell::new(0) };
    static CHAIN_DEPTH: Cell<i32> = const { Cell::new(0) };
}

static DEFINED: Mutex<Option<HashSet<Vec<u8>>>> = Mutex::new(None);

fn defined() -> MutexGuard<'static, Option<HashSet<Vec<u8>>>> {
    DEFINED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn register_defined_element(tag: &[u8]) {
    if !tag.is_empty() {
        defined()
            .get_or_insert_with(HashSet::new)
            .insert(tag.to_ascii_lowercase());
    }
}

pub(crate) fn clear_defined_elements() {
    *defined() = None;
}

fn is_defined_element(tag: &[u8]) -> bool {
    defined()
        .as_ref()
        .is_some_and(|set| set.contains(&tag.to_ascii_lowercase()))
}

pub(crate) fn batch_begin() {
    let depth = BATCH_DEPTH.get();
    BATCH_DEPTH.set(depth + 1);
    if depth == 0 {
        POSITIONS.set(Some(AddrMap::default()));
    }
}

pub(crate) fn batch_end() {
    let depth = BATCH_DEPTH.get();
    if depth == 0 {
        return;
    }
    BATCH_DEPTH.set(depth - 1);
    if depth == 1 {
        POSITIONS.set(None);
    }
}

pub(crate) fn has_memo_begin() {
    HAS_MEMO.set(Some(AddrMap::default()));
}

pub(crate) fn has_memo_end() {
    HAS_MEMO.set(None);
}

fn addr(node: Node<'_>) -> usize {
    node.as_ptr() as usize
}

fn raw_name(el: Node<'_>) -> Option<&[u8]> {
    el.name().map(|n| n.to_bytes())
}

fn named(node: Node<'_>, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

fn self_and_ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(Some(node), |n| n.parent())
}

fn prev_element(node: Node<'_>) -> Option<Node<'_>> {
    core::iter::successors(node.prev_sibling(), |s| s.prev_sibling()).find(|s| s.is_element())
}

fn next_element(node: Node<'_>) -> Option<Node<'_>> {
    core::iter::successors(node.next_sibling(), |s| s.next_sibling()).find(|s| s.is_element())
}

fn name_equals_lower(name: &[u8], lower: &[u8]) -> bool {
    name.len() == lower.len()
        && name
            .iter()
            .zip(lower)
            .all(|(&c, &l)| c.to_ascii_lowercase() == l)
}

fn eq_case(a: &[u8], b: &[u8], ci: bool) -> bool {
    if ci {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn word_matches(value: &[u8], want: &[u8], ci: bool) -> bool {
    let mut p = 0;
    while p < value.len() {
        while p < value.len() && is_ws(value[p]) {
            p += 1;
        }
        let token = p;
        while p < value.len() && !is_ws(value[p]) {
            p += 1;
        }
        if p - token == want.len() && eq_case(&value[token..p], want, ci) {
            return true;
        }
    }
    false
}

pub(crate) fn attr_value_matches(op: u32, v: &[u8], want: &[u8], ci: bool) -> bool {
    let (vl, wl) = (v.len(), want.len());
    match op {
        ATTR_EQ => eq_case(v, want, ci),
        ATTR_PREFIX => wl != 0 && vl >= wl && eq_case(&v[..wl], want, ci),
        ATTR_SUFFIX => wl != 0 && vl >= wl && eq_case(&v[vl - wl..], want, ci),
        ATTR_SUBSTR => wl != 0 && v.windows(wl).any(|w| eq_case(w, want, ci)),
        ATTR_WORD => word_matches(v, want, ci),
        ATTR_HYPHEN => vl >= wl && eq_case(&v[..wl], want, ci) && (vl == wl || v[wl] == b'-'),
        _ => true,
    }
}

fn attrs_match(sel: CompoundRef<'_>, el: Node<'_>) -> bool {
    let preds = sel.attrs();
    if preds.is_empty() {
        return true;
    }
    let bloom = attrs::bloom(el);
    let html_doc =
        el.flags() & FLAG_XML_DOC == 0 && el.flags() & (FLAG_FOREIGN_NS | FLAG_SVG_NS) == 0;
    preds.iter().all(|a| {
        if a.name_bit() != 0 && bloom & a.name_bit() == 0 {
            return false;
        }
        let v = a
            .name()
            .and_then(|name| attrs::get(el, name))
            .map(|v| v.to_bytes());
        if a.op() == ATTR_PRESENT {
            return v.is_some();
        }
        let (Some(v), Some(want)) = (v, a.value()) else {
            return false;
        };
        let ci = a.case_insensitive() || (!a.case_sensitive() && html_doc && a.html_ci());
        attr_value_matches(a.op(), v, want, ci)
    })
}

fn fill_positions(parent: Node<'_>, positions: &mut AddrMap<usize, Position>) {
    let elements: Vec<Node<'_>> = children(parent).filter(|c| c.is_element()).collect();
    let n = elements.len() as i32;
    let mut type_counts: HashMap<&[u8], i32> = HashMap::new();
    let mut of_type = Vec::with_capacity(elements.len());
    for &c in &elements {
        let seen = match raw_name(c) {
            Some(name) => {
                let count = type_counts.entry(name).or_insert(0);
                *count += 1;
                *count
            }
            None => 1,
        };
        of_type.push(seen);
    }
    for (i, &c) in elements.iter().enumerate() {
        let index = i as i32 + 1;
        let last_of_type = raw_name(c).map_or(1, |name| type_counts[name] - of_type[i] + 1);
        positions.insert(
            addr(c),
            Position {
                child: index,
                last_child: n - index + 1,
                of_type: of_type[i],
                last_of_type,
            },
        );
    }
}

fn position_of(el: Node<'_>) -> Option<Position> {
    let parent = el.parent()?;
    POSITIONS.with_borrow_mut(|positions| {
        let positions = positions.as_mut()?;
        if let Some(&pos) = positions.get(&addr(el)) {
            return Some(pos);
        }
        fill_positions(parent, positions);
        positions.get(&addr(el)).copied()
    })
}

fn group_matches(group: GroupRef<'_>, el: Node<'_>) -> bool {
    group
        .selectors()
        .any(|sel| sel.is_some_and(|sel| matches(sel, el)))
}

fn sibling(node: Node<'_>, reverse: bool) -> Option<Node<'_>> {
    if reverse {
        node.next_sibling()
    } else {
        node.prev_sibling()
    }
}

fn nth_index(el: Node<'_>, pc: &PseudoRef) -> Option<i32> {
    let kind = pc.kind();
    let of_group = pc.of_group();
    if of_group.is_none() {
        if let Some(pos) = position_of(el) {
            return Some(match kind {
                PC_NTH_CHILD => pos.child,
                PC_NTH_LAST_CHILD => pos.last_child,
                PC_NTH_OF_TYPE => pos.of_type,
                _ => pos.last_of_type,
            });
        }
    }
    let reverse = kind == PC_NTH_LAST_CHILD || kind == PC_NTH_LAST_OF_TYPE;
    let typed = kind == PC_NTH_OF_TYPE || kind == PC_NTH_LAST_OF_TYPE;
    let name = raw_name(el);
    let mut idx = 1i32;
    let mut s = sibling(el, reverse);
    while let Some(sib) = s {
        if sib.is_element()
            && (!typed || name.is_some_and(|n| named(sib, n)))
            && of_group.is_none_or(|g| group_matches(g, sib))
        {
            idx = idx.wrapping_add(1);
        }
        s = sibling(sib, reverse);
    }
    if of_group.is_some_and(|g| !group_matches(g, el)) {
        return None;
    }
    Some(idx)
}

fn nth_matches(el: Node<'_>, pc: &PseudoRef) -> bool {
    let Some(idx) = nth_index(el, pc) else {
        return false;
    };
    let (a, b) = (pc.a(), pc.b());
    if a == 0 {
        return idx == b;
    }
    let diff = idx.wrapping_sub(b);
    diff.wrapping_rem(a) == 0 && diff.wrapping_div(a) >= 0
}

fn under(node: Option<Node<'_>>, el: Node<'_>) -> bool {
    node.is_some_and(|n| self_and_ancestors(n).any(|a| a == el))
}

fn pseudo_matches(el: Node<'_>, pc: &PseudoRef) -> bool {
    let kind = pc.kind();
    match kind {
        PC_FIRST_CHILD => prev_element(el).is_none(),
        PC_LAST_CHILD => next_element(el).is_none(),
        PC_ONLY_CHILD => prev_element(el).is_none() && next_element(el).is_none(),
        PC_ONLY_OF_TYPE | PC_FIRST_OF_TYPE | PC_LAST_OF_TYPE => {
            let Some(name) = raw_name(el) else {
                return false;
            };
            let before = kind != PC_LAST_OF_TYPE
                && core::iter::successors(el.prev_sibling(), |s| s.prev_sibling())
                    .any(|s| named(s, name));
            let after = kind != PC_FIRST_OF_TYPE
                && core::iter::successors(el.next_sibling(), |s| s.next_sibling())
                    .any(|s| named(s, name));
            !before && !after
        }
        PC_ROOT => el
            .parent()
            .is_some_and(|p| p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0),
        PC_SCOPE => match ffi::match_scope() {
            Some(scope) => el == scope,
            None => !el.parent().is_some_and(|p| p.is_element()),
        },
        PC_NTH_CHILD | PC_NTH_LAST_CHILD | PC_NTH_OF_TYPE | PC_NTH_LAST_OF_TYPE => {
            nth_matches(el, pc)
        }
        PC_HOVER => under(ffi::hover_node(), el),
        PC_ACTIVE => under(ffi::active_node(), el),
        PC_FOCUS => ffi::focus_node() == Some(el),
        PC_FOCUS_VISIBLE => ffi::focus_node() == Some(el) && ffi::focus_visible_node() == Some(el),
        PC_FOCUS_WITHIN => under(ffi::focus_node(), el),
        PC_DEFINED => {
            raw_name(el).is_some_and(|name| !name.contains(&b'-') || is_defined_element(name))
        }
        PC_FULLSCREEN => ffi::fullscreen_node() == Some(el),
        _ if STATE_KINDS.contains(&kind) => element_state::matches(el, kind, pc.arg()),
        _ => true,
    }
}

fn type_matches(sel: CompoundRef<'_>, el: Node<'_>) -> bool {
    let Some(ty) = sel.type_name().filter(|t| *t != b"*") else {
        return true;
    };
    let Some(name) = raw_name(el) else {
        return false;
    };
    if el.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) != 0 {
        name == ty
    } else {
        name_equals_lower(name, ty)
    }
}

pub(crate) fn matches_compound(sel: CompoundRef<'_>, el: Node<'_>) -> bool {
    if sel.never_match() || !el.is_element() {
        return false;
    }
    if sel.ns_none() {
        let null_ns = el.flags() & FLAG_FOREIGN_NS != 0
            && el.flags() & FLAG_SVG_NS == 0
            && attrs::get(el, c"data-nd-ns-uri").is_none();
        if !null_ns {
            return false;
        }
    }
    if !type_matches(sel, el) {
        return false;
    }
    if let Some(id) = sel.id() {
        if attrs::get(el, c"id").map(|v| v.to_bytes()) != Some(id) {
            return false;
        }
    }
    if !sel.classes().all(|class| attrs::has_class(el, class)) {
        return false;
    }
    if !attrs_match(sel, el) {
        return false;
    }
    if !sel.pseudos().iter().all(|pc| pseudo_matches(el, pc)) {
        return false;
    }
    if !sel.matches_any().all(|group| group_matches(group, el)) {
        return false;
    }
    if sel.matches_none().any(|group| group_matches(group, el)) {
        return false;
    }
    sel.has_groups().all(|group| has_group_matches(group, el))
}

fn has_scope_pseudo(sel: CompoundRef<'_>) -> bool {
    sel.pseudos().iter().any(|pc| pc.kind() == PC_SCOPE)
}

fn relative_try(
    rel: SelectorRef<'_>,
    anchor: Node<'_>,
    candidate: Option<Node<'_>>,
    idx: usize,
    depth: i32,
) -> bool {
    let Some(candidate) = candidate.filter(|c| c.is_element()) else {
        return false;
    };
    rel.compound(idx)
        .is_some_and(|c| matches_compound(c, candidate))
        && relative_chain(rel, anchor, candidate, idx + 1, depth + 1)
}

fn relative_descendants(
    rel: SelectorRef<'_>,
    anchor: Node<'_>,
    base: Node<'_>,
    idx: usize,
    depth: i32,
) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    children(base).filter(|c| c.is_element()).any(|c| {
        relative_try(rel, anchor, Some(c), idx, depth)
            || relative_descendants(rel, anchor, c, idx, depth + 1)
    })
}

fn relative_chain(
    rel: SelectorRef<'_>,
    anchor: Node<'_>,
    base: Node<'_>,
    idx: usize,
    depth: i32,
) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if idx >= rel.len() {
        return true;
    }
    let Some(compound) = rel.compound(idx) else {
        return false;
    };
    let comb = rel.combinator(idx);
    if idx == 0 && (comb == COMB_NONE || comb == COMB_DESCENDANT) {
        if has_scope_pseudo(compound) && relative_try(rel, anchor, Some(anchor), idx, depth) {
            return true;
        }
        return relative_descendants(rel, anchor, anchor, idx, depth + 1);
    }
    match comb {
        COMB_CHILD => children(base).any(|c| relative_try(rel, anchor, Some(c), idx, depth)),
        COMB_ADJACENT => relative_try(rel, anchor, next_element(base), idx, depth),
        COMB_SIBLING => core::iter::successors(next_element(base), |s| next_element(*s))
            .any(|s| relative_try(rel, anchor, Some(s), idx, depth)),
        _ => relative_descendants(rel, anchor, base, idx, depth + 1),
    }
}

fn has_relative_matches(rel: Option<SelectorRef<'_>>, anchor: Node<'_>) -> bool {
    let Some(rel) = rel.filter(|r| r.pseudo_element() == PE_NONE) else {
        return false;
    };
    let prev = ffi::match_scope();
    if prev.is_none() {
        ffi::set_match_scope(Some(anchor));
    }
    let matched = relative_chain(rel, anchor, anchor, 0, 0);
    ffi::set_match_scope(prev);
    matched
}

fn has_group_matches(group: GroupRef<'_>, anchor: Node<'_>) -> bool {
    let key = (group.addr(), addr(anchor));
    if let Some(cached) =
        HAS_MEMO.with_borrow(|memo| memo.as_ref().and_then(|m| m.get(&key).copied()))
    {
        return cached;
    }
    let matched = group
        .selectors()
        .any(|sel| has_relative_matches(sel, anchor));
    HAS_MEMO.with_borrow_mut(|memo| {
        if let Some(memo) = memo.as_mut() {
            memo.insert(key, matched);
        }
    });
    matched
}

fn take_op() -> bool {
    let ops = MATCH_OPS.get() + 1;
    MATCH_OPS.set(ops);
    ops <= MATCH_BUDGET
}

fn compound_then_chain(sel: SelectorRef<'_>, idx: usize, el: Node<'_>) -> Chain {
    if CHAIN_DEPTH.get() >= MAX_CHAIN {
        return Chain::FailsCompletely;
    }
    if !sel.compound(idx).is_some_and(|c| matches_compound(c, el)) {
        return Chain::FailsLocally;
    }
    CHAIN_DEPTH.set(CHAIN_DEPTH.get() + 1);
    let result = complex_chain(sel, idx, el);
    CHAIN_DEPTH.set(CHAIN_DEPTH.get() - 1);
    result
}

fn complex_chain(sel: SelectorRef<'_>, idx: usize, cur: Node<'_>) -> Chain {
    if idx == 0 {
        return Chain::Matches;
    }
    match sel.combinator(idx) {
        COMB_CHILD => {
            if !take_op() {
                return Chain::FailsCompletely;
            }
            match cur.parent() {
                Some(p) => compound_then_chain(sel, idx - 1, p),
                None => Chain::FailsCompletely,
            }
        }
        COMB_ADJACENT => {
            let s = prev_element(cur);
            if !take_op() {
                return Chain::FailsCompletely;
            }
            match s {
                Some(s) => compound_then_chain(sel, idx - 1, s),
                None => Chain::FailsAllSiblings,
            }
        }
        COMB_SIBLING => {
            let mut depth = 0;
            let mut s = cur.prev_sibling();
            while let Some(sib) = s {
                if depth >= MAX_DEPTH {
                    return Chain::FailsLocally;
                }
                depth += 1;
                if !take_op() {
                    return Chain::FailsCompletely;
                }
                if sib.is_element() {
                    let r = compound_then_chain(sel, idx - 1, sib);
                    if r != Chain::FailsLocally {
                        return r;
                    }
                }
                s = sib.prev_sibling();
            }
            Chain::FailsAllSiblings
        }
        _ => {
            let mut depth = 0;
            let mut p = cur.parent();
            while let Some(ancestor) = p {
                if depth >= MAX_DEPTH {
                    return Chain::FailsLocally;
                }
                depth += 1;
                if ancestor.kind() == Kind::Document {
                    return Chain::FailsCompletely;
                }
                if !take_op() {
                    return Chain::FailsCompletely;
                }
                let r = compound_then_chain(sel, idx - 1, ancestor);
                if r == Chain::Matches || r == Chain::FailsCompletely {
                    return r;
                }
                p = ancestor.parent();
            }
            Chain::FailsCompletely
        }
    }
}

fn matches_structural(sel: SelectorRef<'_>, el: Node<'_>) -> bool {
    let len = sel.len();
    if len == 0 {
        return false;
    }
    if MATCH_DEPTH.get() == 0 {
        MATCH_OPS.set(0);
    }
    MATCH_DEPTH.set(MATCH_DEPTH.get() + 1);
    let matched = compound_then_chain(sel, len - 1, el) == Chain::Matches;
    MATCH_DEPTH.set(MATCH_DEPTH.get() - 1);
    matched
}

pub(crate) fn matches(sel: SelectorRef<'_>, el: Node<'_>) -> bool {
    sel.pseudo_element() == PE_NONE && matches_structural(sel, el)
}

fn matches_for(sel: SelectorRef<'_>, el: Node<'_>, pe: u32) -> bool {
    sel.pseudo_element() == pe && matches_structural(sel, el)
}

fn group_matches_with_scope(
    group: Option<GroupRef<'_>>,
    el: Node<'_>,
    scope: Option<Node<'_>>,
) -> bool {
    let prev = ffi::match_scope();
    ffi::set_match_scope(scope);
    let matched = group.is_some_and(|g| group_matches(g, el));
    ffi::set_match_scope(prev);
    matched
}

fn scope_root_matches(scope: ScopeRef<'_>, el: Node<'_>) -> bool {
    group_matches_with_scope(scope.roots(), el, ffi::match_scope())
}

fn scope_hops(root: Node<'_>, el: Node<'_>) -> Option<i32> {
    self_and_ancestors(el)
        .position(|n| n == root)
        .map(|hops| hops as i32)
}

fn scope_limit_excludes(scope: ScopeRef<'_>, root: Node<'_>, el: Node<'_>) -> bool {
    let Some(limits) = scope.limits() else {
        return false;
    };
    for n in self_and_ancestors(el) {
        if n.is_element() && group_matches_with_scope(Some(limits), n, Some(root)) {
            return true;
        }
        if n == root {
            break;
        }
    }
    false
}

fn scope_contains(scope: ScopeRef<'_>, root: Node<'_>, el: Node<'_>) -> bool {
    scope_hops(root, el).is_some() && !scope_limit_excludes(scope, root, el)
}

fn scope_applies_to(scope: ScopeRef<'_>, el: Node<'_>) -> bool {
    self_and_ancestors(el)
        .filter(|root| root.is_element())
        .any(|root| scope_root_matches(scope, root) && scope_contains(scope, root, el))
}

pub(crate) fn rule_matches(
    rule: Option<RuleRef<'_>>,
    sel: Option<SelectorRef<'_>>,
    el: Node<'_>,
    pe: u32,
) -> Option<i32> {
    let one = |sel: Option<SelectorRef<'_>>| {
        sel.is_some_and(|sel| {
            if pe == PE_NONE {
                matches(sel, el)
            } else {
                matches_for(sel, el, pe)
            }
        })
    };
    let scopes: Vec<ScopeRef<'_>> = rule.map(|r| r.scopes().collect()).unwrap_or_default();
    let Some((&inner, outer)) = scopes.split_last() else {
        return one(sel).then_some(0);
    };
    let mut best = 0;
    for root in self_and_ancestors(el).filter(|n| n.is_element()) {
        if !scope_root_matches(inner, root) || !scope_contains(inner, root, el) {
            continue;
        }
        if !outer
            .iter()
            .all(|&s| scope_applies_to(s, root) && scope_applies_to(s, el))
        {
            continue;
        }
        let prev = ffi::match_scope();
        ffi::set_match_scope(Some(root));
        let matched = one(sel);
        ffi::set_match_scope(prev);
        if !matched {
            continue;
        }
        if let Some(hops) = scope_hops(root, el) {
            best = best.max(i32::MAX - hops);
        }
    }
    (best > 0).then_some(best)
}
