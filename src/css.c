/* Southstar — CSS parser, selectors, cascade.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "css.h"
#include "css_internal.h"
#include "css_syntax.h"

#include "config.h"
#include "image.h"
#include "net.h"

#include <limits.h>
#include <math.h>
#include <string.h>

static double g_viewport_w = 1000;
static double g_viewport_h = 800;

static GHashTable *g_js_registered_props;
static guint64 g_js_registered_serial;

static void css_property_rule_free(gpointer data);

static guint64
ns_css_registered_property_serial(void)
{
    return g_js_registered_serial;
}

static gboolean
css_custom_property_name(const char *name)
{
    if (!name || name[0] != '-' || name[1] != '-' || !name[2]) return FALSE;
    for (const char *p = name + 2; *p; p++)
        if (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r' || *p == '\f')
            return FALSE;
    return TRUE;
}

ns_css_register_status
ns_css_register_property(const char *name, const char *syntax_text,
                         gboolean inherits, const char *initial_value,
                         gboolean has_initial)
{
    if (!css_custom_property_name(name)) return NS_CSS_REGISTER_BAD_NAME;
    if (g_js_registered_props &&
        g_hash_table_contains(g_js_registered_props, name))
        return NS_CSS_REGISTER_EXISTS;
    ns_css_syntax_def *syntax = ns_css_syntax_def_parse(syntax_text);
    if (!syntax) return NS_CSS_REGISTER_BAD_SYNTAX;
    gboolean universal = ns_css_syntax_def_universal(syntax);
    if ((!universal && !has_initial) ||
        (has_initial &&
         !ns_css_syntax_def_initial_valid(syntax, initial_value))) {
        ns_css_syntax_def_free(syntax);
        return NS_CSS_REGISTER_BAD_INITIAL;
    }
    if (!g_js_registered_props)
        g_js_registered_props = g_hash_table_new_full(
            g_str_hash, g_str_equal, NULL, css_property_rule_free);
    ns_css_property_rule *pr = g_new0(ns_css_property_rule, 1);
    pr->name = g_strdup(name);
    pr->initial_value = has_initial ? g_strdup(initial_value) : NULL;
    pr->syntax_text = g_strdup(syntax_text ? syntax_text : "*");
    pr->syntax = syntax;
    pr->inherits = inherits;
    pr->has_initial = has_initial;
    g_hash_table_replace(g_js_registered_props, pr->name, pr);
    g_js_registered_serial++;
    return NS_CSS_REGISTER_OK;
}

void
ns_css_clear_registered_properties(void)
{
    if (g_js_registered_props) {
        g_hash_table_destroy(g_js_registered_props);
        g_js_registered_props = NULL;
        g_js_registered_serial++;
    }
}



void
ns_css_set_viewport(double vw_px, double vh_px)
{
    if (vw_px > 0) g_viewport_w = vw_px;
    if (vh_px > 0) g_viewport_h = vh_px;
}

double ns_css_viewport_w(void) { return g_viewport_w; }
double ns_css_viewport_h(void) { return g_viewport_h; }


static void (*g_frame_viewport_cb)(const ns_node *frame, double *w, double *h);

void
ns_css_set_frame_viewport_cb(void (*cb)(const ns_node *frame,
                                        double *w, double *h))
{
    g_frame_viewport_cb = cb;
}


static gboolean
is_ws(char c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f'; }

static gunichar
css_unescape_cp(gunichar cp)
{
    if (cp == 0 || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF))
        return 0xFFFD;
    return cp;
}

void
ns_css_append_unescaped(GString *out, const char **pp)
{
    const char *p = *pp;
    if (*p == '\\' && p[1]) {
        p++;
        if (g_ascii_isxdigit(*p)) {
            gunichar cp = 0;
            int n = 0;
            while (n < 6 && g_ascii_isxdigit(*p)) {
                cp = cp * 16 + (gunichar)g_ascii_xdigit_value(*p);
                p++;
                n++;
            }
            if (is_ws(*p)) {
                gboolean cr = *p == '\r';
                p++;
                if (cr && *p == '\n') p++;
            }
            g_string_append_unichar(out, css_unescape_cp(cp));
        } else {
            g_string_append_c(out, *p++);
        }
    } else {
        g_string_append_c(out, *p++);
    }
    *pp = p;
}


ns_css_value *
ns_css_value_dup(const ns_css_value *v)
{
    if (!v) return NULL;
    ((ns_css_value *)v)->ref++;
    return (ns_css_value *)v;
}

void
ns_css_value_free(ns_css_value *v)
{
    while (v) {
        if (v->ref > 0) { v->ref--; return; }
        if (v->kind == NS_CSS_V_KEYWORD) g_free(v->u.keyword);
        else if (v->kind == NS_CSS_V_URL) g_free(v->u.url);
        else if (v->kind == NS_CSS_V_AREAS) {
            for (int i = 0; i < v->u.areas.n_rects; i++)
                g_free(v->u.areas.rects[i].name);
        }
        else if (v->kind == NS_CSS_V_ANIM) {
            for (int i = 0; i < v->u.anim.n; i++)
                g_free(v->u.anim.entries[i].name);
        }
        g_free(v->image_set_text);
        g_free(v->specified);
        ns_css_value *next = v->next_layer;
        g_free(v);
        v = next;
    }
}


typedef struct ns_css_scope {
    GPtrArray *roots;
    GPtrArray *limits;
} ns_css_scope;

#define NS_CSS_MAX_AT_NESTING 32














static ns_css_value *
keyword_value_dup(const char *canonical)
{
    ns_css_value *v = g_new0(ns_css_value, 1);
    v->kind = NS_CSS_V_KEYWORD;
    v->u.keyword = g_strdup(canonical);
    return v;
}

typedef enum ns_custom_prop_wide {
    NS_CUSTOM_WIDE_NONE,
    NS_CUSTOM_WIDE_INHERIT,
    NS_CUSTOM_WIDE_INITIAL,
    NS_CUSTOM_WIDE_UNSET,
    NS_CUSTOM_WIDE_REVERT,
    NS_CUSTOM_WIDE_REVERT_LAYER,
    NS_CUSTOM_WIDE_REVERT_RULE,
} ns_custom_prop_wide;

typedef struct ns_var_map {
    int ref;
    GHashTable *own;
    struct ns_var_map *parent;
    GPtrArray *names;
} ns_var_map;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_var_map) == 32);
#endif

static __thread GHashTable *g_registered_props;

static ns_var_map *
ns_var_map_ref(ns_var_map *m)
{
    if (m) m->ref++;
    return m;
}

static void
ns_var_map_unref(ns_var_map *m)
{
    while (m && --m->ref <= 0) {
        ns_var_map *parent = m->parent;
        if (m->own) g_hash_table_destroy(m->own);
        if (m->names) g_ptr_array_unref(m->names);
        g_free(m);
        m = parent;
    }
}


char *
ns_css_resolve_style_vars(const char *text, const ns_style *style)
{
    return ns_css_substitute_vars(text, style ? style->vars : NULL,
                                  g_registered_props, 0);
}

static void
css_property_rule_free(gpointer data)
{
    ns_css_property_rule_clear(data);
    g_free(data);
}

ns_css_keyframes *
ns_css_keyframes_resolve(const ns_css_keyframes *kf,
                         const struct ns_var_map *vars)
{
    if (!kf) return NULL;
    gboolean any_raw = FALSE;
    for (int i = 0; i < kf->n_stops && !any_raw; i++)
        if (kf->stops[i].raw_props) any_raw = TRUE;
    if (!any_raw) return NULL;
    ns_css_keyframes *out = g_new0(ns_css_keyframes, 1);
    out->n_stops = kf->n_stops;
    out->stops = g_new(ns_css_keyframe_stop, (gsize)kf->n_stops);
    memcpy(out->stops, kf->stops,
           (gsize)kf->n_stops * sizeof(ns_css_keyframe_stop));
    for (int i = 0; i < out->n_stops; i++) {
        ns_css_keyframe_stop *s = &out->stops[i];
        const char *rawp = s->raw_props;
        s->raw_props = NULL;
        if (!rawp) continue;
        char *resolved = ns_css_substitute_vars(rawp, vars,
                                                g_registered_props, 0);
        if (!resolved) continue;
        ns_css_transform ind = { 0 };
        ns_css_transform list = s->has_transform ? s->transform
                                                 : (ns_css_transform){ 0 };
        char **decls = g_strsplit(resolved, ";", -1);
        for (int d = 0; decls[d]; d++) {
            char *colon = strchr(decls[d], ':');
            if (!colon) continue;
            *colon = '\0';
            char *prop = g_strstrip(decls[d]);
            char *val  = g_strstrip(colon + 1);
            ns_css_value *tv = NULL;
            if (g_ascii_strcasecmp(prop, "transform") == 0) {
                tv = ns_css_parse_transform(val);
                if (tv) list = tv->u.transform;
            } else if (g_ascii_strcasecmp(prop, "translate") == 0) {
                tv = ns_css_parse_translate_prop(val);
            } else if (g_ascii_strcasecmp(prop, "rotate") == 0) {
                tv = ns_css_parse_rotate_prop(val);
            } else if (g_ascii_strcasecmp(prop, "scale") == 0) {
                tv = ns_css_parse_scale_prop(val);
            }
            if (tv && g_ascii_strcasecmp(prop, "transform") != 0 &&
                ind.n_ops < NS_CSS_TRANSFORM_OPS_MAX)
                ind.ops[ind.n_ops++] = tv->u.transform.ops[0];
            if (tv) ns_css_value_free(tv);
        }
        g_strfreev(decls);
        s->raw_props = resolved;
        ns_css_transform merged = ind;
        for (int k = 0; k < list.n_ops &&
                        merged.n_ops < NS_CSS_TRANSFORM_OPS_MAX; k++)
            merged.ops[merged.n_ops++] = list.ops[k];
        if (merged.n_ops > 0) {
            s->transform = merged;
            s->has_transform = TRUE;
        }
    }
    return out;
}

void
ns_css_keyframes_resolved_free(ns_css_keyframes *kf)
{
    if (!kf) return;
    g_free(kf->name);
    for (int i = 0; i < kf->n_stops; i++)
        g_free(kf->stops[i].raw_props);
    g_free(kf->stops);
    g_free(kf);
}

static ns_css_color_scheme g_color_scheme = NS_CSS_COLOR_SCHEME_LIGHT;
static ns_css_reduced_motion g_reduced_motion = NS_CSS_REDUCED_MOTION_NO_PREFERENCE;

void
ns_css_set_color_scheme(ns_css_color_scheme scheme)
{
    ns_css_color_scheme next = scheme == NS_CSS_COLOR_SCHEME_DARK
        ? NS_CSS_COLOR_SCHEME_DARK : NS_CSS_COLOR_SCHEME_LIGHT;
    if (g_color_scheme == next) return;
    g_color_scheme = next;
    ns_css_stylesheet_cache_drop();
}

void
ns_css_set_reduced_motion(ns_css_reduced_motion motion)
{
    ns_css_reduced_motion next = motion == NS_CSS_REDUCED_MOTION_REDUCE
        ? NS_CSS_REDUCED_MOTION_REDUCE : NS_CSS_REDUCED_MOTION_NO_PREFERENCE;
    if (g_reduced_motion == next) return;
    g_reduced_motion = next;
    ns_css_stylesheet_cache_drop();
}

ns_css_reduced_motion
ns_css_get_reduced_motion(void)
{
    return g_reduced_motion;
}

ns_css_color_scheme
ns_css_get_color_scheme(void)
{
    return g_color_scheme;
}


static __thread GHashTable *g_var_adjust_cache;

typedef struct css_candidate {
    guint rule_idx;
    guint selector_idx;
} css_candidate;

typedef struct ns_css_rule_index {
    GHashTable *by_id;
    GHashTable *by_class;
    GHashTable *by_tag;
    GHashTable *by_attr;
    GArray     *universal;
} ns_css_rule_index;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_css_rule_index) == 40 && sizeof(css_candidate) == 8);
#endif

static void ns_css_rule_index_free(ns_css_rule_index *idx);

gboolean
ns_css_stylesheet_has_container_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_container_rules;
}

gboolean
ns_css_stylesheet_has_container_units(const ns_css_stylesheet *sh)
{
    return sh && sh->has_container_units;
}

gboolean
ns_css_stylesheet_has_hover_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_hover_rules;
}

gboolean
ns_css_stylesheet_has_active_rules(const ns_css_stylesheet *sh)
{
    return sh && sh->has_active_rules;
}

void
ns_css_stylesheet_free(ns_css_stylesheet *s)
{
    if (!s || s->cached) return;
    if (s->rules) g_ptr_array_free(s->rules, TRUE);
    if (s->imports) g_array_free(s->imports, TRUE);
    if (s->layers) g_hash_table_destroy(s->layers);
    if (s->layer_names) g_ptr_array_free(s->layer_names, TRUE);
    if (s->font_faces) g_array_free(s->font_faces, TRUE);
    if (s->keyframes) g_array_free(s->keyframes, TRUE);
    if (s->property_rules) g_array_free(s->property_rules, TRUE);
    g_clear_pointer(&s->page_rule, g_free);
    g_clear_pointer(&s->resolved_base, g_free);
    if (s->index) ns_css_rule_index_free(s->index);
    s->rules = NULL;
    s->imports = NULL;
    s->layers = NULL;
    s->layer_names = NULL;
    s->font_faces = NULL;
    s->keyframes = NULL;
    s->property_rules = NULL;
    s->index = NULL;
    g_free(s);
}

static void
ns_css_rule_index_free(ns_css_rule_index *idx)
{
    if (!idx) return;
    if (idx->by_id)    g_hash_table_destroy(idx->by_id);
    if (idx->by_class) g_hash_table_destroy(idx->by_class);
    if (idx->by_tag)   g_hash_table_destroy(idx->by_tag);
    if (idx->by_attr)  g_hash_table_destroy(idx->by_attr);
    if (idx->universal) g_array_free(idx->universal, TRUE);
    g_free(idx);
}



static ns_style *g_style_pool[16384];
static int g_style_pool_n;

static ns_style *
ns_style_alloc(void)
{
    if (g_style_pool_n > 0) {
        ns_style *s = g_style_pool[--g_style_pool_n];
        memset(s, 0, sizeof(*s));
        return s;
    }
    return g_new0(ns_style, 1);
}

void
ns_style_free(ns_style *s)
{
    if (!s) return;
    if (s->ref > 0) { s->ref--; return; }
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++)
        if (s->values[i]) ns_css_value_free(s->values[i]);
    ns_style_free(s->before);
    ns_style_free(s->after);
    ns_style_free(s->first_letter);
    ns_style_free(s->first_line);
    ns_style_free(s->placeholder);
    ns_style_free(s->selection);
    ns_style_free(s->marker);
    ns_style_free(s->backdrop);
    ns_style_free(s->file_selector_button);
    ns_style_free(s->hidden_before);
    ns_style_free(s->hidden_after);
    if (s->vars) ns_var_map_unref(s->vars);
    if (g_style_pool_n < (int)G_N_ELEMENTS(g_style_pool))
        g_style_pool[g_style_pool_n++] = s;
    else
        g_free(s);
}

static GHashTable *g_incr_exclude;

void
ns_css_incremental_exclude(const void *node, gboolean exclude)
{
    if (!node) return;
    if (!g_incr_exclude)
        g_incr_exclude = g_hash_table_new(g_direct_hash, g_direct_equal);
    if (exclude) g_hash_table_add(g_incr_exclude, (gpointer)node);
    else g_hash_table_remove(g_incr_exclude, node);
}

typedef enum ns_css_origin {
    NS_CSS_ORIGIN_UA,
    NS_CSS_ORIGIN_PRESENTATIONAL,
    NS_CSS_ORIGIN_AUTHOR,
} ns_css_origin;

typedef struct match_entry {
    int          origin;
    int          spec_a, spec_b, spec_c;
    int          sheet_index;
    int          layer_order;
    int          scope_order;
    int          source_order;
    int          decl_order;
    gboolean     important;
    gboolean     inline_style;
    const ns_css_rule *rule;
    ns_css_value *value;
    ns_css_prop  prop;
} match_entry;

typedef struct var_match {
    int origin;
    int spec_a, spec_b, spec_c;
    int sheet_index;
    int layer_order;
    int scope_order;
    int source_order;
    int decl_order;
    gboolean important;
    gboolean inline_style;
    const ns_css_rule *rule;
    const char *name;
    const char *text;
} var_match;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(var_match) == 72 && sizeof(match_entry) == 72);
#endif

typedef struct pending_match {
    int origin;
    int spec_a, spec_b, spec_c;
    int sheet_index;
    int layer_order;
    int scope_order;
    int source_order;
    int decl_order_base;
    gboolean inline_style;
    const ns_css_rule *rule;
    ns_css_pending_decl *pd;
} pending_match;

typedef struct {
    ns_css_pseudo_element pe;
    GArray *out;
    GArray *var_out;
    GArray *pending_out;
} gather_dest;

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(gather_dest) == 32);
#endif

static void
css_collect_property_rules(GHashTable *reg, const ns_css_stylesheet *sh)
{
    if (!reg || !sh || !sh->property_rules) return;
    for (guint i = 0; i < sh->property_rules->len; i++) {
        ns_css_property_rule *pr =
            &g_array_index(sh->property_rules, ns_css_property_rule, i);
        if (pr->name) g_hash_table_replace(reg, pr->name, pr);
    }
}

static gboolean
pending_uses_attr(const GArray *pending_matches)
{
    if (!pending_matches) return FALSE;
    for (guint i = 0; i < pending_matches->len; i++) {
        const pending_match *e = &g_array_index((GArray *)pending_matches, pending_match, i);
        if (e->pd && e->pd->raw_vtext && strstr(e->pd->raw_vtext, "attr(")) return TRUE;
    }
    return FALSE;
}

static const ns_css_stylesheet *
ua_sheet_for(const ns_node *doc)
{
    return ns_css_ua_sheet(doc && (doc->flags & NS_NODE_QUIRKS));
}

#define NS_CSS_MAX_CASCADE_DEPTH 512

static void
cascade_walk(ns_node *node,
             const ns_css_stylesheet *ua,
             const ns_css_stylesheet *const *author, gsize n_author,
             const ns_style *parent_style,
             const ns_style *layout_parent,
             double *root_px,
             GHashTable *layer_ranks,
             GHashTable *out,
             gboolean under_dirty);

static GHashTable    *g_incr_prev_styles;
static GHashTable    *g_incr_before_styles;

const ns_style *
ns_css_style_before_change(const void *node)
{
    if (!node || !g_incr_before_styles) return NULL;
    return g_hash_table_lookup(g_incr_before_styles, node);
}
static ns_node       *g_incr_prev_doc;
static guint64        g_incr_prev_sig;
static guint64        g_incr_prev_cq_sig;
static const ns_node *g_incr_prev_focus;
static const ns_node *g_incr_prev_hover;
static const ns_node *g_incr_prev_active;
static const ns_node *g_incr_prev_fullscreen;
static gboolean       g_incr_pass_active;
static guint          g_incr_reused;
static guint          g_incr_recomputed;
static double         g_incr_zoom = 1.0;

void
ns_css_set_render_zoom(double zoom)
{
    g_incr_zoom = zoom > 0 ? zoom : 1.0;
}

static guint64
incr_sheet_sig(const ns_css_stylesheet *ua,
               const ns_css_stylesheet *const *author, gsize n)
{
    guint64 h = 1469598103934665603ULL;
    guint64 vals[3] = { ua ? ua->serial : 0, (guint64)n,
                        ns_css_registered_property_serial() };
    for (int i = 0; i < 3; i++) { h ^= vals[i]; h *= 1099511628211ULL; }
    for (gsize i = 0; i < n; i++) {
        h ^= author[i] ? author[i]->serial : 0;
        h *= 1099511628211ULL;
    }
    return h;
}

static GHashTable *g_style_share;
static GByteArray *g_share_scratch;
static guint64 g_style_share_next_id;

typedef struct {
    guint32  hash;
    guint32  len;
    guint8  *data;
} share_key_t;

static guint
share_key_hash(gconstpointer p)
{
    return ((const share_key_t *)p)->hash;
}

static gboolean
share_key_equal(gconstpointer a, gconstpointer b)
{
    const share_key_t *x = a, *y = b;
    return x->hash == y->hash && x->len == y->len &&
           memcmp(x->data, y->data, x->len) == 0;
}

static void
share_key_free(gpointer p)
{
    share_key_t *k = p;
    g_free(k->data);
    g_free(k);
}

static guint32
share_key_djb2(const guint8 *d, guint32 n)
{
    guint64 h = 1469598103934665603ULL;
    guint32 i = 0;
    for (; i + 8 <= n; i += 8) {
        guint64 w;
        memcpy(&w, d + i, 8);
        h = (h ^ w) * 1099511628211ULL;
    }
    for (; i < n; i++)
        h = (h ^ d[i]) * 1099511628211ULL;
    return (guint32)(h ^ (h >> 32));
}

typedef struct {
    ns_css_pseudo_element pe;
    GArray *m;
    GArray *v;
    GArray *p;
} ns_pe_gather;

static ns_var_map *
ns_style_vars_clone(ns_var_map *vars)
{
    return ns_var_map_ref(vars);
}

static ns_style *
ns_style_clone_shared(const ns_style *s)
{
    if (!s) return NULL;
    ns_style *c = ns_style_alloc();
    c->share_id = s->share_id;
    c->display  = s->display;
    for (int i = 0; i < NS_CSS_PROP_COUNT; i++) {
        c->values[i] = s->values[i];
        if (c->values[i]) c->values[i]->ref++;
    }
    c->before       = ns_style_clone_shared(s->before);
    c->after        = ns_style_clone_shared(s->after);
    c->first_letter = ns_style_clone_shared(s->first_letter);
    c->first_line   = ns_style_clone_shared(s->first_line);
    c->placeholder  = ns_style_clone_shared(s->placeholder);
    c->selection    = ns_style_clone_shared(s->selection);
    c->marker       = ns_style_clone_shared(s->marker);
    c->backdrop     = ns_style_clone_shared(s->backdrop);
    c->file_selector_button = ns_style_clone_shared(
        s->file_selector_button);
    c->hidden_before = ns_style_clone_shared(s->hidden_before);
    c->hidden_after  = ns_style_clone_shared(s->hidden_after);
    c->vars = ns_style_vars_clone(s->vars);
    return c;
}

#define SHARE_KEY_MATCH_BYTES \
    (sizeof(int) * 9 + sizeof(((match_entry *)0)->important) + \
     sizeof(((match_entry *)0)->inline_style) + \
     sizeof(((match_entry *)0)->rule) + sizeof(((match_entry *)0)->value) + \
     sizeof(((match_entry *)0)->prop))
#define SHARE_KEY_VAR_BYTES \
    (sizeof(int) * 9 + sizeof(((var_match *)0)->important) + \
     sizeof(((var_match *)0)->inline_style) + \
     sizeof(((var_match *)0)->rule) + sizeof(((var_match *)0)->name) + \
     sizeof(((var_match *)0)->text))
#define SHARE_KEY_PENDING_BYTES \
    (sizeof(int) * 9 + sizeof(((pending_match *)0)->inline_style) + \
     sizeof(((pending_match *)0)->rule) + sizeof(((pending_match *)0)->pd))

static inline guint8 *
share_key_put_raw(guint8 *p, const void *src, gsize n)
{
    memcpy(p, src, n);
    return p + n;
}

static guint8 *
share_key_put_matches(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const match_entry *e = &g_array_index((GArray *)arr, match_entry, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->important, sizeof e->important);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->value, sizeof e->value);
        p = share_key_put_raw(p, &e->prop, sizeof e->prop);
    }
    return p;
}

static guint8 *
share_key_put_vars(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const var_match *e = &g_array_index((GArray *)arr, var_match, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->important, sizeof e->important);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->name, sizeof e->name);
        p = share_key_put_raw(p, &e->text, sizeof e->text);
    }
    return p;
}

static guint8 *
share_key_put_pending(guint8 *p, const GArray *arr)
{
    guint n = arr ? arr->len : 0;
    p = share_key_put_raw(p, &n, sizeof n);
    for (guint i = 0; i < n; i++) {
        const pending_match *e = &g_array_index((GArray *)arr, pending_match, i);
        p = share_key_put_raw(p, &e->origin, sizeof(int) * 9);
        p = share_key_put_raw(p, &e->inline_style, sizeof e->inline_style);
        p = share_key_put_raw(p, &e->rule, sizeof e->rule);
        p = share_key_put_raw(p, &e->pd, sizeof e->pd);
    }
    return p;
}

static gboolean
container_relative_unit(ns_css_unit unit)
{
    return unit == NS_CSS_UNIT_CQW || unit == NS_CSS_UNIT_CQH ||
           unit == NS_CSS_UNIT_CQMIN || unit == NS_CSS_UNIT_CQMAX;
}

static gboolean
share_matches_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const match_entry *e = &g_array_index((GArray *)matches,
                                               match_entry, i);
        if (e->prop == NS_CSS_FONT_SIZE && e->value &&
            e->value->kind == NS_CSS_V_LENGTH &&
            container_relative_unit(e->value->u.length.unit))
            return TRUE;
    }
    return FALSE;
}

static gboolean
share_vars_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const var_match *e = &g_array_index((GArray *)matches, var_match, i);
        if (ns_css_text_has_container_units(e->text, -1)) return TRUE;
    }
    return FALSE;
}

static gboolean
share_pending_need_container(const GArray *matches)
{
    if (!matches) return FALSE;
    for (guint i = 0; i < matches->len; i++) {
        const pending_match *e = &g_array_index((GArray *)matches,
                                                 pending_match, i);
        if (e->pd && ns_css_text_has_container_units(e->pd->raw_vtext, -1))
            return TRUE;
    }
    return FALSE;
}

static gboolean
share_key_needs_container(const GArray *matches,
                          const GArray *var_matches,
                          const GArray *pending_matches,
                          const ns_pe_gather *pe_g, int n_pe)
{
    if (share_matches_need_container(matches) ||
        share_vars_need_container(var_matches) ||
        share_pending_need_container(pending_matches))
        return TRUE;
    for (int i = 0; i < n_pe; i++)
        if (share_matches_need_container(pe_g[i].m) ||
            share_vars_need_container(pe_g[i].v) ||
            share_pending_need_container(pe_g[i].p))
            return TRUE;
    return FALSE;
}

static gsize
share_key_arrays_bytes(const GArray *matches, const GArray *var_matches,
                       const GArray *pending_matches)
{
    return sizeof(guint) * 3 +
           (matches ? matches->len : 0) * SHARE_KEY_MATCH_BYTES +
           (var_matches ? var_matches->len : 0) * SHARE_KEY_VAR_BYTES +
           (pending_matches ? pending_matches->len : 0) *
               SHARE_KEY_PENDING_BYTES;
}

static void
style_share_key(GByteArray *b,
                const ns_style *parent_style, double root_px,
                const GArray *matches, const GArray *var_matches,
                const GArray *pending_matches,
                const ns_pe_gather *pe_g, int n_pe)
{
    gsize cq_bytes = ns_css_container_stack_copy(NULL, 0);
    if (cq_bytes && !share_key_needs_container(matches, var_matches,
                                               pending_matches, pe_g, n_pe))
        cq_bytes = 0;
    guint cq_len = (guint)(cq_bytes / NS_CSS_CONTAINER_BYTES);

    gsize need = sizeof(guint64) + sizeof(double) + sizeof(guint) +
                 cq_bytes +
                 share_key_arrays_bytes(matches, var_matches, pending_matches);
    for (int i = 0; i < n_pe; i++)
        need += sizeof(guint) +
                share_key_arrays_bytes(pe_g[i].m, pe_g[i].v, pe_g[i].p);
    if (b->len < need) g_byte_array_set_size(b, (guint)need);

    guint8 *p = b->data;
    guint64 parent_id = parent_style ? parent_style->share_id : 0;
    p = share_key_put_raw(p, &parent_id, sizeof parent_id);
    p = share_key_put_raw(p, &root_px, sizeof root_px);
    p = share_key_put_raw(p, &cq_len, sizeof cq_len);
    if (cq_len) {
        ns_css_container_stack_copy(p, cq_bytes);
        p += cq_bytes;
    }
    p = share_key_put_matches(p, matches);
    p = share_key_put_vars(p, var_matches);
    p = share_key_put_pending(p, pending_matches);
    for (int i = 0; i < n_pe; i++) {
        guint pe = (guint)pe_g[i].pe;
        p = share_key_put_raw(p, &pe, sizeof pe);
        p = share_key_put_matches(p, pe_g[i].m);
        p = share_key_put_vars(p, pe_g[i].v);
        p = share_key_put_pending(p, pe_g[i].p);
    }
    b->len = (guint)(p - b->data);
}

static gboolean
element_cannot_be_unboxed(const ns_node *el)
{
    if (el->kind != NS_NODE_ELEMENT || !el->name) return FALSE;
    if (el->flags & NS_NODE_SVG_NS)
        return strcmp(el->name, "svg") == 0 && el->parent &&
               !(el->parent->flags & NS_NODE_SVG_NS);
    if (el->flags & NS_NODE_FOREIGN_NS) return FALSE;
    static const char *const unusual[] = {
        "audio", "br", "canvas", "embed", "frame", "frameset", "iframe",
        "img", "input", "meter", "object", "progress", "select",
        "textarea", "video", "wbr",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(unusual); i++)
        if (g_ascii_strcasecmp(el->name, unusual[i]) == 0) return TRUE;
    return FALSE;
}

static gboolean
display_contents_to_none(const ns_node *el, ns_style *s)
{
    if (s->display.box != NS_DISPLAY_BOX_CONTENTS ||
        !element_cannot_be_unboxed(el))
        return FALSE;
    ns_css_value_free(s->values[NS_CSS_DISPLAY]);
    s->values[NS_CSS_DISPLAY] = keyword_value_dup("none");
    s->display = ns_css_display_from_keyword("none");
    return TRUE;
}

static void
strip_native_widget_decorations(const ns_node *el, ns_style *s)
{
    if (!ns_node_is_element_named(el, "input")) return;
    const char *type = ns_element_get_attr(el, "type");
    if (!type || (g_ascii_strcasecmp(type, "checkbox") != 0 &&
                  g_ascii_strcasecmp(type, "radio") != 0))
        return;
    const ns_css_value *ap = s->values[NS_CSS_APPEARANCE];
    if (ap && ap->kind == NS_CSS_V_KEYWORD && ap->u.keyword &&
        strcmp(ap->u.keyword, "none") == 0)
        return;
    static const ns_css_prop stripped[] = {
        NS_CSS_BACKGROUND_COLOR, NS_CSS_BACKGROUND_IMAGE,
        NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
        NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
        NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
        NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
        NS_CSS_BORDER_TOP_COLOR, NS_CSS_BORDER_RIGHT_COLOR,
        NS_CSS_BORDER_BOTTOM_COLOR, NS_CSS_BORDER_LEFT_COLOR,
        NS_CSS_BORDER_TOP_LEFT_RADIUS, NS_CSS_BORDER_TOP_RIGHT_RADIUS,
        NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS, NS_CSS_BORDER_BOTTOM_LEFT_RADIUS,
        NS_CSS_PADDING_TOP, NS_CSS_PADDING_RIGHT,
        NS_CSS_PADDING_BOTTOM, NS_CSS_PADDING_LEFT,
        NS_CSS_BOX_SHADOW,
    };
    for (gsize i = 0; i < G_N_ELEMENTS(stripped); i++) {
        if (s->values[stripped[i]]) {
            ns_css_value_free(s->values[stripped[i]]);
            s->values[stripped[i]] = NULL;
        }
    }
}

static double
frame_edge_px(const ns_style *s, ns_css_prop prop)
{
    const ns_css_value *v = s->values[prop];
    if (!v || v->kind != NS_CSS_V_LENGTH || v->u.length.unit != NS_CSS_UNIT_PX)
        return 0;
    return v->u.length.v;
}

static double
frame_border_px(const ns_style *s, ns_css_prop width_prop,
                ns_css_prop style_prop)
{
    const ns_css_value *st = s->values[style_prop];
    if (!st || (st->kind == NS_CSS_V_KEYWORD && st->u.keyword &&
                (strcmp(st->u.keyword, "none") == 0 ||
                 strcmp(st->u.keyword, "hidden") == 0)))
        return 0;
    return frame_edge_px(s, width_prop);
}

static gboolean
frame_viewport_from_style(const ns_style *s, double *w, double *h)
{
    if (!s) return FALSE;
    const ns_css_value *wv = s->values[NS_CSS_WIDTH];
    const ns_css_value *hv = s->values[NS_CSS_HEIGHT];
    if (!wv || wv->kind != NS_CSS_V_LENGTH ||
        wv->u.length.unit != NS_CSS_UNIT_PX ||
        !hv || hv->kind != NS_CSS_V_LENGTH ||
        hv->u.length.unit != NS_CSS_UNIT_PX)
        return FALSE;
    double fw = wv->u.length.v, fh = hv->u.length.v;
    if (ns_css_keyword_is(s->values[NS_CSS_BOX_SIZING], "border-box")) {
        fw -= frame_edge_px(s, NS_CSS_PADDING_LEFT) +
              frame_edge_px(s, NS_CSS_PADDING_RIGHT) +
              frame_border_px(s, NS_CSS_BORDER_LEFT_WIDTH,
                              NS_CSS_BORDER_LEFT_STYLE) +
              frame_border_px(s, NS_CSS_BORDER_RIGHT_WIDTH,
                              NS_CSS_BORDER_RIGHT_STYLE);
        fh -= frame_edge_px(s, NS_CSS_PADDING_TOP) +
              frame_edge_px(s, NS_CSS_PADDING_BOTTOM) +
              frame_border_px(s, NS_CSS_BORDER_TOP_WIDTH,
                              NS_CSS_BORDER_TOP_STYLE) +
              frame_border_px(s, NS_CSS_BORDER_BOTTOM_WIDTH,
                              NS_CSS_BORDER_BOTTOM_STYLE);
    }
    if (fw <= 0 || fh <= 0) return FALSE;
    *w = fw;
    *h = fh;
    return TRUE;
}

/* The sheets of each document, when the caller of ns_css_compute() says
 * whose each sheet is. */
static __thread GHashTable *g_doc_sheets;

static GHashTable *
doc_sheets_new(const ns_css_stylesheet *const *sheets,
               const ns_node *const *docs, gsize n)
{
    if (!docs) return NULL;
    GHashTable *map = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                            NULL,
                                            (GDestroyNotify)g_ptr_array_unref);
    for (gsize i = 0; i < n; i++) {
        GPtrArray *own = g_hash_table_lookup(map, docs[i]);
        if (!own) {
            own = g_ptr_array_new();
            g_hash_table_insert(map, (gpointer)docs[i], own);
        }
        g_ptr_array_add(own, (gpointer)sheets[i]);
    }
    return map;
}

/* The elements of a document are styled by its own sheets only, not by
 * those of the document its frame is in, nor by those of its frames'
 * documents. */
static void
doc_own_sheets(const ns_node *node, const ns_css_stylesheet *const **author,
               gsize *n_author)
{
    if (!g_doc_sheets || node->kind != NS_NODE_DOCUMENT) return;
    const GPtrArray *own = g_hash_table_lookup(g_doc_sheets, node);
    *author = own ? (const ns_css_stylesheet *const *)own->pdata : NULL;
    *n_author = own ? own->len : 0;
}

static void
cascade_walk(ns_node *node,
             const ns_css_stylesheet *ua,
             const ns_css_stylesheet *const *author, gsize n_author,
             const ns_style *parent_style,
             const ns_style *layout_parent,
             double *root_px,
             GHashTable *layer_ranks,
             GHashTable *out,
             gboolean under_dirty)
{
    static int depth;
    if (depth >= NS_CSS_MAX_CASCADE_DEPTH) return;
    depth++;
    double frame_vw = 0, frame_vh = 0;
    gboolean frame_viewport = FALSE;
    if (node->kind == NS_NODE_DOCUMENT && node->parent && g_frame_viewport_cb) {
        double fw = 0, fh = 0;
        if (!frame_viewport_from_style(parent_style, &fw, &fh))
            g_frame_viewport_cb(node->parent, &fw, &fh);
        if (fw > 0 && fh > 0 &&
            (fabs(fw - g_viewport_w) > 0.01 ||
             fabs(fh - g_viewport_h) > 0.01)) {
            frame_vw = g_viewport_w;
            frame_vh = g_viewport_h;
            g_viewport_w = fw;
            g_viewport_h = fh;
            frame_viewport = TRUE;
        }
    }
    doc_own_sheets(node, &author, &n_author);
    const ns_style *child_parent_style = parent_style;
    const ns_style *child_layout_parent = layout_parent;
    gboolean nd_recurse_dirty = under_dirty;
    if (node->kind == NS_NODE_ELEMENT) {
        gboolean nd_node_dirty = under_dirty ||
            ns_css_restyle_dirty(node) ||
            (g_incr_exclude && g_hash_table_contains(g_incr_exclude, node));
        ns_style *nd_prev =
            (g_incr_pass_active && !nd_node_dirty && g_incr_prev_styles)
            ? g_hash_table_lookup(g_incr_prev_styles, node) : NULL;
        ns_style *s;
        if (nd_prev) {
            s = nd_prev;
            s->ref++;
            g_incr_reused++;
        } else {
        s = ns_style_alloc();
        g_incr_recomputed++;
        nd_node_dirty = TRUE;
        static GArray *sc_matches, *sc_var, *sc_pending;
        static GPtrArray *sc_owned;
        static GArray *sc_pe_m[9], *sc_pe_v[9], *sc_pe_p[9];
        if (!sc_matches) {
            sc_matches  = g_array_new(FALSE, FALSE, sizeof(match_entry));
            sc_var      = g_array_new(FALSE, FALSE, sizeof(var_match));
            sc_pending  = g_array_new(FALSE, FALSE, sizeof(pending_match));
            sc_owned    = g_ptr_array_new_with_free_func(
                              (GDestroyNotify)ns_css_value_free);
        }
        GArray *matches = sc_matches;
        GArray *var_matches = sc_var;
        GArray *pending_matches = sc_pending;
        GPtrArray *owned_values = sc_owned;
        g_array_set_size(matches, 0);
        g_array_set_size(var_matches, 0);
        g_array_set_size(pending_matches, 0);
        g_ptr_array_set_size(owned_values, 0);
        guint pe_mask = ua ? ua->pseudo_mask : 0;
        for (gsize i = 0; i < n_author; i++)
            if (author[i]) pe_mask |= author[i]->pseudo_mask;
        ns_pe_gather pe_g[9];
        int n_pe = 0;
        gather_dest dests[10];
        dests[0].pe = NS_CSS_PE_NONE;
        dests[0].out = matches;
        dests[0].var_out = var_matches;
        dests[0].pending_out = pending_matches;
        for (int pi = 0; pe_mask && pi < 9; pi++) {
            ns_css_pseudo_element pe = (pi == 0) ? NS_CSS_PE_BEFORE :
                                       (pi == 1) ? NS_CSS_PE_AFTER :
                                       (pi == 2) ? NS_CSS_PE_FIRST_LETTER :
                                       (pi == 3) ? NS_CSS_PE_FIRST_LINE :
                                       (pi == 4) ? NS_CSS_PE_SELECTION :
                                       (pi == 5) ? NS_CSS_PE_MARKER :
                                       (pi == 6) ? NS_CSS_PE_BACKDROP :
                                       (pi == 7) ? NS_CSS_PE_PLACEHOLDER :
                                                   NS_CSS_PE_FILE_SELECTOR_BUTTON;
            if (!(pe_mask & (1u << pe))) continue;
            ns_pe_gather *pg = &pe_g[n_pe];
            pg->pe = pe;
            if (!sc_pe_m[n_pe]) {
                sc_pe_m[n_pe] = g_array_new(FALSE, FALSE, sizeof(match_entry));
                sc_pe_v[n_pe] = g_array_new(FALSE, FALSE, sizeof(var_match));
                sc_pe_p[n_pe] = g_array_new(FALSE, FALSE, sizeof(pending_match));
            }
            pg->m = sc_pe_m[n_pe];
            pg->v = sc_pe_v[n_pe];
            pg->p = sc_pe_p[n_pe];
            g_array_set_size(pg->m, 0);
            g_array_set_size(pg->v, 0);
            g_array_set_size(pg->p, 0);
            dests[n_pe + 1].pe = pe;
            dests[n_pe + 1].out = pg->m;
            dests[n_pe + 1].var_out = pg->v;
            dests[n_pe + 1].pending_out = pg->p;
            n_pe++;
        }
        ns_css_ancestor_filter_subject(node);
        ns_css_gather_matches(ua, NS_CSS_ORIGIN_UA, 0, node, dests,
                              (guint)n_pe + 1, layer_ranks);
        for (gsize i = 0; i < n_author; i++)
            ns_css_gather_matches(author[i], NS_CSS_ORIGIN_AUTHOR,
                                  (int)(i + 1), node, dests,
                                  (guint)n_pe + 1, layer_ranks);

        ns_css_gather_element_declarations(node, matches, var_matches,
                                           pending_matches);

        share_key_t probe;
        gboolean have_key = FALSE;
        const ns_style *shared = NULL;
        if (g_style_share) {
            style_share_key(g_share_scratch, parent_style, *root_px, matches,
                            var_matches, pending_matches, pe_g, n_pe);
            probe.data = g_share_scratch->data;
            probe.len  = g_share_scratch->len;
            probe.hash = share_key_djb2(probe.data, probe.len);
            have_key = TRUE;
            shared = g_hash_table_lookup(g_style_share, &probe);
            gboolean uses_attr = pending_uses_attr(pending_matches);
            for (int i = 0; i < n_pe && !uses_attr; i++)
                uses_attr = pending_uses_attr(pe_g[i].p);
            if (uses_attr) {
                have_key = FALSE;
                shared = NULL;
                ns_css_incremental_exclude(node, TRUE);
            }
        }
        if (shared) {
            ns_style_free(s);
            s = ns_style_clone_shared(shared);
            display_contents_to_none(node, s);
            g_array_set_size(matches, 0);
            g_array_set_size(var_matches, 0);
            g_array_set_size(pending_matches, 0);
            g_ptr_array_set_size(owned_values, 0);
        } else {
            s->share_id = ++g_style_share_next_id;
            s->vars = ns_css_build_vars(parent_style ? parent_style->vars : NULL,
                                        var_matches, g_registered_props,
                                        g_var_adjust_cache);
            ns_css_resolve_pending(pending_matches, s->vars, g_registered_props,
                                   matches, owned_values, node);

            ns_css_cascade_apply(matches, s, parent_style, layout_parent,
                                 node->parent &&
                                     node->parent->kind == NS_NODE_DOCUMENT,
                                 *root_px);
            ns_css_compute_registered_vars(s, parent_style, g_registered_props,
                                           *root_px);
            strip_native_widget_decorations(node, s);
            if (display_contents_to_none(node, s)) have_key = FALSE;
            g_array_set_size(matches, 0);
            g_array_set_size(var_matches, 0);
            g_array_set_size(pending_matches, 0);
            g_ptr_array_set_size(owned_values, 0);

            for (int gi = 0; gi < n_pe; gi++) {
                ns_css_pseudo_element pe = pe_g[gi].pe;
                GArray *pm = pe_g[gi].m;
                GArray *pe_vars = pe_g[gi].v;
                GArray *pe_pending = pe_g[gi].p;
                if (pm->len == 0 && pe_pending->len == 0) continue;
                GPtrArray *pe_owned =
                    g_ptr_array_new_with_free_func(
                        (GDestroyNotify)ns_css_value_free);
                ns_style *ps = ns_style_alloc();
                ps->vars = ns_css_build_vars(s->vars, pe_vars, g_registered_props,
                                         g_var_adjust_cache);
                ns_css_resolve_pending(pe_pending, ps->vars, g_registered_props,
                                       pm, pe_owned, node);
                ns_css_cascade_apply(pm, ps, s,
                                     pe == NS_CSS_PE_BEFORE ||
                                         pe == NS_CSS_PE_AFTER
                                         ? s : NULL,
                                     FALSE, *root_px);
                ns_css_compute_registered_vars(ps, s, g_registered_props,
                                               *root_px);
                gboolean keep = TRUE;
                if (pe == NS_CSS_PE_BEFORE || pe == NS_CSS_PE_AFTER)
                    keep = ps->values[NS_CSS_CONTENT] != NULL;
                if (keep && (pe == NS_CSS_PE_BEFORE || pe == NS_CSS_PE_AFTER) &&
                    ns_display_is_none(ns_css_display_of(ps))) {
                    if (pe == NS_CSS_PE_BEFORE) s->hidden_before = ps;
                    else                        s->hidden_after  = ps;
                } else if (keep) {
                    if (pe == NS_CSS_PE_BEFORE)            s->before       = ps;
                    else if (pe == NS_CSS_PE_AFTER)        s->after        = ps;
                    else if (pe == NS_CSS_PE_FIRST_LETTER) s->first_letter = ps;
                    else if (pe == NS_CSS_PE_FIRST_LINE)   s->first_line   = ps;
                    else if (pe == NS_CSS_PE_SELECTION)    s->selection    = ps;
                    else if (pe == NS_CSS_PE_MARKER)       s->marker       = ps;
                    else if (pe == NS_CSS_PE_BACKDROP)     s->backdrop     = ps;
                    else if (pe == NS_CSS_PE_PLACEHOLDER)  s->placeholder  = ps;
                    else s->file_selector_button = ps;
                } else {
                    ns_style_free(ps);
                }
                g_ptr_array_free(pe_owned, TRUE);
            }
            if (have_key) {
                share_key_t *k = g_new(share_key_t, 1);
                k->len  = probe.len;
                k->hash = probe.hash;
                k->data = g_memdup2(probe.data, probe.len);
                g_hash_table_insert(g_style_share, k, s);
            }
        }
        }
        g_hash_table_insert(out, node, s);
        child_parent_style = s;
        child_layout_parent = ns_display_is_contents(ns_css_display_of(s))
            ? layout_parent : s;
        if (*root_px <= 0 &&
            s->values[NS_CSS_FONT_SIZE] &&
            s->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
            s->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_PX)
            *root_px = s->values[NS_CSS_FONT_SIZE]->u.length.v;
        nd_recurse_dirty = nd_node_dirty;
    }
    gboolean pushed = ns_css_container_stack_push(node);
    void *outer_filter = node->kind == NS_NODE_DOCUMENT && node->parent
        ? ns_css_ancestor_filter_save() : NULL;
    gboolean filter_element = ns_css_ancestor_filter_enter(node);
    for (ns_node *c = node->first_child; c; c = c->next_sibling)
        cascade_walk(c, ua, author, n_author, child_parent_style,
                     child_layout_parent, root_px,
                     layer_ranks, out, nd_recurse_dirty);
    if (filter_element) ns_css_ancestor_filter_leave(node);
    ns_css_ancestor_filter_restore(outer_filter);
    if (pushed) ns_css_container_stack_pop();
    if (frame_viewport) {
        g_viewport_w = frame_vw;
        g_viewport_h = frame_vh;
    }
    depth--;
}

GHashTable *
ns_css_compute(ns_node *doc,
               const ns_css_stylesheet *const *author_sheets,
               const ns_node *const *sheet_docs,
               gsize n_sheets)
{
    GHashTable *out = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                            NULL, (GDestroyNotify)ns_style_free);

    ns_css_language_cache_reset();

    const ns_css_stylesheet *cached_ua = ua_sheet_for(doc);

    gboolean profile = g_getenv("NS_PROFILE") != NULL;
    gint64 t0 = profile ? g_get_monotonic_time() : 0;
    (void)ns_css_rule_index_ensure(cached_ua);
    for (gsize i = 0; i < n_sheets; i++)
        (void)ns_css_rule_index_ensure(author_sheets[i]);
    gint64 t_idx = profile ? g_get_monotonic_time() : 0;

    GHashTable *layer_ranks =
        ns_css_layer_ranks_build(cached_ua, author_sheets, n_sheets);

    g_registered_props = g_hash_table_new(g_str_hash, g_str_equal);
    css_collect_property_rules(g_registered_props, cached_ua);
    for (gsize i = 0; i < n_sheets; i++)
        css_collect_property_rules(g_registered_props, author_sheets[i]);
    if (g_js_registered_props) {
        GHashTableIter it;
        gpointer k, v;
        g_hash_table_iter_init(&it, g_js_registered_props);
        while (g_hash_table_iter_next(&it, &k, &v))
            g_hash_table_replace(g_registered_props, k, v);
    }

    double root_px = 0;
    ns_css_decl_sheet_cache_trim();
    ns_css_container_stack_reset();
    if (!g_share_scratch)
        g_share_scratch = g_byte_array_sized_new(512);
    g_style_share = g_hash_table_new_full(share_key_hash, share_key_equal,
                                          share_key_free, NULL);
    g_var_adjust_cache = g_hash_table_new_full(
        g_direct_hash, g_direct_equal,
        (GDestroyNotify)ns_var_map_unref, (GDestroyNotify)ns_var_map_unref);
    ns_css_has_memo_begin();
    ns_css_selector_batch_begin();

    guint64 sig = incr_sheet_sig(cached_ua, author_sheets, n_sheets);
    gboolean incr_eligible =
        ns_css_restyle_prepare(cached_ua, author_sheets, n_sheets, sig);
    gboolean incr_usable = g_getenv("NS_NO_INCR_RESTYLE") == NULL
        && incr_eligible
        && fabs(g_incr_zoom - 1.0) <= 0.001;
    gboolean incr_want = incr_usable;
    guint64 cq_sig = ns_css_container_map_signature();
    g_incr_pass_active = incr_want
        && g_incr_prev_styles != NULL
        && g_incr_prev_doc == doc
        && g_incr_prev_sig == sig
        && g_incr_prev_cq_sig == cq_sig
        && ns_css_focus_node() == g_incr_prev_focus
        && ns_css_hover_node() == g_incr_prev_hover
        && ns_css_active_node() == g_incr_prev_active
        && ns_css_fullscreen_node() == g_incr_prev_fullscreen;
    g_incr_reused = 0;
    g_incr_recomputed = 0;

    ns_css_ancestor_filter_begin(ns_css_selector_attr_ancestor_hashes());
    GHashTable *outer_doc_sheets = g_doc_sheets;
    g_doc_sheets = doc_sheets_new(author_sheets, sheet_docs, n_sheets);
    cascade_walk(doc, cached_ua, author_sheets, n_sheets, NULL, NULL,
                 &root_px, layer_ranks, out, FALSE);
    g_clear_pointer(&g_doc_sheets, g_hash_table_destroy);
    g_doc_sheets = outer_doc_sheets;
    ns_css_ancestor_filter_end();

    if (incr_want) {
        GHashTable *new_prev = g_hash_table_new_full(
            g_direct_hash, g_direct_equal, NULL, (GDestroyNotify)ns_style_free);
        GHashTableIter pit; gpointer pk, pv;
        g_hash_table_iter_init(&pit, out);
        while (g_hash_table_iter_next(&pit, &pk, &pv)) {
            ((ns_style *)pv)->ref++;
            g_hash_table_insert(new_prev, pk, pv);
        }
        if (g_incr_before_styles) g_hash_table_destroy(g_incr_before_styles);
        g_incr_before_styles = g_incr_prev_styles;
        g_incr_prev_styles = new_prev;
        g_incr_prev_doc = doc;
        g_incr_prev_sig = sig;
        g_incr_prev_cq_sig = cq_sig;
        g_incr_prev_focus = ns_css_focus_node();
        g_incr_prev_hover = ns_css_hover_node();
        g_incr_prev_active = ns_css_active_node();
        g_incr_prev_fullscreen = ns_css_fullscreen_node();
        if (g_getenv("NS_PROFILE"))
            g_printerr("[incr] active=%d reused=%u recomputed=%u\n",
                       g_incr_pass_active, g_incr_reused, g_incr_recomputed);
    } else if (g_incr_prev_styles && !incr_usable) {
        g_hash_table_destroy(g_incr_prev_styles);
        g_incr_prev_styles = NULL;
        g_incr_prev_doc = NULL;
        if (g_incr_before_styles) g_hash_table_destroy(g_incr_before_styles);
        g_incr_before_styles = NULL;
    }
    ns_css_restyle_dirty_clear();

    ns_css_has_memo_end();
    ns_css_selector_batch_end();
    g_hash_table_destroy(g_style_share);
    g_style_share = NULL;
    g_hash_table_destroy(g_var_adjust_cache);
    g_var_adjust_cache = NULL;
    g_hash_table_destroy(layer_ranks);
    g_hash_table_destroy(g_registered_props);
    g_registered_props = NULL;
    gint64 t_cascade = profile ? g_get_monotonic_time() : 0;
    if (profile)
        g_printerr("[profile]   css.idx=%.1fms css.cascade=%.1fms\n",
                   (t_idx - t0) / 1000.0,
                   (t_cascade - t_idx) / 1000.0);
    return out;
}

void
ns_css_style_scale_font_size(ns_style *s, double factor)
{
    if (!s || !s->values[NS_CSS_FONT_SIZE] ||
        s->values[NS_CSS_FONT_SIZE]->kind != NS_CSS_V_LENGTH)
        return;
    ns_css_value_cow(s, NS_CSS_FONT_SIZE)->u.length.v *= factor;
}
