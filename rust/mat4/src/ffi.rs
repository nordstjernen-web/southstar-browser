//! Southstar — the C ABI of the 4x4 transform matrices, as declared in src/mat4.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::Mat4;
use core::ffi::c_int;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_identity(out: *mut Mat4) {
    unsafe { *out = Mat4::IDENTITY };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_multiply(a: *const Mat4, b: *const Mat4, out: *mut Mat4) {
    unsafe { *out = (*a).multiply(&*b) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_translate(m: *mut Mat4, x: f64, y: f64, z: f64) {
    unsafe { (*m).translate(x, y, z) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_scale(m: *mut Mat4, x: f64, y: f64, z: f64) {
    unsafe { (*m).scale(x, y, z) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_rotate_axis(m: *mut Mat4, x: f64, y: f64, z: f64, deg: f64) {
    unsafe { (*m).rotate_axis(x, y, z, deg) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_skew(m: *mut Mat4, ax_deg: f64, ay_deg: f64) {
    unsafe { (*m).skew(ax_deg, ay_deg) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_affine2d(
    m: *mut Mat4,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) {
    unsafe { (*m).affine2d(a, b, c, d, e, f) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_perspective(m: *mut Mat4, d: f64) {
    unsafe { (*m).perspective(d) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_apply(
    m: *const Mat4,
    x: f64,
    y: f64,
    z: f64,
    ox: *mut f64,
    oy: *mut f64,
    oz: *mut f64,
    ow: *mut f64,
) {
    let [rx, ry, rz, rw] = unsafe { (*m).apply(x, y, z) };
    unsafe {
        *ox = rx;
        *oy = ry;
        *oz = rz;
        *ow = rw;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mat4_is_affine2d(m: *const Mat4) -> c_int {
    c_int::from(unsafe { (*m).is_affine2d() })
}
