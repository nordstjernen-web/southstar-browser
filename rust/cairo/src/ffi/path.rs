//! Southstar — copied cairo paths: reading their segments and transforming their points in place.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::NonNull;

use super::RawPath;
use crate::types::{Error, Matrix, PathSegment};

const MOVE_TO: c_int = 0;
const LINE_TO: c_int = 1;
const CURVE_TO: c_int = 2;
const CLOSE_PATH: c_int = 3;

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct PathHeader {
    pub(crate) kind: c_int,
    pub(crate) length: c_int,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) union PathData {
    pub(crate) header: PathHeader,
    pub(crate) point: [f64; 2],
}

fn header_length(entry: &PathData) -> usize {
    // SAFETY: cairo starts every element with a header entry, and this is only called at element starts.
    let length = unsafe { entry.header.length };
    usize::try_from(length).unwrap_or(0).max(1)
}

fn point(data: &[PathData], i: usize) -> Option<[f64; 2]> {
    let entry = data.get(i)?;
    // SAFETY: the entries after an element's header are its points.
    Some(unsafe { entry.point })
}

pub(crate) fn decode(data: &[PathData]) -> Vec<PathSegment> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(entry) = data.get(i) {
        // SAFETY: cairo starts every element with a header entry, and i is always at an element start.
        let kind = unsafe { entry.header.kind };
        let segment = match kind {
            MOVE_TO => point(data, i + 1).map(|[x, y]| PathSegment::MoveTo(x, y)),
            LINE_TO => point(data, i + 1).map(|[x, y]| PathSegment::LineTo(x, y)),
            CURVE_TO => match (point(data, i + 1), point(data, i + 2), point(data, i + 3)) {
                (Some([x1, y1]), Some([x2, y2]), Some([x3, y3])) => {
                    Some(PathSegment::CurveTo([x1, y1, x2, y2, x3, y3]))
                }
                _ => None,
            },
            CLOSE_PATH => Some(PathSegment::ClosePath),
            _ => None,
        };
        out.extend(segment);
        i += header_length(entry);
    }
    out
}

pub(crate) fn transform_points(data: &mut [PathData], m: &Matrix) {
    let total = data.len();
    let mut i = 0;
    while i < total {
        let length = header_length(&data[i]);
        let end = (i + length).min(total);
        for entry in &mut data[i + 1..end] {
            // SAFETY: the entries after an element's header are its points.
            let [x, y] = unsafe { entry.point };
            entry.point = [m.xx * x + m.xy * y + m.x0, m.yx * x + m.yy * y + m.y0];
        }
        i += length;
    }
}

pub struct Path(NonNull<RawPath>);

impl Path {
    /// # Safety
    ///
    /// `raw` is a path from cairo_copy_path that the returned Path takes over.
    pub(super) unsafe fn own(raw: *mut RawPath) -> Path {
        Path(NonNull::new(raw).expect("cairo returns a path, never null"))
    }

    pub(super) fn raw(&self) -> *mut RawPath {
        self.0.as_ptr()
    }

    fn header(&self) -> &RawPath {
        // SAFETY: the Path owns a live cairo_path_t until it drops.
        unsafe { self.0.as_ref() }
    }

    fn len(&self) -> usize {
        let path = self.header();
        if path.data.is_null() {
            return 0;
        }
        usize::try_from(path.num_data).unwrap_or(0)
    }

    fn data(&self) -> &[PathData] {
        let len = self.len();
        if len == 0 {
            return &[];
        }
        // SAFETY: a copied path's data holds num_data entries owned by this Path.
        unsafe { core::slice::from_raw_parts(self.header().data, len) }
    }

    fn data_mut(&mut self) -> &mut [PathData] {
        let len = self.len();
        if len == 0 {
            return &mut [];
        }
        // SAFETY: a copied path's data holds num_data entries owned by this Path, which is borrowed mutably.
        unsafe { core::slice::from_raw_parts_mut(self.header().data, len) }
    }

    pub fn status(&self) -> Result<(), Error> {
        Error::check(self.header().status)
    }

    pub fn is_empty(&self) -> bool {
        self.header().num_data <= 0
    }

    pub fn segments(&self) -> Vec<PathSegment> {
        decode(self.data())
    }

    pub fn transform(&mut self, m: &Matrix) {
        transform_points(self.data_mut(), m);
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        // SAFETY: the Path owns its cairo_path_t and frees it once, here.
        unsafe { super::cairo_path_destroy(self.raw()) };
    }
}
