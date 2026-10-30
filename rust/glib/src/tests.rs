//! Southstar — unit tests for the GLib replacements: the locale-independent strtod.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{ascii_strtod, ascii_strtod_at, ascii_strtod_prefix};

fn prefix(text: &str) -> (f64, usize) {
    ascii_strtod_prefix(text.as_bytes())
}

#[test]
fn decimal_prefixes() {
    assert_eq!(prefix("  -1.5e3px"), (-1500.0, 8));
    assert_eq!(prefix("+.5"), (0.5, 3));
    assert_eq!(prefix("1."), (1.0, 2));
    assert_eq!(prefix("1.e5"), (100000.0, 4));
    assert_eq!(prefix("1e"), (1.0, 1));
    assert_eq!(prefix("1e+"), (1.0, 1));
    assert_eq!(prefix("12abc"), (12.0, 2));
    assert_eq!(prefix("100%"), (100.0, 3));
    assert_eq!(prefix("\t\n 42"), (42.0, 5));
    assert_eq!(
        prefix("3.14159265358979323846"),
        (core::f64::consts::PI, 22)
    );
}

#[test]
fn nothing_converts() {
    for text in ["", " ", ".", "e5", "+-1", "abc"] {
        assert_eq!(prefix(text), (0.0, 0), "{text:?}");
    }
}

#[test]
fn signed_zero_and_ranges() {
    let (zero, used) = prefix("-0");
    assert_eq!((zero, used), (0.0, 2));
    assert!(zero.is_sign_negative());
    assert_eq!(prefix("1e400"), (f64::INFINITY, 5));
    assert_eq!(prefix("-1e400"), (f64::NEG_INFINITY, 6));
    assert_eq!(prefix("1e-400"), (0.0, 6));
    assert_eq!(prefix("4.9e-324"), (5e-324, 8));
}

#[test]
fn hexadecimal() {
    assert_eq!(prefix("0x"), (0.0, 1));
    assert_eq!(prefix("0xg"), (0.0, 1));
    assert_eq!(prefix("0x1p4"), (16.0, 5));
    assert_eq!(prefix("0x.8"), (0.5, 4));
    assert_eq!(prefix("-0X1.8P-1z"), (-0.75, 9));
    assert_eq!(prefix("0x10.8p1"), (33.0, 8));
    assert_eq!(prefix("0x1P"), (1.0, 3));
    assert_eq!(prefix("0x1p-1074"), (5e-324, 9));
    assert_eq!(prefix("0x1.fffffffffffffp1023"), (f64::MAX, 22));
}

#[test]
fn special_values() {
    assert_eq!(prefix("inf"), (f64::INFINITY, 3));
    assert_eq!(prefix("-Infinity"), (f64::NEG_INFINITY, 9));
    assert_eq!(prefix("infx"), (f64::INFINITY, 3));
    let (nan, used) = prefix("NaN(abc)");
    assert!(nan.is_nan());
    assert_eq!(used, 8);
    let (nan, used) = prefix("nan(");
    assert!(nan.is_nan());
    assert_eq!(used, 3);
}

#[test]
fn stops_at_nul_and_offsets() {
    assert_eq!(ascii_strtod(b"2.5\0" as &[u8]), 2.5);
    assert_eq!(ascii_strtod_prefix(b"7\x008"), (7.0, 1));
    assert_eq!(ascii_strtod_at(c"x 12y", 1), (12.0, 4));
    assert_eq!(ascii_strtod_at(c"ab", 9), (0.0, 2));
}

#[test]
fn converts_between_charsets() {
    assert_eq!(
        crate::convert(b"caf\xe9", b"UTF-8", b"ISO-8859-1").as_deref(),
        Some("café".as_bytes())
    );
    assert_eq!(
        crate::convert("café".as_bytes(), b"ISO-8859-1", b"UTF-8").as_deref(),
        Some(b"caf\xe9".as_slice())
    );
    assert_eq!(crate::convert(b"x", b"UTF-8", b"no-such-charset"), None);
}

#[test]
fn collates_and_formats_local_times() {
    assert_eq!(
        crate::utf8_collate(b"apple", b"banana"),
        core::cmp::Ordering::Less
    );
    assert_eq!(
        crate::utf8_collate(b"same", b"same"),
        core::cmp::Ordering::Equal
    );
    let when = crate::LocalDateTime::from_unix(86_400 * 200).unwrap();
    assert_eq!(when.year(), 1970);
    assert!((199..=201).contains(&when.day_of_year()));
    assert_eq!(when.format(c"%Y").as_deref(), Some(b"1970".as_slice()));
    assert!(crate::LocalDateTime::now().is_some_and(|now| now.year() >= 2024));
}
