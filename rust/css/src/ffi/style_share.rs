//! Southstar — the C ABI of style sharing: the share key built from css.c's match arrays for an element and its pseudo-elements, looked up and stored in the per-pass share table.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_uint};
use core::ptr;
use std::rc::Rc;

use super::cascade::RawMatch;
use super::computed_units::RawStyle;
use super::container::Container;
use super::custom_props::RawVarMatch;
use super::gather::Matches;
use super::pending::RawPendingMatch;
use super::style_alloc::{NewStyle, RetainedStyle, style_clone_shared};
use super::value::NsCssValue;
use crate::container;
use crate::nesting;
use crate::prop::Prop;
use crate::style_share::{self, ShareTable};
use crate::units::{CQH, CQMAX, CQMIN, CQW};

thread_local! {
    static TABLE: RefCell<ShareTable<RetainedStyle>> = RefCell::new(ShareTable::default());
}

fn put_ints(key: &mut Vec<u8>, ints: &[c_int]) {
    for i in ints {
        key.extend_from_slice(&i.to_ne_bytes());
    }
}

fn put_ptr<T>(key: &mut Vec<u8>, p: *const T) {
    key.extend_from_slice(&(p as usize).to_ne_bytes());
}

fn put_len<T>(key: &mut Vec<u8>, items: &[T]) {
    key.extend_from_slice(&(items.len() as u32).to_ne_bytes());
}

fn container_length(value: &NsCssValue) -> bool {
    value
        .length()
        .is_some_and(|(_, unit)| matches!(unit, CQW | CQH | CQMIN | CQMAX))
}

struct Arrays<'a> {
    pe: c_uint,
    matches: &'a [RawMatch],
    vars: &'a [RawVarMatch],
    pending: &'a [RawPendingMatch],
}

impl<'a> Arrays<'a> {
    fn of(pe: c_uint, matches: &'a Matches) -> Self {
        Arrays {
            pe,
            matches: &matches.decls,
            vars: &matches.vars,
            pending: &matches.pending,
        }
    }

    fn pending_texts(&self) -> impl Iterator<Item = &'a [u8]> + use<'a> {
        self.pending.iter().filter_map(|e| e.pd.raw_text())
    }

    fn uses_attr(&self) -> bool {
        self.pending_texts().any(style_share::uses_attr)
    }

    fn needs_container(&self) -> bool {
        let font_size = Prop::FontSize.id() as c_int;
        self.matches
            .iter()
            .any(|e| e.prop == font_size && container_length(&e.value))
            || self
                .vars
                .iter()
                .any(|e| nesting::has_container_units(e.text.to_bytes()))
            || self.pending_texts().any(nesting::has_container_units)
    }

    fn put(&self, key: &mut Vec<u8>) {
        put_len(key, self.matches);
        for e in self.matches {
            put_ints(
                key,
                &[
                    e.origin,
                    e.spec_a,
                    e.spec_b,
                    e.spec_c,
                    e.sheet_index,
                    e.layer_order,
                    e.scope_order,
                    e.source_order,
                    e.decl_order,
                    e.important,
                    e.inline_style,
                ],
            );
            put_ptr(key, e.rule);
            put_ptr(key, e.value.as_ptr());
            put_ints(key, &[e.prop]);
        }
        put_len(key, self.vars);
        for e in self.vars {
            put_ints(
                key,
                &[
                    e.origin,
                    e.spec_a,
                    e.spec_b,
                    e.spec_c,
                    e.sheet_index,
                    e.layer_order,
                    e.scope_order,
                    e.source_order,
                    e.decl_order,
                    e.important,
                    e.inline_style,
                ],
            );
            put_ptr(key, e.rule);
            put_ptr(key, e.name.as_ptr());
            put_ptr(key, e.text.as_ptr());
        }
        put_len(key, self.pending);
        for e in self.pending {
            put_ints(
                key,
                &[
                    e.origin,
                    e.spec_a,
                    e.spec_b,
                    e.spec_c,
                    e.sheet_index,
                    e.layer_order,
                    e.scope_order,
                    e.source_order,
                    e.decl_order_base,
                    e.inline_style,
                ],
            );
            put_ptr(key, e.rule);
            put_ptr(key, Rc::as_ptr(&e.pd));
        }
    }
}

fn put_container(key: &mut Vec<u8>, c: &Container) {
    put_ptr(key, c.names().map_or(ptr::null(), CStr::as_ptr));
    key.extend_from_slice(&c.width.to_ne_bytes());
    key.extend_from_slice(&c.height.to_ne_bytes());
    put_ints(key, &[c.kind, c.vertical, c.sibling_index, c.sibling_count]);
}

fn arrays<'a>(pes: &'a [c_uint], outs: &'a [Matches]) -> impl Iterator<Item = Arrays<'a>> + Clone {
    pes.iter()
        .zip(outs)
        .map(|(&pe, matches)| Arrays::of(pe, matches))
}

fn build_key(key: &mut Vec<u8>, parent_id: u64, root_px: f64, pes: &[c_uint], outs: &[Matches]) {
    let needs_container = arrays(pes, outs).any(|a| a.needs_container());
    key.extend_from_slice(&parent_id.to_ne_bytes());
    key.extend_from_slice(&root_px.to_ne_bytes());
    container::with_stack(|stack| {
        let stack = if needs_container { stack } else { &[] };
        key.extend_from_slice(&(stack.len() as u32).to_ne_bytes());
        for c in stack {
            put_container(key, c);
        }
    });
    for (i, a) in arrays(pes, outs).enumerate() {
        if i > 0 {
            key.extend_from_slice(&a.pe.to_ne_bytes());
        }
        a.put(key);
    }
}

pub(super) fn style_share_begin() {
    TABLE.with_borrow_mut(ShareTable::begin);
}

pub(super) fn style_share_end() {
    TABLE.with_borrow_mut(ShareTable::end);
}

pub(super) enum Shared {
    Unkeyed,
    Miss,
    Hit(NewStyle),
}

pub(super) fn style_share_find(
    parent: Option<&RawStyle>,
    root_px: f64,
    pes: &[c_uint],
    outs: &[Matches],
) -> Shared {
    TABLE.with_borrow_mut(|table| {
        if !table.active() {
            return Shared::Miss;
        }
        if arrays(pes, outs).any(|a| a.uses_attr()) {
            return Shared::Unkeyed;
        }
        let parent_id = parent.map_or(0, |p| p.share_id);
        build_key(table.new_key(), parent_id, root_px, pes, outs);
        match table.find() {
            Some(style) => Shared::Hit(style_clone_shared(style.get())),
            None => Shared::Miss,
        }
    })
}

pub(super) fn style_share_insert(style: &RetainedStyle) {
    TABLE.with_borrow_mut(|table| table.insert(style.clone()));
}
