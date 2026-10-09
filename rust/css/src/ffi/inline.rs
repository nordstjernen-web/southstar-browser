//! Southstar — the C ABI of the inline style text, and the css.c stylesheet parser, declaration checks and value serialization it still calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean, GPtrArray};

use super::property::prop_of;
use super::shorthand::RawDecl;
use super::value::NsCssValue;
use crate::inline;
use crate::prop::Prop;

#[repr(C)]
struct RawSheet {
    rules: *mut GPtrArray,
}

#[repr(C)]
struct RawRule {
    selectors: *mut GPtrArray,
    decls: *mut GArray,
}

unsafe extern "C" {
    fn ns_css_stylesheet_parse(text: *const c_char, len: isize) -> *mut RawSheet;
    fn ns_css_stylesheet_free(sheet: *mut RawSheet);
    fn ns_css_value_serialize(v: *const NsCssValue) -> *mut c_char;
    fn ns_css_value_serialize_specified(v: *const NsCssValue) -> *mut c_char;
    fn ns_css_parse_declarations(text: *const c_char) -> *mut GArray;
    fn ns_css_declarations_free(decls: *mut GArray);
    fn ns_css_named_property_supported(name: *const c_char) -> GBoolean;
    fn ns_css_named_declaration_valid(name: *const c_char, text: *const c_char) -> GBoolean;
    fn ns_css_specified_canonical(prop: *const c_char, value: *const c_char) -> *mut c_char;
}

pub(crate) struct SheetDecl {
    pub prop: Option<Prop>,
    pub text: Vec<u8>,
    pub important: bool,
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

fn take(s: *mut c_char) -> Option<Vec<u8>> {
    unsafe { glib::GStr::take(s) }.map(|s| s.to_bytes().to_vec())
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
    let sheet = unsafe { ns_css_stylesheet_parse(text.as_ptr(), -1) };
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
                    text: take(unsafe { ns_css_value_serialize_specified(decl.value) })
                        .unwrap_or_default(),
                    important: decl.important != 0,
                });
            }
        }
        unsafe { ns_css_stylesheet_free(sheet) };
    }
    out
}

pub(crate) fn declarations_serialized(text: &[u8]) -> Vec<(Option<Prop>, Vec<u8>)> {
    let text = c_string(text);
    let decls = unsafe { ns_css_parse_declarations(text.as_ptr()) };
    let out = unsafe { decls_of(decls) }
        .iter()
        .map(|decl| {
            let serialized = take(unsafe { ns_css_value_serialize(decl.value) });
            (prop_of(decl.prop), serialized.unwrap_or_default())
        })
        .collect();
    unsafe { ns_css_declarations_free(decls) };
    out
}

pub(crate) fn named_property_supported(name: &[u8]) -> bool {
    let name = c_string(name);
    unsafe { ns_css_named_property_supported(name.as_ptr()) != 0 }
}

pub(crate) fn named_declaration_valid(name: &[u8], text: &[u8]) -> bool {
    let (name, text) = (c_string(name), c_string(text));
    unsafe { ns_css_named_declaration_valid(name.as_ptr(), text.as_ptr()) != 0 }
}

pub(crate) fn specified_canonical(prop: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    let (prop, value) = (c_string(prop), c_string(value));
    take(unsafe { ns_css_specified_canonical(prop.as_ptr(), value.as_ptr()) })
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
