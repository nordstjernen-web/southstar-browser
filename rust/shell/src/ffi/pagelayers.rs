//! Southstar — the tab view's side of the page layers: parsing layer updates and driving the Rust page layer cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_double, c_int};

use southstar_glib::strdup;
use southstar_gtk::SnapshotRef;

use super::gdk::{Snapshot, Under};
use crate::pagelayers::{LayerUpdate, PageLayers};

pub(super) fn layer_update_parse(desc: &[u8], map: &[u8]) -> *mut LayerUpdate {
    LayerUpdate::parse(desc, map).map_or(core::ptr::null_mut(), |u| Box::into_raw(Box::new(u)))
}

/// # Safety
/// `u` is null or a layer update `layer_update_parse` returned that has not been freed.
pub(super) unsafe fn ns_layer_update_free(u: *mut LayerUpdate) {
    if !u.is_null() {
        // SAFETY: the caller passes a box `layer_update_parse` leaked and nothing has freed yet.
        drop(unsafe { Box::from_raw(u) });
    }
}

pub(super) fn ns_page_layers_new() -> *mut PageLayers {
    Box::into_raw(Box::default())
}

/// # Safety
/// `pl` is null or page layers `ns_page_layers_new` returned that have not been freed.
pub(super) unsafe fn ns_page_layers_free(pl: *mut PageLayers) {
    if !pl.is_null() {
        // SAFETY: the caller passes a box `ns_page_layers_new` leaked and nothing has freed yet.
        drop(unsafe { Box::from_raw(pl) });
    }
}

pub(super) fn ns_page_layers_reset(pl: Option<&mut PageLayers>) {
    if let Some(pl) = pl {
        pl.reset();
    }
}

pub(super) fn ns_page_layers_apply(
    pl: Option<&mut PageLayers>,
    u: Option<&mut LayerUpdate>,
) -> bool {
    match (pl, u) {
        (Some(pl), Some(u)) => pl.apply(u),
        _ => false,
    }
}

pub(super) fn ns_page_layers_active(pl: Option<&PageLayers>) -> bool {
    pl.is_some_and(PageLayers::active)
}

pub(super) fn ns_page_layers_gen(pl: Option<&PageLayers>) -> c_int {
    pl.map_or(-1, PageLayers::generation)
}

pub(super) fn ns_page_layers_vp_held(pl: Option<&PageLayers>) -> c_int {
    pl.map_or(-1, PageLayers::vp_held)
}

pub(super) fn ns_page_layers_have(
    pl: Option<&PageLayers>,
    y0: c_double,
    y1: c_double,
    any_gen: bool,
) -> *mut c_char {
    let have = pl.map_or_else(String::new, |pl| pl.have(y0, y1, any_gen));
    strdup(have.as_bytes())
}

pub(super) fn ns_page_layers_missing(
    pl: Option<&PageLayers>,
    y0: c_double,
    y1: c_double,
    view_y0: c_double,
    view_y1: c_double,
    page_h: c_double,
) -> bool {
    pl.is_some_and(|pl| pl.missing(y0, y1, view_y0, view_y1, page_h))
}

pub(super) fn ns_page_layers_evict(pl: Option<&mut PageLayers>, y0: c_double, y1: c_double) {
    if let Some(pl) = pl {
        pl.evict(y0, y1);
    }
}

pub(super) fn ns_page_layers_scroller_at(
    pl: Option<&PageLayers>,
    x: c_double,
    y: c_double,
    scroll_x: c_double,
    scroll_y: c_double,
    vertical: bool,
) -> bool {
    pl.is_some_and(|pl| pl.scroller_at(x, y, scroll_x, scroll_y, vertical))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ns_page_layers_snapshot(
    pl: Option<&PageLayers>,
    snapshot: SnapshotRef<'_>,
    width: c_double,
    height: c_double,
    raster: c_double,
    scroll_x: c_double,
    scroll_y: c_double,
    under: Under<'_>,
) {
    let Some(pl) = pl else {
        return;
    };
    let mut canvas = Snapshot { snapshot, under };
    pl.snapshot(&mut canvas, width, height, raster, scroll_x, scroll_y);
}
