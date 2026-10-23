//! Southstar text — unit tests for attribute lists, their string form and the attribute iterator.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::*;

#[test]
fn insert_orders_by_start() {
    let mut list = AttrList::new();
    list.insert(attr_weight_new(Weight::BOLD).with_range(5, 10));
    list.insert(attr_size_new(12 * 1024).with_range(0, 20));
    list.insert_before(attr_rise_new(3).with_range(5, 6));
    list.insert(attr_family_new("Mono\n\"x\"").with_range(5, 7));
    assert_eq!(
        list.to_string(),
        "0 20 7 12288\n5 6 13 3\n5 10 4 700\n5 7 2 \"Mono\\n\\\"x\\\"\""
    );
}

#[test]
fn iterates_ranges_and_fonts() {
    let mut list = AttrList::new();
    list.insert(attr_weight_new(Weight::BOLD).with_range(2, 6));
    list.insert(attr_scale_new(2.0).with_range(4, 8));
    let mut iterator = list.iterator();
    let mut ranges = Vec::new();
    loop {
        ranges.push(iterator.range());
        if !iterator.next() {
            break;
        }
    }
    assert_eq!(ranges, [(0, 2), (2, 4), (4, 6), (6, 8), (8, i32::MAX)]);

    let mut iterator = list.iterator();
    assert!(iterator.advance(5));
    let mut desc = FontDescription::from_string("Sans 10");
    iterator.get_font(&mut desc, None, None);
    assert_eq!(desc.to_string(), "Sans Bold 20");
}

#[test]
fn formats_floats_like_printf_g17() {
    assert_eq!(format_g17(1.5), "1.5");
    assert_eq!(format_g17(1.2), "1.2");
    assert_eq!(format_g17(0.1), "0.10000000000000001");
    assert_eq!(format_g17(1e20), "1e+20");
    assert_eq!(format_g17(-2.5e-7), "-2.4999999999999999e-07");
    assert_eq!(format_g17(0.0), "0");
}
