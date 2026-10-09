//! Southstar — the C ABI of composing grid placements and the grid shorthands from their longhands.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_glib::{self as glib, GBoolean};

use crate::grid;

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn values<'a, const N: usize>(v: *const *mut c_char) -> Option<[&'a [u8]; N]> {
    if v.is_null() {
        return None;
    }
    let mut out = [&b""[..]; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = unsafe { bytes(*v.add(i)) }?;
    }
    Some(out)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_placement_compose(
    v: *const *mut c_char,
    area: GBoolean,
) -> *mut c_char {
    let parts = if area != 0 {
        unsafe { values::<4>(v) }.map(|v| v.to_vec())
    } else {
        unsafe { values::<2>(v) }.map(|v| v.to_vec())
    };
    let text = parts
        .map(|parts| grid::placement_compose(&parts, area != 0))
        .unwrap_or_default();
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_shorthand_compose(
    v: *const *mut c_char,
    full: GBoolean,
) -> *mut c_char {
    let text = if full != 0 {
        unsafe { values::<6>(v) }
            .filter(|v| v.iter().all(|part| !part.is_empty()))
            .map(|v| grid::grid_compose(&v))
    } else {
        unsafe { values::<3>(v) }
            .filter(|v| v.iter().all(|part| !part.is_empty()))
            .map(|[rows, cols, areas]| grid::template_compose(rows, cols, areas))
    };
    glib::strdup(&text.unwrap_or_default())
}
