/* Southstar — the css.c sections ported to Rust (rust/css) that only css.c calls, and the css.c functions they call back. */
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

double        ns_css_container_unit_resolve(double v, ns_css_unit unit);

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(offsetof(ns_css_value, u.calc.fn) == 88 &&
                offsetof(ns_css_value, u.calc.args) == 96 &&
                sizeof(((ns_css_value *)0)->u.calc) == 152);
#endif

#endif
