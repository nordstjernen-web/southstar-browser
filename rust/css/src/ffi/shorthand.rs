//! Southstar — the C ABI of declaration expansion: a declaration's longhands appended to css.c's array of ns_css_decl, sharing one value where css.c shares it, and the property-name and list-style lookups expansion makes into css.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean};

use super::property::{id_of, prop_of, to_c};
use super::value::NsCssValue;
use crate::prop::Prop;
use crate::shorthand::{self, Slot};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_value_dup(v: *const NsCssValue) -> *mut NsCssValue;
    fn ns_css_list_style_split(
        text: *const c_char,
        out_type: *mut *mut c_char,
        out_position: *mut *mut c_char,
        out_image: *mut *mut c_char,
    ) -> GBoolean;
}

#[repr(C)]
struct RawDecl {
    prop: c_int,
    value: *mut NsCssValue,
    important: GBoolean,
}

const _: () = assert!(core::mem::size_of::<RawDecl>() == 24);

pub(crate) fn prop_named(name: &[u8]) -> Option<Prop> {
    let name = CString::new(name).ok()?;
    prop_of(unsafe { ns_css_prop_id(name.as_ptr()) })
}

pub(crate) fn list_style_split(text: &[u8]) -> Option<[Vec<u8>; 3]> {
    let text = CString::new(text).ok()?;
    let mut parts = [ptr::null_mut(); 3];
    let [kind, position, image] = &mut parts;
    let ok = unsafe { ns_css_list_style_split(text.as_ptr(), kind, position, image) } != 0;
    let owned = parts.map(|part| {
        unsafe { glib::GStr::take(part) }
            .map(|s| s.to_bytes().to_vec())
            .unwrap_or_default()
    });
    ok.then_some(owned)
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_expand_declaration(
    name: *const c_char,
    text: *const c_char,
    important: GBoolean,
    decls: *mut GArray,
) {
    let (Some(name), Some(text), false) = (
        unsafe { bytes(name) },
        unsafe { bytes(text) },
        decls.is_null(),
    ) else {
        return;
    };
    let mut raw: Vec<RawDecl> = Vec::new();
    for decl in shorthand::expand(name, text) {
        let value = match decl.slot {
            Slot::Own(value) => to_c(value),
            Slot::Dup(of) => unsafe { ns_css_value_dup(raw[of].value) },
        };
        raw.push(RawDecl {
            prop: id_of(decl.prop),
            value,
            important,
        });
    }
    if !raw.is_empty() {
        unsafe { glib::g_array_append_vals(decls, raw.as_ptr().cast(), raw.len() as c_uint) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_radius_canonical(value: *const c_char) -> *mut c_char {
    match unsafe { bytes(value) }.and_then(shorthand::border_radius_canonical) {
        Some(canon) => glib::strdup(&canon),
        None => ptr::null_mut(),
    }
}
