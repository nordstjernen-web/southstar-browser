/* Southstar — the functions of the css.c sections ported to Rust (rust/css) that only css.c calls. */
#ifndef NS_CSS_INTERNAL_H
#define NS_CSS_INTERNAL_H

#include "css.h"

gboolean      ns_css_parse_length(const char *text, double *out_v,
                                  ns_css_unit *out_unit);
ns_css_value *ns_css_parse_calc(const char *text);
gboolean      ns_css_resolve_to_px_pct(const char *text, gsize len,
                                       double *out_px, double *out_pct);
gboolean      ns_css_resolve_to_px_pct_font(const char *text, gsize len,
                                            double *out_px, double *out_pct,
                                            double *out_em, double *out_rem);
double        ns_css_viewport_resolve(double v, ns_css_unit unit);
const char   *ns_css_unit_suffix(int unit);
char         *ns_css_number_str(double n);
gboolean      ns_css_value_has_relative_unit(const char *s);
char         *ns_css_angle_expr_rewrite(const char *s, gboolean to_radians);

#define NS_CSS_CONTAINER_BYTES 40

double        ns_css_container_unit_resolve(double v, ns_css_unit unit);
void          ns_css_container_features_note(void);
guint64       ns_css_container_map_signature(void);
void          ns_css_container_stack_reset(void);
gboolean      ns_css_container_stack_push(const void *node);
void          ns_css_container_stack_pop(void);
gsize         ns_css_container_stack_copy(guint8 *out, gsize max_bytes);
gboolean      ns_css_container_rule_matches(const char *condition,
                                            ns_css_container_query **cache);
void          ns_css_container_query_free(ns_css_container_query *query);

char         *ns_css_color_text(guint8 r, guint8 g, guint8 b, guint8 a);
void          ns_css_append_color(GString *s, guint8 r, guint8 g, guint8 b,
                                  guint8 a);
gboolean      ns_css_wide_keyword_or_default(const char *item);
gboolean      ns_css_position_is_h_edge(const char *t);
gboolean      ns_css_position_is_v_edge(const char *t);
gboolean      ns_css_position_is_keyword(const char *t);
void          ns_css_position_split(const char *text, char **out_x,
                                    char **out_y);
char         *ns_css_position_canonical_ex(const char *text,
                                           gboolean expand_single,
                                           gboolean allow_three);
gboolean      ns_css_math_text_has_unit(const char *t,
                                        const char *const *units,
                                        gsize n_units);
double        ns_css_parse_angle_deg(const char *s);
gboolean      ns_css_text_starts_gradient(const char *t);
gboolean      ns_css_text_starts_image_set(const char *t);
ns_css_value *ns_css_parse_gradient(const char *t);
char         *ns_css_gradient_serialize(const ns_css_gradient *gr);
char         *ns_css_image_set_canonical(const char *text, gboolean computed);
char         *ns_css_content_symbols_canonical(const char *args);
gboolean      ns_css_content_ident_valid(const char *s);
const char   *ns_css_quoted_end(const char *u, char quote);
char         *ns_css_unescape_url(const char *u, gsize len);
char         *ns_css_pick_image_set_url(const char *t);

ns_css_value *ns_css_parse_transform(const char *text);
ns_css_value *ns_css_parse_transform_origin(const char *text);
ns_css_value *ns_css_parse_translate_prop(const char *text);
ns_css_value *ns_css_parse_rotate_prop(const char *text);
ns_css_value *ns_css_parse_scale_prop(const char *text);
char         *ns_css_transform_serialize(const ns_css_transform *tf);
char         *ns_css_transform_list_canonical(const char *value);
char         *ns_css_individual_transform_canonical(const char *value,
                                                    ns_css_prop prop);
char         *ns_css_transform_origin_canonical(const char *value,
                                                gboolean two_only);
gboolean      ns_css_is_math_fn_start(const char *s);
char         *ns_css_add_leading_zeros(char *v);
char         *ns_css_normalize_negative_zero(char *v);
GPtrArray    *ns_css_split_top_level_commas(const char *text);

double        ns_css_font_relative_unit_px(ns_css_unit unit, double font_px,
                                           const char *family, int weight,
                                           gboolean italic);
int           ns_css_font_weight_relative(int parent, gboolean bolder);
double        ns_css_font_size_keyword_px(const char *t);
int           ns_css_split_ws_paren(const char *text, char **out, int max);
const char   *ns_css_font_shorthand_slash(const char *tok);
gboolean      ns_css_font_shorthand_is_size_token(const char *tok);
gboolean      ns_css_font_stretch_keyword(const char *s);
gboolean      ns_css_font_ligatures_valid(const char *s);
gboolean      ns_css_font_feature_settings_valid(const char *s);
gboolean      ns_css_font_variation_settings_valid(const char *s);

ns_css_value *ns_css_parse_time_property(const char *t);
ns_css_value *ns_css_parse_animation_duration(const char *t);
ns_css_value *ns_css_parse_anim_longhand(ns_css_prop prop, const char *t);
ns_css_value *ns_css_parse_anim_value(const char *t, gboolean is_animation);
char         *ns_css_anim_entry_longhand_text(const ns_css_anim_entry *e,
                                              ns_css_prop prop);
gboolean      ns_css_anim_range_shorthand_expand(const char *text,
                                                 char **out_start,
                                                 char **out_end);
char         *ns_css_ident_decode(const char *tok);
gboolean      ns_css_starts_math_fn(const char *s, const char *e);

char         *ns_css_display_normalize(const char *text);
char         *ns_css_overflow_clip_margin_canonical(const char *text);
char         *ns_css_counter_list_canonical(const char *text, ns_css_prop prop);
char         *ns_css_list_style_type_canonical(const char *text);

ns_css_value *ns_css_parse_value_for(ns_css_prop prop, const char *text);
const char   *ns_css_mask_box_keyword(const char *t);
const char   *ns_css_mask_composite_keyword(const char *t);
gboolean      ns_css_bg_repeat_token(const char *tok, gboolean allow_axis);
char         *ns_css_bg_repeat_canonical(const char *a, const char *b);
char         *ns_css_bg_clip_canonical(const char *text);

ns_css_value *ns_css_parse_border_image_slice(const char *t);
ns_css_value *ns_css_parse_border_image_width(const char *t);
ns_css_value *ns_css_parse_border_image_outset(const char *t);
ns_css_value *ns_css_parse_border_image_repeat(const char *t);
char         *ns_css_border_image_length_serialize(const char *token,
                                                   gboolean allow_auto,
                                                   gboolean allow_percent);
gboolean      ns_css_border_image_tile_keyword(const char *token);
GPtrArray    *ns_css_border_image_tokens(const char *text);

ns_css_value *ns_css_parse_box_shadow(const char *text);
char         *ns_css_shadow_specified_canonical(const char *text,
                                                gboolean is_text);
char         *ns_css_shadow_serialize(const ns_css_shadow_list *list);

char         *ns_css_read_ident(const char **pp, const char *end);
char         *ns_css_read_string(const char **pp, const char *end);

ns_css_value *ns_css_parse_tracks(const char *text);
ns_css_value *ns_css_parse_areas(const char *text);
char         *ns_css_grid_line_canonical(const char *text,
                                         gboolean *ident_only);
int           ns_css_grid_placement_expand(const char *text, gboolean area,
                                           char *out[4],
                                           gboolean ident_only[4]);
char         *ns_css_grid_placement_canonical(const char *text,
                                              gboolean area);
char         *ns_css_grid_track_text_canonical(const char *text);
gboolean      ns_css_grid_template_parse(const char *text, char *out[3],
                                         char **canon);
gboolean      ns_css_grid_shorthand_parse(const char *text, char *out[6],
                                          char **canon);
char         *ns_css_grid_auto_flow_canonical(const char *text);
char         *ns_css_grid_template_compose(char *const v[3]);
char         *ns_css_grid_compose(char *const v[6]);

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_css_gradient_stop) == 40 &&
                sizeof(ns_css_gradient) == 1448 &&
                offsetof(ns_css_gradient, size_x) == 48 &&
                offsetof(ns_css_gradient, has_from) == 88 &&
                offsetof(ns_css_gradient, interp) == 132 &&
                offsetof(ns_css_gradient, stops) == 168);
G_STATIC_ASSERT(sizeof(ns_css_transform_op) == 264 &&
                offsetof(ns_css_transform_op, m3d) == 56 &&
                offsetof(ns_css_transform_op, a_is_percent) == 184 &&
                offsetof(ns_css_transform_op, a_pct) == 200 &&
                offsetof(ns_css_transform_op, rem) == 240 &&
                sizeof(ns_css_transform) == 2120);
G_STATIC_ASSERT(sizeof(ns_css_track) == 88 &&
                sizeof(ns_css_line_name) == 28 &&
                offsetof(ns_css_tracks, auto_repeat) == 2120 &&
                offsetof(ns_css_tracks, line_names) == 2148 &&
                sizeof(ns_css_tracks) == 3048 &&
                sizeof(ns_css_area_rect) == 24 &&
                sizeof(ns_css_areas) == 784 &&
                sizeof(ns_css_font_metrics) == 56 &&
                sizeof(ns_css_shadow) == 112 &&
                sizeof(ns_css_shadow_list) == 904 &&
                sizeof(ns_css_timing) == 48 &&
                sizeof(ns_css_anim_entry) == 120 &&
                offsetof(ns_css_anim_entry, timing) == 32 &&
                offsetof(ns_css_anim_entry, iterations) == 88 &&
                sizeof(ns_css_anim_list) == 968);
G_STATIC_ASSERT(sizeof(ns_display) == 5);
G_STATIC_ASSERT(sizeof(ns_border_image) == 176);
#endif

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(offsetof(ns_css_value, u.calc.fn) == 88 &&
                offsetof(ns_css_value, u.calc.args) == 96 &&
                sizeof(((ns_css_value *)0)->u.calc) == 152);
#endif

#endif
