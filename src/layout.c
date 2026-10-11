/* Southstar — block layout.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "layout.h"
#include "layout_internal.h"

#include <math.h>
#include "ns_pango.h"
#include <stdlib.h>
#include <string.h>

#include "css.h"
#include "html.h"
#include "image.h"
#include "svg.h"
#include "mathml.h"
#include "net.h"
#include "paint.h"

#define length_or ns_css_length_or

static double
length_resolve(const ns_css_value *v, double basis, double fallback)
{
    if (!v) return fallback;
    if (ns_css_calc_is_math_fn(v))
        return ns_css_calc_math_fn_px(v, basis);
    if (v->kind == NS_CSS_V_CALC)
        return v->u.calc.pct / 100.0 * basis + v->u.calc.px;
    if (v->kind != NS_CSS_V_LENGTH) return fallback;
    if (v->u.length.unit == NS_CSS_UNIT_PX ||
        v->u.length.unit == NS_CSS_UNIT_NUMBER) return v->u.length.v;
    if (v->u.length.unit == NS_CSS_UNIT_EM ||
        v->u.length.unit == NS_CSS_UNIT_REM) return v->u.length.v * 16.0;
    if (v->u.length.unit == NS_CSS_UNIT_PERCENT)
        return v->u.length.v * basis / 100.0;
    if (v->u.length.unit == NS_CSS_UNIT_VW)
        return v->u.length.v * ns_css_viewport_w() / 100.0;
    if (v->u.length.unit == NS_CSS_UNIT_VH)
        return v->u.length.v * ns_css_viewport_h() / 100.0;
    if (v->u.length.unit == NS_CSS_UNIT_VMIN) {
        double m = MIN(ns_css_viewport_w(), ns_css_viewport_h());
        return v->u.length.v * m / 100.0;
    }
    if (v->u.length.unit == NS_CSS_UNIT_VMAX) {
        double m = MAX(ns_css_viewport_w(), ns_css_viewport_h());
        return v->u.length.v * m / 100.0;
    }
    if (v->u.length.unit == NS_CSS_UNIT_CQW) {
        double cw = ns_css_container_w();
        return v->u.length.v * (cw > 0 ? cw : ns_css_viewport_w()) / 100.0;
    }
    if (v->u.length.unit == NS_CSS_UNIT_CQH) {
        double ch = ns_css_container_h();
        return v->u.length.v * (ch > 0 ? ch : ns_css_viewport_h()) / 100.0;
    }
    if (v->u.length.unit == NS_CSS_UNIT_CQMIN ||
        v->u.length.unit == NS_CSS_UNIT_CQMAX) {
        double cw = ns_css_container_w(); if (cw <= 0) cw = ns_css_viewport_w();
        double ch = ns_css_container_h(); if (ch <= 0) ch = ns_css_viewport_h();
        double m = v->u.length.unit == NS_CSS_UNIT_CQMIN ? MIN(cw, ch) : MAX(cw, ch);
        return v->u.length.v * m / 100.0;
    }
    return fallback;
}

static double
length_resolve_nonnegative(const ns_css_value *v, double basis,
                           double fallback)
{
    double resolved = length_resolve(v, basis, fallback);
    return resolved < 0 ? 0 : resolved;
}

double
ns_inline_text_indent_px(const ns_box *run, const ns_style *s, double basis)
{
    return run && run->inline_split_tail ? 0 : ns_text_indent_px(s, basis);
}

double
ns_text_indent_px(const ns_style *s, double basis)
{
    const ns_css_value *v = s ? s->values[NS_CSS_TEXT_INDENT] : NULL;
    return v ? length_resolve(v, basis, 0) : 0;
}

static gboolean
length_is_auto(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "auto") == 0;
}

static double
aspect_ratio_number(const ns_css_value *v, gboolean *with_auto)
{
    if (with_auto) *with_auto = FALSE;
    if (!v || v->kind != NS_CSS_V_SIZE || v->u.size.w_unit != NS_CSS_UNIT_NUMBER ||
        v->u.size.h_unit != NS_CSS_UNIT_NUMBER)
        return -1;
    if (with_auto) *with_auto = v->u.size.w_auto;
    return v->u.size.w > 0 && v->u.size.h > 0 ? v->u.size.w / v->u.size.h : -1;
}

static gboolean
value_is_percent(const ns_css_value *v)
{
    if (!v) return FALSE;
    if (v->kind == NS_CSS_V_LENGTH && v->u.length.unit == NS_CSS_UNIT_PERCENT)
        return TRUE;
    if (v->kind == NS_CSS_V_CALC && v->u.calc.pct != 0.0)
        return TRUE;
    if (ns_css_calc_is_math_fn(v))
        for (int i = 0; i < v->u.calc.n_args; i++)
            if (v->u.calc.args[i].pct != 0.0) return TRUE;
    return FALSE;
}

static gboolean
box_is_doc_root(const ns_box *b)
{
    return b && b->dom && b->dom->name &&
           (strcmp(b->dom->name, "html") == 0 ||
            strcmp(b->dom->name, "body") == 0);
}

static double containing_block_definite_height(const ns_box *box);
static gboolean style_is_absolute_or_fixed(const ns_style *s);
static gboolean height_keyword_stretches(const ns_css_value *v);

static double
resolve_used_height(const ns_box *box, const ns_css_value *hv,
                    double width_basis, double fallback)
{
    if (!hv) return fallback;
    if (value_is_percent(hv)) {
        double vh;
        if (box_is_doc_root(box)) {
            vh = ns_css_viewport_h();
        } else if (box && box->style &&
                   ns_css_keyword_is(box->style->values[NS_CSS_POSITION],
                                     "fixed")) {
            vh = ns_css_viewport_h();
        } else {
            vh = containing_block_definite_height(box);
            if (vh < 0) return fallback;
        }
        if (hv->kind == NS_CSS_V_CALC)
            return hv->u.calc.pct / 100.0 * vh + hv->u.calc.px;
        return hv->u.length.v * vh / 100.0;
    }
    return length_resolve(hv, width_basis, fallback);
}

static double
clamp_height_minmax_px(const ns_style *s, double h)
{
    if (!s || h < 0) return h;
    const ns_css_value *mx = s->values[NS_CSS_MAX_HEIGHT];
    if (mx && mx->kind == NS_CSS_V_LENGTH &&
        mx->u.length.unit != NS_CSS_UNIT_PERCENT) {
        double m = length_resolve(mx, 0, -1);
        if (m >= 0 && h > m) h = m;
    }
    const ns_css_value *mn = s->values[NS_CSS_MIN_HEIGHT];
    if (mn && mn->kind == NS_CSS_V_LENGTH &&
        mn->u.length.unit != NS_CSS_UNIT_PERCENT) {
        double m = length_resolve(mn, 0, -1);
        if (m >= 0 && h < m) h = m;
    }
    return h;
}

static double
specified_height_to_content(const ns_box *b, double h)
{
    if (h < 0 || !b->style ||
        !ns_css_keyword_is(b->style->values[NS_CSS_BOX_SIZING], "border-box"))
        return h;
    h -= b->padding.top + b->padding.bottom + b->border.top + b->border.bottom;
    return h < 0 ? 0 : h;
}

static double
box_read_definite_height(const ns_box *box)
{
    ((ns_box *)box)->definite_height_read = TRUE;
    return box->definite_height;
}

static double
containing_block_definite_height(const ns_box *box)
{
    if (box && box->cb_height_override > 0) return box->cb_height_override;
    const ns_box *p = box ? box->parent : NULL;
    while (p && !p->style) p = p->parent;
    if (!p) return -1;
    if (box_read_definite_height(p) > 0) return p->definite_height;
    const ns_css_value *h = p->style->values[NS_CSS_HEIGHT];
    if (height_keyword_stretches(h)) {
        double base = containing_block_definite_height(p);
        if (base < 0) return -1;
        double inner = base - p->margin.top - p->margin.bottom
                     - p->border.top - p->border.bottom
                     - p->padding.top - p->padding.bottom;
        return clamp_height_minmax_px(p->style, inner > 0 ? inner : 0);
    }
    if (h && h->kind == NS_CSS_V_KEYWORD) h = NULL;
    if (!h) {
        const ns_css_value *top = p->style->values[NS_CSS_TOP];
        const ns_css_value *bottom = p->style->values[NS_CSS_BOTTOM];
        if (style_is_absolute_or_fixed(p->style) &&
            top && !length_is_auto(top) &&
            bottom && !length_is_auto(bottom) &&
            p->content_height > 0)
            return p->content_height;
        double ratio = aspect_ratio_number(p->style->values[NS_CSS_ASPECT_RATIO], NULL);
        if (ratio > 0 && p->content_width > 0)
            return p->content_width / ratio;
        return -1;
    }
    if (value_is_percent(h)) {
        double base = box_is_doc_root(p) ? ns_css_viewport_h()
                                         : containing_block_definite_height(p);
        if (base < 0) return -1;
        double ch = h->kind == NS_CSS_V_CALC
            ? h->u.calc.pct / 100.0 * base + h->u.calc.px
            : h->u.length.v * base / 100.0;
        return specified_height_to_content(p, clamp_height_minmax_px(p->style, ch));
    }
    if (p->content_height > 0) return p->content_height;
    return specified_height_to_content(
        p, clamp_height_minmax_px(p->style, length_resolve(h, 0, -1)));
}

static double
resolve_height_with_basis(const ns_css_value *hv, double width_basis,
                          double height_basis, double fallback)
{
    if (!hv) return fallback;
    if (hv->kind == NS_CSS_V_CALC) {
        if (ns_css_calc_is_math_fn(hv) && height_basis >= 0)
            return ns_css_calc_math_fn_px(hv, height_basis);
        if (hv->u.calc.pct != 0.0 && height_basis >= 0)
            return hv->u.calc.pct / 100.0 * height_basis + hv->u.calc.px;
        return length_resolve(hv, width_basis, fallback);
    }
    if (hv->kind == NS_CSS_V_LENGTH &&
        hv->u.length.unit == NS_CSS_UNIT_PERCENT) {
        if (height_basis >= 0)
            return hv->u.length.v * height_basis / 100.0;
        return fallback;
    }
    return length_resolve(hv, width_basis, fallback);
}

#define is_keyword ns_css_keyword_is
#define keyword_is ns_css_keyword_is

static gboolean
display_is_atomic_inline_container(ns_display d)
{
    return ns_display_is_atomic_inline(d) && d.inner != NS_DISPLAY_INNER_TABLE;
}

static gboolean
style_is_block(const ns_style *s)
{
    return ns_display_generates_own_box(ns_css_display_of(s));
}

static gboolean
style_is_block_level(const ns_style *s)
{
    return ns_display_is_block_level(ns_css_display_of(s));
}

static gboolean
style_display_is_table(const ns_style *s)
{
    return ns_display_is_table_wrapper(ns_css_display_of(s));
}

static gboolean
style_display_is_table_row(const ns_style *s)
{
    return ns_display_is(ns_css_display_of(s), NS_DISPLAY_INTERNAL_TABLE_ROW);
}

static gboolean
style_display_is_table_cell(const ns_style *s)
{
    return ns_display_is(ns_css_display_of(s), NS_DISPLAY_INTERNAL_TABLE_CELL);
}

static gboolean
style_display_is_table_caption(const ns_style *s)
{
    return ns_display_is(ns_css_display_of(s),
                         NS_DISPLAY_INTERNAL_TABLE_CAPTION);
}

static gboolean
style_is_flex_container(const ns_style *s)
{
    return ns_display_is_flex_container(ns_css_display_of(s));
}

static gboolean
style_is_grid_container(const ns_style *s)
{
    return ns_display_is_grid_container(ns_css_display_of(s));
}

static const char *
keyword_or(const ns_style *s, ns_css_prop p, const char *fallback)
{
    if (!s || !s->values[p]) return fallback;
    const ns_css_value *v = s->values[p];
    if (v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return fallback;
    return ns_css_alignment_base(v->u.keyword);
}

static const char *
flex_direction_of(const ns_style *s)
{
    const char *dir = keyword_or(s, NS_CSS_FLEX_DIRECTION, "row");
    int writing_mode = ns_css_writing_mode(s);
    if (!writing_mode) return dir;
    gboolean reverse = strstr(dir, "-reverse") != NULL;
    if (strncmp(dir, "row", 3) == 0) {
        gboolean rtl = keyword_is(s->values[NS_CSS_DIRECTION], "rtl");
        return reverse != rtl ? "column-reverse" : "column";
    }
    gboolean right_to_left = writing_mode == 1;
    return reverse != right_to_left ? "row-reverse" : "row";
}

static gboolean
self_start_is_far_side(const ns_style *s, gboolean horizontal_axis)
{
    int writing_mode = ns_css_writing_mode(s);
    gboolean rtl = s && keyword_is(s->values[NS_CSS_DIRECTION], "rtl");
    if (horizontal_axis)
        return writing_mode == 1 ? TRUE : writing_mode == 2 ? FALSE : rtl;
    if (!writing_mode) return FALSE;
    gboolean upward = keyword_is(s->values[NS_CSS_WRITING_MODE], "sideways-lr");
    return upward != rtl;
}

static double
number_or(const ns_css_value *v, double fallback)
{
    if (!v) return fallback;
    if (v->kind == NS_CSS_V_LENGTH) return v->u.length.v;
    return fallback;
}

static int
box_css_order(const ns_box *b)
{
    if (!b || !b->style) return 0;
    return (int)number_or(b->style->values[NS_CSS_ORDER], 0);
}

static void
reorder_children_by_order(ns_box *box)
{
    int n = 0;
    gboolean any = FALSE;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        n++;
        if (box_css_order(c) != 0) any = TRUE;
    }
    if (n < 2 || !any) return;
    ns_box **arr = g_new(ns_box *, n);
    int i = 0;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) arr[i++] = c;
    for (int a = 1; a < n; a++) {
        ns_box *key = arr[a];
        int ko = box_css_order(key);
        int b = a - 1;
        while (b >= 0 && box_css_order(arr[b]) > ko) {
            arr[b + 1] = arr[b];
            b--;
        }
        arr[b + 1] = key;
    }
    for (int k = 0; k < n; k++)
        arr[k]->next_sibling = (k + 1 < n) ? arr[k + 1] : NULL;
    box->first_child = arr[0];
    box->last_child  = arr[n - 1];
    g_free(arr);
}

static gboolean
style_is_absolute_or_fixed(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_POSITION] : NULL;
    return keyword_is(v, "absolute") || keyword_is(v, "fixed");
}

static gboolean
style_is_none(const ns_style *s)
{
    return ns_display_is_none(ns_css_display_of(s));
}

static gboolean
style_is_contents(const ns_style *s)
{
    return ns_display_is_contents(ns_css_display_of(s));
}

static gboolean
text_is_ws_only(const char *text)
{
    if (!text) return TRUE;
    for (const char *p = text; *p; p++)
        if (!g_ascii_isspace((unsigned char)*p)) return FALSE;
    return TRUE;
}

static double
border_side_width(const ns_style *s, ns_css_prop width_prop,
                  ns_css_prop style_prop)
{
    const ns_css_value *st = s->values[style_prop];
    if (st && st->kind == NS_CSS_V_KEYWORD && st->u.keyword &&
        (strcmp(st->u.keyword, "none") == 0 ||
         strcmp(st->u.keyword, "hidden") == 0))
        return 0;
    return length_or(s->values[width_prop], 0);
}

static void
edges_from_style(const ns_style *s, double basis,
                 ns_edges *margin, ns_edges *padding, ns_edges *border)
{
    if (!s) {
        memset(margin, 0, sizeof(*margin));
        memset(padding, 0, sizeof(*padding));
        memset(border, 0, sizeof(*border));
        return;
    }
    margin->top    = length_resolve(s->values[NS_CSS_MARGIN_TOP],    basis, 0);
    margin->right  = length_resolve(s->values[NS_CSS_MARGIN_RIGHT],  basis, 0);
    margin->bottom = length_resolve(s->values[NS_CSS_MARGIN_BOTTOM], basis, 0);
    margin->left   = length_resolve(s->values[NS_CSS_MARGIN_LEFT],   basis, 0);
    padding->top    = length_resolve_nonnegative(
        s->values[NS_CSS_PADDING_TOP], basis, 0);
    padding->right  = length_resolve_nonnegative(
        s->values[NS_CSS_PADDING_RIGHT], basis, 0);
    padding->bottom = length_resolve_nonnegative(
        s->values[NS_CSS_PADDING_BOTTOM], basis, 0);
    padding->left   = length_resolve_nonnegative(
        s->values[NS_CSS_PADDING_LEFT], basis, 0);
    border->top    = border_side_width(s, NS_CSS_BORDER_TOP_WIDTH,
                                       NS_CSS_BORDER_TOP_STYLE);
    border->right  = border_side_width(s, NS_CSS_BORDER_RIGHT_WIDTH,
                                       NS_CSS_BORDER_RIGHT_STYLE);
    border->bottom = border_side_width(s, NS_CSS_BORDER_BOTTOM_WIDTH,
                                       NS_CSS_BORDER_BOTTOM_STYLE);
    border->left   = border_side_width(s, NS_CSS_BORDER_LEFT_WIDTH,
                                       NS_CSS_BORDER_LEFT_STYLE);
}

static ns_box *g_box_pool[16384];
static int g_box_pool_n;

static ns_box *
box_new(ns_box_kind kind)
{
    ns_box *b;
    if (g_box_pool_n > 0) {
        b = g_box_pool[--g_box_pool_n];
        memset(b, 0, sizeof(*b));
    } else {
        b = g_new0(ns_box, 1);
    }
    b->kind = kind;
    b->colspan = 1;
    b->rowspan = 1;
    return b;
}

static void
link_clear(gpointer data)
{
    ns_link_range *r = data;
    g_free(r->href);
    g_free(r->target);
}

static GArray *inline_links_ensure(ns_box *b);

static ns_box_media *
ns_box_media_ensure(ns_box *b)
{
    if (!b->media) b->media = g_new0(ns_box_media, 1);
    return b->media;
}

static ns_box *
inline_merge_prefix(ns_box *prefix, ns_box *suffix)
{
    if (!prefix) return suffix;
    if (!suffix) return prefix;
    gsize plen = prefix->text ? strlen(prefix->text) : 0;
    gsize slen = suffix->text ? strlen(suffix->text) : 0;
    if (plen > G_MAXSIZE - slen - 1) { ns_box_free(suffix); return prefix; }
    char *combined = g_malloc(plen + slen + 1);
    if (plen) memcpy(combined, prefix->text, plen);
    if (slen) memcpy(combined + plen, suffix->text, slen);
    combined[plen + slen] = '\0';
    g_free(suffix->text);
    suffix->text = combined;

    if (suffix->attrs) {
        for (guint i = 0; i < suffix->attrs->len; i++) {
            ns_inline_attr *a = &g_array_index(suffix->attrs, ns_inline_attr, i);
            a->start += plen;
        }
    }
    if (suffix->links) {
        for (guint i = 0; i < suffix->links->len; i++) {
            ns_link_range *l = &g_array_index(suffix->links, ns_link_range, i);
            l->start += plen;
        }
    }
    if (prefix->attrs) {
        for (guint i = 0; i < prefix->attrs->len; i++) {
            ns_inline_attr a = g_array_index(prefix->attrs, ns_inline_attr, i);
            g_array_append_val(suffix->attrs, a);
        }
    }
    if (suffix->inline_atomics) {
        for (guint i = 0; i < suffix->inline_atomics->len; i++)
            g_array_index(suffix->inline_atomics, ns_inline_atomic, i).byte_off += plen;
    }
    if (prefix->inline_atomics) {
        if (!suffix->inline_atomics)
            suffix->inline_atomics = g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
        for (guint i = 0; i < prefix->inline_atomics->len; i++) {
            ns_inline_atomic ia = g_array_index(prefix->inline_atomics, ns_inline_atomic, i);
            if (ia.box && ia.box->parent == prefix) ia.box->parent = suffix;
            g_array_append_val(suffix->inline_atomics, ia);
        }
        g_array_free(prefix->inline_atomics, TRUE);
        prefix->inline_atomics = NULL;
    }
    if (prefix->links) {
        GArray *dst = inline_links_ensure(suffix);
        for (guint i = 0; i < prefix->links->len; i++) {
            ns_link_range src = g_array_index(prefix->links, ns_link_range, i);
            ns_link_range dup = src;
            dup.href   = src.href   ? g_strdup(src.href)   : NULL;
            dup.target = src.target ? g_strdup(src.target) : NULL;
            g_array_append_val(dst, dup);
        }
    }
    ns_box_free(prefix);
    return suffix;
}

static ns_box *
box_new_inline(void)
{
    ns_box *b = box_new(NS_BOX_INLINE);
    b->attrs = g_array_new(FALSE, FALSE, sizeof(ns_inline_attr));
    return b;
}

static GArray *
inline_links_ensure(ns_box *b)
{
    if (!b->links) {
        b->links = g_array_new(FALSE, FALSE, sizeof(ns_link_range));
        g_array_set_clear_func(b->links, link_clear);
    }
    return b->links;
}

static void
inline_run_drop_caches(ns_box *run)
{
    run->inline_layout_cache_valid = FALSE;
    run->inline_natural_cache_valid = FALSE;
    run->inline_min_cache_valid = FALSE;
    if (run->paint_layout) ns_paint_drop_box_cache(run);
}

static void
inline_split_attrs(ns_box *run, ns_box *tail, gsize split)
{
    GArray *head_attrs = g_array_new(FALSE, FALSE, sizeof(ns_inline_attr));
    for (guint i = 0; run->attrs && i < run->attrs->len; i++) {
        ns_inline_attr a = g_array_index(run->attrs, ns_inline_attr, i);
        gsize end = a.start + a.len;
        if (a.start < split) {
            ns_inline_attr h = a;
            if (end > split) h.len = split - a.start;
            g_array_append_val(head_attrs, h);
        }
        if (end > split || a.start >= split) {
            ns_inline_attr t = a;
            t.start = MAX(a.start, split) - split;
            t.len = end > split ? end - MAX(a.start, split) : 0;
            g_array_append_val(tail->attrs, t);
        }
    }
    if (run->attrs) g_array_free(run->attrs, TRUE);
    run->attrs = head_attrs;
}

static ns_link_range
link_range_copy(const ns_link_range *l, gsize start, gsize len)
{
    ns_link_range c = *l;
    c.href = l->href ? g_strdup(l->href) : NULL;
    c.target = l->target ? g_strdup(l->target) : NULL;
    c.start = start;
    c.len = len;
    return c;
}

static void
inline_split_links(ns_box *run, ns_box *tail, gsize split)
{
    if (!run->links) return;
    GArray *head_links = g_array_new(FALSE, FALSE, sizeof(ns_link_range));
    g_array_set_clear_func(head_links, link_clear);
    for (guint i = 0; i < run->links->len; i++) {
        const ns_link_range *l = &g_array_index(run->links, ns_link_range, i);
        gsize end = l->start + l->len;
        if (l->start < split) {
            ns_link_range h = link_range_copy(l, l->start,
                                              MIN(end, split) - l->start);
            g_array_append_val(head_links, h);
        }
        if (end > split) {
            gsize from = MAX(l->start, split);
            ns_link_range t = link_range_copy(l, from - split, end - from);
            g_array_append_val(inline_links_ensure(tail), t);
        }
    }
    g_array_free(run->links, TRUE);
    run->links = head_links;
}

static void
inline_split_atomics(ns_box *run, ns_box *tail, gsize split)
{
    if (!run->inline_atomics) return;
    GArray *head_atomics = g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
    for (guint i = 0; i < run->inline_atomics->len; i++) {
        ns_inline_atomic ia =
            g_array_index(run->inline_atomics, ns_inline_atomic, i);
        if (ia.byte_off < split) {
            g_array_append_val(head_atomics, ia);
            continue;
        }
        if (!tail->inline_atomics)
            tail->inline_atomics =
                g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
        ia.byte_off -= split;
        if (ia.box && ia.box->parent == run) ia.box->parent = tail;
        g_array_append_val(tail->inline_atomics, ia);
    }
    g_array_free(run->inline_atomics, TRUE);
    run->inline_atomics = head_atomics;
}

static ns_box *
inline_run_split(ns_box *run, gsize split)
{
    ns_box *tail = box_new_inline();
    tail->dom = run->dom;
    tail->style = run->style;
    tail->inline_split_tail = TRUE;
    tail->text = g_strdup(run->text + split);
    run->text[split] = '\0';

    inline_split_attrs(run, tail, split);
    inline_split_links(run, tail, split);
    inline_split_atomics(run, tail, split);

    tail->parent = run->parent;
    tail->next_sibling = run->next_sibling;
    run->next_sibling = tail;
    if (run->parent && run->parent->last_child == run)
        run->parent->last_child = tail;
    inline_run_drop_caches(run);
    return tail;
}

static void
inline_runs_join_splits(ns_box *box)
{
    ns_box *prev = NULL;
    for (ns_box *c = box->first_child; c; ) {
        ns_box *next = c->next_sibling;
        if (!next || !next->inline_split_tail ||
            c->kind != NS_BOX_INLINE || next->kind != NS_BOX_INLINE) {
            prev = c;
            c = next;
            continue;
        }
        gboolean head_is_tail = c->inline_split_tail;
        ns_box *after = next->next_sibling;
        ns_box *joined = inline_merge_prefix(c, next);
        joined->inline_split_tail = head_is_tail;
        joined->parent = box;
        joined->next_sibling = after;
        if (prev) prev->next_sibling = joined;
        else box->first_child = joined;
        if (!after) box->last_child = joined;
        inline_run_drop_caches(joined);
        c = joined;
    }
}

static gsize
inline_run_break_before(const ns_box *run, double band_height)
{
    NsPangoLayout *layout = ns_paint_build_inline_layout(NULL, run);
    if (!layout) return 0;
    gsize split = 0;
    NsPangoLayoutIter *iter = ns_pango_layout_get_iter(layout);
    do {
        NsPangoRectangle logical;
        ns_pango_layout_iter_get_line_extents(iter, NULL, &logical);
        if ((double)logical.y / NS_PANGO_SCALE >= band_height - 0.5) {
            split = (gsize)ns_pango_layout_iter_get_index(iter);
            break;
        }
    } while (ns_pango_layout_iter_next_line(iter));
    ns_pango_layout_iter_free(iter);
    g_object_unref(layout);
    return split;
}

static void
box_append_child(ns_box *parent, ns_box *child)
{
    if (parent->last_child && parent->last_child->kind == NS_BOX_INLINE &&
        child->kind == NS_BOX_INLINE && !parent->last_child->style &&
        !child->style) {
        ns_box *prefix = parent->last_child;
        ns_box *previous = NULL;
        for (ns_box *c = parent->first_child; c && c != prefix;
             c = c->next_sibling)
            previous = c;
        ns_box *merged = inline_merge_prefix(prefix, child);
        merged->parent = parent;
        if (previous) previous->next_sibling = merged;
        else parent->first_child = merged;
        parent->last_child = merged;
        return;
    }
    child->parent = parent;
    if (!parent->first_child) parent->first_child = child;
    else                       parent->last_child->next_sibling = child;
    parent->last_child = child;
}

void
ns_box_free(ns_box *box)
{
    if (!box) return;
    GPtrArray *stack = g_ptr_array_new();
    g_ptr_array_add(stack, box);
    while (stack->len > 0) {
        ns_box *cur = g_ptr_array_index(stack, stack->len - 1);
        g_ptr_array_set_size(stack, stack->len - 1);
        for (ns_box *c = cur->first_child; c; ) {
            ns_box *next = c->next_sibling;
            g_ptr_array_add(stack, c);
            c = next;
        }
        if (cur->paint_layout) ns_paint_drop_box_cache(cur);
        if (cur->links) g_array_free(cur->links, TRUE);
        if (cur->attrs) g_array_free(cur->attrs, TRUE);
        if (cur->table_col_hints) g_array_free(cur->table_col_hints, TRUE);
        if (cur->grid_col_tracks) g_array_free(cur->grid_col_tracks, TRUE);
        if (cur->grid_row_tracks) g_array_free(cur->grid_row_tracks, TRUE);
        if (cur->inline_atomics) {
            for (guint i = 0; i < cur->inline_atomics->len; i++) {
                ns_inline_atomic *ia = &g_array_index(cur->inline_atomics,
                                                      ns_inline_atomic, i);
                if (ia->box) g_ptr_array_add(stack, ia->box);
            }
            g_array_free(cur->inline_atomics, TRUE);
        }
        if (cur->atomic_line_heights) {
            g_array_free(cur->atomic_line_heights, TRUE);
        }
        g_free(cur->text);
        if (cur->media) {
            g_free(cur->media->image_src);
            g_free(cur->media->bg_image_src);
            g_free(cur->media->marker_image_src);
            g_free(cur->media->border_image_src);
            if (cur->media->bg_layer_srcs)
                g_ptr_array_free(cur->media->bg_layer_srcs, TRUE);
            if (cur->media->bg_layer_images)
                g_ptr_array_free(cur->media->bg_layer_images, TRUE);
            g_free(cur->media->video_src);
            g_free(cur->media->video_poster);
            g_free(cur->media->video_audio_src);
            g_free(cur->media);
        }
        if (g_box_pool_n < (int)G_N_ELEMENTS(g_box_pool))
            g_box_pool[g_box_pool_n++] = cur;
        else
            g_free(cur);
    }
    g_ptr_array_free(stack, TRUE);
}

static gboolean box_first_baseline(const ns_box *b, double *out);

static gboolean
box_clips_for_page_height(const ns_box *b)
{
    if (!b->parent) return FALSE;
    if (b->dom && b->dom->name &&
        (strcmp(b->dom->name, "html") == 0 ||
         strcmp(b->dom->name, "body") == 0))
        return FALSE;
    return ns_layout_box_clips_children(b);
}

static void
box_walk_max_bottom(const ns_box *b, double *out)
{
    if (!b) return;
    double bottom = b->y + b->content_height;
    if (bottom > *out) *out = bottom;
    if (box_clips_for_page_height(b)) return;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        box_walk_max_bottom(c, out);
}

double
ns_box_max_bottom(const ns_box *root, double seed)
{
    double out = seed;
    box_walk_max_bottom(root, &out);
    return out;
}

static gboolean
is_replaced_block_tag(const char *name)
{
    return name && (strcmp(name, "img") == 0 ||
                    strcmp(name, "svg") == 0 ||
                    strcmp(name, "canvas") == 0 ||
                    strcmp(name, "audio") == 0 ||
                    strcmp(name, "video") == 0 ||
                    strcmp(name, "math") == 0 ||
                    strcmp(name, "table") == 0);
}

static gboolean
is_inline_level_replaced(const ns_node *n, GHashTable *styles)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    if (!(strcmp(n->name, "img") == 0 || strcmp(n->name, "svg") == 0 ||
          strcmp(n->name, "audio") == 0 ||
          strcmp(n->name, "video") == 0 || strcmp(n->name, "math") == 0 ||
          strcmp(n->name, "canvas") == 0))
        return FALSE;
    const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
    return !ns_display_is_block_level(ns_css_display_of(s));
}

static gboolean
node_has_media_metadata(const ns_node *n)
{
    return n && n->kind == NS_NODE_ELEMENT &&
           (ns_element_get_attr(n, NS_MEDIA_SRC_ATTR) != NULL ||
            ns_element_get_attr(n, NS_MEDIA_STREAM_ATTR) != NULL);
}

#define NS_LAYOUT_MAX_DEPTH 512

static gboolean
node_is_frame_fallback(const ns_node *n)
{
    if (!n || n->kind == NS_NODE_DOCUMENT) return FALSE;
    const ns_node *p = n->parent;
    return p && p->kind == NS_NODE_ELEMENT && p->name &&
           (strcmp(p->name, "iframe") == 0 || strcmp(p->name, "frame") == 0);
}

static GHashTable *g_contains_block_media_cache;

static gboolean
contains_block_media_depth(const ns_node *n, GHashTable *styles, int depth)
{
    if (!n || depth >= NS_LAYOUT_MAX_DEPTH || n->kind != NS_NODE_ELEMENT)
        return FALSE;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
        if (ns_layout_node_is_non_rendering(c)) continue;
        if (node_has_media_metadata(c) ||
            (is_replaced_block_tag(c->name) &&
             !is_inline_level_replaced(c, styles)) ||
            strcmp(c->name, "iframe") == 0)
            return TRUE;
        if (styles) {
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (cs) {
                if (style_is_none(cs) || style_is_absolute_or_fixed(cs))
                    continue;
                if (style_is_block_level(cs)) return TRUE;
                if (ns_display_is_atomic_inline(ns_css_display_of(cs)))
                    continue;
            }
        }
        if (contains_block_media_depth(c, styles, depth + 1)) return TRUE;
    }
    return FALSE;
}

static gboolean
contains_block_media(const ns_node *n, GHashTable *styles)
{
    if (!n) return FALSE;
    if (g_contains_block_media_cache) {
        gpointer hit = g_hash_table_lookup(g_contains_block_media_cache, n);
        if (hit) return GPOINTER_TO_INT(hit) == 2;
    }
    gboolean has_media = contains_block_media_depth(n, styles, 0);
    if (g_contains_block_media_cache)
        g_hash_table_insert(g_contains_block_media_cache,
                            (gpointer)n, GINT_TO_POINTER(has_media ? 2 : 1));
    return has_media;
}

static gboolean is_inline_dom(const ns_node *n, GHashTable *styles);

static gboolean
abs_joins_inline_run(const ns_node *n, const ns_style *s, GHashTable *styles)
{
    if (!s || !s->specified_inline || !n->parent) return FALSE;
    const ns_style *parent = g_hash_table_lookup(styles, n->parent);
    if (parent && (style_is_flex_container(parent) ||
                   style_is_grid_container(parent)))
        return FALSE;
    for (const ns_node *p = n->prev_sibling; p; p = p->prev_sibling) {
        if (p->kind == NS_NODE_TEXT) {
            for (const char *t = p->text; t && *t; t++)
                if (!g_ascii_isspace(*t)) return TRUE;
            continue;
        }
        if (p->kind != NS_NODE_ELEMENT) continue;
        const ns_style *ps = g_hash_table_lookup(styles, p);
        if (ps && (style_is_none(ps) || style_is_absolute_or_fixed(ps)))
            continue;
        return is_inline_dom(p, styles);
    }
    return FALSE;
}

static gboolean
is_inline_dom(const ns_node *n, GHashTable *styles)
{
    if (!n) return FALSE;
    if (n->kind == NS_NODE_TEXT) return TRUE;
    if (n->kind != NS_NODE_ELEMENT) return FALSE;
    if (n->name && strcmp(n->name, "slot") == 0) return FALSE;
    if (node_has_media_metadata(n)) return FALSE;
    for (const ns_node *sc = n->first_child; sc; sc = sc->next_sibling)
        if (sc->kind == NS_NODE_ELEMENT &&
            ns_element_get_attr(sc, NS_SHADOW_ATTR))
            return FALSE;
    if (is_replaced_block_tag(n->name)) {
        if (strcmp(n->name, "table") == 0) return FALSE;
        const ns_style *rs = g_hash_table_lookup(styles, n);
        if (rs && style_is_none(rs)) return FALSE;
        if (rs && style_is_absolute_or_fixed(rs))
            return abs_joins_inline_run(n, rs, styles);
        return is_inline_level_replaced(n, styles);
    }
    const ns_style *s = g_hash_table_lookup(styles, n);
    if (!s) return n->name && strchr(n->name, '-') != NULL;
    if (style_is_none(s)) return FALSE;
    if (style_is_absolute_or_fixed(s)) return abs_joins_inline_run(n, s, styles);
    ns_display d = ns_css_display_of(s);
    if (ns_display_is_table_internal(d)) return FALSE;
    if (ns_display_is_flex_container(d) || ns_display_is_grid_container(d)) {
        if (d.outer == NS_DISPLAY_OUTER_INLINE) return TRUE;
    }
    if (n->name && strcmp(n->name, "br") == 0 && ns_layout_clear_kind_of(s))
        return FALSE;
    if (!style_is_block(s) && contains_block_media(n, styles)) return FALSE;
    if (display_is_atomic_inline_container(d)) return TRUE;
    return !style_is_block(s);
}

static gboolean
node_leaves_inline_run_open(const ns_node *n, GHashTable *styles)
{
    if (n->kind == NS_NODE_COMMENT) return TRUE;
    if (n->kind != NS_NODE_ELEMENT) return FALSE;
    const ns_style *s = g_hash_table_lookup(styles, n);
    return s && style_is_none(s);
}

static gboolean
continues_inline_run(const ns_node *n, GHashTable *styles)
{
    return is_inline_dom(n, styles) || node_leaves_inline_run_open(n, styles);
}

static ns_display_internal
node_table_internal(const ns_node *n, GHashTable *styles)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return NS_DISPLAY_INTERNAL_NONE;
    ns_display d = ns_css_display_of(
        styles ? g_hash_table_lookup(styles, n) : NULL);
    return ns_display_is_table_internal(d) ? d.internal
                                           : NS_DISPLAY_INTERNAL_NONE;
}

static gboolean
node_is_row_group(const ns_node *n, GHashTable *styles)
{
    ns_display_internal k = node_table_internal(n, styles);
    return k == NS_DISPLAY_INTERNAL_TABLE_ROW_GROUP ||
           k == NS_DISPLAY_INTERNAL_TABLE_HEADER_GROUP ||
           k == NS_DISPLAY_INTERNAL_TABLE_FOOTER_GROUP;
}

static gboolean
is_table_row(const ns_node *n, GHashTable *styles)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return FALSE;
    if (n->name && strcmp(n->name, "tr") == 0) return TRUE;
    return style_display_is_table_row(g_hash_table_lookup(styles, n));
}

static gboolean
is_table_box(const ns_node *n, GHashTable *styles)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return FALSE;
    if (n->name && strcmp(n->name, "table") == 0) return TRUE;
    return style_display_is_table(g_hash_table_lookup(styles, n));
}

static gboolean
is_cell_element(const ns_node *n, GHashTable *styles)
{
    if (ns_node_is_element_named(n, "td") ||
        ns_node_is_element_named(n, "th"))
        return TRUE;
    if (!n || n->kind != NS_NODE_ELEMENT) return FALSE;
    return style_display_is_table_cell(g_hash_table_lookup(styles, n));
}

static gboolean
is_table_caption(const ns_node *n, GHashTable *styles)
{
    if (ns_node_is_element_named(n, "caption")) return TRUE;
    if (!n || n->kind != NS_NODE_ELEMENT) return FALSE;
    return style_display_is_table_caption(g_hash_table_lookup(styles, n));
}

static gboolean
node_is_table_internal(const ns_node *n, GHashTable *styles)
{
    return node_table_internal(n, styles) != NS_DISPLAY_INTERNAL_NONE ||
           is_cell_element(n, styles) || is_table_row(n, styles) ||
           is_table_caption(n, styles);
}

static void queue_absolute_node(const ns_node *n, const ns_style *s);

static void
collect_rows_recurse(const ns_node *n, GHashTable *styles, GPtrArray *out, int depth)
{
    if (!n || depth >= NS_LAYOUT_MAX_DEPTH) return;
    if (n->kind == NS_NODE_ELEMENT && n->name) {
        if (is_table_row(n, styles)) {
            const ns_style *rs = g_hash_table_lookup(styles, n);
            if (rs && style_is_none(rs)) return;
            if (rs && style_is_absolute_or_fixed(rs)) {
                queue_absolute_node(n, rs);
                return;
            }
            g_ptr_array_add(out, (gpointer)n);
            return;
        }
        if (n != NULL && (strcmp(n->name, "table") == 0 ||
                          style_display_is_table_caption(g_hash_table_lookup(styles, n)) ||
                          style_display_is_table(g_hash_table_lookup(styles, n))))
            return;
    }
    for (const ns_node *c = n->first_child; c; c = c->next_sibling)
        collect_rows_recurse(c, styles, out, depth + 1);
}

typedef enum { ROW_GROUP_BODY, ROW_GROUP_HEADER, ROW_GROUP_FOOTER } row_group_kind;

static row_group_kind
node_row_group(const ns_node *n, GHashTable *styles)
{
    if (ns_node_is_element_named(n, "thead")) return ROW_GROUP_HEADER;
    if (ns_node_is_element_named(n, "tfoot")) return ROW_GROUP_FOOTER;
    const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
    ns_display d = ns_css_display_of(s);
    if (ns_display_is(d, NS_DISPLAY_INTERNAL_TABLE_HEADER_GROUP))
        return ROW_GROUP_HEADER;
    if (ns_display_is(d, NS_DISPLAY_INTERNAL_TABLE_FOOTER_GROUP))
        return ROW_GROUP_FOOTER;
    return ROW_GROUP_BODY;
}

static void
collect_rows(const ns_node *table, GHashTable *styles, GPtrArray *out)
{
    if (!table) return;
    GPtrArray *header = g_ptr_array_new();
    GPtrArray *body   = g_ptr_array_new();
    GPtrArray *footer = g_ptr_array_new();
    for (const ns_node *c = table->first_child; c; c = c->next_sibling) {
        row_group_kind g = node_row_group(c, styles);
        GPtrArray *dst = g == ROW_GROUP_HEADER ? header
                       : g == ROW_GROUP_FOOTER ? footer
                                               : body;
        collect_rows_recurse(c, styles, dst, 0);
    }
    for (guint i = 0; i < header->len; i++)
        g_ptr_array_add(out, g_ptr_array_index(header, i));
    for (guint i = 0; i < body->len; i++)
        g_ptr_array_add(out, g_ptr_array_index(body, i));
    for (guint i = 0; i < footer->len; i++)
        g_ptr_array_add(out, g_ptr_array_index(footer, i));
    g_ptr_array_free(header, TRUE);
    g_ptr_array_free(body, TRUE);
    g_ptr_array_free(footer, TRUE);
}

static ns_box *build_block(const ns_node *n, GHashTable *styles);
static ns_box *build_inline_run(const ns_node *first, const ns_node *last_excl, GHashTable *styles);
static ns_box *build_inline_run_no_abs_placeholders(const ns_node *first, const ns_node *last_excl, GHashTable *styles);
static ns_box *build_pseudo_inline_for(const ns_style *ps, const ns_node *host);
static ns_box *build_pseudo_block_for(const ns_style *ps, const ns_node *host);
static void register_abs_pseudo(const ns_node *host, const ns_style *ps);
static ns_box *build_blockified_inline_item(const ns_node *n, GHashTable *styles,
                                            ns_box **pending_before);
static void append_display_contents_children(ns_box *block, const ns_node *n,
                                             GHashTable *styles,
                                             gboolean blockify_children,
                                             ns_box **pending_before);

static gboolean
button_has_replaced_child_depth(const ns_node *n, int depth)
{
    if (depth >= NS_LAYOUT_MAX_DEPTH) return FALSE;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            (strcmp(c->name, "svg") == 0 || strcmp(c->name, "img") == 0 ||
             strcmp(c->name, "audio") == 0 ||
             strcmp(c->name, "video") == 0))
            return TRUE;
        if (c->kind == NS_NODE_ELEMENT &&
            button_has_replaced_child_depth(c, depth + 1))
            return TRUE;
    }
    return FALSE;
}

static gboolean
button_has_replaced_child(const ns_node *n)
{
    return button_has_replaced_child_depth(n, 0);
}

static gboolean
style_has_atomic_inline_box(const ns_style *s)
{
    if (!s) return FALSE;
    const ns_css_value *w = s->values[NS_CSS_WIDTH];
    const ns_css_value *h = s->values[NS_CSS_HEIGHT];
    gboolean has_w = w && (w->kind == NS_CSS_V_LENGTH || w->kind == NS_CSS_V_CALC);
    gboolean has_h = h && (h->kind == NS_CSS_V_LENGTH || h->kind == NS_CSS_V_CALC);
    if (has_w || has_h) return TRUE;
    if (s->values[NS_CSS_BACKGROUND_IMAGE] &&
        (s->values[NS_CSS_BACKGROUND_IMAGE]->kind == NS_CSS_V_URL ||
         s->values[NS_CSS_BACKGROUND_IMAGE]->kind == NS_CSS_V_GRADIENT))
        return TRUE;
    if (s->values[NS_CSS_BORDER_RADIUS] ||
        s->values[NS_CSS_BORDER_TOP_LEFT_RADIUS] ||
        s->values[NS_CSS_BORDER_TOP_RIGHT_RADIUS] ||
        s->values[NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS] ||
        s->values[NS_CSS_BORDER_BOTTOM_LEFT_RADIUS])
        return TRUE;
    const ns_css_prop widths[4] = {
        NS_CSS_BORDER_TOP_WIDTH,
        NS_CSS_BORDER_RIGHT_WIDTH,
        NS_CSS_BORDER_BOTTOM_WIDTH,
        NS_CSS_BORDER_LEFT_WIDTH,
    };
    const ns_css_prop styles_p[4] = {
        NS_CSS_BORDER_TOP_STYLE,
        NS_CSS_BORDER_RIGHT_STYLE,
        NS_CSS_BORDER_BOTTOM_STYLE,
        NS_CSS_BORDER_LEFT_STYLE,
    };
    for (int i = 0; i < 4; i++) {
        if (length_or(s->values[widths[i]], 0) <= 0) continue;
        const ns_css_value *st = s->values[styles_p[i]];
        if (!keyword_is(st, "none") && !keyword_is(st, "hidden"))
            return TRUE;
    }
    const ns_css_value *bg = s->values[NS_CSS_BACKGROUND_COLOR];
    return bg && bg->kind == NS_CSS_V_COLOR && bg->u.color.a > 0;
}

static gboolean
is_atomic_inline(const ns_node *n, GHashTable *styles)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    const char *nm = n->name;
    if (strcmp(nm, "button") == 0) {
        const ns_style *bs = styles ? g_hash_table_lookup(styles, n) : NULL;
        if (display_is_atomic_inline_container(ns_css_display_of(bs)) ||
            style_has_atomic_inline_box(bs))
            return TRUE;
        if (button_has_replaced_child(n)) return TRUE;
        const ns_css_value *bw = bs ? bs->values[NS_CSS_WIDTH]  : NULL;
        const ns_css_value *bh = bs ? bs->values[NS_CSS_HEIGHT] : NULL;
        return (bw && (bw->kind == NS_CSS_V_LENGTH || bw->kind == NS_CSS_V_CALC)) ||
               (bh && (bh->kind == NS_CSS_V_LENGTH || bh->kind == NS_CSS_V_CALC));
    }
    if (strcmp(nm, "a") == 0 || strcmp(nm, "label") == 0 ||
        strcmp(nm, "summary") == 0) {
        const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
        ns_display d = ns_css_display_of(s);
        return display_is_atomic_inline_container(d) ||
               ns_display_is_flex_container(d) ||
               ns_display_is_grid_container(d);
    }
    if (strcmp(nm, "img") == 0 || strcmp(nm, "svg") == 0 ||
        strcmp(nm, "audio") == 0 || strcmp(nm, "video") == 0 ||
        strcmp(nm, "math") == 0 || strcmp(nm, "canvas") == 0)
        return is_inline_level_replaced(n, styles);
    if (strcmp(nm, "input") == 0 || strcmp(nm, "textarea") == 0 ||
        strcmp(nm, "select") == 0 ||
        strcmp(nm, "progress") == 0 || strcmp(nm, "meter") == 0) {
        const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
        gboolean atomic_display =
            display_is_atomic_inline_container(ns_css_display_of(s));
        if (atomic_display && strcmp(nm, "input") == 0) {
            const char *type = ns_element_get_attr(n, "type");
            if (type && (g_ascii_strcasecmp(type, "radio") == 0 ||
                         g_ascii_strcasecmp(type, "checkbox") == 0)) {
                const ns_css_value *ap = s ? s->values[NS_CSS_APPEARANCE] : NULL;
                const ns_css_value *w = s ? s->values[NS_CSS_WIDTH]  : NULL;
                const ns_css_value *h = s ? s->values[NS_CSS_HEIGHT] : NULL;
                gboolean styled =
                    keyword_is(ap, "none") ||
                    (w && (w->kind == NS_CSS_V_LENGTH || w->kind == NS_CSS_V_CALC)) ||
                    (h && (h->kind == NS_CSS_V_LENGTH || h->kind == NS_CSS_V_CALC));
                if (!styled) return FALSE;
            }
        }
        return atomic_display;
    }
    if (strcmp(nm, "br") == 0 || strcmp(nm, "wbr") == 0)
        return FALSE;
    const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
    if (!s) return FALSE;
    return display_is_atomic_inline_container(ns_css_display_of(s));
}
static const ns_node *g_focused_input_for_layout;
static const ns_node *g_open_select_for_layout;
static gboolean       g_focused_is_contenteditable_for_layout;
static gsize          g_focused_caret_byte_for_layout;
static gsize          g_focused_sel_anchor_byte_for_layout;
static gboolean       g_datalist_open_for_layout;

void
ns_layout_set_open_select(const ns_node *select)
{
    g_open_select_for_layout = select;
}

void
ns_layout_set_datalist_open(gboolean open)
{
    g_datalist_open_for_layout = open;
}

char *
ns_vertical_stack_text(const char *text)
{
    if (!text) return g_strdup("");
    GString *out = g_string_new(NULL);
    for (const char *p = text; *p; ) {
        const char *next = g_utf8_next_char(p);
        if (out->len) g_string_append_c(out, '\n');
        g_string_append_len(out, p, (gssize)(next - p));
        p = next;
    }
    return g_string_free(out, FALSE);
}
static struct ns_image_cache *g_image_cache_for_layout;
static const char    *g_base_url_for_layout;
static gboolean       g_svg_defs_computed_for_layout;
static ns_box *ns_layout_build_(const ns_node *doc, GHashTable *styles, double viewport_width);

typedef struct ns_table_col_hint {
    const ns_style *style;
    int span;
} ns_table_col_hint;

typedef struct ns_abs_entry {
    const ns_node *dom;
    const ns_style *pseudo;
    gboolean       fixed;
} ns_abs_entry;

typedef struct ns_abs_static {
    ns_box *run;
    double  rel_x;
    double  rel_y;
} ns_abs_static;

enum {
    NS_LAYOUT_DATA_IMAGE_BUDGET = 64ULL * 1024ULL * 1024ULL,
};

static GArray      *g_abs_pending;
static gboolean     g_abs_force_build;
static const ns_node *g_form_control_inline;
static GHashTable  *g_abs_ph_set;
static GHashTable  *g_abs_static;
static GHashTable  *g_abs_seen;

static void
queue_absolute_node(const ns_node *n, const ns_style *s)
{
    if (!g_abs_pending ||
        (g_abs_seen && !g_hash_table_add(g_abs_seen, (gpointer)n)))
        return;
    const ns_css_value *pv = s->values[NS_CSS_POSITION];
    ns_abs_entry e;
    e.dom = n;
    e.pseudo = NULL;
    e.fixed = pv && pv->kind == NS_CSS_V_KEYWORD && pv->u.keyword &&
              strcmp(pv->u.keyword, "fixed") == 0;
    g_array_append_val(g_abs_pending, e);
}
static const ns_node *g_inline_skip_node;
static const ns_node *g_pseudo_blocks_host;
static gboolean g_pseudo_block_before;
static gboolean g_pseudo_block_after;
static int          g_inline_collect_depth;

static void *
collect_peek_image(const char *src)
{
    if (!src || !g_image_cache_for_layout) return NULL;
    char *abs = g_base_url_for_layout
        ? ns_url_resolve(g_base_url_for_layout, src)
        : NULL;
    void *img = ns_image_cache_peek(g_image_cache_for_layout,
                                    abs ? abs : src);
    g_free(abs);
    return img;
}

void *
ns_layout_peek_image(const char *src)
{
    return collect_peek_image(src);
}

static void
collect_box_bg_image(ns_box *box, const ns_style *s)
{
    const ns_css_value *mi = s ? s->values[NS_CSS_LIST_STYLE_IMAGE] : NULL;
    if (mi && mi->kind == NS_CSS_V_URL && mi->u.url &&
        box->dom && ns_node_is_element_named(box->dom, "li")) {
        ns_box_media *m = ns_box_media_ensure(box);
        m->marker_image_src = g_strdup(mi->u.url);
        m->marker_image = collect_peek_image(m->marker_image_src);
    }
    const ns_css_value *bi = ns_css_border_image_source(s);
    if (bi && bi->kind == NS_CSS_V_URL && bi->u.url) {
        ns_box_media *m = ns_box_media_ensure(box);
        m->border_image_src = g_strdup(bi->u.url);
        m->border_image = collect_peek_image(m->border_image_src);
    }
    const ns_css_value *bg = s ? s->values[NS_CSS_BACKGROUND_IMAGE] : NULL;
    gboolean any_url = FALSE;
    for (const ns_css_value *l = bg; l; l = l->next_layer)
        if (l->kind == NS_CSS_V_URL && l->u.url) { any_url = TRUE; break; }
    const ns_css_value *mask =
        (s && s->values[NS_CSS_MASK_IMAGE] &&
         s->values[NS_CSS_MASK_IMAGE]->kind == NS_CSS_V_URL &&
         s->values[NS_CSS_MASK_IMAGE]->u.url)
            ? s->values[NS_CSS_MASK_IMAGE]
            : NULL;
    if (!any_url && !mask) return;
    ns_box_media *m = ns_box_media_ensure(box);
    if (!any_url) {
        m->bg_image_src = g_strdup(mask->u.url);
        m->bg_image = collect_peek_image(m->bg_image_src);
        return;
    }
    if (bg->next_layer) {
        m->bg_layer_srcs = g_ptr_array_new_with_free_func(g_free);
        m->bg_layer_images = g_ptr_array_new();
        for (const ns_css_value *l = bg; l; l = l->next_layer) {
            char *src = (l->kind == NS_CSS_V_URL && l->u.url)
                        ? g_strdup(l->u.url) : NULL;
            g_ptr_array_add(m->bg_layer_srcs, src);
            g_ptr_array_add(m->bg_layer_images, collect_peek_image(src));
        }
    }
    for (const ns_css_value *l = bg; l; l = l->next_layer) {
        if (l->kind != NS_CSS_V_URL || !l->u.url) continue;
        m->bg_image_src = g_strdup(l->u.url);
        m->bg_image = collect_peek_image(m->bg_image_src);
        break;
    }
}

static void
inline_append_attrs(ns_box *last, const ns_box *gen, gsize offset)
{
    if (!gen->attrs || !gen->attrs->len) return;
    if (!last->attrs)
        last->attrs = g_array_new(FALSE, FALSE, sizeof(ns_inline_attr));
    for (guint i = 0; i < gen->attrs->len; i++) {
        ns_inline_attr a = g_array_index(gen->attrs, ns_inline_attr, i);
        a.start += offset;
        g_array_append_val(last->attrs, a);
    }
}

static void
inline_take_atomics(ns_box *last, ns_box *gen, gsize offset)
{
    if (!gen->inline_atomics) return;
    if (!last->inline_atomics)
        last->inline_atomics = g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
    for (guint i = 0; i < gen->inline_atomics->len; i++) {
        ns_inline_atomic ia = g_array_index(gen->inline_atomics,
                                            ns_inline_atomic, i);
        ia.byte_off += offset;
        if (ia.box) ia.box->parent = last;
        g_array_append_val(last->inline_atomics, ia);
    }
    g_array_free(gen->inline_atomics, TRUE);
    gen->inline_atomics = NULL;
}

static void
append_generated_after(ns_box *block, ns_box *gen)
{
    ns_box *last = block->first_child;
    while (last && last->next_sibling) last = last->next_sibling;
    if (!last || last->kind != NS_BOX_INLINE) {
        box_append_child(block, gen);
        return;
    }
    gsize ll = last->text ? strlen(last->text) : 0;
    gsize gl = gen->text  ? strlen(gen->text)  : 0;
    if (ll > G_MAXSIZE - gl - 1) { ns_box_free(gen); return; }
    char *combined = g_malloc(ll + gl + 1);
    if (ll) memcpy(combined, last->text, ll);
    if (gl) memcpy(combined + ll, gen->text, gl);
    combined[ll + gl] = '\0';
    g_free(last->text);
    last->text = combined;
    inline_append_attrs(last, gen, ll);
    inline_take_atomics(last, gen, ll);
    ns_box_free(gen);
}

static ns_box *
cell_pseudo_before(ns_box *cell, const ns_node *n, const ns_style *s)
{
    if (!s || !s->before) return NULL;
    ns_box *before_block = build_pseudo_block_for(s->before, n);
    if (before_block) {
        box_append_child(cell, before_block);
        return NULL;
    }
    if (style_is_absolute_or_fixed(s->before)) return NULL;
    return build_pseudo_inline_for(s->before, n);
}

static void
cell_pseudo_after(ns_box *cell, const ns_node *n, const ns_style *s)
{
    if (!s || !s->after) return;
    ns_box *after_block = build_pseudo_block_for(s->after, n);
    if (after_block) {
        box_append_child(cell, after_block);
        return;
    }
    if (style_is_absolute_or_fixed(s->after)) return;
    ns_box *gen = build_pseudo_inline_for(s->after, n);
    if (gen) append_generated_after(cell, gen);
}

static void
cell_append_run(ns_box *cell, ns_box *run, ns_box **pending_before)
{
    if (*pending_before) {
        run = inline_merge_prefix(*pending_before, run);
        *pending_before = NULL;
    }
    if (run->text && run->text[0] != '\0')
        box_append_child(cell, run);
    else
        ns_box_free(run);
}

static void
cell_flush_pending(ns_box *cell, ns_box **pending_before)
{
    if (!*pending_before) return;
    box_append_child(cell, *pending_before);
    *pending_before = NULL;
}

static ns_box *
build_cell(const ns_node *n, GHashTable *styles)
{
    ns_box *cell = box_new(NS_BOX_TABLE_CELL);
    cell->dom = n;
    cell->style = g_hash_table_lookup(styles, n);
    collect_box_bg_image(cell, cell->style);
    const char *cs_attr = ns_element_get_attr(n, "colspan");
    if (cs_attr) cell->colspan = ns_parse_int(cs_attr, 1, 1, 100);
    const char *rs_attr = ns_element_get_attr(n, "rowspan");
    if (rs_attr) cell->rowspan = ns_parse_int(rs_attr, 1, 1, 100);
    const ns_style *s = cell->style;
    if (s) {
        register_abs_pseudo(n, s->before);
        register_abs_pseudo(n, s->after);
    }
    ns_box *pending_before = cell_pseudo_before(cell, n, s);
    const ns_node *c = n->first_child;
    while (c) {
        if (is_inline_dom(c, styles)) {
            const ns_node *start = c;
            while (c && continues_inline_run(c, styles)) c = c->next_sibling;
            cell_append_run(cell, build_inline_run(start, c, styles),
                            &pending_before);
        } else {
            cell_flush_pending(cell, &pending_before);
            ns_box *child = build_block(c, styles);
            if (child) box_append_child(cell, child);
            if (c) c = c->next_sibling;
        }
    }
    cell_flush_pending(cell, &pending_before);
    cell_pseudo_after(cell, n, s);
    return cell;
}

static void
append_table_col_hint(ns_box *table, const ns_node *n, GHashTable *styles,
                      int span, const ns_style *fallback)
{
    if (span < 1) span = 1;
    if (!table->table_col_hints)
        table->table_col_hints = g_array_new(FALSE, FALSE,
                                             sizeof(ns_table_col_hint));
    const ns_style *s = styles ? g_hash_table_lookup(styles, n) : NULL;
    if (!s) s = fallback;
    ns_table_col_hint hint = { .style = s, .span = span };
    g_array_append_val(table->table_col_hints, hint);
}

static void
collect_table_col_hints(ns_box *table, const ns_node *n, GHashTable *styles)
{
    if (!table || !n) return;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
        if (strcmp(c->name, "col") == 0) {
            int span = ns_parse_int(ns_element_get_attr(c, "span"), 1, 1, 1000);
            append_table_col_hint(table, c, styles, span, NULL);
            continue;
        }
        if (strcmp(c->name, "colgroup") != 0) continue;
        const ns_style *group_style = styles ? g_hash_table_lookup(styles, c) : NULL;
        gboolean saw_col = FALSE;
        for (const ns_node *col = c->first_child; col; col = col->next_sibling) {
            if (!ns_node_is_element_named(col, "col")) continue;
            int span = ns_parse_int(ns_element_get_attr(col, "span"), 1, 1, 1000);
            append_table_col_hint(table, col, styles, span, group_style);
            saw_col = TRUE;
        }
        if (!saw_col) {
            int span = ns_parse_int(ns_element_get_attr(c, "span"), 1, 1, 1000);
            append_table_col_hint(table, c, styles, span, NULL);
        }
    }
}

static ns_box *
build_table_caption(const ns_node *n, GHashTable *styles)
{
    ns_box *caption = box_new(NS_BOX_TABLE_CAPTION);
    caption->dom = n;
    caption->style = g_hash_table_lookup(styles, n);
    collect_box_bg_image(caption, caption->style);
    append_display_contents_children(caption, n, styles, FALSE, NULL);
    return caption;
}

static ns_box *
build_anonymous_table_cell(const ns_node *n, GHashTable *styles)
{
    ns_box *cell = box_new(NS_BOX_TABLE_CELL);
    gboolean any = FALSE;
    const ns_node *c = n ? n->first_child : NULL;
    while (c) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            ns_layout_tag_is_non_rendering(c->name)) {
            c = c->next_sibling;
            continue;
        }
        if (is_table_caption(c, styles)) {
            c = c->next_sibling;
            continue;
        }
        if (c->kind == NS_NODE_TEXT && text_is_ws_only(c->text)) {
            c = c->next_sibling;
            continue;
        }
        if (c->kind == NS_NODE_ELEMENT) {
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (cs && style_is_none(cs)) {
                c = c->next_sibling;
                continue;
            }
            if (style_is_contents(cs)) {
                append_display_contents_children(cell, c, styles, FALSE, NULL);
                if (cell->last_child) any = TRUE;
                c = c->next_sibling;
                continue;
            }
        }
        if (is_inline_dom(c, styles)) {
            const ns_node *start = c;
            c = c->next_sibling;
            while (c && !is_table_caption(c, styles)) {
                if (c->kind == NS_NODE_ELEMENT && c->name &&
                    ns_layout_tag_is_non_rendering(c->name)) {
                    c = c->next_sibling;
                    continue;
                }
                if (c->kind == NS_NODE_ELEMENT) {
                    const ns_style *cs = g_hash_table_lookup(styles, c);
                    if (style_is_contents(cs)) break;
                }
                if (!continues_inline_run(c, styles)) break;
                c = c->next_sibling;
            }
            ns_box *run = build_inline_run(start, c, styles);
            if (run && run->text && run->text[0]) {
                box_append_child(cell, run);
                any = TRUE;
            } else if (run) {
                ns_box_free(run);
            }
        } else {
            ns_box *child = build_block(c, styles);
            if (child) {
                box_append_child(cell, child);
                any = TRUE;
            }
            if (c) c = c->next_sibling;
        }
    }
    if (!any) {
        ns_box_free(cell);
        return NULL;
    }
    return cell;
}

static ns_box *
build_table_row(const ns_node *tr, GHashTable *styles)
{
    ns_box *row = box_new(NS_BOX_TABLE_ROW);
    row->dom = tr;
    row->style = g_hash_table_lookup(styles, tr);
    for (const ns_node *c = tr->first_child; c; c = c->next_sibling) {
        if (!is_cell_element(c, styles)) continue;
        if (style_is_none(g_hash_table_lookup(styles, c))) continue;
        box_append_child(row, build_cell(c, styles));
    }
    return row;
}

static ns_box *
build_row_from_cell_run(const ns_node *start, const ns_node *end,
                        const ns_node *dom, GHashTable *styles)
{
    ns_box *row = box_new(NS_BOX_TABLE_ROW);
    row->dom = dom;
    for (const ns_node *c = start; c != end; c = c->next_sibling) {
        if (!is_cell_element(c, styles)) continue;
        const ns_style *cs = g_hash_table_lookup(styles, c);
        if (cs && (style_is_none(cs) || style_is_absolute_or_fixed(cs)))
            continue;
        box_append_child(row, build_cell(c, styles));
    }
    if (row->first_child) return row;
    ns_box_free(row);
    return NULL;
}

static ns_box *
build_table(const ns_node *n, GHashTable *styles)
{
    ns_box *table = box_new(NS_BOX_TABLE);
    table->dom = n;
    table->style = g_hash_table_lookup(styles, n);
    collect_table_col_hints(table, n, styles);
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (!is_table_caption(c, styles)) continue;
        ns_box *caption = build_table_caption(c, styles);
        box_append_child(table, caption);
    }
    GPtrArray *rows = g_ptr_array_new();
    collect_rows(n, styles, rows);
    gboolean has_direct_cells = FALSE;
    if (rows->len == 0)
        for (const ns_node *c = n->first_child; c; c = c->next_sibling)
            if (is_cell_element(c, styles)) { has_direct_cells = TRUE; break; }
    if (rows->len == 0 && has_direct_cells) {
        ns_box *row = build_row_from_cell_run(n->first_child, NULL, n, styles);
        if (row) box_append_child(table, row);
    } else if (rows->len == 0) {
        ns_box *cell = build_anonymous_table_cell(n, styles);
        if (cell) {
            ns_box *row = box_new(NS_BOX_TABLE_ROW);
            row->dom = n;
            box_append_child(row, cell);
            box_append_child(table, row);
        }
    } else {
        for (guint i = 0; i < rows->len; i++)
            box_append_child(table,
                             build_table_row(g_ptr_array_index(rows, i),
                                             styles));
    }
    g_ptr_array_free(rows, TRUE);
    return table;
}

static ns_box *
build_anonymous_table(const ns_node *start, const ns_node *end,
                      GHashTable *styles)
{
    ns_box *table = box_new(NS_BOX_TABLE);
    for (const ns_node *c = start; c != end; c = c->next_sibling)
        if (is_table_caption(c, styles))
            box_append_child(table, build_table_caption(c, styles));

    const ns_node *cells = NULL;
    for (const ns_node *c = start; c != end; c = c->next_sibling) {
        if (is_cell_element(c, styles)) {
            if (!cells) cells = c;
            continue;
        }
        if (cells) {
            ns_box *row = build_row_from_cell_run(cells, c, NULL, styles);
            if (row) box_append_child(table, row);
            cells = NULL;
        }
        if (is_table_row(c, styles))
            box_append_child(table, build_table_row(c, styles));
        else if (node_is_row_group(c, styles))
            for (const ns_node *r = c->first_child; r; r = r->next_sibling)
                if (is_table_row(r, styles))
                    box_append_child(table, build_table_row(r, styles));
    }
    if (cells) {
        ns_box *row = build_row_from_cell_run(cells, end, NULL, styles);
        if (row) box_append_child(table, row);
    }

    if (table->first_child) return table;
    ns_box_free(table);
    return NULL;
}

typedef struct collector_ctx {
    GHashTable *styles;
    const char *active_href;
    const char *active_target;
    const ns_node *active_link_node;
    GString    *out;
    GArray     *links;
    GArray     *attrs;
    int  bold_depth;
    int  italic_depth;
    int  mono_depth;
    int  underline_depth;
    int  overline_depth;
    int  strike_depth;
    int  q_depth;
    gsize bold_start;
    gsize italic_start;
    gsize mono_start;
    gsize underline_start;
    gsize overline_start;
    gsize strike_start;
    const char *text_transform;
    GArray     *atomics;
    gboolean    abs_placeholders;
    GArray     *ws_ranges;
    const ns_node *ws_parent;
    int         ws_parent_mode;
} collector_ctx;

typedef struct ns_ws_range {
    gsize start, end;
    int mode;
} ns_ws_range;

typedef struct ns_atomic_raw {
    gsize start;
    ns_box *box;
} ns_atomic_raw;

static int white_space_mode(const ns_node *node, GHashTable *styles);

static void
collector_note_ws(collector_ctx *ctx, const ns_node *parent, gsize start)
{
    if (!ctx->ws_ranges || !parent || ctx->out->len <= start) return;
    if (parent != ctx->ws_parent) {
        ctx->ws_parent = parent;
        ctx->ws_parent_mode = white_space_mode(parent, ctx->styles);
    }
    ns_ws_range r = { .start = start, .end = ctx->out->len,
                      .mode = ctx->ws_parent_mode };
    g_array_append_val(ctx->ws_ranges, r);
}

static double
inline_box_side_px(const ns_style *s, ns_css_prop padding,
                   ns_css_prop border_width, ns_css_prop border_style)
{
    if (!s) return 0;
    double side = length_or(s->values[padding], 0);
    if (s->values[border_style])
        side += border_side_width(s, border_width, border_style);
    return side;
}

static ns_box *pseudo_block_with_text(const ns_style *ps, const char *txt);

static void
append_pseudo_inline(collector_ctx *ctx, const ns_style *ps,
                     const ns_node *host)
{
    if (!ns_layout_pseudo_generates_box(ps) || style_is_none(ps)) return;
    if (style_is_absolute_or_fixed(ps)) {
        ns_layout_append_pseudo_content(ctx->out, ps->values[NS_CSS_CONTENT],
                                        host, ps);
        return;
    }
    if (ctx->atomics && ns_display_is_atomic_inline(ns_css_display_of(ps))) {
        char *txt = ns_layout_resolve_pseudo_content(
            ps->values[NS_CSS_CONTENT]->u.keyword, host);
        ns_atomic_raw rec = {
            .start = ctx->out->len,
            .box = pseudo_block_with_text(ps, txt),
        };
        g_free(txt);
        g_string_append(ctx->out, "\xef\xbf\xbc");
        g_array_append_val(ctx->atomics, rec);
        return;
    }
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs,
                                   length_or(ps->values[NS_CSS_MARGIN_LEFT], 0));
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs,
                                   length_or(ps->values[NS_CSS_PADDING_LEFT], 0));
    ns_layout_append_pseudo_content(ctx->out, ps->values[NS_CSS_CONTENT], host, ps);
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs,
                                   length_or(ps->values[NS_CSS_PADDING_RIGHT], 0));
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs,
                                   length_or(ps->values[NS_CSS_MARGIN_RIGHT], 0));
}

static void
collect_walk(const ns_node *n, collector_ctx *ctx, int depth)
{
    if (!n || depth >= NS_LAYOUT_MAX_DEPTH) return;
    if (node_is_frame_fallback(n)) return;
    if (n->kind == NS_NODE_TEXT) {
        if (!n->text) return;
        gsize start = ctx->out->len;
        if (n->parent && ns_node_is_contenteditable_host(n->parent)) {
            gboolean focused = g_focused_is_contenteditable_for_layout &&
                               n->parent == g_focused_input_for_layout;
            const char *val = n->text;
            gsize vlen = strlen(val);
            gsize cb = g_focused_caret_byte_for_layout;
            gsize ab = g_focused_sel_anchor_byte_for_layout;
            if (cb > vlen) cb = vlen;
            if (ab > vlen) ab = vlen;
            gsize caret_pos = start, anchor_pos = start;
            for (gsize i = 0; i <= vlen; i++) {
                if (i == cb) caret_pos = ctx->out->len;
                if (i == ab) anchor_pos = ctx->out->len;
                if (i == vlen) break;
                char ch = val[i];
                if (ch == '\n') g_string_append(ctx->out, "\xe2\x80\xa8");
                else if (ch != '\r') g_string_append_c(ctx->out, ch);
            }
            if (ctx->active_href) {
                ns_link_range r = {
                    .start = start, .len = ctx->out->len - start,
                    .href  = g_strdup(ctx->active_href),
                    .target = ctx->active_target ? g_strdup(ctx->active_target) : NULL,
                    .dom   = ctx->active_link_node,
                };
                g_array_append_val(ctx->links, r);
            }
            if (focused) {
                g_string_append(ctx->out, "\xc2\xa0");
                if (anchor_pos != caret_pos) {
                    gsize s0 = anchor_pos < caret_pos ? anchor_pos : caret_pos;
                    gsize s1 = anchor_pos < caret_pos ? caret_pos : anchor_pos;
                    ns_layout_emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
                }
                ns_layout_emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
            }
            return;
        }
        char *xformed = ctx->text_transform
                        ? ns_layout_apply_text_transform(n->text, ctx->text_transform)
                        : NULL;
        g_string_append(ctx->out, xformed ? xformed : n->text);
        g_free(xformed);
        collector_note_ws(ctx, n->parent, start);
        if (ctx->active_href) {
            ns_link_range r = {
                .start = start,
                .len   = ctx->out->len - start,
                .href  = g_strdup(ctx->active_href),
                .target = ctx->active_target ? g_strdup(ctx->active_target) : NULL,
                .dom   = ctx->active_link_node,
            };
            g_array_append_val(ctx->links, r);
        }
        return;
    }
    if (n->kind != NS_NODE_ELEMENT) return;
    if (ns_layout_node_is_non_rendering(n)) return;
    if (n == g_inline_skip_node) return;
    const ns_style *s = g_hash_table_lookup(ctx->styles, n);
    if (s && style_is_none(s)) return;
    if (s && style_is_absolute_or_fixed(s)) {
        const ns_css_value *pv = s->values[NS_CSS_POSITION];
        gboolean fixed = pv && pv->kind == NS_CSS_V_KEYWORD && pv->u.keyword &&
                         strcmp(pv->u.keyword, "fixed") == 0;
        if (g_abs_pending &&
            (!g_abs_seen || g_hash_table_add(g_abs_seen, (gpointer)n))) {
            ns_abs_entry e;
            e.dom = n;
            e.pseudo = NULL;
            e.fixed = fixed;
            g_array_append_val(g_abs_pending, e);
        }
        if (ctx->atomics && !fixed && ctx->abs_placeholders && g_abs_ph_set) {
            ns_box *ph = box_new(NS_BOX_BLOCK);
            ns_atomic_raw rec = { .start = ctx->out->len, .box = ph };
            g_string_append(ctx->out, "\xef\xbf\xbc");
            g_array_append_val(ctx->atomics, rec);
            g_hash_table_insert(g_abs_ph_set, ph, (gpointer)n);
        }
        return;
    }

    if (ctx->atomics && n != g_form_control_inline &&
        is_atomic_inline(n, ctx->styles)) {
        int saved_collect_depth = g_inline_collect_depth;
        g_inline_collect_depth = depth + 1;
        ns_box *sub = build_block(n, ctx->styles);
        g_inline_collect_depth = saved_collect_depth;
        if (sub) {
            ns_atomic_raw rec = { .start = ctx->out->len, .box = sub };
            g_string_append(ctx->out, "\xef\xbf\xbc");
            g_array_append_val(ctx->atomics, rec);
        }
        return;
    }

    if (!n->name) {
        for (const ns_node *c = n->first_child; c; c = c->next_sibling)
            collect_walk(c, ctx, depth + 1);
        return;
    }
    if (strcmp(n->name, "br") == 0) {
        g_string_append(ctx->out, "\xe2\x80\xa8");
        return;
    }
    if (strcmp(n->name, "wbr") == 0) {
        g_string_append(ctx->out, "\xe2\x80\x8b");
        return;
    }
    if (strcmp(n->name, "progress") == 0 || strcmp(n->name, "meter") == 0) {
        gboolean is_meter = strcmp(n->name, "meter") == 0;
        gsize start = ctx->out->len;
        for (int i = 0; i < 12; i++) g_string_append(ctx->out, "\xc2\xa0");
        double frac = 0;
        guint8 r = 0, g = 0, b = 0, alpha = 0;
        if (is_meter) {
            int quality = 0;
            frac = ns_layout_meter_fraction(n, &quality);
            if (quality == 0) { r = 0x2e; g = 0x9d; b = 0x54; }
            else if (quality == 1) { r = 0xd0; g = 0xa4; b = 0x1f; }
            else { r = 0xc4; g = 0x43; b = 0x3c; }
            alpha = 255;
        } else {
            frac = ns_layout_progress_fraction(n);
        }
        ns_inline_attr attr = {
            .kind = is_meter ? NS_INLINE_METER : NS_INLINE_PROGRESS,
            .start = start, .len = ctx->out->len - start,
            .font_size_px = frac,
            .r = r, .g = g, .b = b, .a = alpha,
            .dom = n,
        };
        g_array_append_val(ctx->attrs, attr);
        return;
    }
    if (strcmp(n->name, "input") == 0) {
        const char *type = ns_element_get_attr(n, "type");
        gboolean is_password = type && g_ascii_strcasecmp(type, "password") == 0;
        gboolean is_text = !type || !*type ||
                           is_password ||
                           g_ascii_strcasecmp(type, "text") == 0 ||
                           g_ascii_strcasecmp(type, "search") == 0 ||
                           g_ascii_strcasecmp(type, "email") == 0 ||
                           g_ascii_strcasecmp(type, "url") == 0 ||
                           g_ascii_strcasecmp(type, "tel") == 0 ||
                           g_ascii_strcasecmp(type, "number") == 0;
        if (is_text) {
            if (!ns_layout_inline_run_at_line_start(ctx->out))
                g_string_append(ctx->out, "\xc2\xa0\xc2\xa0\xc2\xa0");
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            int leading = ns_layout_text_input_leading_spaces(s);
            for (int i = 0; i < leading; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            const char *real_value = ns_input_used_value(n);
            const char *v = real_value;
            gboolean is_placeholder = FALSE;
            if (!v || !*v) {
                v = ns_element_get_attr(n, "placeholder");
                is_placeholder = (v && *v);
            }
            gboolean focused = (n == g_focused_input_for_layout);
            gsize val_start = ctx->out->len;
            gsize disp_end = val_start;
            gsize caret_pos = val_start;
            gsize anchor_pos = val_start;
            gsize caret_byte = g_focused_caret_byte_for_layout;
            gsize anchor_byte = g_focused_sel_anchor_byte_for_layout;
            if (real_value && caret_byte > strlen(real_value))
                caret_byte = strlen(real_value);
            if (real_value && anchor_byte > strlen(real_value))
                anchor_byte = strlen(real_value);
            else if (!real_value) anchor_byte = 0;
            int size = ns_layout_text_input_columns(n);
            glong displayed_chars = 0;
            if (v && *v && is_password && !is_placeholder) {
                glong cps = g_utf8_strlen(v, -1);
                glong skip_cps = focused
                    ? ns_layout_text_input_focused_skip_cps(real_value, caret_byte,
                                                  cps, size)
                    : ns_layout_text_input_align_skip_cps(s, n, cps, size);
                glong shown_cps = cps - skip_cps;
                if (shown_cps > size) shown_cps = size;
                for (glong i = 0; i < shown_cps; i++)
                    g_string_append(ctx->out, "\xe2\x80\xa2");
                displayed_chars = shown_cps;
                disp_end = ctx->out->len;
                if (focused) {
                    glong cp_before = real_value
                        ? g_utf8_pointer_to_offset(real_value, real_value + caret_byte)
                        : 0;
                    glong cp_in_shown = cp_before > skip_cps
                                        ? cp_before - skip_cps : 0;
                    if (cp_in_shown > shown_cps) cp_in_shown = shown_cps;
                    caret_pos = val_start + (gsize)cp_in_shown * 3;
                }
            } else if (v && *v) {
                const char *display_v = v;
                gsize skip_bytes = 0;
                gsize display_bytes = strlen(v);
                glong value_cps = g_utf8_strlen(v, -1);
                if (value_cps > size) {
                    glong skip_cps = focused && real_value && v == real_value
                        ? ns_layout_text_input_focused_skip_cps(real_value, caret_byte,
                                                      value_cps, size)
                        : ns_layout_text_input_align_skip_cps(s, n, value_cps, size);
                    const char *p = ns_layout_text_input_advance_cps(v, skip_cps);
                    const char *q = ns_layout_text_input_advance_cps(p, size);
                    display_v = p;
                    display_bytes = (gsize)(q - p);
                    if (real_value && v == real_value) {
                        skip_bytes = (gsize)(p - real_value);
                    }
                }
                g_string_append_len(ctx->out, display_v, (gssize)display_bytes);
                displayed_chars = g_utf8_strlen(display_v, (gssize)display_bytes);
                disp_end = ctx->out->len;
                if (focused && real_value && *real_value) {
                    gsize caret_in_shown = caret_byte > skip_bytes
                                           ? caret_byte - skip_bytes : 0;
                    if (caret_in_shown > display_bytes) caret_in_shown = display_bytes;
                    caret_pos = val_start + caret_in_shown;
                    gsize anchor_in_shown = anchor_byte > skip_bytes
                                            ? anchor_byte - skip_bytes : 0;
                    if (anchor_in_shown > display_bytes) anchor_in_shown = display_bytes;
                    anchor_pos = val_start + anchor_in_shown;
                } else if (focused) {
                    caret_pos = val_start;
                    anchor_pos = val_start;
                }
            } else {
                if (focused) { caret_pos = val_start; anchor_pos = val_start; }
            }
            glong pad = (glong)size - displayed_chars;
            if (pad < 0) pad = 0;
            glong left_pad = 0, right_pad = pad;
            if (pad > 0) {
                left_pad = ns_layout_text_input_align_offset(s, n, pad);
                right_pad = pad - left_pad;
            }
            if (left_pad > 0) {
                GString *lp = g_string_new(NULL);
                for (glong i = 0; i < left_pad; i++)
                    g_string_append(lp, "\xc2\xa0");
                g_string_insert_len(ctx->out, val_start, lp->str, lp->len);
                gsize shift = (gsize)lp->len;
                g_string_free(lp, TRUE);
                caret_pos += shift;
                anchor_pos += shift;
                disp_end += shift;
                val_start += shift;
            }
            for (glong i = 0; i < right_pad; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, "\xc2\xa0");
            ns_inline_attr_kind kind = focused
                                       ? NS_INLINE_INPUT_FIELD_FOCUSED
                                       : NS_INLINE_INPUT_FIELD;
            ns_layout_emit_form_attr_sized(ctx->attrs, kind, start, ctx->out->len, n, ctx->styles);
            ns_layout_emit_control_text_style(ctx->attrs, s, start, ctx->out->len,
                                    val_start, disp_end, is_placeholder,
                                    ns_element_get_attr(n, "disabled") != NULL);
            if (!is_placeholder && disp_end > val_start &&
                ns_element_get_attr(n, "disabled") == NULL &&
                ns_node_spellcheck_used(n)) {
                ns_inline_attr sc = { .kind = NS_INLINE_SPELLCHECK,
                                      .start = val_start,
                                      .len = disp_end - val_start, .dom = n };
                g_array_append_val(ctx->attrs, sc);
            }
            if (focused) {
                if (anchor_pos != caret_pos) {
                    gsize s0 = anchor_pos < caret_pos ? anchor_pos : caret_pos;
                    gsize s1 = anchor_pos < caret_pos ? caret_pos : anchor_pos;
                    ns_layout_emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
                }
                ns_layout_emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
            }
            if (focused && g_datalist_open_for_layout)
                ns_layout_emit_datalist_suggestions(ctx->out, ctx->attrs, ctx->styles, n);
        } else if (type && (g_ascii_strcasecmp(type, "submit") == 0 ||
                            g_ascii_strcasecmp(type, "button") == 0 ||
                            g_ascii_strcasecmp(type, "reset") == 0)) {
            const char *v = ns_element_get_attr(n, "value");
            gsize start = ctx->out->len;
            if (!v)
                v = g_ascii_strcasecmp(type, "submit") == 0 ? "Submit"
                  : g_ascii_strcasecmp(type, "reset")  == 0 ? "Reset"
                                                            : "Button";
            int lead  = ns_layout_control_pad_spaces(s, FALSE);
            int trail = ns_layout_control_pad_spaces(s, TRUE);
            g_string_append(ctx->out, "\xc2\xa0");
            for (int i = 0; i < lead; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, v);
            for (int i = 0; i < trail; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, "\xc2\xa0");
            ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "checkbox") == 0) {
            gboolean checked = ns_input_is_checked(n);
            gsize start = ctx->out->len;
            g_string_append(ctx->out, checked ? "\xe2\x98\x91" : "\xe2\x98\x90");
            ns_layout_emit_form_attr_sized(ctx->attrs,
                checked ? NS_INLINE_CHECKBOX_CHECKED : NS_INLINE_CHECKBOX,
                start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "radio") == 0) {
            gboolean checked = ns_input_is_checked(n);
            gsize start = ctx->out->len;
            g_string_append(ctx->out, checked ? "\xe2\x97\x89" : "\xe2\x97\x8b");
            ns_layout_emit_form_attr_sized(ctx->attrs,
                checked ? NS_INLINE_RADIO_CHECKED : NS_INLINE_RADIO,
                start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "file") == 0) {
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0" "Choose File" "\xc2\xa0");
            ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
            const char *fpath = ns_element_get_attr(n, "data-nd-file-path");
            if (fpath && *fpath) {
                const char *base = strrchr(fpath, '/');
#ifdef G_OS_WIN32
                const char *base_w = strrchr(fpath, '\\');
                if (!base || (base_w && base_w > base)) base = base_w;
#endif
                const char *show = base ? base + 1 : fpath;
                g_string_append_c(ctx->out, ' ');
                g_string_append(ctx->out, show);
            } else {
                g_string_append(ctx->out, " (no file chosen)");
            }
        } else if (type && g_ascii_strcasecmp(type, "color") == 0) {
            const char *v = ns_element_get_attr(n, "value");
            const char *hex = v && *v ? v : "#000000";
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            gsize swatch_start = ctx->out->len;
            g_string_append(ctx->out, "\xe2\x96\xa0");
            gsize swatch_end = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, hex);
            g_string_append(ctx->out, "\xc2\xa0");
            ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
            guint8 r8 = 0, g8 = 0, b8 = 0;
            if (hex[0] == '#' && strlen(hex) >= 7) {
                unsigned int rv, gv, bv;
                if (sscanf(hex + 1, "%2x%2x%2x", &rv, &gv, &bv) == 3) {
                    r8 = (guint8)rv; g8 = (guint8)gv; b8 = (guint8)bv;
                }
            }
            ns_layout_emit_color_attr(ctx->attrs, swatch_start, swatch_end, r8, g8, b8, 255);
        } else if (type && (g_ascii_strcasecmp(type, "range") == 0)) {
            const char *v = ns_element_get_attr(n, "value");
            const char *mn = ns_element_get_attr(n, "min");
            const char *mx = ns_element_get_attr(n, "max");
            double vv = v && *v ? g_ascii_strtod(v, NULL) : 50;
            double mnv = mn && *mn ? g_ascii_strtod(mn, NULL) : 0;
            double mxv = mx && *mx ? g_ascii_strtod(mx, NULL) : 100;
            if (mxv <= mnv) mxv = mnv + 1;
            double frac = (vv - mnv) / (mxv - mnv);
            if (frac < 0) frac = 0;
            if (frac > 1) frac = 1;
            int knob_at = (int)(frac * 10 + 0.5);
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            for (int i = 0; i <= 10; i++) {
                if (i == knob_at)
                    g_string_append(ctx->out, "\xe2\x97\x8f");
                else
                    g_string_append(ctx->out, "\xe2\x94\x80");
            }
            g_string_append_printf(ctx->out, " %g\xc2\xa0", vv);
            ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
        } else if (type && (g_ascii_strcasecmp(type, "date") == 0 ||
                            g_ascii_strcasecmp(type, "datetime-local") == 0 ||
                            g_ascii_strcasecmp(type, "time") == 0 ||
                            g_ascii_strcasecmp(type, "month") == 0 ||
                            g_ascii_strcasecmp(type, "week") == 0)) {
            const char *v = ns_element_get_attr(n, "value");
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            if (v && *v) g_string_append(ctx->out, v);
            else         g_string_append(ctx->out, "____-__-__");
            g_string_append(ctx->out, "\xc2\xa0");
            ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
        }
        return;
    }
    if (strcmp(n->name, "button") == 0) {
        char *label = ns_node_collect_text(n);
        if (!label || !*label) {
            g_free(label);
            const ns_style *bs = ctx->styles
                ? g_hash_table_lookup(ctx->styles, n) : NULL;
            GString *pseudo = g_string_new(NULL);
            if (bs && bs->before)
                ns_layout_append_pseudo_content(pseudo, bs->before->values[NS_CSS_CONTENT],
                                      n, bs->before);
            if (bs && bs->after)
                ns_layout_append_pseudo_content(pseudo, bs->after->values[NS_CSS_CONTENT],
                                      n, bs->after);
            if (pseudo->len > 0) {
                label = g_string_free(pseudo, FALSE);
            } else {
                g_string_free(pseudo, TRUE);
                const char *aria = ns_element_get_attr(n, "aria-label");
                const char *title = ns_element_get_attr(n, "title");
                const char *value = ns_element_get_attr(n, "value");
                if (aria && *aria) label = g_strdup(aria);
                else if (title && *title) label = g_strdup(title);
                else if (value && *value) label = g_strdup(value);
                else label = g_strdup("");
            }
        }
        if (!*label) {
            g_free(label);
            return;
        }
        gsize start = ctx->out->len;
        g_string_append(ctx->out, "\xc2\xa0");
        g_string_append(ctx->out, label);
        g_string_append(ctx->out, "\xc2\xa0");
        ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
        g_free(label);
        return;
    }
    if (strcmp(n->name, "select") == 0) {
        gboolean multi = ns_element_get_attr(n, "multiple") != NULL;
        const char *size_attr = ns_element_get_attr(n, "size");
        int size_n = size_attr ? ns_parse_int(size_attr, 0, 0, 1000) : 0;
        gboolean listbox = multi || size_n > 1;
        if (listbox) {
            int shown = 0;
            int cap = size_n > 0 ? size_n : 6;
            gboolean first = TRUE;
            for (const ns_node *c = n->first_child; c && shown < cap; c = c->next_sibling) {
                if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
                if (strcmp(c->name, "optgroup") == 0) {
                    const char *gl = ns_element_get_attr(c, "label");
                    if (gl && *gl) {
                        if (!first) g_string_append(ctx->out, "\xe2\x80\xa8");
                        g_string_append_printf(ctx->out, "\xc2\xa0%s\xc2\xa0", gl);
                        first = FALSE;
                        shown++;
                    }
                    for (const ns_node *opt = c->first_child;
                         opt && shown < cap; opt = opt->next_sibling) {
                        if (!ns_node_is_element_named(opt, "option")) continue;
                        ns_layout_emit_listbox_option(ctx->out, ctx->attrs, ctx->styles, opt, first);
                        first = FALSE;
                        shown++;
                    }
                } else if (strcmp(c->name, "option") == 0) {
                    ns_layout_emit_listbox_option(ctx->out, ctx->attrs, ctx->styles, c, first);
                    first = FALSE;
                    shown++;
                }
            }
            if (shown == 0) {
                gsize start = ctx->out->len;
                g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
                ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
                                     start, ctx->out->len, n, ctx->styles);
            }
            return;
        }
        const ns_node *chosen = ns_select_chosen_option(n);
        char *label = chosen ? ns_node_collect_text(chosen) : g_strdup("");
        if (!label) label = g_strdup("");
        gsize start = ctx->out->len;
        g_string_append(ctx->out, "\xc2\xa0");
        if (*label) g_string_append(ctx->out, label);
        g_string_append(ctx->out, n == g_open_select_for_layout
                                  ? " \xe2\x96\xb4\xc2\xa0" : " \xe2\x96\xbe\xc2\xa0");
        ns_layout_emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
                             start, ctx->out->len, n, ctx->styles);
        g_free(label);
        if (n == g_open_select_for_layout) {
            for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
                if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
                const ns_node *opts[2] = { NULL, NULL };
                const ns_node *grp = NULL;
                if (strcmp(c->name, "option") == 0) {
                    opts[0] = c;
                } else if (strcmp(c->name, "optgroup") == 0) {
                    grp = c;
                } else {
                    continue;
                }
                if (grp) {
                    const char *gl = ns_element_get_attr(grp, "label");
                    if (gl && *gl) {
                        g_string_append(ctx->out, "\xe2\x80\xa8\xc2\xa0");
                        g_string_append(ctx->out, gl);
                        g_string_append(ctx->out, "\xc2\xa0");
                    }
                    for (const ns_node *o = grp->first_child; o; o = o->next_sibling)
                        if (ns_node_is_element_named(o, "option"))
                            ns_layout_emit_open_select_option(ctx->out, ctx->attrs, ctx->styles, o);
                } else {
                    ns_layout_emit_open_select_option(ctx->out, ctx->attrs, ctx->styles, opts[0]);
                }
            }
        }
        return;
    }
    if (strcmp(n->name, "option") == 0 || strcmp(n->name, "optgroup") == 0)
        return;
    if (strcmp(n->name, "textarea") == 0) {
        gsize start = ctx->out->len;
        gboolean focused = (n == g_focused_input_for_layout);
        g_string_append(ctx->out, "\xc2\xa0");
        gsize val_start = ctx->out->len;
        char *raw = ns_textarea_value_dup(n);
        gsize value_byte_len = strlen(raw);
        gboolean any = value_byte_len > 0;
        gsize caret_byte = g_focused_caret_byte_for_layout;
        if (caret_byte > value_byte_len) caret_byte = value_byte_len;
        gsize anchor_byte = g_focused_sel_anchor_byte_for_layout;
        if (anchor_byte > value_byte_len) anchor_byte = value_byte_len;
        gsize caret_pos = val_start;
        gsize anchor_pos = val_start;
        for (gsize i = 0; i <= value_byte_len; i++) {
            if (i == caret_byte) caret_pos = ctx->out->len;
            if (i == anchor_byte) anchor_pos = ctx->out->len;
            if (i == value_byte_len) break;
            char ch = raw[i];
            if (ch == '\n')
                g_string_append(ctx->out, "\xe2\x80\xa8");
            else if (ch != '\r')
                g_string_append_c(ctx->out, ch);
        }
        g_free(raw);
        gsize disp_end = ctx->out->len;
        gboolean is_placeholder = FALSE;
        gboolean ta_sized = ns_element_get_attr(n, "rows") ||
                            ns_element_get_attr(n, "cols");
        if (!any) {
            const char *ph = ns_element_get_attr(n, "placeholder");
            if (ph && *ph) {
                for (const char *p = ph; *p; p++) {
                    if (*p == '\n') g_string_append(ctx->out, "\xe2\x80\xa8");
                    else if (*p != '\r') g_string_append_c(ctx->out, *p);
                }
                disp_end = ctx->out->len;
                is_placeholder = TRUE;
                if (focused) { caret_pos = val_start; anchor_pos = val_start; }
                else         { caret_pos = 0; }
            } else if (ta_sized) {
                const char *rows_attr = ns_element_get_attr(n, "rows");
                int row_lines = rows_attr ? atoi(rows_attr) : 2;
                if (row_lines < 1) row_lines = 1;
                if (row_lines > 1000) row_lines = 1000;
                for (int r = 0; r < row_lines; r++) {
                    if (r) g_string_append(ctx->out, "\xe2\x80\xa8");
                    g_string_append(ctx->out, "\xc2\xa0");
                }
                if (focused) { caret_pos = val_start; anchor_pos = val_start; }
                else         { caret_pos = 0; }
            } else {
                for (int i = 0; i < 40; i++) g_string_append(ctx->out, "\xc2\xa0");
                if (focused) { caret_pos = val_start; anchor_pos = val_start; }
                else         { caret_pos = 0; }
            }
        }
        g_string_append(ctx->out, "\xc2\xa0");
        ns_inline_attr_kind ta_kind = focused
                                       ? NS_INLINE_INPUT_FIELD_FOCUSED
                                       : NS_INLINE_INPUT_FIELD;
        ns_layout_emit_form_attr_sized(ctx->attrs, ta_kind, start, ctx->out->len,
                             n, ctx->styles);
        ns_layout_emit_control_text_style(ctx->attrs, s, start, ctx->out->len,
                                val_start, disp_end, is_placeholder,
                                ns_element_get_attr(n, "disabled") != NULL);
        if (!is_placeholder && disp_end > val_start &&
            ns_element_get_attr(n, "disabled") == NULL &&
            ns_node_spellcheck_used(n)) {
            ns_inline_attr sc = { .kind = NS_INLINE_SPELLCHECK,
                                  .start = val_start,
                                  .len = disp_end - val_start, .dom = n };
            g_array_append_val(ctx->attrs, sc);
        }
        if (focused) {
            if (anchor_pos != caret_pos) {
                gsize s0 = anchor_pos < caret_pos ? anchor_pos : caret_pos;
                gsize s1 = anchor_pos < caret_pos ? caret_pos : anchor_pos;
                ns_layout_emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
            }
            ns_layout_emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
        }
        return;
    }

    const char *prev_href   = ctx->active_href;
    const char *prev_target = ctx->active_target;
    const ns_node *prev_link_node = ctx->active_link_node;
    if (strcmp(n->name, "a") == 0) {
        const char *h = ns_element_get_attr(n, "href");
        if (h && *h) {
            ctx->active_href   = h;
            ctx->active_target = ns_element_get_attr(n, "target");
            ctx->active_link_node = n;
        }
    }
    double ml = length_or(s ? s->values[NS_CSS_MARGIN_LEFT]  : NULL, 0);
    double mr = length_or(s ? s->values[NS_CSS_MARGIN_RIGHT] : NULL, 0);
    double pl = inline_box_side_px(s, NS_CSS_PADDING_LEFT,
                                   NS_CSS_BORDER_LEFT_WIDTH,
                                   NS_CSS_BORDER_LEFT_STYLE);
    double pr = inline_box_side_px(s, NS_CSS_PADDING_RIGHT,
                                   NS_CSS_BORDER_RIGHT_WIDTH,
                                   NS_CSS_BORDER_RIGHT_STYLE);
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs, ml);
    gsize elem_start = ctx->out->len;
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs, pl);
    gboolean bold   = ns_layout_tag_is_bold(n->name);
    gboolean italic = ns_layout_tag_is_italic(n->name);
    gboolean mono   = ns_layout_tag_is_monospace(n->name);
    gboolean uline  = ns_layout_tag_is_underline(n->name);
    gboolean oline  = FALSE;
    gboolean strike = ns_layout_tag_is_strike(n->name);
    const ns_css_value *fw = s ? s->values[NS_CSS_FONT_WEIGHT] : NULL;
    int font_weight_self = ns_css_font_weight_number(fw, -1);
    gboolean font_weight_active = font_weight_self > 0;
    if (font_weight_active) {
        bold = FALSE;
    } else if (fw && fw->kind == NS_CSS_V_KEYWORD && fw->u.keyword) {
        const char *kw = fw->u.keyword;
        if (strcmp(kw, "bold") == 0 || strcmp(kw, "bolder") == 0) bold = TRUE;
    }
    if (s && s->values[NS_CSS_FONT_STYLE] &&
        s->values[NS_CSS_FONT_STYLE]->kind == NS_CSS_V_KEYWORD &&
        (strcmp(s->values[NS_CSS_FONT_STYLE]->u.keyword, "italic") == 0 ||
         strcmp(s->values[NS_CSS_FONT_STYLE]->u.keyword, "oblique") == 0))
        italic = TRUE;
    if (s && s->values[NS_CSS_TEXT_DECORATION] &&
        s->values[NS_CSS_TEXT_DECORATION]->kind == NS_CSS_V_KEYWORD) {
        const char *kw = s->values[NS_CSS_TEXT_DECORATION]->u.keyword;
        if (strstr(kw, "underline")) uline = TRUE;
        if (strstr(kw, "overline")) oline = TRUE;
        if (strstr(kw, "line-through")) strike = TRUE;
        if (strstr(kw, "none")) { uline = FALSE; oline = FALSE; strike = FALSE; }
    }
    const ns_css_value *tdc = s ? s->values[NS_CSS_TEXT_DECORATION_COLOR] : NULL;
    if (tdc && tdc->kind == NS_CSS_V_COLOR && tdc->u.color.a == 0)
        { uline = FALSE; oline = FALSE; strike = FALSE; }
    const ns_style *decor_style = (uline || oline || strike) ? s : NULL;
    if (bold && ctx->bold_depth++ == 0) ctx->bold_start = ctx->out->len;
    if (italic && ctx->italic_depth++ == 0) ctx->italic_start = ctx->out->len;
    if (mono && ctx->mono_depth++ == 0) ctx->mono_start = ctx->out->len;
    if (uline && ctx->underline_depth++ == 0) ctx->underline_start = ctx->out->len;
    if (oline && ctx->overline_depth++ == 0) ctx->overline_start = ctx->out->len;
    if (strike && ctx->strike_depth++ == 0) ctx->strike_start = ctx->out->len;

    gsize weight_start = ctx->out->len;
    const ns_css_value *fst = s ? s->values[NS_CSS_FONT_STRETCH] : NULL;
    int font_stretch_self = ns_css_font_stretch_rank(fst);
    gboolean font_stretch_active = fst != NULL;
    gsize stretch_start = ctx->out->len;
    int font_kerning_self = ns_layout_font_kerning_from_style(s);
    const char *font_ligatures_self = ns_layout_font_ligatures_from_style(s);
    const char *font_features_self = ns_layout_font_feature_settings_from_style(s);
    gboolean font_features_active =
        font_kerning_self >= 0 || font_ligatures_self != NULL ||
        font_features_self != NULL;
    gsize features_start = ctx->out->len;
    const char *font_variations_self = ns_layout_font_variation_settings_from_style(s);
    gsize variations_start = ctx->out->len;

    gboolean is_q = strcmp(n->name, "q") == 0;
    if (is_q) {
        char *open_q = ns_layout_quotes_string_for(s, ctx->q_depth, FALSE);
        g_string_append(ctx->out, open_q);
        g_free(open_q);
        ctx->q_depth++;
    }

    gboolean bidi_override = FALSE;
    gboolean bidi_isolate = FALSE;
    gboolean bidi_plaintext = FALSE;
    if (s) {
        const char *ub = ns_style_keyword(s, NS_CSS_UNICODE_BIDI);
        if (ub) {
            if (strcmp(ub, "bidi-override") == 0)
                bidi_override = TRUE;
            else if (strcmp(ub, "isolate-override") == 0)
                bidi_override = bidi_isolate = TRUE;
            else if (strcmp(ub, "isolate") == 0)
                bidi_isolate = TRUE;
            else if (strcmp(ub, "plaintext") == 0)
                bidi_plaintext = TRUE;
        }
    }
    if (bidi_override || bidi_isolate || bidi_plaintext) {
        const char *bd_dir = ns_element_get_attr(n, "dir");
        if (!bd_dir || !*bd_dir) bd_dir = ns_style_keyword(s, NS_CSS_DIRECTION);
        gboolean rtl = bd_dir && g_ascii_strcasecmp(bd_dir, "rtl") == 0;
        gboolean dir_auto = (bd_dir && g_ascii_strcasecmp(bd_dir, "auto") == 0) ||
                            (strcmp(n->name, "bdi") == 0 &&
                             !ns_element_get_attr(n, "dir"));
        if (bidi_isolate || bidi_plaintext)
            g_string_append(ctx->out,
                (bidi_plaintext || dir_auto) ? "\xe2\x81\xa8"   /* FSI */
                : rtl                        ? "\xe2\x81\xa7"   /* RLI */
                                             : "\xe2\x81\xa6"); /* LRI */
        if (bidi_override)
            g_string_append(ctx->out, rtl ? "\xe2\x80\xae"   /* RLO */
                                           : "\xe2\x80\xad"); /* LRO */
    }

    gboolean sup = strcmp(n->name, "sup") == 0;
    gboolean sub = strcmp(n->name, "sub") == 0;
    gsize rise_start = ctx->out->len;
    gboolean small_caps = s && keyword_is(s->values[NS_CSS_FONT_VARIANT],
                                          "small-caps");
    gsize sc_start = ctx->out->len;

    double font_size_self = 0;
    if (s && s->values[NS_CSS_FONT_SIZE]) {
        const ns_css_value *fv = s->values[NS_CSS_FONT_SIZE];
        if (fv->kind == NS_CSS_V_LENGTH && fv->u.length.unit == NS_CSS_UNIT_PX)
            font_size_self = fv->u.length.v;
    }
    gsize fs_start = ctx->out->len;
    gboolean fs_active = font_size_self > 0;

    gsize color_start = ctx->out->len;
    gboolean color_active = FALSE;
    guint8 cr = 0, cg = 0, cb = 0, ca = 0;
    if (s && s->values[NS_CSS_COLOR] &&
        s->values[NS_CSS_COLOR]->kind == NS_CSS_V_COLOR) {
        cr = s->values[NS_CSS_COLOR]->u.color.r;
        cg = s->values[NS_CSS_COLOR]->u.color.g;
        cb = s->values[NS_CSS_COLOR]->u.color.b;
        ca = s->values[NS_CSS_COLOR]->u.color.a;
        color_active = TRUE;
    }
    {
        const char *vis = s ? ns_style_keyword(s, NS_CSS_VISIBILITY) : NULL;
        if (vis && (strcmp(vis, "hidden") == 0 || strcmp(vis, "collapse") == 0)) {
            ca = 0;
            color_active = TRUE;
        }
    }

    gsize family_start = ctx->out->len;
    const char *family_str = NULL;
    if (s && s->values[NS_CSS_FONT_FAMILY] &&
        s->values[NS_CSS_FONT_FAMILY]->kind == NS_CSS_V_KEYWORD)
        family_str = s->values[NS_CSS_FONT_FAMILY]->u.keyword;

    const char *prev_text_transform = ctx->text_transform;
    if (s && s->values[NS_CSS_TEXT_TRANSFORM] &&
        s->values[NS_CSS_TEXT_TRANSFORM]->kind == NS_CSS_V_KEYWORD) {
        const char *kw = s->values[NS_CSS_TEXT_TRANSFORM]->u.keyword;
        if (strcmp(kw, "none") == 0)
            ctx->text_transform = NULL;
        else if (strcmp(kw, "uppercase") == 0 ||
                 strcmp(kw, "lowercase") == 0 ||
                 strcmp(kw, "capitalize") == 0)
            ctx->text_transform = kw;
    }

    gboolean pseudo_blocks = n == g_pseudo_blocks_host;
    if (s && s->before && s->before->values[NS_CSS_CONTENT] &&
        !(pseudo_blocks && g_pseudo_block_before))
        append_pseudo_inline(ctx, s->before, n);

    for (const ns_node *c = n->first_child; c; c = c->next_sibling)
        collect_walk(c, ctx, depth + 1);

    if (s && s->after && s->after->values[NS_CSS_CONTENT] &&
        !(pseudo_blocks && g_pseudo_block_after))
        append_pseudo_inline(ctx, s->after, n);

    ctx->text_transform = prev_text_transform;

    if (fs_active && ctx->out->len > fs_start)
        ns_layout_emit_font_size_attr(ctx->attrs, fs_start, ctx->out->len, font_size_self);
    if (color_active && ctx->out->len > color_start)
        ns_layout_emit_color_attr(ctx->attrs, color_start, ctx->out->len, cr, cg, cb, ca);
    if (family_str && ctx->out->len > family_start)
        ns_layout_emit_font_family_attr(ctx->attrs, family_start, ctx->out->len, family_str);
    if (font_weight_active && ctx->out->len > weight_start)
        ns_layout_emit_font_weight_attr(ctx->attrs, weight_start, ctx->out->len, font_weight_self);
    if (font_stretch_active && ctx->out->len > stretch_start)
        ns_layout_emit_font_stretch_attr(ctx->attrs, stretch_start, ctx->out->len,
                               font_stretch_self);
    if (font_features_active && ctx->out->len > features_start)
        ns_layout_emit_font_features_attr(ctx->attrs, features_start, ctx->out->len,
                                font_kerning_self, font_ligatures_self,
                                font_features_self);
    if (font_variations_self && ctx->out->len > variations_start)
        ns_layout_emit_font_variations_attr(ctx->attrs, variations_start, ctx->out->len,
                                  font_variations_self);

    if (bold && --ctx->bold_depth == 0)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_BOLD, ctx->bold_start, ctx->out->len);
    if (italic && --ctx->italic_depth == 0)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_ITALIC, ctx->italic_start, ctx->out->len);
    if (mono && --ctx->mono_depth == 0)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_MONOSPACE, ctx->mono_start, ctx->out->len);
    if (uline && --ctx->underline_depth == 0)
        ns_layout_emit_attr_styled(ctx->attrs, NS_INLINE_UNDERLINE, ctx->underline_start,
                         ctx->out->len, decor_style);
    if (oline && --ctx->overline_depth == 0)
        ns_layout_emit_attr_styled(ctx->attrs, NS_INLINE_OVERLINE, ctx->overline_start,
                         ctx->out->len, decor_style);
    if (strike && --ctx->strike_depth == 0)
        ns_layout_emit_attr_styled(ctx->attrs, NS_INLINE_STRIKETHROUGH, ctx->strike_start,
                         ctx->out->len, decor_style);
    if (sup && ctx->out->len > rise_start)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_SUPERSCRIPT, rise_start, ctx->out->len);
    if (sub && ctx->out->len > rise_start)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_SUBSCRIPT, rise_start, ctx->out->len);
    if (small_caps && ctx->out->len > sc_start)
        ns_layout_emit_attr(ctx->attrs, NS_INLINE_SMALL_CAPS, sc_start, ctx->out->len);
    if (bidi_override)
        g_string_append(ctx->out, "\xe2\x80\xac");   /* PDF */
    if (bidi_isolate || bidi_plaintext)
        g_string_append(ctx->out, "\xe2\x81\xa9");   /* PDI */
    if (is_q) {
        ctx->q_depth--;
        char *close_q = ns_layout_quotes_string_for(s, ctx->q_depth, TRUE);
        g_string_append(ctx->out, close_q);
        g_free(close_q);
    }
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs, pr);
    if (ctx->out->len > elem_start) {
        ns_inline_attr elem = {
            .kind = NS_INLINE_ELEMENT,
            .start = elem_start,
            .len = ctx->out->len - elem_start,
            .dom = n,
            .style = s,
        };
        g_array_append_val(ctx->attrs, elem);
    }
    ns_layout_append_inline_spacer(ctx->out, ctx->attrs, mr);
    ctx->active_href   = prev_href;
    ctx->active_target = prev_target;
    ctx->active_link_node = prev_link_node;
}

enum { NS_WS_COLLAPSE = 0, NS_WS_PRESERVE = 1, NS_WS_PRE_LINE = 2 };

static int
white_space_mode(const ns_node *node, GHashTable *styles)
{
    for (const ns_node *p = node; p; p = p->parent) {
        if (p->kind != NS_NODE_ELEMENT) continue;
        const ns_style *ps = g_hash_table_lookup(styles, p);
        if (ps) {
            const ns_css_value *ws = ps->values[NS_CSS_WHITE_SPACE];
            if (ws && ws->kind == NS_CSS_V_KEYWORD && ws->u.keyword) {
                const char *kw = ws->u.keyword;
                if (strcmp(kw, "pre-line") == 0)
                    return NS_WS_PRE_LINE;
                if (strcmp(kw, "pre") == 0 ||
                    strcmp(kw, "pre-wrap") == 0 ||
                    strcmp(kw, "break-spaces") == 0)
                    return NS_WS_PRESERVE;
                if (strcmp(kw, "normal") == 0 || strcmp(kw, "nowrap") == 0)
                    return NS_WS_COLLAPSE;
            }
        }
        if (p->name && (strcmp(p->name, "pre") == 0 ||
                        strcmp(p->name, "textarea") == 0))
            return NS_WS_PRESERVE;
    }
    return NS_WS_COLLAPSE;
}

static ns_box *
build_inline_run_impl(const ns_node *first, const ns_node *last_excl,
                      GHashTable *styles, gboolean abs_placeholders)
{
    GString *buf = g_string_new(NULL);
    GArray  *raw_links = g_array_new(FALSE, FALSE, sizeof(ns_link_range));
    GArray  *raw_attrs = g_array_new(FALSE, FALSE, sizeof(ns_inline_attr));
    GArray  *raw_atomics = g_array_new(FALSE, FALSE, sizeof(ns_atomic_raw));
    GArray  *ws_ranges = g_array_new(FALSE, FALSE, sizeof(ns_ws_range));
    g_array_set_clear_func(raw_links, link_clear);
    collector_ctx ctx = {
        .styles = styles, .out = buf, .links = raw_links, .attrs = raw_attrs,
        .atomics = raw_atomics, .abs_placeholders = abs_placeholders,
        .ws_ranges = ws_ranges,
    };
    if (first && first->parent) {
        const ns_style *ps = g_hash_table_lookup(styles, first->parent);
        if (ps && ps->values[NS_CSS_TEXT_TRANSFORM] &&
            ps->values[NS_CSS_TEXT_TRANSFORM]->kind == NS_CSS_V_KEYWORD) {
            const char *kw = ps->values[NS_CSS_TEXT_TRANSFORM]->u.keyword;
            if (kw && strcmp(kw, "none") != 0) ctx.text_transform = kw;
        }
    }
    if (first && first->parent && first->parent->kind == NS_NODE_ELEMENT &&
        first == first->parent->first_child) {
        const ns_style *li_style = g_hash_table_lookup(styles, first->parent);
        if (ns_paint_li_is_inside(li_style)) {
            char marker[64];
            if (ns_paint_li_marker_text(first->parent, li_style,
                                        marker, sizeof marker))
                g_string_append(buf, marker);
        }
    }
    for (const ns_node *n = first; n && n != last_excl; n = n->next_sibling)
        collect_walk(n, &ctx, g_inline_collect_depth);

    int run_ws_mode = first ? white_space_mode(first, styles) : NS_WS_COLLAPSE;

    GString *collapsed = g_string_new(NULL);
    gsize   *map = g_new(gsize, buf->len + 1);
    gboolean prev_ws = run_ws_mode != NS_WS_PRESERVE;
    gboolean trailing_preserved = FALSE;
    guint ri = 0;
    for (gsize i = 0; i < buf->len; i++) {
        char c = buf->str[i];
        while (ri < ws_ranges->len &&
               g_array_index(ws_ranges, ns_ws_range, ri).end <= i)
            ri++;
        int ws_mode = run_ws_mode;
        if (ri < ws_ranges->len &&
            g_array_index(ws_ranges, ns_ws_range, ri).start <= i)
            ws_mode = g_array_index(ws_ranges, ns_ws_range, ri).mode;
        trailing_preserved = ws_mode == NS_WS_PRESERVE;
        if (ws_mode == NS_WS_PRESERVE) {
            map[i] = collapsed->len;
            g_string_append_c(collapsed, c);
            prev_ws = c == '\n';
            continue;
        }
        if (ws_mode == NS_WS_PRE_LINE && c == '\n') {
            map[i] = collapsed->len;
            g_string_append_c(collapsed, '\n');
            prev_ws = TRUE;
            continue;
        }
        gboolean ws = (c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f');
        if (ws) {
            if (!prev_ws) {
                map[i] = collapsed->len;
                g_string_append_c(collapsed, ' ');
            } else {
                map[i] = collapsed->len;
            }
            prev_ws = TRUE;
        } else {
            map[i] = collapsed->len;
            g_string_append_c(collapsed, c);
            prev_ws = FALSE;
        }
    }
    map[buf->len] = collapsed->len;

    ns_box *box = box_new_inline();
    if (!trailing_preserved && collapsed->len > 0 &&
        collapsed->str[collapsed->len - 1] == ' ')
        g_string_set_size(collapsed, collapsed->len - 1);

    for (guint i = 0; i < raw_links->len; i++) {
        ns_link_range *r = &g_array_index(raw_links, ns_link_range, i);
        if (r->start > buf->len) r->start = buf->len;
        gsize end = r->start + r->len;
        if (end > buf->len) end = buf->len;
        gsize ns = map[r->start];
        gsize ne = map[end];
        if (ne > collapsed->len) ne = collapsed->len;
        if (ne <= ns) continue;
        ns_link_range out = {
            .start = ns,
            .len = ne - ns,
            .href = g_strdup(r->href),
            .target = r->target ? g_strdup(r->target) : NULL,
            .dom = r->dom,
        };
        g_array_append_val(inline_links_ensure(box), out);
    }

    for (guint i = 0; i < raw_attrs->len; i++) {
        ns_inline_attr *a = &g_array_index(raw_attrs, ns_inline_attr, i);
        if (a->start > buf->len) a->start = buf->len;
        gsize end = a->start + a->len;
        if (end > buf->len) end = buf->len;
        gsize ns = map[a->start];
        gsize ne = map[end];
        if (ne > collapsed->len) ne = collapsed->len;
        if (ne <= ns) continue;
        ns_inline_attr out = *a;
        out.start = ns;
        out.len = ne - ns;
        g_array_append_val(box->attrs, out);
        if (out.bg_image_src) {
            ns_box_media *m = ns_box_media_ensure(box);
            if (!m->bg_image_src) {
                m->bg_image_src = g_strdup(out.bg_image_src);
                m->bg_image = out.bg_image;
            }
        }
    }

    if (first && first->parent && collapsed->len > 0) {
        const ns_style *ps = g_hash_table_lookup(styles, first->parent);
        if (ps && ps->values[NS_CSS_TEXT_DECORATION] &&
            ps->values[NS_CSS_TEXT_DECORATION]->kind == NS_CSS_V_KEYWORD &&
            ps->values[NS_CSS_TEXT_DECORATION]->u.keyword) {
            const char *kw = ps->values[NS_CSS_TEXT_DECORATION]->u.keyword;
            const ns_css_value *pc = ps->values[NS_CSS_TEXT_DECORATION_COLOR];
            gboolean invisible = pc && pc->kind == NS_CSS_V_COLOR &&
                                 pc->u.color.a == 0;
            if (!strstr(kw, "none") && !invisible) {
                if (strstr(kw, "underline")) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_UNDERLINE, .style = ps,
                        .start = 0, .len = collapsed->len
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (strstr(kw, "overline")) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_OVERLINE, .style = ps,
                        .start = 0, .len = collapsed->len
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (strstr(kw, "line-through")) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_STRIKETHROUGH, .style = ps,
                        .start = 0, .len = collapsed->len
                    };
                    g_array_append_val(box->attrs, a);
                }
            }
        }
    }

    if (first && first->parent && collapsed->len > 0 &&
        first == first->parent->first_child) {
        const ns_style *ps = g_hash_table_lookup(styles, first->parent);
        if (ps && ps->first_letter) {
            const ns_style *fl = ps->first_letter;
            const char *txt = collapsed->str;
            gsize i = 0;
            while (i < collapsed->len &&
                   (txt[i] == ' ' || txt[i] == '\t' || txt[i] == '\n'))
                i++;
            while (i < collapsed->len) {
                gunichar ch = g_utf8_get_char(txt + i);
                if (g_unichar_isalnum(ch)) break;
                i = (gsize)(g_utf8_next_char(txt + i) - txt);
            }
            if (i < collapsed->len) {
                gsize fl_start = i;
                gsize fl_end = (gsize)(g_utf8_next_char(txt + i) - txt);
                if (fl_end > collapsed->len) fl_end = collapsed->len;
                gsize fl_len = fl_end - fl_start;
                if (fl->values[NS_CSS_FONT_SIZE] &&
                    fl->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
                    fl->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_PX) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_FONT_SIZE,
                        .start = fl_start, .len = fl_len,
                        .font_size_px = fl->values[NS_CSS_FONT_SIZE]->u.length.v,
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (fl->values[NS_CSS_COLOR] &&
                    fl->values[NS_CSS_COLOR]->kind == NS_CSS_V_COLOR) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_COLOR,
                        .start = fl_start, .len = fl_len,
                        .r = fl->values[NS_CSS_COLOR]->u.color.r,
                        .g = fl->values[NS_CSS_COLOR]->u.color.g,
                        .b = fl->values[NS_CSS_COLOR]->u.color.b,
                        .a = fl->values[NS_CSS_COLOR]->u.color.a,
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (fl->values[NS_CSS_BACKGROUND_COLOR] &&
                    fl->values[NS_CSS_BACKGROUND_COLOR]->kind == NS_CSS_V_COLOR) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_BG_COLOR,
                        .start = fl_start, .len = fl_len,
                        .r = fl->values[NS_CSS_BACKGROUND_COLOR]->u.color.r,
                        .g = fl->values[NS_CSS_BACKGROUND_COLOR]->u.color.g,
                        .b = fl->values[NS_CSS_BACKGROUND_COLOR]->u.color.b,
                        .a = fl->values[NS_CSS_BACKGROUND_COLOR]->u.color.a,
                    };
                    g_array_append_val(box->attrs, a);
                }
                const ns_css_value *fw = fl->values[NS_CSS_FONT_WEIGHT];
                int font_weight = ns_css_font_weight_number(fw, -1);
                if (font_weight > 0) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_FONT_WEIGHT,
                        .start = fl_start, .len = fl_len,
                        .font_weight = font_weight,
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (keyword_is(fl->values[NS_CSS_FONT_STYLE], "italic") ||
                    keyword_is(fl->values[NS_CSS_FONT_STYLE], "oblique")) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_ITALIC,
                        .start = fl_start, .len = fl_len,
                    };
                    g_array_append_val(box->attrs, a);
                }
                if (fl->values[NS_CSS_FONT_FAMILY] &&
                    fl->values[NS_CSS_FONT_FAMILY]->kind == NS_CSS_V_KEYWORD) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_FONT_FAMILY,
                        .start = fl_start, .len = fl_len,
                        .family = fl->values[NS_CSS_FONT_FAMILY]->u.keyword,
                    };
                    g_array_append_val(box->attrs, a);
                }
                int fk = ns_layout_font_kerning_from_style(fl);
                const char *flig = ns_layout_font_ligatures_from_style(fl);
                const char *ffea = ns_layout_font_feature_settings_from_style(fl);
                if (fk >= 0 || flig || ffea) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_FONT_FEATURES,
                        .start = fl_start, .len = fl_len,
                        .font_kerning = fk,
                        .font_ligatures = flig,
                        .font_features = ffea,
                    };
                    g_array_append_val(box->attrs, a);
                }
                const char *fvar = ns_layout_font_variation_settings_from_style(fl);
                if (fvar) {
                    ns_inline_attr a = {
                        .kind = NS_INLINE_FONT_VARIATIONS,
                        .start = fl_start, .len = fl_len,
                        .font_variations = fvar,
                    };
                    g_array_append_val(box->attrs, a);
                }
            }
        }
    }

    for (guint i = 0; i < raw_atomics->len; i++) {
        ns_atomic_raw *rr = &g_array_index(raw_atomics, ns_atomic_raw, i);
        gsize start = rr->start;
        if (start > buf->len) start = buf->len;
        gsize ns = map[start];
        if (ns > collapsed->len) ns = collapsed->len;
        if (!box->inline_atomics)
            box->inline_atomics = g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
        ns_inline_atomic ia = { .byte_off = ns, .box = rr->box };
        if (rr->box && !rr->box->parent) rr->box->parent = box;
        g_array_append_val(box->inline_atomics, ia);
    }

    g_free(map);
    g_array_free(raw_links, TRUE);
    g_array_free(raw_attrs, TRUE);
    g_array_free(raw_atomics, TRUE);
    g_array_free(ws_ranges, TRUE);
    g_string_free(buf, TRUE);

    box->text = g_string_free(collapsed, FALSE);
    return box;
}

static ns_box *
build_inline_run(const ns_node *first, const ns_node *last_excl, GHashTable *styles)
{
    return build_inline_run_impl(first, last_excl, styles, TRUE);
}

static ns_box *
build_inline_run_no_abs_placeholders(const ns_node *first,
                                     const ns_node *last_excl,
                                     GHashTable *styles)
{
    return build_inline_run_impl(first, last_excl, styles, FALSE);
}

static ns_box *
build_form_control_block(const ns_node *n, const ns_style *s, GHashTable *styles)
{
    ns_box *block = box_new(NS_BOX_BLOCK);
    block->dom = n;
    block->style = s;
    collect_box_bg_image(block, s);
    const ns_node *prev_fc = g_form_control_inline;
    g_form_control_inline = n;
    ns_box *run = build_inline_run(n, n->next_sibling, styles);
    g_form_control_inline = prev_fc;
    if (run && run->text && run->text[0]) {
        box_append_child(block, run);
    } else if (run) {
        ns_box_free(run);
    }
    return block;
}

static double
image_dimension_attr(const ns_node *n, const char *name)
{
    const char *s = ns_element_get_attr(n, name);
    if (!s || !*s) return 0;
    char *end = NULL;
    double v = g_ascii_strtod(s, &end);
    if (end == s || !(v > 0)) return 0;
    return v;
}

static ns_image *
decode_data_image_for_layout(const char *url, const char *key)
{
    if (!url || !key || !g_image_cache_for_layout ||
        !g_str_has_prefix(url, "data:image/"))
        return NULL;
    ns_image *cached = ns_image_cache_peek(g_image_cache_for_layout, key);
    if (cached) return cached;
    GByteArray *bytes = g_byte_array_new();
    char *ctype = NULL;
    gboolean too_large = FALSE;
    gboolean ok = ns_data_url_decode(url, bytes, NS_LAYOUT_DATA_IMAGE_BUDGET,
                                     &ctype, &too_large);
    g_free(ctype);
    if (!ok || too_large || bytes->len == 0) {
        g_byte_array_free(bytes, TRUE);
        return NULL;
    }
    ns_image *img = ns_image_cache_insert_encoded(g_image_cache_for_layout, key,
                                                  bytes->data, bytes->len);
    g_byte_array_free(bytes, TRUE);
    return img;
}

static ns_box *
build_image_box(const ns_node *n)
{
    const ns_node *img = n;
    double density = 1.0;
    char *url = ns_layout_choose_img_url(n, &img, &density);
    ns_box *box = box_new(NS_BOX_IMAGE);
    box->dom = img;
    ns_box_media *m = ns_box_media_ensure(box);
    m->image_src = url;
    m->image_density = density > 0 ? density : 1.0;
    box->content_width = image_dimension_attr(img, "width");
    box->content_height = image_dimension_attr(img, "height");
    double file_w = image_dimension_attr(img, "data-file-width");
    double file_h = image_dimension_attr(img, "data-file-height");
    if (box->content_width <= 0 && box->content_height > 0 &&
        file_w > 0 && file_h > 0)
        box->content_width = box->content_height * file_w / file_h;
    if (box->content_height <= 0 && box->content_width > 0 &&
        file_w > 0 && file_h > 0)
        box->content_height = box->content_width * file_h / file_w;
    m->declared_image_size =
        box->content_width > 0 && box->content_height > 0;
    if (g_image_cache_for_layout) {
        char *abs = g_base_url_for_layout
            ? ns_url_resolve(g_base_url_for_layout, url)
            : NULL;
        const char *key = abs ? abs : url;
        m->image = decode_data_image_for_layout(url, key);
        if (!m->image)
            m->image = ns_image_cache_peek(g_image_cache_for_layout, key);
        g_free(abs);
    }
    if (!m->declared_image_size && url && !m->image &&
        box->content_width <= 0 && box->content_height <= 0) {
        box->content_width = 200;
        box->content_height = 150;
        m->placeholder_image_size = TRUE;
    }
    return box;
}

static const char *
first_media_source_url(const ns_node *n)
{
    const char *src = ns_element_get_attr(n, NS_MEDIA_SRC_ATTR);
    if (src && *src) return src;
    src = ns_element_get_attr(n, "src");
    if (src && *src) return src;
    src = ns_element_get_attr(n, "data-mp4");
    if (src && *src) return src;
    src = ns_element_get_attr(n, "data-webm");
    if (src && *src) return src;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
        if (strcmp(c->name, "source") != 0) continue;
        const char *csrc = ns_element_get_attr(c, "src");
        if (csrc && *csrc) return csrc;
    }
    return NULL;
}

static ns_box *
build_audio_box(const ns_node *n)
{
    ns_box *box = box_new(NS_BOX_VIDEO);
    box->dom = n;
    ns_box_media *m = ns_box_media_ensure(box);
    const char *src = first_media_source_url(n);
    if (src) m->video_audio_src = g_strdup(src);
    if (ns_element_get_attr(n, "controls")) {
        const char *ws = ns_element_get_attr(n, "width");
        const char *hs = ns_element_get_attr(n, "height");
        box->content_width = ws ? g_ascii_strtod(ws, NULL) : 250;
        box->content_height = hs ? g_ascii_strtod(hs, NULL) : 32;
    }
    return box;
}

static ns_box *
build_video_box(const ns_node *n)
{
    const char *src = first_media_source_url(n);
    ns_box *box = box_new(NS_BOX_VIDEO);
    box->dom = n;
    ns_box_media *m = ns_box_media_ensure(box);
    if (src) m->video_src = g_strdup(src);
    const char *poster = ns_element_get_attr(n, "poster");
    const char *data_poster = ns_element_get_attr(n, "data-poster");
    const char *fallback_poster = ns_element_get_attr(n, NS_MEDIA_POSTER_ATTR);
    if ((!poster || !*poster || g_str_has_prefix(poster, "data:image/")) &&
        data_poster && *data_poster)
        poster = data_poster;
    if ((!poster || !*poster) && fallback_poster && *fallback_poster)
        poster = fallback_poster;
    if (poster && *poster) m->video_poster = g_strdup(poster);
    const char *ws = ns_element_get_attr(n, "width");
    const char *hs = ns_element_get_attr(n, "height");
    gboolean metadata = node_has_media_metadata(n);
    box->content_width  = ws ? g_ascii_strtod(ws, NULL) : (metadata ? 640 : 300);
    box->content_height = hs ? g_ascii_strtod(hs, NULL) : (metadata ? 360 : 150);
    const char *audio = ns_element_get_attr(n, "data-audio-src");
    if (audio && *audio) m->video_audio_src = g_strdup(audio);
    return box;
}

static ns_box *
pseudo_block_with_text(const ns_style *ps, const char *txt)
{
    ns_box *block = box_new(NS_BOX_BLOCK);
    block->style = ps;
    collect_box_bg_image(block, ps);
    if (txt && *txt) {
        ns_box *txtrun = box_new_inline();
        txtrun->text = g_strdup(txt);
        box_append_child(block, txtrun);
    }
    return block;
}

static void
pseudo_box_add_edge_spacers(ns_box *box, const ns_style *ps)
{
    double edges[4] = {
        length_or(ps->values[NS_CSS_MARGIN_LEFT], 0),
        length_or(ps->values[NS_CSS_PADDING_LEFT], 0),
        length_or(ps->values[NS_CSS_PADDING_RIGHT], 0),
        length_or(ps->values[NS_CSS_MARGIN_RIGHT], 0),
    };
    static const char spacer[] = "\xef\xbf\xbc";
    gsize slen = sizeof spacer - 1;
    gsize lead = (edges[0] > 0 ? slen : 0) + (edges[1] > 0 ? slen : 0);
    if (lead == 0 && !(edges[2] > 0) && !(edges[3] > 0)) return;
    GString *text = g_string_new(NULL);
    for (guint i = 0; i < box->attrs->len; i++)
        g_array_index(box->attrs, ns_inline_attr, i).start += lead;
    for (int e = 0; e < 4; e++) {
        if (e == 2) g_string_append(text, box->text);
        if (!(edges[e] > 0)) continue;
        ns_inline_attr a = {
            .kind = NS_INLINE_SPACER,
            .start = text->len,
            .len = slen,
            .box_w = edges[e],
        };
        g_string_append(text, spacer);
        g_array_append_val(box->attrs, a);
    }
    g_free(box->text);
    box->text = g_string_free(text, FALSE);
}

static ns_box *
build_pseudo_inline_for(const ns_style *ps, const ns_node *host)
{
    if (!ps) return NULL;
    const ns_css_value *cv = ps->values[NS_CSS_CONTENT];
    if (!cv || cv->kind != NS_CSS_V_KEYWORD || !cv->u.keyword) return NULL;
    gboolean inline_atomic = ns_display_is_atomic_inline(ns_css_display_of(ps));
    char *resolved = ns_layout_resolve_pseudo_content(cv->u.keyword, host);
    if (!resolved) {
        if (!inline_atomic) return NULL;
        resolved = g_strdup("");
    }
    char *quote = NULL;
    const char *txt = resolved;
    if (strcmp(txt, "open-quote") == 0)
        txt = quote = ns_layout_quotes_string_for(ps, 0, FALSE);
    else if (strcmp(txt, "close-quote") == 0)
        txt = quote = ns_layout_quotes_string_for(ps, 0, TRUE);
    else if (strcmp(txt, "no-open-quote") == 0 ||
             strcmp(txt, "no-close-quote") == 0) { g_free(resolved); return NULL; }

    if (inline_atomic) {
        ns_box *inner = pseudo_block_with_text(ps, txt);
        g_free(quote);
        g_free(resolved);
        ns_box *run = box_new_inline();
        run->style = ps;
        run->text = g_strdup("\xef\xbf\xbc");
        run->inline_atomics = g_array_new(FALSE, FALSE, sizeof(ns_inline_atomic));
        ns_inline_atomic ia = { .byte_off = 0, .box = inner };
        inner->parent = run;
        g_array_append_val(run->inline_atomics, ia);
        return run;
    }

    ns_box *box = box_new_inline();
    box->text = g_strdup(txt);
    g_free(quote);
    g_free(resolved);
    box->style = ps;

    gsize tlen = strlen(box->text);
    const char *vis = ns_style_keyword(ps, NS_CSS_VISIBILITY);
    gboolean invisible = vis && (strcmp(vis, "hidden") == 0 ||
                                 strcmp(vis, "collapse") == 0);
    if (invisible ||
        (ps->values[NS_CSS_COLOR] && ps->values[NS_CSS_COLOR]->kind == NS_CSS_V_COLOR)) {
        const ns_css_value *color = ps->values[NS_CSS_COLOR];
        gboolean have = color && color->kind == NS_CSS_V_COLOR;
        ns_inline_attr a = {
            .kind = NS_INLINE_COLOR,
            .start = 0, .len = tlen,
            .r = have ? color->u.color.r : 0,
            .g = have ? color->u.color.g : 0,
            .b = have ? color->u.color.b : 0,
            .a = invisible ? 0 : color->u.color.a,
        };
        g_array_append_val(box->attrs, a);
    }
    if (ps->values[NS_CSS_BACKGROUND_COLOR] &&
        ps->values[NS_CSS_BACKGROUND_COLOR]->kind == NS_CSS_V_COLOR) {
        ns_inline_attr a = {
            .kind = NS_INLINE_BG_COLOR,
            .start = 0, .len = tlen,
            .r = ps->values[NS_CSS_BACKGROUND_COLOR]->u.color.r,
            .g = ps->values[NS_CSS_BACKGROUND_COLOR]->u.color.g,
            .b = ps->values[NS_CSS_BACKGROUND_COLOR]->u.color.b,
            .a = ps->values[NS_CSS_BACKGROUND_COLOR]->u.color.a,
        };
        g_array_append_val(box->attrs, a);
    }
    if (ps->values[NS_CSS_FONT_SIZE] &&
        ps->values[NS_CSS_FONT_SIZE]->kind == NS_CSS_V_LENGTH &&
        ps->values[NS_CSS_FONT_SIZE]->u.length.unit == NS_CSS_UNIT_PX) {
        ns_inline_attr a = {
            .kind = NS_INLINE_FONT_SIZE,
            .start = 0, .len = tlen,
            .font_size_px = ps->values[NS_CSS_FONT_SIZE]->u.length.v,
        };
        g_array_append_val(box->attrs, a);
    }
    const ns_css_value *fw = ps->values[NS_CSS_FONT_WEIGHT];
    int font_weight = ns_css_font_weight_number(fw, -1);
    if (font_weight > 0) {
        ns_inline_attr a = {
            .kind = NS_INLINE_FONT_WEIGHT,
            .start = 0, .len = tlen,
            .font_weight = font_weight,
        };
        g_array_append_val(box->attrs, a);
    }
    if (ps->values[NS_CSS_FONT_STRETCH]) {
        ns_inline_attr a = {
            .kind = NS_INLINE_FONT_STRETCH,
            .start = 0, .len = tlen,
            .font_stretch =
                ns_css_font_stretch_rank(ps->values[NS_CSS_FONT_STRETCH]),
        };
        g_array_append_val(box->attrs, a);
    }
    int fk = ns_layout_font_kerning_from_style(ps);
    const char *flig = ns_layout_font_ligatures_from_style(ps);
    const char *ffea = ns_layout_font_feature_settings_from_style(ps);
    if (fk >= 0 || flig || ffea) {
        ns_inline_attr a = {
            .kind = NS_INLINE_FONT_FEATURES,
            .start = 0, .len = tlen,
            .font_kerning = fk,
            .font_ligatures = flig,
            .font_features = ffea,
        };
        g_array_append_val(box->attrs, a);
    }
    const char *fvar = ns_layout_font_variation_settings_from_style(ps);
    if (fvar) {
        ns_inline_attr a = {
            .kind = NS_INLINE_FONT_VARIATIONS,
            .start = 0, .len = tlen,
            .font_variations = fvar,
        };
        g_array_append_val(box->attrs, a);
    }
    if (keyword_is(ps->values[NS_CSS_FONT_STYLE], "italic") ||
        keyword_is(ps->values[NS_CSS_FONT_STYLE], "oblique")) {
        ns_inline_attr a = { .kind = NS_INLINE_ITALIC, .start = 0, .len = tlen };
        g_array_append_val(box->attrs, a);
    }
    pseudo_box_add_edge_spacers(box, ps);
    return box;
}

static int g_build_block_depth;

static const ns_node *
layout_shadow_root(const ns_node *host)
{
    if (!host || host->kind != NS_NODE_ELEMENT) return NULL;
    for (const ns_node *c = host->first_child; c; c = c->next_sibling)
        if (c->kind == NS_NODE_ELEMENT && ns_element_get_attr(c, NS_SHADOW_ATTR))
            return c;
    return NULL;
}

static const ns_node *
layout_slot_host(const ns_node *slot)
{
    for (const ns_node *p = slot ? slot->parent : NULL; p; p = p->parent)
        if (p->kind == NS_NODE_ELEMENT && ns_element_get_attr(p, NS_SHADOW_ATTR))
            return p->parent;
    return NULL;
}

static const ns_node *
layout_find_slot(const ns_node *scope, const char *name)
{
    for (const ns_node *c = scope ? scope->first_child : NULL; c;
         c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || ns_element_get_attr(c, NS_SHADOW_ATTR))
            continue;
        if (c->name && strcmp(c->name, "slot") == 0) {
            const char *slot_name = ns_element_get_attr(c, "name");
            if (g_strcmp0(slot_name ? slot_name : "", name) == 0) return c;
        }
        const ns_node *found = layout_find_slot(c, name);
        if (found) return found;
    }
    return NULL;
}

static const ns_node *
layout_flat_parent(const ns_node *n)
{
    const ns_node *p = n ? n->parent : NULL;
    if (!p || n->kind != NS_NODE_ELEMENT) return p;
    const ns_node *sr = layout_shadow_root(p);
    if (!sr || sr == n) return p;
    const char *name = ns_element_get_attr(n, "slot");
    const ns_node *slot = layout_find_slot(sr, name ? name : "");
    return slot ? slot : p;
}

static ns_box *build_block_impl(const ns_node *n, GHashTable *styles);

static ns_box *
build_block(const ns_node *n, GHashTable *styles)
{
    if (!n || g_build_block_depth >= NS_LAYOUT_MAX_DEPTH) return NULL;
    g_build_block_depth++;
    ns_box *out = build_block_impl(n, styles);
    g_build_block_depth--;
    return out;
}

static int g_contents_depth;

static void
append_display_contents_children(ns_box *block, const ns_node *n,
                                 GHashTable *styles,
                                 gboolean blockify_children,
                                 ns_box **pending_before)
{
    if (g_contents_depth >= NS_LAYOUT_MAX_DEPTH) return;
    g_contents_depth++;
    const ns_style *s = g_hash_table_lookup(styles, n);
    ns_box *contents_before = (s && s->before &&
                               !style_is_absolute_or_fixed(s->before))
        ? build_pseudo_inline_for(s->before, n) : NULL;
    if (pending_before && *pending_before) {
        contents_before = inline_merge_prefix(*pending_before, contents_before);
        *pending_before = NULL;
    }
    if (contents_before)
        box_append_child(block, contents_before);

    const ns_node *shadow_root = layout_shadow_root(n);
    const ns_node *c = shadow_root ? shadow_root->first_child : n->first_child;
    while (c) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            ns_layout_tag_is_non_rendering(c->name)) {
            c = c->next_sibling;
            continue;
        }
        if (blockify_children) {
            if (c->kind == NS_NODE_TEXT) {
                if (text_is_ws_only(c->text)) { c = c->next_sibling; continue; }
                ns_box *item = box_new(NS_BOX_BLOCK);
                item->style = NULL;
                ns_box *run = build_inline_run(c, c->next_sibling, styles);
                if (run && run->text && run->text[0]) {
                    box_append_child(item, run);
                    box_append_child(block, item);
                } else {
                    if (run) ns_box_free(run);
                    ns_box_free(item);
                }
                c = c->next_sibling;
                continue;
            }
            if (c->kind != NS_NODE_ELEMENT) { c = c->next_sibling; continue; }
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (cs && style_is_none(cs)) { c = c->next_sibling; continue; }
            if (style_is_contents(cs)) {
                append_display_contents_children(block, c, styles, TRUE, NULL);
                c = c->next_sibling;
                continue;
            }
            if (cs && style_is_absolute_or_fixed(cs)) {
                ns_box *child = build_block(c, styles);
                if (child) box_append_child(block, child);
                c = c->next_sibling;
                continue;
            }
            if (style_is_block(cs) ||
                contains_block_media(c, styles) ||
                node_has_media_metadata(c) ||
                (c->name && (strcmp(c->name, "img") == 0 ||
                             strcmp(c->name, "svg") == 0 ||
                             strcmp(c->name, "audio") == 0 ||
                             strcmp(c->name, "video") == 0 ||
                             strcmp(c->name, "table") == 0))) {
                ns_box *child = build_block(c, styles);
                if (child) box_append_child(block, child);
                c = c->next_sibling;
                continue;
            }
            ns_box *item = build_blockified_inline_item(c, styles, pending_before);
            if (item) box_append_child(block, item);
            c = c->next_sibling;
            continue;
        }

        if (c->kind == NS_NODE_ELEMENT) {
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (cs && style_is_none(cs)) { c = c->next_sibling; continue; }
            if (style_is_contents(cs)) {
                append_display_contents_children(block, c, styles, FALSE, NULL);
                c = c->next_sibling;
                continue;
            }
        }
        if (is_inline_dom(c, styles)) {
            const ns_node *start = c;
            c = c->next_sibling;
            while (c) {
                if (c->kind == NS_NODE_ELEMENT && c->name &&
                    ns_layout_tag_is_non_rendering(c->name)) {
                    c = c->next_sibling;
                    continue;
                }
                if (c->kind == NS_NODE_ELEMENT) {
                    const ns_style *cs = g_hash_table_lookup(styles, c);
                    if (style_is_contents(cs)) break;
                }
                if (!continues_inline_run(c, styles)) break;
                c = c->next_sibling;
            }
            ns_box *run = build_inline_run(start, c, styles);
            if (run && run->text && run->text[0])
                box_append_child(block, run);
            else if (run)
                ns_box_free(run);
        } else {
            ns_box *child = build_block(c, styles);
            if (child) box_append_child(block, child);
            if (c) c = c->next_sibling;
        }
    }

    if (s && s->after && !style_is_absolute_or_fixed(s->after)) {
        ns_box *contents_after = build_pseudo_inline_for(s->after, n);
        if (contents_after) box_append_child(block, contents_after);
    }
    g_contents_depth--;
}

static ns_box *
pseudo_item_block(ns_box *run, const ns_style *ps)
{
    ns_box *item = box_new(NS_BOX_BLOCK);
    item->style = ps;
    collect_box_bg_image(item, ps);
    for (guint i = run->attrs ? run->attrs->len : 0; i-- > 0;)
        if (g_array_index(run->attrs, ns_inline_attr, i).kind ==
            NS_INLINE_BG_COLOR)
            g_array_remove_index(run->attrs, i);
    box_append_child(item, run);
    return item;
}

static ns_box *
build_pseudo_block_for(const ns_style *ps, const ns_node *host)
{
    if (!ps) return NULL;
    if (style_is_absolute_or_fixed(ps)) return NULL;
    const ns_css_value *cv = ps->values[NS_CSS_CONTENT];
    if (!cv || cv->kind != NS_CSS_V_KEYWORD || !cv->u.keyword) return NULL;
    ns_display d = ns_css_display_of(ps);
    if (!ps->values[NS_CSS_DISPLAY] || ns_display_is_none(d) ||
        d.outer == NS_DISPLAY_OUTER_INLINE)
        return NULL;
    char *resolved = *cv->u.keyword
        ? ns_layout_resolve_pseudo_content(cv->u.keyword, host) : g_strdup("");
    if (!resolved) return NULL;
    char *quote = NULL;
    const char *txt = resolved;
    if (strcmp(txt, "open-quote") == 0)
        txt = quote = ns_layout_quotes_string_for(ps, 0, FALSE);
    else if (strcmp(txt, "close-quote") == 0)
        txt = quote = ns_layout_quotes_string_for(ps, 0, TRUE);
    else if (strcmp(txt, "no-open-quote") == 0 ||
             strcmp(txt, "no-close-quote") == 0)
        txt = "";
    ns_box *pb = pseudo_block_with_text(ps, txt);
    g_free(quote);
    g_free(resolved);
    return pb;
}

static void
register_abs_pseudo(const ns_node *host, const ns_style *ps)
{
    if (!ps || !g_abs_pending) return;
    if (!style_is_absolute_or_fixed(ps)) return;
    const ns_css_value *cv = ps->values[NS_CSS_CONTENT];
    if (!cv || cv->kind != NS_CSS_V_KEYWORD || !cv->u.keyword) return;
    ns_abs_entry e;
    e.dom = host;
    e.pseudo = ps;
    const ns_css_value *pv = ps->values[NS_CSS_POSITION];
    e.fixed = pv && pv->kind == NS_CSS_V_KEYWORD && pv->u.keyword &&
              strcmp(pv->u.keyword, "fixed") == 0;
    g_array_append_val(g_abs_pending, e);
}

static ns_box *
build_blockified_inline_item(const ns_node *n, GHashTable *styles,
                             ns_box **pending_before)
{
    const ns_style *s = g_hash_table_lookup(styles, n);
    ns_box *item = box_new(NS_BOX_BLOCK);
    item->dom = n;
    item->style = s;
    collect_box_bg_image(item, s);

    if (s) {
        register_abs_pseudo(n, s->before);
        register_abs_pseudo(n, s->after);
    }

    ns_box *before_block = (s && s->before)
        ? build_pseudo_block_for(s->before, n) : NULL;
    if (before_block) box_append_child(item, before_block);
    ns_box *after_block = (s && s->after)
        ? build_pseudo_block_for(s->after, n) : NULL;

    const ns_node *saved_host = g_pseudo_blocks_host;
    gboolean saved_before = g_pseudo_block_before;
    gboolean saved_after = g_pseudo_block_after;
    g_pseudo_blocks_host = n;
    g_pseudo_block_before = before_block != NULL;
    g_pseudo_block_after = after_block != NULL;
    ns_box *run = build_inline_run_no_abs_placeholders(n, n->next_sibling,
                                                       styles);
    g_pseudo_blocks_host = saved_host;
    g_pseudo_block_before = saved_before;
    g_pseudo_block_after = saved_after;
    if (pending_before && *pending_before) {
        run = inline_merge_prefix(*pending_before, run);
        *pending_before = NULL;
    }
    if (run && run->text && run->text[0]) {
        box_append_child(item, run);
    } else if (run) {
        ns_box_free(run);
    }

    if (after_block) box_append_child(item, after_block);

    if (!item->first_child && !style_has_atomic_inline_box(s)) {
        ns_box_free(item);
        return NULL;
    }
    return item;
}

static const ns_node *g_blockified_legend;

static gboolean
style_can_be_rendered_legend(const ns_style *s)
{
    return s && !style_is_none(s) && ns_layout_float_side_of(s) < 0 &&
           !style_is_absolute_or_fixed(s);
}

static const ns_node *
fieldset_rendered_legend_node(const ns_node *n, GHashTable *styles)
{
    if (!ns_node_is_element_named(n, "fieldset")) return NULL;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
        if (!ns_node_is_element_named(c, "legend")) continue;
        if (style_can_be_rendered_legend(g_hash_table_lookup(styles, c)))
            return c;
    }
    return NULL;
}

static ns_box *
build_rendered_legend(const ns_node *n, GHashTable *styles)
{
    const ns_node *saved = g_blockified_legend;
    g_blockified_legend = n;
    ns_box *legend = build_block(n, styles);
    g_blockified_legend = saved;
    if (!legend) {
        legend = box_new(NS_BOX_BLOCK);
        legend->dom = n;
        legend->style = g_hash_table_lookup(styles, n);
    }
    legend->is_rendered_legend = TRUE;
    return legend;
}

static ns_box *
build_block_impl(const ns_node *n, GHashTable *styles)
{
    if (!n) return NULL;
    if (n->kind == NS_NODE_DOCUMENT) {
        const char *saved_base = g_base_url_for_layout;
        if (n->parent && n->parent->kind == NS_NODE_ELEMENT &&
            n->parent->name &&
            (strcmp(n->parent->name, "iframe") == 0 ||
             strcmp(n->parent->name, "frame") == 0 ||
             strcmp(n->parent->name, "object") == 0)) {
            const char *fu = ns_element_get_attr(n->parent,
                                                 "data-nd-frame-url");
            if (fu && *fu) g_base_url_for_layout = fu;
        }
        ns_box *root = box_new(NS_BOX_BLOCK);
        for (const ns_node *c = n->first_child; c; c = c->next_sibling) {
            const ns_style *cs = c->kind == NS_NODE_ELEMENT
                ? g_hash_table_lookup(styles, c) : NULL;
            if (style_is_contents(cs)) {
                append_display_contents_children(root, c, styles, FALSE, NULL);
                continue;
            }
            ns_box *child = build_block(c, styles);
            if (child) box_append_child(root, child);
        }
        g_base_url_for_layout = saved_base;
        return root;
    }
    if (n->kind != NS_NODE_ELEMENT) return NULL;
    if (node_is_frame_fallback(n)) return NULL;
    if (ns_element_get_attr(n, NS_SHADOW_ATTR)) return NULL;

    const ns_style *s = g_hash_table_lookup(styles, n);
    if (s && style_is_none(s)) return NULL;
    if (s && style_is_absolute_or_fixed(s)) {
        if (!g_abs_force_build) {
            if (g_abs_pending &&
                (!g_abs_seen || g_hash_table_add(g_abs_seen, (gpointer)n))) {
                ns_abs_entry e;
                e.dom = n;
                e.pseudo = NULL;
                const ns_css_value *pv = s->values[NS_CSS_POSITION];
                e.fixed = pv && pv->kind == NS_CSS_V_KEYWORD && pv->u.keyword &&
                          strcmp(pv->u.keyword, "fixed") == 0;
                g_array_append_val(g_abs_pending, e);
            }
            return NULL;
        }
        g_abs_force_build = FALSE;
    }

    if (n->name && strcmp(n->name, "br") == 0) {
        if (!ns_layout_clear_kind_of(s)) return NULL;
        ns_box *clearance = box_new(NS_BOX_BLOCK);
        clearance->dom = n;
        clearance->style = s;
        return clearance;
    }

    if (n->name && strcmp(n->name, "img") == 0) {
        ns_box *ib = build_image_box(n);
        if (ib) ib->style = s;
        return ib;
    }

    if (n->name && strcmp(n->name, "math") == 0) {
        ns_box *mb = box_new(NS_BOX_MATH);
        mb->dom = n;
        mb->style = s;
        return mb;
    }

    if (n->name && strcmp(n->name, "input") == 0) {
        if (s && (style_is_absolute_or_fixed(s) || style_is_block(s)))
            return build_form_control_block(n, s, styles);
        ns_box *ir = build_inline_run(n, n->next_sibling, styles);
        if (ir) return ir;
    }

    if (n->name && strcmp(n->name, "textarea") == 0) {
        if (s && (style_is_absolute_or_fixed(s) || style_is_block(s)))
            return build_form_control_block(n, s, styles);
        ns_box *ir = build_inline_run(n, n->next_sibling, styles);
        if (ir) return ir;
    }

    if (n->name && strcmp(n->name, "select") == 0) {
        if (s && (style_is_absolute_or_fixed(s) || style_is_block(s)))
            return build_form_control_block(n, s, styles);
        ns_box *ir = build_inline_run(n, n->next_sibling, styles);
        if (ir) return ir;
    }

    if (n->name && strcmp(n->name, "svg") == 0) {
        ns_box *box = box_new(NS_BOX_SVG);
        box->dom = n;
        box->style = s;
        box->svg_styles = styles;
        ns_box_media *m = ns_box_media_ensure(box);

        ns_svg_size size;
        ns_svg_intrinsic_size(n, &size);

        double css_w = -1, css_h = -1;
        if (s) {
            const ns_css_value *wv = s->values[NS_CSS_WIDTH];
            const ns_css_value *hv = s->values[NS_CSS_HEIGHT];
            if (wv && wv->kind == NS_CSS_V_LENGTH &&
                wv->u.length.unit == NS_CSS_UNIT_PX)
                css_w = wv->u.length.v;
            if (hv && hv->kind == NS_CSS_V_LENGTH &&
                hv->u.length.unit == NS_CSS_UNIT_PX)
                css_h = hv->u.length.v;
        }

        double w = css_w > 0 ? css_w : (size.has_width  ? size.width  : 0);
        double h = css_h > 0 ? css_h : (size.has_height ? size.height : 0);
        if (w > 0 && h <= 0 && size.has_ratio) h = w / size.ratio;
        else if (h > 0 && w <= 0 && size.has_ratio) w = h * size.ratio;

        gboolean root_document = !n->parent || n->parent->kind == NS_NODE_DOCUMENT;
        if (root_document) {
            double vw = ns_css_viewport_w();
            double vh = ns_css_viewport_h();
            if (w <= 0 && vw > 0) w = vw;
            if (h <= 0 && vh > 0) h = vh;
        }

        if (w <= 0 && h <= 0 && size.has_ratio && !root_document) {
            m->intrinsic_ratio_only = TRUE;
            w = 0;
            h = 0;
        } else if (w <= 0 && h <= 0) { w = 300; h = 150; }
        else if (w <= 0) w = 300;
        else if (h <= 0) h = 150;

        box->content_width  = w;
        box->content_height = h;
        m->declared_image_size = w > 0 && h > 0;
        return box;
    }

    if (n->name && strcmp(n->name, "canvas") == 0) {
        ns_box *cb = box_new(NS_BOX_IMAGE);
        cb->dom = n;
        cb->style = s;
        ns_box_media *cm = ns_box_media_ensure(cb);
        cb->content_width  = image_dimension_attr(n, "width");
        cb->content_height = image_dimension_attr(n, "height");
        if (cb->content_width  <= 0) cb->content_width  = 300;
        if (cb->content_height <= 0) cb->content_height = 150;
        cm->declared_image_size = TRUE;
        return cb;
    }

    if (n->name && strcmp(n->name, "audio") == 0) {
        ns_box *ab = build_audio_box(n);
        if (ab) ab->style = s;
        return ab;
    }

    if (n->name && strcmp(n->name, "video") == 0) {
        ns_box *vb = build_video_box(n);
        if (vb) {
            vb->style = s;
            collect_box_bg_image(vb, s);
        }
        return vb;
    }

    if (node_has_media_metadata(n)) {
        gboolean has_src = ns_element_get_attr(n, NS_MEDIA_SRC_ATTR) != NULL;
        gboolean empty_skeleton = TRUE;
        for (const ns_node *mc = n->first_child; mc; mc = mc->next_sibling)
            if (mc->kind == NS_NODE_ELEMENT) { empty_skeleton = FALSE; break; }
        if (has_src || empty_skeleton) {
            ns_box *vb = build_video_box(n);
            if (vb) {
                vb->style = s;
                collect_box_bg_image(vb, s);
            }
            return vb;
        }
    }

    if (is_table_box(n, styles))
        return build_table(n, styles);

    if (n->name && strcmp(n->name, "slot") == 0) {
        const ns_node *host = layout_slot_host(n);
        const ns_node *sr = host ? layout_shadow_root(host) : NULL;
        const char *slot_name = ns_element_get_attr(n, "name");
        ns_box *sbox = box_new(NS_BOX_BLOCK);
        sbox->dom = n;
        sbox->style = s;
        gboolean any = FALSE;
        if (host && sr) {
            for (const ns_node *lc = host->first_child; lc; lc = lc->next_sibling) {
                if (lc == sr) continue;
                const char *cs = (lc->kind == NS_NODE_ELEMENT)
                    ? ns_element_get_attr(lc, "slot") : NULL;
                if (g_strcmp0(cs ? cs : "", slot_name ? slot_name : "") != 0)
                    continue;
                ns_box *cb;
                if (is_inline_dom(lc, styles)) {
                    cb = build_inline_run(lc, lc->next_sibling, styles);
                    if (cb && !(cb->text && cb->text[0])) {
                        ns_box_free(cb);
                        cb = NULL;
                    }
                } else {
                    cb = build_block(lc, styles);
                }
                if (cb) { box_append_child(sbox, cb); any = TRUE; }
            }
        }
        if (any) return sbox;
        ns_box_free(sbox);
    }

    if (!style_is_block(s) && !contains_block_media(n, styles) &&
        !style_is_absolute_or_fixed(s) && n != g_blockified_legend) return NULL;

    ns_box *block = box_new(NS_BOX_BLOCK);
    block->dom = n;
    block->style = s;
    const ns_node *rendered_legend = fieldset_rendered_legend_node(n, styles);

    collect_box_bg_image(block, s);

    gboolean details_collapsed = FALSE;
    if (n->name && strcmp(n->name, "details") == 0 &&
        !ns_element_get_attr(n, "open"))
        details_collapsed = TRUE;

    if (s) {
        register_abs_pseudo(n, s->before);
        register_abs_pseudo(n, s->after);
    }

    ns_box *before_block = (s && s->before)
        ? build_pseudo_block_for(s->before, n) : NULL;
    if (before_block) box_append_child(block, before_block);

    ns_box *pending_before = (s && s->before && !before_block &&
                              !style_is_absolute_or_fixed(s->before))
        ? build_pseudo_inline_for(s->before, n) : NULL;

    gboolean blockify_children = style_is_flex_container(s) ||
                                 style_is_grid_container(s);
    if (blockify_children && pending_before) {
        box_append_child(block, pseudo_item_block(pending_before, s->before));
        pending_before = NULL;
    }

    const ns_node *saved_skip = g_inline_skip_node;
    if (rendered_legend) {
        ns_box *legend = build_rendered_legend(rendered_legend, styles);
        if (legend) box_append_child(block, legend);
        g_inline_skip_node = rendered_legend;
    }

    const ns_node *shadow_host_root = layout_shadow_root(n);
    const ns_node *c = shadow_host_root ? shadow_host_root->first_child
                                        : n->first_child;
    while (c) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            ns_layout_tag_is_non_rendering(c->name)) {
            c = c->next_sibling;
            continue;
        }
        if (details_collapsed) {
            if (c->kind != NS_NODE_ELEMENT || !c->name ||
                strcmp(c->name, "summary") != 0) {
                c = c->next_sibling;
                continue;
            }
        }
        if (c == rendered_legend) {
            c = c->next_sibling;
            continue;
        }
        if (blockify_children) {
            if (c->kind == NS_NODE_TEXT) {
                if (text_is_ws_only(c->text)) { c = c->next_sibling; continue; }
                ns_box *item = box_new(NS_BOX_BLOCK);
                item->style = NULL;
                ns_box *run = build_inline_run(c, c->next_sibling, styles);
                if (pending_before) {
                    run = inline_merge_prefix(pending_before, run);
                    pending_before = NULL;
                }
                if (run && run->text && run->text[0]) {
                    box_append_child(item, run);
                    box_append_child(block, item);
                } else {
                    if (run) ns_box_free(run);
                    ns_box_free(item);
                }
                c = c->next_sibling;
                continue;
            }
            if (c->kind != NS_NODE_ELEMENT) { c = c->next_sibling; continue; }
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (cs && style_is_none(cs)) { c = c->next_sibling; continue; }
            if (style_is_contents(cs)) {
                append_display_contents_children(block, c, styles, TRUE,
                                                 &pending_before);
                c = c->next_sibling;
                continue;
            }
            if (cs && style_is_absolute_or_fixed(cs)) {
                ns_box *child = build_block(c, styles);
                if (child) box_append_child(block, child);
                c = c->next_sibling;
                continue;
            }
            if (style_is_block(cs) ||
                contains_block_media(c, styles) ||
                node_has_media_metadata(c) ||
                (c->name && (strcmp(c->name, "img") == 0 ||
                             strcmp(c->name, "svg") == 0 ||
                             strcmp(c->name, "audio") == 0 ||
                             strcmp(c->name, "video") == 0 ||
                             strcmp(c->name, "table") == 0))) {
                ns_box *child = build_block(c, styles);
                if (child) box_append_child(block, child);
                c = c->next_sibling;
                continue;
            }
            ns_box *item = build_blockified_inline_item(c, styles,
                                                        &pending_before);
            if (item) box_append_child(block, item);
            c = c->next_sibling;
            continue;
        }
        if (c->kind == NS_NODE_ELEMENT) {
            const ns_style *cs = g_hash_table_lookup(styles, c);
            if (style_is_contents(cs)) {
                append_display_contents_children(block, c, styles, FALSE,
                                                 &pending_before);
                c = c->next_sibling;
                continue;
            }
        }
        if (node_is_table_internal(c, styles)) {
            const ns_node *start = c;
            while (c && (node_is_table_internal(c, styles) ||
                         (c->kind == NS_NODE_TEXT && text_is_ws_only(c->text))))
                c = c->next_sibling;
            if (pending_before) {
                box_append_child(block, pending_before);
                pending_before = NULL;
            }
            ns_box *anon = build_anonymous_table(start, c, styles);
            if (anon) box_append_child(block, anon);
            continue;
        }
        if (is_inline_dom(c, styles)) {
            const ns_node *start = c;
            c = c->next_sibling;
            while (c) {
                if (c->kind == NS_NODE_ELEMENT && c->name &&
                    ns_layout_tag_is_non_rendering(c->name)) {
                    c = c->next_sibling;
                    continue;
                }
                if (c == rendered_legend) {
                    c = c->next_sibling;
                    continue;
                }
                if (details_collapsed &&
                    (c->kind != NS_NODE_ELEMENT || !c->name ||
                     strcmp(c->name, "summary") != 0)) break;
                if (c->kind == NS_NODE_ELEMENT) {
                    const ns_style *cs = g_hash_table_lookup(styles, c);
                    if (style_is_contents(cs)) break;
                }
                if (!continues_inline_run(c, styles)) break;
                c = c->next_sibling;
            }
            ns_box *run = build_inline_run(start, c, styles);
            if (pending_before) {
                run = inline_merge_prefix(pending_before, run);
                pending_before = NULL;
            }

            if (run->text && run->text[0] != '\0')
                box_append_child(block, run);
            else
                ns_box_free(run);
        } else {
            if (pending_before) {
                box_append_child(block, pending_before);
                pending_before = NULL;
            }
            ns_box *child = build_block(c, styles);
            if (child) box_append_child(block, child);
            if (c) c = c->next_sibling;
        }
    }
    g_inline_skip_node = saved_skip;

    if (pending_before) {
        box_append_child(block, pending_before);
        pending_before = NULL;
    }

    ns_box *after_block = (s && s->after)
        ? build_pseudo_block_for(s->after, n) : NULL;
    if (after_block) box_append_child(block, after_block);

    if (s && s->after && !after_block &&
        !style_is_absolute_or_fixed(s->after)) {
        ns_box *gen = build_pseudo_inline_for(s->after, n);
        if (gen && blockify_children) {
            box_append_child(block, pseudo_item_block(gen, s->after));
            gen = NULL;
        }
        if (gen) append_generated_after(block, gen);
    }
    return block;
}

static NsPangoLayout *
make_pango_layout(const ns_style *parent_style)
{
    NsPangoLayout *layout = ns_pango_layout_new(ns_paint_text_context());
    ns_paint_apply_inline_font(layout, parent_style);
    return layout;
}

static gboolean
box_first_baseline(const ns_box *b, double *out)
{
    if (!b) return FALSE;
    if (b->kind == NS_BOX_INLINE) {
        if (b->first_baseline <= 0) return FALSE;
        *out = b->margin.top + b->border.top + b->padding.top +
               b->first_baseline;
        return TRUE;
    }
    for (const ns_box *c = b->first_child; c; c = c->next_sibling) {
        if (style_is_absolute_or_fixed(c->style) || c->is_rendered_legend)
            continue;
        double child_baseline;
        if (box_first_baseline(c, &child_baseline)) {
            *out = (c->y - b->y) + child_baseline;
            return TRUE;
        }
    }
    return FALSE;
}

static const char *
flex_item_align(const ns_box *c, const char *container_align)
{
    if (c && c->style) {
        const char *as = ns_style_keyword(c->style, NS_CSS_ALIGN_SELF);
        if (as && strcmp(as, "auto") != 0) return as;
    }
    return container_align;
}

static gboolean
flex_align_is_baseline(const char *align)
{
    return align && (strcmp(align, "baseline") == 0 ||
                     strcmp(align, "first baseline") == 0);
}

static double
flex_item_baseline(const ns_box *c, double fallback)
{
    if (c->parent && ns_css_writing_mode(c->parent->style) &&
        ns_css_writing_mode(c->style)) {
        double border_h = c->content_height + c->padding.top +
                          c->padding.bottom + c->border.top + c->border.bottom;
        return c->margin.top + border_h / 2.0;
    }
    double baseline;
    return box_first_baseline(c, &baseline) ? baseline : fallback;
}

static gboolean
inline_attr_is_form_hit(ns_inline_attr_kind k)
{
    return k == NS_INLINE_INPUT_FIELD ||
           k == NS_INLINE_INPUT_FIELD_FOCUSED ||
           k == NS_INLINE_BUTTON ||
           k == NS_INLINE_CHECKBOX ||
           k == NS_INLINE_CHECKBOX_CHECKED ||
           k == NS_INLINE_RADIO ||
           k == NS_INLINE_RADIO_CHECKED;
}

static gboolean
inline_attr_is_button_hit(ns_inline_attr_kind k)
{
    return k == NS_INLINE_BUTTON ||
           k == NS_INLINE_CHECKBOX ||
           k == NS_INLINE_CHECKBOX_CHECKED ||
           k == NS_INLINE_RADIO ||
           k == NS_INLINE_RADIO_CHECKED;
}

static const ns_node *
inline_box_form_hit(const ns_box *box, double local_x, double local_y,
                    const ns_style *parent_style)
{
    if (!box) return NULL;
    if ((!box->attrs || box->attrs->len == 0) &&
        (!box->inline_atomics || box->inline_atomics->len == 0))
        return NULL;
    if (!box->text || !*box->text) return NULL;
    NsPangoLayout *layout = make_pango_layout(parent_style);
    ns_pango_layout_set_width(layout, (int)(box->content_width * NS_PANGO_SCALE));
    ns_pango_layout_set_wrap(layout, ns_paint_wrap_mode_for(parent_style));
    if (!(box->inline_atomics && box->inline_atomics->len > 0))
        ns_paint_apply_css_line_spacing(layout, parent_style);
    {
        double ti = ns_inline_text_indent_px(box, parent_style, box->content_width);
        if (ti > 0) ns_pango_layout_set_indent(layout, (int)(ti * NS_PANGO_SCALE));
    }
    if (keyword_is(parent_style ? parent_style->values[NS_CSS_TEXT_OVERFLOW] : NULL,
                   "ellipsis"))
        ns_pango_layout_set_ellipsize(layout, NS_PANGO_ELLIPSIZE_END);
    ns_pango_layout_set_text(layout, box->text, -1);
    NsPangoAttrList *i18n = ns_pango_attr_list_new();
    ns_paint_apply_i18n(layout, i18n, box);
    ns_paint_apply_font_features(i18n, parent_style, 0, G_MAXUINT);
    ns_inline_apply_atomic_shapes(i18n, box);
    ns_layout_apply_inline_spacing(i18n, parent_style, box->text);
    ns_layout_apply_inline_layout_attrs(i18n, box);
    ns_inline_layout_set_attrs(layout, i18n, box);
    ns_pango_attr_list_unref(i18n);
    const ns_css_value *ta_v =
        parent_style ? parent_style->values[NS_CSS_TEXT_ALIGN] : NULL;
    gboolean rtl = ns_pango_context_get_base_dir(
        ns_pango_layout_get_context(layout)) == NS_PANGO_DIRECTION_RTL;
    if (keyword_is(ta_v, "center"))
        ns_pango_layout_set_alignment(layout, NS_PANGO_ALIGN_CENTER);
    else if (keyword_is(ta_v, "right") ||
             (keyword_is(ta_v, "end") && !rtl) ||
             (keyword_is(ta_v, "start") && rtl) ||
             (!ta_v && rtl))
        ns_pango_layout_set_alignment(layout, NS_PANGO_ALIGN_RIGHT);
    else if (keyword_is(ta_v, "justify"))
        ns_pango_layout_set_justify(layout, TRUE);
    else
        ns_pango_layout_set_alignment(layout, NS_PANGO_ALIGN_LEFT);
    ns_paint_start_align_overflow(layout);

    int index = 0, trailing = 0;
    gboolean inside = ns_pango_layout_xy_to_index(
        layout,
        (int)(local_x * NS_PANGO_SCALE),
        (int)(local_y * NS_PANGO_SCALE),
        &index, &trailing);
    const ns_node *button_hit = NULL;
    const ns_node *field_hit = NULL;
    const ns_node *atomic_hit = NULL;
    if (inside && index >= 0) {
        gsize idx = (gsize)index;
        if (box->inline_atomics) {
            for (guint i = 0; i < box->inline_atomics->len; i++) {
                const ns_inline_atomic *a =
                    &g_array_index(box->inline_atomics, ns_inline_atomic, i);
                if (!a->box || idx < a->byte_off || idx >= a->byte_off + 3)
                    continue;
                if (ns_layout_node_is_form_hit_target(a->box->dom))
                    atomic_hit = a->box->dom;
            }
        }
        if (box->attrs) {
            for (guint i = 0; i < box->attrs->len; i++) {
                const ns_inline_attr *r =
                    &g_array_index(box->attrs, ns_inline_attr, i);
                if (!inline_attr_is_form_hit(r->kind)) continue;
                if (!r->dom) continue;
                const ns_style *rs = r->style ? r->style : parent_style;
                if (ns_layout_style_blocks_hit_testing(rs)) continue;
                if (idx < r->start || idx >= r->start + r->len) continue;
                if (inline_attr_is_button_hit(r->kind)) button_hit = r->dom;
                else if (!field_hit)             field_hit = r->dom;
            }
        }
    }
    if (box->attrs) {
        for (guint i = 0; i < box->attrs->len; i++) {
            const ns_inline_attr *r =
                &g_array_index(box->attrs, ns_inline_attr, i);
            if (!inline_attr_is_form_hit(r->kind)) continue;
            if (!r->dom) continue;
            const ns_style *rs = r->style ? r->style : parent_style;
            if (ns_layout_style_blocks_hit_testing(rs)) continue;
            NsPangoRectangle r0, r1;
            ns_pango_layout_index_to_pos(layout, (int)r->start, &r0);
            ns_pango_layout_index_to_pos(layout, (int)(r->start + r->len - 1), &r1);
            double bleed_x = r->box_w > 0 || r->box_h > 0 ? 0 : 10;
            double bleed_y = r->box_w > 0 || r->box_h > 0 ? 0 : 5;
            double x0 = (double)r0.x / NS_PANGO_SCALE - bleed_x;
            double y0 = (double)r0.y / NS_PANGO_SCALE - bleed_y;
            double x1 = (double)(r1.x + r1.width) / NS_PANGO_SCALE + bleed_x;
            double y1 = (double)(r0.y + r0.height) / NS_PANGO_SCALE + bleed_y;
            double css_w = ns_layout_inline_attr_control_width(r, box);
            if (css_w > 0) {
                x0 = (double)r0.x / NS_PANGO_SCALE;
                x1 = x0 + css_w;
            }
            if (r->box_h > 0) {
                double cy = (y0 + y1) / 2.0;
                y0 = cy - r->box_h / 2.0;
                y1 = cy + r->box_h / 2.0;
            }
            if (local_x < x0 || local_x > x1 || local_y < y0 || local_y > y1)
                continue;
            if (inline_attr_is_button_hit(r->kind)) button_hit = r->dom;
            else if (!field_hit)             field_hit = r->dom;
        }
    }
    g_object_unref(layout);
    if (atomic_hit) return atomic_hit;
    if (button_hit) return button_hit;
    return field_hit;
}

static gboolean
inline_attr_can_fragment(ns_inline_attr_kind k)
{
    switch (k) {
    case NS_INLINE_INPUT_FIELD:
    case NS_INLINE_INPUT_FIELD_FOCUSED:
    case NS_INLINE_BUTTON:
    case NS_INLINE_CHECKBOX:
    case NS_INLINE_CHECKBOX_CHECKED:
    case NS_INLINE_RADIO:
    case NS_INLINE_RADIO_CHECKED:
    case NS_INLINE_PROGRESS:
    case NS_INLINE_METER:
    case NS_INLINE_CARET:
    case NS_INLINE_SELECTION:
        return FALSE;
    default:
        return TRUE;
    }
}

static gboolean
inline_box_can_fragment_multicol(const ns_box *box, const ns_style *style,
                                 double basis)
{
    if (!box || box->kind != NS_BOX_INLINE || !box->text || !*box->text)
        return FALSE;
    if (box->inline_atomics && box->inline_atomics->len > 0)
        return FALSE;
    if (keyword_is(style ? style->values[NS_CSS_WHITE_SPACE] : NULL, "nowrap") ||
        keyword_is(style ? style->values[NS_CSS_WHITE_SPACE] : NULL, "pre"))
        return FALSE;
    if (keyword_is(style ? style->values[NS_CSS_TEXT_OVERFLOW] : NULL, "ellipsis"))
        return FALSE;
    if (style && style->values[NS_CSS_LINE_CLAMP])
        return FALSE;
    if (fabs(ns_text_indent_px(style, basis)) > 0.01)
        return FALSE;
    if (box->attrs) {
        for (guint i = 0; i < box->attrs->len; i++) {
            const ns_inline_attr *a =
                &g_array_index(box->attrs, ns_inline_attr, i);
            if (!inline_attr_can_fragment(a->kind)) return FALSE;
        }
    }
    return TRUE;
}

static gsize
range_end_clamped(gsize start, gsize len, gsize cap)
{
    gsize end = len > G_MAXSIZE - start ? G_MAXSIZE : start + len;
    return end > cap ? cap : end;
}

static ns_box *
inline_box_clone_range(const ns_box *src, gsize start, gsize end)
{
    gsize text_len = src && src->text ? strlen(src->text) : 0;
    if (!src || start >= text_len || end <= start) return NULL;
    if (end > text_len) end = text_len;

    ns_box *out = box_new_inline();
    out->dom = src->dom;
    out->style = src->style;
    out->text = g_strndup(src->text + start, end - start);

    if (src->attrs) {
        for (guint i = 0; i < src->attrs->len; i++) {
            const ns_inline_attr *a =
                &g_array_index(src->attrs, ns_inline_attr, i);
            gsize a_end = range_end_clamped(a->start, a->len, text_len);
            gsize is = MAX(a->start, start);
            gsize ie = MIN(a_end, end);
            if (ie <= is) continue;
            ns_inline_attr copy = *a;
            copy.start = is - start;
            copy.len = ie - is;
            g_array_append_val(out->attrs, copy);
            if (copy.bg_image_src) {
                ns_box_media *m = ns_box_media_ensure(out);
                if (!m->bg_image_src) {
                    m->bg_image_src = g_strdup(copy.bg_image_src);
                    m->bg_image = copy.bg_image;
                }
            }
        }
    }

    if (src->links) {
        for (guint i = 0; i < src->links->len; i++) {
            const ns_link_range *r =
                &g_array_index(src->links, ns_link_range, i);
            gsize r_end = range_end_clamped(r->start, r->len, text_len);
            gsize is = MAX(r->start, start);
            gsize ie = MIN(r_end, end);
            if (ie <= is) continue;
            ns_link_range copy = {
                .start = is - start,
                .len = ie - is,
                .href = r->href ? g_strdup(r->href) : NULL,
                .target = r->target ? g_strdup(r->target) : NULL,
                .dom = r->dom,
            };
            g_array_append_val(inline_links_ensure(out), copy);
        }
    }

    return out;
}

static NsPangoLayout *
inline_box_layout_for_multicol(const ns_box *box, double content_width,
                               const ns_style *parent_style)
{
    NsPangoLayout *layout = make_pango_layout(parent_style);
    ns_pango_layout_set_width(layout, (int)(content_width * NS_PANGO_SCALE));
    ns_pango_layout_set_wrap(layout, ns_paint_wrap_mode_for(parent_style));
    if (!(box->inline_atomics && box->inline_atomics->len > 0))
        ns_paint_apply_css_line_spacing(layout, parent_style);
    ns_pango_layout_set_text(layout, box->text, -1);

    NsPangoAttrList *i18n = ns_pango_attr_list_new();
    ns_paint_apply_i18n(layout, i18n, box);
    ns_paint_apply_font_features(i18n, parent_style, 0, G_MAXUINT);
    ns_layout_apply_inline_spacing(i18n, parent_style, box->text);
    ns_layout_apply_inline_layout_attrs(i18n, box);
    ns_inline_layout_set_attrs(layout, i18n, box);
    ns_pango_attr_list_unref(i18n);
    return layout;
}

static gboolean
layout_multicol_single_inline(ns_box *box, double inner_x, double inner_y,
                              double col_w, double col_gap, int n_cols,
                              const ns_style *child_inherited,
                              double *cursor_y)
{
    ns_box *src = box ? box->first_child : NULL;
    if (!src || src->next_sibling || n_cols < 2)
        return FALSE;
    if (!inline_box_can_fragment_multicol(src, child_inherited, col_w))
        return FALSE;

    NsPangoLayout *layout = inline_box_layout_for_multicol(src, col_w,
                                                         child_inherited);
    int line_count = ns_pango_layout_get_line_count(layout);
    if (line_count <= 1) {
        g_object_unref(layout);
        return FALSE;
    }

    GPtrArray *fragments = g_ptr_array_new();
    int per_col = line_count / n_cols + (line_count % n_cols != 0);
    gsize text_len = strlen(src->text);
    for (int col = 0; col < n_cols; col++) {
        int first_line = col * per_col;
        int after_line = first_line + per_col;
        if (first_line >= line_count) break;
        if (after_line > line_count) after_line = line_count;
        NsPangoLayoutLine *first =
            ns_pango_layout_get_line_readonly(layout, first_line);
        NsPangoLayoutLine *after =
            after_line < line_count
            ? ns_pango_layout_get_line_readonly(layout, after_line) : NULL;
        if (!first) continue;
        gsize start = (gsize)first->start_index;
        gsize end = after ? (gsize)after->start_index : text_len;
        ns_box *frag = inline_box_clone_range(src, start, end);
        if (!frag) continue;
        frag->x = inner_x + col * (col_w + col_gap);
        frag->y = inner_y;
        ns_layout_box(frag, col_w, child_inherited);
        g_ptr_array_add(fragments, frag);
    }
    g_object_unref(layout);

    if (fragments->len < 2) {
        for (guint i = 0; i < fragments->len; i++)
            ns_box_free(g_ptr_array_index(fragments, i));
        g_ptr_array_free(fragments, TRUE);
        return FALSE;
    }

    box->first_child = NULL;
    box->last_child = NULL;
    for (guint i = 0; i < fragments->len; i++) {
        ns_box *frag = g_ptr_array_index(fragments, i);
        frag->parent = NULL;
        frag->next_sibling = NULL;
        box_append_child(box, frag);
    }
    g_ptr_array_free(fragments, TRUE);
    src->parent = NULL;
    src->next_sibling = NULL;
    ns_box_free(src);

    double max_h = 0;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        if (c->content_height > max_h) max_h = c->content_height;
    }
    box->content_width = col_w * n_cols + col_gap * (n_cols - 1);
    *cursor_y = inner_y + max_h;
    return TRUE;
}

static gboolean
height_keyword_stretches(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           (strcmp(v->u.keyword, "stretch") == 0 ||
            strcmp(v->u.keyword, "-webkit-fill-available") == 0 ||
            strcmp(v->u.keyword, "-moz-available") == 0);
}

ns_box *
ns_layout_build(const ns_node *doc, GHashTable *styles, double viewport_width,
                const ns_node *focused_input, gsize focused_caret_byte,
                gsize focused_sel_anchor_byte,
                struct ns_image_cache *image_cache, const char *base_url)
{
    g_focused_input_for_layout = focused_input;
    g_focused_is_contenteditable_for_layout =
        focused_input && focused_input->kind == NS_NODE_ELEMENT &&
        focused_input->name &&
        strcmp(focused_input->name, "input") != 0 &&
        strcmp(focused_input->name, "textarea") != 0 &&
        ns_ce_attr_enables(ns_element_get_attr(focused_input,
                                               "contenteditable"));
    g_focused_caret_byte_for_layout = focused_caret_byte;
    g_focused_sel_anchor_byte_for_layout = focused_sel_anchor_byte;
    g_image_cache_for_layout = image_cache;
    g_base_url_for_layout = base_url;
    g_svg_defs_computed_for_layout = FALSE;
    ns_image_cache_begin_generation(image_cache);
    ns_layout_counters_build(doc, styles);
    ns_box *root = ns_layout_build_(doc, styles, viewport_width);
    if (!ns_layout_input_columns_active() &&
        ns_layout_input_columns_fit(root)) {
        ns_box_free(root);
        ns_image_cache_begin_generation(image_cache);
        root = ns_layout_build_(doc, styles, viewport_width);
        ns_layout_input_columns_clear();
    }
    ns_image_cache_collect(image_cache);
    g_focused_input_for_layout = NULL;
    g_focused_is_contenteditable_for_layout = FALSE;
    g_focused_caret_byte_for_layout = 0;
    g_focused_sel_anchor_byte_for_layout = 0;
    g_image_cache_for_layout = NULL;
    g_base_url_for_layout = NULL;
    g_svg_defs_computed_for_layout = FALSE;
    ns_layout_counters_clear();
    ns_layout_record_frame_viewports(root);
    return root;
}

static gboolean
style_creates_fixed_cb(const ns_style *s)
{
    if (!s) return FALSE;
    static const ns_css_prop tprops[4] = {
        NS_CSS_TRANSFORM, NS_CSS_TRANSLATE, NS_CSS_ROTATE, NS_CSS_SCALE,
    };
    for (int i = 0; i < 4; i++) {
        const ns_css_value *tv = s->values[tprops[i]];
        if (tv && tv->kind == NS_CSS_V_TRANSFORM && tv->u.transform.n_ops > 0)
            return TRUE;
    }
    const ns_css_value *pv = s->values[NS_CSS_PERSPECTIVE];
    if (pv && pv->kind == NS_CSS_V_LENGTH && pv->u.length.v > 0) return TRUE;
    return FALSE;
}

static void
translate_subtree(ns_box *box, double dx, double dy)
{
    if (!box || (dx == 0 && dy == 0)) return;
    ns_layout_shift_box_tree(box, dx, dy);
}

static ns_box *
ns_layout_build_(const ns_node *doc, GHashTable *styles, double viewport_width)
{
    g_abs_pending = g_array_new(FALSE, FALSE, sizeof(ns_abs_entry));
    g_abs_seen = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_abs_ph_set = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_abs_static = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                         NULL, g_free);
    g_contains_block_media_cache = g_hash_table_new(g_direct_hash, g_direct_equal);
    ns_layout_reset_container_queries();
    ns_css_set_container_dims(0, 0);
    ns_box *root = build_block(doc, styles);
    if (!root) {
        g_array_free(g_abs_pending, TRUE);
        g_abs_pending = NULL;
        g_clear_pointer(&g_abs_seen, g_hash_table_destroy);
        g_clear_pointer(&g_abs_ph_set, g_hash_table_destroy);
        g_clear_pointer(&g_abs_static, g_hash_table_destroy);
        g_hash_table_destroy(g_contains_block_media_cache);
        g_contains_block_media_cache = NULL;
        return NULL;
    }
    root->x = 0;
    root->y = 0;

    ns_layout_block(root, viewport_width, NULL);
    ns_layout_apply_position_offsets(root, viewport_width, root->content_height);
    ns_layout_process_absolute_boxes(root, styles, viewport_width);
    ns_paint_sync_inline_atomic_offsets(root);
    ns_layout_compute_paint_bounds(root);

    g_array_free(g_abs_pending, TRUE);
    g_abs_pending = NULL;
    g_clear_pointer(&g_abs_seen, g_hash_table_destroy);
    g_clear_pointer(&g_abs_ph_set, g_hash_table_destroy);
    g_clear_pointer(&g_abs_static, g_hash_table_destroy);
    g_hash_table_destroy(g_contains_block_media_cache);
    g_contains_block_media_cache = NULL;
    return root;
}

double
ns_layout_length_resolve(const ns_css_value *v, double basis, double fallback)
{
    return length_resolve(v, basis, fallback);
}

gboolean
ns_layout_value_is_percent(const ns_css_value *v)
{
    return value_is_percent(v);
}

void
ns_layout_edges_from_style(const ns_style *s, double basis, ns_edges *margin,
                           ns_edges *padding, ns_edges *border)
{
    edges_from_style(s, basis, margin, padding, border);
}

double
ns_layout_resolve_used_height(const ns_box *box, const ns_css_value *hv,
                              double width_basis, double fallback)
{
    return resolve_used_height(box, hv, width_basis, fallback);
}

double
ns_layout_specified_height_to_content(const ns_box *b, double h)
{
    return specified_height_to_content(b, h);
}

double
ns_layout_clamp_height_minmax_px(const ns_style *s, double h)
{
    return clamp_height_minmax_px(s, h);
}

gboolean
ns_layout_self_start_is_far_side(const ns_style *s, gboolean horizontal_axis)
{
    return self_start_is_far_side(s, horizontal_axis);
}

void
ns_layout_translate_subtree(ns_box *box, double dx, double dy)
{
    translate_subtree(box, dx, dy);
}

double
ns_layout_resolve_height_with_basis(const ns_css_value *hv, double width_basis,
                                    double height_basis, double fallback)
{
    return resolve_height_with_basis(hv, width_basis, height_basis, fallback);
}

double
ns_layout_containing_block_definite_height(const ns_box *box)
{
    return containing_block_definite_height(box);
}

gboolean
ns_layout_height_keyword_stretches(const ns_css_value *v)
{
    return height_keyword_stretches(v);
}

double
ns_layout_box_read_definite_height(const ns_box *box)
{
    return box_read_definite_height(box);
}

gboolean
ns_layout_style_is_absolute_or_fixed(const ns_style *s)
{
    return style_is_absolute_or_fixed(s);
}

gboolean
ns_layout_style_is_flex_container(const ns_style *s)
{
    return style_is_flex_container(s);
}

const char *
ns_layout_keyword_or(const ns_style *s, ns_css_prop p, const char *fallback)
{
    return keyword_or(s, p, fallback);
}

double
ns_layout_aspect_ratio_number(const ns_css_value *v, gboolean *with_auto)
{
    return aspect_ratio_number(v, with_auto);
}

const char *
ns_layout_flex_item_align(const ns_box *c, const char *container_align)
{
    return flex_item_align(c, container_align);
}

gboolean
ns_layout_flex_align_is_baseline(const char *align)
{
    return flex_align_is_baseline(align);
}

double
ns_layout_flex_item_baseline(const ns_box *c, double fallback)
{
    return flex_item_baseline(c, fallback);
}

const ns_node *
ns_layout_inline_box_form_hit(const ns_box *box, double local_x, double local_y,
                              const ns_style *parent_style)
{
    return inline_box_form_hit(box, local_x, local_y, parent_style);
}

gboolean
ns_layout_style_creates_fixed_cb(const ns_style *s)
{
    return style_creates_fixed_cb(s);
}

const char *
ns_layout_flex_direction_of(const ns_style *s)
{
    return flex_direction_of(s);
}

void
ns_layout_box_append_child(ns_box *parent, ns_box *child)
{
    box_append_child(parent, child);
}

const ns_node *
ns_layout_flat_parent(const ns_node *n)
{
    return layout_flat_parent(n);
}

guint
ns_layout_abs_pending_len(void)
{
    return g_abs_pending ? g_abs_pending->len : 0;
}

gboolean
ns_layout_abs_pending_entry(guint i, const ns_node **dom,
                            const ns_style **pseudo, gboolean *fixed)
{
    if (!g_abs_pending || i >= g_abs_pending->len) return FALSE;
    const ns_abs_entry *e = &g_array_index(g_abs_pending, ns_abs_entry, i);
    *dom = e->dom;
    *pseudo = e->pseudo;
    *fixed = e->fixed;
    return TRUE;
}

void
ns_layout_abs_pending_clear(void)
{
    if (g_abs_pending) g_array_set_size(g_abs_pending, 0);
}

ns_box *
ns_layout_abs_static_run(const ns_node *dom, double *rel_x, double *rel_y)
{
    const ns_abs_static *st = g_abs_static
        ? g_hash_table_lookup(g_abs_static, dom) : NULL;
    if (!st || !st->run) return NULL;
    *rel_x = st->rel_x;
    *rel_y = st->rel_y;
    return st->run;
}

ns_box *
ns_layout_abs_build_box(const ns_node *dom, const ns_style *pseudo,
                        GHashTable *styles)
{
    if (!pseudo) {
        g_abs_force_build = TRUE;
        ns_box *abox = build_block(dom, styles);
        g_abs_force_build = FALSE;
        return abox;
    }
    ns_box *abox = box_new(NS_BOX_BLOCK);
    abox->style = pseudo;
    collect_box_bg_image(abox, pseudo);
    ns_box *gen = build_pseudo_inline_for(pseudo, dom);
    if (gen && gen->kind == NS_BOX_INLINE && gen->text && !*gen->text &&
        !gen->inline_atomics) {
        ns_box_free(gen);
        gen = NULL;
    }
    if (gen) box_append_child(abox, gen);
    return abox;
}

gboolean
ns_layout_box_first_baseline(const ns_box *b, double *out)
{
    return box_first_baseline(b, out);
}

gboolean
ns_layout_box_is_abs_placeholder(const ns_box *b)
{
    return g_abs_ph_set && g_hash_table_contains(g_abs_ph_set, b);
}

const ns_node *
ns_layout_abs_static_target(const ns_box *b)
{
    if (!g_abs_static || !g_abs_ph_set) return NULL;
    return g_hash_table_lookup(g_abs_ph_set, b);
}

void
ns_layout_record_abs_static(const ns_node *dom, ns_box *run, double rel_x,
                            double rel_y)
{
    ns_abs_static *st = g_new0(ns_abs_static, 1);
    st->run   = run;
    st->rel_x = rel_x;
    st->rel_y = rel_y;
    g_hash_table_insert(g_abs_static, (gpointer)dom, st);
}

double
ns_layout_image_dimension_attr(const ns_node *n, const char *name)
{
    return image_dimension_attr(n, name);
}

void
ns_layout_inline_runs_join_splits(ns_box *box)
{
    inline_runs_join_splits(box);
}

void
ns_layout_reorder_children_by_order(ns_box *box)
{
    reorder_children_by_order(box);
}

gboolean
ns_layout_inline_box_can_fragment_multicol(const ns_box *box,
                                           const ns_style *style, double basis)
{
    return inline_box_can_fragment_multicol(box, style, basis);
}

gboolean
ns_layout_multicol_single_inline(ns_box *box, double inner_x, double inner_y,
                                 double col_w, double col_gap, int n_cols,
                                 const ns_style *child_inherited,
                                 double *cursor_y)
{
    return layout_multicol_single_inline(box, inner_x, inner_y, col_w, col_gap,
                                         n_cols, child_inherited, cursor_y);
}

gsize
ns_layout_inline_run_break_before(const ns_box *run, double band_height)
{
    return inline_run_break_before(run, band_height);
}

ns_box *
ns_layout_inline_run_split(ns_box *run, gsize split)
{
    return inline_run_split(run, split);
}
