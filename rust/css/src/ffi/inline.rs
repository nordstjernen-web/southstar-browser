//! Southstar — the declarations inline style text reads back out of parsed style sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use super::property::prop_of;
use super::sheet::OwnedSheet;
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

pub(crate) fn sheet_declarations(
    css: &[u8],
    wanted: impl Fn(Option<Prop>) -> bool,
) -> Vec<SheetDecl> {
    let text = c_string(css);
    let sheet = parse_sheet(&text);
    let mut out = Vec::new();
    for rule in sheet.rules() {
        for decl in &rule.decls {
            let prop = prop_of(decl.prop());
            if !wanted(prop) {
                continue;
            }
            out.push(SheetDecl {
                prop,
                // SAFETY: a declaration's value is a live value the rule holds a reference on.
                text: unsafe { text_of(decl.value(), true) },
                important: decl.important(),
            });
        }
    }
    out
}

pub(crate) fn first_rule_declares(css: &[u8]) -> bool {
    let text = c_string(css);
    let sheet = parse_sheet(&text);
    let Some(rule) = sheet.rules().first() else {
        return false;
    };
    !rule.decls.is_empty()
        || rule.vars.as_ref().is_some_and(|vars| vars.len() > 0)
        || !rule.pending.is_empty()
}
