//! Southstar — a declaration's longhands appended to css.c's array of ns_css_decl, sharing one value where css.c shares it, and the property-name lookup expansion makes into css.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint};
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean};

use super::property::{id_of, prop_of, to_c};
use super::value::NsCssValue;
use crate::prop::Prop;
use crate::shorthand::{self, Slot};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_css_value_dup(v: *const NsCssValue) -> *mut NsCssValue;
}

#[repr(C)]
pub(super) struct RawDecl {
    pub(super) prop: c_int,
    pub(super) value: *mut NsCssValue,
    pub(super) important: GBoolean,
}

const _: () = assert!(core::mem::size_of::<RawDecl>() == 24);

pub(crate) fn prop_named(name: &[u8]) -> Option<Prop> {
    let name = CString::new(name).ok()?;
    prop_of(unsafe { ns_css_prop_id(name.as_ptr()) })
}

pub(super) unsafe fn append_expanded(
    name: &[u8],
    text: &[u8],
    important: bool,
    decls: *mut GArray,
) {
    if decls.is_null() {
        return;
    }
    let mut raw: Vec<RawDecl> = Vec::new();
    for decl in shorthand::expand(name, text) {
        let value = match decl.slot {
            Slot::Own(value) => to_c(value),
            Slot::Dup(of) => unsafe { ns_css_value_dup(raw[of].value) },
        };
        raw.push(RawDecl {
            prop: id_of(decl.prop),
            value,
            important: glib::boolean(important),
        });
    }
    if !raw.is_empty() {
        unsafe { glib::g_array_append_vals(decls, raw.as_ptr().cast(), raw.len() as c_uint) };
    }
}
