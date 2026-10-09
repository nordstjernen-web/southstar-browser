//! Southstar — the C ABI of the inline style text, and the declarations it reads back out of parsed style sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean};

use super::declarations::RawRule;
use super::property::prop_of;
use super::sheet::{RawSheet, ns_css_stylesheet_free, ns_css_stylesheet_parse};
use super::shorthand::RawDecl;
use super::values::text_of;
use crate::inline;
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

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_style_get(
    style: *const c_char,
    prop: *const c_char,
) -> *mut c_char {
    let (Some(style), Some(prop)) = (unsafe { bytes(style) }, unsafe { bytes(prop) }) else {
        return ptr::null_mut();
    };
    owned(inline::get(style, prop))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_style_set(
    style: *const c_char,
    prop: *const c_char,
    value: *const c_char,
) -> *mut c_char {
    let style = unsafe { bytes(style) };
    let Some(prop) = (unsafe { bytes(prop) }) else {
        return glib::strdup(style.unwrap_or_default());
    };
    glib::strdup(&inline::set(style, prop, unsafe { bytes(value) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_style_serialize(style: *const c_char) -> *mut c_char {
    glib::strdup(&inline::serialize(
        unsafe { bytes(style) }.unwrap_or_default(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_value_strip_important(value: *mut c_char) -> GBoolean {
    let Some(text) = (unsafe { bytes(value) }) else {
        return glib::FALSE;
    };
    let (kept, important) = crate::scan::strip_important(text);
    if important {
        unsafe { *value.add(kept.len()) = 0 };
    }
    glib::boolean(important)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_background_position_join(
    xs: *const c_char,
    ys: *const c_char,
) -> *mut c_char {
    let xs = unsafe { bytes(xs) }.unwrap_or_default();
    let ys = unsafe { bytes(ys) }.unwrap_or_default();
    owned(inline::background_position_zip(xs, ys))
}
