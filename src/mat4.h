/* Southstar — 4x4 transform matrices for CSS 3D rendering, implemented in rust/mat4.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_MAT4_H
#define NS_MAT4_H

typedef struct ns_mat4 {
    double m[16];
} ns_mat4;

void ns_mat4_identity(ns_mat4 *out);
void ns_mat4_multiply(const ns_mat4 *a, const ns_mat4 *b, ns_mat4 *out);
void ns_mat4_translate(ns_mat4 *m, double x, double y, double z);
void ns_mat4_scale(ns_mat4 *m, double x, double y, double z);
void ns_mat4_rotate_axis(ns_mat4 *m, double x, double y, double z, double deg);
void ns_mat4_skew(ns_mat4 *m, double ax_deg, double ay_deg);
void ns_mat4_affine2d(ns_mat4 *m, double a, double b, double c, double d,
                      double e, double f);
void ns_mat4_perspective(ns_mat4 *m, double d);
void ns_mat4_apply(const ns_mat4 *m, double x, double y, double z,
                   double *ox, double *oy, double *oz, double *ow);
int  ns_mat4_is_affine2d(const ns_mat4 *m);

#endif
