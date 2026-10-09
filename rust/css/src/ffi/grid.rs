//! Southstar — the C ABI of grid track lists, template areas, lines, placements and the grid shorthands.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::value::{self, NsCssValue};
use crate::grid;

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
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

unsafe fn store(out: *mut *mut c_char, parts: &[Vec<u8>]) {
    for (i, part) in parts.iter().enumerate() {
        unsafe { *out.add(i) = glib::strdup(part) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_tracks(text: *const c_char) -> *mut NsCssValue {
    unsafe { bytes(text) }
        .and_then(grid::parse_tracks)
        .map_or(ptr::null_mut(), |tracks| value::new_tracks(&tracks))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_areas(text: *const c_char) -> *mut NsCssValue {
    unsafe { bytes(text) }
        .and_then(grid::parse_areas)
        .map_or(ptr::null_mut(), |areas| value::new_areas(&areas))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_line_canonical(
    text: *const c_char,
    ident_only: *mut GBoolean,
) -> *mut c_char {
    if !ident_only.is_null() {
        unsafe { *ident_only = 0 };
    }
    let Some((canon, only)) = unsafe { bytes(text) }.and_then(grid::line_canonical) else {
        return ptr::null_mut();
    };
    if !ident_only.is_null() {
        unsafe { *ident_only = glib::boolean(only) };
    }
    glib::strdup(&canon)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_placement_canonical(
    text: *const c_char,
    area: GBoolean,
) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(|t| grid::placement_canonical(t, area != 0)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_track_text_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(grid::track_text_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_template_parse(
    text: *const c_char,
    out: *mut *mut c_char,
    canon: *mut *mut c_char,
) -> GBoolean {
    let Some(template) = unsafe { bytes(text) }.and_then(grid::template_parse) else {
        return 0;
    };
    unsafe {
        store(out, &[template.rows, template.cols, template.areas]);
        *canon = glib::strdup(&template.canon);
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_shorthand_parse(
    text: *const c_char,
    out: *mut *mut c_char,
    canon: *mut *mut c_char,
) -> GBoolean {
    let Some(shorthand) = unsafe { bytes(text) }.and_then(grid::shorthand_parse) else {
        return 0;
    };
    unsafe {
        store(out, &shorthand.values);
        *canon = glib::strdup(&shorthand.canon);
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_auto_flow_canonical(text: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(grid::auto_flow_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_template_compose(v: *const *mut c_char) -> *mut c_char {
    let text = unsafe { values::<3>(v) }
        .map(|[rows, cols, areas]| grid::template_compose(rows, cols, areas))
        .unwrap_or_default();
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_grid_compose(v: *const *mut c_char) -> *mut c_char {
    let text = unsafe { values::<6>(v) }
        .map(|v| grid::grid_compose(&v))
        .unwrap_or_default();
    glib::strdup(&text)
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
