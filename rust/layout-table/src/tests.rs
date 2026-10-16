//! Southstar — unit tests for the table grid's slot placement.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::grid::{MAX_COLS, Placement, Span, place};

fn span(cols: usize, rows: usize) -> Span {
    Span { cols, rows }
}

fn at(row: usize, col: usize, cols: usize, rows: usize) -> Placement {
    Placement {
        row,
        col,
        cols,
        rows,
    }
}

#[test]
fn plain_rows_fill_left_to_right() {
    let rows = vec![vec![span(1, 1), span(1, 1)], vec![span(1, 1)]];
    let (cells, cols) = place(rows, 2);
    assert_eq!(cols, 2);
    assert_eq!(cells, vec![at(0, 0, 1, 1), at(0, 1, 1, 1), at(1, 0, 1, 1)]);
}

#[test]
fn rowspan_pushes_later_rows_right() {
    let rows = vec![vec![span(1, 2), span(1, 1)], vec![span(1, 1), span(1, 1)]];
    let (cells, cols) = place(rows, 2);
    assert_eq!(cols, 3);
    assert_eq!(cells[2], at(1, 1, 1, 1));
    assert_eq!(cells[3], at(1, 2, 1, 1));
}

#[test]
fn rowspan_in_middle_column_is_skipped() {
    let rows = vec![
        vec![span(1, 1), span(1, 3), span(1, 1)],
        vec![span(1, 1), span(1, 1)],
        vec![span(1, 1), span(1, 1)],
    ];
    let (cells, cols) = place(rows, 3);
    assert_eq!(cols, 3);
    assert_eq!(cells[3], at(1, 0, 1, 1));
    assert_eq!(cells[4], at(1, 2, 1, 1));
    assert_eq!(cells[6], at(2, 2, 1, 1));
}

#[test]
fn colspan_and_rowspan_combine() {
    let rows = vec![vec![span(2, 2), span(1, 1)], vec![span(1, 1)]];
    let (cells, cols) = place(rows, 2);
    assert_eq!(cols, 3);
    assert_eq!(cells[0], at(0, 0, 2, 2));
    assert_eq!(cells[2], at(1, 2, 1, 1));
}

#[test]
fn rowspan_is_clamped_to_the_rows_left() {
    let rows = vec![vec![span(1, 1)], vec![span(1, 5)]];
    let (cells, _) = place(rows, 2);
    assert_eq!(cells[1], at(1, 0, 1, 1));
    assert_eq!(cells[1].last_row(), 1);
}

#[test]
fn columns_stop_at_the_limit() {
    let rows = vec![vec![span(MAX_COLS - 1, 1), span(10, 1), span(1, 1)]];
    let (cells, cols) = place(rows, 1);
    assert_eq!(cols, MAX_COLS);
    assert_eq!(cells[1], at(0, MAX_COLS - 1, 1, 1));
    assert_eq!(cells[2], at(0, MAX_COLS, 0, 1));
}
