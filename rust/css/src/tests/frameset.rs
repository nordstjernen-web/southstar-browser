//! Southstar — unit tests for the frameset rows and cols lists of dimensions and the grid tracks they map to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::frameset::{Dimension, grid_tracks, parse_dimensions};

fn tracks(input: &str) -> Option<String> {
    grid_tracks(&parse_dimensions(input.as_bytes()))
}

#[test]
fn parses_each_unit() {
    assert_eq!(
        parse_dimensions(b"100, 25%,*, 3*,0.5*,"),
        vec![
            Dimension::Absolute(100.0),
            Dimension::Percentage(25.0),
            Dimension::Relative(1.0),
            Dimension::Relative(3.0),
            Dimension::Relative(0.5),
        ]
    );
    assert_eq!(parse_dimensions(b""), Vec::new());
    assert_eq!(parse_dimensions(b"junk"), vec![Dimension::Absolute(0.0)]);
}

#[test]
fn maps_mixed_lists_to_fixed_and_flexible_tracks() {
    assert_eq!(tracks("200,*").as_deref(), Some("200px 1fr"));
    assert_eq!(tracks("30%,2*,*").as_deref(), Some("30% 2fr 1fr"));
    assert_eq!(tracks("100,20%").as_deref(), Some("100px 20%"));
}

#[test]
fn scales_lists_without_relative_entries_to_fill() {
    assert_eq!(tracks("50%,50%").as_deref(), Some("50fr 50fr"));
    assert_eq!(tracks("100,300").as_deref(), Some("100fr 300fr"));
    assert_eq!(tracks(""), None);
}
