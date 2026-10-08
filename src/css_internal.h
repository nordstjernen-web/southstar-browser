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
#endif

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(offsetof(ns_css_value, u.calc.fn) == 88 &&
                offsetof(ns_css_value, u.calc.args) == 96 &&
                sizeof(((ns_css_value *)0)->u.calc) == 152);
#endif

#endif
