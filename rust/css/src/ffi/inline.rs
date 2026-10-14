//! Southstar — the declarations inline style text reads back out of parsed style sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray};

use super::declarations::RawRule;
use super::property::prop_of;
use super::sheet::{RawSheet, ns_css_stylesheet_free, ns_css_stylesheet_parse};
use super::shorthand::RawDecl;
use super::values::text_of;
use crate::prop::Prop;

unsafe fn parse_sheet(text: &CStr) -> *mut RawSheet {
    unsafe { ns_css_stylesheet_parse(text.as_ptr(), -1) }.cast()
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
    let sheet = unsafe { parse_sheet(&text) };
    let mut out = Vec::new();
    if let Some(sheet_ref) = unsafe { sheet.as_ref() } {
        let rules = match unsafe { sheet_ref.rules.as_ref() } {
            Some(rules) if !rules.pdata.is_null() => unsafe {
                core::slice::from_raw_parts(
                    rules.pdata.cast::<*const RawRule>(),
                    rules.len as usize,
                )
            },
            _ => &[],
        };
        for &rule in rules {
            let Some(rule) = (unsafe { rule.as_ref() }) else {
                continue;
            };
            for decl in unsafe { decls_of(rule.decls) } {
                let prop = prop_of(decl.prop);
                if !wanted(prop) {
                    continue;
                }
                out.push(SheetDecl {
                    prop,
                    text: unsafe { text_of(decl.value, true) },
                    important: decl.important != 0,
                });
            }
        }
        unsafe { ns_css_stylesheet_free(sheet) };
    }
    out
}

pub(crate) fn first_rule_declares(css: &[u8]) -> bool {
    let text = c_string(css);
    let sheet = unsafe { parse_sheet(&text) };
    let Some(sheet_ref) = (unsafe { sheet.as_ref() }) else {
        return false;
    };
    let declares = unsafe {
        let first = sheet_ref
            .rules
            .as_ref()
            .filter(|rules| rules.len > 0 && !rules.pdata.is_null())
            .and_then(|rules| (*rules.pdata).cast::<RawRule>().as_ref());
        first.is_some_and(|rule| {
            rule.decls.as_ref().is_some_and(|d| d.len > 0)
                || (!rule.vars.is_null() && glib::g_hash_table_size(rule.vars) > 0)
                || rule.pending.as_ref().is_some_and(|p| p.len > 0)
        })
    };
    unsafe { ns_css_stylesheet_free(sheet) };
    declares
}
