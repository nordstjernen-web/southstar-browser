//! Southstar — a declaration's longhands as owned values, sharing one value where css.c shares it, and the property-name lookup expansion makes into css.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use super::property::{id_of, to_c};
use super::value::OwnedValue;
use crate::prop::Prop;
use crate::shorthand::{self, Slot};

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
