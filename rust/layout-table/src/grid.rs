//! Southstar — the table grid: the slot each cell starts in once the rowspans of earlier rows have claimed theirs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const MAX_COLS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub cols: usize,
    pub rows: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub row: usize,
    pub col: usize,
    pub cols: usize,
    pub rows: usize,
}

impl Placement {
    pub fn last_row(&self) -> usize {
        self.row + self.rows.max(1) - 1
    }

    pub fn columns(&self) -> core::ops::Range<usize> {
        self.col..self.col + self.cols
    }
}

pub fn place<I, R>(rows: I, row_count: usize) -> (Vec<Placement>, usize)
where
    I: IntoIterator<Item = R>,
    R: IntoIterator<Item = Span>,
{
    let mut held_for: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    let mut max_cols = 0;
    for (row, cells) in rows.into_iter().enumerate() {
        let rows_left = row_count.saturating_sub(row).max(1);
        let mut col = 0;
        for span in cells {
            while held_for.get(col).is_some_and(|&n| n > 0) {
                col += 1;
            }
            let col_start = col.min(MAX_COLS);
            let cols = span.cols.max(1).min(MAX_COLS - col_start);
            let rows = span.rows.clamp(1, rows_left);
            if held_for.len() < col_start + cols {
                held_for.resize(col_start + cols, 0);
            }
            for held in &mut held_for[col_start..col_start + cols] {
                *held = (*held).max(rows);
            }
            out.push(Placement {
                row,
                col: col_start,
                cols,
                rows,
            });
            max_cols = max_cols.max(col_start + cols);
            col = col_start + cols;
        }
        for held in &mut held_for {
            *held = held.saturating_sub(1);
        }
    }
    (out, max_cols)
}
