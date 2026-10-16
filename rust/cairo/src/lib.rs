//! Southstar — safe, single-threaded wrappers over the system libcairo; its objects are interior-mutable and every call goes through a handle that keeps them alive.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod types;

pub use ffi::{
    Context, ContextRef, FontOptions, FontOptionsRef, Path, Pattern, Surface, SurfaceRef,
    encode_png,
};
pub use types::{
    Antialias, Content, Error, Extend, FillRule, Filter, Format, HintMetrics, HintStyle, LineCap,
    LineJoin, Matrix, Operator, PathSegment, PdfMetadata, Rectangle, SubpixelOrder, TextExtents,
};

#[cfg(test)]
mod tests;
