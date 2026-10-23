//! Southstar text — tests for the text renderer's shape extents and render component flags.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::{render_component, shape_get_extents};
use crate::types::Rectangle;

#[test]
fn shape_extents_repeat_the_logical_width() {
    let ink = Rectangle::new(-100, -2000, 1500, 2500);
    let logical = Rectangle::new(0, -2048, 1024, 3000);
    let (ink_rect, logical_rect) = shape_get_extents(3, &ink, &logical);
    assert_eq!(ink_rect, Rectangle::new(-100, -2000, 3548, 2500));
    assert_eq!(logical_rect, Rectangle::new(0, -2048, 3072, 3000));
}

#[test]
fn shape_extents_of_negative_widths_keep_the_leftmost_origin() {
    let ink = Rectangle::new(0, 0, 10, 10);
    let logical = Rectangle::new(0, 0, -1024, 10);
    let (ink_rect, logical_rect) = shape_get_extents(2, &ink, &logical);
    assert_eq!(ink_rect, Rectangle::new(-1024, 0, 10, 10));
    assert_eq!(logical_rect, Rectangle::new(-1024, 0, -1024, 10));
}

#[test]
fn shape_extents_of_no_characters_are_empty() {
    let rect = Rectangle::new(1, 2, 3, 4);
    assert_eq!(
        shape_get_extents(0, &rect, &rect),
        (Rectangle::default(), Rectangle::default())
    );
}

#[test]
fn background_and_underline_share_a_component_bit() {
    assert_eq!(render_component::BACKGROUND, render_component::UNDERLINE);
    assert_eq!(render_component::ALL, 0b11_1110);
}
