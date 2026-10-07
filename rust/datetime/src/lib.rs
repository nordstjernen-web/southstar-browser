//! Southstar — civil-date math and HTML date/time string parsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};

mod ffi;

pub const MAX_YEAR: c_int = 275_760;

pub fn floormod(a: c_long, b: c_long) -> c_long {
    let r = a % b;
    if r != 0 && ((r < 0) != (b < 0)) {
        r + b
    } else {
        r
    }
}

pub fn days_from_civil(y: c_int, m: c_int, d: c_int) -> c_long {
    let yy = y - c_int::from(m <= 2);
    let era = c_long::from(if yy >= 0 { yy } else { yy - 399 } / 400);
    let yoe = (c_long::from(yy) - era * 400) as u32;
    let doy = ((153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1) as u32;
    let doe = yoe
        .wrapping_mul(365)
        .wrapping_add(yoe / 4)
        .wrapping_sub(yoe / 100)
        .wrapping_add(doy);
    era * 146_097 + doe as c_long - 719_468
}

pub fn civil_from_days(z: c_long) -> (c_int, c_int, c_int) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let yy = yoe as c_int + (era * 400) as c_int;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1;
    let mm = if mp < 10 { mp + 3 } else { mp - 9 };
    (yy + c_int::from(mm <= 2), mm as c_int, dd as c_int)
}

pub fn days_in_month(y: c_int, m: c_int) -> c_int {
    const DAYS: [c_int; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        1..=12 => DAYS[(m - 1) as usize],
        _ => 0,
    }
}

pub fn iso_weeks_in_year(y: c_int) -> c_int {
    let p = (y + y / 4 - y / 100 + y / 400) % 7;
    let py = y - 1;
    let p1 = (py + py / 4 - py / 100 + py / 400) % 7;
    if p == 4 || p1 == 3 { 53 } else { 52 }
}

pub fn iso_week1_monday(y: c_int) -> c_long {
    let jan4 = days_from_civil(y, 1, 4);
    jan4 - floormod(jan4 + 3, 7)
}

pub fn read_digits(s: &[u8], min: c_int, max: c_int) -> Option<(usize, c_int)> {
    let mut value: c_int = 0;
    let mut count: c_int = 0;
    while count < max {
        match s.get(count as usize) {
            Some(&c) if c.is_ascii_digit() => {
                value = value.wrapping_mul(10).wrapping_add(c_int::from(c - b'0'));
                count += 1;
            }
            _ => break,
        }
    }
    (count >= min).then_some((count as usize, value))
}

fn expect(s: &[u8], at: usize, byte: u8) -> Option<usize> {
    (s.get(at) == Some(&byte)).then_some(at + 1)
}

pub fn read_date(s: &[u8]) -> Option<(usize, c_int, c_int, c_int)> {
    let (n, y) = read_digits(s, 4, 9)?;
    let at = expect(s, n, b'-')?;
    let (n, m) = read_digits(&s[at..], 2, 2)?;
    let at = expect(s, at + n, b'-')?;
    let (n, d) = read_digits(&s[at..], 2, 2)?;
    let valid =
        (1..=MAX_YEAR).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m);
    valid.then_some((at + n, y, m, d))
}

pub fn read_time(s: &[u8]) -> Option<(usize, c_int)> {
    let (n, h) = read_digits(s, 2, 2)?;
    let at = expect(s, n, b':')?;
    let (n, mi) = read_digits(&s[at..], 2, 2)?;
    let mut at = at + n;
    let mut se = 0;
    let mut frac = 0;
    if s.get(at) == Some(&b':') {
        let (n, seconds) = read_digits(&s[at + 1..], 2, 2)?;
        se = seconds;
        at += 1 + n;
        if s.get(at) == Some(&b'.') {
            at += 1;
            let digits = s[at..].iter().take_while(|c| c.is_ascii_digit()).count();
            if digits == 0 {
                return None;
            }
            let (_, millis) = read_digits(&s[at..], 1, 3)?;
            frac = millis * [100, 10, 1][digits.min(3) - 1];
            at += digits;
        }
    }
    (h <= 23 && mi <= 59 && se <= 59).then_some((at, ((h * 60 + mi) * 60 + se) * 1000 + frac))
}
