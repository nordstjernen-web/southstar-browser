//! Southstar — the C ABI of the viewport the style engine resolves viewport units against, kept by rust/css-media, and the callback that sizes a framed document's viewport.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::{Mutex, PoisonError};

use southstar_dom::NsNode;

pub(super) type FrameViewport = unsafe extern "C" fn(*const NsNode, *mut f64, *mut f64);

static FRAME_VIEWPORT: Mutex<Option<FrameViewport>> = Mutex::new(None);

pub fn get() -> (f64, f64) {
    southstar_css_media::base_viewport()
}

pub(super) fn replace(w: f64, h: f64) -> (f64, f64) {
    southstar_css_media::replace_base_viewport(w, h)
}

pub(super) fn frame_viewport() -> Option<FrameViewport> {
    *FRAME_VIEWPORT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_set_frame_viewport_cb(cb: Option<FrameViewport>) {
    *FRAME_VIEWPORT
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = cb;
}
