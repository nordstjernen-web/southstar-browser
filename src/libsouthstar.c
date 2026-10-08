/* Southstar — public C embedding API implementation.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "libsouthstar.h"

#include <glib.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

#include "anim.h"
#include "css.h"
#include "dom.h"
#include "forms.h"
#include "image.h"
#include "js.h"
#include "layout.h"
#include "net.h"
#include "render.h"
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

