/* Southstar — public C embedding API implementation.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "libsouthstar.h"
#include "layers.h"

#include <cairo.h>
#include <gio/gio.h>
#include <glib.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

#include "anim.h"
#include "config.h"
#include "css.h"
#include "dom.h"
#include "engine.h"
#include "forms.h"
#include "html.h"
#include "image.h"
#include "js.h"
#include "layout.h"
#include "net.h"
#include "paint.h"
#include "pdf.h"
#include "render.h"
#include "safebrowsing.h"
#include "selection.h"
#include "video.h"

struct ns_browser {
    ns_node        *doc;
    ns_box         *layout;
    GHashTable     *styles;
    ns_js          *js;
    ns_anim        *anim;
    ns_image_cache *images;
    ns_video_cache *videos;
    GHashTable     *css_cache;
    char           *base_url;
    char           *doc_charset;
    char           *doc_language;
    int             vw;
    double          vh;
    gboolean        images_fetched;
    gboolean        has_deferred_lazy;
    gboolean        bfcache_ok;
    double          cur_scroll_x;
    double          cur_scroll_y;
    double          cur_scale;
    double          js_scroll_x;
    double          js_scroll_y;
    double          cur_viewport_h;
    int             pending_scroll_x;
    int             pending_scroll_y;
    gboolean        pending_scroll;
    const ns_node  *scroll_anchor;
    int             scroll_anchor_y;
    GPtrArray      *img_sessions;
    guint           image_arrivals_since_layout;
    guint           media_events_source;
    gboolean        images_arrived_since_layout;
    gint64          load_delay_deadline_us;
    GHashTable     *img_requested;
    gboolean        dirty;
    double          dppx;
    gboolean        cascade_dirty;
    gboolean        relaying;
    char           *pending_nav;
    gboolean        soft_nav_pushed;
    char           *pending_download;
    char           *pending_clipboard;
    char           *pending_window_action;
    GString        *pending_audio;
    char           *refresh_url;
    gint64          refresh_due_us;
    char           *pending_post_body;
    gsize           pending_post_len;
    char           *pending_post_ct;
    gsize           caret_byte;
    gsize           sel_anchor_byte;
    const ns_node  *caret_blink_node;
    gsize           caret_blink_byte;
    gsize           caret_blink_anchor;
    gint64          caret_blink_epoch_us;
    gboolean        caret_blink_active;
    gboolean        caret_paint_visible;
    ns_selection    selection;
    gboolean        selection_dragged;
    const ns_node  *hover_node;
    const ns_node  *open_select;
    gboolean        datalist_suppressed;
    const ns_node  *press_node;
    int             press_x;
    int             press_y;
    int             press_mods;
    gboolean        press_active;
    gboolean        keydown_prevented;
    char           *search_query;
    gboolean        search_case;
    const ns_box   *search_active;
    GString        *console_buf;
    guint64         layout_sig[2];
    int             layout_osc;
    gint64          last_layout_us;
    gint64          damp_until_us;
    gboolean        damp_logged;
    gint64          hover_relayout_us;
    gint64          relayout_cost_us;
    gboolean        hover_restyle_pending;
    ns_box         *sb_box;
    const ns_node  *sb_node;
    double          sb_grab;
    gboolean        sb_dragging;
    int             security;
    char           *remote_ip;
};

#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(struct ns_browser) == 656 &&
                G_STRUCT_OFFSET(struct ns_browser, base_url) == 64 &&
                G_STRUCT_OFFSET(struct ns_browser, vh) == 96 &&
                G_STRUCT_OFFSET(struct ns_browser, cur_viewport_h) == 160 &&
                G_STRUCT_OFFSET(struct ns_browser, scroll_anchor_y) == 192 &&
                G_STRUCT_OFFSET(struct ns_browser, media_events_source) == 212 &&
                G_STRUCT_OFFSET(struct ns_browser, dirty) == 240 &&
                G_STRUCT_OFFSET(struct ns_browser, soft_nav_pushed) == 272 &&
                G_STRUCT_OFFSET(struct ns_browser, refresh_due_us) == 320 &&
                G_STRUCT_OFFSET(struct ns_browser, caret_blink_node) == 368 &&
                G_STRUCT_OFFSET(struct ns_browser, selection) == 408 &&
                G_STRUCT_OFFSET(struct ns_browser, datalist_suppressed) == 472 &&
                G_STRUCT_OFFSET(struct ns_browser, keydown_prevented) == 504 &&
                G_STRUCT_OFFSET(struct ns_browser, layout_sig) == 544 &&
                G_STRUCT_OFFSET(struct ns_browser, damp_logged) == 584 &&
                G_STRUCT_OFFSET(struct ns_browser, sb_grab) == 632 &&
                G_STRUCT_OFFSET(struct ns_browser, remote_ip) == 648);
#endif

ns_browser *ns_browser_core_build(ns_node *doc, char *base, int viewport_width,
                                  double viewport_height, int settle_ms,
                                  gboolean bfcache_ok, char *refresh_hdr,
                                  char *doc_language, char *csp_header,
                                  char *doc_charset, const char *url,
                                  const ns_js_navigation_timing *timing);
void  ns_browser_core_relayout(ns_browser *b);
void  ns_browser_core_prune(ns_browser *b);
void  ns_browser_core_damp_reset(ns_browser *b);
void  ns_browser_core_ensure_images(ns_browser *b);
char *ns_browser_core_resolve_navigation(ns_browser *b, const char *href);
void  ns_browser_core_sync_js_selection(ns_browser *b);
gsize ns_browser_core_utf8_boundary(const char *s, gsize off);
void  ns_browser_core_submit_form(ns_browser *b, const ns_node *clicked);
void  ns_browser_core_js_download(const char *url, const char *filename,
                                  ns_browser *b);

static char *
resolve_local_path(const char *url)
{
    if (!url || strstr(url, "://") ||
        g_str_has_prefix(url, "about:") || g_str_has_prefix(url, "data:") ||
        !g_file_test(url, G_FILE_TEST_EXISTS))
        return NULL;
    char *abs = g_canonicalize_filename(url, NULL);
    char *file_url = g_filename_to_uri(abs, NULL, NULL);
    g_free(abs);
    return file_url;
}

static gboolean
browser_content_type_starts(const char *content_type, const char *prefix)
{
    return content_type && prefix &&
        g_ascii_strncasecmp(content_type, prefix, strlen(prefix)) == 0;
}

static gboolean
browser_content_type_is_html(const char *content_type)
{
    return browser_content_type_starts(content_type, "text/html") ||
           browser_content_type_starts(content_type, "application/xhtml");
}

static gboolean
browser_content_type_is_json(const char *content_type)
{
    return browser_content_type_starts(content_type, "application/json") ||
           browser_content_type_starts(content_type, "text/json") ||
           (content_type && strstr(content_type, "+json") != NULL);
}

static gboolean
browser_content_type_is_xml(const char *content_type)
{
    if (!content_type) return FALSE;
    if (strstr(content_type, "xhtml") || strstr(content_type, "svg"))
        return FALSE;
    return browser_content_type_starts(content_type, "text/xml") ||
           browser_content_type_starts(content_type, "application/xml") ||
           strstr(content_type, "+xml") != NULL;
}

static gboolean
browser_content_type_is_pdf(const char *content_type)
{
    return browser_content_type_starts(content_type, "application/pdf") ||
           browser_content_type_starts(content_type, "application/x-pdf");
}

static char *
browser_text_document(const char *url, const char *text)
{
    char *esc_url = ns_html_escape_text(url && *url ? url : "text file");
    char *esc_text = ns_html_escape_text(text ? text : "");
    char *html = g_strconcat(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>",
        esc_url,
        "</title><style>"
        "body{margin:0;background:#fff;color:#111}"
        "pre{margin:0;padding:12px;font:13px/1.45 ui-monospace,"
        "\"SF Mono\",Menlo,Consolas,monospace;white-space:pre-wrap;"
        "overflow-wrap:anywhere}"
        "</style></head><body><pre>",
        esc_text,
        "</pre></body></html>",
        NULL);
    g_free(esc_url);
    g_free(esc_text);
    return html;
}

static void
browser_prepare_document_response(ns_response *resp)
{
    if (!resp || !resp->body || !resp->content_type)
        return;
    const char *final_url = resp->final_url ? resp->final_url : "";
    char *html = NULL;
    if (browser_content_type_starts(resp->content_type, "image/")) {
        html = ns_html_image_document(final_url);
    } else if (browser_content_type_is_pdf(resp->content_type)) {
        html = ns_pdf_document_html(resp->body->data, resp->body->len,
                                    final_url);
    } else if (browser_content_type_is_json(resp->content_type)) {
        char *decoded = ns_html_decode_body_full((const char *)resp->body->data,
                                                 resp->body->len,
                                                 resp->content_type, NULL);
        html = ns_html_json_document(final_url, decoded,
                                     decoded ? strlen(decoded) : 0);
        if (!html) html = browser_text_document(final_url, decoded);
        g_free(decoded);
    } else if (browser_content_type_is_xml(resp->content_type)) {
        char *decoded = ns_html_decode_body_full((const char *)resp->body->data,
                                                 resp->body->len,
                                                 resp->content_type, NULL);
        html = ns_html_xml_document(final_url, decoded,
                                    decoded ? strlen(decoded) : 0);
        if (!html) html = browser_text_document(final_url, decoded);
        g_free(decoded);
    } else if (browser_content_type_starts(resp->content_type, "text/") &&
               !browser_content_type_is_html(resp->content_type)) {
        char *decoded = ns_html_decode_body_full((const char *)resp->body->data,
                                                 resp->body->len,
                                                 resp->content_type, NULL);
        html = browser_text_document(final_url, decoded);
        g_free(decoded);
    }
    if (!html)
        return;
    g_byte_array_set_size(resp->body, 0);
    g_byte_array_append(resp->body, (const guint8 *)html, strlen(html));
    g_free(html);
    g_free(resp->content_type);
    resp->content_type = g_strdup("text/html; charset=utf-8");
}

static gboolean
headers_have_no_store(const char *raw)
{
    if (!raw) return FALSE;
    char *low = g_ascii_strdown(raw, -1);
    gboolean found = strstr(low, "no-store") != NULL;
    g_free(low);
    return found;
}

static char *g_pending_referrer;
static int g_pending_user_activated = -1;

void
ns_browser_set_next_referrer(const char *url)
{
    g_free(g_pending_referrer);
    g_pending_referrer = (url && *url) ? g_strdup(url) : NULL;
}

void
ns_browser_set_next_user_activated(int user_activated)
{
    g_pending_user_activated = user_activated ? 1 : 0;
}

void
ns_browser_set_color_scheme(int dark)
{
    ns_css_set_color_scheme(dark ? NS_CSS_COLOR_SCHEME_DARK
                                 : NS_CSS_COLOR_SCHEME_LIGHT);
}

void
ns_browser_set_reduced_motion(int reduce)
{
    ns_css_set_reduced_motion(reduce ? NS_CSS_REDUCED_MOTION_REDUCE
                                     : NS_CSS_REDUCED_MOTION_NO_PREFERENCE);
}

static ns_browser *
browser_open_common(const char *url, int viewport_width, double viewport_height,
                    int settle_ms,
                    const void *body, size_t body_len, const char *content_type)
{
    if (!url || !*url) return NULL;

    g_autofree char *referrer = g_pending_referrer;
    g_pending_referrer = NULL;
    int pending_user_activated = g_pending_user_activated;
    g_pending_user_activated = -1;

    if (g_str_has_prefix(url, NS_UNSAFE_CONTINUE_SCHEME)) {
        const char *rest = url;
        while (g_str_has_prefix(rest, NS_UNSAFE_CONTINUE_SCHEME))
            rest += strlen(NS_UNSAFE_CONTINUE_SCHEME);
        if (!*rest) return NULL;
        char *real = g_strdup(rest);
        char *host = ns_url_host_from(real);
        if (host) {
            ns_safebrowsing_allow_host(host);
            g_free(host);
        }
        ns_browser *b = browser_open_common(real, viewport_width,
                                            viewport_height, settle_ms,
                                            body, body_len, content_type);
        g_free(real);
        return b;
    }

    if (!body) {
        char *host = ns_url_host_from(url);
        if (host && ns_safebrowsing_blocked(host)) {
            char *html = ns_safebrowsing_interstitial(url, host);
            g_free(host);
            ns_node *doc = ns_html_parse(html, html ? (gssize)strlen(html) : 0);
            g_free(html);
            return ns_browser_core_build(doc, g_strdup(url), viewport_width,
                                          viewport_height, settle_ms, FALSE,
                                          NULL, NULL, NULL, g_strdup("UTF-8"),
                                          url, NULL);
        }
        g_free(host);
    }

    char *file_url = resolve_local_path(url);
    const char *fetch_url = file_url ? file_url : url;

    char *stripped_url = body ? NULL : ns_url_strip_tracking_params(fetch_url);
    if (stripped_url)
        fetch_url = stripped_url;

    char *https_url = body ? NULL : ns_net_https_first_upgrade(fetch_url);

    GError *err = NULL;
    ns_response *resp = NULL;
    gboolean user_activated = pending_user_activated >= 0
        ? pending_user_activated != 0 : referrer == NULL;
    if (https_url) {
        resp = ns_engine_navigate_blocking(https_url, referrer,
                                           user_activated, &err);
        if (resp && !resp->error && resp->body) {
            fetch_url = https_url;
        } else {
            if (resp) ns_response_free(resp);
            resp = NULL;
            g_clear_error(&err);
        }
    }
    if (!resp)
        resp = body
            ? ns_engine_navigate_post_blocking(
                  fetch_url, referrer, body, body_len, content_type,
                  user_activated, &err)
            : ns_engine_navigate_blocking(fetch_url, referrer,
                                          user_activated, &err);
    if (resp && !body && (resp->error || resp->status >= 400) &&
        (!resp->body || resp->body->len == 0)) {
        gboolean tls_failure = resp->error &&
            g_str_has_prefix(fetch_url, "https://");
        char *html = ns_build_error_page(fetch_url,
                                         resp->error ? 0 : resp->status,
                                         resp->error);
        if (html) {
            if (!resp->body)
                resp->body = g_byte_array_new();
            g_byte_array_set_size(resp->body, 0);
            g_byte_array_append(resp->body, (const guint8 *)html, strlen(html));
            g_free(html);
            g_free(resp->error);
            resp->error = NULL;
            g_free(resp->content_type);
            resp->content_type = g_strdup("text/html; charset=utf-8");
            g_free(resp->final_url);
            resp->final_url = g_strdup(fetch_url);
            if (tls_failure)
                resp->security = NS_SEC_INVALID;
        }
    }
    if (!resp || resp->error || !resp->body) {
        if (resp) ns_response_free(resp);
        g_clear_error(&err);
        g_free(file_url);
        g_free(stripped_url);
        g_free(https_url);
        return NULL;
    }
    g_clear_error(&err);

    char *base = g_strdup(resp->final_url ? resp->final_url : fetch_url);
    if (!body) {
        char *base_stripped = ns_url_strip_tracking_params(base);
        if (base_stripped) {
            g_free(base);
            base = base_stripped;
        }
    }
    gboolean bfcache_ok = !body &&
        (g_str_has_prefix(base, "http://") ||
         g_str_has_prefix(base, "https://")) &&
        resp->status >= 200 && resp->status < 400 &&
        !headers_have_no_store(resp->raw_headers);
    char *refresh_hdr = g_strdup(resp->refresh);
    char *doc_language = g_strdup(resp->content_language);
    char *csp_header = g_strdup(resp->csp_header);
    g_free(file_url);
    g_free(stripped_url);
    g_free(https_url);
    browser_prepare_document_response(resp);

    char *doc_charset = NULL;
    char *decoded = ns_html_decode_body_full((const char *)resp->body->data,
                                             resp->body->len,
                                             resp->content_type,
                                             &doc_charset);
    const ns_config *parse_cfg = ns_config_get();
    gboolean scripting_on = !parse_cfg || parse_cfg->javascript_enabled;
    ns_node *doc = scripting_on
        ? ns_html_parse(decoded ? decoded : "",
                        decoded ? (gssize)strlen(decoded) : 0)
        : ns_html_parse_with_scripting(decoded ? decoded : "",
                                       decoded ? (gssize)strlen(decoded) : 0,
                                       FALSE);
    g_free(decoded);
    int sec = resp->security;
    if (sec == NS_SEC_NONE) {
        const char *u = resp->final_url ? resp->final_url : fetch_url;
        if (g_str_has_prefix(u, "https://"))
            sec = NS_SEC_SECURE;
        else if (g_str_has_prefix(u, "http://"))
            sec = NS_SEC_PLAIN;
    }
    char *ip = g_strdup(resp->remote_ip);
    ns_js_navigation_timing navigation_timing = {
        .origin_us = resp->request_start_us,
        .origin_real_ms = resp->request_start_real_ms,
        .domain_lookup_start_ms = 0,
        .domain_lookup_end_ms = resp->domain_lookup_ms,
        .connect_start_ms = resp->domain_lookup_ms,
        .connect_end_ms = resp->connect_ms,
        .secure_connection_start_ms = resp->connect_ms < resp->tls_ms
            ? resp->connect_ms : 0,
        .request_start_ms = resp->pretransfer_ms,
        .response_start_ms = resp->response_start_ms,
        .response_end_ms = resp->response_end_ms,
    };
    ns_response_free(resp);

    ns_browser *b = ns_browser_core_build(doc, base, viewport_width,
                                           viewport_height, settle_ms,
                                           bfcache_ok, refresh_hdr, doc_language,
                                           csp_header, doc_charset, url,
                                           &navigation_timing);
    if (b) {
        b->security = sec;
        b->remote_ip = ip;
    } else {
        g_free(ip);
    }
    return b;
}

ns_browser *
ns_browser_open(const char *url, int viewport_width, int settle_ms)
{
    return browser_open_common(url, viewport_width, 0.0, settle_ms,
                               NULL, 0, NULL);
}

ns_browser *
ns_browser_open_viewport(const char *url, int viewport_width,
                         double viewport_height, int settle_ms)
{
    return browser_open_common(url, viewport_width, viewport_height, settle_ms,
                               NULL, 0, NULL);
}

ns_browser *
ns_browser_open_post(const char *url, int viewport_width, int settle_ms,
                     const void *body, size_t body_len,
                     const char *content_type)
{
    return browser_open_common(url, viewport_width, 0.0, settle_ms,
                               body, body_len, content_type);
}

ns_browser *
ns_browser_open_post_viewport(const char *url, int viewport_width,
                              double viewport_height, int settle_ms,
                              const void *body, size_t body_len,
                              const char *content_type)
{
    return browser_open_common(url, viewport_width, viewport_height, settle_ms,
                               body, body_len, content_type);
}

int
ns_browser_render_rgba(ns_browser *browser, int scroll_x, int scroll_y,
                       int width, int height, double scale,
                       unsigned char *out, int stride)
{
    if (!browser || !browser->layout || !out) return -1;
    if (width <= 0 || height <= 0 || stride < width * 4) return -1;
    if (!(scale > 0)) scale = 1.0;

    browser->cur_scroll_x = (double)scroll_x;
    browser->cur_scroll_y = (double)scroll_y;
    browser->cur_scale = scale;
    browser->cur_viewport_h = (double)height / scale;
    ns_browser_core_ensure_images(browser);
    if (browser->videos && browser->layout) {
        gint64 now = g_get_monotonic_time();
        ns_video_cache_discover(browser->videos, browser->layout,
                                browser->doc, now);
        ns_video_cache_note_layout(browser->videos, browser->layout,
                                   browser->cur_scroll_x,
                                   browser->cur_scroll_y,
                                   browser->cur_scale);
    }

    cairo_surface_t *surf =
        cairo_image_surface_create(CAIRO_FORMAT_ARGB32, width, height);
    if (cairo_surface_status(surf) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(surf);
        return -1;
    }
    cairo_t *cr = cairo_create(surf);
    cairo_set_tolerance(cr, scale > 0 ? 0.5 / scale : 0.5);
    cairo_set_antialias(cr, CAIRO_ANTIALIAS_FAST);
    cairo_rectangle(cr, 0, 0, width, height);
    cairo_clip(cr);
    cairo_scale(cr, scale, scale);
    cairo_translate(cr, -(double)scroll_x, -(double)scroll_y);

    ns_paint_set_js(browser->js);
    ns_paint_set_anim(browser->anim);
    ns_paint_set_search(browser->search_case, browser->search_active);
    const char *highlight = browser->search_query;
    if (ns_selection_has_range(&browser->selection))
        ns_paint_with_selection(cr, browser->layout, highlight,
                                &browser->selection);
    else
        ns_paint(cr, browser->layout, highlight);
    ns_video_cache_flush_composites(browser->videos, g_get_monotonic_time());
    ns_paint_set_search(FALSE, NULL);
    ns_paint_set_anim(NULL);
    ns_paint_set_js(NULL);

    cairo_destroy(cr);
    cairo_surface_flush(surf);

    const unsigned char *src = cairo_image_surface_get_data(surf);
    int src_stride = cairo_image_surface_get_stride(surf);
    for (int y = 0; y < height; y++) {
        const unsigned char *srow = src + (size_t)y * src_stride;
        unsigned char *drow = out + (size_t)y * stride;
        for (int x = 0; x < width; x++) {
            uint32_t px;
            memcpy(&px, srow + x * 4, sizeof px);
            drow[x * 4 + 0] = (unsigned char)((px >> 16) & 0xFF);
            drow[x * 4 + 1] = (unsigned char)((px >> 8) & 0xFF);
            drow[x * 4 + 2] = (unsigned char)(px & 0xFF);
            drow[x * 4 + 3] = (unsigned char)((px >> 24) & 0xFF);
        }
    }
    cairo_surface_destroy(surf);
    return 0;
}

static const ns_style *
viewport_snap_style(ns_browser *browser)
{
    static const char *const sources[] = { "html", "body" };
    if (!browser->doc || !browser->styles) return NULL;
    for (gsize i = 0; i < G_N_ELEMENTS(sources); i++) {
        const ns_node *n = ns_node_find_first_element(browser->doc, sources[i]);
        const ns_style *s = n ? g_hash_table_lookup(browser->styles, n) : NULL;
        const char *type = s ? ns_style_keyword(s, NS_CSS_SCROLL_SNAP_TYPE)
                             : NULL;
        if (type && strcmp(type, "none") != 0) return s;
    }
    return NULL;
}

int
ns_browser_snap_document(ns_browser *browser, double viewport_w,
                         double viewport_h, int prev_x, int prev_y,
                         int *scroll_x, int *scroll_y)
{
    if (!browser || !browser->layout || !scroll_x || !scroll_y) return 0;
    const ns_style *s = viewport_snap_style(browser);
    if (!s) return 0;

    int page_w = 0, page_h = 0;
    if (ns_browser_page_size(browser, &page_w, &page_h) != 0) return 0;
    double max_x = page_w - viewport_w;
    double max_y = page_h - viewport_h;
    if (max_x < 0) max_x = 0;
    if (max_y < 0) max_y = 0;

    double x = *scroll_x, y = *scroll_y;
    if (!ns_box_scroll_snap_viewport(browser->layout, s, viewport_w, viewport_h,
                                     max_x, max_y, prev_x, prev_y, &x, &y))
        return 0;

    int nx = (int)(x + 0.5), ny = (int)(y + 0.5);
    if (nx == *scroll_x && ny == *scroll_y) return 0;
    *scroll_x = nx;
    *scroll_y = ny;
    return 1;
}

static void
browser_note_viewport(ns_browser *browser, int scroll_x, int scroll_y,
                      int height, double scale)
{
    browser->cur_scroll_x = (double)scroll_x;
    browser->cur_scroll_y = (double)scroll_y;
    browser->cur_scale = scale;
    browser->cur_viewport_h = (double)height / scale;
    ns_box_set_hit_viewport(browser->cur_scroll_x, browser->cur_scroll_y);
    ns_browser_core_ensure_images(browser);
    if (browser->videos && browser->layout) {
        gint64 now = g_get_monotonic_time();
        ns_video_cache_discover(browser->videos, browser->layout,
                                browser->doc, now);
        ns_video_cache_note_layout(browser->videos, browser->layout,
                                   browser->cur_scroll_x,
                                   browser->cur_scroll_y,
                                   browser->cur_scale);
    }
}

static cairo_t *
browser_paint_context(unsigned char *out, int width, int height, int stride,
                      double scale)
{
    cairo_surface_t *surf =
        cairo_image_surface_create_for_data(out, CAIRO_FORMAT_ARGB32,
                                            width, height, stride);
    if (cairo_surface_status(surf) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(surf);
        return NULL;
    }
    cairo_t *cr = cairo_create(surf);
    cairo_surface_destroy(surf);
    cairo_set_tolerance(cr, scale > 0 ? 0.5 / scale : 0.5);
    cairo_set_antialias(cr, CAIRO_ANTIALIAS_FAST);
    cairo_rectangle(cr, 0, 0, width, height);
    cairo_clip(cr);
    return cr;
}

static void
browser_paint_context_done(cairo_t *cr)
{
    cairo_surface_t *surf = cairo_surface_reference(cairo_get_target(cr));
    cairo_destroy(cr);
    cairo_surface_flush(surf);
    cairo_surface_destroy(surf);
}

static void
browser_paint_begin(ns_browser *browser)
{
    ns_paint_set_js(browser->js);
    ns_paint_set_anim(browser->anim);
    ns_paint_set_search(browser->search_case, browser->search_active);
    ns_paint_set_caret_visible(browser->caret_paint_visible);
}

static void
browser_paint_end(void)
{
    ns_paint_set_caret_visible(TRUE);
    ns_paint_set_search(FALSE, NULL);
    ns_paint_set_anim(NULL);
    ns_paint_set_js(NULL);
}

static const ns_selection *
browser_paint_selection(ns_browser *browser)
{
    return ns_selection_has_range(&browser->selection) ? &browser->selection
                                                       : NULL;
}

static void
browser_dump_frame(cairo_surface_t *surf)
{
    const char *dump_dir = g_getenv("NS_FRAME_DUMP");
    if (!dump_dir) return;
    static int frame_no;
    if (frame_no % 30 == 0) {
        char *path = g_strdup_printf("%s/frame-%05d.png", dump_dir, frame_no);
        cairo_surface_write_to_png(surf, path);
        g_free(path);
    }
    frame_no++;
}

int
ns_browser_render_argb32(ns_browser *browser, int scroll_x, int scroll_y,
                         int width, int height, double scale,
                         unsigned char *out, int stride)
{
    if (!browser || !browser->layout || !out) return -1;
    if (width <= 0 || height <= 0 || stride < width * 4) return -1;
    if (!(scale > 0)) scale = 1.0;

    ns_video_cache_set_page_coords(browser->videos, FALSE);
    browser_note_viewport(browser, scroll_x, scroll_y, height, scale);
    cairo_t *cr = browser_paint_context(out, width, height, stride, scale);
    if (!cr) return -1;
    cairo_scale(cr, scale, scale);
    cairo_translate(cr, -(double)scroll_x, -(double)scroll_y);

    browser_paint_begin(browser);
    gint64 paint_t0 = g_get_monotonic_time();
    const ns_selection *sel = browser_paint_selection(browser);
    if (sel)
        ns_paint_with_selection(cr, browser->layout, browser->search_query,
                                sel);
    else
        ns_paint(cr, browser->layout, browser->search_query);
    ns_video_cache_flush_composites(browser->videos, g_get_monotonic_time());
    if (g_getenv("NS_PROFILE"))
        g_printerr("[profile] paint %6.1fms %dx%d\n",
                   (double)(g_get_monotonic_time() - paint_t0) / 1000.0,
                   width, height);
    browser_paint_end();

    cairo_surface_t *surf = cairo_surface_reference(cairo_get_target(cr));
    browser_paint_context_done(cr);
    browser_dump_frame(surf);
    cairo_surface_destroy(surf);
    return 0;
}

void
ns_browser_note_viewport(ns_browser *browser, int scroll_x, int scroll_y,
                         int height, double scale)
{
    if (!browser || !browser->layout) return;
    ns_video_cache_set_page_coords(browser->videos, TRUE);
    browser_note_viewport(browser, scroll_x, scroll_y, height,
                          scale > 0 ? scale : 1.0);
}

void
ns_browser_flush_video_rects(ns_browser *browser)
{
    if (browser && browser->videos)
        ns_video_cache_flush_composites(browser->videos,
                                        g_get_monotonic_time());
}

int
ns_browser_layers_prepare(ns_browser *browser, int scroll_x, int scroll_y,
                          int width, int height, double scale,
                          ns_paint_layer_plan *plan)
{
    if (!browser || !browser->layout || !plan || width <= 0 || height <= 0)
        return -1;
    if (!(scale > 0)) scale = 1.0;
    ns_video_cache_set_page_coords(browser->videos, TRUE);
    browser_note_viewport(browser, scroll_x, scroll_y, height, scale);
    unsigned char *row = g_malloc((size_t)width * 4u);
    cairo_t *cr = browser_paint_context(row, width, 1, width * 4, scale);
    if (!cr) {
        g_free(row);
        return -1;
    }
    cairo_scale(cr, scale, scale);
    cairo_translate(cr, -(double)scroll_x, -(double)scroll_y);
    browser_paint_begin(browser);
    ns_paint_plan_layers(cr, browser->layout, plan);
    browser_paint_end();
    browser_paint_context_done(cr);
    g_free(row);
    return plan->dynamic ? -1 : 0;
}

typedef struct doc_tile_paint {
    unsigned char *const *bufs;
    cairo_t             **upper;
    gboolean             *used;
    int                   width, height, stride;
    int                   scroll_x, tile_y;
    double                scale;
} doc_tile_paint;

static void
doc_tile_transform(cairo_t *cr, const doc_tile_paint *p)
{
    cairo_translate(cr, 0, -(double)p->tile_y);
    cairo_scale(cr, p->scale, p->scale);
    cairo_translate(cr, -(double)p->scroll_x, 0);
}

static cairo_t *
doc_tile_upper(int index, gpointer data)
{
    doc_tile_paint *p = data;
    if (p->upper[index]) return p->upper[index];
    memset(p->bufs[index + 1], 0, (size_t)p->stride * (size_t)p->height);
    p->upper[index] = browser_paint_context(p->bufs[index + 1], p->width,
                                            p->height, p->stride, p->scale);
    if (p->upper[index]) doc_tile_transform(p->upper[index], p);
    p->used[index] = p->upper[index] != NULL;
    return p->upper[index];
}

static gboolean
layer_target_ok(const ns_browser *browser, const ns_paint_layer_plan *plan,
                int width, int height, int stride)
{
    return browser && browser->layout && plan && width > 0 && height > 0 &&
           stride >= width * 4;
}

int
ns_browser_render_doc_tile(ns_browser *browser,
                           const ns_paint_layer_plan *plan, int scroll_x,
                           int tile_y, int width, int height, double scale,
                           unsigned char *const *bufs, int stride,
                           gboolean *upper_used)
{
    if (!bufs || !layer_target_ok(browser, plan, width, height, stride))
        return -1;
    if (!(scale > 0)) scale = 1.0;
    int n_upper = (int)plan->vp->len;
    doc_tile_paint p = { bufs, g_new0(cairo_t *, MAX(n_upper, 1)),
                         upper_used, width, height, stride, scroll_x, tile_y,
                         scale };
    for (int i = 0; i < n_upper; i++) upper_used[i] = FALSE;
    cairo_t *cr = browser_paint_context(bufs[0], width, height, stride,
                                        scale);
    if (!cr) {
        g_free(p.upper);
        return -1;
    }
    doc_tile_transform(cr, &p);
    browser_paint_begin(browser);
    gboolean ok = ns_paint_doc_layers(cr, doc_tile_upper, &p, browser->layout,
                                      browser->search_query,
                                      browser_paint_selection(browser), plan);
    browser_paint_end();
    for (int i = 0; i < n_upper; i++)
        if (p.upper[i]) browser_paint_context_done(p.upper[i]);
    g_free(p.upper);
    browser_paint_context_done(cr);
    return ok ? 0 : -2;
}

static const ns_paint_vp_capture *
plan_capture(const ns_paint_layer_plan *plan, int index)
{
    if (!plan || index < 0 || index >= (int)plan->vp->len) return NULL;
    return &g_array_index(plan->vp, ns_paint_vp_capture, index);
}

static double
browser_viewport_css_h(const ns_browser *browser)
{
    return browser->cur_viewport_h > 0 ? browser->cur_viewport_h
                                       : browser->vh;
}

static int
fixed_layer_info(const ns_box *box, double vh, ns_vp_layer_info *out)
{
    gboolean exact = ns_box_subtree_extent_y(box, &out->top, &out->bottom);
    out->top = exact ? MAX(out->top - NS_VP_LAYER_PAD, 0) : 0;
    out->bottom = exact ? MIN(out->bottom + NS_VP_LAYER_PAD, vh) : vh;
    if (out->bottom <= out->top) out->top = out->bottom = 0;
    return 0;
}

static int
sticky_layer_info(const ns_browser *browser, const ns_box *box, double vh,
                  ns_vp_layer_info *out)
{
    gboolean exact = ns_box_subtree_extent_y(box, &out->top, &out->bottom);
    if (!ns_box_sticky_y_model(box, vh, &out->sticky)) return -1;
    double dx = 0, dy = 0;
    ns_box_sticky_offset(box, browser->cur_scroll_x, browser->cur_scroll_y,
                         browser->cur_scroll_x + browser->vw,
                         browser->cur_scroll_y + vh, &dx, &dy);
    out->x_offset = dx;
    double pad = exact ? NS_VP_LAYER_PAD : vh / 2;
    out->top -= pad;
    out->bottom += pad;
    return out->bottom - out->top <= vh * 4 ? 0 : -1;
}

int
ns_browser_vp_layer_info(ns_browser *browser, const ns_paint_layer_plan *plan,
                         int index, ns_vp_layer_info *out)
{
    memset(out, 0, sizeof *out);
    const ns_paint_vp_capture *cap = plan_capture(plan, index);
    if (!browser || !browser->layout || !cap) return -1;
    out->kind = cap->kind;
    double vh = browser_viewport_css_h(browser);
    return cap->kind == NS_PAINT_VP_STICKY
        ? sticky_layer_info(browser, cap->box, vh, out)
        : fixed_layer_info(cap->box, vh, out);
}

int
ns_browser_render_vp_layer(ns_browser *browser,
                           const ns_paint_layer_plan *plan, int index,
                           int scroll_x, int scroll_y, int origin_y,
                           int width, int height, double scale,
                           unsigned char *out, int stride)
{
    const ns_paint_vp_capture *cap = plan_capture(plan, index);
    if (!out || !cap || !layer_target_ok(browser, plan, width, height, stride))
        return -1;
    if (!(scale > 0)) scale = 1.0;
    memset(out, 0, (size_t)stride * (size_t)height);
    cairo_t *cr = browser_paint_context(out, width, height, stride, scale);
    if (!cr) return -1;
    cairo_translate(cr, 0, -(double)origin_y);
    cairo_scale(cr, scale, scale);
    if (cap->kind == NS_PAINT_VP_STICKY)
        cairo_translate(cr, -(double)scroll_x, 0);
    else
        cairo_translate(cr, -(double)scroll_x, -(double)scroll_y);
    browser_paint_begin(browser);
    ns_paint_vp_layer(cr, browser->layout, cap, scroll_x, scroll_y,
                      browser->search_query, browser_paint_selection(browser));
    browser_paint_end();
    browser_paint_context_done(cr);
    return 0;
}

static gboolean
box_under_fixed(const ns_box *b)
{
    for (const ns_box *a = b; a; a = a->parent)
        if (ns_box_is_fixed(a)) return TRUE;
    return FALSE;
}

typedef struct scroller_walk {
    double vx, vy;
    GString *out;
    int left;
} scroller_walk;

static void
scroller_rect_emit(const ns_box *b, double ox, double oy, scroller_walk *w)
{
    gboolean fixed = box_under_fixed(b);
    double x = b->x + b->margin.left + b->border.left + ox;
    double y = b->y + b->margin.top + b->border.top + oy;
    double bw = b->content_width + b->padding.left + b->padding.right;
    double bh = b->content_height + b->padding.top + b->padding.bottom;
    if (fixed) {
        x -= w->vx;
        y -= w->vy;
    }
    if (!(bw > 0 && bh > 0 && isfinite(x) && isfinite(y))) return;
    int axes = (b->scroll_max_x > 0 ? 1 : 0) | (b->scroll_max_y > 0 ? 2 : 0);
    g_string_append_printf(w->out, "sr %d %d %d %d %d %d\n", (int)floor(x),
                           (int)floor(y), (int)ceil(bw), (int)ceil(bh), axes,
                           fixed ? 1 : 0);
    w->left--;
}

static void
scroller_rects_walk(const ns_box *b, double ox, double oy, scroller_walk *w)
{
    if (!b || w->left <= 0) return;
    double hx = 0, hy = 0;
    ns_box_hit_offset(b, &hx, &hy);
    ox += hx;
    oy += hy;
    if (b->scrolls && (b->scroll_max_x > 0 || b->scroll_max_y > 0))
        scroller_rect_emit(b, ox, oy, w);
    double cx = ox - b->scroll_x, cy = oy - b->scroll_y;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        scroller_rects_walk(c, cx, cy, w);
}

void
ns_browser_scroller_rects(ns_browser *browser, GString *out, int max_rects)
{
    if (!browser || !browser->layout || !out) return;
    scroller_walk w = { browser->cur_scroll_x, browser->cur_scroll_y, out,
                        max_rects };
    scroller_rects_walk(browser->layout, 0, 0, &w);
    if (w.left <= 0)
        g_string_append(out, "sr-all\n");
}

gboolean
ns_browser_canvas_color(ns_browser *browser, double rgba_out[4])
{
    return browser && browser->layout &&
           ns_paint_canvas_color(browser->layout, rgba_out);
}

static const ns_node *browser_hit_node(ns_browser *browser, int x, int y);

static gboolean
browser_node_is_hyperlink(const ns_node *n)
{
    return ns_node_is_element_named(n, "a") ||
           ns_node_is_element_named(n, "area");
}

static char *
browser_link_near(ns_browser *browser, int x, int y, int probes)
{
    if (!browser || !browser->layout) return NULL;

    static const int kR = 6;
    static const int probe[][2] = {
        { 0, 0 },
        { 0, -kR }, { 0, kR }, { -kR, 0 }, { kR, 0 },
        { -kR, -kR }, { kR, -kR }, { -kR, kR }, { kR, kR },
    };
    for (int i = 0; i < probes; i++) {
        int px = x + probe[i][0], py = y + probe[i][1];
        const char *href = ns_box_hit_link(browser->layout,
                                           (double)px, (double)py);
        if (!href || !*href) {
            const ns_node *node = browser_hit_node(browser, px, py);
            for (const ns_node *a = node; a && (!href || !*href); a = a->parent)
                if (browser_node_is_hyperlink(a))
                    href = ns_element_get_attr(a, "href");
        }
        if (href && *href) return ns_browser_core_resolve_navigation(browser, href);
    }
    return NULL;
}

char *
ns_browser_link_at(ns_browser *browser, int x, int y)
{
    return browser_link_near(browser, x, y, 9);
}

char *
ns_browser_link_under(ns_browser *browser, int x, int y)
{
    return browser_link_near(browser, x, y, 1);
}

char *
ns_browser_cursor_at(ns_browser *browser, int x, int y)
{
    static const char *const known[] = {
        "default", "none", "context-menu", "help", "pointer", "progress",
        "wait", "cell", "crosshair", "text", "vertical-text", "alias",
        "copy", "move", "no-drop", "not-allowed", "grab", "grabbing",
        "all-scroll", "col-resize", "row-resize", "n-resize", "e-resize",
        "s-resize", "w-resize", "ne-resize", "nw-resize", "se-resize",
        "sw-resize", "ew-resize", "ns-resize", "nesw-resize", "nwse-resize",
        "zoom-in", "zoom-out",
    };
    if (!browser || !browser->layout || !browser->styles) return NULL;

    const ns_node *node = browser_hit_node(browser, x, y);
    const ns_node *form_node =
        ns_box_hit_form_dom(browser->layout, (double)x, (double)y);

    const ns_style *style = NULL;
    for (const ns_node *n = node; n && !style; n = n->parent)
        style = g_hash_table_lookup(browser->styles, n);
    if (!style) return NULL;

    const ns_css_value *v = style->values[NS_CSS_CURSOR];
    char *match = NULL;
    if (v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword) {
        char **tokens = g_strsplit_set(v->u.keyword, ", \t", -1);
        for (int i = 0; tokens && tokens[i]; i++) {
            if (!tokens[i][0]) continue;
            for (gsize k = 0; k < G_N_ELEMENTS(known); k++)
                if (g_ascii_strcasecmp(tokens[i], known[k]) == 0) {
                    g_free(match);
                    match = g_strdup(known[k]);
                }
        }
        g_strfreev(tokens);
    }
    if (match) return match;

    if (ns_box_hit_link(browser->layout, (double)x, (double)y)) return NULL;
    for (const ns_node *n = node; n; n = n->parent)
        if (browser_node_is_hyperlink(n) &&
            ns_element_get_attr(n, "href")) return NULL;
    if (form_node)
        return ns_node_is_text_input(form_node) ? g_strdup("text") : NULL;
    for (const ns_node *n = node; n; n = n->parent)
        if (ns_node_is_contenteditable_host(n)) return g_strdup("text");
    if (ns_selection_text_at(browser->layout, (double)x, (double)y))
        return g_strdup("text");
    return NULL;
}

static ns_node *
browser_focused_field(ns_browser *b)
{
    const ns_node *f = b->js ? ns_js_focused_node(b->js) : NULL;
    return ns_node_is_editable(f) ? (ns_node *)f : NULL;
}

static gboolean
browser_field_writable(const ns_node *field)
{
    if (!ns_node_is_editable(field)) return FALSE;
    if (ns_node_is_contenteditable_host(field)) return TRUE;
    return !ns_element_get_attr(field, "readonly") &&
           !ns_element_get_attr(field, "disabled");
}

static gboolean
browser_field_copyable(const ns_node *field)
{
    if (!ns_node_is_element_named(field, "input")) return TRUE;
    const char *type = ns_element_get_attr(field, "type");
    return !type || g_ascii_strcasecmp(type, "password") != 0;
}

static gboolean
browser_field_selection(ns_browser *b, const ns_node *field, gsize *lo,
                        gsize *hi)
{
    const char *cur = ns_node_editable_value(field);
    gsize caret = ns_browser_core_utf8_boundary(cur, b->caret_byte);
    gsize anchor = ns_browser_core_utf8_boundary(cur, b->sel_anchor_byte);
    *lo = MIN(caret, anchor);
    *hi = MAX(caret, anchor);
    return *lo < *hi;
}

static char *
browser_field_selected_text(ns_browser *b, const ns_node *field)
{
    gsize lo, hi;
    if (!browser_field_selection(b, field, &lo, &hi) ||
        !browser_field_copyable(field))
        return NULL;
    return g_strndup(ns_node_editable_value(field) + lo, hi - lo);
}

static int
browser_field_edit_state(ns_browser *b, const ns_node *field)
{
    gsize lo, hi;
    int state = NS_BROWSER_EDIT_FIELD;
    if (browser_field_writable(field))
        state |= NS_BROWSER_EDIT_WRITABLE;
    if (browser_field_selection(b, field, &lo, &hi) &&
        browser_field_copyable(field))
        state |= NS_BROWSER_EDIT_SELECTION;
    return state;
}

static void browser_input_replace(ns_browser *b, ns_node *node, gsize del_start,
                                  gsize del_end, const char *insert,
                                  const char *input_type);

static void
browser_field_select_all(ns_browser *b, const ns_node *field)
{
    b->sel_anchor_byte = 0;
    b->caret_byte = strlen(ns_node_editable_value(field));
    ns_browser_core_relayout(b);
    b->dirty = FALSE;
}

static char *
browser_field_cut(ns_browser *b, ns_node *field)
{
    gsize lo, hi;
    char *text = browser_field_writable(field)
               ? browser_field_selected_text(b, field) : NULL;
    if (!text) return NULL;
    browser_field_selection(b, field, &lo, &hi);
    browser_input_replace(b, field, lo, hi, NULL, "deleteByCut");
    ns_browser_core_relayout(b);
    b->dirty = FALSE;
    return text;
}

static char *
browser_copy_text(ns_browser *b)
{
    ns_node *field = browser_focused_field(b);
    gsize lo, hi;
    if (field && browser_field_selection(b, field, &lo, &hi))
        return browser_field_selected_text(b, field);
    return ns_selection_collect_text(b->layout, &b->selection);
}

char *
ns_browser_select(ns_browser *browser, int kind, int x, int y)
{
    if (!browser || !browser->layout) return NULL;
    ns_node *field = browser_focused_field(browser);
    switch (kind) {
    case 0: ns_selection_anchor_at(&browser->selection, browser->layout,
                                   (double)x, (double)y); break;
    case 1: ns_selection_extend_to(&browser->selection, browser->layout,
                                   (double)x, (double)y);
            browser->selection_dragged = TRUE; break;
    case 2: ns_selection_clear(&browser->selection); break;
    case 3: if (field) {
                browser_field_select_all(browser, field);
                return NULL;
            }
            ns_selection_select_all(&browser->selection, browser->layout);
            break;
    case 4: return browser_copy_text(browser);
    case 7: return field ? browser_field_cut(browser, field) : NULL;
    case 5: ns_selection_select_word_at(&browser->selection, browser->layout,
                                        (double)x, (double)y);
            browser->selection_dragged = TRUE; break;
    case 6: ns_selection_select_block_at(&browser->selection, browser->layout,
                                         (double)x, (double)y);
            browser->selection_dragged = TRUE; break;
    default: break;
    }
    ns_browser_core_sync_js_selection(browser);
    return NULL;
}

static void
browser_hover_dispatch(ns_browser *b, const ns_node *target, int x, int y,
                       const char *ptr_type, const char *mouse_type,
                       const ns_node *related)
{
    if (!b->js || !target) return;
    ns_js_dispatch_mouse_event(b->js, target, ptr_type,
                               (double)x - b->cur_scroll_x,
                               (double)y - b->cur_scroll_y,
                               (double)x, (double)y, 0, 0,
                               FALSE, FALSE, FALSE, FALSE, related, NULL);
    ns_js_dispatch_mouse_event(b->js, target, mouse_type,
                               (double)x - b->cur_scroll_x,
                               (double)y - b->cur_scroll_y,
                               (double)x, (double)y, 0, 0,
                               FALSE, FALSE, FALSE, FALSE, related, NULL);
}

int
ns_browser_hover(ns_browser *browser, int x, int y)
{
    if (!browser || !browser->layout) return -1;

    const ns_node *node = browser_hit_node(browser, x, y);

    ns_browser_core_prune(browser);
    const ns_node *prev = browser->hover_node;
    gboolean changed = node != prev;
    browser->hover_node = node;

    gboolean dirty = FALSE;
    if (browser->js) {
        if (changed) {
            browser_hover_dispatch(browser, prev, x, y, "pointerout",
                                   "mouseout", node);
            browser_hover_dispatch(browser, prev, x, y, "pointerleave",
                                   "mouseleave", node);
            browser_hover_dispatch(browser, node, x, y, "pointerover",
                                   "mouseover", prev);
            browser_hover_dispatch(browser, node, x, y, "pointerenter",
                                   "mouseenter", prev);
        }
        browser_hover_dispatch(browser, node, x, y, "pointermove",
                               "mousemove", NULL);
        if (ns_js_consume_mutated(browser->js)) dirty = TRUE;
    }

    gboolean hover_restyle = changed && ns_render_page_uses_hover() &&
                             !ns_selection_has_range(&browser->selection);
    if (dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
        return 1;
    }
    if (hover_restyle) {
        gint64 now = g_get_monotonic_time();
        gint64 min_gap = browser->relayout_cost_us * 2;
        if (min_gap < 60000) min_gap = 60000;
        if (now - browser->hover_relayout_us >= min_gap) {
            browser->hover_relayout_us = now;
            browser->hover_restyle_pending = FALSE;
            ns_browser_core_relayout(browser);
            browser->dirty = FALSE;
            return 1;
        }
        browser->hover_restyle_pending = TRUE;
    }
    return 0;
}

int
ns_browser_scroll_at(ns_browser *browser, int x, int y, int dx, int dy)
{
    return ns_browser_scroll_at_full(browser, x, y, dx, dy, NULL);
}

int
ns_browser_scroll_at_full(ns_browser *browser, int x, int y, int dx, int dy,
                          int *out_snapped)
{
    if (out_snapped) *out_snapped = 0;
    if (!browser || !browser->layout) return 0;

    ns_box *box = ns_box_hit_scrollable(browser->layout, (double)x, (double)y);
    if (!box) return 0;

    double prev_x = box->scroll_x, prev_y = box->scroll_y;
    int consumed = 0;
    if (dy != 0 && box->scroll_max_y > 0) {
        double ny = box->scroll_y + dy;
        if (ny < 0) ny = 0;
        if (ny > box->scroll_max_y) ny = box->scroll_max_y;
        if (ny != box->scroll_y) { box->scroll_y = ny; consumed = 1; }
    }
    if (dx != 0 && box->scroll_max_x > 0) {
        double nx = box->scroll_x + dx;
        if (nx < 0) nx = 0;
        if (nx > box->scroll_max_x) nx = box->scroll_max_x;
        if (nx != box->scroll_x) { box->scroll_x = nx; consumed = 1; }
    }

    if (consumed) {
        double moved_x = box->scroll_x, moved_y = box->scroll_y;
        ns_box_scroll_snap_from(box, prev_x, prev_y);
        if (out_snapped)
            *out_snapped = box->scroll_x != moved_x ||
                           box->scroll_y != moved_y;
        if (browser->js && box->dom)
            ns_js_dispatch_event(browser->js, box->dom, "scroll", NULL);
    }
    return consumed;
}

static gboolean
sb_vgeom(const ns_box *b, double *track_x, double *track_w,
         double *track_y, double *track_h, double *thumb_y, double *thumb_h)
{
    if (!b || b->scroll_max_y <= 0) return FALSE;
    double py = b->y + b->margin.top + b->border.top;
    double ph = b->content_height + b->padding.top + b->padding.bottom;
    if (ph <= 16.0) return FALSE;
    double px = b->x + b->margin.left + b->border.left;
    double pw = b->content_width + b->padding.left + b->padding.right;
    double tw = 8.0;
    double th = ph - 2.0;
    double total = ph + b->scroll_max_y;
    double thh = th * (ph / total);
    if (thh < 16.0) thh = 16.0;
    if (thh > th) thh = th;
    *track_x = px + pw - tw - 1.0;
    *track_w = tw;
    *track_y = py + 1.0;
    *track_h = th;
    *thumb_h = thh;
    *thumb_y = *track_y + (th - thh) * (b->scroll_y / b->scroll_max_y);
    return TRUE;
}

static ns_box *
box_find_scrollable_by_dom(ns_box *root, const ns_node *node)
{
    if (!root || !node) return NULL;
    if (root->dom == node && root->scrolls) return root;
    for (ns_box *c = root->first_child; c; c = c->next_sibling) {
        ns_box *m = box_find_scrollable_by_dom(c, node);
        if (m) return m;
    }
    return NULL;
}

int
ns_browser_scrollbar_press(ns_browser *browser, int x, int y)
{
    if (!browser || !browser->layout) return 0;
    double lx = 0, ly = 0;
    ns_box *box = ns_box_hit_scrollbar(browser->layout, (double)x, (double)y,
                                       &lx, &ly);
    if (!box) return 0;

    double tx, tw, ty, th, thy, thh;
    if (!sb_vgeom(box, &tx, &tw, &ty, &th, &thy, &thh)) return 0;
    if (lx < tx - 3.0 || lx > tx + tw + 3.0 || ly < ty || ly > ty + th)
        return 0;

    const ns_node *box_dom = box->dom;
    double grab;
    if (ly >= thy && ly <= thy + thh) {
        grab = ly - thy;
    } else {
        grab = thh / 2.0;
        double ns = (th > thh)
            ? (ly - ty - grab) / (th - thh) * box->scroll_max_y : 0.0;
        if (ns < 0) ns = 0;
        if (ns > box->scroll_max_y) ns = box->scroll_max_y;
        box->scroll_y = ns;
        if (box_dom && browser->js) {
            ns_js_dispatch_event(browser->js, box_dom, "scroll", NULL);
            box = box_find_scrollable_by_dom(browser->layout, box_dom);
        }
    }

    browser->sb_dragging = TRUE;
    browser->sb_box = box;
    browser->sb_node = box_dom;
    browser->sb_grab = grab;
    return 1;
}

int
ns_browser_scrollbar_drag(ns_browser *browser, int x, int y)
{
    (void)x;
    if (!browser || !browser->sb_dragging) return 0;
    ns_box *box = browser->sb_box;
    if (browser->sb_node)
        box = box_find_scrollable_by_dom(browser->layout, browser->sb_node);
    if (!box) {
        browser->sb_dragging = FALSE;
        browser->sb_box = NULL;
        return 0;
    }
    browser->sb_box = box;

    double tx, tw, ty, th, thy, thh;
    if (!sb_vgeom(box, &tx, &tw, &ty, &th, &thy, &thh) || th <= thh)
        return 0;
    double ns = ((double)y - ty - browser->sb_grab) / (th - thh)
              * box->scroll_max_y;
    if (ns < 0) ns = 0;
    if (ns > box->scroll_max_y) ns = box->scroll_max_y;
    if (ns == box->scroll_y) return 0;
    box->scroll_y = ns;
    if (box->dom && browser->js)
        ns_js_dispatch_event(browser->js, box->dom, "scroll", NULL);
    return 1;
}

void
ns_browser_scrollbar_release(ns_browser *browser)
{
    if (!browser) return;
    browser->sb_dragging = FALSE;
    browser->sb_box = NULL;
    browser->sb_node = NULL;
}

int
ns_browser_drop_files(ns_browser *browser, int x, int y,
                      const char *const *paths, int n_paths)
{
    if (!browser || !browser->js || !browser->layout || !paths || n_paths <= 0)
        return 0;

    const ns_box *hit = ns_box_hit_test(browser->layout, (double)x, (double)y);
    const ns_node *target = hit ? hit->dom : NULL;
    if (!target && browser->doc)
        target = ns_node_find_first_element(browser->doc, "body");
    if (!target) target = browser->doc;
    if (!target) return 0;

    ns_js_drag_session *session = ns_js_drag_session_new(browser->js);
    if (!session) return 0;
    for (int i = 0; i < n_paths; i++)
        if (paths[i]) ns_js_drag_session_add_file(session, paths[i]);

    gboolean accept = FALSE;
    gboolean prevented = FALSE;
    ns_js_dispatch_drag_event(browser->js, session, target, "dragenter",
                              x, y, x, y, 0, 0, FALSE, FALSE, FALSE, FALSE,
                              NULL, &prevented);
    if (prevented) accept = TRUE;
    prevented = FALSE;
    ns_js_dispatch_drag_event(browser->js, session, target, "dragover",
                              x, y, x, y, 0, 0, FALSE, FALSE, FALSE, FALSE,
                              NULL, &prevented);
    if (prevented) accept = TRUE;
    if (accept)
        ns_js_dispatch_drag_event(browser->js, session, target, "drop",
                                  x, y, x, y, 0, 0, FALSE, FALSE, FALSE, FALSE,
                                  NULL, &prevented);
    else
        ns_js_dispatch_drag_event(browser->js, session, target, "dragleave",
                                  x, y, x, y, 0, 0, FALSE, FALSE, FALSE, FALSE,
                                  NULL, &prevented);
    ns_js_drag_session_free(session);

    if (ns_js_consume_mutated(browser->js)) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
        return 1;
    }
    return 0;
}

char *
ns_browser_eval(ns_browser *browser, const char *src)
{
    if (!browser || !browser->js || !src) return NULL;
    ns_browser_core_damp_reset(browser);
    char *res = ns_js_eval_source(browser->js, src, "devtools-console");
    if (ns_js_run_animation_frame(browser->js)) browser->dirty = TRUE;
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }
    return res;
}

static int
browser_context_field(ns_browser *browser, int x, int y)
{
    if (!browser->layout) return 0;
    const ns_node *form = ns_box_hit_form_dom(browser->layout, (double)x,
                                              (double)y);
    const ns_node *field = ns_node_is_editable(form) ? form : NULL;
    for (const ns_node *n = browser_hit_node(browser, x, y); n && !field;
         n = n->parent)
        if (ns_node_is_contenteditable_host(n)) field = n;
    if (!field) return 0;
    if (ns_js_focused_node(browser->js) != field) {
        ns_js_set_focus(browser->js, field);
        browser->dirty = TRUE;
        if (ns_js_focused_node(browser->js) != field) return 0;
        browser->caret_byte = strlen(ns_node_editable_value(field));
        browser->sel_anchor_byte = browser->caret_byte;
    }
    return browser_field_edit_state(browser, field);
}

int
ns_browser_contextmenu_full(ns_browser *browser, int x, int y, int *out_edit)
{
    if (out_edit) *out_edit = 0;
    if (!browser || !browser->layout || !browser->js) return 0;
    const ns_node *node = browser_hit_node(browser, x, y);
    if (!node) return 0;
    gboolean prevented = FALSE;
    ns_js_dispatch_mouse_event(browser->js, node, "contextmenu",
                               (double)x - browser->cur_scroll_x,
                               (double)y - browser->cur_scroll_y,
                               (double)x, (double)y,
                               2, 0, FALSE, FALSE, FALSE, FALSE, NULL,
                               &prevented);
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }
    int edit = prevented ? 0 : browser_context_field(browser, x, y);
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    if (ns_js_run_animation_frame(browser->js)) browser->dirty = TRUE;
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }
    if (out_edit) *out_edit = edit;
    return prevented ? 1 : 0;
}

int
ns_browser_contextmenu(ns_browser *browser, int x, int y)
{
    return ns_browser_contextmenu_full(browser, x, y, NULL);
}

char *
ns_browser_media_at(ns_browser *browser, int x, int y, int *out_is_video,
                    int *out_stream)
{
    if (out_is_video) *out_is_video = 0;
    if (out_stream) *out_stream = 0;
    if (!browser || !browser->layout) return NULL;

    const ns_box *hit = ns_box_hit_test(browser->layout, (double)x, (double)y);
    const ns_box *media = NULL;
    for (const ns_box *b = hit; b; b = b->parent) {
        gboolean has_media_url =
            b->media && (b->media->video_src || b->media->video_audio_src);
        gboolean has_internal_video =
            b->dom && b->dom->kind == NS_NODE_ELEMENT &&
            (ns_element_get_attr(b->dom, NS_MEDIA_SRC_ATTR) != NULL ||
             ns_element_get_attr(b->dom, NS_MEDIA_STREAM_ATTR) != NULL);
        if (b->dom && (ns_node_is_element_named(b->dom, "video") ||
                       ns_node_is_element_named(b->dom, "audio") ||
                       has_media_url || has_internal_video)) {
            media = b;
            break;
        }
    }
    if (!media || !media->dom) return NULL;

    if (media->media && media->media->video) {
        ns_video *iv = media->media->video;
        if (iv->is_camera)
            return NULL;
        if (iv->player) {
            ns_video_cache_toggle(browser->videos, iv, g_get_monotonic_time());
            return NULL;
        }
    }
    if (media->dom->kind == NS_NODE_ELEMENT) {
        const char *stream_kind = ns_element_get_attr(media->dom, NS_MEDIA_STREAM_ATTR);
        if (stream_kind && g_strcmp0(stream_kind, "camera") == 0)
            return NULL;
    }

    gboolean is_video =
        ns_node_is_element_named(media->dom, "video") ||
        (media->media && media->media->video_src) ||
        (media->dom->kind == NS_NODE_ELEMENT &&
         (ns_element_get_attr(media->dom, NS_MEDIA_SRC_ATTR) != NULL ||
          ns_element_get_attr(media->dom, NS_MEDIA_STREAM_ATTR) != NULL));
    gboolean force_stream =
        media->dom->kind == NS_NODE_ELEMENT &&
        ns_element_get_attr(media->dom, NS_MEDIA_STREAM_ATTR) != NULL;
    const char *msrc = NULL;
    if (media->media) {
        if (is_video) {
            msrc = media->media->video_src;
            if (!msrc) msrc = media->media->video_audio_src;
        } else {
            msrc = media->media->video_audio_src;
        }
    }
    if ((!msrc || !*msrc) && media->dom->kind == NS_NODE_ELEMENT)
        msrc = ns_element_get_attr(media->dom, NS_MEDIA_SRC_ATTR);
    char *abs = (!force_stream && msrc) ? ns_url_resolve(browser->base_url, msrc)
                                        : NULL;
    gboolean stream = force_stream || !abs || g_str_has_prefix(abs, "blob:") ||
                      g_str_has_prefix(abs, "data:");
    if (!stream && is_video && abs && ns_video_url_is_inline(abs)) {
        g_free(abs);
        return NULL;
    }
    if (stream) {
        g_free(abs);
        abs = browser->base_url ? g_strdup(browser->base_url) : NULL;
    }
    if (!abs) return NULL;
    if (g_str_has_prefix(abs, "file://") &&
        (!browser->base_url || !g_str_has_prefix(browser->base_url, "file://"))) {
        g_free(abs);
        return NULL;
    }
    if (out_is_video) *out_is_video = is_video ? 1 : 0;
    if (out_stream) *out_stream = stream ? 1 : 0;
    return abs;
}

int
ns_browser_find(ns_browser *browser, const char *query, int case_sensitive,
                int direction, int from_y, int *out_total, int *out_current,
                int *out_y)
{
    if (out_total) *out_total = 0;
    if (out_current) *out_current = 0;
    if (out_y) *out_y = 0;
    if (!browser || !browser->layout) return -1;

    gboolean cs = case_sensitive != 0;
    if (!query || !*query) {
        g_clear_pointer(&browser->search_query, g_free);
        browser->search_active = NULL;
        browser->search_case = cs;
        return 0;
    }

    if (!browser->search_query || strcmp(browser->search_query, query) != 0 ||
        browser->search_case != cs) {
        g_free(browser->search_query);
        browser->search_query = g_strdup(query);
        browser->search_active = NULL;
    }
    browser->search_case = cs;

    guint total = ns_box_count_matches(browser->layout, query, cs);
    if (out_total) *out_total = (int)total;
    if (total == 0) {
        browser->search_active = NULL;
        return 0;
    }

    double cur_y = browser->search_active ? browser->search_active->y
                                          : (double)from_y;
    const ns_box *target = NULL;
    if (direction == 2) {
        target = ns_box_first_match_above(browser->layout, query, cur_y, cs);
        if (!target)
            target = ns_box_first_match_above(browser->layout, query,
                                              G_MAXDOUBLE, cs);
    } else if (direction == 1) {
        target = ns_box_first_match_below(browser->layout, query, cur_y + 2,
                                          cs);
        if (!target)
            target = ns_box_first_match_below(browser->layout, query, -1, cs);
    } else {
        target = ns_box_first_match_below(browser->layout, query,
                                          (double)from_y - 1, cs);
        if (!target)
            target = ns_box_first_match_below(browser->layout, query, -1, cs);
    }

    browser->search_active = target;
    if (target) {
        if (out_y) *out_y = (int)target->y;
        if (out_current)
            *out_current = (int)ns_box_match_ordinal(browser->layout, query,
                                                     target, cs);
    }
    return 0;
}

static const ns_node *
browser_hit_node(ns_browser *browser, int x, int y)
{
    return ns_box_hit_node(browser->layout, (double)x, (double)y);
}

char *
ns_browser_press(ns_browser *browser, int x, int y, int mods)
{
    if (!browser || !browser->layout) return NULL;
    ns_js_note_pointer_input(browser->js, TRUE);
    ns_browser_core_damp_reset(browser);
    g_clear_pointer(&browser->pending_nav, g_free);
    gboolean extending = (mods & 1) != 0 &&
                         ns_selection_has_range(&browser->selection);
    if (!extending) {
        ns_selection_clear(&browser->selection);
        ns_browser_core_sync_js_selection(browser);
    }
    browser->selection_dragged = FALSE;

    const ns_node *node = browser_hit_node(browser, x, y);
    browser->press_node = node;
    browser->press_x = x;
    browser->press_y = y;
    browser->press_mods = mods;
    browser->press_active = node != NULL;

    ns_css_set_active_node(node);
    if (node && ns_render_page_uses_active()) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }

    if (browser->js && node) {
        gboolean sh = (mods & 1) != 0, ct = (mods & 2) != 0;
        gboolean al = (mods & 4) != 0, me = (mods & 8) != 0;
        ns_js_dispatch_mouse_event(browser->js, node, "pointerdown",
                                   (double)x - browser->cur_scroll_x,
                                   (double)y - browser->cur_scroll_y,
                                   (double)x, (double)y,
                                   0, 1, sh, ct, al, me, NULL, NULL);
        ns_js_dispatch_mouse_event(browser->js, node, "mousedown",
                                   (double)x - browser->cur_scroll_x,
                                   (double)y - browser->cur_scroll_y,
                                   (double)x, (double)y,
                                   0, 1, sh, ct, al, me, NULL, NULL);
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    }

    gboolean in_datalist = FALSE;
    for (const ns_node *a = node; a; a = a->parent)
        if (ns_node_is_element_named(a, "datalist")) { in_datalist = TRUE; break; }
    if (browser->js && !in_datalist) {
        const ns_node *focus = NULL;
        for (const ns_node *a = node; a; a = a->parent)
            if (ns_node_is_focusable(a)) { focus = a; break; }
        ns_js_focus_from_pointer(browser->js, node);
        const char *val = focus ? ns_node_editable_value(focus) : NULL;
        browser->caret_byte = val ? strlen(val) : 0;
        browser->sel_anchor_byte = browser->caret_byte;
        browser->datalist_suppressed =
            !(focus && ns_node_is_element_named(focus, "input") &&
              ns_element_get_attr(focus, "list") != NULL);
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    }

    if (browser->js) {
        if (ns_js_run_animation_frame(browser->js)) browser->dirty = TRUE;
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    }
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }

    char *nav = browser->pending_nav;
    browser->pending_nav = NULL;
    return nav;
}

static gboolean
ns_is_dropdown_select(const ns_node *n)
{
    if (!ns_node_is_element_named(n, "select")) return FALSE;
    if (ns_element_get_attr(n, "multiple")) return FALSE;
    const char *sz = ns_element_get_attr(n, "size");
    if (sz && atoi(sz) > 1) return FALSE;
    return TRUE;
}

static gboolean
browser_datalist_click(ns_browser *browser, const ns_node *node)
{
    const ns_node *option = NULL, *dl = NULL;
    for (const ns_node *a = node; a; a = a->parent) {
        if (!option && ns_node_is_element_named(a, "option")) option = a;
        if (ns_node_is_element_named(a, "datalist")) { dl = a; break; }
        if (ns_node_is_element_named(a, "select")) return FALSE;
    }
    if (!option || !dl || !browser->js) return FALSE;
    ns_node *inp = (ns_node *)ns_js_focused_node(browser->js);
    if (!inp || !ns_node_is_element_named(inp, "input")) return FALSE;
    const char *ov = ns_element_get_attr(option, "value");
    char *val = (ov && *ov) ? g_strdup(ov) : ns_option_label_dup(option);
    const char *cur = ns_node_editable_value(inp);
    browser_input_replace(browser, inp, 0, cur ? strlen(cur) : 0,
                          val ? val : "", "insertReplacementText");
    ns_js_commit_change(browser->js, inp);
    ns_js_consume_mutated(browser->js);
    g_free(val);
    browser->datalist_suppressed = TRUE;
    browser->dirty = TRUE;
    return TRUE;
}

static gboolean
browser_dropdown_click(ns_browser *browser, const ns_node *node)
{
    if (browser_datalist_click(browser, node)) return TRUE;
    const ns_node *option = NULL, *select = NULL;
    for (const ns_node *a = node; a; a = a->parent) {
        if (!option && ns_node_is_element_named(a, "option")) option = a;
        if (ns_node_is_element_named(a, "select")) { select = a; break; }
    }
    if (select && option && !ns_is_dropdown_select(select) &&
        !ns_element_get_attr(select, "disabled")) {
        gboolean multiple = ns_element_get_attr(select, "multiple") != NULL;
        gboolean toggle = multiple && (browser->press_mods & 2) != 0;
        if (browser->js) {
            if (toggle)
                ns_js_select_toggle_option(browser->js, (ns_node *)option);
            else
                ns_js_select_choose_option(browser->js, (ns_node *)option);
            ns_js_consume_mutated(browser->js);
        }
        browser->dirty = TRUE;
        return TRUE;
    }
    if (browser->open_select && option && select == browser->open_select) {
        if (browser->js &&
            ns_js_select_choose_option(browser->js, (ns_node *)option)) {
            ns_js_consume_mutated(browser->js);
            browser->open_select = NULL;
        }
        browser->dirty = TRUE;
        return TRUE;
    }
    if (select && ns_is_dropdown_select(select) &&
        !ns_element_get_attr(select, "disabled")) {
        browser->open_select =
            (browser->open_select == select) ? NULL : select;
        browser->dirty = TRUE;
        return TRUE;
    }
    if (browser->open_select) {
        browser->open_select = NULL;
        browser->dirty = TRUE;
    }
    return FALSE;
}

char *
ns_browser_release_click(ns_browser *browser, int *out_changed)
{
    if (out_changed) *out_changed = 0;
    if (!browser) {
        if (out_changed) *out_changed = -1;
        return NULL;
    }
    ns_browser_core_damp_reset(browser);
    g_clear_pointer(&browser->pending_nav, g_free);

    ns_browser_core_prune(browser);
    const ns_node *node = browser->press_active ? browser->press_node : NULL;
    int x = browser->press_x;
    int y = browser->press_y;
    int mods = browser->press_mods;
    browser->press_node = NULL;
    browser->press_active = FALSE;

    gboolean drag_selected = browser->selection_dragged &&
                             ns_selection_has_range(&browser->selection);
    browser->selection_dragged = FALSE;

    gboolean prevented = FALSE;
    if (browser->js && node) {
        gboolean sh = (mods & 1) != 0, ct = (mods & 2) != 0;
        gboolean al = (mods & 4) != 0, me = (mods & 8) != 0;
        ns_js_dispatch_mouse_event(browser->js, node, "pointerup",
                                   (double)x - browser->cur_scroll_x,
                                   (double)y - browser->cur_scroll_y,
                                   (double)x, (double)y,
                                   0, 0, sh, ct, al, me, NULL, NULL);
        ns_js_dispatch_mouse_event(browser->js, node, "mouseup",
                                   (double)x - browser->cur_scroll_x,
                                   (double)y - browser->cur_scroll_y,
                                   (double)x, (double)y,
                                   0, 0, sh, ct, al, me, NULL, NULL);
        if (!drag_selected)
            ns_js_dispatch_mouse_event(browser->js, node, "click",
                                       (double)x - browser->cur_scroll_x,
                                       (double)y - browser->cur_scroll_y,
                                       (double)x, (double)y,
                                       0, 0, sh, ct, al, me, NULL, &prevented);
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    }
    if (drag_selected) prevented = TRUE;

    gboolean select_consumed =
        !prevented && node && browser_dropdown_click(browser, node);

    if (!select_consumed) {
    if (!prevented && browser->js && node &&
        ns_js_click_activate(browser->js, node))
        browser->dirty = TRUE;

    if (!prevented && node && browser->js &&
        ns_js_activate_summary(browser->js, node)) {
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    } else if (!prevented && !browser->pending_nav && node &&
        ns_form_is_submit_trigger(node)) {
        ns_browser_core_submit_form(browser, node);
    } else if (!prevented && node && browser->js && browser->doc &&
               ns_form_is_reset_trigger(node)) {
        ns_node *form = (ns_node *)ns_form_owner(node, browser->doc);
        if (form) {
            ns_js_form_reset(browser->js, form);
            if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
        }
    } else if (!prevented && !browser->pending_nav) {
        const char *href = NULL;
        const char *download = NULL;
        for (const ns_node *a = node; a && !href; a = a->parent) {
            if (browser_node_is_hyperlink(a)) {
                const char *h = ns_element_get_attr(a, "href");
                if (h && *h) {
                    href = h;
                    download = ns_element_get_attr(a, "download");
                }
            }
        }
        if (!href)
            href = ns_box_hit_link(browser->layout, (double)x, (double)y);
        if (href && *href && download)
            ns_browser_core_js_download(href, download, browser);
        else if (href && *href)
            browser->pending_nav = ns_browser_core_resolve_navigation(browser, href);
    }
    }

    const ns_node *prev = ns_css_set_active_node(NULL);
    if (prev && ns_render_page_uses_active())
        browser->dirty = TRUE;

    if (browser->js) {
        if (ns_js_run_animation_frame(browser->js)) browser->dirty = TRUE;
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    }
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
        if (out_changed) *out_changed = 1;
    }

    char *nav = browser->pending_nav;
    browser->pending_nav = NULL;
    return nav;
}

int
ns_browser_release(ns_browser *browser)
{
    int changed = 0;
    char *nav = ns_browser_release_click(browser, &changed);
    free(nav);
    return changed;
}

static void
browser_video_click_toggle(ns_browser *browser, int x, int y)
{
    if (!browser || !browser->layout || !browser->videos) return;
    const ns_box *hit = ns_box_hit_test(browser->layout, (double)x, (double)y);
    for (const ns_box *b = hit; b; b = b->parent) {
        if (!b->media || !b->media->video) continue;
        ns_video *iv = b->media->video;
        if (iv->is_camera || !iv->player) return;
        if (browser->js && b->dom &&
            ns_js_node_has_click_handler(browser->js, b->dom))
            return;
        ns_video_cache_toggle(browser->videos, iv, g_get_monotonic_time());
        browser->dirty = TRUE;
        return;
    }
}

char *
ns_browser_click(ns_browser *browser, int x, int y, int mods)
{
    char *nav = ns_browser_press(browser, x, y, mods);
    if (nav && *nav) {
        if (browser) {
            browser->press_node = NULL;
            browser->press_active = FALSE;
            ns_css_set_active_node(NULL);
        }
        return nav;
    }
    free(nav);
    char *out = ns_browser_release_click(browser, NULL);
    if (!out || !*out)
        browser_video_click_toggle(browser, x, y);
    return out;
}

static void
browser_input_replace(ns_browser *b, ns_node *node, gsize del_start,
                      gsize del_end, const char *insert,
                      const char *input_type)
{
    const char *cur = ns_node_editable_value(node);
    if (!cur) return;
    gsize cur_len = strlen(cur);
    if (del_start > cur_len) del_start = cur_len;
    if (del_end > cur_len) del_end = cur_len;
    if (del_end < del_start) del_end = del_start;
    gsize ins_len = insert ? strlen(insert) : 0;

    GString *s = g_string_sized_new(cur_len - (del_end - del_start) + ins_len);
    g_string_append_len(s, cur, (gssize)del_start);
    if (ins_len) g_string_append_len(s, insert, (gssize)ins_len);
    g_string_append_len(s, cur + del_end, (gssize)(cur_len - del_end));

    const char *data = strcmp(input_type, "insertLineBreak") == 0 ? NULL
                                                                   : insert;
    if (b->js) {
        gboolean prevented = FALSE;
        ns_js_dispatch_input_event(b->js, node, "beforeinput", input_type,
                                   data, &prevented);
        if (prevented || ns_js_focused_node(b->js) != node) {
            g_string_free(s, TRUE);
            return;
        }
        ns_js_note_user_edit(b->js, node, ns_node_editable_value(node));
    }
    ns_node_set_editable_value(node, s->str);
    b->caret_byte = del_start + ins_len;
    b->sel_anchor_byte = b->caret_byte;
    g_string_free(s, TRUE);
    if (b->js) {
        ns_js_dispatch_input_event(b->js, node, "input", input_type, data,
                                   NULL);
        (void)ns_js_consume_mutated(b->js);
    }
}

static gboolean
browser_edit_key(ns_browser *b, ns_node *node, const char *key, int mods)
{
    gboolean shift = (mods & 1) != 0, ctrl = (mods & 2) != 0;
    if (mods & (4 | 8)) return FALSE;
    const char *cur = ns_node_editable_value(node);
    if (!cur || !key || !*key) return FALSE;
    gsize cur_len = strlen(cur);
    if (b->caret_byte > cur_len) b->caret_byte = cur_len;
    if (b->sel_anchor_byte > cur_len) b->sel_anchor_byte = cur_len;
    gsize sel_lo = b->sel_anchor_byte < b->caret_byte ? b->sel_anchor_byte
                                                      : b->caret_byte;
    gsize sel_hi = b->sel_anchor_byte < b->caret_byte ? b->caret_byte
                                                      : b->sel_anchor_byte;
    gboolean has_sel = sel_lo != sel_hi;
    gboolean multiline = (node->name && strcmp(node->name, "textarea") == 0) ||
                         ns_node_is_contenteditable_host(node);

    if (ctrl) {
        if ((key[0] == 'a' || key[0] == 'A') && !key[1]) {
            b->sel_anchor_byte = 0;
            b->caret_byte = cur_len;
            return TRUE;
        }
        return FALSE;
    }
    if (strcmp(key, "Backspace") == 0) {
        if (has_sel)
            browser_input_replace(b, node, sel_lo, sel_hi, NULL,
                                  "deleteContentBackward");
        else if (b->caret_byte > 0) {
            const char *prev = g_utf8_prev_char(cur + b->caret_byte);
            browser_input_replace(b, node, (gsize)(prev - cur), b->caret_byte,
                                  NULL, "deleteContentBackward");
        }
        return TRUE;
    }
    if (strcmp(key, "Delete") == 0) {
        if (has_sel)
            browser_input_replace(b, node, sel_lo, sel_hi, NULL,
                                  "deleteContentForward");
        else if (b->caret_byte < cur_len) {
            const char *nxt = g_utf8_next_char(cur + b->caret_byte);
            browser_input_replace(b, node, b->caret_byte, (gsize)(nxt - cur),
                                  NULL, "deleteContentForward");
        }
        return TRUE;
    }
    if (strcmp(key, "ArrowLeft") == 0) {
        if (has_sel && !shift) b->caret_byte = sel_lo;
        else if (b->caret_byte > 0)
            b->caret_byte = (gsize)(g_utf8_prev_char(cur + b->caret_byte) - cur);
        if (!shift) b->sel_anchor_byte = b->caret_byte;
        return TRUE;
    }
    if (strcmp(key, "ArrowRight") == 0) {
        if (has_sel && !shift) b->caret_byte = sel_hi;
        else if (b->caret_byte < cur_len)
            b->caret_byte = (gsize)(g_utf8_next_char(cur + b->caret_byte) - cur);
        if (!shift) b->sel_anchor_byte = b->caret_byte;
        return TRUE;
    }
    if (strcmp(key, "Home") == 0) {
        b->caret_byte = 0;
        if (!shift) b->sel_anchor_byte = 0;
        return TRUE;
    }
    if (strcmp(key, "End") == 0) {
        b->caret_byte = cur_len;
        if (!shift) b->sel_anchor_byte = cur_len;
        return TRUE;
    }
    if (strcmp(key, "Enter") == 0) {
        if (multiline) {
            browser_input_replace(b, node, sel_lo, sel_hi, "\n",
                                  "insertLineBreak");
            return TRUE;
        }
        if (b->js) {
            ns_js_commit_change(b->js, node);
            if (ns_js_focused_node(b->js) != node) return TRUE;
        }
        ns_browser_core_submit_form(b, node);
        return TRUE;
    }
    if (g_utf8_strlen(key, -1) == 1 &&
        !g_unichar_iscntrl(g_utf8_get_char(key))) {
        browser_input_replace(b, node, sel_lo, sel_hi, key, "insertText");
        return TRUE;
    }
    return FALSE;
}

static int
browser_key_char_code(const char *key)
{
    if (!key || !*key || !g_utf8_validate(key, -1, NULL))
        return 0;
    const char *next = g_utf8_next_char(key);
    if (!next || *next)
        return 0;
    gunichar ch = g_utf8_get_char(key);
    return g_unichar_iscntrl(ch) ? 0 : (int)ch;
}

static gboolean
browser_accesskey_matches(const ns_node *el, const char *key)
{
    const char *ak = ns_element_get_attr(el, "accesskey");
    if (!ak || !*ak || !key || !*key) return FALSE;
    gsize klen = strlen(key);
    for (const char *p = ak; *p;) {
        while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
        const char *s = p;
        while (*p && *p != ' ' && *p != '\t' && *p != '\n' && *p != '\r') p++;
        gsize len = (gsize)(p - s);
        if (len == klen && g_ascii_strncasecmp(s, key, len) == 0)
            return TRUE;
    }
    return FALSE;
}

static const ns_node *
browser_find_accesskey(const ns_node *n, const char *key, int depth)
{
    if (!n || depth > 1024) return NULL;
    if (n->kind == NS_NODE_ELEMENT && n->name &&
        g_ascii_strcasecmp(n->name, "template") == 0)
        return NULL;
    if (n->kind == NS_NODE_ELEMENT && browser_accesskey_matches(n, key) &&
        !ns_element_effectively_inert(n))
        return n;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        const ns_node *m = browser_find_accesskey(c, key, depth + 1);
        if (m) return m;
    }
    return NULL;
}

static gboolean
browser_select_key(ns_browser *browser, ns_node *select, const char *key,
                   int mods)
{
    if (!key || !*key || !browser->js) return FALSE;
    gboolean dropdown = ns_is_dropdown_select(select);
    if (strcmp(key, "ArrowDown") == 0)
        return ns_js_select_step(browser->js, select, +1);
    if (strcmp(key, "ArrowUp") == 0)
        return ns_js_select_step(browser->js, select, -1);
    if (strcmp(key, "Home") == 0)
        return ns_js_select_edge(browser->js, select, FALSE);
    if (strcmp(key, "End") == 0)
        return ns_js_select_edge(browser->js, select, TRUE);
    if (dropdown && (strcmp(key, "Enter") == 0 || strcmp(key, " ") == 0)) {
        browser->open_select = (browser->open_select == select) ? NULL : select;
        return TRUE;
    }
    if (dropdown && strcmp(key, "Escape") == 0 &&
        browser->open_select == select) {
        browser->open_select = NULL;
        return TRUE;
    }
    if ((mods & (2 | 4 | 8)) == 0 && g_utf8_validate(key, -1, NULL) &&
        g_utf8_strlen(key, -1) == 1 && key[0] != ' ' &&
        g_unichar_isprint(g_utf8_get_char(key)))
        return ns_js_select_typeahead(browser->js, select, key);
    return FALSE;
}

static char *
browser_paste_text_for(const ns_node *field, const char *text, gsize lo,
                       gsize hi)
{
    gboolean multiline = ns_node_is_element_named(field, "textarea") ||
                         ns_node_is_contenteditable_host(field);
    gsize len = strlen(text);
    if (!multiline)
        while (len > 0 && (text[len - 1] == '\n' || text[len - 1] == '\r'))
            len--;
    GString *out = g_string_sized_new(len);
    for (gsize i = 0; i < len; i++) {
        char c = text[i];
        if (c == '\r') {
            if (i + 1 < len && text[i + 1] == '\n') i++;
            c = '\n';
        }
        g_string_append_c(out, c == '\n' && !multiline ? ' ' : c);
    }
    const char *maxlength = ns_node_is_contenteditable_host(field)
                          ? NULL : ns_element_get_attr(field, "maxlength");
    char *end = NULL;
    long max = maxlength ? strtol(maxlength, &end, 10) : -1;
    if (maxlength && end != maxlength && max >= 0) {
        const char *cur = ns_node_editable_value(field);
        glong kept = g_utf8_strlen(cur, (gssize)lo) +
                     g_utf8_strlen(cur + hi, -1);
        glong room = max > kept ? max - kept : 0;
        if (g_utf8_strlen(out->str, -1) > room)
            g_string_truncate(out, (gsize)(g_utf8_offset_to_pointer(out->str,
                                                                    room) -
                                           out->str));
    }
    return g_string_free(out, FALSE);
}

static void
browser_paste(ns_browser *browser, const ns_node *target, const char *text)
{
    gboolean prevented = FALSE;
    ns_js_dispatch_clipboard_event(browser->js, target, "paste", text,
                                   &prevented);
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    ns_node *field = browser_focused_field(browser);
    if (prevented || !browser_field_writable(field)) return;
    gsize lo, hi;
    browser_field_selection(browser, field, &lo, &hi);
    char *insert = browser_paste_text_for(field, text, lo, hi);
    if (*insert) {
        browser_input_replace(browser, field, lo, hi, insert,
                              "insertFromPaste");
        browser->datalist_suppressed = FALSE;
        browser->dirty = TRUE;
    }
    g_free(insert);
}

char *
ns_browser_key_full(ns_browser *browser, int kind, const char *key,
                    const char *code, int keycode, int mods,
                    int *out_prevented)
{
    if (out_prevented) *out_prevented = 0;
    if (!browser || !browser->js) return NULL;
    ns_js_note_pointer_input(browser->js, FALSE);
    ns_browser_core_damp_reset(browser);
    g_clear_pointer(&browser->pending_nav, g_free);

    const ns_node *target = ns_js_focused_node(browser->js);
    if (!target && browser->doc)
        target = ns_node_find_first_element(browser->doc, "body");
    if (!target) return NULL;

    if (kind == 2) {
        const ns_node *f = ns_js_focused_node(browser->js);
        if (f && ns_node_editable_value(f) && key && *key &&
            g_utf8_validate(key, -1, NULL)) {
            const char *cur = ns_node_editable_value(f);
            gsize cur_len = cur ? strlen(cur) : 0;
            if (browser->caret_byte > cur_len)
                browser->caret_byte = cur_len;
            if (browser->sel_anchor_byte > cur_len)
                browser->sel_anchor_byte = cur_len;
            gsize lo = browser->sel_anchor_byte < browser->caret_byte
                       ? browser->sel_anchor_byte : browser->caret_byte;
            gsize hi = browser->sel_anchor_byte < browser->caret_byte
                       ? browser->caret_byte : browser->sel_anchor_byte;
            browser_input_replace(browser, (ns_node *)f, lo, hi, key,
                                  "insertText");
            browser->datalist_suppressed = FALSE;
            browser->dirty = TRUE;
        }
    } else if (kind == 4) {
        if (key && *key && g_utf8_validate(key, -1, NULL))
            browser_paste(browser, target, key);
    } else if (kind == 3) {
        int char_code = browser_key_char_code(key);
        if (!browser->keydown_prevented && char_code > 0 &&
            (mods & (2 | 4 | 8)) == 0) {
            gboolean press_prevented = FALSE;
            ns_js_dispatch_key_event_full(browser->js, target, "keypress",
                                          key ? key : "", code ? code : "",
                                          keycode, char_code,
                                          (mods & 1) != 0, FALSE, FALSE, FALSE,
                                          &press_prevented);
            if (out_prevented && press_prevented)
                *out_prevented = 1;
            if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
        }
        browser->keydown_prevented = FALSE;
    } else {
        gboolean prevented = FALSE;
        ns_js_dispatch_key_event_full(browser->js, target,
                                      kind == 1 ? "keyup" : "keydown",
                                      key ? key : "", code ? code : "",
                                      keycode, 0,
                                      (mods & 1) != 0, (mods & 2) != 0,
                                      (mods & 4) != 0, (mods & 8) != 0,
                                      &prevented);
        if (out_prevented && prevented)
            *out_prevented = 1;
        browser->keydown_prevented = kind == 0 && prevented;
        if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;

        if (kind == 0 && !prevented && (mods & 4) && !(mods & (2 | 8)) &&
            key && g_utf8_validate(key, -1, NULL) &&
            g_utf8_strlen(key, -1) == 1 && browser->doc) {
            const ns_node *ak = browser_find_accesskey(browser->doc, key, 0);
            if (ak) {
                ns_js_set_focus(browser->js, ak);
                ns_js_activate_element(browser->js, ak);
                if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
                if (out_prevented) *out_prevented = 1;
            }
        }

        if (!prevented && kind == 0 && key && strcmp(key, "Tab") == 0) {
            gboolean backward = (mods & 1) != 0;
            const ns_node *next =
                ns_js_sequential_focus_target(browser->js, backward);
            if (next) {
                ns_js_set_focus(browser->js, (ns_node *)next);
                const char *val = ns_node_editable_value(next);
                browser->caret_byte = val ? strlen(val) : 0;
                browser->sel_anchor_byte = browser->caret_byte;
                browser->dirty = TRUE;
            }
        } else if (!prevented && kind == 0) {
            const ns_node *f = ns_js_focused_node(browser->js);
            if (f && ns_node_is_element_named(f, "input") &&
                !browser->datalist_suppressed && key &&
                strcmp(key, "Escape") == 0 &&
                ns_element_get_attr(f, "list") != NULL) {
                browser->datalist_suppressed = TRUE;
                browser->dirty = TRUE;
                if (out_prevented) *out_prevented = 1;
            } else if (f && ns_node_is_element_named(f, "select") &&
                !ns_element_get_attr(f, "disabled") &&
                browser_select_key(browser, (ns_node *)f, key, mods)) {
                browser->dirty = TRUE;
                if (out_prevented) *out_prevented = 1;
            } else if (f && !(mods & (2 | 4 | 8)) &&
                       ns_js_keyboard_activates(f, key)) {
                if (out_prevented) *out_prevented = 1;
            } else if (f && !(mods & (2 | 4 | 8)) &&
                       ns_js_keyboard_activate(browser->js, f, key, FALSE)) {
                browser->dirty = TRUE;
                if (out_prevented) *out_prevented = 1;
            } else if (f && ns_node_editable_value(f) &&
                browser_edit_key(browser, (ns_node *)f, key, mods)) {
                browser->dirty = TRUE;
            } else if (key && strcmp(key, "Escape") == 0 &&
                       ns_js_process_close_request(browser->js)) {
                browser->dirty = TRUE;
                if (out_prevented) *out_prevented = 1;
            }
        } else if (!prevented && kind == 1) {
            const ns_node *f = ns_js_focused_node(browser->js);
            if (f && !(mods & (2 | 4 | 8)) &&
                ns_js_keyboard_activate(browser->js, f, key, TRUE))
                browser->dirty = TRUE;
        }
    }

    if (ns_js_run_animation_frame(browser->js)) browser->dirty = TRUE;
    if (ns_js_consume_mutated(browser->js)) browser->dirty = TRUE;
    if (browser->dirty) {
        ns_browser_core_relayout(browser);
        browser->dirty = FALSE;
    }

    char *nav = browser->pending_nav;
    browser->pending_nav = NULL;
    return nav;
}

char *
ns_browser_key(ns_browser *browser, int kind, const char *key,
               const char *code, int keycode, int mods)
{
    return ns_browser_key_full(browser, kind, key, code, keycode, mods, NULL);
}

