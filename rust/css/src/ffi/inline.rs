//! Southstar — the declarations inline style text reads back out of parsed style sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray};

use super::property::prop_of;
use super::sheet::OwnedSheet;
use super::shorthand::RawDecl;
use super::values::text_of;
use crate::prop::Prop;

fn parse_sheet(text: &CStr) -> OwnedSheet {
    OwnedSheet::parse(text.to_bytes())
}

pub(crate) struct SheetDecl {
    pub prop: Option<Prop>,
    pub text: Vec<u8>,
    pub important: bool,
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

unsafe fn decls_of<'a>(array: *const GArray) -> &'a [RawDecl] {
    match unsafe { array.as_ref() } {
        Some(array) if !array.data.is_null() => unsafe {
            core::slice::from_raw_parts(array.data.cast::<RawDecl>(), array.len as usize)
        },
        _ => &[],
    }
}

pub(crate) fn sheet_declarations(
    css: &[u8],
    wanted: impl Fn(Option<Prop>) -> bool,
) -> Vec<SheetDecl> {
    let text = c_string(css);
    let sheet = parse_sheet(&text);
    let mut out = Vec::new();
    for &rule in sheet.rules() {
        // SAFETY: a sheet's rules are null or live RawRules freed with the sheet.
        let Some(rule) = (unsafe { rule.as_ref() }) else {
            continue;
        };
        // SAFETY: decls is null or the rule's array of RawDecl.
        for decl in unsafe { decls_of(rule.decls) } {
            let prop = prop_of(decl.prop);
            if !wanted(prop) {
                continue;
            }
            out.push(SheetDecl {
                prop,
                // SAFETY: a declaration's value is null or a live value the rule owns.
                text: unsafe { text_of(decl.value, true) },
                important: decl.important != 0,
            });
        }
    }
    out
}

pub(crate) fn first_rule_declares(css: &[u8]) -> bool {
    let text = c_string(css);
    let sheet = parse_sheet(&text);
    let Some(&first) = sheet.rules().first() else {
        return false;
    };
    // SAFETY: a sheet's rules are null or live RawRules freed with the sheet.
    let Some(rule) = (unsafe { first.as_ref() }) else {
        return false;
    };
    // SAFETY: decls is null or the rule's live GArray.
    let has_decls = unsafe { rule.decls.as_ref() }.is_some_and(|d| d.len > 0);
    has_decls || {
        // SAFETY: vars is the rule's live hash table when it is not null.
        let has_vars = !rule.vars.is_null() && unsafe { glib::g_hash_table_size(rule.vars) } > 0;
        // SAFETY: pending is null or the rule's live GArray.
        has_vars || unsafe { rule.pending.as_ref() }.is_some_and(|p| p.len > 0)
    }
}
