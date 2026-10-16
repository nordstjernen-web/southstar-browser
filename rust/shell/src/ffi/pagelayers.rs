//! Southstar — the C entry points of the page layers over the Rust page layer cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uchar, c_void};

use southstar_glib::{boolean, slice, strdup};
use southstar_gtk_ffi::GtkSnapshot;

use super::gdk::{Snapshot, Under};
use crate::pagelayers::{LayerUpdate, PageLayers};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layer_update_parse(
    desc: *const c_char,
    map: *const c_uchar,
    map_size: usize,
) -> *mut LayerUpdate {
    if desc.is_null() {
        return core::ptr::null_mut();
    }
    let desc = unsafe { CStr::from_ptr(desc) }.to_bytes();
    let map = unsafe { slice(map, map_size) };
    LayerUpdate::parse(desc, map).map_or(core::ptr::null_mut(), |u| Box::into_raw(Box::new(u)))
}

/// # Safety
/// `u` is null or a layer update `ns_layer_update_parse` returned that has not been freed.
pub(super) unsafe fn ns_layer_update_free(u: *mut LayerUpdate) {
    if !u.is_null() {
        drop(unsafe { Box::from_raw(u) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_page_layers_new() -> *mut PageLayers {
    Box::into_raw(Box::default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_free(pl: *mut PageLayers) {
    if !pl.is_null() {
        drop(unsafe { Box::from_raw(pl) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_reset(pl: *mut PageLayers) {
    if let Some(pl) = unsafe { pl.as_mut() } {
        pl.reset();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_apply(pl: *mut PageLayers, u: *mut LayerUpdate) -> c_int {
    match unsafe { (pl.as_mut(), u.as_mut()) } {
        (Some(pl), Some(u)) => boolean(pl.apply(u)),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_active(pl: *const PageLayers) -> c_int {
    boolean(unsafe { pl.as_ref() }.is_some_and(PageLayers::active))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_gen(pl: *const PageLayers) -> c_int {
    unsafe { pl.as_ref() }.map_or(-1, PageLayers::generation)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_vp_held(pl: *const PageLayers) -> c_int {
    unsafe { pl.as_ref() }.map_or(-1, PageLayers::vp_held)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_have(
    pl: *const PageLayers,
    y0: c_double,
    y1: c_double,
    any_gen: c_int,
) -> *mut c_char {
    let have = unsafe { pl.as_ref() }.map_or_else(String::new, |pl| pl.have(y0, y1, any_gen != 0));
    strdup(have.as_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_missing(
    pl: *const PageLayers,
    y0: c_double,
    y1: c_double,
    view_y0: c_double,
    view_y1: c_double,
    page_h: c_double,
) -> c_int {
    boolean(unsafe { pl.as_ref() }.is_some_and(|pl| pl.missing(y0, y1, view_y0, view_y1, page_h)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_evict(pl: *mut PageLayers, y0: c_double, y1: c_double) {
    if let Some(pl) = unsafe { pl.as_mut() } {
        pl.evict(y0, y1);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_page_layers_scroller_at(
    pl: *const PageLayers,
    x: c_double,
    y: c_double,
    scroll_x: c_double,
    scroll_y: c_double,
    vertical: c_int,
) -> c_int {
    boolean(
        unsafe { pl.as_ref() }
            .is_some_and(|pl| pl.scroller_at(x, y, scroll_x, scroll_y, vertical != 0)),
    )
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_page_layers_snapshot(
    pl: *const PageLayers,
    snapshot: *mut GtkSnapshot,
    width: c_double,
    height: c_double,
    raster: c_double,
    scroll_x: c_double,
    scroll_y: c_double,
    under: Under,
    under_data: *mut c_void,
) {
    let Some(pl) = (unsafe { pl.as_ref() }) else {
        return;
    };
    let mut canvas = Snapshot {
        snapshot,
        under,
        under_data,
    };
    pl.snapshot(&mut canvas, width, height, raster, scroll_x, scroll_y);
}
