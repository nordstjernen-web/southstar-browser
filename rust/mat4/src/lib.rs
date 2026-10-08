//! Southstar — 4x4 transform matrices for CSS 3D rendering.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::f64::consts::PI;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Mat4 {
    pub m: [f64; 16],
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    pub fn multiply(&self, b: &Mat4) -> Mat4 {
        let mut r = Mat4 { m: [0.0; 16] };
        for row in 0..4 {
            for col in 0..4 {
                let mut s = 0.0;
                for k in 0..4 {
                    s += self.m[row * 4 + k] * b.m[k * 4 + col];
                }
                r.m[row * 4 + col] = s;
            }
        }
        r
    }

    fn then(&mut self, set: impl FnOnce(&mut [f64; 16])) {
        let mut t = Mat4::IDENTITY;
        set(&mut t.m);
        *self = self.multiply(&t);
    }

    pub fn translate(&mut self, x: f64, y: f64, z: f64) {
        self.then(|t| {
            t[3] = x;
            t[7] = y;
            t[11] = z;
        });
    }

    pub fn scale(&mut self, x: f64, y: f64, z: f64) {
        self.then(|t| {
            t[0] = x;
            t[5] = y;
            t[10] = z;
        });
    }

    pub fn rotate_axis(&mut self, x: f64, y: f64, z: f64, deg: f64) {
        let len = (x * x + y * y + z * z).sqrt();
        if len < 1e-12 {
            return;
        }
        let (x, y, z) = (x / len, y / len, z / len);
        let rad = deg * PI / 180.0;
        let (mut c, mut s) = (rad.cos(), rad.sin());
        let quarters = deg / 90.0;
        if quarters.is_finite() && quarters == quarters.round_ties_even() {
            let q = (((quarters % 4.0) as i32) + 4) % 4;
            c = match q {
                0 => 1.0,
                2 => -1.0,
                _ => 0.0,
            };
            s = match q {
                1 => 1.0,
                3 => -1.0,
                _ => 0.0,
            };
        }
        let ic = 1.0 - c;
        self.then(|t| {
            t[0] = c + x * x * ic;
            t[1] = x * y * ic - z * s;
            t[2] = x * z * ic + y * s;
            t[4] = y * x * ic + z * s;
            t[5] = c + y * y * ic;
            t[6] = y * z * ic - x * s;
            t[8] = z * x * ic - y * s;
            t[9] = z * y * ic + x * s;
            t[10] = c + z * z * ic;
        });
    }

    pub fn skew(&mut self, ax_deg: f64, ay_deg: f64) {
        self.then(|t| {
            t[1] = (ax_deg * PI / 180.0).tan();
            t[4] = (ay_deg * PI / 180.0).tan();
        });
    }

    pub fn affine2d(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.then(|t| {
            t[0] = a;
            t[1] = c;
            t[3] = e;
            t[4] = b;
            t[5] = d;
            t[7] = f;
        });
    }

    pub fn perspective(&mut self, d: f64) {
        if d <= 0.0 {
            return;
        }
        self.then(|t| t[14] = -1.0 / d);
    }

    pub fn apply(&self, x: f64, y: f64, z: f64) -> [f64; 4] {
        let m = &self.m;
        [
            m[0] * x + m[1] * y + m[2] * z + m[3],
            m[4] * x + m[5] * y + m[6] * z + m[7],
            m[8] * x + m[9] * y + m[10] * z + m[11],
            m[12] * x + m[13] * y + m[14] * z + m[15],
        ]
    }

    pub fn is_affine2d(&self) -> bool {
        let e = 1e-9;
        let m = &self.m;
        m[2].abs() < e
            && m[6].abs() < e
            && m[8].abs() < e
            && m[9].abs() < e
            && (m[10] - 1.0).abs() < e
            && m[11].abs() < e
            && m[12].abs() < e
            && m[13].abs() < e
            && m[14].abs() < e
            && (m[15] - 1.0).abs() < e
    }
}
