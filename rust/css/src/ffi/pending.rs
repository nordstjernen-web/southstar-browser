//! Southstar — the C ABI of pending declarations: css.c's pending_match entries read through a mirror, each var()- or attr()-dependent value substituted and parsed into match entries the cascade sorts, or unset when it cannot stand.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use std::rc::Rc;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean};

use super::cascade::RawMatch;
use super::declarations::{Declarations, PendingDecl};
use super::registry::RegisteredProps;
use super::vars::{VarMap, substitute_in};
use crate::pending::{self, Pending};

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
    pub(super) pd: Rc<PendingDecl>,
}

fn append_decls(
    pm: &RawPendingMatch,
    pd: &PendingDecl,
    name: &[u8],
    value: &[u8],
    matches: &mut Vec<RawMatch>,
) -> bool {
    let mut synth = Vec::with_capacity(name.len() + value.len() + 4);
    synth.extend_from_slice(name);
    synth.extend_from_slice(b": ");
    synth.extend_from_slice(value);
    synth.extend_from_slice(b";}");
    let mut any = false;
    for d in Declarations::parse(&synth).into_vec() {
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
            important: glib::boolean(pd.important() || d.important()),
            inline_style: pm.inline_style,
            rule: pm.rule,
            prop: d.prop(),
            value: d.into_value(),
        };
        matches.push(entry);
        any = true;
    }
    any
}

pub(super) fn resolve_pending(
    raw: &[RawPendingMatch],
    vars: Option<&VarMap>,
    registered: &RegisteredProps,
    matches: &mut Vec<RawMatch>,
    node: Node<'_>,
) {
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
            important: m.pd.important(),
            inline_style: m.inline_style != 0,
        })
        .collect();
    let node = Some(node);
    for i in pending::order(&list) {
        let pm = &raw[i];
        let pd = &*pm.pd;
        let (Some(name), Some(text)) = (pd.name(), pd.raw_text()) else {
            continue;
        };
        let custom = pending::is_custom(name);
        let substituted = pending::substituted_value(
            text,
            custom,
            |t| substitute_in(t, vars, Some(registered)),
            node,
        );
        let applied = substituted.is_some_and(|value| append_decls(pm, pd, name, &value, matches));
        if !applied && !custom {
            append_decls(pm, pd, name, b"unset", matches);
        }
    }
}
