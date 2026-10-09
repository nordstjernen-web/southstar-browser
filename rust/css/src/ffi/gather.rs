//! Southstar — the C ABI of gathering an element's matching rules: candidates looked up in a sheet's rule index by id, class, tag and attribute, filtered by @container conditions and the ancestor Bloom filter, matched with a per-pass selector cache, and appended as css.c's match_entry, var_match and pending_match arrays per pseudo-element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_uint, c_void};
use core::hash::BuildHasherDefault;
use core::mem::size_of;
use core::ptr;
use core::slice;
use std::collections::HashMap;

use southstar_dom::{Node, NsNode, attrs};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::cascade::RawMatch;
use super::container::{ns_css_container_features_note, ns_css_container_rule_matches};
use super::custom_props::RawVarMatch;
use super::declarations::{DECL_SLOT_SPAN, RawPending, RawRule};
use super::matcher::match_scope;
use super::pending::RawPendingMatch;
use super::rule_index::{Candidate, RawIndex, ns_css_rule_index_build};
use super::selector::RawSelector;
use super::selector_view::{RuleRef, SelectorRef};
use super::sheet::RawSheet;
use super::shorthand::RawDecl;
use crate::gather::{Accum, AncestorFilter, DESTS, class_tokens};
use crate::matcher::{self, AddrHasher};
use crate::selector::{attr_value_hash, identifier_hash};

const LAYER_NONE: c_int = c_int::MAX;
const SELECTOR_CACHE_MAX: usize = 262_144;

#[repr(C)]
struct RawDest {
    pe: c_uint,
    out: *mut GArray,
    var_out: *mut GArray,
    pending_out: *mut GArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawDest>() == 32);

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

unsafe fn pointers<'a, T>(array: *const GPtrArray) -> &'a [*mut T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.pdata.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.pdata.cast::<*mut T>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe fn elements<'a, T>(array: *const GArray) -> &'a [T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.data.cast::<T>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe fn append<T>(array: *mut GArray, item: &T) {
    unsafe { glib::g_array_append_vals(array, ptr::from_ref(item).cast(), 1) };
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

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_ancestor_filter_begin(attr_hashes: GBoolean) {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        f.counts.fill(0);
        f.active = true;
        f.attrs = attr_hashes != 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_ancestor_filter_end() {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        f.active = false;
        f.subject = 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_ancestor_filter_subject(node: *const NsNode) {
    FILTER.with(|f| f.borrow_mut().subject = node as usize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ancestor_filter_enter(node: *const NsNode) -> GBoolean {
    let Some(el) = (unsafe { Node::from_ptr(node) }) else {
        return glib::FALSE;
    };
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        let enter = f.active && el.is_element() && el.first_child().is_some();
        if enter {
            filter_update(&mut f, el, 1);
        }
        glib::boolean(enter)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ancestor_filter_leave(node: *const NsNode) {
    if let Some(el) = unsafe { Node::from_ptr(node) } {
        FILTER.with(|f| filter_update(&mut f.borrow_mut(), el, -1));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_ancestor_filter_save() -> *mut c_void {
    FILTER.with(|f| {
        let mut f = f.borrow_mut();
        if !f.active {
            return ptr::null_mut();
        }
        let saved = Box::new(f.counts);
        f.counts.fill(0);
        Box::into_raw(saved).cast()
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ancestor_filter_restore(saved: *mut c_void) {
    if saved.is_null() {
        return;
    }
    let saved = unsafe { Box::from_raw(saved.cast::<[u8; crate::gather::FILTER_SIZE]>()) };
    FILTER.with(|f| f.borrow_mut().counts = *saved);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_selector_cache_begin() {
    SELECTOR_CACHE.with(|c| *c.borrow_mut() = Some(SelectorCache::default()));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_selector_cache_end() {
    SELECTOR_CACHE.with(|c| *c.borrow_mut() = None);
}

fn cached_match(
    cache: &mut Option<SelectorCache>,
    rule: &RawRule,
    sel: *const RawSelector,
    el: Option<Node<'_>>,
    pe: c_uint,
) -> (bool, c_int) {
    let key = (
        ptr::from_ref(rule) as usize,
        sel as usize,
        Node::ptr_or_null(el) as usize,
        pe,
    );
    if let Some(&hit) = cache.as_ref().and_then(|m| m.get(&key)) {
        return hit;
    }
    let order = el.and_then(|el| {
        matcher::rule_matches(
            unsafe { RuleRef::from_ptr(ptr::from_ref(rule).cast()) },
            unsafe { SelectorRef::from_ptr(sel.cast()) },
            el,
            pe,
        )
    });
    let result = (order.is_some(), order.unwrap_or(0));
    if let Some(map) = cache.as_mut() {
        if map.len() < SELECTOR_CACHE_MAX {
            map.insert(key, result);
        }
    }
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_rule_index_ensure(sheet: *const RawSheet) -> *const RawIndex {
    let Some(raw) = (unsafe { sheet.cast_mut().as_mut() }) else {
        return ptr::null();
    };
    if raw.index.is_null() {
        raw.index = unsafe { ns_css_rule_index_build((&raw mut *raw).cast()) }.cast();
    }
    raw.index.cast()
}

fn bucket<'a>(
    table: *mut GHashTable,
    key: &mut Vec<u8>,
    name: &[u8],
    lower: bool,
) -> &'a [Candidate] {
    if table.is_null() {
        return &[];
    }
    key.clear();
    if lower {
        key.extend(name.iter().map(u8::to_ascii_lowercase));
    } else {
        key.extend_from_slice(name);
    }
    key.push(0);
    let found = unsafe { glib::g_hash_table_lookup(table, key.as_ptr().cast()) };
    unsafe { elements(found.cast::<GArray>()) }
}

fn collect_candidates(idx: &RawIndex, el: Option<Node<'_>>, scratch: &mut Scratch) {
    let Scratch {
        candidates, key, ..
    } = scratch;
    candidates.clear();
    if let Some(el) = el.filter(|e| e.is_element()) {
        if let Some(id) = attrs::get(el, c"id").filter(|id| !id.is_empty()) {
            candidates.extend_from_slice(bucket(idx.by_id, key, id.to_bytes(), false));
        }
        if let Some(cls) = attrs::get(el, c"class") {
            for token in class_tokens(cls.to_bytes()) {
                candidates.extend_from_slice(bucket(idx.by_class, key, token, false));
            }
        }
        if let Some(name) = el.name().filter(|n| !n.is_empty()) {
            candidates.extend_from_slice(bucket(idx.by_tag, key, name.to_bytes(), true));
        }
        if !idx.by_attr.is_null() && unsafe { glib::g_hash_table_size(idx.by_attr) } > 0 {
            for attr in el.attrs() {
                if let Some(name) = attr.name() {
                    candidates.extend_from_slice(bucket(idx.by_attr, key, name.to_bytes(), true));
                }
            }
        }
    }
    candidates.extend_from_slice(unsafe { elements(idx.universal) });
}

fn layer_rank(layer_ranks: *mut GHashTable, name: *const c_char) -> c_int {
    if name.is_null() || layer_ranks.is_null() {
        return LAYER_NONE;
    }
    let v = unsafe { glib::g_hash_table_lookup(layer_ranks, name.cast()) };
    if v.is_null() {
        LAYER_NONE
    } else {
        v as isize as c_int - 1
    }
}

struct Output<'a> {
    origin: c_int,
    sheet_index: c_int,
    dests: &'a [RawDest],
}

impl Output<'_> {
    fn emit(&self, rule: &RawRule, acc: &Accum, layer_order: c_int) {
        let rule_ptr = ptr::from_ref(rule).cast::<c_void>();
        for (dd, dst) in self.dests.iter().enumerate() {
            if !acc.any[dd] {
                continue;
            }
            let (spec_a, spec_b, spec_c) = acc.specificity[dd];
            let scope_order = acc.scope_order[dd];
            for (di, d) in unsafe { elements::<RawDecl>(rule.decls) }
                .iter()
                .enumerate()
            {
                let entry = RawMatch {
                    origin: self.origin,
                    spec_a,
                    spec_b,
                    spec_c,
                    sheet_index: self.sheet_index,
                    layer_order,
                    scope_order,
                    source_order: rule.source_order,
                    decl_order: di as c_int * DECL_SLOT_SPAN,
                    important: d.important,
                    inline_style: glib::FALSE,
                    rule: rule_ptr,
                    value: d.value,
                    prop: d.prop,
                };
                unsafe { append(dst.out, &entry) };
            }
            if !dst.var_out.is_null() && !rule.vars.is_null() {
                for (decl_order, (k, v)) in
                    unsafe { glib::hash_table_entries(rule.vars) }.enumerate()
                {
                    let important = !rule.var_important.is_null()
                        && unsafe { glib::g_hash_table_contains(rule.var_important, k) } != 0;
                    let entry = RawVarMatch {
                        origin: self.origin,
                        spec_a,
                        spec_b,
                        spec_c,
                        sheet_index: self.sheet_index,
                        layer_order,
                        scope_order,
                        source_order: rule.source_order,
                        decl_order: decl_order as c_int,
                        important: glib::boolean(important),
                        inline_style: glib::FALSE,
                        rule: rule_ptr,
                        name: k.cast(),
                        text: v.cast(),
                    };
                    unsafe { append(dst.var_out, &entry) };
                }
            }
            if !dst.pending_out.is_null() {
                for pd in unsafe { elements::<RawPending>(rule.pending) } {
                    let entry = RawPendingMatch {
                        origin: self.origin,
                        spec_a,
                        spec_b,
                        spec_c,
                        sheet_index: self.sheet_index,
                        layer_order,
                        scope_order,
                        source_order: rule.source_order,
                        decl_order_base: pd.decl_slot(),
                        inline_style: glib::FALSE,
                        rule: rule_ptr,
                        pd,
                    };
                    unsafe { append(dst.pending_out, &entry) };
                }
            }
        }
    }
}

struct Gather<'a> {
    rules: &'a [*mut RawRule],
    dests: &'a [RawDest],
    dest_of_pe: &'a [Option<usize>; DESTS],
    node: Option<Node<'a>>,
    filter: Option<&'a AncestorFilter>,
}

impl Gather<'_> {
    fn rejected(&self, rule: &RawRule, sel: &RawSelector) -> bool {
        let Some(filter) = self.filter else {
            return false;
        };
        if !unsafe { pointers::<c_void>(rule.scopes) }.is_empty() {
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
        idx: &RawIndex,
        cache: &mut Option<SelectorCache>,
        scratch: &mut Scratch,
        layer_ranks: *mut GHashTable,
        origin: c_int,
        sheet_index: c_int,
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
            let Some(rule) = self.rules.get(ri).and_then(|&r| unsafe { r.as_mut() }) else {
                continue;
            };
            let selectors = unsafe { pointers::<RawSelector>(rule.selectors) };
            let Some(&sel_ptr) = selectors.get(cand.selector_idx as usize) else {
                continue;
            };
            if !rule.container_condition.is_null()
                && unsafe {
                    ns_css_container_rule_matches(
                        rule.container_condition,
                        &raw mut rule.container_query,
                    )
                } == 0
            {
                continue;
            }
            let sel = unsafe { sel_ptr.as_ref() };
            if sel.is_some_and(|sel| self.rejected(rule, sel)) {
                continue;
            }
            let range = match sel {
                Some(sel) => match self.dest_of_pe.get(sel.pseudo_element as usize) {
                    Some(&Some(dd)) => dd..dd + 1,
                    _ => continue,
                },
                None => 0..self.dests.len(),
            };
            for dd in range {
                let pe = self.dests[dd].pe;
                if pe != 0 && rule.pe_mask & 1u32.checked_shl(pe).unwrap_or(0) == 0 {
                    continue;
                }
                if sel.is_some_and(|s| s.pseudo_element != pe) {
                    continue;
                }
                let (matched, scope_order) = cached_match(cache, rule, sel_ptr, self.node, pe);
                if !matched {
                    continue;
                }
                if !rule.container_condition.is_null() {
                    ns_css_container_features_note();
                }
                let acc = &mut scratch.accums[ri];
                if acc.epoch != epoch {
                    *acc = Accum {
                        epoch,
                        ..Accum::default()
                    };
                    scratch.matched.push(ri);
                }
                let spec = sel.map_or((0, 0, 0), |s| (s.spec_a, s.spec_b, s.spec_c));
                acc.note(dd, spec, scope_order);
            }
        }
        scratch.candidates = candidates;
        let output = Output {
            origin,
            sheet_index,
            dests: self.dests,
        };
        for &ri in &scratch.matched {
            let Some(rule) = self.rules.get(ri).and_then(|&r| unsafe { r.as_ref() }) else {
                continue;
            };
            let acc = &mut scratch.accums[ri];
            let layer_order = *acc
                .layer_order
                .get_or_insert_with(|| layer_rank(layer_ranks, rule.layer_name));
            output.emit(rule, acc, layer_order);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_gather_matches(
    sheet: *const RawSheet,
    origin: c_int,
    sheet_index: c_int,
    el: *const NsNode,
    dests: *const c_void,
    n_dests: c_uint,
    layer_ranks: *mut GHashTable,
) {
    let Some(raw_sheet) = (unsafe { sheet.as_ref() }) else {
        return;
    };
    let Some(idx) = (unsafe { ns_css_rule_index_ensure(sheet).as_ref() }) else {
        return;
    };
    let dests: &[RawDest] = if dests.is_null() || n_dests == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(dests.cast(), n_dests as usize) }
    };
    let node = unsafe { Node::from_ptr(el) };
    let rules = unsafe { pointers::<RawRule>(raw_sheet.rules) };
    let mut dest_of_pe = [None; DESTS];
    for (dd, dst) in dests.iter().enumerate() {
        if let Some(slot) = dest_of_pe.get_mut(dst.pe as usize) {
            *slot = Some(dd);
        }
    }
    FILTER.with(|f| {
        SELECTOR_CACHE.with(|c| {
            SCRATCH.with(|s| {
                let filter = f.borrow();
                let filter_usable =
                    filter.active && filter.subject == el as usize && match_scope().is_none();
                let mut cache = c.borrow_mut();
                let mut scratch = s.borrow_mut();
                let gather = Gather {
                    rules,
                    dests,
                    dest_of_pe: &dest_of_pe,
                    node,
                    filter: filter_usable.then_some(&*filter),
                };
                gather.run(
                    idx,
                    &mut cache,
                    &mut scratch,
                    layer_ranks,
                    origin,
                    sheet_index,
                );
            })
        })
    });
}
