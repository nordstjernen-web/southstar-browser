//! Southstar text — the 2D affine matrix fonts and layouts are transformed with, and the scale and slant read from it.
//! Copyright 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub xx: f64,
    pub xy: f64,
    pub yx: f64,
    pub yy: f64,
    pub x0: f64,
    pub y0: f64,
}

impl Default for Matrix {
    fn default() -> Matrix {
        Matrix::IDENTITY
    }
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        xx: 1.0,
        xy: 0.0,
        yx: 0.0,
        yy: 1.0,
        x0: 0.0,
        y0: 0.0,
    };

    pub fn concat(&mut self, new_matrix: &Matrix) {
        let tmp = *self;
        self.xx = tmp.xx * new_matrix.xx + tmp.xy * new_matrix.yx;
        self.xy = tmp.xx * new_matrix.xy + tmp.xy * new_matrix.yy;
        self.yx = tmp.yx * new_matrix.xx + tmp.yy * new_matrix.yx;
        self.yy = tmp.yx * new_matrix.xy + tmp.yy * new_matrix.yy;
        self.x0 = tmp.xx * new_matrix.x0 + tmp.xy * new_matrix.y0 + tmp.x0;
        self.y0 = tmp.yx * new_matrix.x0 + tmp.yy * new_matrix.y0 + tmp.y0;
    }

    pub fn transform_distance(&self, dx: &mut f64, dy: &mut f64) {
        let new_x = self.xx * *dx + self.xy * *dy;
        let new_y = self.yx * *dx + self.yy * *dy;
        *dx = new_x;
        *dy = new_y;
    }

    pub fn font_scale_factor(&self) -> f64 {
        self.font_scale_factors().1
    }

    pub fn font_scale_factors(&self) -> (f64, f64) {
        let x = self.xx;
        let y = self.yx;
        let major = (x * x + y * y).sqrt();
        let minor = if major != 0.0 {
            let det = self.xx * self.yy - self.yx * self.xy;
            let unmirrored = if det < 0.0 { -det } else { det };
            unmirrored / major
        } else {
            0.0
        };
        (major, minor)
    }

    pub fn slant_ratio(&self) -> f64 {
        let (mut x0, mut y0) = (0.0, 1.0);
        self.transform_distance(&mut x0, &mut y0);
        let (mut x1, mut y1) = (1.0, 0.0);
        self.transform_distance(&mut x1, &mut y1);
        (x0 * x1 + y0 * y1) / (x0 * x0 + y0 * y0)
    }
}

pub fn matrix_transform_distance(matrix: Option<&Matrix>, dx: &mut f64, dy: &mut f64) {
    if let Some(matrix) = matrix {
        matrix.transform_distance(dx, dy);
    }
}

pub fn matrix_get_font_scale_factor(matrix: Option<&Matrix>) -> f64 {
    matrix_get_font_scale_factors(matrix).1
}

pub fn matrix_get_font_scale_factors(matrix: Option<&Matrix>) -> (f64, f64) {
    matrix.map_or((1.0, 1.0), Matrix::font_scale_factors)
}

pub fn matrix_get_slant_ratio(matrix: Option<&Matrix>) -> f64 {
    matrix.map_or(0.0, Matrix::slant_ratio)
}
