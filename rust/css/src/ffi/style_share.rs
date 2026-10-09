//! Southstar — the C ABI of style sharing: the share key built from css.c's match arrays for an element and its pseudo-elements, looked up and stored in the per-pass share table.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_char, c_int, c_uint};
use core::ptr;
use core::slice;

use southstar_glib::{self as glib, GArray, GBoolean};

use super::cascade::RawMatch;
use super::computed_units::RawStyle;
use super::container::ns_css_container_stack_copy;
use super::custom_props::RawVarMatch;
use super::gather::RawDest;
use super::pending::RawPendingMatch;
use super::value::{KIND_LENGTH, NsCssValue};
use crate::nesting;
use crate::prop::Prop;
use crate::style_share::{self, ShareTable};
use crate::units::{CQH, CQMAX, CQMIN, CQW};

const CONTAINER_BYTES: usize = 40;

thread_local! {
    static TABLE: RefCell<ShareTable> = RefCell::new(ShareTable::default());
}

unsafe fn elements<'a, T>(array: *const GArray) -> &'a [T] {
    match unsafe { array.as_ref() } {
        Some(array) if !array.data.is_null() && array.len > 0 => unsafe {
            slice::from_raw_parts(array.data.cast::<T>(), array.len as usize)
        },
        _ => &[],
    }
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
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

fn container_length(value: *const NsCssValue) -> bool {
    unsafe { value.as_ref() }.is_some_and(|v| {
        v.kind == KIND_LENGTH && matches!(unsafe { v.u.length.unit }, CQW | CQH | CQMIN | CQMAX)
    })
}

struct Arrays<'a> {
    pe: c_uint,
    matches: &'a [RawMatch],
    vars: &'a [RawVarMatch],
    pending: &'a [RawPendingMatch],
}

impl<'a> Arrays<'a> {
    fn of(dest: &'a RawDest) -> Self {
        unsafe {
            Arrays {
                pe: dest.pe,
                matches: elements(dest.out),
                vars: elements(dest.var_out),
                pending: elements(dest.pending_out),
            }
        }
    }

    fn pending_texts(&self) -> impl Iterator<Item = &'a [u8]> + use<'a> {
        self.pending
            .iter()
            .filter_map(|e| unsafe { e.pd.as_ref() })
            .filter_map(|pd| text(pd.raw_vtext))
    }

    fn uses_attr(&self) -> bool {
        self.pending_texts().any(style_share::uses_attr)
    }

    fn needs_container(&self) -> bool {
        let font_size = Prop::FontSize.id() as c_int;
        self.matches
            .iter()
            .any(|e| e.prop == font_size && container_length(e.value))
            || self
                .vars
                .iter()
                .any(|e| text(e.text).is_some_and(nesting::has_container_units))
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
            put_ptr(key, e.value);
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
            put_ptr(key, e.name);
            put_ptr(key, e.text);
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
            put_ptr(key, e.pd);
        }
    }
}

fn arrays(dests: &[RawDest]) -> impl Iterator<Item = Arrays<'_>> + Clone {
    dests.iter().map(Arrays::of)
}

fn build_key(key: &mut Vec<u8>, parent_id: u64, root_px: f64, dests: &[RawDest]) {
    let mut cq_bytes = unsafe { ns_css_container_stack_copy(ptr::null_mut(), 0) };
    if cq_bytes > 0 && !arrays(dests).any(|a| a.needs_container()) {
        cq_bytes = 0;
    }
    let cq_len = (cq_bytes / CONTAINER_BYTES) as u32;
    key.extend_from_slice(&parent_id.to_ne_bytes());
    key.extend_from_slice(&root_px.to_ne_bytes());
    key.extend_from_slice(&cq_len.to_ne_bytes());
    if cq_len > 0 {
        let start = key.len();
        key.resize(start + cq_bytes, 0);
        unsafe { ns_css_container_stack_copy(key[start..].as_mut_ptr(), cq_bytes) };
    }
    for (i, a) in arrays(dests).enumerate() {
        if i > 0 {
            key.extend_from_slice(&a.pe.to_ne_bytes());
        }
        a.put(key);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_share_begin() {
    TABLE.with_borrow_mut(ShareTable::begin);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_share_end() {
    TABLE.with_borrow_mut(ShareTable::end);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_style_share_find(
    parent: *const RawStyle,
    root_px: f64,
    dests: *const RawDest,
    n_dests: c_uint,
    shared: *mut *const RawStyle,
) -> GBoolean {
    let Some(shared) = (unsafe { shared.as_mut() }) else {
        return glib::FALSE;
    };
    *shared = ptr::null();
    let dests = if dests.is_null() {
        &[][..]
    } else {
        unsafe { slice::from_raw_parts(dests, n_dests as usize) }
    };
    TABLE.with_borrow_mut(|table| {
        if !table.active() {
            return glib::TRUE;
        }
        if arrays(dests).any(|a| a.uses_attr()) {
            return glib::FALSE;
        }
        let parent_id = unsafe { parent.as_ref() }.map_or(0, |p| p.share_id);
        build_key(table.new_key(), parent_id, root_px, dests);
        *shared = table
            .find()
            .map_or(ptr::null(), |style| style as *const RawStyle);
        glib::TRUE
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_share_insert(style: *const RawStyle) {
    TABLE.with_borrow_mut(|table| table.insert(style as usize));
}
