//! Southstar — flexbox layout ported from layout.c: single-line and wrapping row containers and column containers, with flex base sizes, resolved flexible lengths, cross sizes, alignment, gaps and auto margins.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod column;
mod ffi;
mod items;
mod row;
mod wrap;

use southstar_layout::Style;

pub use column::layout as layout_column;
pub use row::layout as layout_row;
pub use wrap::layout as layout_row_wrap;

#[derive(Clone, Copy)]
pub struct Frame {
    pub cw: f64,
    pub inner_x: f64,
    pub inner_y: f64,
    pub child_inherited: *const Style,
    pub reverse: bool,
}
