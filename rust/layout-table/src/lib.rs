//! Southstar — table layout ported from src/layout.c: column widths for auto and fixed table-layout, colspan and rowspan, col and colgroup widths, border-spacing and collapsed borders, row heights, vertical-align in cells, captions and a table's intrinsic widths.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod grid;
mod table;

pub use table::{intrinsic_width as table_intrinsic_width, layout as layout_table};

#[cfg(test)]
mod tests;
