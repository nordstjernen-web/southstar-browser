//! Southstar — the C ABI of gathering an element's matching rules: candidates looked up in a sheet's rule index by id, class, tag and attribute, filtered by @container conditions and the ancestor Bloom filter, matched with a per-pass selector cache, and appended as css.c's match_entry, var_match and pending_match arrays per pseudo-element, followed by the element's presentational hints and style attribute.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_int, c_uint, c_void};
use core::hash::BuildHasherDefault;
use core::ptr;
use core::slice;
use std::collections::HashMap;

use southstar_dom::{Node, attrs};
use southstar_glib::{self as glib, GBoolean};

use super::cascade::RawMatch;
use super::custom_props::RawVarMatch;
use super::declarations::{DECL_SLOT_SPAN, Rule};
use super::layers::LayerRanks;
use super::matcher::match_scope;
use super::pending::RawPendingMatch;
use super::rule_index::{Buckets, Candidate, RuleIndex, rule_index_build};
use super::selector::SelectorData;
use super::selector_view::{RuleRef, SelectorRef};
use super::sheet::{OwnedSheet, SheetData};
use crate::gather::{Accum, AncestorFilter, DESTS, class_tokens};
use crate::hints;
use crate::matcher::{self, AddrHasher};
use crate::selector::{attr_value_hash, identifier_hash};

const LAYER_NONE: c_int = c_int::MAX;
const SELECTOR_CACHE_MAX: usize = 262_144;
const DECL_SHEETS_MAX: usize = 8192;
const ORIGIN_PRESENTATIONAL: c_int = 1;
const ORIGIN_AUTHOR: c_int = 2;
const INLINE_SPECIFICITY: (c_int, c_int, c_int) = (1000, 0, 0);

#[derive(Default)]
pub(super) struct Matches {
    pub(super) decls: Vec<RawMatch>,
    pub(super) vars: Vec<RawVarMatch>,
    pub(super) pending: Vec<RawPendingMatch>,
}

impl Matches {
    pub(super) fn clear(&mut self) {
        self.decls.clear();
        self.vars.clear();
        self.pending.clear();
    }
}

pub(super) type SavedFilter = Box<[u8; crate::gather::FILTER_SIZE]>;

type CacheKey = (usize, usize, usize, c_uint);
type SelectorCache = HashMap<CacheKey, (bool, c_int), BuildHasherDefault<AddrHasher>>;

struct Scratch {
    candidates: Vec<Candidate>,
    accums: Vec<Accum>,
    matched: Vec<usize>,
    epoch: u32,
    key: Vec<u8>,
}

thread_local! {
    static FILTER: RefCell<AncestorFilter> = const { RefCell::new(AncestorFilter::new()) };
    static SELECTOR_CACHE: RefCell<Option<SelectorCache>> = const { RefCell::new(None) };
    static SCRATCH: RefCell<Scratch> = const {
        RefCell::new(Scratch {
            candidates: Vec::new(),
            accums: Vec::new(),
            matched: Vec::new(),
            epoch: 0,
            key: Vec::new(),
        })
    };
}

fn filter_update(filter: &mut AncestorFilter, el: Node<'_>, delta: i32) {
    if let Some(name) = el.name() {
        filter.count(identifier_hash(b'%', name.to_bytes()), delta);
    }
    if let Some(id) = attrs::get(el, c"id") {
        filter.count(identifier_hash(b'#', id.to_bytes()), delta);
    }
    if let Some(cls) = attrs::get(el, c"class") {
        for token in class_tokens(cls.to_bytes()) {
            filter.count(identifier_hash(b'.', token), delta);
        }
    }
    if filter.attrs {
        for attr in el.attrs() {
            if let (Some(name), Some(value)) = (attr.name(), attr.value()) {
                filter.count(attr_value_hash(name.to_bytes(), value.to_bytes()), delta);
            }
        }
    }
}

pub(super) fn ancestor_filter_begin(attr_hashes: bool) {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        f.counts.fill(0);
        f.active = true;
        f.attrs = attr_hashes;
    });
}

pub(super) fn ancestor_filter_end() {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        f.active = false;
        f.subject = 0;
    });
}

pub(super) fn ancestor_filter_subject(node: Node<'_>) {
    FILTER.with(|f| f.borrow_mut().subject = node.as_ptr() as usize);
}

pub(super) fn ancestor_filter_enter(el: Node<'_>) -> bool {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        let enter = f.active && el.is_element() && el.first_child().is_some();
        if enter {
            filter_update(&mut f, el, 1);
        }
        enter
    })
}

pub(super) fn ancestor_filter_leave(el: Node<'_>) {
    FILTER.with(|f| filter_update(&mut f.borrow_mut(), el, -1));
}

pub(super) fn ancestor_filter_save() -> Option<SavedFilter> {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        if !f.active {
            return None;
        }
        let saved = Box::new(f.counts);
        f.counts.fill(0);
        Some(saved)
    })
}

pub(super) fn ancestor_filter_restore(saved: Option<SavedFilter>) {
    if let Some(saved) = saved {
        FILTER.with(|f| f.borrow_mut().counts = *saved);
    }
}

pub fn selector_cache_begin() {
    SELECTOR_CACHE.with(|c| *c.borrow_mut() = Some(SelectorCache::default()));
}

pub fn selector_cache_end() {
    SELECTOR_CACHE.with(|c| *c.borrow_mut() = None);
}

fn cached_match(
    cache: &mut Option<SelectorCache>,
    rule: &Rule,
    sel: &SelectorData,
    el: Option<Node<'_>>,
    pe: c_uint,
) -> (bool, c_int) {
    let key = (
        ptr::from_ref(rule) as usize,
        ptr::from_ref(sel) as usize,
        Node::ptr_or_null(el) as usize,
        pe,
    );
    if let Some(&hit) = cache.as_ref().and_then(|m| m.get(&key)) {
        return hit;
    }
    let order = el.and_then(|el| {
        matcher::rule_matches(
            Some(RuleRef::new(rule)),
            Some(SelectorRef::new(sel)),
            el,
            pe,
        )
    });
    let result = (order.is_some(), order.unwrap_or(0));
    if let Some(map) = cache.as_mut()
        && map.len() < SELECTOR_CACHE_MAX
    {
        map.insert(key, result);
    }
    result
}

pub(super) fn rule_index_ensure(sheet: &SheetData) {
    sheet.index.get_or_init(|| rule_index_build(sheet));
}

fn bucket<'a>(table: &'a Buckets, key: &mut Vec<u8>, name: &[u8], lower: bool) -> &'a [Candidate] {
    if table.is_empty() {
        return &[];
    }
    let found = if lower {
        key.clear();
        key.extend(name.iter().map(u8::to_ascii_lowercase));
        table.get(key.as_slice())
    } else {
        table.get(name)
    };
    found.map_or(&[], Vec::as_slice)
}

fn collect_candidates(idx: &RuleIndex, el: Option<Node<'_>>, scratch: &mut Scratch) {
    let Scratch {
        candidates, key, ..
    } = scratch;
    candidates.clear();
    if let Some(el) = el.filter(|e| e.is_element()) {
        if let Some(id) = attrs::get(el, c"id").filter(|id| !id.is_empty()) {
            candidates.extend_from_slice(bucket(&idx.by_id, key, id.to_bytes(), false));
        }
        if let Some(cls) = attrs::get(el, c"class") {
            for token in class_tokens(cls.to_bytes()) {
                candidates.extend_from_slice(bucket(&idx.by_class, key, token, false));
            }
        }
        if let Some(name) = el.name().filter(|n| !n.is_empty()) {
            candidates.extend_from_slice(bucket(&idx.by_tag, key, name.to_bytes(), true));
        }
        if !idx.by_attr.is_empty() {
            for attr in el.attrs() {
                if let Some(name) = attr.name() {
                    candidates.extend_from_slice(bucket(&idx.by_attr, key, name.to_bytes(), true));
                }
            }
        }
    }
    candidates.extend_from_slice(&idx.universal);
}

fn layer_rank(layer_ranks: &LayerRanks, name: Option<&[u8]>) -> c_int {
    name.and_then(|name| layer_ranks.get(name).copied())
        .unwrap_or(LAYER_NONE)
}

struct Output {
    origin: c_int,
    sheet_index: c_int,
    source_order: Option<c_int>,
    inline_style: GBoolean,
}

impl Output {
    fn emit(&self, outs: &mut [Matches], rule: &Rule, acc: &Accum, layer_order: c_int) {
        let rule_ptr = ptr::from_ref(rule).cast::<c_void>();
        for (dd, dst) in outs.iter_mut().enumerate() {
            if !acc.any[dd] {
                continue;
            }
            let (spec_a, spec_b, spec_c) = acc.specificity[dd];
            let scope_order = acc.scope_order[dd];
            for (di, d) in rule.decls.iter().enumerate() {
                let entry = RawMatch {
                    origin: self.origin,
                    spec_a,
                    spec_b,
                    spec_c,
                    sheet_index: self.sheet_index,
                    layer_order,
                    scope_order,
                    source_order: self.source_order.unwrap_or(rule.source_order),
                    decl_order: di as c_int * DECL_SLOT_SPAN,
                    important: glib::boolean(d.important()),
                    inline_style: self.inline_style,
                    rule: rule_ptr,
                    value: d.value_handle().clone(),
                    prop: d.prop(),
                };
                dst.decls.push(entry);
            }
            if let Some(vars) = &rule.vars {
                for (decl_order, (k, v, important)) in vars.entries().enumerate() {
                    let entry = RawVarMatch {
                        origin: self.origin,
                        spec_a,
                        spec_b,
                        spec_c,
                        sheet_index: self.sheet_index,
                        layer_order,
                        scope_order,
                        source_order: self.source_order.unwrap_or(rule.source_order),
                        decl_order: decl_order as c_int,
                        important: glib::boolean(important),
                        inline_style: self.inline_style,
                        rule: rule_ptr,
                        name: k.clone(),
                        text: v.clone(),
                    };
                    dst.vars.push(entry);
                }
            }
            for pd in &rule.pending {
                let entry = RawPendingMatch {
                    origin: self.origin,
                    spec_a,
                    spec_b,
                    spec_c,
                    sheet_index: self.sheet_index,
                    layer_order,
                    scope_order,
                    source_order: self.source_order.unwrap_or(rule.source_order),
                    decl_order_base: pd.decl_slot(),
                    inline_style: self.inline_style,
                    rule: rule_ptr,
                    pd: pd.clone(),
                };
                dst.pending.push(entry);
            }
        }
    }
}

struct Gather<'a> {
    rules: &'a [Rule],
    pes: &'a [c_uint],
    dest_of_pe: &'a [Option<usize>; DESTS],
    node: Option<Node<'a>>,
    filter: Option<&'a AncestorFilter>,
}

impl Gather<'_> {
    fn rejected(&self, rule: &Rule, sel: &SelectorData) -> bool {
        let Some(filter) = self.filter else {
            return false;
        };
        if !rule.scopes.is_empty() {
            return false;
        }
        let n = (sel.n_ancestor_hashes as usize).min(sel.ancestor_hashes.len());
        filter.rejects(
            &sel.ancestor_hashes[..n],
            sel.n_ancestor_attr_hashes as usize,
        )
    }

    fn run(
        &self,
        idx: &RuleIndex,
        cache: &mut Option<SelectorCache>,
        scratch: &mut Scratch,
        outs: &mut [Matches],
        layer_ranks: &LayerRanks,
        origin: Origin,
    ) {
        collect_candidates(idx, self.node, scratch);
        if scratch.accums.len() < self.rules.len() {
            scratch.accums.resize(self.rules.len(), Accum::default());
        }
        scratch.epoch = scratch.epoch.wrapping_add(1);
        if scratch.epoch == 0 {
            scratch.accums.fill(Accum::default());
            scratch.epoch = 1;
        }
        let epoch = scratch.epoch;
        scratch.matched.clear();
        let candidates = core::mem::take(&mut scratch.candidates);
        for cand in &candidates {
            let ri = cand.rule_idx as usize;
            let Some(rule) = self.rules.get(ri) else {
                continue;
            };
            let Some(sel) = rule.selectors.selectors().get(cand.selector_idx as usize) else {
                continue;
            };
            if rule.container_condition.is_some() && !rule.container_matches() {
                continue;
            }
            if self.rejected(rule, sel) {
                continue;
            }
            let Some(&Some(dd)) = self.dest_of_pe.get(sel.pseudo_element as usize) else {
                continue;
            };
            let pe = self.pes[dd];
            if pe != 0 && rule.pe_mask.get() & 1u32.checked_shl(pe).unwrap_or(0) == 0 {
                continue;
            }
            if sel.pseudo_element != pe {
                continue;
            }
            let (matched, scope_order) = cached_match(cache, rule, sel, self.node, pe);
            if !matched {
                continue;
            }
            if rule.container_condition.is_some() {
                crate::container::note_features();
            }
            let acc = &mut scratch.accums[ri];
            if acc.epoch != epoch {
                *acc = Accum {
                    epoch,
                    ..Accum::default()
                };
                scratch.matched.push(ri);
            }
            let spec = (sel.spec_a, sel.spec_b, sel.spec_c);
            acc.note(dd, spec, scope_order);
        }
        scratch.candidates = candidates;
        let output = Output {
            origin: origin.origin,
            sheet_index: origin.sheet_index,
            source_order: None,
            inline_style: glib::FALSE,
        };
        for &ri in &scratch.matched {
            let Some(rule) = self.rules.get(ri) else {
                continue;
            };
            let acc = &mut scratch.accums[ri];
            let layer_order = *acc
                .layer_order
                .get_or_insert_with(|| layer_rank(layer_ranks, rule.layer_name.as_deref()));
            output.emit(outs, rule, acc, layer_order);
        }
    }
}

pub(super) struct Origin {
    pub(super) origin: c_int,
    pub(super) sheet_index: c_int,
}

pub(super) fn gather_matches(
    sheet: &SheetData,
    origin: Origin,
    el: Node<'_>,
    pes: &[c_uint],
    outs: &mut [Matches],
    layer_ranks: &LayerRanks,
) {
    let Some(idx) = sheet.index.get() else {
        return;
    };
    let rules = &sheet.rules[..];
    let mut dest_of_pe = [None; DESTS];
    for (dd, &pe) in pes.iter().enumerate() {
        if let Some(slot) = dest_of_pe.get_mut(pe as usize) {
            *slot = Some(dd);
        }
    }
    FILTER.with(|f| {
        SELECTOR_CACHE.with(|c| {
            SCRATCH.with(|s| {
                let filter = f.borrow();
                let filter_usable = filter.active
                    && filter.subject == el.as_ptr() as usize
                    && match_scope().is_none();
                let mut cache = c.borrow_mut();
                let mut scratch = s.borrow_mut();
                let gather = Gather {
                    rules,
                    pes,
                    dest_of_pe: &dest_of_pe,
                    node: Some(el),
                    filter: filter_usable.then_some(&*filter),
                };
                gather.run(idx, &mut cache, &mut scratch, outs, layer_ranks, origin);
            })
        })
    });
}

thread_local! {
    static DECL_SHEETS: RefCell<HashMap<Vec<u8>, OwnedSheet>> = RefCell::new(HashMap::new());
}

fn emit_decl_sheet(
    decls: &[u8],
    output: &Output,
    out: &mut Matches,
    specificity: (c_int, c_int, c_int),
) {
    let decls = &decls[..decls.iter().position(|&c| c == 0).unwrap_or(decls.len())];
    if decls.is_empty() {
        return;
    }
    DECL_SHEETS.with_borrow_mut(|sheets| {
        let sheet = sheets.entry(decls.to_vec()).or_insert_with(|| {
            let wrapped = [&b"* { "[..], decls, b" }"].concat();
            OwnedSheet::parse(&wrapped)
        });
        let mut acc = Accum::default();
        acc.note(0, specificity, 0);
        for rule in sheet.rules() {
            output.emit(slice::from_mut(out), rule, &acc, LAYER_NONE);
        }
    });
}

pub(super) fn gather_element_declarations(node: Node<'_>, out: &mut Matches) {
    if let Some(hints) = hints::presentational_hints(node) {
        let output = Output {
            origin: ORIGIN_PRESENTATIONAL,
            sheet_index: 0,
            source_order: Some(c_int::MIN),
            inline_style: glib::FALSE,
        };
        emit_decl_sheet(&hints, &output, out, (0, 0, 0));
    }
    if let Some(style) = node.attr(c"style") {
        let output = Output {
            origin: ORIGIN_AUTHOR,
            sheet_index: 0,
            source_order: Some(c_int::MAX),
            inline_style: glib::TRUE,
        };
        emit_decl_sheet(style.to_bytes(), &output, out, INLINE_SPECIFICITY);
    }
}

pub(super) fn decl_sheet_cache_trim() {
    let dropped = DECL_SHEETS.with_borrow_mut(|sheets| {
        if sheets.len() >= DECL_SHEETS_MAX {
            core::mem::take(sheets)
        } else {
            HashMap::new()
        }
    });
    drop(dropped);
}
