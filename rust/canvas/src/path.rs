//! Southstar — SVG path data for the Path2D constructor, as move, line, cubic and elliptical-arc segments.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::f64::consts::PI;

use crate::color::is_space;
use crate::ffi;

pub(crate) trait PathSink {
    fn move_to(&mut self, x: f64, y: f64);
    fn line_to(&mut self, x: f64, y: f64);
    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64);
    fn close_path(&mut self);
    fn unit_arc(
        &mut self,
        center: (f64, f64),
        phi: f64,
        radii: (f64, f64),
        angles: (f64, f64),
        sweep: bool,
    );
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn skip_separators(&mut self) {
        while self.pos < self.d.len() && (is_space(self.d[self.pos]) || self.d[self.pos] == b',') {
            self.pos += 1;
        }
    }

    fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let (v, used) = ffi::strtod_prefix(&self.d[self.pos..]);
        if used == 0 {
            return None;
        }
        self.pos += used;
        Some(v)
    }

    fn numbers<const N: usize>(&mut self) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.number()?;
        }
        Some(out)
    }
}

pub(crate) struct Arc {
    pub from: (f64, f64),
    pub radii: (f64, f64),
    pub rotation_deg: f64,
    pub large_arc: bool,
    pub sweep: bool,
    pub to: (f64, f64),
}

pub(crate) fn arc(sink: &mut impl PathSink, a: &Arc) {
    let (x1, y1) = a.from;
    let (x2, y2) = a.to;
    let (mut rx, mut ry) = a.radii;
    if rx == 0.0 || ry == 0.0 {
        sink.line_to(x2, y2);
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    let phi = a.rotation_deg * PI / 180.0;
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let (mut rx2, mut ry2) = (rx * rx, ry * ry);
    let (x1p2, y1p2) = (x1p * x1p, y1p * y1p);
    let radii_check = x1p2 / rx2 + y1p2 / ry2;
    if radii_check > 1.0 {
        let s = radii_check.sqrt();
        rx *= s;
        ry *= s;
        rx2 = rx * rx;
        ry2 = ry * ry;
    }
    let sign = if a.large_arc == a.sweep { -1.0 } else { 1.0 };
    let denom = rx2 * y1p2 + ry2 * x1p2;
    let mut frac = (rx2 * ry2 - denom) / denom;
    if frac < 0.0 {
        frac = 0.0;
    }
    let factor = sign * frac.sqrt();
    let cxp = factor * (rx * y1p) / ry;
    let cyp = factor * -(ry * x1p) / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let a1 = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
    let mut a2 = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
    if a.sweep {
        if a2 < a1 {
            a2 += 2.0 * PI;
        }
    } else if a2 > a1 {
        a2 -= 2.0 * PI;
    }
    sink.unit_arc((cx, cy), phi, (rx, ry), (a1, a2), a.sweep);
}

fn quadratic(sink: &mut impl PathSink, from: (f64, f64), control: (f64, f64), to: (f64, f64)) {
    let (cx, cy) = from;
    let (x1, y1) = control;
    let (x, y) = to;
    sink.curve_to(
        cx + 2.0 / 3.0 * (x1 - cx),
        cy + 2.0 / 3.0 * (y1 - cy),
        x + 2.0 / 3.0 * (x1 - x),
        y + 2.0 / 3.0 * (y1 - y),
        x,
        y,
    );
}

pub(crate) fn parse(sink: &mut impl PathSink, d: &[u8]) {
    let mut r = Reader { d, pos: 0 };
    let (mut cx, mut cy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    let (mut last_cx, mut last_cy, mut last_qx, mut last_qy) = (0.0, 0.0, 0.0, 0.0);
    let (mut prev_cmd, mut cmd) = (0u8, 0u8);
    loop {
        r.skip_separators();
        let Some(&c) = r.d.get(r.pos) else {
            break;
        };
        if c.is_ascii_alphabetic() {
            cmd = c;
            r.pos += 1;
        } else {
            match cmd {
                0 | b'Z' | b'z' => break,
                b'M' => cmd = b'L',
                b'm' => cmd = b'l',
                _ => {}
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let offset = |x: f64, y: f64| if rel { (x + cx, y + cy) } else { (x, y) };
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let Some([x, y]) = r.numbers() else { return };
                let (x, y) = offset(x, y);
                sink.move_to(x, y);
                (cx, cy, sx, sy) = (x, y, x, y);
            }
            b'L' => {
                let Some([x, y]) = r.numbers() else { return };
                let (x, y) = offset(x, y);
                sink.line_to(x, y);
                (cx, cy) = (x, y);
            }
            b'H' => {
                let Some([x]) = r.numbers() else { return };
                let x = if rel { x + cx } else { x };
                sink.line_to(x, cy);
                cx = x;
            }
            b'V' => {
                let Some([y]) = r.numbers() else { return };
                let y = if rel { y + cy } else { y };
                sink.line_to(cx, y);
                cy = y;
            }
            b'Z' => {
                sink.close_path();
                (cx, cy) = (sx, sy);
            }
            b'C' => {
                let Some([x1, y1, x2, y2, x, y]) = r.numbers() else {
                    return;
                };
                let ((x1, y1), (x2, y2), (x, y)) = (offset(x1, y1), offset(x2, y2), offset(x, y));
                sink.curve_to(x1, y1, x2, y2, x, y);
                (last_cx, last_cy, cx, cy) = (x2, y2, x, y);
            }
            b'S' => {
                let Some([x2, y2, x, y]) = r.numbers() else {
                    return;
                };
                let ((x2, y2), (x, y)) = (offset(x2, y2), offset(x, y));
                let (x1, y1) = if matches!(prev_cmd, b'C' | b'c' | b'S' | b's') {
                    (2.0 * cx - last_cx, 2.0 * cy - last_cy)
                } else {
                    (cx, cy)
                };
                sink.curve_to(x1, y1, x2, y2, x, y);
                (last_cx, last_cy, cx, cy) = (x2, y2, x, y);
            }
            b'Q' => {
                let Some([x1, y1, x, y]) = r.numbers() else {
                    return;
                };
                let ((x1, y1), (x, y)) = (offset(x1, y1), offset(x, y));
                quadratic(sink, (cx, cy), (x1, y1), (x, y));
                (last_qx, last_qy, cx, cy) = (x1, y1, x, y);
            }
            b'T' => {
                let Some([x, y]) = r.numbers() else { return };
                let (x, y) = offset(x, y);
                let (x1, y1) = if matches!(prev_cmd, b'Q' | b'q' | b'T' | b't') {
                    (2.0 * cx - last_qx, 2.0 * cy - last_qy)
                } else {
                    (cx, cy)
                };
                quadratic(sink, (cx, cy), (x1, y1), (x, y));
                (last_qx, last_qy, cx, cy) = (x1, y1, x, y);
            }
            b'A' => {
                let Some([rx, ry, rot, large, sweep, x, y]) = r.numbers() else {
                    return;
                };
                let (x, y) = offset(x, y);
                arc(
                    sink,
                    &Arc {
                        from: (cx, cy),
                        radii: (rx, ry),
                        rotation_deg: rot,
                        large_arc: large != 0.0,
                        sweep: sweep != 0.0,
                        to: (x, y),
                    },
                );
                (cx, cy) = (x, y);
            }
            _ => return,
        }
        prev_cmd = cmd;
    }
}
