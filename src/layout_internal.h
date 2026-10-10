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
void ns_layout_flex_row(ns_box *box, double cw, double inner_x, double inner_y,
                        const ns_style *child_inherited, gboolean reverse,
                        double parent_content_width, double *cursor_y_out);
void ns_layout_flex_row_wrap(ns_box *box, double cw, double inner_x,
                             double inner_y, const ns_style *child_inherited,
                             gboolean reverse, double *cursor_y_out);
void ns_layout_flex_column(ns_box *box, double cw, double inner_x,
                           double inner_y, const ns_style *child_inherited,
                           gboolean reverse, double parent_content_height,
                           double *cursor_y_out);
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
double ns_layout_resolve_height_with_basis(const ns_css_value *hv,
                                           double width_basis,
                                           double height_basis, double fallback);
double ns_layout_containing_block_definite_height(const ns_box *box);
gboolean ns_layout_size_keyword_is_intrinsic(const ns_css_value *v);
gboolean ns_layout_height_keyword_stretches(const ns_css_value *v);
double ns_layout_intrinsic_keyword_width(ns_box *box, const char *kw,
                                         const ns_style *mi, double avail);
gboolean ns_layout_box_is_scroll_container(const ns_box *b);
double ns_layout_box_read_definite_height(const ns_box *box);
gboolean ns_layout_style_is_absolute_or_fixed(const ns_style *s);
gboolean ns_layout_style_is_flex_container(const ns_style *s);
const char *ns_layout_keyword_or(const ns_style *s, ns_css_prop p,
                                 const char *fallback);
const char *ns_layout_overflow_axis_keyword(const ns_style *s, ns_css_prop axis);
gboolean ns_layout_overflow_kw_scrolls(const char *ov);
double ns_layout_aspect_ratio_number(const ns_css_value *v, gboolean *with_auto);
double ns_layout_gap_px(const ns_css_value *specific,
                        const ns_css_value *shorthand, double basis);
gboolean ns_layout_flex_box_is_border_box(const ns_box *c);
double ns_layout_flex_grow_of(const ns_box *c);
double ns_layout_flex_shrink_of(const ns_box *c);
double ns_layout_flex_gap_of(const ns_style *s, double basis);
gboolean ns_layout_flex_wraps(const ns_style *s);
const char *ns_layout_flex_item_align(const ns_box *c,
                                      const char *container_align);
gboolean ns_layout_flex_align_is_baseline(const char *align);
double ns_layout_flex_item_baseline(const ns_box *c, double fallback);
double ns_layout_specified_height_to_content(const ns_box *b, double h);
double ns_layout_clamp_height_minmax_px(const ns_style *s, double h);
gboolean ns_layout_overflow_establishes_bfc(const ns_style *s);
gboolean ns_layout_self_start_is_far_side(const ns_style *s,
                                          gboolean horizontal_axis);
void ns_layout_grid(ns_box *box, double cw, double inner_x, double inner_y,
                    const ns_style *child_inherited, double *cursor_y_out);
gboolean ns_layout_grid_flows_by_column(const ns_style *s);
double ns_layout_grid_column_flow_width(ns_box *box, const ns_style *child_style,
                                        gboolean min_content);
double ns_layout_grid_natural_width(ns_box *box, const ns_style *child_style);
gboolean ns_layout_grid_abs_containing_block(const ns_box *cb, const ns_style *st,
                                             double *x, double *y,
                                             double *w, double *h);
void ns_layout_grid_static_position(ns_box *abox, const ns_box *cb,
                                    double area_x, double area_y,
                                    double area_w, double area_h,
                                    gboolean static_x, gboolean static_y);
double ns_layout_grid_static_align_offset(const char *align, double free_space,
                                          gboolean flip);
gboolean ns_layout_style_blocks_hit_testing(const ns_style *s);
gboolean ns_layout_node_is_form_hit_target(const ns_node *n);
const ns_node *ns_layout_inline_box_form_hit(const ns_box *box, double local_x,
                                             double local_y,
                                             const ns_style *parent_style);
gboolean ns_layout_box_clips_children(const ns_box *b);
gboolean ns_layout_style_creates_fixed_cb(const ns_style *s);

#endif
