/* Southstar — synchronous fetch/cascade/layout/capture pipeline.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "engine.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

#include "css.h"
#include "css_syntax.h"
#include "debuglog.h"
#include "image.h"
#include "paint.h"
#include "print.h"
#include "render.h"

GBytes *ns_engine_fetch_css_bytes(const char *url, const char *top_url,
                                  GHashTable *cache, gboolean strict_mime,
                                  const char *initiator,
                                  gboolean render_blocking, gboolean in_frame);
void ns_engine_remember_linked_css(const char *url, GBytes *bytes);

static guint64 g_engine_relayout_count;
static gint64  g_engine_relayout_us;

static void
ns_engine_perf_add_relayout(gint64 elapsed_us)
{
    g_engine_relayout_count++;
    g_engine_relayout_us += elapsed_us;
}

void
ns_engine_layout_perf(guint64 *relayouts, double *total_ms)
{
    if (relayouts) *relayouts = g_engine_relayout_count;
    if (total_ms)  *total_ms  = g_engine_relayout_us / 1000.0;
}

static gboolean
rel_has_token(const char *rel, const char *token)
{
    if (!rel || !token || !*token) return FALSE;
    gchar **parts = g_strsplit_set(rel, " \t\r\n\f", -1);
    gboolean found = FALSE;
    for (gchar **p = parts; *p; p++) {
        if (**p && g_ascii_strcasecmp(*p, token) == 0) {
            found = TRUE;
            break;
        }
    }
    g_strfreev(parts);
    return found;
}

static gboolean
rel_is_stylesheet(const char *rel)
{
    return rel_has_token(rel, "stylesheet") &&
           !rel_has_token(rel, "alternate");
}

static void
append_stylesheet_expanded(GPtrArray *out, ns_css_stylesheet *sh,
                           const char *base_url, const char *top_url,
                           GHashTable *seen, GHashTable *cache, int depth,
                           gboolean render_blocking, gboolean in_frame)
{
    if (!out || !sh) return;
    if (depth < NS_CSS_IMPORT_MAX_DEPTH && sh->imports) {
        for (guint i = 0; i < sh->imports->len; i++) {
            ns_css_import *im = &g_array_index(sh->imports, ns_css_import, i);
            if (!im->url || !*im->url) continue;
            if (im->media && *im->media &&
                !ns_css_media_query_matches(im->media))
                continue;
            char *abs = ns_url_resolve(base_url, im->url);
            if (!abs) continue;
            if (seen && g_hash_table_contains(seen, abs)) {
                g_free(abs);
                continue;
            }
            if (seen) g_hash_table_add(seen, g_strdup(abs));
            GBytes *bytes = ns_engine_fetch_css_bytes(abs, top_url, cache, TRUE, "css",
                                            render_blocking, in_frame);
            if (bytes) {
                ns_css_stylesheet *child =
                    ns_css_stylesheet_parse_import_cached(abs, im->layer_name,
                                                          bytes);
                if (child)
                    append_stylesheet_expanded(out, child, abs, top_url, seen,
                                               cache, depth + 1,
                                               render_blocking, in_frame);
                g_bytes_unref(bytes);
            }
            g_free(abs);
        }
    }
    ns_css_stylesheet_resolve_urls(sh, base_url);
    g_ptr_array_add(out, sh);
}

typedef struct {
    GPtrArray  *out;
    GHashTable *cache;
    GString    *run;
    GPtrArray  *run_chunks;
    const char *run_base;
    const char *top_url;
    gboolean    strict_css_mime;
    gboolean    media_seen;
    int         frame_depth;   /* inside a frame's document when > 0 */
    GPtrArray  *docs;          /* the document of each sheet in out, or NULL */
    ns_node    *doc;           /* the document whose sheets are collected */
} sheet_collect_ctx;

/* Whether a <style> or <link> sits in its document's <head>, which makes
 * the sheet render-blocking for resource timing. */
static gboolean
engine_node_in_head(const ns_node *n)
{
    for (const ns_node *p = n ? n->parent : NULL;
         p && p->kind == NS_NODE_ELEMENT; p = p->parent)
        if (ns_node_is_element_named(p, "head")) return TRUE;
    return FALSE;
}

static void
sheet_run_append(sheet_collect_ctx *cc, const char *css, const char *base_url)
{
    g_string_append(cc->run, css);
    g_string_append_c(cc->run, '\n');
    g_ptr_array_add(cc->run_chunks, g_strdup(css));
    cc->run_base = base_url;
}

static void
sheet_run_drop_repeats(sheet_collect_ctx *cc)
{
    guint n = cc->run_chunks->len;
    if (n < 2) return;
    GHashTable *later = g_hash_table_new(g_str_hash, g_str_equal);
    gboolean *drop = g_new0(gboolean, n);
    gboolean any = FALSE;
    for (guint i = n; i-- > 0; ) {
        const char *chunk = g_ptr_array_index(cc->run_chunks, i);
        if (strchr(chunk, '@')) continue;
        if (g_hash_table_contains(later, chunk)) drop[i] = any = TRUE;
        else g_hash_table_add(later, (gpointer)chunk);
    }
    if (any) {
        g_string_set_size(cc->run, 0);
        for (guint i = 0; i < n; i++) {
            if (drop[i]) continue;
            g_string_append(cc->run, g_ptr_array_index(cc->run_chunks, i));
            g_string_append_c(cc->run, '\n');
        }
    }
    g_free(drop);
    g_hash_table_destroy(later);
}

static void
sheet_run_flush(sheet_collect_ctx *cc)
{
    if (!cc->run || cc->run->len == 0) return;
    sheet_run_drop_repeats(cc);
    g_ptr_array_set_size(cc->run_chunks, 0);
    ns_css_stylesheet *sh =
        ns_css_merged_styles_cached(cc->run->str, (gssize)cc->run->len,
                                    cc->run_base);
    if (sh) {
        GHashTable *seen = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                 g_free, NULL);
        append_stylesheet_expanded(cc->out, sh, cc->run_base, cc->top_url, seen,
                                   cc->cache, 0, FALSE, cc->frame_depth > 0);
        g_hash_table_destroy(seen);
    }
    g_string_set_size(cc->run, 0);
    cc->run_base = NULL;
}

/* The sheets added since the last call are those of the document being
 * walked. */
static void
sheet_docs_sync(sheet_collect_ctx *cc)
{
    sheet_run_flush(cc);
    if (!cc->docs) return;
    while (cc->docs->len < cc->out->len)
        g_ptr_array_add(cc->docs, cc->doc);
}

static double
frame_dimension_from_style_attr(const ns_node *frame, const char *prop,
                                double *out)
{
    const char *style = ns_element_get_attr(frame, "style");
    if (!style) return FALSE;
    gsize plen = strlen(prop);
    for (const char *p = style; (p = strstr(p, prop)) != NULL; p += plen) {
        if (p != style && (g_ascii_isalnum(p[-1]) || p[-1] == '-'))
            continue;
        const char *q = p + plen;
        while (*q == ' ' || *q == '\t') q++;
        if (*q != ':') continue;
        q++;
        while (*q == ' ' || *q == '\t') q++;
        char *end = NULL;
        double v = g_ascii_strtod(q, &end);
        if (end && end != q && v >= 0 &&
            (g_ascii_strncasecmp(end, "px", 2) == 0 ||
             *end == ';' || *end == '\0' || *end == ' ')) {
            *out = v;
            return TRUE;
        }
    }
    return FALSE;
}

static gboolean
frame_dimension_from_attr(const ns_node *frame, const char *attr, double *out)
{
    const char *av = ns_element_get_attr(frame, attr);
    if (!av || !*av) return FALSE;
    char *end = NULL;
    double v = g_ascii_strtod(av, &end);
    if (end == av || v < 0) return FALSE;
    *out = v;
    return TRUE;
}

typedef struct {
    double w, h;
} ns_collect_frame_vp;

static gboolean
css_has_viewport_media(const char *css)
{
    if (!css) return FALSE;
    for (const char *p = css; (p = strstr(p, "@media")) != NULL; p += 6) {
        const char *end = strchr(p, '{');
        if (!end) break;
        for (const char *q = p; q < end; q++)
            if (g_ascii_strncasecmp(q, "width", 5) == 0 ||
                g_ascii_strncasecmp(q, "height", 6) == 0 ||
                g_ascii_strncasecmp(q, "aspect-ratio", 12) == 0 ||
                g_ascii_strncasecmp(q, "orientation", 11) == 0)
                return TRUE;
    }
    return FALSE;
}

static GHashTable *g_collect_frame_vp;
static GHashTable *g_viewport_media_memo;

static gboolean
css_bytes_have_viewport_media(GBytes *bytes)
{
    if (!g_viewport_media_memo)
        g_viewport_media_memo =
            g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                  (GDestroyNotify)g_bytes_unref, NULL);
    gpointer memo = g_hash_table_lookup(g_viewport_media_memo, bytes);
    if (memo) return GPOINTER_TO_INT(memo) == 2;
    if (g_hash_table_size(g_viewport_media_memo) >= 256)
        g_hash_table_remove_all(g_viewport_media_memo);
    gsize len = 0;
    const char *data = g_bytes_get_data(bytes, &len);
    char *terminated = g_strndup(data, len);
    gboolean seen = css_has_viewport_media(terminated);
    g_free(terminated);
    g_hash_table_insert(g_viewport_media_memo, g_bytes_ref(bytes),
                        GINT_TO_POINTER(seen ? 2 : 1));
    return seen;
}

static void
frame_viewport_px(const ns_node *frame, double *w, double *h)
{
    gboolean have_w = frame_dimension_from_style_attr(frame, "width", w);
    gboolean have_h = frame_dimension_from_style_attr(frame, "height", h);
    if (!have_w || !have_h) {
        double lw = 0, lh = 0;
        if (ns_layout_frame_viewport(frame, &lw, &lh)) {
            if (!have_w) { *w = lw; have_w = TRUE; }
            if (!have_h) { *h = lh; have_h = TRUE; }
        }
    }
    if (!have_w && !frame_dimension_from_attr(frame, "width", w)) *w = 300;
    if (!have_h && !frame_dimension_from_attr(frame, "height", h)) *h = 150;
}

static void
frame_viewport_measured(const ns_node *frame, double *w, double *h)
{
    *w = 0;
    *h = 0;
    gboolean have_w = frame_dimension_from_style_attr(frame, "width", w);
    gboolean have_h = frame_dimension_from_style_attr(frame, "height", h);
    if (!have_w || !have_h) {
        double lw = 0, lh = 0;
        if (ns_layout_frame_viewport(frame, &lw, &lh)) {
            if (!have_w) { *w = lw; have_w = TRUE; }
            if (!have_h) { *h = lh; have_h = TRUE; }
        }
    }
    if (!have_w) have_w = frame_dimension_from_attr(frame, "width", w);
    if (!have_h) have_h = frame_dimension_from_attr(frame, "height", h);
    if (!have_w || !have_h) { *w = 0; *h = 0; }
}

static void
frame_viewport_record(const ns_node *frame, double w, double h)
{
    if (!g_collect_frame_vp) return;
    ns_collect_frame_vp *e = g_new0(ns_collect_frame_vp, 1);
    e->w = w;
    e->h = h;
    g_hash_table_insert(g_collect_frame_vp, (gpointer)frame, e);
}

static gboolean
frame_viewports_disagree_with_layout(void)
{
    if (!g_collect_frame_vp) return FALSE;
    GHashTableIter it;
    gpointer k, v;
    g_hash_table_iter_init(&it, g_collect_frame_vp);
    while (g_hash_table_iter_next(&it, &k, &v)) {
        const ns_collect_frame_vp *e = v;
        double lw = 0, lh = 0;
        if (!ns_layout_frame_viewport(k, &lw, &lh)) continue;
        if (fabs(e->w - lw) > 0.01 || fabs(e->h - lh) > 0.01) return TRUE;
    }
    return FALSE;
}

static gboolean
sheet_type_is_css(const char *type)
{
    if (!type || !*type) return TRUE;
    gsize n = strcspn(type, ";");
    while (n > 0 && g_ascii_isspace(type[n - 1])) n--;
    return n == 8 && g_ascii_strncasecmp(type, "text/css", 8) == 0;
}

static gboolean
style_sheet_enabled(const ns_node *style)
{
    return !(style->flags & NS_NODE_SHEET_DISABLED) &&
           sheet_type_is_css(ns_element_get_attr(style, "type"));
}

static gboolean
link_sheet_enabled(const ns_node *link)
{
    return !ns_element_get_attr(link, "disabled") &&
           sheet_type_is_css(ns_element_get_attr(link, "type"));
}

static void collect_stylesheets_walk(ns_node *n, const char *base_url,
                                     sheet_collect_ctx *cc, int depth);

/* A frame's document has sheets of its own, which style its elements only
 * (CSSOM: a CSS style sheet belongs to the document of its owner node). */
static void
collect_frame_children(ns_node *frame, const char *base_url,
                       sheet_collect_ctx *cc, int depth)
{
    for (ns_node *c = frame->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_DOCUMENT) {
            collect_stylesheets_walk(c, base_url, cc, depth + 1);
            continue;
        }
        sheet_docs_sync(cc);
        ns_node *outer = cc->doc;
        cc->doc = c;
        collect_stylesheets_walk(c, base_url, cc, depth + 1);
        sheet_docs_sync(cc);
        cc->doc = outer;
    }
}

static void
collect_adopted_css(ns_node *root, const char *base_url, sheet_collect_ctx *cc)
{
    if (root->kind != NS_NODE_ELEMENT) return;
    char *css = ns_css_shadow_adopted_css(root);
    if (!css) return;
    if (css_has_viewport_media(css)) cc->media_seen = TRUE;
    gboolean alone = !ns_css_syntax_is_self_contained(css, strlen(css));
    if (alone || (cc->run_base && cc->run_base != base_url))
        sheet_run_flush(cc);
    sheet_run_append(cc, css, base_url);
    if (alone) sheet_run_flush(cc);
    g_free(css);
}

static void
collect_stylesheets_walk(ns_node *n, const char *base_url,
                         sheet_collect_ctx *cc, int depth)
{
    if (!n || depth >= 512 || ns_node_is_element_named(n, "noscript")) return;
    if (ns_node_is_element_named(n, "iframe") ||
        ns_node_is_element_named(n, "frame") ||
        ns_node_is_element_named(n, "object")) {
        sheet_run_flush(cc);
        const char *furl = ns_element_get_attr(n, "data-nd-frame-url");
        if (furl && *furl) base_url = furl;
        gboolean has_doc = FALSE;
        for (const ns_node *c = n->first_child; c; c = c->next_sibling)
            if (c->kind == NS_NODE_DOCUMENT) { has_doc = TRUE; break; }
        if (has_doc) {
            double fw, fh;
            frame_viewport_px(n, &fw, &fh);
            gboolean outer_media = cc->media_seen;
            cc->media_seen = FALSE;
            ns_css_media_viewport_push(fw, fh);
            cc->frame_depth++;
            collect_frame_children(n, base_url, cc, depth);
            sheet_run_flush(cc);
            cc->frame_depth--;
            gboolean frame_media = cc->media_seen;
            ns_css_media_viewport_pop();
            cc->media_seen = outer_media || frame_media;
            if (frame_media) frame_viewport_record(n, fw, fh);
            return;
        }
    }
    GPtrArray *out = cc->out;
    GHashTable *cache = cc->cache;
    if (ns_node_is_element_named(n, "style") && style_sheet_enabled(n)) {
        char *css = ns_css_style_element_text(n);
        if (css) {
            if (css_has_viewport_media(css)) cc->media_seen = TRUE;
            if (cc->run_base && cc->run_base != base_url)
                sheet_run_flush(cc);
            if (strstr(css, "@import") ||
                !ns_css_syntax_is_self_contained(css, strlen(css))) {
                sheet_run_flush(cc);
                ns_css_stylesheet *sh =
                    ns_css_stylesheet_from_style_element_cached(n);
                if (sh) {
                    GHashTable *seen =
                        g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, NULL);
                    append_stylesheet_expanded(out, sh, base_url, cc->top_url,
                                               seen, cache, 0,
                                               engine_node_in_head(n),
                                               cc->frame_depth > 0);
                    g_hash_table_destroy(seen);
                }
            } else {
                sheet_run_append(cc, css, base_url);
            }
            g_free(css);
        }
    } else if (ns_node_is_element_named(n, "link") && base_url) {
        sheet_run_flush(cc);
        const char *rel = ns_element_get_attr(n, "rel");
        const char *href = ns_element_get_attr(n, "href");
        const char *media = ns_element_get_attr(n, "media");
        if (href && *href && rel_is_stylesheet(rel) &&
            link_sheet_enabled(n) &&
            (!media || !*media || ns_css_media_query_matches(media))) {
            char *abs = ns_url_resolve(base_url, href);
            gboolean blocking = engine_node_in_head(n);
            GBytes *bytes = ns_engine_fetch_css_bytes(abs, cc->top_url, cache,
                                            cc->strict_css_mime, "link",
                                            blocking, cc->frame_depth > 0);
            if (bytes) {
                gsize len = 0;
                const char *data = g_bytes_get_data(bytes, &len);
                ns_engine_remember_linked_css(abs, bytes);
                if (css_bytes_have_viewport_media(bytes)) cc->media_seen = TRUE;
                ns_css_stylesheet *sh =
                    ns_css_stylesheet_parse_url_cached(abs, data, (gssize)len);
                if (sh) {
                    GHashTable *seen =
                        g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, NULL);
                    if (abs) g_hash_table_add(seen, g_strdup(abs));
                    append_stylesheet_expanded(out, sh, abs, cc->top_url, seen,
                                               cache, 0, blocking,
                                               cc->frame_depth > 0);
                    g_hash_table_destroy(seen);
                }
                g_bytes_unref(bytes);
            }
            g_free(abs);
        }
    }
    for (ns_node *c = n->first_child; c; c = c->next_sibling)
        collect_stylesheets_walk(c, base_url, cc, depth + 1);
    collect_adopted_css(n, base_url, cc);
}

void
ns_engine_collect_stylesheets(ns_node *doc, const char *base_url,
                              GPtrArray *out, GPtrArray *out_docs,
                              GHashTable *css_cache)
{
    if (!g_collect_frame_vp)
        g_collect_frame_vp = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                                   NULL, g_free);
    else
        g_hash_table_remove_all(g_collect_frame_vp);
    sheet_collect_ctx cc = {
        .out = out, .cache = css_cache,
        .run = g_string_new(NULL),
        .run_chunks = g_ptr_array_new_with_free_func(g_free),
        .run_base = NULL,
        .top_url = base_url,
        .strict_css_mime = doc && !(doc->flags & NS_NODE_QUIRKS),
        .docs = out_docs, .doc = doc,
    };
    collect_stylesheets_walk(doc, base_url, &cc, 0);
    sheet_docs_sync(&cc);
    g_string_free(cc.run, TRUE);
    g_ptr_array_free(cc.run_chunks, TRUE);
    ns_css_style_element_cache_end();
}

GHashTable *
ns_engine_compute_cascade(ns_node *doc, const char *base_url,
                          GHashTable *css_cache, ns_anim *anim)
{
    ns_css_set_frame_viewport_cb(frame_viewport_measured);
    ns_css_relayout_enter();
    ns_css_set_doc_base(base_url);
    ns_css_style_element_cache_begin();
    GPtrArray *page_sheets = g_ptr_array_new();
    GPtrArray *sheet_docs = g_ptr_array_new();
    ns_engine_collect_stylesheets(doc, base_url, page_sheets, sheet_docs,
                                  css_cache);
    if (anim)
        for (guint i = 0; i < page_sheets->len; i++)
            ns_anim_load_from_stylesheet(anim, g_ptr_array_index(page_sheets, i));
    GHashTable *styles = ns_css_compute(doc,
        (const ns_css_stylesheet *const *)page_sheets->pdata,
        (const ns_node *const *)sheet_docs->pdata, page_sheets->len);
    for (guint i = 0; i < page_sheets->len; i++)
        ns_css_stylesheet_free(g_ptr_array_index(page_sheets, i));
    g_ptr_array_free(page_sheets, TRUE);
    g_ptr_array_free(sheet_docs, TRUE);
    ns_css_relayout_leave();
    return styles;
}

GHashTable *
ns_engine_relayout(ns_node *doc, const char *base_url,
                   int viewport_width, double viewport_height,
                   ns_image_cache *images, ns_anim *anim,
                   ns_js *js, GHashTable *css_cache,
                   const ns_node *focused, const ns_node *hover,
                   gsize caret_byte,
                   gsize sel_anchor_byte, ns_box **out_layout)
{
    ns_css_set_frame_viewport_cb(frame_viewport_measured);
    ns_css_relayout_enter();
    ns_css_set_doc_base(base_url);
    ns_css_style_element_cache_begin();
    GPtrArray *sheets = g_ptr_array_new();
    GPtrArray *sheet_docs = g_ptr_array_new();
    ns_engine_collect_stylesheets(doc, base_url, sheets, sheet_docs, css_cache);

    ns_render_ctx rc = {
        .doc             = doc,
        .sheets          = (const ns_css_stylesheet *const *)sheets->pdata,
        .sheet_docs      = (const ns_node *const *)sheet_docs->pdata,
        .n_sheets        = sheets->len,
        .viewport_width  = (double)viewport_width,
        .viewport_height = viewport_height > 0 ? viewport_height
                                               : (double)viewport_width * 0.75,
        .zoom            = 1.0,
        .images          = images,
        .base_url        = base_url,
        .anim            = anim,
        .js              = js,
        .focused_input   = focused,
        .hover_node      = hover,
        .caret_byte      = caret_byte,
        .sel_anchor_byte = sel_anchor_byte,
    };
    static int profile_env = -1;
    if (profile_env < 0)
        profile_env = g_getenv("NS_PROFILE") != NULL ? 1 : 0;
    GHashTable *styles;
    gint64 relayout_t0 = g_get_monotonic_time();
    if (profile_env) {
        ns_render_profile prof;
        gint64 t0 = g_get_monotonic_time();
        styles = ns_render_relayout_profile(&rc, out_layout, &prof);
        gint64 total = g_get_monotonic_time() - t0;
        guint nstyles = styles ? g_hash_table_size(styles) : 0u;
        g_printerr("[profile] relayout vw=%d nodes=%u total=%.2fms "
                   "css=%.2f style=%.2f layout=%.2f",
                   viewport_width, nstyles, total / 1000.0,
                   prof.css1_us / 1000.0, prof.style1_us / 1000.0,
                   prof.layout1_us / 1000.0);
        if (prof.container_pass)
            g_printerr(" | containers=%u passes=%u cq_collect=%.2f css2=%.2f "
                       "style2=%.2f layout2=%.2f",
                       prof.containers, prof.container_passes,
                       prof.container_us / 1000.0,
                       prof.css2_us / 1000.0, prof.style2_us / 1000.0,
                       prof.layout2_us / 1000.0);
        g_printerr("\n");
    } else {
        styles = ns_render_relayout(&rc, out_layout);
    }
    if (frame_viewports_disagree_with_layout()) {
        if (js) {
            ns_js_set_layout_root(js, NULL);
            ns_js_set_style_table(js, NULL);
        }
        if (out_layout && *out_layout) {
            ns_box_free(*out_layout);
            *out_layout = NULL;
        }
        if (styles) g_hash_table_destroy(styles);
        for (guint i = 0; i < sheets->len; i++)
            ns_css_stylesheet_free(g_ptr_array_index(sheets, i));
        g_ptr_array_set_size(sheets, 0);
        g_ptr_array_set_size(sheet_docs, 0);
        ns_css_style_element_cache_begin();
        ns_engine_collect_stylesheets(doc, base_url, sheets, sheet_docs,
                                      css_cache);
        rc.sheets     = (const ns_css_stylesheet *const *)sheets->pdata;
        rc.sheet_docs = (const ns_node *const *)sheet_docs->pdata;
        rc.n_sheets   = sheets->len;
        styles = ns_render_relayout(&rc, out_layout);
    }
    ns_engine_perf_add_relayout(g_get_monotonic_time() - relayout_t0);
    ns_debug_log_emit(NS_DLOG_RENDER, "relayout", "styles=%u vw=%d",
                      styles ? g_hash_table_size(styles) : 0u, viewport_width);

    for (guint i = 0; i < sheets->len; i++)
        ns_css_stylesheet_free(g_ptr_array_index(sheets, i));
    g_ptr_array_free(sheets, TRUE);
    g_ptr_array_free(sheet_docs, TRUE);
    ns_css_relayout_leave();
    return styles;
}
