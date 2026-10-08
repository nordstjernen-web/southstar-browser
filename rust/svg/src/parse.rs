//! Southstar — SVG attribute syntax: numbers, lengths, url() references, transforms, dash arrays and path data.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use core::ffi::CStr;

use crate::ffi::{Canvas, Matrix, strtod, uri_unescape};

const MAX_DASHES: usize = 256;

pub fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c)
}

pub struct Cursor<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(s: &'a CStr) -> Cursor<'a> {
        Cursor {
            s: s.to_bytes_with_nul(),
            pos: 0,
        }
    }

    pub fn peek(&self) -> u8 {
        self.s[self.pos]
    }

    pub fn bump(&mut self) {
        if self.peek() != 0 {
            self.pos += 1;
        }
    }

    pub fn skip_sep(&mut self) {
        while self.peek() != 0 && (is_ws(self.peek()) || self.peek() == b',') {
            self.pos += 1;
        }
    }

    pub fn num(&mut self) -> Option<f64> {
        self.skip_sep();
        if self.peek() == 0 {
            return None;
        }
        let (v, used) = strtod(self.s, self.pos);
        if used == 0 || !v.is_finite() {
            return None;
        }
        self.pos += used;
        Some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip_sep();
        let f = match self.peek() {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.pos += 1;
        Some(f)
    }

    fn rest(&self) -> &'a [u8] {
        &self.s[self.pos..self.s.len() - 1]
    }
}

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub fn eq_ci(s: &[u8], other: &[u8]) -> bool {
    s.eq_ignore_ascii_case(other)
}

pub fn length(s: Option<&CStr>, pct_basis: f64, font_size: f64, fallback: f64) -> f64 {
    let Some(s) = s else {
        return fallback;
    };
    let mut c = Cursor::new(s);
    while c.peek() != 0 && is_ws(c.peek()) {
        c.bump();
    }
    if c.peek() == 0 {
        return fallback;
    }
    let (v, used) = strtod(c.s, c.pos);
    if used == 0 || !v.is_finite() {
        return fallback;
    }
    c.pos += used;
    while c.peek() != 0 && is_ws(c.peek()) {
        c.bump();
    }
    let unit = c.rest();
    if unit.first() == Some(&b'%') {
        return v / 100.0 * pct_basis;
    }
    let factor = [
        (&b"px"[..], 1.0),
        (b"pt", 4.0 / 3.0),
        (b"pc", 16.0),
        (b"mm", 96.0 / 25.4),
        (b"cm", 96.0 / 2.54),
        (b"in", 96.0),
    ];
    for (name, k) in factor {
        if starts_ci(unit, name) {
            return if name == b"px" { v } else { v * k };
        }
    }
    if starts_ci(unit, b"em") {
        return v * font_size;
    }
    if starts_ci(unit, b"ex") {
        return v * font_size * 0.5;
    }
    if starts_ci(unit, b"rem") {
        return v * 16.0;
    }
    v
}

pub fn url_id(s: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut p = 0;
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    while at(p) != 0 && is_ws(at(p)) {
        p += 1;
    }
    if !starts_ci(&s[p..], b"url(") {
        return None;
    }
    p += 4;
    while at(p) != 0 && is_ws(at(p)) {
        p += 1;
    }
    let mut quote = 0;
    if at(p) == b'"' || at(p) == b'\'' {
        quote = at(p);
        p += 1;
    }
    let mut start = p;
    while at(p) != 0 && at(p) != b')' && (quote == 0 || at(p) != quote) {
        p += 1;
    }
    let mut end = p;
    if quote != 0 && at(p) == quote {
        p += 1;
    }
    while at(p) != 0 && at(p) != b')' {
        p += 1;
    }
    if at(p) == b')' {
        p += 1;
    }
    let rest = p;
    while end > start && is_ws(s[end - 1]) {
        end -= 1;
    }
    if end > start && s[start] == b'#' {
        start += 1;
    }
    if end <= start {
        return None;
    }
    let raw = &s[start..end];
    if !raw.contains(&b'%') {
        return Some((raw.to_vec(), rest));
    }
    Some((uri_unescape(raw).unwrap_or_else(|| raw.to_vec()), rest))
}

pub fn dashes(text: &CStr) -> Option<Vec<f64>> {
    if eq_ci(text.to_bytes(), b"none") {
        return None;
    }
    let mut out = Vec::new();
    let mut c = Cursor::new(text);
    let mut any_positive = false;
    while let Some(v) = c.num() {
        while c.peek() != 0
            && !is_ws(c.peek())
            && !matches!(c.peek(), b',' | b'-' | b'+' | b'.')
            && !c.peek().is_ascii_digit()
        {
            c.bump();
        }
        if v < 0.0 {
            return None;
        }
        if v > 0.0 {
            any_positive = true;
        }
        out.push(v);
        if out.len() >= MAX_DASHES {
            break;
        }
    }
    if out.is_empty() || !any_positive {
        return None;
    }
    if out.len() % 2 == 1 {
        out.extend_from_within(..);
    }
    Some(out)
}

pub fn transform(s: Option<&CStr>) -> (Matrix, bool) {
    let mut out = Matrix::identity();
    let Some(s) = s else {
        return (out, false);
    };
    let mut any = false;
    let mut c = Cursor::new(s);
    loop {
        c.skip_sep();
        if c.peek() == 0 {
            break;
        }
        let name_start = c.pos;
        while c.peek().is_ascii_alphabetic() {
            c.bump();
        }
        let name = &c.s[name_start..c.pos];
        if name.is_empty() {
            break;
        }
        while c.peek() != 0 && is_ws(c.peek()) {
            c.bump();
        }
        if c.peek() != b'(' {
            break;
        }
        c.bump();
        let mut a = [0.0f64; 6];
        let mut n = 0;
        while n < 6 {
            match c.num() {
                Some(v) => {
                    a[n] = v;
                    n += 1;
                }
                None => break,
            }
        }
        while c.peek() != 0 && c.peek() != b')' {
            c.bump();
        }
        if c.peek() == b')' {
            c.bump();
        }
        let m = if eq_ci(name, b"matrix") && n == 6 {
            Matrix::new(a[0], a[1], a[2], a[3], a[4], a[5])
        } else if eq_ci(name, b"translate") && n >= 1 {
            Matrix::translation(a[0], if n >= 2 { a[1] } else { 0.0 })
        } else if eq_ci(name, b"scale") && n >= 1 {
            Matrix::scaling(a[0], if n >= 2 { a[1] } else { a[0] })
        } else if eq_ci(name, b"rotate") && n >= 1 {
            if n >= 3 {
                let mut m = Matrix::translation(a[1], a[2]);
                m.rotate(a[0] * PI / 180.0);
                m.translate(-a[1], -a[2]);
                m
            } else {
                Matrix::rotation(a[0] * PI / 180.0)
            }
        } else if eq_ci(name, b"skewX") && n >= 1 {
            Matrix::new(1.0, 0.0, (a[0] * PI / 180.0).tan(), 1.0, 0.0, 0.0)
        } else if eq_ci(name, b"skewY") && n >= 1 {
            Matrix::new(1.0, (a[0] * PI / 180.0).tan(), 0.0, 1.0, 0.0, 0.0)
        } else {
            continue;
        };
        out = Matrix::multiply(&m, &out);
        any = true;
    }
    (out, any)
}

pub fn viewbox(s: Option<&CStr>) -> Option<[f64; 4]> {
    let mut c = Cursor::new(s?);
    Some([c.num()?, c.num()?, c.num()?, c.num()?])
}

pub fn viewbox_size(s: Option<&CStr>) -> Option<(f64, f64)> {
    let [_, _, w, h] = viewbox(s)?;
    (w > 0.0 && h > 0.0).then_some((w, h))
}

#[allow(clippy::too_many_arguments)]
fn arc_to(
    cr: Canvas,
    x1: f64,
    y1: f64,
    mut rx: f64,
    mut ry: f64,
    phi_deg: f64,
    large_arc: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) {
    if x1 == x2 && y1 == y2 {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        cr.line_to(x2, y2);
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = phi_deg * PI / 180.0;
    let (cosp, sinp) = (phi.cos(), phi.sin());
    let (dx2, dy2) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let x1p = cosp * dx2 + sinp * dy2;
    let y1p = -sinp * dx2 + cosp * dy2;

    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        let k = lam.sqrt();
        rx *= k;
        ry *= k;
    }

    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let num = rx * rx * ry * ry - den;
    let mut co = 0.0;
    if den > 0.0 && num > 0.0 {
        co = (num / den).sqrt();
    }
    if large_arc == sweep {
        co = -co;
    }

    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cosp * cxp - sinp * cyp + (x1 + x2) / 2.0;
    let cy = sinp * cxp + cosp * cyp + (y1 + y2) / 2.0;

    let t1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
    let t2 = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
    let mut dt = t2 - t1;
    if !sweep && dt > 0.0 {
        dt -= 2.0 * PI;
    } else if sweep && dt < 0.0 {
        dt += 2.0 * PI;
    }

    let quarters = (dt.abs() / (PI / 2.0) - 1e-9).ceil();
    let segs = if quarters > 1.0 {
        (if quarters < 8.0 { quarters } else { 8.0 }) as i32
    } else {
        1
    };
    let seg = dt / f64::from(segs);
    let alpha = 4.0 / 3.0 * (seg / 4.0).tan();

    for i in 0..segs {
        let a1 = t1 + f64::from(i) * seg;
        let a2 = a1 + seg;
        let (ca1, sa1) = (a1.cos(), a1.sin());
        let (ca2, sa2) = (a2.cos(), a2.sin());
        let px1 = cx + rx * cosp * ca1 - ry * sinp * sa1;
        let py1 = cy + rx * sinp * ca1 + ry * cosp * sa1;
        let px2 = cx + rx * cosp * ca2 - ry * sinp * sa2;
        let py2 = cy + rx * sinp * ca2 + ry * cosp * sa2;
        let d1x = -rx * cosp * sa1 - ry * sinp * ca1;
        let d1y = -rx * sinp * sa1 + ry * cosp * ca1;
        let d2x = -rx * cosp * sa2 - ry * sinp * ca2;
        let d2y = -rx * sinp * sa2 + ry * cosp * ca2;
        cr.curve_to(
            px1 + alpha * d1x,
            py1 + alpha * d1y,
            px2 - alpha * d2x,
            py2 - alpha * d2y,
            px2,
            py2,
        );
    }
}

struct Pen {
    cr: Canvas,
    cx: f64,
    cy: f64,
    sx: f64,
    sy: f64,
    started: bool,
}

impl Pen {
    fn start_at(&mut self, x: f64, y: f64) {
        if !self.started {
            self.cr.move_to(x, y);
            self.sx = x;
            self.sy = y;
            self.started = true;
        }
    }

    fn line(&mut self) {
        if self.started {
            self.cr.line_to(self.cx, self.cy);
        } else {
            self.start_at(self.cx, self.cy);
        }
    }
}

fn nums<const N: usize>(c: &mut Cursor<'_>) -> Option<[f64; N]> {
    let mut a = [0.0; N];
    for v in &mut a {
        *v = c.num()?;
    }
    Some(a)
}

pub fn path_data(cr: Canvas, d: &CStr) {
    let mut c = Cursor::new(d);
    let mut pen = Pen {
        cr,
        cx: 0.0,
        cy: 0.0,
        sx: 0.0,
        sy: 0.0,
        started: false,
    };
    let (mut ctrl_x, mut ctrl_y) = (0.0, 0.0);
    let (mut cmd, mut prev) = (0u8, 0u8);
    loop {
        c.skip_sep();
        if c.peek() == 0 {
            break;
        }
        if c.peek().is_ascii_alphabetic() {
            cmd = c.peek();
            c.bump();
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        } else if cmd == 0 {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (pen.cx, pen.cy) } else { (0.0, 0.0) };
        if path_command(
            &mut c,
            &mut pen,
            cmd.to_ascii_uppercase(),
            rel,
            (ox, oy),
            prev,
            &mut ctrl_x,
            &mut ctrl_y,
        )
        .is_none()
        {
            return;
        }
        prev = cmd;
    }
}

#[allow(clippy::too_many_arguments)]
fn path_command(
    c: &mut Cursor<'_>,
    pen: &mut Pen,
    cmd: u8,
    rel: bool,
    (ox, oy): (f64, f64),
    prev: u8,
    ctrl_x: &mut f64,
    ctrl_y: &mut f64,
) -> Option<()> {
    let cr = pen.cr;
    match cmd {
        b'M' => {
            let [mut x, mut y] = nums::<2>(c)?;
            if rel && pen.started {
                x += pen.cx;
                y += pen.cy;
            }
            pen.cx = x;
            pen.cy = y;
            pen.sx = x;
            pen.sy = y;
            cr.move_to(x, y);
            pen.started = true;
        }
        b'L' => {
            let [x, y] = nums::<2>(c)?;
            pen.cx = if rel { x + ox } else { x };
            pen.cy = if rel { y + oy } else { y };
            pen.line();
        }
        b'H' => {
            let [x] = nums::<1>(c)?;
            pen.cx = if rel { x + ox } else { x };
            pen.line();
        }
        b'V' => {
            let [y] = nums::<1>(c)?;
            pen.cy = if rel { y + oy } else { y };
            pen.line();
        }
        b'C' => {
            let mut a = nums::<6>(c)?;
            if rel {
                for (i, v) in a.iter_mut().enumerate() {
                    *v += if i % 2 == 0 { ox } else { oy };
                }
            }
            pen.start_at(a[0], a[1]);
            cr.curve_to(a[0], a[1], a[2], a[3], a[4], a[5]);
            *ctrl_x = a[2];
            *ctrl_y = a[3];
            pen.cx = a[4];
            pen.cy = a[5];
        }
        b'S' => {
            let mut a = nums::<4>(c)?;
            if rel {
                for (i, v) in a.iter_mut().enumerate() {
                    *v += if i % 2 == 0 { ox } else { oy };
                }
            }
            let pc = prev.to_ascii_uppercase();
            let smooth = pc == b'C' || pc == b'S';
            let rx = if smooth {
                2.0 * pen.cx - *ctrl_x
            } else {
                pen.cx
            };
            let ry = if smooth {
                2.0 * pen.cy - *ctrl_y
            } else {
                pen.cy
            };
            pen.start_at(rx, ry);
            cr.curve_to(rx, ry, a[0], a[1], a[2], a[3]);
            *ctrl_x = a[0];
            *ctrl_y = a[1];
            pen.cx = a[2];
            pen.cy = a[3];
        }
        b'Q' => {
            let mut a = nums::<4>(c)?;
            if rel {
                for (i, v) in a.iter_mut().enumerate() {
                    *v += if i % 2 == 0 { ox } else { oy };
                }
            }
            pen.start_at(pen.cx, pen.cy);
            let (cx, cy) = (pen.cx, pen.cy);
            let c1x = cx + 2.0 / 3.0 * (a[0] - cx);
            let c1y = cy + 2.0 / 3.0 * (a[1] - cy);
            let c2x = a[2] + 2.0 / 3.0 * (a[0] - a[2]);
            let c2y = a[3] + 2.0 / 3.0 * (a[1] - a[3]);
            cr.curve_to(c1x, c1y, c2x, c2y, a[2], a[3]);
            *ctrl_x = a[0];
            *ctrl_y = a[1];
            pen.cx = a[2];
            pen.cy = a[3];
        }
        b'T' => {
            let [mut x, mut y] = nums::<2>(c)?;
            if rel {
                x += ox;
                y += oy;
            }
            let pc = prev.to_ascii_uppercase();
            let smooth = pc == b'Q' || pc == b'T';
            let (cx, cy) = (pen.cx, pen.cy);
            let qx = if smooth { 2.0 * cx - *ctrl_x } else { cx };
            let qy = if smooth { 2.0 * cy - *ctrl_y } else { cy };
            pen.start_at(cx, cy);
            let c1x = cx + 2.0 / 3.0 * (qx - cx);
            let c1y = cy + 2.0 / 3.0 * (qy - cy);
            let c2x = x + 2.0 / 3.0 * (qx - x);
            let c2y = y + 2.0 / 3.0 * (qy - y);
            cr.curve_to(c1x, c1y, c2x, c2y, x, y);
            *ctrl_x = qx;
            *ctrl_y = qy;
            pen.cx = x;
            pen.cy = y;
        }
        b'A' => {
            let [rx, ry, phi] = nums::<3>(c)?;
            let large = c.flag()?;
            let sweep = c.flag()?;
            let [mut x, mut y] = nums::<2>(c)?;
            if rel {
                x += ox;
                y += oy;
            }
            pen.start_at(pen.cx, pen.cy);
            arc_to(cr, pen.cx, pen.cy, rx, ry, phi, large, sweep, x, y);
            pen.cx = x;
            pen.cy = y;
        }
        b'Z' => {
            if pen.started {
                cr.close_path();
            }
            pen.cx = pen.sx;
            pen.cy = pen.sy;
        }
        _ => return None,
    }
    Some(())
}

pub fn points(s: &CStr) -> Vec<(f64, f64)> {
    let mut c = Cursor::new(s);
    let mut out = Vec::new();
    while let Some(x) = c.num() {
        let Some(y) = c.num() else {
            break;
        };
        out.push((x, y));
    }
    out
}

pub fn atoi(s: &[u8]) -> i64 {
    let mut i = 0;
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let neg = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut v: i64 = 0;
    while let Some(d) = s.get(i).filter(|d| d.is_ascii_digit()) {
        v = v.saturating_mul(10).saturating_add(i64::from(d - b'0'));
        i += 1;
    }
    if neg { -v } else { v }
}
