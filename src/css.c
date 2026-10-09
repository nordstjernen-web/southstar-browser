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

static guint64 g_style_share_next_id;

typedef struct {
    ns_css_pseudo_element pe;
    GArray *m;
    GArray *v;
    GArray *p;
} ns_pe_gather;

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
        if (!ns_css_frame_viewport_from_style(parent_style, &fw, &fh))
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

        const ns_style *shared = NULL;
        gboolean have_key = ns_css_style_share_find(parent_style, *root_px,
                                                    dests, (guint)n_pe + 1,
                                                    &shared);
        if (!have_key) ns_css_incremental_exclude(node, TRUE);
        if (shared) {
            ns_style_free(s);
            s = ns_style_clone_shared(shared);
            ns_css_display_contents_to_none(node, s);
            g_array_set_size(matches, 0);
            g_array_set_size(var_matches, 0);
            g_array_set_size(pending_matches, 0);
            g_ptr_array_set_size(owned_values, 0);
        } else {
            GHashTable *registered = ns_css_registered_props();
            s->share_id = ++g_style_share_next_id;
            s->vars = ns_css_build_vars(parent_style ? parent_style->vars : NULL,
                                        var_matches, registered,
                                        g_var_adjust_cache);
            ns_css_resolve_pending(pending_matches, s->vars, registered,
                                   matches, owned_values, node);

            ns_css_cascade_apply(matches, s, parent_style, layout_parent,
                                 node->parent &&
                                     node->parent->kind == NS_NODE_DOCUMENT,
                                 *root_px);
            ns_css_compute_registered_vars(s, parent_style, registered,
                                           *root_px);
            ns_css_strip_native_widget_decorations(node, s);
            if (ns_css_display_contents_to_none(node, s)) have_key = FALSE;
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
                ps->vars = ns_css_build_vars(s->vars, pe_vars, registered,
                                         g_var_adjust_cache);
                ns_css_resolve_pending(pe_pending, ps->vars, registered,
                                       pm, pe_owned, node);
                ns_css_cascade_apply(pm, ps, s,
                                     pe == NS_CSS_PE_BEFORE ||
                                         pe == NS_CSS_PE_AFTER
                                         ? s : NULL,
                                     FALSE, *root_px);
                ns_css_compute_registered_vars(ps, s, registered,
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
            if (have_key) ns_css_style_share_insert(s);
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

    ns_css_registered_props_begin(cached_ua, author_sheets, n_sheets);

    double root_px = 0;
    ns_css_decl_sheet_cache_trim();
    ns_css_container_stack_reset();
    ns_css_style_share_begin();
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
    ns_css_style_share_end();
    g_hash_table_destroy(g_var_adjust_cache);
    g_var_adjust_cache = NULL;
    g_hash_table_destroy(layer_ranks);
    ns_css_registered_props_end();
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
