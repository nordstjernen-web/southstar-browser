/* Southstar — the calls between paint.c and rust/paint while paint.c is ported to Rust section by section.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_PAINT_INTERNAL_H
#define NS_PAINT_INTERNAL_H

#include "paint.h"
#include "texture.h"

G_BEGIN_DECLS

NsPangoLayout *ns_paint_create_layout(void);
int ns_paint_pango_weight_from_css(int weight);
int ns_paint_pango_stretch_from_css(int rank);
gboolean ns_paint_style_is_nowrap(const ns_style *style);
void ns_paint_font_metrics(const char *family, double size_px, int weight,
                           gboolean italic, ns_css_font_metrics *out);
void ns_paint_apply_text_align(NsPangoLayout *layout, const ns_style *s);
void ns_paint_apply_nowrap_align_width(NsPangoLayout *layout,
                                       const ns_box *b);
const ns_style *ns_paint_inherited_style(const ns_box *b);
void ns_paint_marker(cairo_t *cr, const ns_box *b);

cairo_surface_t *ns_paint_texture_surface_cached(ns_texture *tex,
                                                 const char *filter_kw);

G_END_DECLS

#endif
