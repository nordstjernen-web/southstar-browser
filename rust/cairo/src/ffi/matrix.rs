//! Southstar — the matrix arithmetic done by cairo itself, so results match what its drawing calls compute bit for bit.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::types::{Error, Matrix};

impl Matrix {
    pub fn rotation(radians: f64) -> Matrix {
        let mut m = Matrix::default();
        // SAFETY: m is a writable local matrix.
        unsafe { super::cairo_matrix_init_rotate(&mut m, radians) };
        m
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        // SAFETY: self is a writable matrix.
        unsafe { super::cairo_matrix_translate(self, tx, ty) };
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        // SAFETY: self is a writable matrix.
        unsafe { super::cairo_matrix_scale(self, sx, sy) };
    }

    pub fn rotate(&mut self, radians: f64) {
        // SAFETY: self is a writable matrix.
        unsafe { super::cairo_matrix_rotate(self, radians) };
    }

    pub fn invert(&mut self) -> Result<(), Error> {
        // SAFETY: self is a writable matrix, left unchanged when it cannot be inverted.
        Error::check(unsafe { super::cairo_matrix_invert(self) })
    }

    pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
        let mut result = Matrix::default();
        // SAFETY: a and b are readable matrices and result a distinct writable local.
        unsafe { super::cairo_matrix_multiply(&mut result, a, b) };
        result
    }

    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        // SAFETY: self is a readable matrix, and x and y are writable locals.
        unsafe { super::cairo_matrix_transform_point(self, &mut x, &mut y) };
        (x, y)
    }
}
