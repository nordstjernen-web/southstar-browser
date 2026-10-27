//! Southstar — WebGPU's entry points and the cairo, canvas and DOM calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_canvas::host as canvas;
use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};

pub(crate) struct Surface(southstar_cairo::Surface);

impl Surface {
    pub(crate) fn new(w: i32, h: i32) -> Option<Surface> {
        southstar_cairo::Surface::image(southstar_cairo::Format::Argb32, w, h).map(Surface)
    }

    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.0.as_raw()
    }

    pub(crate) fn with_pixels_mut<R>(&self, f: impl FnOnce(&mut [u8], usize) -> R) -> Option<R> {
        self.0.as_ref().with_data_mut(f)
    }
}

pub(crate) struct SourceImage {
    surface: Surface,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

impl SourceImage {
    pub(crate) fn with_pixels<R>(&self, f: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        self.surface.0.as_ref().with_data(f)
    }
}

pub(crate) fn drawimage_source(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<SourceImage>, Value> {
    let source = canvas::drawimage_source_surface(scope, src)?;
    Ok(source.and_then(|(raw, w, h)| {
        // SAFETY: the canvas hands over its own reference to a live surface, or null.
        let surface = unsafe { southstar_cairo::Surface::from_raw_full(raw) }?;
        Some(SourceImage {
            surface: Surface(surface),
            w,
            h,
        })
    }))
}

pub(crate) fn internal_error(scope: &mut Scope<'_>, message: &str) -> Value {
    scope.internal_error(message)
}

pub(crate) fn canvas_attr(canvas: usize, name: &CStr) -> Option<Vec<u8>> {
    let node = unsafe { Node::from_ptr(canvas as *const NsNode) }?;
    node.attr(name).map(|text| text.to_bytes().to_vec())
}

pub(crate) fn canvas_surface(canvas: usize) -> *mut c_void {
    crate::context::canvas_surface(canvas).unwrap_or(ptr::null_mut())
}
