//! Southstar — the C ABI of the viewport the style engine resolves viewport units against, and the callback that sizes a framed document's viewport.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use southstar_dom::NsNode;

pub(super) type FrameViewport = unsafe extern "C" fn(*const NsNode, *mut f64, *mut f64);

static WIDTH: AtomicU64 = AtomicU64::new(1000f64.to_bits());
static HEIGHT: AtomicU64 = AtomicU64::new(800f64.to_bits());
static FRAME_VIEWPORT: Mutex<Option<FrameViewport>> = Mutex::new(None);

pub(crate) fn get() -> (f64, f64) {
    (
        f64::from_bits(WIDTH.load(Ordering::Relaxed)),
        f64::from_bits(HEIGHT.load(Ordering::Relaxed)),
    )
}

pub(super) fn replace(w: f64, h: f64) -> (f64, f64) {
    (
        f64::from_bits(WIDTH.swap(w.to_bits(), Ordering::Relaxed)),
        f64::from_bits(HEIGHT.swap(h.to_bits(), Ordering::Relaxed)),
    )
}

pub(super) fn frame_viewport() -> Option<FrameViewport> {
    *FRAME_VIEWPORT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_viewport(vw_px: f64, vh_px: f64) {
    if vw_px > 0.0 {
        WIDTH.store(vw_px.to_bits(), Ordering::Relaxed);
    }
    if vh_px > 0.0 {
        HEIGHT.store(vh_px.to_bits(), Ordering::Relaxed);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_viewport_w() -> f64 {
    get().0
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_viewport_h() -> f64 {
    get().1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_frame_viewport_cb(cb: Option<FrameViewport>) {
    *FRAME_VIEWPORT
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = cb;
}
