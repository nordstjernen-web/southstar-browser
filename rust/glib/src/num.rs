//! Southstar — GLib's CLAMP, MAX and MIN on doubles, which pass a NaN argument through instead of propagating or panicking.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x > hi {
        hi
    } else if x < lo {
        lo
    } else {
        x
    }
}

pub fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}
