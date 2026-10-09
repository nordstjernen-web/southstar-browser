//! Southstar — the C ABI of pending declarations: css.c's pending_match entries read through a mirror, each var()- or attr()-dependent value substituted and parsed into match entries the cascade sorts, or unset when it cannot stand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::mem::size_of;
use core::slice;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::cascade::RawMatch;
use super::declarations::{RawPending, parse_decls_into};
use super::shorthand::RawDecl;
use super::vars::{RawVarMap, substitute_in};
use crate::pending::{self, Pending};

#[repr(C)]
pub(super) struct RawPendingMatch {
    pub(super) origin: c_int,
    pub(super) spec_a: c_int,
    pub(super) spec_b: c_int,
    pub(super) spec_c: c_int,
    pub(super) sheet_index: c_int,
    pub(super) layer_order: c_int,
    pub(super) scope_order: c_int,
    pub(super) source_order: c_int,
    pub(super) decl_order_base: c_int,
    pub(super) inline_style: GBoolean,
    pub(super) rule: *const c_void,
    pub(super) pd: *const RawPending,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawPendingMatch>() == 56);

unsafe extern "C" {
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

unsafe fn entries<'a>(array: *const GArray) -> &'a [RawPendingMatch] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.data.cast::<RawPendingMatch>(), a.len as usize)
        },
        _ => &[],
    }
}

fn append_decls(
    pm: &RawPendingMatch,
    pd: &RawPending,
    name: &[u8],
    value: &[u8],
    matches: *mut GArray,
    owned: *mut GPtrArray,
) -> bool {
    let mut synth = Vec::with_capacity(name.len() + value.len() + 4);
    synth.extend_from_slice(name);
    synth.extend_from_slice(b": ");
    synth.extend_from_slice(value);
    synth.extend_from_slice(b";}");
    let temp =
        unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, size_of::<RawDecl>() as c_uint) };
    parse_decls_into(&synth, temp);
    let decls: &[RawDecl] = unsafe {
        match temp.as_ref() {
            Some(a) if a.len > 0 => slice::from_raw_parts(a.data.cast(), a.len as usize),
            _ => &[],
        }
    };
    let mut any = false;
    for d in decls.iter().filter(|d| !d.value.is_null()) {
        unsafe { glib::g_ptr_array_add(owned, d.value.cast()) };
        let entry = RawMatch {
            origin: pm.origin,
            spec_a: pm.spec_a,
            spec_b: pm.spec_b,
            spec_c: pm.spec_c,
            sheet_index: pm.sheet_index,
            layer_order: pm.layer_order,
            scope_order: pm.scope_order,
            source_order: pm.source_order,
            decl_order: pm.decl_order_base,
            important: glib::boolean(pd.important() || d.important != 0),
            inline_style: pm.inline_style,
            rule: pm.rule,
            value: d.value,
            prop: d.prop,
        };
        unsafe { glib::g_array_append_vals(matches, (&raw const entry).cast(), 1) };
        any = true;
    }
    unsafe { g_array_free(temp, glib::TRUE) };
    any
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_resolve_pending(
    pending_matches: *const GArray,
    vars: *const RawVarMap,
    registered: *mut GHashTable,
    matches: *mut GArray,
    owned_values: *mut GPtrArray,
    node: *const NsNode,
) {
    let raw = unsafe { entries(pending_matches) };
    if raw.is_empty() {
        return;
    }
    let list: Vec<Pending> = raw
        .iter()
        .map(|m| Pending {
            origin: m.origin,
            specificity: (m.spec_a, m.spec_b, m.spec_c),
            sheet_index: m.sheet_index,
            layer_order: m.layer_order,
            scope_order: m.scope_order,
            source_order: m.source_order,
            important: unsafe { m.pd.as_ref() }.is_some_and(RawPending::important),
            inline_style: m.inline_style != 0,
        })
        .collect();
    let node = unsafe { Node::from_ptr(node) };
    for i in pending::order(&list) {
        let pm = &raw[i];
        let Some(pd) = (unsafe { pm.pd.as_ref() }) else {
            continue;
        };
        let (Some(name), Some(text)) = (pd.name(), pd.raw_text()) else {
            continue;
        };
        let custom = pending::is_custom(name);
        let substituted =
            pending::substituted_value(text, custom, |t| substitute_in(t, vars, registered), node);
        let applied = substituted
            .is_some_and(|value| append_decls(pm, pd, name, &value, matches, owned_values));
        if !applied && !custom {
            append_decls(pm, pd, name, b"unset", matches, owned_values);
        }
    }
}
