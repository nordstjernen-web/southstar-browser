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
double ns_layout_estimate_natural_width(const ns_box *b, double cap);
const char *ns_layout_flex_direction_of(const ns_style *s);
void ns_layout_box_append_child(ns_box *parent, ns_box *child);
const ns_node *ns_layout_flat_parent(const ns_node *n);
guint ns_layout_abs_pending_len(void);
gboolean ns_layout_abs_pending_entry(guint i, const ns_node **dom,
                                     const ns_style **pseudo, gboolean *fixed);
void ns_layout_abs_pending_clear(void);
ns_box *ns_layout_abs_static_run(const ns_node *dom, double *rel_x,
                                 double *rel_y);
ns_box *ns_layout_abs_build_box(const ns_node *dom, const ns_style *pseudo,
                                GHashTable *styles);
void ns_layout_apply_position_offsets(ns_box *box, double parent_w,
                                      double parent_h);
void ns_layout_process_absolute_boxes(ns_box *root, GHashTable *styles,
                                      double viewport_width);
void ns_layout_inline(ns_box *box, double content_width,
                      const ns_style *parent_style);
double ns_layout_inline_line_height(const ns_style *parent_style);
double ns_layout_inline_natural_width(ns_box *box, const ns_style *parent_style);
double ns_layout_inline_min_width(ns_box *box, const ns_style *parent_style);
void ns_layout_apply_inline_spacing(struct _PangoAttrList *list,
                                    const ns_style *style,
                                    const char *text);
void ns_layout_apply_inline_layout_attrs(struct _PangoAttrList *list,
                                         const ns_box *box);
gboolean ns_layout_box_first_baseline(const ns_box *b, double *out);
double ns_layout_inline_attr_control_width(const ns_inline_attr *r,
                                           const ns_box *box);
gboolean ns_layout_box_is_abs_placeholder(const ns_box *b);
const ns_node *ns_layout_abs_static_target(const ns_box *b);
void ns_layout_record_abs_static(const ns_node *dom, ns_box *run, double rel_x,
                                 double rel_y);
void *ns_layout_measure_inline_atomics_begin(ns_box *box,
                                             const ns_style *parent_style,
                                             gboolean max_content);
void ns_layout_measure_inline_atomics_end(void *saved);
void ns_layout_compute_paint_bounds(ns_box *root);
void ns_layout_record_frame_viewports(const ns_box *root);
double ns_layout_image_dimension_attr(const ns_node *n, const char *name);
int ns_layout_float_side_of(const ns_style *s);
int ns_layout_clear_kind_of(const ns_style *s);
void ns_layout_image(ns_box *box, double parent_content_width);
double ns_layout_text_input_intrinsic_width(const ns_box *box);
void ns_layout_reset_container_queries(void);
void ns_layout_inline_runs_join_splits(ns_box *box);
void ns_layout_reorder_children_by_order(ns_box *box);
gboolean ns_layout_inline_box_can_fragment_multicol(const ns_box *box,
                                                    const ns_style *style,
                                                    double basis);
gboolean ns_layout_multicol_single_inline(ns_box *box, double inner_x,
                                          double inner_y, double col_w,
                                          double col_gap, int n_cols,
                                          const ns_style *child_inherited,
                                          double *cursor_y);
gsize ns_layout_inline_run_break_before(const ns_box *run, double band_height);
ns_box *ns_layout_inline_run_split(ns_box *run, gsize split);
void *ns_layout_peek_image(const char *src);
void ns_layout_append_inline_spacer(GString *out, GArray *attrs, double width);
int ns_layout_control_pad_spaces(const ns_style *s, gboolean right);
int ns_layout_text_input_leading_spaces(const ns_style *s);
int ns_layout_text_input_columns(const ns_node *n);
glong ns_layout_text_input_align_offset(const ns_style *s, const ns_node *n,
                                        glong span);
glong ns_layout_text_input_align_skip_cps(const ns_style *s, const ns_node *n,
                                          glong cps, glong size);
glong ns_layout_text_input_focused_skip_cps(const char *value, gsize caret_byte,
                                            glong cps, glong size);
const char *ns_layout_text_input_advance_cps(const char *p, glong cps);
gboolean ns_layout_input_columns_active(void);
gboolean ns_layout_input_columns_fit(const ns_box *root);
void ns_layout_input_columns_clear(void);
gboolean ns_layout_tag_is_bold(const char *name);
gboolean ns_layout_tag_is_italic(const char *name);
gboolean ns_layout_tag_is_monospace(const char *name);
gboolean ns_layout_tag_is_underline(const char *name);
gboolean ns_layout_tag_is_strike(const char *name);
gboolean ns_layout_tag_is_non_rendering(const char *name);
gboolean ns_layout_node_is_non_rendering(const ns_node *n);
void ns_layout_emit_attr(GArray *attrs, ns_inline_attr_kind k, gsize start,
                         gsize end);
void ns_layout_emit_attr_styled(GArray *attrs, ns_inline_attr_kind k,
                                gsize start, gsize end, const ns_style *style);
gboolean ns_layout_inline_run_at_line_start(const GString *out);
void ns_layout_emit_form_attr_sized(GArray *attrs, ns_inline_attr_kind k,
                                    gsize start, gsize end, const ns_node *dom,
                                    GHashTable *styles);
void ns_layout_emit_control_text_style(GArray *attrs, const ns_style *s,
                                       gsize field_start, gsize field_end,
                                       gsize val_start, gsize val_end,
                                       gboolean is_placeholder,
                                       gboolean disabled);
void ns_layout_emit_font_size_attr(GArray *attrs, gsize start, gsize end,
                                   double font_size_px);
void ns_layout_emit_font_weight_attr(GArray *attrs, gsize start, gsize end,
                                     int font_weight);
void ns_layout_emit_font_stretch_attr(GArray *attrs, gsize start, gsize end,
                                      int font_stretch);
void ns_layout_emit_font_features_attr(GArray *attrs, gsize start, gsize end,
                                       int font_kerning,
                                       const char *font_ligatures,
                                       const char *font_features);
void ns_layout_emit_font_variations_attr(GArray *attrs, gsize start, gsize end,
                                         const char *font_variations);
void ns_layout_emit_color_attr(GArray *attrs, gsize start, gsize end,
                               guint8 r, guint8 g, guint8 b, guint8 a);
void ns_layout_emit_font_family_attr(GArray *attrs, gsize start, gsize end,
                                     const char *family);
int ns_layout_font_kerning_from_style(const ns_style *s);
const char *ns_layout_font_ligatures_from_style(const ns_style *s);
const char *ns_layout_font_feature_settings_from_style(const ns_style *s);
const char *ns_layout_font_variation_settings_from_style(const ns_style *s);
void ns_layout_counters_build(const ns_node *root, GHashTable *styles);
void ns_layout_counters_clear(void);
char *ns_layout_resolve_pseudo_content(const char *raw, const ns_node *host);
void ns_layout_append_pseudo_content(GString *out, const ns_css_value *cv,
                                     const ns_node *host,
                                     const ns_style *host_style);
gboolean ns_layout_pseudo_generates_box(const ns_style *ps);
char *ns_layout_quotes_string_for(const ns_style *s, int depth,
                                  gboolean closing);
char *ns_layout_apply_text_transform(const char *src, const char *tt);
double ns_layout_progress_fraction(const ns_node *n);
double ns_layout_meter_fraction(const ns_node *n, int *quality);
void ns_layout_emit_open_select_option(GString *out, GArray *attrs,
                                       GHashTable *styles,
                                       const ns_node *option);
void ns_layout_emit_listbox_option(GString *out, GArray *attrs,
                                   GHashTable *styles, const ns_node *option,
                                   gboolean first);
void ns_layout_emit_datalist_suggestions(GString *out, GArray *attrs,
                                         GHashTable *styles,
                                         const ns_node *input);

#endif
