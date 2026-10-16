//! Southstar — a declaration's longhands as owned values or appended to css.c's array of ns_css_decl, sharing one value where css.c shares it, and the property-name lookup expansion makes into css.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint};

use southstar_glib::{self as glib, GArray, GBoolean};

use super::property::{id_of, to_c};
use super::value::{NsCssValue, OwnedValue};
use crate::prop::Prop;
use crate::shorthand::{self, Slot};

#[repr(C)]
pub(super) struct RawDecl {
    pub(super) prop: c_int,
    pub(super) value: *mut NsCssValue,
    pub(super) important: GBoolean,
}

const _: () = assert!(core::mem::size_of::<RawDecl>() == 24);

pub(crate) fn prop_named(name: &[u8]) -> Option<Prop> {
    Prop::from_name(name)
}

pub(super) fn expanded(name: &[u8], text: &[u8]) -> Vec<(c_int, OwnedValue)> {
    let mut out: Vec<(c_int, OwnedValue)> = Vec::new();
    for decl in shorthand::expand(name, text) {
        let value = match decl.slot {
            Slot::Own(value) => OwnedValue(to_c(value)),
            Slot::Dup(of) => out[of].1.clone(),
        };
        out.push((id_of(decl.prop), value));
    }
    out
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
    let raw: Vec<RawDecl> = expanded(name, text)
        .into_iter()
        .map(|(prop, value)| RawDecl {
            prop,
            value: value.into_raw(),
            important: glib::boolean(important),
        })
        .collect();
    if !raw.is_empty() {
        unsafe { glib::g_array_append_vals(decls, raw.as_ptr().cast(), raw.len() as c_uint) };
    }
}
