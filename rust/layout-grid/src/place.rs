//! Southstar — the grid occupancy map and the slot search behind row-major auto-placement.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::text::ROWS_MAX;
use crate::tracks::MAX;

#[derive(Default)]
pub(crate) struct Occupancy {
    rows: Vec<[bool; MAX]>,
}

pub(crate) struct Slot {
    pub row: i32,
    pub col: i32,
}

pub(crate) struct Area {
    pub col_span: i32,
    pub row_span: i32,
    pub n_cols: i32,
}

impl Occupancy {
    fn occupied(&self, row: i32, col: i32) -> bool {
        if row < 0 || col < 0 || col >= MAX as i32 {
            return true;
        }
        self.rows.get(row as usize).is_some_and(|r| r[col as usize])
    }

    fn occupy(&mut self, row: i32, col: i32) {
        if !(0..ROWS_MAX).contains(&row) || !(0..MAX as i32).contains(&col) {
            return;
        }
        let row = row as usize;
        if row >= self.rows.len() {
            self.rows.resize(row + 1, [false; MAX]);
        }
        self.rows[row][col as usize] = true;
    }

    fn available(&self, row: i32, col: i32, a: &Area) -> bool {
        if row < 0 || col < 0 || a.col_span < 1 || a.row_span < 1 {
            return false;
        }
        if col + a.col_span > a.n_cols || row + a.row_span > ROWS_MAX {
            return false;
        }
        for r in 0..a.row_span {
            for c in 0..a.col_span {
                if self.occupied(row + r, col + c) {
                    return false;
                }
            }
        }
        true
    }

    pub fn mark(&mut self, row: i32, col: i32, a: &Area) {
        if row < 0 || col < 0 || a.col_span < 1 || a.row_span < 1 {
            return;
        }
        let col_span = if col + a.col_span > a.n_cols {
            a.n_cols - col
        } else {
            a.col_span
        };
        let row_span = if row + a.row_span > ROWS_MAX {
            ROWS_MAX - row
        } else {
            a.row_span
        };
        for r in 0..row_span {
            for c in 0..col_span {
                self.occupy(row + r, col + c);
            }
        }
    }

    pub fn find_slot(
        &self,
        a: &Area,
        start: &Slot,
        fixed_row: bool,
        fixed_col: bool,
    ) -> Option<Slot> {
        let r0 = start.row.max(0);
        let c0 = start.col.max(0);
        if fixed_row && fixed_col {
            return (r0 < ROWS_MAX && c0 < a.n_cols).then_some(Slot { row: r0, col: c0 });
        }
        for r in r0..ROWS_MAX {
            let first_col = if fixed_col || r == r0 { c0 } else { 0 };
            let mut last_col = if fixed_col { c0 } else { a.n_cols - a.col_span };
            if last_col < first_col {
                last_col = first_col;
            }
            for c in first_col..=last_col {
                if self.available(r, c, a) {
                    return Some(Slot { row: r, col: c });
                }
            }
            if fixed_row {
                break;
            }
        }
        None
    }

    pub fn advance_cursor(&self, cursor: &mut Slot, n_cols: i32) {
        if cursor.col >= n_cols {
            cursor.col = 0;
            cursor.row += 1;
        }
        while cursor.row < ROWS_MAX && self.occupied(cursor.row, cursor.col) {
            cursor.col += 1;
            if cursor.col >= n_cols {
                cursor.col = 0;
                cursor.row += 1;
            }
        }
    }
}
