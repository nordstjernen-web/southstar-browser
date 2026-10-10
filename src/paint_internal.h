/* Southstar — the calls between paint.c and rust/paint while paint.c is ported to Rust section by section.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_PAINT_INTERNAL_H
#define NS_PAINT_INTERNAL_H

#include "paint.h"
#include "texture.h"

G_BEGIN_DECLS

ns_js *ns_paint_js(void);
struct ns_anim *ns_paint_anim(void);

void ns_paint_block(cairo_t *cr, const ns_box *b);
void ns_paint_inline(cairo_t *cr, const ns_box *b, const char *highlight);
void ns_paint_image(cairo_t *cr, const ns_box *b);
void ns_paint_video(cairo_t *cr, const ns_box *b);
void ns_paint_math(cairo_t *cr, const ns_box *b);
void ns_paint_svg(cairo_t *cr, const ns_box *b);
void ns_paint_hr(cairo_t *cr, const ns_box *b);
void ns_paint_marker(cairo_t *cr, const ns_box *b);

gboolean ns_paint_box_content_clip(cairo_t *cr, const ns_box *b);
void ns_paint_box_radii_path(cairo_t *cr, const ns_box *b, double x, double y,
                             double w, double h);
void ns_paint_apply_image_filter(guchar *data, int stride, int w, int h,
                                 const char *filter);
gboolean ns_paint_filter_has_bitmap_effect(const char *filter);
gboolean ns_paint_mask_layers_paintable(const ns_style *s);
cairo_pattern_t *ns_paint_mask_layers_pattern(cairo_t *cr, const ns_box *b);

gboolean ns_paint_viewport_origin(double *x, double *y);
GHashTable *ns_paint_selection_runs(void);
int ns_paint_layers_mode(void);
void ns_paint_layers_note_video(cairo_t *cr);
void ns_paint_video_hole_record(cairo_t *cr, double x, double y, double w,
                                double h);
void ns_paint_walk_atomic(cairo_t *cr, const ns_box *box,
                          const char *highlight);

G_END_DECLS

#endif
