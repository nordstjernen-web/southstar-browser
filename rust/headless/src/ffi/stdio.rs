//! Southstar — headless output on stdout and stderr, and the parsers for the numbers in scripted actions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};

pub fn out(bytes: &[u8]) {
    southstar_glib::stdout_write(bytes);
}

pub fn flush() {
    southstar_glib::stdout_flush();
}

pub fn err(bytes: &[u8]) {
    southstar_glib::stderr_write(bytes);
}

struct Scanner<'a> {
    rest: &'a [u8],
}

impl<'a> Scanner<'a> {
    fn new(input: &'a [u8]) -> Scanner<'a> {
        let end = input.iter().position(|&b| b == 0).unwrap_or(input.len());
        Scanner {
            rest: &input[..end],
        }
    }

    fn skip_space(&mut self) {
        let start = self
            .rest
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(self.rest.len());
        self.rest = &self.rest[start..];
    }

    fn digits(&self, from: usize) -> usize {
        self.rest[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    }

    fn take(&mut self, len: usize) -> Option<&'a str> {
        let (taken, rest) = self.rest.split_at(len);
        self.rest = rest;
        core::str::from_utf8(taken).ok()
    }

    fn sign_len(&self) -> usize {
        usize::from(matches!(self.rest.first(), Some(b'+' | b'-')))
    }

    fn float(&mut self) -> Option<f64> {
        self.skip_space();
        let mut len = self.sign_len();
        let whole = self.digits(len);
        len += whole;
        let mut fraction = 0;
        if self.rest.get(len) == Some(&b'.') {
            fraction = self.digits(len + 1);
            len += 1 + fraction;
        }
        if whole + fraction == 0 {
            return None;
        }
        if matches!(self.rest.get(len), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(self.rest.get(len + 1), Some(b'+' | b'-')));
            let exponent = self.digits(len + 1 + sign);
            if exponent > 0 {
                len += 1 + sign + exponent;
            }
        }
        self.take(len)?.parse().ok()
    }

    fn int(&mut self) -> Option<i64> {
        self.skip_space();
        let sign = self.sign_len();
        let digits = self.digits(sign);
        if digits == 0 {
            return None;
        }
        self.take(sign + digits)?.parse().ok()
    }

    fn comma(&mut self) -> Option<()> {
        self.skip_space();
        let rest = self.rest.strip_prefix(b",")?;
        self.rest = rest;
        Some(())
    }

    fn point(&mut self) -> Option<(f64, f64)> {
        let x = self.float()?;
        self.comma()?;
        Some((x, self.float()?))
    }
}

pub fn scan_point(s: &[u8]) -> Option<(f64, f64)> {
    Scanner::new(s).point()
}

pub fn scan_select(s: &[u8]) -> Option<(c_int, f64, f64)> {
    let mut scan = Scanner::new(s);
    let kind = c_int::try_from(scan.int()?).ok()?;
    let (x, y) = scan.point()?;
    Some((kind, x, y))
}

pub fn scan_size(s: &[u8]) -> Option<(c_int, c_int)> {
    let mut scan = Scanner::new(s);
    let w = c_int::try_from(scan.int()?).ok()?;
    let h = c_int::try_from(scan.int()?).ok()?;
    Some((w, h))
}

pub fn scan_pair(s: &[u8]) -> Option<(f64, f64)> {
    let mut scan = Scanner::new(s);
    Some((scan.float()?, scan.float()?))
}

pub fn scan_hold(s: &[u8]) -> Option<(f64, f64, c_long)> {
    let mut scan = Scanner::new(s);
    let (x, y) = scan.point()?;
    let ms = c_long::try_from(scan.int()?).ok()?;
    Some((x, y, ms))
}

pub fn scan_drag(s: &[u8]) -> Option<[f64; 4]> {
    let mut scan = Scanner::new(s);
    let (a, b) = scan.point()?;
    let (c, d) = scan.point()?;
    Some([a, b, c, d])
}
