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

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(offsetof(ns_css_value, u.calc.fn) == 88 &&
                offsetof(ns_css_value, u.calc.args) == 96 &&
                sizeof(((ns_css_value *)0)->u.calc) == 152);
#endif

#endif
