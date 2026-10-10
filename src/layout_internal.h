/* Southstar — the Rust functions only layout.c calls and the layout.c internals they call back.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_LAYOUT_INTERNAL_H
#define NS_LAYOUT_INTERNAL_H

#include "dom.h"
#include "layout.h"

char *ns_layout_choose_img_url(const ns_node *n, const ns_node **img_out,
                               double *density);
void ns_layout_table(ns_box *box, double parent_content_width,
                     const ns_style *inherited_style);
double ns_layout_table_intrinsic_width(ns_box *box, const ns_style *inherited,
                                       gboolean min);
double ns_layout_length_resolve(const ns_css_value *v, double basis,
                                double fallback);
gboolean ns_layout_value_is_percent(const ns_css_value *v);
void ns_layout_edges_from_style(const ns_style *s, double basis,
                                ns_edges *margin, ns_edges *padding,
                                ns_edges *border);
double ns_layout_resolve_used_height(const ns_box *box, const ns_css_value *hv,
                                     double width_basis, double fallback);
double ns_layout_min_width_of(ns_box *box, const ns_style *parent_style);
double ns_layout_measure_natural_width(ns_box *box,
                                       const ns_style *parent_style);
double ns_layout_min_content_width_of(ns_box *box,
                                      const ns_style *parent_style);
void ns_layout_box(ns_box *box, double parent_content_width,
                   const ns_style *inherited_style);
void ns_layout_block(ns_box *box, double parent_content_width,
                     const ns_style *inherited_style);
void ns_layout_legacy_align_block_child(ns_box *c, double avail_x,
                                        double avail_w,
                                        const ns_style *inherited);
void ns_layout_shift_box_tree(ns_box *b, double dx, double dy);
void ns_layout_translate_subtree(ns_box *box, double dx, double dy);

#endif
