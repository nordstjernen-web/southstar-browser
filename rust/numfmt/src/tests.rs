//! Southstar — unit tests for the %g and %f formatters against values glibc's printf prints.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{fixed, g, general};

#[test]
fn general_matches_printf() {
    let cases: &[(f64, usize, &str)] = &[
        (0.0, 6, "0"),
        (-0.0, 6, "-0"),
        (1.0, 6, "1"),
        (0.5, 6, "0.5"),
        (100.0, 6, "100"),
        (123456.0, 6, "123456"),
        (1234567.0, 6, "1.23457e+06"),
        (0.0001, 6, "0.0001"),
        (0.00001, 6, "1e-05"),
        (1e100, 6, "1e+100"),
        (-2.5e-300, 6, "-2.5e-300"),
        (0.1 + 0.2, 6, "0.3"),
        (0.1 + 0.2, 17, "0.30000000000000004"),
        (1.0 / 3.0, 10, "0.3333333333"),
        (999999.5, 6, "1e+06"),
        (2.5, 1, "2"),
        (3.5, 1, "4"),
        (0.125, 2, "0.12"),
        (0.375, 2, "0.38"),
        (9.5, 0, "1e+01"),
        (16.666666666, 8, "16.666667"),
        (f64::MAX, 17, "1.7976931348623157e+308"),
        (5e-324, 6, "4.94066e-324"),
    ];
    for &(value, precision, want) in cases {
        assert_eq!(
            general(value, precision),
            want,
            "%.{precision}g of {value:e}"
        );
    }
    assert_eq!(g(f64::INFINITY), "inf");
    assert_eq!(g(f64::NEG_INFINITY), "-inf");
    assert_eq!(g(f64::NAN), "nan");
    assert_eq!(g(-f64::NAN), "-nan");
}

#[test]
fn fixed_matches_printf() {
    let cases: &[(f64, usize, &str)] = &[
        (0.0, 2, "0.00"),
        (-0.0, 1, "-0.0"),
        (0.125, 2, "0.12"),
        (0.375, 2, "0.38"),
        (2.5, 0, "2"),
        (1.005, 2, "1.00"),
        (123.456789, 6, "123.456789"),
        (-1e21, 1, "-1000000000000000000000.0"),
    ];
    for &(value, precision, want) in cases {
        assert_eq!(fixed(value, precision), want, "%.{precision}f of {value:e}");
    }
    assert_eq!(fixed(f64::NAN, 3), "nan");
}
