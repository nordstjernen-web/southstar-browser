//! Southstar — the C ABI of computed-style queries: whether a property of css.c's ns_style took its colour from currentcolor.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_glib::{self as glib, GBoolean};

use super::computed_units::RawStyle;
use crate::cascade::CURRENTCOLOR_PROPS;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_prop_from_currentcolor(
    style: *const RawStyle,
    prop: c_int,
) -> GBoolean {
    let Some(style) = (unsafe { style.as_ref() }) else {
        return glib::FALSE;
    };
    let bit = CURRENTCOLOR_PROPS
        .iter()
        .position(|p| p.id() as c_int == prop);
    glib::boolean(bit.is_some_and(|i| (style.currentcolor_bits >> i) & 1 != 0))
}
