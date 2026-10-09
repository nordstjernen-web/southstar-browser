/* Southstar — the functions of the css.c sections ported to Rust (rust/css) that only css.c calls. */
#ifndef NS_CSS_INTERNAL_H
#define NS_CSS_INTERNAL_H

#include "css.h"

gboolean      ns_css_parse_length(const char *text, double *out_v,
                                  ns_css_unit *out_unit);
ns_css_value *ns_css_parse_calc(const char *text);
double        ns_css_viewport_resolve(double v, ns_css_unit unit);
char         *ns_css_number_str(double n);

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

gboolean      ns_css_content_ident_valid(const char *s);
const char   *ns_css_quoted_end(const char *u, char quote);

ns_css_value *ns_css_parse_transform(const char *text);
ns_css_value *ns_css_parse_translate_prop(const char *text);
ns_css_value *ns_css_parse_rotate_prop(const char *text);
ns_css_value *ns_css_parse_scale_prop(const char *text);
gboolean      ns_css_is_math_fn_start(const char *s);

double        ns_css_font_relative_unit_px(ns_css_unit unit, double font_px,
                                           const char *family, int weight,
                                           gboolean italic);
int           ns_css_font_weight_relative(int parent, gboolean bolder);
int           ns_css_split_ws_paren(const char *text, char **out, int max);

ns_css_value *ns_css_parse_value_for(ns_css_prop prop, const char *text);

const char   *ns_css_parse_declaration_block(const char *p, const char *end,
                                             GArray *decls_out,
                                             ns_css_rule *capture);
gboolean      ns_css_declaration_value_syntax_valid(const char *text);
gboolean      ns_css_attr_unit_ident_valid(const char *unit);

gboolean      ns_css_selector_attr_ancestor_hashes(void);
guint32       ns_css_identifier_hash(char kind, const char *name, gsize len);
guint32       ns_css_attr_value_hash(const char *name, const char *value,
                                     gsize value_len);
gboolean      ns_css_anb_int_strict(const char *text, int *out);

char         *ns_css_scoped_css(const char *css, gsize len,
                                const char *host_id, gboolean frame_scope);
char         *ns_css_presentational_hints(const ns_node *el);
gboolean      ns_css_is_presentational_attr(const char *name);
gboolean      ns_css_element_state_matches(const ns_node *el,
                                           ns_css_pseudo kind,
                                           const char *arg);
void          ns_css_language_cache_reset(void);
char         *ns_css_substitute_vars(const char *value,
                                     const struct ns_var_map *map,
                                     GHashTable *registered, int depth);
int           ns_css_custom_value_wide_kind(const char *text);
struct ns_css_rule_index *ns_css_rule_index_build(ns_css_stylesheet *sheet);
GHashTable *ns_css_layer_ranks_build(const ns_css_stylesheet *ua,
                                     const ns_css_stylesheet *const *author,
                                     gsize n_author);
gboolean ns_css_restyle_prepare(const ns_css_stylesheet *ua,
                                const ns_css_stylesheet *const *author,
                                gsize n_author, guint64 sig);
gboolean ns_css_restyle_dirty(const ns_node *node);
void ns_css_restyle_dirty_clear(void);
ns_css_value *ns_css_value_cow(ns_style *out, int prop);
void ns_css_resolve_em_units(ns_style *out, const ns_style *parent_style,
                             double root_px);
void ns_css_compute_registered_vars(const ns_style *s,
                                    const ns_style *parent_style,
                                    GHashTable *registered, double root_px);
struct ns_var_map *ns_css_build_vars(struct ns_var_map *parent,
                                     GArray *var_matches,
                                     GHashTable *registered,
                                     GHashTable *adjust_cache);
const ns_node *ns_css_match_scope(void);
const ns_node *ns_css_focus_node(void);
const ns_node *ns_css_hover_node(void);
const ns_node *ns_css_active_node(void);
const ns_node *ns_css_fullscreen_node(void);
void          ns_css_has_memo_begin(void);
void          ns_css_has_memo_end(void);
gboolean      ns_css_rule_selector_matches(const ns_css_rule *rule,
                                           const ns_css_selector *sel,
                                           const ns_node *el,
                                           ns_css_pseudo_element pe,
                                           int *scope_order);
char         *ns_css_substitute_attrs(const char *text, const ns_node *node,
                                      gboolean *tainted);
void          ns_css_property_rule_clear(gpointer data);

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
G_STATIC_ASSERT(sizeof(ns_css_decl) == 24 &&
                offsetof(ns_css_decl, value) == 8 &&
                offsetof(ns_css_decl, important) == 16);
G_STATIC_ASSERT(sizeof(ns_css_pending_decl) == 32 &&
                offsetof(ns_css_rule, decls) == 8 &&
                offsetof(ns_css_rule, pending) == 32);
G_STATIC_ASSERT(sizeof(ns_css_selector) == 56 &&
                offsetof(ns_css_selector, ancestor_hashes) == 32 &&
                sizeof(ns_css_simple) == 80 &&
                offsetof(ns_css_simple, never_match) == 72 &&
                sizeof(ns_css_attr_pred) == 48 &&
                offsetof(ns_css_attr_pred, name_bit) == 40 &&
                sizeof(ns_css_pseudo_pred) == 32 &&
                offsetof(ns_css_pseudo_pred, arg) == 16 &&
                sizeof(ns_css_comb) == 4);
G_STATIC_ASSERT(sizeof(ns_css_rule) == 80 &&
                offsetof(ns_css_rule, scopes) == 64 &&
                sizeof(ns_css_import) == 24 &&
                sizeof(ns_css_property_rule) == 40 &&
                sizeof(ns_css_keyframe_stop) == 2176 &&
                offsetof(ns_css_keyframe_stop, has_transform) == 2144 &&
                offsetof(ns_css_keyframe_stop, raw_props) == 2168 &&
                sizeof(ns_css_keyframes) == 24 &&
                sizeof(ns_css_stylesheet) == 112 &&
                offsetof(ns_css_stylesheet, has_container_rules) == 64 &&
                offsetof(ns_css_stylesheet, serial) == 88 &&
                offsetof(ns_css_stylesheet, index) == 104);
#endif

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(offsetof(ns_css_value, u.calc.fn) == 88 &&
                offsetof(ns_css_value, u.calc.args) == 96 &&
                sizeof(((ns_css_value *)0)->u.calc) == 152);
#endif

#endif
