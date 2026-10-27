//! Southstar — the painter's inline text entry points the box tree, selection and box queries call back into, installed once before the first layout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use crate::{BoxRef, InlineAttr};

pub type Extents = (f64, f64, f64, f64);

pub struct PaintLinks {
    pub inline_xy_to_byte: fn(BoxRef<'_>, f64, f64) -> Option<usize>,
    pub inline_word_range: fn(BoxRef<'_>, usize) -> Option<(usize, usize)>,
    pub inline_range_extents: fn(BoxRef<'_>, usize, usize, Option<&InlineAttr>) -> Option<Extents>,
}

static LINKS: OnceLock<PaintLinks> = OnceLock::new();

pub fn install(links: PaintLinks) {
    let _ = LINKS.set(links);
}

pub fn inline_xy_to_byte(b: BoxRef<'_>, rel_x: f64, rel_y: f64) -> Option<usize> {
    (LINKS.get()?.inline_xy_to_byte)(b, rel_x, rel_y)
}

pub fn inline_word_range(b: BoxRef<'_>, byte: usize) -> Option<(usize, usize)> {
    (LINKS.get()?.inline_word_range)(b, byte)
}

pub fn inline_range_extents(
    b: BoxRef<'_>,
    start: usize,
    len: usize,
    element: Option<&InlineAttr>,
) -> Option<(f64, f64, f64, f64)> {
    (LINKS.get()?.inline_range_extents)(b, start, len, element)
}
