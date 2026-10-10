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
static gboolean size_keyword_is_intrinsic(const ns_css_value *v);
static double intrinsic_keyword_width(ns_box *box, const char *kw,
                                      const ns_style *mi, double avail);

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
style_is_multicol(const ns_style *s)
{
    if (!s) return FALSE;
    const ns_css_value *cc = s->values[NS_CSS_COLUMN_COUNT];
    if (cc && cc->kind == NS_CSS_V_LENGTH && cc->u.length.v >= 2) return TRUE;
    const ns_css_value *cw = s->values[NS_CSS_COLUMN_WIDTH];
    return cw && cw->kind == NS_CSS_V_LENGTH && cw->u.length.v > 0;
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
style_content_visibility_hidden(const ns_style *s)
{
    return s && s->values[NS_CSS_CONTENT_VISIBILITY] &&
           is_keyword(s->values[NS_CSS_CONTENT_VISIBILITY], "hidden");
}

static gboolean
style_contains_inline_size(const ns_style *s)
{
    const ns_css_value *ct = s ? s->values[NS_CSS_CONTAINER_TYPE] : NULL;
    if (!ct || ct->kind != NS_CSS_V_KEYWORD || !ct->u.keyword) return FALSE;
    return g_ascii_strcasecmp(ct->u.keyword, "size") == 0 ||
           g_ascii_strcasecmp(ct->u.keyword, "inline-size") == 0;
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

static gboolean box_clips_children(const ns_box *b);
static gboolean box_first_baseline(const ns_box *b, double *out);

static gboolean
box_clips_for_page_height(const ns_box *b)
{
    if (!b->parent) return FALSE;
    if (b->dom && b->dom->name &&
        (strcmp(b->dom->name, "html") == 0 ||
         strcmp(b->dom->name, "body") == 0))
        return FALSE;
    return box_clips_children(b);
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

static gboolean tag_is_non_rendering(const char *name);
static gboolean node_is_non_rendering(const ns_node *n);

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
        if (node_is_non_rendering(c)) continue;
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

static int clear_kind_of(const ns_style *s);

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
    if (n->name && strcmp(n->name, "br") == 0 && clear_kind_of(s))
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
static void layout_box(ns_box *box, double parent_content_width,
                       const ns_style *inherited_style);
static ns_box *build_inline_run(const ns_node *first, const ns_node *last_excl, GHashTable *styles);
static ns_box *build_inline_run_no_abs_placeholders(const ns_node *first, const ns_node *last_excl, GHashTable *styles);
static gboolean inline_atomic_needs_layout(const ns_box *ab);
static ns_box *build_pseudo_inline_for(const ns_style *ps, const ns_node *host);
static ns_box *build_pseudo_block_for(const ns_style *ps, const ns_node *host);
static void register_abs_pseudo(const ns_node *host, const ns_style *ps);
static ns_box *build_blockified_inline_item(const ns_node *n, GHashTable *styles,
                                            ns_box **pending_before);
static void layout_block(ns_box *box, double parent_content_width, const ns_style *inherited_style);
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
static GHashTable    *g_counters_for_layout;
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
            tag_is_non_rendering(c->name)) {
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
                    tag_is_non_rendering(c->name)) {
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

static void
append_inline_spacer(collector_ctx *ctx, double width)
{
    if (!(width > 0)) return;
    gsize start = ctx->out->len;
    g_string_append(ctx->out, "\xef\xbf\xbc");
    ns_inline_attr spacer = {
        .kind = NS_INLINE_SPACER,
        .start = start,
        .len = 3,
        .box_w = width,
    };
    g_array_append_val(ctx->attrs, spacer);
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

static int
control_pad_spaces(const ns_style *s, ns_css_prop prop)
{
    if (!s) return 0;
    double fs = length_or(s->values[NS_CSS_FONT_SIZE], 16);
    if (!(fs > 0)) fs = 16;
    double pad = length_resolve(s->values[prop], fs * 20.0, 0);
    double space = fs * 0.25;
    if (!(space > 0) || pad <= space) return 0;
    int n = (int)((pad - space) / space + 0.5);
    if (n < 0) n = 0;
    if (n > 12) n = 12;
    return n;
}

static int
text_input_leading_spaces(const ns_style *s)
{
    return control_pad_spaces(s, NS_CSS_PADDING_LEFT);
}

static GHashTable *g_input_columns_for_layout;

static int
text_input_size_attr(const ns_node *n)
{
    const char *size_str = ns_element_get_attr(n, "size");
    return size_str ? ns_parse_int(size_str, 20, 4, 80) : 20;
}

static int
text_input_columns(const ns_node *n)
{
    int size = text_input_size_attr(n);
    if (!g_input_columns_for_layout) return size;
    gpointer fitted = g_hash_table_lookup(g_input_columns_for_layout, n);
    return fitted ? MIN(GPOINTER_TO_INT(fitted), 4096) : size;
}

static gboolean
text_input_align_rtl(const ns_style *s, const ns_node *n)
{
    for (const ns_node *p = n; p; p = p->parent) {
        if (p->kind != NS_NODE_ELEMENT) continue;
        const char *dir = ns_element_get_attr(p, "dir");
        if (!dir) continue;
        if (g_ascii_strcasecmp(dir, "rtl") == 0) return TRUE;
        if (g_ascii_strcasecmp(dir, "ltr") == 0) return FALSE;
    }
    return keyword_is(s ? s->values[NS_CSS_DIRECTION] : NULL, "rtl");
}

static glong
text_input_align_offset(const ns_style *s, const ns_node *n, glong span)
{
    if (span <= 0) return 0;
    const ns_css_value *ta = s ? s->values[NS_CSS_TEXT_ALIGN] : NULL;
    gboolean rtl = text_input_align_rtl(s, n);
    if (keyword_is(ta, "center"))
        return span / 2;
    if (keyword_is(ta, "right") ||
        (keyword_is(ta, "end") && !rtl) ||
        (keyword_is(ta, "start") && rtl) ||
        (!ta && rtl))
        return span;
    return 0;
}

static glong
text_input_align_skip_cps(const ns_style *s, const ns_node *n,
                          glong cps, glong size)
{
    if (cps <= size) return 0;
    return text_input_align_offset(s, n, cps - size);
}

static glong
text_input_focused_skip_cps(const char *value, gsize caret_byte,
                            glong cps, glong size)
{
    if (!value || cps <= size) return 0;
    glong max_skip = cps - size;
    glong caret_cp = g_utf8_pointer_to_offset(value, value + caret_byte);
    glong skip = caret_cp > size ? caret_cp - size : 0;
    if (skip > max_skip) skip = max_skip;
    if (skip < 0) skip = 0;
    return skip;
}

static const char *
text_input_advance_cps(const char *p, glong cps)
{
    for (glong i = 0; p && *p && i < cps; i++)
        p = g_utf8_next_char(p);
    return p ? p : "";
}

static gboolean
name_in(const char *name, const char *const *set)
{
    if (!name) return FALSE;
    for (; *set; set++) if (strcmp(name, *set) == 0) return TRUE;
    return FALSE;
}

static gboolean
tag_is_bold(const char *name)
{
    static const char *const set[] = { "b", "strong", NULL };
    return name_in(name, set);
}

static gboolean
tag_is_italic(const char *name)
{
    static const char *const set[] = { "i", "em", "cite", "dfn", NULL };
    return name_in(name, set);
}

static gboolean
tag_is_monospace(const char *name)
{
    static const char *const set[] = { "code", "tt", "kbd", "samp", "pre", NULL };
    return name_in(name, set);
}

static gboolean
tag_is_underline(const char *name)
{
    static const char *const set[] = { "u", "ins", NULL };
    return name_in(name, set);
}

static gboolean
tag_is_strike(const char *name)
{
    static const char *const set[] = { "s", "del", "strike", NULL };
    return name_in(name, set);
}

static gboolean
tag_is_non_rendering(const char *name)
{
    static const char *const set[] = {
        "area", "base", "head", "link", "meta", "noscript", "param",
        "script", "source", "style", "template", "title", "track", NULL,
    };
    return name_in(name, set);
}

static gboolean
node_is_non_rendering(const ns_node *n)
{
    if (!n || !tag_is_non_rendering(n->name)) return FALSE;
    if (n->name && g_ascii_strcasecmp(n->name, "noscript") == 0) {
        const ns_node *root = ns_node_root(n);
        if (root && (root->flags & NS_NODE_SCRIPTING_DISABLED))
            return FALSE;
    }
    return TRUE;
}

static void
emit_attr(GArray *attrs, ns_inline_attr_kind k, gsize start, gsize end)
{
    if (end <= start) return;
    ns_inline_attr a = { .kind = k, .start = start, .len = end - start };
    g_array_append_val(attrs, a);
}

static void
emit_attr_styled(GArray *attrs, ns_inline_attr_kind k, gsize start, gsize end,
                 const ns_style *style)
{
    if (end <= start) return;
    ns_inline_attr a = { .kind = k, .start = start, .len = end - start,
                         .style = style };
    g_array_append_val(attrs, a);
}

static gboolean
inline_run_at_line_start(const GString *out)
{
    if (!out || out->len == 0) return TRUE;
    gsize i = out->len;
    while (i > 0) {
        if ((guchar)out->str[i - 1] == ' ') { i--; continue; }
        if (i >= 2 && (guchar)out->str[i - 2] == 0xc2 &&
            (guchar)out->str[i - 1] == 0xa0) { i -= 2; continue; }
        break;
    }
    if (i == 0) return TRUE;
    if (i >= 3 && (guchar)out->str[i - 3] == 0xe2 &&
        (guchar)out->str[i - 2] == 0x80 &&
        ((guchar)out->str[i - 1] == 0xa8 || (guchar)out->str[i - 1] == 0xa9))
        return TRUE;
    if ((guchar)out->str[i - 1] == '\n') return TRUE;
    return FALSE;
}

static double
control_dim_px_basis(const ns_css_value *v, double font_size, double basis)
{
    if (!v) return 0;
    if (v->kind == NS_CSS_V_CALC) {
        double out = v->u.calc.px;
        if (basis > 0) out += v->u.calc.pct * basis / 100.0;
        return out > 0 ? out : 0;
    }
    if (v->kind != NS_CSS_V_LENGTH) return 0;
    switch (v->u.length.unit) {
    case NS_CSS_UNIT_PX:
    case NS_CSS_UNIT_NUMBER:
        return v->u.length.v;
    case NS_CSS_UNIT_EM:
        return v->u.length.v * font_size;
    case NS_CSS_UNIT_REM:
        return v->u.length.v * 16.0;
    case NS_CSS_UNIT_PERCENT:
        return basis > 0 ? v->u.length.v * basis / 100.0 : 0;
    case NS_CSS_UNIT_VW:
        return v->u.length.v * ns_css_viewport_w() / 100.0;
    case NS_CSS_UNIT_VH:
        return v->u.length.v * ns_css_viewport_h() / 100.0;
    case NS_CSS_UNIT_VMIN:
        return v->u.length.v * MIN(ns_css_viewport_w(), ns_css_viewport_h()) / 100.0;
    case NS_CSS_UNIT_VMAX:
        return v->u.length.v * MAX(ns_css_viewport_w(), ns_css_viewport_h()) / 100.0;
    case NS_CSS_UNIT_CQW:
        return v->u.length.v * (ns_css_container_w() > 0 ? ns_css_container_w() : ns_css_viewport_w()) / 100.0;
    case NS_CSS_UNIT_CQH:
        return v->u.length.v * (ns_css_container_h() > 0 ? ns_css_container_h() : ns_css_viewport_h()) / 100.0;
    case NS_CSS_UNIT_CQMIN: {
        double cw = ns_css_container_w() > 0 ? ns_css_container_w() : ns_css_viewport_w();
        double ch = ns_css_container_h() > 0 ? ns_css_container_h() : ns_css_viewport_h();
        return v->u.length.v * MIN(cw, ch) / 100.0;
    }
    case NS_CSS_UNIT_CQMAX: {
        double cw = ns_css_container_w() > 0 ? ns_css_container_w() : ns_css_viewport_w();
        double ch = ns_css_container_h() > 0 ? ns_css_container_h() : ns_css_viewport_h();
        return v->u.length.v * MAX(cw, ch) / 100.0;
    }
    case NS_CSS_UNIT_EX:
    case NS_CSS_UNIT_CH:
        return v->u.length.v * font_size * 0.5;
    case NS_CSS_UNIT_CAP:
        return v->u.length.v * font_size * 0.7;
    case NS_CSS_UNIT_IC:
        return v->u.length.v * font_size;
    case NS_CSS_UNIT_LH:
        return v->u.length.v * font_size * 1.5;
    case NS_CSS_UNIT_RLH:
        return v->u.length.v * 24.0;
    case NS_CSS_UNIT_REX:
    case NS_CSS_UNIT_RCH:
        return v->u.length.v * 8.0;
    case NS_CSS_UNIT_RCAP:
        return v->u.length.v * 11.2;
    case NS_CSS_UNIT_RIC:
        return v->u.length.v * 16.0;
    }
    return 0;
}

static double
control_dim_px_clamped(const ns_style *s, ns_css_prop value_prop,
                       ns_css_prop min_prop, ns_css_prop max_prop,
                       double font_size, double basis)
{
    if (!s) return 0;
    double out = control_dim_px_basis(s->values[value_prop], font_size, basis);
    double mn = control_dim_px_basis(s->values[min_prop], font_size, basis);
    double mx = control_dim_px_basis(s->values[max_prop], font_size, basis);
    if (mn > 0 && out > 0 && out < mn) out = mn;
    if (mx > 0 && out > mx) out = mx;
    return out;
}

static gboolean
control_is_border_box(const ns_node *dom, const ns_style *s)
{
    if (s && keyword_is(s->values[NS_CSS_BOX_SIZING], "border-box"))
        return TRUE;
    if (s && keyword_is(s->values[NS_CSS_BOX_SIZING], "content-box"))
        return FALSE;
    if (!dom || !dom->name) return FALSE;
    if (strcmp(dom->name, "button") == 0 || strcmp(dom->name, "select") == 0)
        return TRUE;
    if (strcmp(dom->name, "input") != 0) return FALSE;
    const char *type = ns_element_get_attr(dom, "type");
    if (!type || !*type) return FALSE;
    return g_ascii_strcasecmp(type, "submit") == 0 ||
           g_ascii_strcasecmp(type, "reset") == 0 ||
           g_ascii_strcasecmp(type, "button") == 0 ||
           g_ascii_strcasecmp(type, "checkbox") == 0 ||
           g_ascii_strcasecmp(type, "radio") == 0 ||
           g_ascii_strcasecmp(type, "color") == 0 ||
           g_ascii_strcasecmp(type, "search") == 0;
}

double
ns_control_css_extra_w(const ns_node *dom, const ns_style *s)
{
    if (!s || control_is_border_box(dom, s)) return 0;
    ns_edges m, p, b;
    edges_from_style(s, 0, &m, &p, &b);
    return p.left + p.right + b.left + b.right;
}

double
ns_control_css_extra_h(const ns_node *dom, const ns_style *s)
{
    if (!s || control_is_border_box(dom, s)) return 0;
    ns_edges m, p, b;
    edges_from_style(s, 0, &m, &p, &b);
    return p.top + p.bottom + b.top + b.bottom;
}

static double
inline_attr_control_width(const ns_inline_attr *r, const ns_box *box)
{
    if (!r || !r->style) return r && r->box_w > 0 ? r->box_w : 0;
    double fs = length_or(r->style->values[NS_CSS_FONT_SIZE], 16);
    double w = control_dim_px_clamped(r->style, NS_CSS_WIDTH,
                                      NS_CSS_MIN_WIDTH, NS_CSS_MAX_WIDTH,
                                      fs, box ? box->content_width : 0);
    if (w > 0) w += ns_control_css_extra_w(r->dom, r->style);
    return w > 0 ? w : r->box_w;
}

static gboolean
style_has_visible_control_box(const ns_style *s)
{
    if (!s) return FALSE;
    const ns_css_value *bg = s->values[NS_CSS_BACKGROUND_COLOR];
    if (bg && bg->kind == NS_CSS_V_COLOR && bg->u.color.a > 0)
        return TRUE;
    if (s->values[NS_CSS_BOX_SHADOW] &&
        s->values[NS_CSS_BOX_SHADOW]->kind == NS_CSS_V_SHADOW &&
        s->values[NS_CSS_BOX_SHADOW]->u.shadow.n > 0)
        return TRUE;
    if (s->values[NS_CSS_BACKGROUND_IMAGE] &&
        (s->values[NS_CSS_BACKGROUND_IMAGE]->kind == NS_CSS_V_URL ||
         s->values[NS_CSS_BACKGROUND_IMAGE]->kind == NS_CSS_V_GRADIENT))
        return TRUE;
    if (length_or(s->values[NS_CSS_BORDER_RADIUS], 0) > 0 ||
        length_or(s->values[NS_CSS_BORDER_TOP_LEFT_RADIUS], 0) > 0 ||
        length_or(s->values[NS_CSS_BORDER_TOP_RIGHT_RADIUS], 0) > 0 ||
        length_or(s->values[NS_CSS_BORDER_BOTTOM_RIGHT_RADIUS], 0) > 0 ||
        length_or(s->values[NS_CSS_BORDER_BOTTOM_LEFT_RADIUS], 0) > 0)
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
    return FALSE;
}

static gboolean
control_style_strips_chrome(const ns_style *s)
{
    static const ns_css_prop styles_p[4] = {
        NS_CSS_BORDER_TOP_STYLE, NS_CSS_BORDER_RIGHT_STYLE,
        NS_CSS_BORDER_BOTTOM_STYLE, NS_CSS_BORDER_LEFT_STYLE,
    };
    static const ns_css_prop widths_p[4] = {
        NS_CSS_BORDER_TOP_WIDTH, NS_CSS_BORDER_RIGHT_WIDTH,
        NS_CSS_BORDER_BOTTOM_WIDTH, NS_CSS_BORDER_LEFT_WIDTH,
    };
    if (!s) return FALSE;
    (void)widths_p;
    for (int i = 0; i < 4; i++) {
        const ns_css_value *st = s->values[styles_p[i]];
        if (!keyword_is(st, "none") && !keyword_is(st, "hidden"))
            return FALSE;
    }
    return TRUE;
}

static gboolean
control_prefers_css_chrome(ns_inline_attr_kind k, const ns_node *dom,
                           const ns_style *s)
{
    if (!dom || !s) return FALSE;
    if (keyword_is(s->values[NS_CSS_APPEARANCE], "none"))
        return TRUE;
    if (control_style_strips_chrome(s)) return TRUE;
    if (style_has_visible_control_box(s)) return TRUE;
    if (!ns_element_get_attr(dom, "class")) return FALSE;
    ns_display d = ns_css_display_of(s);
    if (ns_display_is_block_level(d) || display_is_atomic_inline_container(d))
        return k == NS_INLINE_INPUT_FIELD ||
               k == NS_INLINE_INPUT_FIELD_FOCUSED ||
               k == NS_INLINE_BUTTON;
    return FALSE;
}

static void
emit_form_attr_sized(GArray *attrs, ns_inline_attr_kind k, gsize start, gsize end,
                     const ns_node *dom, GHashTable *styles)
{
    if (end <= start) return;
    double bw = 0, bh = 0;
    const ns_style *s = NULL;
    double fs = 16;
    const char *bg_image_src = NULL;
    void *bg_image = NULL;
    if (dom && styles) {
        s = g_hash_table_lookup(styles, dom);
        if (s) {
            fs = length_or(s->values[NS_CSS_FONT_SIZE], 16);
            bw = control_dim_px_clamped(s, NS_CSS_WIDTH,
                                        NS_CSS_MIN_WIDTH, NS_CSS_MAX_WIDTH,
                                        fs, 0);
            bh = control_dim_px_clamped(s, NS_CSS_HEIGHT,
                                        NS_CSS_MIN_HEIGHT, NS_CSS_MAX_HEIGHT,
                                        fs, 0);
            if (bw > 0) bw += ns_control_css_extra_w(dom, s);
            if (bh > 0) bh += ns_control_css_extra_h(dom, s);
            const ns_css_value *bg = s->values[NS_CSS_BACKGROUND_IMAGE];
            if (bg && bg->kind == NS_CSS_V_URL && bg->u.url) {
                bg_image_src = bg->u.url;
                if (g_image_cache_for_layout) {
                    char *abs = g_base_url_for_layout
                        ? ns_url_resolve(g_base_url_for_layout, bg_image_src)
                        : NULL;
                    bg_image = ns_image_cache_peek(g_image_cache_for_layout,
                                                   abs ? abs : bg_image_src);
                    g_free(abs);
                }
            }
        }
    }
    if (dom && dom->name && strcmp(dom->name, "textarea") == 0) {
        if (bw <= 0) {
            const char *cols = ns_element_get_attr(dom, "cols");
            int c = cols ? atoi(cols) : 20;
            if (c <= 0) c = 20;
            bw = c * (fs * 0.5) + 8.0;
        }
        if (bh <= 0) {
            const char *rows = ns_element_get_attr(dom, "rows");
            int r = rows ? atoi(rows) : 2;
            if (r <= 0) r = 2;
            double line_h = fs * 1.3;
            bh = r * line_h + 6.0;
        }
    }
    ns_inline_attr a = {
        .kind = k, .start = start, .len = end - start, .dom = dom,
        .box_w = bw, .box_h = bh,
        .native_chrome = !control_prefers_css_chrome(k, dom, s),
        .style = s,
        .bg_image_src = bg_image_src,
        .bg_image = bg_image,
    };
    g_array_append_val(attrs, a);
}

static void
emit_control_text_style(GArray *attrs, const ns_style *s,
                        gsize field_start, gsize field_end,
                        gsize val_start, gsize val_end,
                        gboolean is_placeholder, gboolean disabled)
{
    if (field_end > field_start) {
        double ifs = s ? length_or(s->values[NS_CSS_FONT_SIZE], 0) : 0;
        if (ifs > 0) {
            ns_inline_attr a = {
                .kind = NS_INLINE_FONT_SIZE,
                .start = field_start, .len = field_end - field_start,
                .font_size_px = ifs,
            };
            g_array_append_val(attrs, a);
        }
        const ns_css_value *ffam = s ? s->values[NS_CSS_FONT_FAMILY] : NULL;
        if (ffam && ffam->kind == NS_CSS_V_KEYWORD && ffam->u.keyword) {
            ns_inline_attr a = {
                .kind = NS_INLINE_FONT_FAMILY,
                .start = field_start, .len = field_end - field_start,
                .family = ffam->u.keyword,
            };
            g_array_append_val(attrs, a);
        }
    }
    if (val_end <= val_start) return;
    const ns_style *phs = (s && is_placeholder) ? s->placeholder : NULL;
    guint8 cr = 0, cg = 0, cb = 0, ca = 255;
    gboolean have_color = FALSE;
    if (is_placeholder) {
        const ns_css_value *pc = phs ? phs->values[NS_CSS_COLOR] : NULL;
        if (pc && pc->kind == NS_CSS_V_COLOR) {
            cr = pc->u.color.r; cg = pc->u.color.g;
            cb = pc->u.color.b; ca = pc->u.color.a;
        } else {
            cr = cg = cb = 0x75;
        }
        have_color = TRUE;
    } else {
        const ns_css_value *cv = s ? s->values[NS_CSS_COLOR] : NULL;
        if (cv && cv->kind == NS_CSS_V_COLOR) {
            cr = cv->u.color.r; cg = cv->u.color.g;
            cb = cv->u.color.b; ca = cv->u.color.a;
            have_color = TRUE;
        }
    }
    if (disabled) { cr = cg = cb = 0x82; ca = 255; have_color = TRUE; }
    if (phs) {
        const ns_css_value *ov = phs->values[NS_CSS_OPACITY];
        if (ov && ov->kind == NS_CSS_V_LENGTH)
            ca = (guint8)lround(ca * CLAMP(ov->u.length.v, 0.0, 1.0));
        if (keyword_is(phs->values[NS_CSS_VISIBILITY], "hidden") ||
            keyword_is(phs->values[NS_CSS_VISIBILITY], "collapse"))
            ca = 0;
    }
    if (have_color) {
        ns_inline_attr a = {
            .kind = NS_INLINE_COLOR,
            .start = val_start, .len = val_end - val_start,
            .r = cr, .g = cg, .b = cb, .a = ca,
        };
        g_array_append_val(attrs, a);
    }
    if (phs) {
        if (keyword_is(phs->values[NS_CSS_FONT_STYLE], "italic") ||
            keyword_is(phs->values[NS_CSS_FONT_STYLE], "oblique"))
            emit_attr(attrs, NS_INLINE_ITALIC, val_start, val_end);
        int phw = ns_css_font_weight_number(phs->values[NS_CSS_FONT_WEIGHT], -1);
        if (phw > 0) {
            ns_inline_attr a = {
                .kind = NS_INLINE_FONT_WEIGHT,
                .start = val_start, .len = val_end - val_start,
                .font_weight = phw,
            };
            g_array_append_val(attrs, a);
        }
    }
}

static void
emit_font_size_attr(GArray *attrs, gsize start, gsize end, double font_size_px)
{
    if (end <= start) return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_SIZE, .start = start,
                         .len = end - start, .font_size_px = font_size_px };
    g_array_append_val(attrs, a);
}

static void
emit_font_weight_attr(GArray *attrs, gsize start, gsize end, int font_weight)
{
    if (end <= start || font_weight <= 0) return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_WEIGHT, .start = start,
                         .len = end - start, .font_weight = font_weight };
    g_array_append_val(attrs, a);
}

static void
emit_font_stretch_attr(GArray *attrs, gsize start, gsize end, int font_stretch)
{
    if (end <= start) return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_STRETCH, .start = start,
                         .len = end - start, .font_stretch = font_stretch };
    g_array_append_val(attrs, a);
}

static int
font_kerning_int_from_style(const ns_style *s)
{
    const char *kw = s ? ns_style_keyword(s, NS_CSS_FONT_KERNING) : NULL;
    if (!kw) return -1;
    if (strcmp(kw, "none") == 0) return 0;
    if (strcmp(kw, "normal") == 0 || strcmp(kw, "auto") == 0) return 1;
    return -1;
}

static const char *
font_ligatures_from_style(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_FONT_VARIANT_LIGATURES] : NULL;
    return v && v->kind == NS_CSS_V_KEYWORD ? v->u.keyword : NULL;
}

static const char *
font_feature_settings_from_style(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_FONT_FEATURE_SETTINGS] : NULL;
    return v && v->kind == NS_CSS_V_KEYWORD ? v->u.keyword : NULL;
}

static const char *
font_variation_settings_from_style(const ns_style *s)
{
    const ns_css_value *v = s ? s->values[NS_CSS_FONT_VARIATION_SETTINGS] : NULL;
    return v && v->kind == NS_CSS_V_KEYWORD ? v->u.keyword : NULL;
}

static void
emit_font_features_attr(GArray *attrs, gsize start, gsize end,
                        int font_kerning, const char *font_ligatures,
                        const char *font_features)
{
    if (end <= start ||
        (font_kerning < 0 && !font_ligatures && !font_features))
        return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_FEATURES, .start = start,
                         .len = end - start, .font_kerning = font_kerning,
                         .font_ligatures = font_ligatures,
                         .font_features = font_features };
    g_array_append_val(attrs, a);
}

static void
emit_font_variations_attr(GArray *attrs, gsize start, gsize end,
                          const char *font_variations)
{
    if (end <= start || !font_variations) return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_VARIATIONS, .start = start,
                         .len = end - start,
                         .font_variations = font_variations };
    g_array_append_val(attrs, a);
}

static void
emit_color_attr(GArray *attrs, gsize start, gsize end,
                guint8 r, guint8 g, guint8 b, guint8 a8)
{
    if (end <= start) return;
    ns_inline_attr a = { .kind = NS_INLINE_COLOR, .start = start,
                         .len = end - start, .r = r, .g = g, .b = b, .a = a8 };
    g_array_append_val(attrs, a);
}

static void
emit_font_family_attr(GArray *attrs, gsize start, gsize end, const char *family)
{
    if (end <= start || !family) return;
    ns_inline_attr a = { .kind = NS_INLINE_FONT_FAMILY, .start = start,
                         .len = end - start, .family = family };
    g_array_append_val(attrs, a);
}

static void
counter_apply_decl(GHashTable *counters, const char *decl, gboolean increment)
{
    if (!decl || !*decl) return;
    const char *p = decl;
    while (*p) {
        while (*p && g_ascii_isspace(*p)) p++;
        if (!*p) break;
        const char *name_s = p;
        while (*p && !g_ascii_isspace(*p)) p++;
        gsize nlen = (gsize)(p - name_s);
        if (nlen == 0) break;
        char *name = g_strndup(name_s, nlen);
        if (strcmp(name, "none") == 0) { g_free(name); break; }
        while (*p && g_ascii_isspace(*p)) p++;
        int val = increment ? 1 : 0;
        if (*p == '-' || g_ascii_isdigit(*p)) {
            char *end = NULL;
            long v = strtol(p, &end, 10);
            if (end != p) { val = (int)v; p = end; }
        }
        gint cur = increment
            ? GPOINTER_TO_INT(g_hash_table_lookup(counters, name))
            : 0;
        g_hash_table_insert(counters, g_strdup(name),
                            GINT_TO_POINTER(cur + val));
        g_free(name);
    }
}

static gchar *
counter_format(gint v, const char *style)
{
    if (!style || strcmp(style, "decimal") == 0)
        return g_strdup_printf("%d", v);
    if (strcmp(style, "decimal-leading-zero") == 0)
        return g_strdup_printf("%02d", v);
    if (strcmp(style, "lower-roman") == 0 || strcmp(style, "upper-roman") == 0) {
        if (v < 1 || v > 3999) return g_strdup_printf("%d", v);
        static const struct { int n; const char *s; } R[] = {
            {1000,"m"},{900,"cm"},{500,"d"},{400,"cd"},{100,"c"},{90,"xc"},
            {50,"l"},{40,"xl"},{10,"x"},{9,"ix"},{5,"v"},{4,"iv"},{1,"i"}
        };
        GString *s = g_string_new(NULL);
        for (gsize i = 0; i < G_N_ELEMENTS(R); i++)
            while (v >= R[i].n) { g_string_append(s, R[i].s); v -= R[i].n; }
        if (style[0] == 'u') {
            char *up = g_ascii_strup(s->str, -1);
            g_string_free(s, TRUE);
            return up;
        }
        return g_string_free(s, FALSE);
    }
    if (strcmp(style, "lower-alpha") == 0 || strcmp(style, "lower-latin") == 0 ||
        strcmp(style, "upper-alpha") == 0 || strcmp(style, "upper-latin") == 0) {
        if (v < 1) return g_strdup_printf("%d", v);
        char base = (style[0] == 'u') ? 'A' : 'a';
        GString *s = g_string_new(NULL);
        char buf[16]; int n = 0;
        while (v > 0 && n < (int)sizeof(buf)) { v--; buf[n++] = base + (v % 26); v /= 26; }
        while (n > 0) g_string_append_c(s, buf[--n]);
        return g_string_free(s, FALSE);
    }
    if (strcmp(style, "lower-greek") == 0) {
        static const gunichar greek[24] = {
            0x3B1, 0x3B2, 0x3B3, 0x3B4, 0x3B5, 0x3B6, 0x3B7, 0x3B8,
            0x3B9, 0x3BA, 0x3BB, 0x3BC, 0x3BD, 0x3BE, 0x3BF, 0x3C0,
            0x3C1, 0x3C3, 0x3C4, 0x3C5, 0x3C6, 0x3C7, 0x3C8, 0x3C9,
        };
        if (v < 1) return g_strdup_printf("%d", v);
        gunichar bg[16];
        int n = 0, w = v;
        while (w > 0 && n < 16) { w--; bg[n++] = greek[w % 24]; w /= 24; }
        GString *s = g_string_new(NULL);
        while (n > 0) g_string_append_unichar(s, bg[--n]);
        return g_string_free(s, FALSE);
    }
    return g_strdup_printf("%d", v);
}

static void
counter_apply_style(GHashTable *current, const ns_style *s)
{
    if (!s) return;
    if (s->values[NS_CSS_COUNTER_RESET] &&
        s->values[NS_CSS_COUNTER_RESET]->kind == NS_CSS_V_KEYWORD)
        counter_apply_decl(current,
            s->values[NS_CSS_COUNTER_RESET]->u.keyword, FALSE);
    if (s->values[NS_CSS_COUNTER_INCREMENT] &&
        s->values[NS_CSS_COUNTER_INCREMENT]->kind == NS_CSS_V_KEYWORD)
        counter_apply_decl(current,
            s->values[NS_CSS_COUNTER_INCREMENT]->u.keyword, TRUE);
}

static void
counter_walk(const ns_node *n, GHashTable *styles,
             GHashTable *current, GHashTable *snapshots, int depth)
{
    if (!n || depth >= NS_LAYOUT_MAX_DEPTH) return;
    const ns_style *s = NULL;
    if (n->kind == NS_NODE_ELEMENT && styles) {
        s = g_hash_table_lookup(styles, n);
        counter_apply_style(current, s);
        if (s) counter_apply_style(current, s->before);
        gboolean need = FALSE;
        if (s && s->before && s->before->values[NS_CSS_CONTENT]) {
            const ns_css_value *cv = s->before->values[NS_CSS_CONTENT];
            if (cv->kind == NS_CSS_V_KEYWORD && cv->u.keyword &&
                strstr(cv->u.keyword, "counter")) need = TRUE;
        }
        if (s && s->after && s->after->values[NS_CSS_CONTENT]) {
            const ns_css_value *cv = s->after->values[NS_CSS_CONTENT];
            if (cv->kind == NS_CSS_V_KEYWORD && cv->u.keyword &&
                strstr(cv->u.keyword, "counter")) need = TRUE;
        }
        if (need) {
            GHashTable *snap = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                     g_free, NULL);
            GHashTableIter it;
            gpointer k, v;
            g_hash_table_iter_init(&it, current);
            while (g_hash_table_iter_next(&it, &k, &v))
                g_hash_table_insert(snap, g_strdup((const char *)k), v);
            g_hash_table_insert(snapshots, (gpointer)n, snap);
        }
    }
    for (const ns_node *c = n->first_child; c; c = c->next_sibling)
        counter_walk(c, styles, current, snapshots, depth + 1);
    if (s) counter_apply_style(current, s->after);
}

static void
counter_snapshot_destroy(gpointer p) { g_hash_table_destroy(p); }

static GHashTable *
build_counter_snapshots(const ns_node *root, GHashTable *styles)
{
    GHashTable *snapshots = g_hash_table_new_full(
        g_direct_hash, g_direct_equal, NULL, counter_snapshot_destroy);
    GHashTable *current = g_hash_table_new_full(
        g_str_hash, g_str_equal, g_free, NULL);
    counter_walk(root, styles, current, snapshots, 0);
    g_hash_table_destroy(current);
    return snapshots;
}

static char *
substitute_one_counter(const char *body, const ns_node *host, gboolean is_counters)
{
    const char *p = body;
    while (*p && g_ascii_isspace(*p)) p++;
    const char *name_s = p;
    while (*p && *p != ',' && *p != ')' && !g_ascii_isspace(*p)) p++;
    gsize nlen = (gsize)(p - name_s);
    if (nlen == 0) return g_strdup("");
    char *name = g_strndup(name_s, nlen);
    char *sep = NULL;
    char *style = NULL;
    while (*p == ',' || g_ascii_isspace(*p)) {
        if (*p == ',') p++;
        while (*p && g_ascii_isspace(*p)) p++;
        if (*p == '"' || *p == '\'') {
            char q = *p++;
            const char *vs = p;
            while (*p && *p != q) p++;
            char *v = g_strndup(vs, p - vs);
            if (*p) p++;
            if (!sep) sep = v;
            else { g_free(v); }
        } else if (g_ascii_isalpha(*p)) {
            const char *vs = p;
            while (*p && *p != ',' && *p != ')' && !g_ascii_isspace(*p)) p++;
            if (!style) style = g_strndup(vs, p - vs);
        } else {
            break;
        }
    }
    if (is_counters && !sep) sep = g_strdup("");
    GHashTable *snap = g_counters_for_layout
        ? g_hash_table_lookup(g_counters_for_layout, host) : NULL;
    int v = snap ? GPOINTER_TO_INT(g_hash_table_lookup(snap, name)) : 0;
    char *formatted = counter_format(v, style);
    g_free(name); g_free(sep); g_free(style);
    return formatted;
}

static char *
resolve_pseudo_content(const char *raw, const ns_node *host)
{
    if (!raw || !*raw) return NULL;
    if (strcmp(raw, "none") == 0 || strcmp(raw, "normal") == 0) return NULL;
    gboolean has_func = strchr(raw, '(') != NULL;
    gboolean has_string = strchr(raw, '"') || strchr(raw, '\'');
    if (!has_func && !has_string) return g_strdup(raw);
    GString *out = g_string_new(NULL);
    const char *p = raw;
    while (*p) {
        while (*p && g_ascii_isspace(*p)) p++;
        if (!*p || *p == '/') break;
        if (*p == '"' || *p == '\'') {
            char q = *p++;
            const char *start = p;
            while (*p && *p != q) {
                if (*p == '\\' && p[1]) { p += 2; continue; }
                p++;
            }
            char *raw_str = g_strndup(start, p - start);
            for (const char *r = raw_str; *r; )
                ns_css_append_unescaped(out, &r);
            g_free(raw_str);
            if (*p == q) p++;
        } else if (g_str_has_prefix(p, "attr(")) {
            p += 5;
            while (*p == ' ') p++;
            const char *start = p;
            while (*p && *p != ')' && *p != ',' && *p != ' ') p++;
            if (host && p != start) {
                char *attr_name = g_strndup(start, p - start);
                const char *val = ns_element_get_attr(host, attr_name);
                if (val) g_string_append(out, val);
                g_free(attr_name);
            }
            while (*p && *p != ')') p++;
            if (*p == ')') p++;
        } else if (g_str_has_prefix(p, "counter(") ||
                   g_str_has_prefix(p, "counters(")) {
            gboolean is_counters = (p[7] == 's');
            p += is_counters ? 9 : 8;
            const char *body_s = p;
            int depth = 1;
            while (*p && depth > 0) {
                if (*p == '(') depth++;
                else if (*p == ')') { depth--; if (depth == 0) break; }
                p++;
            }
            char *body = g_strndup(body_s, p - body_s);
            if (*p == ')') p++;
            char *sub = substitute_one_counter(body, host, is_counters);
            if (sub) g_string_append(out, sub);
            g_free(sub);
            g_free(body);
        } else {
            const char *id = p;
            while (*p && (g_ascii_isalnum((guchar)*p) || *p == '-')) p++;
            if (*p == '(') {
                int depth = 0;
                for (; *p; p++) {
                    if (*p == '(') depth++;
                    else if (*p == ')') { depth--; if (depth == 0) { p++; break; } }
                }
            } else {
                while (*p && !g_ascii_isspace(*p) && *p != '"' && *p != '\'') p++;
                if (p > id)
                    g_string_append_len(out, id, p - id);
                else
                    p++;
            }
        }
    }
    return g_string_free(out, FALSE);
}

static char *
apply_text_transform(const char *src, const char *tt)
{
    if (!src || !tt) return NULL;
    if (strcmp(tt, "uppercase") == 0)
        return g_utf8_strup(src, -1);
    if (strcmp(tt, "lowercase") == 0)
        return g_utf8_strdown(src, -1);
    if (strcmp(tt, "capitalize") == 0) {
        GString *out = g_string_new(NULL);
        gboolean at_word_start = TRUE;
        for (const char *p = src; p && *p; ) {
            gunichar c = g_utf8_get_char(p);
            const char *next = g_utf8_next_char(p);
            if (g_unichar_isspace(c) || c == '-' || c == '/') {
                g_string_append_len(out, p, next - p);
                at_word_start = TRUE;
            } else {
                if (at_word_start) {
                    gunichar uc = g_unichar_totitle(c);
                    char buf[8];
                    gint nb = g_unichar_to_utf8(uc, buf);
                    g_string_append_len(out, buf, nb);
                    at_word_start = FALSE;
                } else {
                    g_string_append_len(out, p, next - p);
                }
            }
            p = next;
        }
        return g_string_free(out, FALSE);
    }
    return NULL;
}

typedef struct ns_progress_state {
    gboolean determinate;
    double max;
    double value;
    double frac;
} ns_progress_state;

typedef struct ns_meter_state {
    double min;
    double max;
    double value;
    double low;
    double high;
    double optimum;
    double frac;
    int quality;
} ns_meter_state;

static gboolean
parse_float_attr(const ns_node *n, const char *attr, double *out)
{
    return ns_html_parse_float(ns_element_get_attr(n, attr), out);
}

static ns_progress_state
progress_state_for(const ns_node *n)
{
    ns_progress_state st = {0};
    double parsed;
    st.max = parse_float_attr(n, "max", &parsed) && parsed > 0 ? parsed : 1.0;
    st.determinate = ns_element_get_attr(n, "value") != NULL;
    if (st.determinate) {
        st.value = parse_float_attr(n, "value", &parsed) && parsed > 0 ? parsed : 0.0;
        if (st.value > st.max) st.value = st.max;
        st.frac = st.max > 0 ? st.value / st.max : 0;
    } else {
        st.value = 0;
        st.frac = -1;
    }
    return st;
}

static ns_meter_state
meter_state_for(const ns_node *n)
{
    ns_meter_state st = {0};
    double parsed;
    st.min = parse_float_attr(n, "min", &parsed) ? parsed : 0.0;
    double candidate_max = parse_float_attr(n, "max", &parsed) ? parsed : 1.0;
    st.max = candidate_max >= st.min ? candidate_max : st.min;
    double candidate_value = parse_float_attr(n, "value", &parsed) ? parsed : 0.0;
    st.value = candidate_value;
    if (st.value < st.min) st.value = st.min;
    if (st.value > st.max) st.value = st.max;
    double candidate_low = parse_float_attr(n, "low", &parsed) ? parsed : st.min;
    st.low = candidate_low;
    if (st.low < st.min) st.low = st.min;
    if (st.low > st.max) st.low = st.max;
    double candidate_high = parse_float_attr(n, "high", &parsed) ? parsed : st.max;
    st.high = candidate_high;
    if (st.high < st.low) st.high = st.low;
    if (st.high > st.max) st.high = st.max;
    double midpoint = st.min + (st.max - st.min) / 2.0;
    double candidate_optimum = parse_float_attr(n, "optimum", &parsed) ? parsed : midpoint;
    st.optimum = candidate_optimum;
    if (st.optimum < st.min) st.optimum = st.min;
    if (st.optimum > st.max) st.optimum = st.max;
    st.frac = st.max > st.min ? (st.value - st.min) / (st.max - st.min) : 1.0;
    if (st.optimum >= st.low && st.optimum <= st.high)
        st.quality = (st.value >= st.low && st.value <= st.high) ? 0 : 1;
    else if (st.optimum < st.low)
        st.quality = st.value < st.low ? 0 : (st.value <= st.high ? 1 : 2);
    else
        st.quality = st.value > st.high ? 0 : (st.value >= st.low ? 1 : 2);
    return st;
}

static char *
quotes_string_for(const ns_style *s, int depth, gboolean closing)
{
    if (depth < 0) depth = 0;
    const ns_css_value *qv = s ? s->values[NS_CSS_QUOTES] : NULL;
    const char *spec = (qv && qv->kind == NS_CSS_V_KEYWORD) ? qv->u.keyword
                                                            : NULL;
    if (spec && strcmp(spec, "none") == 0) return g_strdup("");
    if (spec && strchr(spec, '"') == NULL && strchr(spec, '\'') == NULL)
        spec = NULL;
    if (spec) {
        GPtrArray *parts = g_ptr_array_new_with_free_func(g_free);
        for (const char *p = spec; *p; ) {
            if (*p == '"' || *p == '\'') {
                char term = *p++;
                GString *part = g_string_new(NULL);
                while (*p && *p != term)
                    ns_css_append_unescaped(part, &p);
                g_ptr_array_add(parts, g_string_free(part, FALSE));
                if (*p) p++;
            } else {
                p++;
            }
        }
        guint pairs = parts->len / 2;
        char *result = NULL;
        if (pairs > 0) {
            guint pair = (guint)depth < pairs ? (guint)depth : pairs - 1;
            result = g_strdup(g_ptr_array_index(parts,
                                                pair * 2 + (closing ? 1 : 0)));
        }
        g_ptr_array_free(parts, TRUE);
        if (result) return result;
    }
    if (depth % 2 == 0)
        return g_strdup(closing ? "\xe2\x80\x9d" : "\xe2\x80\x9c");
    return g_strdup(closing ? "\xe2\x80\x99" : "\xe2\x80\x98");
}

char *
ns_layout_pseudo_content_text(const ns_css_value *content, const ns_node *host)
{
    if (!content || content->kind != NS_CSS_V_KEYWORD || !content->u.keyword)
        return NULL;
    return resolve_pseudo_content(content->u.keyword, host);
}

static void
append_pseudo_content(GString *out, const ns_css_value *cv,
                      const ns_node *host, const ns_style *host_style)
{
    if (!cv || cv->kind != NS_CSS_V_KEYWORD || !cv->u.keyword) return;
    char *resolved = resolve_pseudo_content(cv->u.keyword, host);
    if (!resolved) return;
    char *quote = NULL;
    const char *txt = resolved;
    if (strcmp(resolved, "open-quote") == 0)
        txt = quote = quotes_string_for(host_style, 0, FALSE);
    else if (strcmp(resolved, "close-quote") == 0)
        txt = quote = quotes_string_for(host_style, 0, TRUE);
    else if (strcmp(resolved, "no-open-quote") == 0 ||
             strcmp(resolved, "no-close-quote") == 0) { g_free(resolved); return; }
    g_string_append(out, txt);
    g_free(quote);
    g_free(resolved);
}

static gboolean
pseudo_generates_box(const ns_style *ps)
{
    const ns_css_value *cv = ps ? ps->values[NS_CSS_CONTENT] : NULL;
    return cv && cv->kind == NS_CSS_V_KEYWORD && cv->u.keyword &&
           strcmp(cv->u.keyword, "none") != 0 &&
           strcmp(cv->u.keyword, "normal") != 0;
}

static ns_box *pseudo_block_with_text(const ns_style *ps, const char *txt);

static void
append_pseudo_inline(collector_ctx *ctx, const ns_style *ps,
                     const ns_node *host)
{
    if (!pseudo_generates_box(ps) || style_is_none(ps)) return;
    if (style_is_absolute_or_fixed(ps)) {
        append_pseudo_content(ctx->out, ps->values[NS_CSS_CONTENT], host, ps);
        return;
    }
    if (ctx->atomics && ns_display_is_atomic_inline(ns_css_display_of(ps))) {
        char *txt = resolve_pseudo_content(
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
    append_inline_spacer(ctx, length_or(ps->values[NS_CSS_MARGIN_LEFT], 0));
    append_inline_spacer(ctx, length_or(ps->values[NS_CSS_PADDING_LEFT], 0));
    append_pseudo_content(ctx->out, ps->values[NS_CSS_CONTENT], host, ps);
    append_inline_spacer(ctx, length_or(ps->values[NS_CSS_PADDING_RIGHT], 0));
    append_inline_spacer(ctx, length_or(ps->values[NS_CSS_MARGIN_RIGHT], 0));
}

static void
emit_open_select_option(collector_ctx *ctx, const ns_node *option)
{
    if (!option) return;
    g_string_append(ctx->out, "\xe2\x80\xa8");
    gsize start = ctx->out->len;
    g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
    char *t = ns_option_label_dup(option);
    if (t && *t) g_string_append(ctx->out, t);
    g_free(t);
    g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
    emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
                         start, ctx->out->len, option, ctx->styles);
}

static void
emit_listbox_option(collector_ctx *ctx, const ns_node *option, gboolean first)
{
    if (!option) return;
    if (!first) g_string_append(ctx->out, "\xe2\x80\xa8");
    gsize start = ctx->out->len;
    g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
    char *t = ns_option_label_dup(option);
    if (t && *t) g_string_append(ctx->out, t);
    g_free(t);
    g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
    emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
                         start, ctx->out->len, option, ctx->styles);
}

static const ns_node *
find_datalist_by_id(const ns_node *node, const char *id, int depth)
{
    if (!node || !id || depth >= NS_LAYOUT_MAX_DEPTH) return NULL;
    if (node->kind == NS_NODE_ELEMENT && node->name &&
        strcmp(node->name, "datalist") == 0) {
        const char *did = ns_element_get_attr(node, "id");
        if (did && strcmp(did, id) == 0) return node;
    }
    for (const ns_node *c = node->first_child; c; c = c->next_sibling) {
        const ns_node *m = find_datalist_by_id(c, id, depth + 1);
        if (m) return m;
    }
    return NULL;
}

static char *
datalist_option_value(const ns_node *option)
{
    const char *v = ns_element_get_attr(option, "value");
    if (v && *v) return g_strdup(v);
    return ns_option_label_dup(option);
}

static void
emit_datalist_suggestions(collector_ctx *ctx, const ns_node *input)
{
    const char *list_id = ns_element_get_attr(input, "list");
    if (!list_id || !*list_id) return;
    const ns_node *root = input;
    while (root->parent) root = root->parent;
    const ns_node *dl = find_datalist_by_id(root, list_id, 0);
    if (!dl) return;

    const char *cur = ns_input_used_value(input);
    char *needle = (cur && *cur) ? g_utf8_casefold(cur, -1) : NULL;
    int shown = 0;
    for (const ns_node *o = dl->first_child; o && shown < 8; o = o->next_sibling) {
        if (!ns_node_is_element_named(o, "option")) continue;
        char *val = datalist_option_value(o);
        if (!val || !*val) { g_free(val); continue; }
        gboolean match = TRUE;
        if (needle) {
            char *vl = g_utf8_casefold(val, -1);
            match = strstr(vl, needle) != NULL &&
                    g_ascii_strcasecmp(vl, needle) != 0;
            g_free(vl);
        }
        if (match) {
            g_string_append(ctx->out, "\xe2\x80\xa8");
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
            g_string_append(ctx->out, val);
            g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
            emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
                                 start, ctx->out->len, o, ctx->styles);
            shown++;
        }
        g_free(val);
    }
    g_free(needle);
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
                    emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
                }
                emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
            }
            return;
        }
        char *xformed = ctx->text_transform
                        ? apply_text_transform(n->text, ctx->text_transform)
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
    if (node_is_non_rendering(n)) return;
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
            ns_meter_state st = meter_state_for(n);
            frac = st.frac;
            if (st.quality == 0) { r = 0x2e; g = 0x9d; b = 0x54; }
            else if (st.quality == 1) { r = 0xd0; g = 0xa4; b = 0x1f; }
            else { r = 0xc4; g = 0x43; b = 0x3c; }
            alpha = 255;
        } else {
            ns_progress_state st = progress_state_for(n);
            frac = st.frac;
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
            if (!inline_run_at_line_start(ctx->out))
                g_string_append(ctx->out, "\xc2\xa0\xc2\xa0\xc2\xa0");
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0");
            int leading = text_input_leading_spaces(s);
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
            int size = text_input_columns(n);
            glong displayed_chars = 0;
            if (v && *v && is_password && !is_placeholder) {
                glong cps = g_utf8_strlen(v, -1);
                glong skip_cps = focused
                    ? text_input_focused_skip_cps(real_value, caret_byte,
                                                  cps, size)
                    : text_input_align_skip_cps(s, n, cps, size);
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
                        ? text_input_focused_skip_cps(real_value, caret_byte,
                                                      value_cps, size)
                        : text_input_align_skip_cps(s, n, value_cps, size);
                    const char *p = text_input_advance_cps(v, skip_cps);
                    const char *q = text_input_advance_cps(p, size);
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
                left_pad = text_input_align_offset(s, n, pad);
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
            emit_form_attr_sized(ctx->attrs, kind, start, ctx->out->len, n, ctx->styles);
            emit_control_text_style(ctx->attrs, s, start, ctx->out->len,
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
                    emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
                }
                emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
            }
            if (focused && g_datalist_open_for_layout)
                emit_datalist_suggestions(ctx, n);
        } else if (type && (g_ascii_strcasecmp(type, "submit") == 0 ||
                            g_ascii_strcasecmp(type, "button") == 0 ||
                            g_ascii_strcasecmp(type, "reset") == 0)) {
            const char *v = ns_element_get_attr(n, "value");
            gsize start = ctx->out->len;
            if (!v)
                v = g_ascii_strcasecmp(type, "submit") == 0 ? "Submit"
                  : g_ascii_strcasecmp(type, "reset")  == 0 ? "Reset"
                                                            : "Button";
            int lead  = control_pad_spaces(s, NS_CSS_PADDING_LEFT);
            int trail = control_pad_spaces(s, NS_CSS_PADDING_RIGHT);
            g_string_append(ctx->out, "\xc2\xa0");
            for (int i = 0; i < lead; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, v);
            for (int i = 0; i < trail; i++)
                g_string_append(ctx->out, "\xc2\xa0");
            g_string_append(ctx->out, "\xc2\xa0");
            emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "checkbox") == 0) {
            gboolean checked = ns_input_is_checked(n);
            gsize start = ctx->out->len;
            g_string_append(ctx->out, checked ? "\xe2\x98\x91" : "\xe2\x98\x90");
            emit_form_attr_sized(ctx->attrs,
                checked ? NS_INLINE_CHECKBOX_CHECKED : NS_INLINE_CHECKBOX,
                start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "radio") == 0) {
            gboolean checked = ns_input_is_checked(n);
            gsize start = ctx->out->len;
            g_string_append(ctx->out, checked ? "\xe2\x97\x89" : "\xe2\x97\x8b");
            emit_form_attr_sized(ctx->attrs,
                checked ? NS_INLINE_RADIO_CHECKED : NS_INLINE_RADIO,
                start, ctx->out->len, n, ctx->styles);
            g_string_append_c(ctx->out, ' ');
        } else if (type && g_ascii_strcasecmp(type, "file") == 0) {
            gsize start = ctx->out->len;
            g_string_append(ctx->out, "\xc2\xa0" "Choose File" "\xc2\xa0");
            emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
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
            emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
            guint8 r8 = 0, g8 = 0, b8 = 0;
            if (hex[0] == '#' && strlen(hex) >= 7) {
                unsigned int rv, gv, bv;
                if (sscanf(hex + 1, "%2x%2x%2x", &rv, &gv, &bv) == 3) {
                    r8 = (guint8)rv; g8 = (guint8)gv; b8 = (guint8)bv;
                }
            }
            emit_color_attr(ctx->attrs, swatch_start, swatch_end, r8, g8, b8, 255);
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
            emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
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
            emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD, start, ctx->out->len, n, ctx->styles);
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
                append_pseudo_content(pseudo, bs->before->values[NS_CSS_CONTENT],
                                      n, bs->before);
            if (bs && bs->after)
                append_pseudo_content(pseudo, bs->after->values[NS_CSS_CONTENT],
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
        emit_form_attr_sized(ctx->attrs, NS_INLINE_BUTTON, start, ctx->out->len, n, ctx->styles);
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
                        emit_listbox_option(ctx, opt, first);
                        first = FALSE;
                        shown++;
                    }
                } else if (strcmp(c->name, "option") == 0) {
                    emit_listbox_option(ctx, c, first);
                    first = FALSE;
                    shown++;
                }
            }
            if (shown == 0) {
                gsize start = ctx->out->len;
                g_string_append(ctx->out, "\xc2\xa0\xc2\xa0");
                emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
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
        emit_form_attr_sized(ctx->attrs, NS_INLINE_INPUT_FIELD,
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
                            emit_open_select_option(ctx, o);
                } else {
                    emit_open_select_option(ctx, opts[0]);
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
        emit_form_attr_sized(ctx->attrs, ta_kind, start, ctx->out->len,
                             n, ctx->styles);
        emit_control_text_style(ctx->attrs, s, start, ctx->out->len,
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
                emit_attr(ctx->attrs, NS_INLINE_SELECTION, s0, s1);
            }
            emit_attr(ctx->attrs, NS_INLINE_CARET, caret_pos, caret_pos + 1);
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
    append_inline_spacer(ctx, ml);
    gsize elem_start = ctx->out->len;
    append_inline_spacer(ctx, pl);
    gboolean bold   = tag_is_bold(n->name);
    gboolean italic = tag_is_italic(n->name);
    gboolean mono   = tag_is_monospace(n->name);
    gboolean uline  = tag_is_underline(n->name);
    gboolean oline  = FALSE;
    gboolean strike = tag_is_strike(n->name);
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
    int font_kerning_self = font_kerning_int_from_style(s);
    const char *font_ligatures_self = font_ligatures_from_style(s);
    const char *font_features_self = font_feature_settings_from_style(s);
    gboolean font_features_active =
        font_kerning_self >= 0 || font_ligatures_self != NULL ||
        font_features_self != NULL;
    gsize features_start = ctx->out->len;
    const char *font_variations_self = font_variation_settings_from_style(s);
    gsize variations_start = ctx->out->len;

    gboolean is_q = strcmp(n->name, "q") == 0;
    if (is_q) {
        char *open_q = quotes_string_for(s, ctx->q_depth, FALSE);
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
        emit_font_size_attr(ctx->attrs, fs_start, ctx->out->len, font_size_self);
    if (color_active && ctx->out->len > color_start)
        emit_color_attr(ctx->attrs, color_start, ctx->out->len, cr, cg, cb, ca);
    if (family_str && ctx->out->len > family_start)
        emit_font_family_attr(ctx->attrs, family_start, ctx->out->len, family_str);
    if (font_weight_active && ctx->out->len > weight_start)
        emit_font_weight_attr(ctx->attrs, weight_start, ctx->out->len, font_weight_self);
    if (font_stretch_active && ctx->out->len > stretch_start)
        emit_font_stretch_attr(ctx->attrs, stretch_start, ctx->out->len,
                               font_stretch_self);
    if (font_features_active && ctx->out->len > features_start)
        emit_font_features_attr(ctx->attrs, features_start, ctx->out->len,
                                font_kerning_self, font_ligatures_self,
                                font_features_self);
    if (font_variations_self && ctx->out->len > variations_start)
        emit_font_variations_attr(ctx->attrs, variations_start, ctx->out->len,
                                  font_variations_self);

    if (bold && --ctx->bold_depth == 0)
        emit_attr(ctx->attrs, NS_INLINE_BOLD, ctx->bold_start, ctx->out->len);
    if (italic && --ctx->italic_depth == 0)
        emit_attr(ctx->attrs, NS_INLINE_ITALIC, ctx->italic_start, ctx->out->len);
    if (mono && --ctx->mono_depth == 0)
        emit_attr(ctx->attrs, NS_INLINE_MONOSPACE, ctx->mono_start, ctx->out->len);
    if (uline && --ctx->underline_depth == 0)
        emit_attr_styled(ctx->attrs, NS_INLINE_UNDERLINE, ctx->underline_start,
                         ctx->out->len, decor_style);
    if (oline && --ctx->overline_depth == 0)
        emit_attr_styled(ctx->attrs, NS_INLINE_OVERLINE, ctx->overline_start,
                         ctx->out->len, decor_style);
    if (strike && --ctx->strike_depth == 0)
        emit_attr_styled(ctx->attrs, NS_INLINE_STRIKETHROUGH, ctx->strike_start,
                         ctx->out->len, decor_style);
    if (sup && ctx->out->len > rise_start)
        emit_attr(ctx->attrs, NS_INLINE_SUPERSCRIPT, rise_start, ctx->out->len);
    if (sub && ctx->out->len > rise_start)
        emit_attr(ctx->attrs, NS_INLINE_SUBSCRIPT, rise_start, ctx->out->len);
    if (small_caps && ctx->out->len > sc_start)
        emit_attr(ctx->attrs, NS_INLINE_SMALL_CAPS, sc_start, ctx->out->len);
    if (bidi_override)
        g_string_append(ctx->out, "\xe2\x80\xac");   /* PDF */
    if (bidi_isolate || bidi_plaintext)
        g_string_append(ctx->out, "\xe2\x81\xa9");   /* PDI */
    if (is_q) {
        ctx->q_depth--;
        char *close_q = quotes_string_for(s, ctx->q_depth, TRUE);
        g_string_append(ctx->out, close_q);
        g_free(close_q);
    }
    append_inline_spacer(ctx, pr);
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
    append_inline_spacer(ctx, mr);
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
                int fk = font_kerning_int_from_style(fl);
                const char *flig = font_ligatures_from_style(fl);
                const char *ffea = font_feature_settings_from_style(fl);
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
                const char *fvar = font_variation_settings_from_style(fl);
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
    char *resolved = resolve_pseudo_content(cv->u.keyword, host);
    if (!resolved) {
        if (!inline_atomic) return NULL;
        resolved = g_strdup("");
    }
    char *quote = NULL;
    const char *txt = resolved;
    if (strcmp(txt, "open-quote") == 0)
        txt = quote = quotes_string_for(ps, 0, FALSE);
    else if (strcmp(txt, "close-quote") == 0)
        txt = quote = quotes_string_for(ps, 0, TRUE);
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
    int fk = font_kerning_int_from_style(ps);
    const char *flig = font_ligatures_from_style(ps);
    const char *ffea = font_feature_settings_from_style(ps);
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
    const char *fvar = font_variation_settings_from_style(ps);
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
            tag_is_non_rendering(c->name)) {
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
                    tag_is_non_rendering(c->name)) {
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
        ? resolve_pseudo_content(cv->u.keyword, host) : g_strdup("");
    if (!resolved) return NULL;
    char *quote = NULL;
    const char *txt = resolved;
    if (strcmp(txt, "open-quote") == 0)
        txt = quote = quotes_string_for(ps, 0, FALSE);
    else if (strcmp(txt, "close-quote") == 0)
        txt = quote = quotes_string_for(ps, 0, TRUE);
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
static int float_side_of(const ns_style *s);

static gboolean
style_can_be_rendered_legend(const ns_style *s)
{
    return s && !style_is_none(s) && float_side_of(s) < 0 &&
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
        if (!clear_kind_of(s)) return NULL;
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
            tag_is_non_rendering(c->name)) {
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
                    tag_is_non_rendering(c->name)) {
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

static double measure_natural_width(ns_box *box, const ns_style *parent_style);
static double measure_max_content_width(ns_box *box, const ns_style *parent_style);
static double flex_gap_of(const ns_style *s, double basis);
static double flex_grow_of(const ns_box *c);
static double flex_shrink_of(const ns_box *c);
static gboolean flex_wraps(const ns_style *s);
static double width_contribution_keyword_limits(ns_box *box, double w,
                                                const ns_style *parent_style,
                                                gboolean max_content);
static gboolean flex_box_is_border_box(const ns_box *c);
static void legacy_align_block_child(ns_box *c, double avail_x, double avail_w,
                                     const ns_style *inherited);

static void shift_box_tree(ns_box *b, double dx, double dy);

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
            double css_w = inline_attr_control_width(r, box);
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
        layout_box(frag, col_w, child_inherited);
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

static const char *
overflow_axis_keyword(const ns_style *s, ns_css_prop axis)
{
    return ns_style_overflow_keyword(s, axis);
}

static gboolean
overflow_kw_clips(const char *ov)
{
    return ov && (g_ascii_strcasecmp(ov, "hidden") == 0 ||
                  g_ascii_strcasecmp(ov, "clip")   == 0 ||
                  g_ascii_strcasecmp(ov, "auto")   == 0 ||
                  g_ascii_strcasecmp(ov, "scroll") == 0);
}

static gboolean
overflow_kw_scrolls(const char *ov)
{
    return ov && (g_ascii_strcasecmp(ov, "auto")   == 0 ||
                  g_ascii_strcasecmp(ov, "scroll") == 0);
}

static gboolean
box_clips_children(const ns_box *b)
{
    if (!b || !b->style) return FALSE;
    if (b->kind != NS_BOX_BLOCK && b->kind != NS_BOX_TABLE_CAPTION &&
        b->kind != NS_BOX_TABLE_CELL) return FALSE;
    return overflow_kw_clips(overflow_axis_keyword(b->style, NS_CSS_OVERFLOW_X)) ||
           overflow_kw_clips(overflow_axis_keyword(b->style, NS_CSS_OVERFLOW_Y));
}

static gboolean
overflow_kw_scroll_container(const char *ov)
{
    return overflow_kw_clips(ov) && g_ascii_strcasecmp(ov, "clip") != 0;
}

static gboolean
box_is_scroll_container(const ns_box *b)
{
    if (!box_clips_children(b)) return FALSE;
    return overflow_kw_scroll_container(
               overflow_axis_keyword(b->style, NS_CSS_OVERFLOW_X)) ||
           overflow_kw_scroll_container(
               overflow_axis_keyword(b->style, NS_CSS_OVERFLOW_Y));
}

static gboolean
box_padding_contains(const ns_box *b, double x, double y)
{
    double x0 = b->x + b->margin.left + b->border.left;
    double y0 = b->y + b->margin.top  + b->border.top;
    double x1 = x0 + b->content_width + b->padding.left + b->padding.right;
    double y1 = y0 + b->content_height + b->padding.top + b->padding.bottom;
    return x >= x0 && x <= x1 && y >= y0 && y <= y1;
}

gboolean
ns_box_clips_out_point(const ns_box *b, double x, double y)
{
    return box_clips_children(b) && !box_padding_contains(b, x, y);
}

static double
min_width_of(ns_box *box, const ns_style *parent_style);
static double
min_content_width_of(ns_box *box, const ns_style *parent_style);

static int
float_side_of(const ns_style *s)
{
    if (!s) return -1;
    const ns_css_value *v = s->values[NS_CSS_FLOAT];
    if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return -1;
    if (strcmp(v->u.keyword, "left") == 0) return 0;
    if (strcmp(v->u.keyword, "right") == 0) return 1;
    return -1;
}

static int
multicol_distributable_children(const ns_box *box)
{
    int n = 0;
    for (const ns_box *c = box->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_BOX_BLOCK && c->kind != NS_BOX_TABLE) continue;
        if (style_is_absolute_or_fixed(c->style)) continue;
        if (float_side_of(c->style) >= 0) continue;
        if (++n >= 2) break;
    }
    return n;
}

static ns_box *
multicol_column_host(ns_box *box)
{
    ns_box *only = NULL;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_BOX_BLOCK) continue;
        if (style_is_absolute_or_fixed(c->style)) continue;
        if (float_side_of(c->style) >= 0) continue;
        if (only) return NULL;
        only = c;
    }
    if (!only || multicol_distributable_children(only) < 2) return NULL;
    return only;
}

static int
clear_kind_of(const ns_style *s)
{
    if (!s) return 0;
    const ns_css_value *v = s->values[NS_CSS_CLEAR];
    if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return 0;
    if (strcmp(v->u.keyword, "left") == 0) return 1;
    if (strcmp(v->u.keyword, "right") == 0) return 2;
    if (strcmp(v->u.keyword, "both") == 0) return 3;
    return 0;
}

typedef struct float_ref {
    ns_box *box;
    int side;
    double top, bottom;
    double outer_w;
} float_ref;

static gboolean
overflow_establishes_bfc(const ns_style *s)
{
    if (!s) return FALSE;
    const ns_css_value *values[] = {
        s->values[NS_CSS_OVERFLOW],
        s->values[NS_CSS_OVERFLOW_X],
        s->values[NS_CSS_OVERFLOW_Y],
    };
    for (guint i = 0; i < G_N_ELEMENTS(values); i++) {
        const ns_css_value *v = values[i];
        if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) continue;
        if (strcmp(v->u.keyword, "hidden") == 0 ||
            strcmp(v->u.keyword, "auto") == 0 ||
            strcmp(v->u.keyword, "scroll") == 0)
            return TRUE;
    }
    return FALSE;
}

static gboolean
box_establishes_bfc(const ns_box *box)
{
    if (!box || !box->parent) return TRUE;
    if (box->dom && box->dom->kind == NS_NODE_ELEMENT &&
        (!box->dom->parent || box->dom->parent->kind != NS_NODE_ELEMENT))
        return TRUE;
    if (box->kind == NS_BOX_TABLE_CELL || box->kind == NS_BOX_TABLE ||
        box->kind == NS_BOX_TABLE_CAPTION)
        return TRUE;
    if (float_side_of(box->style) >= 0 ||
        style_is_absolute_or_fixed(box->style) ||
        style_is_multicol(box->style) ||
        style_is_flex_container(box->style) ||
        style_is_grid_container(box->style))
        return TRUE;
    if (style_is_flex_container(box->parent->style) ||
        style_is_grid_container(box->parent->style))
        return TRUE;
    if (box->is_rendered_legend ||
        ns_node_is_element_named(box->dom, "fieldset"))
        return TRUE;
    const ns_css_value *d = box->style
        ? box->style->values[NS_CSS_DISPLAY] : NULL;
    if (keyword_is(d, "flow-root") || keyword_is(d, "inline-block") ||
        keyword_is(d, "inline-table"))
        return TRUE;
    return style_is_block_level(box->style) &&
           overflow_establishes_bfc(box->style);
}

static void
collect_escaping_floats(ns_box *box, GArray *floats, int depth)
{
    if (!box || !floats || depth >= NS_LAYOUT_MAX_DEPTH) return;
    int side = float_side_of(box->style);
    if (side >= 0) {
        float_ref ref = {
            .box = box,
            .side = side,
            .top = box->y,
            .bottom = box->y + box->margin.top + box->content_height
                + box->padding.top + box->padding.bottom
                + box->border.top + box->border.bottom
                + box->margin.bottom,
            .outer_w = box->content_width
                + box->padding.left + box->padding.right
                + box->border.left + box->border.right
                + box->margin.left + box->margin.right,
        };
        g_array_append_val(floats, ref);
        return;
    }
    if (box_establishes_bfc(box)) return;
    for (ns_box *child = box->first_child; child;
         child = child->next_sibling)
        collect_escaping_floats(child, floats, depth + 1);
}

static void
floats_offsets_at(const GArray *floats, double y,
                  double *left_out, double *right_out)
{
    double l = 0, r = 0;
    if (floats) {
        for (guint i = 0; i < floats->len; i++) {
            const float_ref *f = &g_array_index(floats, float_ref, i);
            if (y < f->top || y >= f->bottom) continue;
            if (f->side == 0) l += f->outer_w;
            else              r += f->outer_w;
        }
    }
    *left_out = l;
    *right_out = r;
}

static void
floats_offsets_over(const GArray *floats, double top, double bottom,
                    double *left_out, double *right_out)
{
    double l = 0, r = 0;
    floats_offsets_at(floats, top, &l, &r);
    if (floats && bottom > top) {
        for (guint i = 0; i < floats->len; i++) {
            const float_ref *f = &g_array_index(floats, float_ref, i);
            if (f->top <= top || f->top >= bottom) continue;
            double bl = 0, br = 0;
            floats_offsets_at(floats, f->top, &bl, &br);
            if (bl > l) l = bl;
            if (br > r) r = br;
        }
    }
    *left_out = l;
    *right_out = r;
}

static double
floats_clear_y(const GArray *floats, double y, int clear)
{
    if (!floats || clear == 0) return y;
    double out = y;
    for (guint i = 0; i < floats->len; i++) {
        const float_ref *f = &g_array_index(floats, float_ref, i);
        if (clear == 1 && f->side != 0) continue;
        if (clear == 2 && f->side != 1) continue;
        if (f->bottom > out) out = f->bottom;
    }
    return out;
}

static double
floats_max_bottom(const GArray *floats)
{
    double y = 0;
    if (!floats) return y;
    for (guint i = 0; i < floats->len; i++) {
        const float_ref *f = &g_array_index(floats, float_ref, i);
        if (f->bottom > y) y = f->bottom;
    }
    return y;
}

static gboolean
floats_advance_to_readable_width(const GArray *floats, double cw,
                                 double *y, double *left_out,
                                 double *right_out)
{
    if (!floats || floats->len == 0 || !y) return FALSE;
    double min_w = cw * 0.40;
    if (min_w > 260) min_w = 260;
    if (min_w < 120) min_w = 120;
    gboolean moved = FALSE;
    for (;;) {
        double left = 0, right = 0;
        floats_offsets_at(floats, *y, &left, &right);
        double avail = cw - left - right;
        if (avail >= min_w || cw <= min_w) {
            if (left_out) *left_out = left;
            if (right_out) *right_out = right;
            return moved;
        }
        double next_y = *y;
        gboolean advanced = FALSE;
        for (guint i = 0; i < floats->len; i++) {
            const float_ref *f = &g_array_index(floats, float_ref, i);
            if (f->bottom > *y && (!advanced || f->bottom < next_y)) {
                next_y = f->bottom;
                advanced = TRUE;
            }
        }
        if (!advanced || next_y <= *y) {
            if (left_out) *left_out = left;
            if (right_out) *right_out = right;
            return moved;
        }
        *y = next_y;
        moved = TRUE;
    }
}

typedef struct margin_above {
    const ns_box *box;
    double collapsed;
} margin_above;

static __thread margin_above g_margin_above;

typedef struct inherited_floats {
    const ns_box *box;
    const GArray *floats;
    double inner_x, cw;
} inherited_floats;

static __thread inherited_floats g_inherited_floats;

static gint
double_cmp(gconstpointer a, gconstpointer b)
{
    double x = *(const double *)a, y = *(const double *)b;
    return x < y ? -1 : x > y ? 1 : 0;
}

static void
floats_inherit(GArray *floats, const inherited_floats *from, double inner_x,
               double cw, double from_y)
{
    const GArray *src = from->floats;
    if (!src || src->len == 0) return;
    GArray *edges = g_array_sized_new(FALSE, FALSE, sizeof(double),
                                      src->len * 2);
    for (guint i = 0; i < src->len; i++) {
        const float_ref *f = &g_array_index(src, float_ref, i);
        if (f->bottom <= from_y) continue;
        g_array_append_val(edges, f->top);
        g_array_append_val(edges, f->bottom);
    }
    g_array_sort(edges, double_cmp);
    for (guint i = 0; i + 1 < edges->len; i++) {
        double top = g_array_index(edges, double, i);
        double bottom = g_array_index(edges, double, i + 1);
        if (bottom <= top || bottom <= from_y) continue;
        double l = 0, r = 0;
        floats_offsets_at(src, top, &l, &r);
        double left_in = from->inner_x + l - inner_x;
        double right_in = inner_x + cw - (from->inner_x + from->cw - r);
        if (l > 0 && left_in > 0) {
            float_ref band = { NULL, 0, top, bottom, left_in };
            g_array_append_val(floats, band);
        }
        if (r > 0 && right_in > 0) {
            float_ref band = { NULL, 1, top, bottom, right_in };
            g_array_append_val(floats, band);
        }
    }
    g_array_free(edges, TRUE);
}

static void
floats_shift_placed(GArray *floats, double dy)
{
    if (dy == 0) return;
    for (guint i = 0; i < floats->len; i++) {
        float_ref *f = &g_array_index(floats, float_ref, i);
        if (!f->box) continue;
        shift_box_tree(f->box, 0, dy);
        f->top += dy;
        f->bottom += dy;
    }
}

static double
floats_band_bottom(const GArray *floats, double y)
{
    double bottom = -1;
    for (guint i = 0; floats && i < floats->len; i++) {
        const float_ref *f = &g_array_index(floats, float_ref, i);
        if (y < f->top || y >= f->bottom) continue;
        if (bottom < 0 || f->bottom < bottom) bottom = f->bottom;
    }
    return bottom;
}

static gboolean
replaced_size_keyword(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           (strcmp(v->u.keyword, "fit-content") == 0 ||
            strcmp(v->u.keyword, "min-content") == 0 ||
            strcmp(v->u.keyword, "max-content") == 0);
}

static void
layout_image(ns_box *box, double parent_content_width)
{
    edges_from_style(box->style, parent_content_width,
                     &box->margin, &box->padding, &box->border);
    const ns_css_value *wv  = box->style ? box->style->values[NS_CSS_WIDTH]      : NULL;
    const ns_css_value *hv  = box->style ? box->style->values[NS_CSS_HEIGHT]     : NULL;
    const ns_css_value *mxw = box->style ? box->style->values[NS_CSS_MAX_WIDTH]  : NULL;
    const ns_css_value *mxh = box->style ? box->style->values[NS_CSS_MAX_HEIGHT] : NULL;
    const ns_css_value *mnw = box->style ? box->style->values[NS_CSS_MIN_WIDTH]  : NULL;
    const ns_css_value *mnh = box->style ? box->style->values[NS_CSS_MIN_HEIGHT] : NULL;

    gboolean declared_size = box->media && box->media->declared_image_size;
    gboolean placeholder_size = box->media && box->media->placeholder_image_size;
    const char *parent_flex_dir = box->parent
        ? flex_direction_of(box->parent->style) : "row";
    gboolean flex_row_item = box->parent &&
        style_is_flex_container(box->parent->style) &&
        (strcmp(parent_flex_dir, "row") == 0 ||
         strcmp(parent_flex_dir, "row-reverse") == 0);
    double pct_width_base = parent_content_width;
    if (flex_row_item && box->parent->content_width > 0)
        pct_width_base = box->parent->content_width;
    double w = -1, h = -1;
    if (wv && (wv->kind == NS_CSS_V_LENGTH || wv->kind == NS_CSS_V_CALC))
        w = length_resolve(wv, pct_width_base, -1);
    else if (height_keyword_stretches(wv)) {
        w = parent_content_width
          - box->margin.left - box->margin.right
          - box->padding.left - box->padding.right
          - box->border.left - box->border.right;
        if (w < 0) w = 0;
    }
    if (hv && (hv->kind == NS_CSS_V_LENGTH || hv->kind == NS_CSS_V_CALC)) {
        if (value_is_percent(hv)) {
            double cb_h = containing_block_definite_height(box);
            if (cb_h >= 0) {
                h = (hv->kind == NS_CSS_V_CALC)
                    ? hv->u.calc.pct / 100.0 * cb_h + hv->u.calc.px
                    : hv->u.length.v * cb_h / 100.0;
            }
        } else {
            h = resolve_used_height(box, hv, parent_content_width, -1);
        }
    } else if (height_keyword_stretches(hv)) {
        double cb_h = containing_block_definite_height(box);
        if (cb_h >= 0) {
            h = cb_h - box->margin.top - box->margin.bottom
              - box->padding.top - box->padding.bottom
              - box->border.top - box->border.bottom;
            if (h < 0) h = 0;
        }
    }

    const ns_image *img = box->media ? (const ns_image *)box->media->image : NULL;
    double density = box->media && box->media->image_density > 0
        ? box->media->image_density : 1.0;
    double nat_w = (img && img->loaded && img->natural_width > 0)
                   ? (double)img->natural_width / density : -1;
    double nat_h = (img && img->loaded && img->natural_height > 0)
                   ? (double)img->natural_height / density : -1;
    if (declared_size) {
        if (nat_w < 0 && box->content_width  > 0) nat_w = box->content_width;
        if (nat_h < 0 && box->content_height > 0) nat_h = box->content_height;
    }
    double intrinsic_ratio = nat_w > 0 && nat_h > 0 ? nat_w / nat_h : -1;
    if (box->kind == NS_BOX_SVG && box->dom) {
        ns_svg_size svg_size;
        ns_svg_intrinsic_size(box->dom, &svg_size);
        if (svg_size.has_ratio && svg_size.ratio > 0)
            intrinsic_ratio = svg_size.ratio;
    }
    gboolean ratio_with_auto = FALSE;
    double specified_ratio = box->style
        ? aspect_ratio_number(box->style->values[NS_CSS_ASPECT_RATIO], &ratio_with_auto) : -1;
    gboolean ratio_overrides = specified_ratio > 0 &&
        (!ratio_with_auto || intrinsic_ratio <= 0);
    if (ratio_overrides) intrinsic_ratio = specified_ratio;
    if (box->media)
        box->media->size_independent_of_image =
            (w >= 0 && h >= 0) || declared_size || placeholder_size;

    gboolean metadata_video =
        box->kind == NS_BOX_VIDEO && node_has_media_metadata(box->dom);
    gboolean video_without_metadata = box->kind == NS_BOX_VIDEO && !metadata_video &&
        ns_node_is_element_named(box->dom, "video");
    if (nat_w < 0 && box->content_width  > 0) nat_w = box->content_width;
    if (nat_h < 0 && box->content_height > 0) nat_h = box->content_height;

    gboolean ratio_only = box->media && box->media->intrinsic_ratio_only;
    gboolean w_specified = w >= 0;
    gboolean h_specified = h >= 0;
    if (w < 0 && h < 0) {
        if (ratio_only && intrinsic_ratio > 0) {
            double cb_h = containing_block_definite_height(box);
            if (parent_content_width > 0 && cb_h >= 0) {
                w = parent_content_width; h = cb_h;
            } else if (cb_h >= 0) {
                h = cb_h; w = h * intrinsic_ratio;
            } else if (parent_content_width > 0) {
                w = parent_content_width; h = w / intrinsic_ratio;
            } else { w = 0; h = 0; }
        } else if (nat_w > 0 && nat_h > 0) {
            w = nat_w;
            h = ratio_overrides && !ratio_with_auto ? w / intrinsic_ratio : nat_h;
        } else { w = 0; h = 0; }
    } else if (w < 0) {
        w = intrinsic_ratio > 0 ? h * intrinsic_ratio
          : video_without_metadata ? 300 : h;
    } else if (h < 0) {
        h = intrinsic_ratio > 0 ? w / intrinsic_ratio
          : video_without_metadata ? 150 : w;
    }
    if (metadata_video && h <= 0 && w > 0 && nat_w > 0 && nat_h > 0)
        h = w * (nat_h / nat_w);
    if (metadata_video && w <= 0 && h > 0 && nat_w > 0 && nat_h > 0)
        w = h * (nat_w / nat_h);

    double max_w = length_resolve(mxw, pct_width_base, -1);
    double max_h = resolve_used_height(box, mxh, parent_content_width, -1);
    double min_w = length_resolve(mnw, pct_width_base, -1);
    double min_h = resolve_used_height(box, mnh, parent_content_width, -1);
    double keyword_w = intrinsic_ratio > 0 && h_specified ? h * intrinsic_ratio : nat_w;
    if (max_w < 0 && replaced_size_keyword(mxw) && keyword_w >= 0) max_w = keyword_w;
    if (min_w < 0 && replaced_size_keyword(mnw) && keyword_w >= 0) min_w = keyword_w;

    if (max_w >= 0 && w > max_w) {
        if (h > 0 && w > 0 && !h_specified) h *= max_w / w;
        w = max_w;
    }
    if (max_h >= 0 && h > max_h) {
        if (w > 0 && h > 0 && !w_specified) w *= max_h / h;
        h = max_h;
    }
    if (min_w >= 0 && w < min_w) {
        if (h > 0 && w > 0 && !h_specified && intrinsic_ratio > 0) h = min_w / intrinsic_ratio;
        w = min_w;
    }
    if (min_h >= 0 && h < min_h) {
        if (w > 0 && h > 0 && !w_specified && intrinsic_ratio > 0) w = min_h * intrinsic_ratio;
        h = min_h;
    }

    box->content_width = w;
    box->content_height = h;
}

static gboolean
inline_atomic_needs_layout(const ns_box *ab)
{
    if (!ab) return FALSE;
    return !(g_abs_ph_set && g_hash_table_contains(g_abs_ph_set, ab));
}

static double
inline_atomic_measure_basis(const ns_box *box)
{
    const ns_css_value *wv = box && box->style
        ? box->style->values[NS_CSS_WIDTH] : NULL;
    gboolean replaced = box && (box->kind == NS_BOX_IMAGE ||
                                box->kind == NS_BOX_VIDEO ||
                                box->kind == NS_BOX_SVG);
    if (value_is_percent(wv) && !replaced) {
        double content = measure_natural_width((ns_box *)box, box->style);
        if (content >= 0) return content;
    }
    double basis = ns_css_container_w();
    if (!(basis > 0)) {
        for (const ns_box *p = box ? box->parent : NULL; p; p = p->parent) {
            if (p->content_width > 0) {
                basis = p->content_width;
                break;
            }
        }
    }
    if (!(basis > 0)) basis = ns_css_viewport_w();
    if (!(basis > 0)) basis = 1000;
    if (basis < 32) basis = 32;
    if (basis > 1600) basis = 1600;
    return basis;
}

static gboolean
box_inline_size_is_definite(const ns_box *box)
{
    for (const ns_box *b = box; b; b = b->parent) {
        if (b->kind == NS_BOX_TABLE_CELL) return FALSE;
        const ns_style *s = b->style;
        if (!s) continue;
        const ns_css_value *wv = s->values[NS_CSS_WIDTH];
        gboolean fixed = wv && (wv->kind == NS_CSS_V_LENGTH ||
                                wv->kind == NS_CSS_V_CALC) &&
                         !value_is_percent(wv);
        if (fixed) return TRUE;
        if (style_is_absolute_or_fixed(s) || float_side_of(s) >= 0 ||
            ns_display_is_atomic_inline(ns_css_display_of(s)))
            return FALSE;
        if (b->parent && (style_is_flex_container(b->parent->style) ||
                          style_is_grid_container(b->parent->style)))
            return FALSE;
    }
    return TRUE;
}

static double
replaced_height_for_width(const ns_box *box)
{
    const ns_css_value *hv = box->style ? box->style->values[NS_CSS_HEIGHT] : NULL;
    if (!hv || !(hv->kind == NS_CSS_V_LENGTH || hv->kind == NS_CSS_V_CALC))
        return -1;
    double h;
    if (!value_is_percent(hv)) {
        h = length_resolve(hv, 0, -1);
    } else {
        double cb_h = containing_block_definite_height(box);
        h = cb_h >= 0 ? resolve_height_with_basis(hv, 0, cb_h, -1) : -1;
    }
    if (h > 0 && ns_css_keyword_is(box->style->values[NS_CSS_BOX_SIZING],
                                   "border-box")) {
        ns_edges m = {0}, pd = {0}, bd = {0};
        edges_from_style(box->style, 0, &m, &pd, &bd);
        h -= pd.top + pd.bottom + bd.top + bd.bottom;
        if (h < 0) h = 0;
    }
    return h;
}

static const ns_box *
replaced_containing_box(const ns_box *box)
{
    for (const ns_box *p = box->parent; p; p = p->parent)
        if (p->kind != NS_BOX_INLINE) return p;
    return NULL;
}

static gboolean
replaced_containing_block_is_definite(const ns_box *box)
{
    if (box->parent && style_is_grid_container(box->parent->style))
        return FALSE;
    const ns_box *cb = replaced_containing_box(box);
    return cb && box_inline_size_is_definite(cb);
}

static double
ratio_only_replaced_width(const ns_box *box, gboolean max_content)
{
    double h = replaced_height_for_width(box);
    if (h > 0) {
        ns_svg_size size;
        ns_svg_intrinsic_size(box->dom, &size);
        if (size.has_ratio && size.ratio > 0) return h * size.ratio;
    }
    if (max_content && replaced_containing_block_is_definite(box)) {
        const ns_box *cb = replaced_containing_box(box);
        if (cb && cb->content_width > 0) return cb->content_width;
    }
    return 0;
}

static double
loaded_image_auto_width(const ns_box *box, const ns_image *img)
{
    double h = replaced_height_for_width(box);
    if (h > 0) return h * img->natural_width / img->natural_height;
    double density = box->media->image_density > 0
        ? box->media->image_density : 1.0;
    return img->natural_width / density;
}

static double
svg_auto_width(const ns_box *box)
{
    ns_svg_size size;
    ns_svg_intrinsic_size(box->dom, &size);
    double h = replaced_height_for_width(box);
    if (h > 0 && size.has_ratio && size.ratio > 0) return h * size.ratio;
    return size.has_width && size.width > 0 ? size.width : 300;
}

static double
width_attribute_px(const ns_node *dom)
{
    const char *width_attr = dom ? ns_element_get_attr(dom, "width") : NULL;
    if (!width_attr || strchr(width_attr, '%')) return 0;
    return image_dimension_attr(dom, "width");
}

static double
replaced_auto_width(const ns_box *box)
{
    if (box->media && box->media->intrinsic_ratio_only) {
        double w = ratio_only_replaced_width(box, FALSE);
        return w > 0 ? w : 300;
    }
    const ns_image *img = box->media ? (const ns_image *)box->media->image : NULL;
    if (img && img->loaded && img->natural_width > 0 && img->natural_height > 0)
        return loaded_image_auto_width(box, img);
    if (box->kind == NS_BOX_SVG && box->dom) return svg_auto_width(box);
    double attr = width_attribute_px(box->dom);
    if (attr > 0) return attr;
    if (box->kind == NS_BOX_VIDEO) return 300;
    return box->media && box->media->placeholder_image_size ? 200 : 0;
}

static gboolean
replaced_width_is_cyclic(const ns_box *box)
{
    if (!box || !(box->kind == NS_BOX_IMAGE || box->kind == NS_BOX_VIDEO ||
                  box->kind == NS_BOX_SVG))
        return FALSE;
    const ns_css_value *wv = box->style ? box->style->values[NS_CSS_WIDTH] : NULL;
    if (value_is_percent(wv)) return !replaced_containing_block_is_definite(box);
    gboolean sized = wv && (wv->kind == NS_CSS_V_LENGTH || wv->kind == NS_CSS_V_CALC);
    return !sized && box->media && box->media->intrinsic_ratio_only;
}

static gboolean
replaced_width_is_percent(const ns_box *box)
{
    return (box->kind == NS_BOX_IMAGE || box->kind == NS_BOX_VIDEO ||
            box->kind == NS_BOX_SVG) && box->style &&
           value_is_percent(box->style->values[NS_CSS_WIDTH]);
}

static double
replaced_intrinsic_contribution(const ns_box *box, gboolean max_content)
{
    const ns_css_value *wv = box->style ? box->style->values[NS_CSS_WIDTH] : NULL;
    double w;
    if (value_is_percent(wv))
        w = max_content ? replaced_auto_width(box) : 0;
    else
        w = ratio_only_replaced_width(box, max_content);
    const ns_css_value *mxw = box->style ? box->style->values[NS_CSS_MAX_WIDTH] : NULL;
    const ns_css_value *mnw = box->style ? box->style->values[NS_CSS_MIN_WIDTH] : NULL;
    double max_w = value_is_percent(mxw) ? -1 : length_resolve(mxw, 0, -1);
    double min_w = value_is_percent(mnw) ? -1 : length_resolve(mnw, 0, -1);
    if (max_w >= 0 && w > max_w) w = max_w;
    if (min_w >= 0 && w < min_w) w = min_w;
    return w;
}

typedef struct ns_atomic_geometry {
    ns_box  *box;
    double   width, height;
    ns_edges margin, padding, border;
} ns_atomic_geometry;

static GArray *
measure_inline_atomics_begin(ns_box *box, const ns_style *parent_style,
                             gboolean max_content)
{
    GArray *saved = NULL;
    if (!box->inline_atomics) return NULL;
    for (guint ai = 0; ai < box->inline_atomics->len; ai++) {
        ns_box *ab = g_array_index(box->inline_atomics, ns_inline_atomic, ai).box;
        if (!ab) continue;
        gboolean cyclic = replaced_width_is_cyclic(ab) ||
            (!max_content && replaced_width_is_percent(ab));
        if (cyclic) {
            if (!saved)
                saved = g_array_new(FALSE, FALSE, sizeof(ns_atomic_geometry));
            ns_atomic_geometry g = {
                .box = ab, .width = ab->content_width, .height = ab->content_height,
                .margin = ab->margin, .padding = ab->padding, .border = ab->border,
            };
            g_array_append_val(saved, g);
        }
        if (inline_atomic_needs_layout(ab))
            layout_box(ab, inline_atomic_measure_basis(ab), parent_style);
        if (cyclic) {
            double w = replaced_intrinsic_contribution(ab, max_content);
            double ratio = ab->content_width > 0 && ab->content_height > 0
                ? ab->content_width / ab->content_height : 0;
            ab->content_width = w;
            if (ratio > 0) ab->content_height = w / ratio;
        }
    }
    return saved;
}

static void
measure_inline_atomics_end(GArray *saved)
{
    if (!saved) return;
    for (guint i = 0; i < saved->len; i++) {
        ns_atomic_geometry *g = &g_array_index(saved, ns_atomic_geometry, i);
        g->box->content_width = g->width;
        g->box->content_height = g->height;
        g->box->margin = g->margin;
        g->box->padding = g->padding;
        g->box->border = g->border;
    }
    g_array_free(saved, TRUE);
}

static double
measure_natural_width(ns_box *box, const ns_style *parent_style)
{
    if (!box) return 0;
    if (box->kind == NS_BOX_INLINE)
        return ns_layout_inline_natural_width(box, parent_style);
    if (box->kind == NS_BOX_IMAGE || box->kind == NS_BOX_VIDEO ||
        box->kind == NS_BOX_SVG) {
        if (replaced_width_is_cyclic(box))
            return replaced_intrinsic_contribution(box, TRUE);
        const ns_css_value *wv = box->style
            ? box->style->values[NS_CSS_WIDTH] : NULL;
        if (wv && (wv->kind == NS_CSS_V_LENGTH ||
                   wv->kind == NS_CSS_V_CALC)) {
            double styled = length_resolve(wv, inline_atomic_measure_basis(box), -1);
            if (styled >= 0) return styled;
        }
        return box->content_width > 0 ? box->content_width : 200;
    }
    if (box->kind == NS_BOX_TEXT) {
        return box->content_width > 0 ? box->content_width : 0;
    }
    {
        const ns_css_value *wv = box->style ? box->style->values[NS_CSS_WIDTH] : NULL;
        double w = -1;
        if (wv && wv->kind == NS_CSS_V_LENGTH &&
            (wv->u.length.unit == NS_CSS_UNIT_PX ||
             wv->u.length.unit == NS_CSS_UNIT_NUMBER))
            w = wv->u.length.v;
        else if (wv && wv->kind == NS_CSS_V_CALC && !value_is_percent(wv))
            w = length_resolve(wv, 0, -1);
        if (w >= 0) {
            if (flex_box_is_border_box(box)) {
                ns_edges m = {0}, pd = {0}, bd = {0};
                edges_from_style(box->style, 0, &m, &pd, &bd);
                w -= pd.left + pd.right + bd.left + bd.right;
                if (w < 0) w = 0;
            }
            return width_contribution_keyword_limits(box, w, parent_style,
                                                     TRUE);
        }
        if (keyword_is(wv, "min-content"))
            return width_contribution_keyword_limits(
                box, min_content_width_of(box, parent_style),
                parent_style, TRUE);
    }
    return width_contribution_keyword_limits(
        box, measure_max_content_width(box, parent_style), parent_style, TRUE);
}

static double
definite_width_limit(const ns_box *box, const ns_css_value *v)
{
    if (!v || !(v->kind == NS_CSS_V_LENGTH || v->kind == NS_CSS_V_CALC) ||
        value_is_percent(v))
        return -1;
    double limit = length_resolve(v, 0, -1);
    if (limit < 0) return -1;
    if (flex_box_is_border_box(box)) {
        ns_edges m = {0}, pd = {0}, bd = {0};
        edges_from_style(box->style, 0, &m, &pd, &bd);
        limit -= pd.left + pd.right + bd.left + bd.right;
        if (limit < 0) limit = 0;
    }
    return limit;
}

static double
width_contribution_keyword_limits(ns_box *box, double w,
                                  const ns_style *parent_style,
                                  gboolean max_content)
{
    if (!box->style || (box->kind != NS_BOX_BLOCK && box->kind != NS_BOX_TABLE))
        return w;
    const ns_css_value *mxw = box->style->values[NS_CSS_MAX_WIDTH];
    const ns_css_value *mnw = box->style->values[NS_CSS_MIN_WIDTH];
    double max_limit = definite_width_limit(box, mxw);
    if (max_limit >= 0 && w > max_limit) w = max_limit;
    double min_limit = definite_width_limit(box, mnw);
    if (min_limit >= 0 && w < min_limit) w = min_limit;
    if (size_keyword_is_intrinsic(mxw)) {
        gboolean use_max = keyword_is(mxw, "max-content") ||
                           (max_content && keyword_is(mxw, "fit-content"));
        double m = use_max ? measure_max_content_width(box, parent_style)
                           : min_content_width_of(box, parent_style);
        if (m >= 0 && w > m) w = m;
    }
    if (size_keyword_is_intrinsic(mnw)) {
        gboolean use_max = keyword_is(mnw, "max-content") ||
                           (max_content && keyword_is(mnw, "fit-content"));
        double m = use_max ? measure_max_content_width(box, parent_style)
                           : min_content_width_of(box, parent_style);
        if (m >= 0 && w < m) w = m;
    }
    return w;
}

static double
measure_max_content_width(ns_box *box, const ns_style *parent_style)
{
    if (box->kind == NS_BOX_INLINE || box->kind == NS_BOX_IMAGE ||
        box->kind == NS_BOX_VIDEO || box->kind == NS_BOX_SVG ||
        box->kind == NS_BOX_TEXT)
        return measure_natural_width(box, parent_style);
    if (style_contains_inline_size(box->style)) return 0;
    const ns_style *child_style = box->style ? box->style : parent_style;
    if (box->kind == NS_BOX_TABLE)
        return ns_layout_table_intrinsic_width(box, child_style, FALSE);
    if (box->style && style_is_grid_container(box->style)) {
        double gw = ns_layout_grid_natural_width(box, child_style);
        if (gw > 0) return gw;
    }
    gboolean flex_row = style_is_flex_container(box->style) &&
        strncmp(flex_direction_of(box->style), "row", 3) == 0;
    double max_child = 0;
    double float_row = 0;
    double row_sum = 0;
    int flex_items = 0;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        double w = measure_natural_width(c, child_style);
        int fside = float_side_of(c->style);
        double outer = w;
        if (c->style) {
            ns_edges m = {0}, pd = {0}, bd = {0};
            edges_from_style(c->style, 0, &m, &pd, &bd);
            outer += m.left + m.right + pd.left + pd.right + bd.left + bd.right;
        }
        if (flex_row) {
            row_sum += outer;
            flex_items++;
        } else if (box->kind == NS_BOX_BLOCK ||
                   box->kind == NS_BOX_TABLE_CELL ||
                   box->kind == NS_BOX_TABLE_CAPTION) {
            if (fside >= 0) {
                float_row += outer;
                if (float_row > max_child) max_child = float_row;
            } else {
                if (outer > max_child) max_child = outer;
                if (w > max_child) max_child = w;
                float_row = 0;
            }
        } else {
            max_child += w;
        }
    }
    if (flex_row && flex_items > 1) {
        const ns_css_value *g = box->style->values[NS_CSS_COLUMN_GAP];
        if (!g || g->kind != NS_CSS_V_LENGTH) g = box->style->values[NS_CSS_GAP];
        double gap = (g && g->kind == NS_CSS_V_LENGTH) ? g->u.length.v : 0;
        row_sum += gap * (flex_items - 1);
    }
    return flex_row ? row_sum : max_child;
}

static int g_min_measure_depth;

static double measure_min_width(ns_box *box, const ns_style *parent_style);

static double
min_width_of(ns_box *box, const ns_style *parent_style)
{
    g_min_measure_depth++;
    double w = measure_min_width(box, parent_style);
    g_min_measure_depth--;
    return w;
}

static double
measure_min_width(ns_box *box, const ns_style *parent_style)
{
    if (!box) return 0;
    if (box->kind == NS_BOX_INLINE)
        return ns_layout_inline_min_width(box, parent_style);
    if (box->kind == NS_BOX_IMAGE || box->kind == NS_BOX_VIDEO ||
        box->kind == NS_BOX_SVG) {
        const ns_css_value *max_width = box->style
            ? box->style->values[NS_CSS_MAX_WIDTH] : NULL;
        if (max_width && max_width->kind == NS_CSS_V_LENGTH &&
            max_width->u.length.unit == NS_CSS_UNIT_PERCENT)
            return 0;
        if (replaced_width_is_cyclic(box) ||
            (g_min_measure_depth > 1 && replaced_width_is_percent(box)))
            return replaced_intrinsic_contribution(box, FALSE);
        return box->content_width > 0 ? box->content_width : 200;
    }
    if (box->kind == NS_BOX_TEXT)
        return box->content_width > 0 ? box->content_width : 0;
    if (style_contains_inline_size(box->style)) return 0;
    if (box->style) {
        const ns_css_value *wv = box->style->values[NS_CSS_WIDTH];
        if (wv && (wv->kind == NS_CSS_V_LENGTH || wv->kind == NS_CSS_V_CALC)) {
            double w = length_resolve(wv, 0, -1);
            if (w >= 0) {
                if (ns_css_keyword_is(box->style->values[NS_CSS_BOX_SIZING],
                                      "border-box")) {
                    ns_edges m = {0}, pd = {0}, bd = {0};
                    edges_from_style(box->style, 0, &m, &pd, &bd);
                    w -= pd.left + pd.right + bd.left + bd.right;
                    if (w < 0) w = 0;
                }
                const ns_css_value *mxw = box->style->values[NS_CSS_MAX_WIDTH];
                if (mxw && (mxw->kind == NS_CSS_V_LENGTH ||
                            mxw->kind == NS_CSS_V_CALC)) {
                    double m = length_resolve(mxw, 0, -1);
                    if (m >= 0 && w > m) w = m;
                }
                const ns_css_value *mnw = box->style->values[NS_CSS_MIN_WIDTH];
                if (mnw && (mnw->kind == NS_CSS_V_LENGTH ||
                            mnw->kind == NS_CSS_V_CALC)) {
                    double m = length_resolve(mnw, 0, -1);
                    if (m >= 0 && w < m) w = m;
                }
                return width_contribution_keyword_limits(box, w, parent_style,
                                                         FALSE);
            }
        }
        if (keyword_is(wv, "max-content"))
            return width_contribution_keyword_limits(
                box, measure_max_content_width(box, parent_style),
                parent_style, FALSE);
    }
    return width_contribution_keyword_limits(
        box, min_content_width_of(box, parent_style), parent_style,
        FALSE);
}

static double
flex_contribution_clamp_to_basis(ns_box *c, double contribution,
                                 double preferred)
{
    const ns_css_value *bv = c->style->values[NS_CSS_FLEX_BASIS];
    double base = definite_width_limit(c, bv);
    if (base < 0 && (!bv || keyword_is(bv, "auto"))) base = preferred;
    if (base < 0) return contribution;
    if (flex_grow_of(c) <= 0 && contribution > base) return base;
    if (flex_shrink_of(c) <= 0 && contribution < base) return base;
    return contribution;
}

static double
flex_row_item_min_contribution(ns_box *c, const ns_style *child_style)
{
    if (!c->style || c->kind == NS_BOX_INLINE || c->kind == NS_BOX_TEXT)
        return min_width_of(c, child_style);
    const ns_style *s = c->style;
    double min_content = min_content_width_of(c, child_style);
    double preferred = definite_width_limit(c, s->values[NS_CSS_WIDTH]);
    double contribution = flex_contribution_clamp_to_basis(
        c, MAX(min_content, preferred), preferred);
    double max_main = definite_width_limit(c, s->values[NS_CSS_MAX_WIDTH]);
    if (max_main >= 0 && contribution > max_main) contribution = max_main;
    double min_main = definite_width_limit(c, s->values[NS_CSS_MIN_WIDTH]);
    if (min_main < 0 && !overflow_establishes_bfc(s))
        min_main = preferred >= 0 ? MIN(min_content, preferred) : min_content;
    if (min_main >= 0 && contribution < min_main) contribution = min_main;
    return contribution;
}

static double measure_min_content_width(ns_box *box,
                                        const ns_style *parent_style);

static double
min_content_width_of(ns_box *box, const ns_style *parent_style)
{
    g_min_measure_depth++;
    double w = measure_min_content_width(box, parent_style);
    g_min_measure_depth--;
    return w;
}

static double
measure_min_content_width(ns_box *box, const ns_style *parent_style)
{
    if (box->kind == NS_BOX_INLINE || box->kind == NS_BOX_IMAGE ||
        box->kind == NS_BOX_VIDEO || box->kind == NS_BOX_SVG ||
        box->kind == NS_BOX_TEXT)
        return min_width_of(box, parent_style);
    if (style_contains_inline_size(box->style)) return 0;
    if (box->kind == NS_BOX_TABLE)
        return ns_layout_table_intrinsic_width(
            box, box->style ? box->style : parent_style, TRUE);
    if (box->style && style_is_grid_container(box->style) &&
        ns_layout_grid_flows_by_column(box->style)) {
        double gw = ns_layout_grid_column_flow_width(box, box->style, TRUE);
        if (gw >= 0) return gw;
    }
    if (box->style && style_is_grid_container(box->style)) {
        const ns_css_value *cv =
            box->style->values[NS_CSS_GRID_TEMPLATE_COLUMNS];
        if (cv && cv->kind == NS_CSS_V_TRACKS && cv->u.tracks.n > 0 &&
            !cv->u.tracks.subgrid) {
            const ns_css_tracks *tk = &cv->u.tracks;
            double sum = 0;
            gboolean all_definite = TRUE;
            for (int i = 0; i < tk->n; i++) {
                if (tk->tracks[i].kind == NS_CSS_TRACK_PX) {
                    sum += tk->tracks[i].v;
                } else if (tk->tracks[i].has_min &&
                           tk->tracks[i].min_kind == NS_CSS_TRACK_PX) {
                    sum += tk->tracks[i].min_v;
                } else {
                    all_definite = FALSE;
                    break;
                }
            }
            if (all_definite && sum > 0) {
                const ns_css_value *gv =
                    box->style->values[NS_CSS_COLUMN_GAP];
                if (!gv || !(gv->kind == NS_CSS_V_LENGTH ||
                             gv->kind == NS_CSS_V_CALC))
                    gv = box->style->values[NS_CSS_GAP];
                if (gv && (gv->kind == NS_CSS_V_LENGTH ||
                           gv->kind == NS_CSS_V_CALC) && tk->n > 1) {
                    double gap = length_resolve(gv, 0, 0);
                    if (gap > 0) sum += gap * (tk->n - 1);
                }
                return sum;
            }
        }
    }
    const ns_style *child_style = box->style ? box->style : parent_style;
    gboolean single_line_row = style_is_flex_container(box->style) &&
        strncmp(flex_direction_of(box->style), "row", 3) == 0 &&
        !flex_wraps(box->style);
    double max_child = 0;
    double row_sum = 0;
    int items = 0;
    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        double w = single_line_row
            ? flex_row_item_min_contribution(c, child_style)
            : min_width_of(c, child_style);
        double outer = w;
        if (c->style) {
            ns_edges m = {0}, pd = {0}, bd = {0};
            edges_from_style(c->style, 0, &m, &pd, &bd);
            outer += m.left + m.right + pd.left + pd.right + bd.left + bd.right;
        }
        if (outer > max_child) max_child = outer;
        row_sum += outer;
        items++;
    }
    if (!single_line_row)
        return max_child;
    if (items > 1)
        row_sum += flex_gap_of(box->style, 0) * (items - 1);
    return row_sum;
}

static __thread gboolean g_cq_seen_container;

static gboolean
box_is_query_container(const ns_box *b)
{
    if (!b || !b->style) return FALSE;
    const ns_css_value *ct = b->style->values[NS_CSS_CONTAINER_TYPE];
    return ct && ct->kind == NS_CSS_V_KEYWORD && ct->u.keyword &&
           g_ascii_strcasecmp(ct->u.keyword, "normal") != 0;
}

static gboolean
box_is_size_query_container(const ns_box *b)
{
    if (!b || !b->style) return FALSE;
    const ns_css_value *ct = b->style->values[NS_CSS_CONTAINER_TYPE];
    return ct && ct->kind == NS_CSS_V_KEYWORD && ct->u.keyword &&
           g_ascii_strcasecmp(ct->u.keyword, "size") == 0;
}

static void
cq_set_dims_from_ancestors(const ns_box *box)
{
    double inline_size = 0;
    double block_size = 0;
    for (const ns_box *a = box->parent; a; a = a->parent) {
        if (inline_size <= 0 && box_is_query_container(a))
            inline_size = a->content_width;
        if (block_size <= 0 && box_is_size_query_container(a))
            block_size = a->content_height;
        if (inline_size > 0 && block_size > 0) break;
    }
    ns_css_set_container_dims(inline_size, block_size);
}

static void
layout_box(ns_box *box, double parent_content_width, const ns_style *inherited_style)
{
    box->definite_height_read = FALSE;
    box->last_layout_width = parent_content_width;
    if (box_is_query_container(box)) g_cq_seen_container = TRUE;
    if (g_cq_seen_container) cq_set_dims_from_ancestors(box);
    if (box->kind == NS_BOX_BLOCK) {
        layout_block(box, parent_content_width, inherited_style);
    } else if (box->kind == NS_BOX_INLINE) {
        ns_layout_inline(box, parent_content_width, inherited_style);
    } else if (box->kind == NS_BOX_IMAGE) {
        layout_image(box, parent_content_width);
    } else if (box->kind == NS_BOX_VIDEO) {
        layout_image(box, parent_content_width);
    } else if (box->kind == NS_BOX_SVG) {
        layout_image(box, parent_content_width);
    } else if (box->kind == NS_BOX_TABLE) {
        ns_layout_table(box, parent_content_width, inherited_style);
    } else if (box->kind == NS_BOX_TABLE_CAPTION) {
        layout_block(box, parent_content_width, inherited_style);
    } else if (box->kind == NS_BOX_MATH) {
        double fpx = length_or(box->style
                               ? box->style->values[NS_CSS_FONT_SIZE] : NULL, 16);
        double w = 0, asc = 0, desc = 0;
        ns_math_measure(box->dom, fpx, &w, &asc, &desc);
        box->content_width = w;
        box->content_height = asc + desc;
    } else {
        box->content_width = parent_content_width;
        box->content_height = 0;
    }
}

static gboolean
flex_box_is_border_box(const ns_box *c)
{
    const ns_css_value *bsv = c && c->style
        ? c->style->values[NS_CSS_BOX_SIZING] : NULL;
    return bsv && bsv->kind == NS_CSS_V_KEYWORD && bsv->u.keyword &&
           strcmp(bsv->u.keyword, "border-box") == 0;
}

static double
estimate_natural_width(const ns_box *b, double cap)
{
    double font_size = 16;
    if (b->style && b->style->values[NS_CSS_FONT_SIZE]) {
        const ns_css_value *fs = b->style->values[NS_CSS_FONT_SIZE];
        if (fs->kind == NS_CSS_V_LENGTH && fs->u.length.unit == NS_CSS_UNIT_PX)
            font_size = fs->u.length.v;
    } else {
        for (const ns_box *p = b->parent; p; p = p->parent) {
            if (p->style && p->style->values[NS_CSS_FONT_SIZE]) {
                const ns_css_value *fs = p->style->values[NS_CSS_FONT_SIZE];
                if (fs->kind == NS_CSS_V_LENGTH &&
                    fs->u.length.unit == NS_CSS_UNIT_PX) {
                    font_size = fs->u.length.v;
                    break;
                }
            }
        }
    }
    const ns_css_value *swv = b->style ? b->style->values[NS_CSS_WIDTH] : NULL;
    if (swv && swv->kind == NS_CSS_V_LENGTH && swv->u.length.v > 0) {
        double sw = -1;
        switch (swv->u.length.unit) {
        case NS_CSS_UNIT_PX:
        case NS_CSS_UNIT_NUMBER:
            sw = swv->u.length.v;
            break;
        case NS_CSS_UNIT_EM:
            sw = swv->u.length.v * font_size;
            break;
        case NS_CSS_UNIT_REM:
            sw = swv->u.length.v * 16.0;
            break;
        case NS_CSS_UNIT_VW:
            sw = swv->u.length.v * ns_css_viewport_w() / 100.0;
            break;
        default:
            break;
        }
        if (sw > 0) {
            const ns_css_value *bsv = b->style->values[NS_CSS_BOX_SIZING];
            gboolean border_box = bsv && bsv->kind == NS_CSS_V_KEYWORD &&
                                  bsv->u.keyword &&
                                  strcmp(bsv->u.keyword, "border-box") == 0;
            if (!border_box)
                sw += b->padding.left + b->padding.right +
                      b->border.left  + b->border.right;
            return sw > cap ? cap : sw;
        }
    }
    if (style_contains_inline_size(b->style)) return 0;
    double w = 0;
    if (b->kind == NS_BOX_INLINE && b->text) {
        double chars = 0;
        for (const char *p = b->text; *p; p = g_utf8_next_char(p))
            if (g_utf8_get_char(p) != 0xFFFC) chars += 1;
        w = chars * font_size * 0.65 + font_size * 0.5;
        if (b->inline_atomics)
            for (guint i = 0; i < b->inline_atomics->len; i++) {
                const ns_box *ab =
                    g_array_index(b->inline_atomics, ns_inline_atomic, i).box;
                if (!ab) continue;
                double aw = ab->content_width > 0
                    ? ab->content_width +
                      ab->padding.left + ab->padding.right +
                      ab->border.left  + ab->border.right
                    : estimate_natural_width(ab, cap);
                w += aw + ab->margin.left + ab->margin.right;
            }
    } else if (b->kind == NS_BOX_IMAGE || b->kind == NS_BOX_VIDEO ||
               b->kind == NS_BOX_SVG) {
        w = b->content_width > 0 ? b->content_width : 0;
    } else {
        int flow_children = 0;
        gboolean column_flex = b->style &&
            style_is_flex_container(b->style) &&
            (strcmp(flex_direction_of(b->style),
                    "column") == 0 ||
             strcmp(flex_direction_of(b->style),
                    "column-reverse") == 0);
        for (const ns_box *c = b->first_child; c; c = c->next_sibling) {
            if (c->style && c->style != b->style &&
                style_is_absolute_or_fixed(c->style)) continue;
            double cw_child = estimate_natural_width(c, cap);
            if (c->style &&
                (c->style->values[NS_CSS_MARGIN_LEFT] ||
                 c->style->values[NS_CSS_MARGIN_RIGHT])) {
                ns_edges m = {0}, pd = {0}, bd = {0};
                edges_from_style(c->style, 0, &m, &pd, &bd);
                cw_child += m.left + m.right;
            }
            if (column_flex) {
                if (cw_child > w) w = cw_child;
            } else {
                w += cw_child;
            }
            flow_children++;
        }
        if (flow_children > 1 && style_is_flex_container(b->style) &&
            strcmp(flex_direction_of(b->style),
                   "column") != 0 &&
            strcmp(flex_direction_of(b->style),
                   "column-reverse") != 0) {
            const ns_css_value *cg = b->style->values[NS_CSS_COLUMN_GAP];
            const ns_css_value *gg = b->style->values[NS_CSS_GAP];
            const ns_css_value *gv =
                (cg && cg->kind == NS_CSS_V_LENGTH) ? cg :
                (gg && gg->kind == NS_CSS_V_LENGTH) ? gg : NULL;
            if (gv && gv->u.length.unit != NS_CSS_UNIT_PERCENT)
                w += (flow_children - 1) * gv->u.length.v;
        }
    }
    if (b->style) {
        if (b->style->values[NS_CSS_PADDING_LEFT] ||
            b->style->values[NS_CSS_PADDING_RIGHT] ||
            b->style->values[NS_CSS_BORDER_LEFT_WIDTH] ||
            b->style->values[NS_CSS_BORDER_RIGHT_WIDTH]) {
            ns_edges bm = {0}, bpd = {0}, bbd = {0};
            edges_from_style(b->style, cap, &bm, &bpd, &bbd);
            w += bpd.left + bpd.right + bbd.left + bbd.right;
        }
    } else {
        w += b->padding.left + b->padding.right +
             b->border.left  + b->border.right;
    }
    if (b->style) {
        const ns_css_value *bsv = b->style->values[NS_CSS_BOX_SIZING];
        gboolean border_box = bsv && bsv->kind == NS_CSS_V_KEYWORD &&
                              bsv->u.keyword &&
                              strcmp(bsv->u.keyword, "border-box") == 0;
        double box_extras = border_box ? 0 :
            b->padding.left + b->padding.right + b->border.left + b->border.right;
        const ns_css_value *mxw = b->style->values[NS_CSS_MAX_WIDTH];
        if (mxw && (mxw->kind == NS_CSS_V_LENGTH || mxw->kind == NS_CSS_V_CALC)) {
            double mx = length_resolve(mxw, cap, -1);
            if (mx >= 0 && w > mx + box_extras) w = mx + box_extras;
        }
        const ns_css_value *mnw = b->style->values[NS_CSS_MIN_WIDTH];
        if (mnw && (mnw->kind == NS_CSS_V_LENGTH || mnw->kind == NS_CSS_V_CALC)) {
            double mn = length_resolve(mnw, cap, -1);
            if (mn > 0 && w < mn + box_extras) w = mn + box_extras;
        }
    }
    if (w > cap) w = cap;
    return w;
}

static double
flex_grow_of(const ns_box *c)
{
    if (!c->style) return 0;
    return number_or(c->style->values[NS_CSS_FLEX_GROW], 0);
}

static double
flex_shrink_of(const ns_box *c)
{
    if (!c->style) return 1;
    double s = number_or(c->style->values[NS_CSS_FLEX_SHRINK], 1);
    return s < 0 ? 0 : s;
}

static void
shift_box_tree(ns_box *b, double dx, double dy)
{
    if (!b) return;
    b->x += dx;
    b->y += dy;
    if (b->grid_col_tracks)
        for (guint i = 0; i < b->grid_col_tracks->len; i++) {
            ns_grid_track_edges *e =
                &g_array_index(b->grid_col_tracks, ns_grid_track_edges, i);
            e->start += dx;
            e->end += dx;
        }
    if (b->grid_row_tracks)
        for (guint i = 0; i < b->grid_row_tracks->len; i++) {
            ns_grid_track_edges *e =
                &g_array_index(b->grid_row_tracks, ns_grid_track_edges, i);
            e->start += dy;
            e->end += dy;
        }
    for (ns_box *c = b->first_child; c; c = c->next_sibling)
        shift_box_tree(c, dx, dy);
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++) {
            ns_box *ab =
                g_array_index(b->inline_atomics, ns_inline_atomic, i).box;
            if (ab) shift_box_tree(ab, dx, dy);
        }
}

static double
gap_px(const ns_css_value *specific, const ns_css_value *shorthand, double basis)
{
    const ns_css_value *v = specific ? specific : shorthand;
    if (!v || (v->kind != NS_CSS_V_LENGTH && v->kind != NS_CSS_V_CALC))
        return 0;
    return length_resolve_nonnegative(v, basis, 0);
}

static double
flex_gap_of(const ns_style *s, double basis)
{
    if (!s) return 0;
    return gap_px(s->values[NS_CSS_COLUMN_GAP], s->values[NS_CSS_GAP], basis);
}

static gboolean
flex_wraps(const ns_style *s)
{
    if (!s) return FALSE;
    const ns_css_value *w = s->values[NS_CSS_FLEX_WRAP];
    if (!w || w->kind != NS_CSS_V_KEYWORD || !w->u.keyword) return FALSE;
    return strcmp(w->u.keyword, "wrap") == 0 ||
           strcmp(w->u.keyword, "wrap-reverse") == 0;
}

#define NS_GRID_NESTING_MAX 64

static int g_grid_nesting;

static gboolean
box_is_block_level_replaced(const ns_box *c)
{
    if (!c || (c->kind != NS_BOX_IMAGE && c->kind != NS_BOX_VIDEO &&
               c->kind != NS_BOX_SVG)) return FALSE;
    return ns_display_is_block_level(ns_css_display_of(c->style));
}

static double
collapsed_margin(double a, double b)
{
    double positive = MAX(a > 0 ? a : 0, b > 0 ? b : 0);
    double negative = MIN(a < 0 ? a : 0, b < 0 ? b : 0);
    return positive + negative;
}

static gboolean
empty_block_collapses_through(const ns_box *box, int clear)
{
    return box && box->kind == NS_BOX_BLOCK && clear == 0 &&
           box->content_height <= 0 &&
           box->padding.top <= 0 && box->padding.bottom <= 0 &&
           box->border.top <= 0 && box->border.bottom <= 0 &&
           !box_establishes_bfc(box);
}

static double
text_input_intrinsic_width(const ns_box *box)
{
    if (!box || !box->dom || !box->dom->name ||
        strcmp(box->dom->name, "input") != 0)
        return 0;
    const char *type = ns_element_get_attr(box->dom, "type");
    if (type && *type &&
        g_ascii_strcasecmp(type, "text") != 0 &&
        g_ascii_strcasecmp(type, "password") != 0 &&
        g_ascii_strcasecmp(type, "search") != 0 &&
        g_ascii_strcasecmp(type, "email") != 0 &&
        g_ascii_strcasecmp(type, "url") != 0 &&
        g_ascii_strcasecmp(type, "tel") != 0 &&
        g_ascii_strcasecmp(type, "number") != 0)
        return 0;
    const char *size_attr = ns_element_get_attr(box->dom, "size");
    int size = size_attr ? ns_parse_int(size_attr, 20, 1, 80) : 20;
    double font_size = length_or(box->style
        ? box->style->values[NS_CSS_FONT_SIZE] : NULL, 16);
    return size * font_size * 0.75;
}

static double
intrinsic_keyword_width(ns_box *box, const char *kw, const ns_style *mi,
                        double avail)
{
    if (!kw) return -1;
    if (avail < 0) avail = 0;
    if (strcmp(kw, "min-content") == 0)
        return min_content_width_of(box, mi);
    if (strcmp(kw, "max-content") == 0)
        return measure_max_content_width(box, mi);
    if (strcmp(kw, "fit-content") == 0) {
        double mn = min_content_width_of(box, mi);
        double mx = measure_max_content_width(box, mi);
        double w = mx < avail ? mx : avail;
        return w < mn ? mn : w;
    }
    if (strcmp(kw, "stretch") == 0 ||
        strcmp(kw, "-webkit-fill-available") == 0 ||
        strcmp(kw, "-moz-available") == 0)
        return avail;
    return -1;
}

static gboolean
size_keyword_is_intrinsic(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           (strcmp(v->u.keyword, "min-content") == 0 ||
            strcmp(v->u.keyword, "max-content") == 0 ||
            strcmp(v->u.keyword, "fit-content") == 0);
}

static gboolean
height_keyword_stretches(const ns_css_value *v)
{
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           (strcmp(v->u.keyword, "stretch") == 0 ||
            strcmp(v->u.keyword, "-webkit-fill-available") == 0 ||
            strcmp(v->u.keyword, "-moz-available") == 0);
}

static const char *
legacy_block_align(const ns_box *c, const ns_style *inherited)
{
    const char *ta = inherited ? ns_style_keyword(inherited, NS_CSS_TEXT_ALIGN) : NULL;
    if (!ta) return NULL;
    if (strcmp(ta, "-webkit-center") == 0 || strcmp(ta, "-moz-center") == 0)
        return "center";
    if (strcmp(ta, "-webkit-right") == 0 || strcmp(ta, "-moz-right") == 0)
        return "right";
    if (strcmp(ta, "center") != 0 && strcmp(ta, "right") != 0) return NULL;
    for (const ns_box *p = c->parent; p; p = p->parent) {
        if (p->style) {
            const char *pt = ns_style_keyword(p->style, NS_CSS_TEXT_ALIGN);
            if (!pt || strcmp(pt, ta) != 0) return NULL;
        }
        if (!p->dom || p->dom->kind != NS_NODE_ELEMENT) continue;
        if (ns_node_is_element_named(p->dom, "center")) return ta;
        const char *al = ns_element_get_attr(p->dom, "align");
        if (al && g_ascii_strcasecmp(al, ta) == 0) return ta;
    }
    return NULL;
}

static void
legacy_align_block_child(ns_box *c, double avail_x, double avail_w,
                         const ns_style *inherited)
{
    if (c->kind != NS_BOX_BLOCK && c->kind != NS_BOX_TABLE) return;
    if (style_is_absolute_or_fixed(c->style) || float_side_of(c->style) >= 0)
        return;
    const ns_css_value *ml = c->style ? c->style->values[NS_CSS_MARGIN_LEFT] : NULL;
    const ns_css_value *mr = c->style ? c->style->values[NS_CSS_MARGIN_RIGHT] : NULL;
    if (length_is_auto(ml) || length_is_auto(mr) ||
        c->margin.left != 0 || c->margin.right != 0)
        return;
    double outer = c->content_width + c->padding.left + c->padding.right +
                   c->border.left + c->border.right;
    if (outer >= avail_w - 0.5) return;
    const char *legacy = legacy_block_align(c, inherited);
    if (!legacy) return;
    double target_x;
    if (strcmp(legacy, "center") == 0)
        target_x = avail_x + (avail_w - outer) / 2.0;
    else if (strcmp(legacy, "right") == 0)
        target_x = avail_x + avail_w - outer;
    else
        return;
    shift_box_tree(c, target_x - c->x, 0);
}

static gboolean
block_height_is_auto(const ns_box *box, double width_basis)
{
    if (!box->style) return TRUE;
    if (resolve_used_height(box, box->style->values[NS_CSS_MIN_HEIGHT],
                            width_basis, -1) > 0)
        return FALSE;
    const ns_css_value *hv = box->style->values[NS_CSS_HEIGHT];
    if (height_keyword_stretches(hv))
        return containing_block_definite_height(box) < 0;
    if (hv && (hv->kind == NS_CSS_V_LENGTH || hv->kind == NS_CSS_V_CALC))
        return resolve_used_height(box, hv, width_basis, -1) < 0;
    return TRUE;
}

static ns_box *
fieldset_rendered_legend(const ns_box *box)
{
    for (ns_box *c = box->first_child; c; c = c->next_sibling)
        if (c->is_rendered_legend) return c;
    return NULL;
}

static void
box_detach_child(ns_box *parent, ns_box *child)
{
    ns_box *prev = NULL;
    for (ns_box *c = parent->first_child; c && c != child; c = c->next_sibling)
        prev = c;
    if (prev) prev->next_sibling = child->next_sibling;
    else parent->first_child = child->next_sibling;
    if (parent->last_child == child) parent->last_child = prev;
    child->next_sibling = NULL;
}

static void
box_prepend_child(ns_box *parent, ns_box *child)
{
    child->next_sibling = parent->first_child;
    parent->first_child = child;
    if (!parent->last_child) parent->last_child = child;
}

static double
legend_inline_offset(const ns_box *fieldset, const ns_box *legend,
                     double free_w)
{
    const ns_style *ls = legend->style;
    gboolean ml_auto = ls && length_is_auto(ls->values[NS_CSS_MARGIN_LEFT]);
    gboolean mr_auto = ls && length_is_auto(ls->values[NS_CSS_MARGIN_RIGHT]);
    if (ml_auto && mr_auto) return free_w / 2.0;
    if (ml_auto) return free_w;
    if (mr_auto) return 0;
    const char *js = ls ? ns_style_keyword(ls, NS_CSS_JUSTIFY_SELF) : NULL;
    if (js && g_str_has_prefix(js, "safe ")) js += 5;
    else if (js && g_str_has_prefix(js, "unsafe ")) js += 7;
    gboolean fieldset_rtl = fieldset->style &&
        keyword_is(fieldset->style->values[NS_CSS_DIRECTION], "rtl");
    gboolean legend_rtl = ls && keyword_is(ls->values[NS_CSS_DIRECTION], "rtl");
    gboolean at_end = fieldset_rtl;
    if (!js) return at_end ? free_w : 0;
    if (strcmp(js, "center") == 0) return free_w / 2.0;
    if (strcmp(js, "left") == 0) at_end = FALSE;
    else if (strcmp(js, "right") == 0) at_end = TRUE;
    else if (strcmp(js, "end") == 0 || strcmp(js, "flex-end") == 0)
        at_end = !fieldset_rtl;
    else if (strcmp(js, "self-start") == 0) at_end = legend_rtl;
    else if (strcmp(js, "self-end") == 0) at_end = !legend_rtl;
    return at_end ? free_w : 0;
}

static double
layout_rendered_legend(ns_box *fieldset, ns_box *legend, double cw,
                       double inner_x, const ns_style *inherited)
{
    edges_from_style(legend->style, cw,
                     &legend->margin, &legend->padding, &legend->border);
    double outer_extras = legend->padding.left + legend->padding.right +
                          legend->border.left + legend->border.right +
                          legend->margin.left + legend->margin.right;
    const ns_css_value *wv = legend->style
        ? legend->style->values[NS_CSS_WIDTH] : NULL;
    double layout_w = cw;
    if (!wv || length_is_auto(wv)) {
        double avail = MAX(cw - outer_extras, 0);
        double fit = measure_natural_width(legend, inherited);
        if (fit > avail) fit = avail;
        double floor_w = min_width_of(legend, inherited);
        if (fit < floor_w) fit = floor_w;
        layout_w = fit + outer_extras;
    }
    double top_edge = fieldset->y + fieldset->margin.top;
    legend->x = inner_x;
    legend->y = top_edge;
    layout_box(legend, layout_w, inherited);
    double outer_w = legend->content_width +
                     legend->padding.left + legend->padding.right +
                     legend->border.left + legend->border.right +
                     legend->margin.left + legend->margin.right;
    double border_box_h = legend->content_height +
                          legend->padding.top + legend->padding.bottom +
                          legend->border.top + legend->border.bottom;
    double border_top = fieldset->border.top;
    double legend_top = legend->margin.top +
                        MAX((border_top - border_box_h) / 2.0, 0);
    double target_x = inner_x + legend_inline_offset(fieldset, legend,
                                                     cw - outer_w);
    double target_y = top_edge + legend_top - legend->margin.top;
    shift_box_tree(legend, target_x - legend->x, target_y - legend->y);
    double painted_border_top =
        MAX(legend_top + border_box_h / 2.0 - border_top / 2.0, 0);
    double legend_bottom = legend_top + border_box_h + legend->margin.bottom;
    double extra = MAX(painted_border_top, legend_bottom - border_top);
    return MAX(extra, 0);
}

static void
fieldset_set_content_block_size(ns_box *box, double width_basis,
                                double sizing_extras, double legend_extra)
{
    const ns_css_value *hv = box->style ? box->style->values[NS_CSS_HEIGHT] : NULL;
    if (!hv || (hv->kind != NS_CSS_V_LENGTH && hv->kind != NS_CSS_V_CALC))
        return;
    double h = resolve_used_height(box, hv, width_basis, -1);
    if (h < 0) return;
    h = MAX(h - sizing_extras - legend_extra, 0);
    for (ns_box *c = box->first_child; c; c = c->next_sibling)
        c->cb_height_override = h;
}

gboolean
ns_box_fieldset_legend_gap(const ns_box *fieldset, double *border_inset,
                           double *gap_x0, double *gap_x1,
                           double *gap_y0, double *gap_y1)
{
    const ns_box *legend = fieldset ? fieldset_rendered_legend(fieldset) : NULL;
    if (!legend) return FALSE;
    double top_edge = fieldset->y + fieldset->margin.top;
    double legend_border_top = legend->y + legend->margin.top;
    double border_box_w = legend->content_width +
                          legend->padding.left + legend->padding.right +
                          legend->border.left + legend->border.right;
    double border_box_h = legend->content_height +
                          legend->padding.top + legend->padding.bottom +
                          legend->border.top + legend->border.bottom;
    double inset = MAX(legend_border_top - top_edge + border_box_h / 2.0 -
                       fieldset->border.top / 2.0, 0);
    *border_inset = inset;
    *gap_x0 = legend->x + legend->margin.left;
    *gap_x1 = *gap_x0 + border_box_w;
    *gap_y0 = MIN(legend->y, top_edge + inset);
    *gap_y1 = MAX(legend_border_top + border_box_h + legend->margin.bottom,
                  top_edge + inset + fieldset->border.top);
    return TRUE;
}

static double
block_align_content_shift(const ns_box *box, double free_space)
{
    if (!box->style || free_space == 0) return 0;
    if (style_is_flex_container(box->style) ||
        style_is_grid_container(box->style))
        return 0;
    const char *acont = keyword_or(box->style, NS_CSS_ALIGN_CONTENT, "normal");
    gboolean unsafe = g_str_has_prefix(acont, "unsafe ");
    if (unsafe) acont += strlen("unsafe ");
    else if (g_str_has_prefix(acont, "safe ")) acont += strlen("safe ");
    if (free_space < 0 &&
        (!unsafe ||
         overflow_kw_scrolls(overflow_axis_keyword(box->style, NS_CSS_OVERFLOW_Y))))
        return 0;
    if (strcmp(acont, "center") == 0 || strcmp(acont, "space-around") == 0 ||
        strcmp(acont, "space-evenly") == 0)
        return free_space / 2.0;
    if (strcmp(acont, "end") == 0 || strcmp(acont, "flex-end") == 0)
        return free_space;
    return 0;
}

static void
layout_block(ns_box *box, double parent_content_width, const ns_style *inherited_style)
{
    inline_runs_join_splits(box);
    edges_from_style(box->style, parent_content_width,
                     &box->margin, &box->padding, &box->border);
    box->margin_top_through = 0;

    const ns_css_value *wv  = box->style ? box->style->values[NS_CSS_WIDTH]     : NULL;
    const ns_css_value *mxw = box->style ? box->style->values[NS_CSS_MAX_WIDTH] : NULL;
    const ns_css_value *mnw = box->style ? box->style->values[NS_CSS_MIN_WIDTH] : NULL;
    double horiz_extras = box->padding.left + box->padding.right +
                          box->border.left + box->border.right;
    double horiz_total  = horiz_extras + box->margin.left + box->margin.right;
    double vert_extras = box->padding.top + box->padding.bottom +
                         box->border.top + box->border.bottom;
    double cw;
    gboolean explicit_width = FALSE;
    gboolean intrinsic_width = FALSE;
    gboolean flex_grow_filled = FALSE;
    const char *parent_flex_dir = box->parent
        ? flex_direction_of(box->parent->style) : "row";
    gboolean flex_row_item = box->parent &&
        style_is_flex_container(box->parent->style) &&
        (strcmp(parent_flex_dir, "row") == 0 ||
         strcmp(parent_flex_dir, "row-reverse") == 0);
    gboolean flex_col_item_stretch = box->parent &&
        style_is_flex_container(box->parent->style) &&
        (strcmp(parent_flex_dir, "column") == 0 ||
         strcmp(parent_flex_dir, "column-reverse") == 0);
    if (flex_col_item_stretch) {
        const char *eff = box->style
            ? ns_style_keyword(box->style, NS_CSS_ALIGN_SELF) : NULL;
        if (!eff || strcmp(eff, "auto") == 0)
            eff = keyword_or(box->parent->style, NS_CSS_ALIGN_ITEMS, "stretch");
        if (strcmp(eff, "stretch") != 0 && strcmp(eff, "normal") != 0)
            flex_col_item_stretch = FALSE;
        if (length_is_auto(box->style ? box->style->values[NS_CSS_MARGIN_LEFT] : NULL) ||
            length_is_auto(box->style ? box->style->values[NS_CSS_MARGIN_RIGHT] : NULL))
            flex_col_item_stretch = FALSE;
    }
    double pct_width_base = parent_content_width;
    if (flex_row_item && box->parent->content_width > 0)
        pct_width_base = box->parent->content_width;
    if (box->has_flex_main) {
        box->has_flex_main = FALSE;
        cw = box->flex_main_size;
        if (cw < 0) cw = 0;
        explicit_width = TRUE;
        flex_grow_filled = TRUE;
    } else if (flex_row_item && flex_grow_of(box) > 0) {
        cw = parent_content_width - horiz_total;
        if (cw < 0) cw = 0;
        explicit_width = TRUE;
        flex_grow_filled = TRUE;
    } else if (wv && wv->kind == NS_CSS_V_LENGTH) {
        cw = length_resolve(wv, pct_width_base, 0);
        explicit_width = TRUE;
    } else if (wv && wv->kind == NS_CSS_V_CALC) {
        cw = length_resolve(wv, pct_width_base, 0);
        explicit_width = TRUE;
    } else if (wv && wv->kind == NS_CSS_V_KEYWORD && wv->u.keyword &&
               (strcmp(wv->u.keyword, "stretch") == 0 ||
                strcmp(wv->u.keyword, "-webkit-fill-available") == 0 ||
                strcmp(wv->u.keyword, "-moz-available") == 0)) {
        cw = parent_content_width - horiz_total;
        if (cw < 0) cw = 0;
        explicit_width = TRUE;
        intrinsic_width = TRUE;
    } else if (wv && wv->kind == NS_CSS_V_KEYWORD && wv->u.keyword &&
               (strcmp(wv->u.keyword, "max-content") == 0 ||
                strcmp(wv->u.keyword, "min-content") == 0 ||
                strcmp(wv->u.keyword, "fit-content") == 0)) {
        const ns_style *mi = inherited_style ? inherited_style : box->style;
        double avail = parent_content_width - horiz_total;
        if (avail < 0) avail = 0;
        if (strcmp(wv->u.keyword, "min-content") == 0) {
            cw = min_width_of(box, mi);
        } else if (strcmp(wv->u.keyword, "max-content") == 0) {
            cw = measure_natural_width(box, mi);
        } else {
            double mn = min_width_of(box, mi);
            double mx = measure_natural_width(box, mi);
            cw = mx < avail ? mx : avail;
            if (cw < mn) cw = mn;
        }
        if (cw < 0) cw = 0;
        explicit_width = TRUE;
        intrinsic_width = TRUE;
    } else if (!flex_col_item_stretch &&
               display_is_atomic_inline_container(
                   ns_css_display_of(box->style))) {
        double avail = parent_content_width - horiz_total;
        if (avail < 0) avail = 0;
        if (height_keyword_stretches(wv)) {
            cw = avail;
            explicit_width = TRUE;
            intrinsic_width = TRUE;
        } else {
            double natural = measure_natural_width(box,
                                                   inherited_style ? inherited_style : box->style);
            double input_width = text_input_intrinsic_width(box);
            if (natural < input_width) natural = input_width;
            cw = natural < avail ? natural : avail;
        }
        if (cw < 0) cw = 0;
    } else {
        cw = parent_content_width - horiz_total;
        if (cw < 0) cw = 0;
    }
    gboolean border_box = FALSE;
    if (box->style && box->style->values[NS_CSS_BOX_SIZING] &&
        box->style->values[NS_CSS_BOX_SIZING]->kind == NS_CSS_V_KEYWORD &&
        strcmp(box->style->values[NS_CSS_BOX_SIZING]->u.keyword, "border-box") == 0)
        border_box = TRUE;
    if (border_box && explicit_width && !intrinsic_width && !flex_grow_filled) {
        cw -= horiz_extras;
        if (cw < 0) cw = 0;
    }
    double max_cw = length_resolve(mxw, pct_width_base, -1);
    if (max_cw < 0 && mxw && mxw->kind == NS_CSS_V_KEYWORD) {
        max_cw = intrinsic_keyword_width(box, mxw->u.keyword,
                                         inherited_style ? inherited_style : box->style,
                                         parent_content_width - horiz_total);
        if (max_cw >= 0 && border_box) max_cw += horiz_extras;
    }
    if (max_cw >= 0) {
        if (border_box) max_cw -= horiz_extras;
        if (max_cw >= 0 && cw > max_cw) { cw = max_cw; explicit_width = TRUE; }
    }
    double min_cw = length_resolve(mnw, pct_width_base, -1);
    if (min_cw < 0 && mnw && mnw->kind == NS_CSS_V_KEYWORD) {
        min_cw = intrinsic_keyword_width(box, mnw->u.keyword,
                                         inherited_style ? inherited_style : box->style,
                                         parent_content_width - horiz_total);
        if (min_cw >= 0 && border_box) min_cw += horiz_extras;
    }
    if (min_cw >= 0) {
        if (border_box) min_cw -= horiz_extras;
        if (min_cw >= 0 && cw < min_cw) { cw = min_cw; explicit_width = TRUE; }
    }
    box->content_width = cw;

    if (explicit_width && !style_is_absolute_or_fixed(box->style)) {
        gboolean ml_auto = length_is_auto(box->style ? box->style->values[NS_CSS_MARGIN_LEFT]  : NULL);
        gboolean mr_auto = length_is_auto(box->style ? box->style->values[NS_CSS_MARGIN_RIGHT] : NULL);
        double available = parent_content_width - cw - horiz_extras;
        if (available < 0) available = 0;
        if (ml_auto && mr_auto) {
            box->margin.left  = available / 2.0;
            box->margin.right = available / 2.0;
        } else if (ml_auto) {
            box->margin.left  = available - box->margin.right;
            if (box->margin.left < 0) box->margin.left = 0;
        } else if (mr_auto) {
            box->margin.right = available - box->margin.left;
            if (box->margin.right < 0) box->margin.right = 0;
        }
    }

    double inner_x = box->x + box->margin.left + box->border.left + box->padding.left;
    double inner_y = box->y + box->margin.top  + box->border.top  + box->padding.top;
    const ns_style *child_inherited = box->style ? box->style : inherited_style;
    ns_box *legend = fieldset_rendered_legend(box);
    double legend_extra = 0;
    if (legend) {
        legend_extra = layout_rendered_legend(box, legend, cw, inner_x,
                                              child_inherited);
        inner_y += legend_extra;
        box_detach_child(box, legend);
        fieldset_set_content_block_size(box, parent_content_width,
                                        border_box ? vert_extras : 0,
                                        legend_extra);
    }
    double cursor_y = inner_y;
    double prev_margin_bottom = 0;
    double top_through = 0;
    double float_shift = 0;
    double outer_margin = g_margin_above.box == box ? g_margin_above.collapsed
                                                    : box->margin.top;
    gboolean collapse_top_with_parent =
        box->padding.top == 0 && box->border.top == 0 &&
        !box_establishes_bfc(box);
    gboolean collapse_bottom_with_parent =
        box->padding.bottom == 0 && box->border.bottom == 0 &&
        box->parent && !box_establishes_bfc(box) &&
        block_height_is_auto(box, parent_content_width);

    if (style_is_flex_container(box->style) || style_is_grid_container(box->style))
        reorder_children_by_order(box);

    if (style_is_flex_container(box->style)) {
        const char *dir = flex_direction_of(box->style);
        gboolean is_row = strcmp(dir, "row") == 0 || strcmp(dir, "row-reverse") == 0;
        gboolean is_col = strcmp(dir, "column") == 0 || strcmp(dir, "column-reverse") == 0;
        if (is_row) {
            if (flex_wraps(box->style))
                ns_layout_flex_row_wrap(box, cw, inner_x, inner_y, child_inherited,
                                     strcmp(dir, "row-reverse") == 0, &cursor_y);
            else
                ns_layout_flex_row(box, cw, inner_x, inner_y, child_inherited,
                                strcmp(dir, "row-reverse") == 0,
                                parent_content_width, &cursor_y);
            goto flex_done;
        }
        if (is_col) {
            ns_layout_flex_column(box, cw, inner_x, inner_y, child_inherited,
                               strcmp(dir, "column-reverse") == 0,
                               parent_content_width, &cursor_y);
            goto flex_done;
        }
    }

    if (style_is_grid_container(box->style) &&
        g_grid_nesting < NS_GRID_NESTING_MAX) {
        g_grid_nesting++;
        ns_layout_grid(box, cw, inner_x, inner_y, child_inherited, &cursor_y);
        g_grid_nesting--;
        goto flex_done;
    }

    double col_gap = 16;
    int n_cols = box->style ? ns_css_used_column_count(box->style, cw, &col_gap)
                            : 1;
    ns_box *column_host = NULL;
    if (n_cols > 1) {
        int distributable = multicol_distributable_children(box);
        gboolean single_fragmentable_inline =
            box->first_child && !box->first_child->next_sibling &&
            inline_box_can_fragment_multicol(box->first_child,
                                             child_inherited, cw);
        if (distributable < 2 && !single_fragmentable_inline) {
            column_host = multicol_column_host(box);
            if (!column_host) n_cols = 1;
        }
    }
    if (n_cols > 1) {
        double col_w = (cw - col_gap * (n_cols - 1)) / n_cols;
        if (col_w > 1) cw = col_w;
        else n_cols = 1;
    }
    box->columns = n_cols;

    GArray *floats = g_array_new(FALSE, FALSE, sizeof(float_ref));
    if (g_inherited_floats.box == box)
        floats_inherit(floats, &g_inherited_floats, inner_x, cw, inner_y);
    double inline_line_top = -1;

    for (ns_box *c = box->first_child; c; c = c->next_sibling) {
        c->x = inner_x;
        int fside = float_side_of(c->style);
        int clr = clear_kind_of(c->style);
        if (fside >= 0 && (c->kind == NS_BOX_BLOCK || c->kind == NS_BOX_TABLE ||
                           c->kind == NS_BOX_IMAGE || c->kind == NS_BOX_VIDEO ||
                           c->kind == NS_BOX_SVG)) {
            edges_from_style(c->style, cw,
                             &c->margin, &c->padding, &c->border);
            double float_max_w = cw;
            double float_floor_w = 0;
            double cw_for_float;
            const ns_css_value *wv2 = c->style ? c->style->values[NS_CSS_WIDTH] : NULL;
            const ns_css_value *mxw2 = c->style ? c->style->values[NS_CSS_MAX_WIDTH] : NULL;
            const ns_css_value *mnw2 = c->style ? c->style->values[NS_CSS_MIN_WIDTH] : NULL;
            double float_sizing_extras = flex_box_is_border_box(c)
                ? c->padding.left + c->padding.right
                  + c->border.left + c->border.right
                : 0;
            if (wv2 && (wv2->kind == NS_CSS_V_LENGTH || wv2->kind == NS_CSS_V_CALC)) {
                cw_for_float = length_resolve(wv2, cw, 0) - float_sizing_extras;
                if (cw_for_float < 0) cw_for_float = 0;
            } else {
                double cap = float_max_w
                    - c->padding.left - c->padding.right
                    - c->border.left - c->border.right
                    - c->margin.left - c->margin.right;
                if (cap < 0) cap = 0;
                cw_for_float = height_keyword_stretches(wv2)
                    ? cap : measure_natural_width(c, child_inherited);
                if (cw_for_float > cap) cw_for_float = cap;
                if (!height_keyword_stretches(wv2)) {
                    float_floor_w = min_width_of(c, child_inherited);
                    if (cw_for_float < float_floor_w) cw_for_float = float_floor_w;
                }
                if (cw_for_float < 0) cw_for_float = 0;
            }
            if (mxw2 && (mxw2->kind == NS_CSS_V_LENGTH || mxw2->kind == NS_CSS_V_CALC)) {
                double mx = length_resolve(mxw2, cw, 0);
                if (mx > 0 && cw_for_float > mx - float_sizing_extras)
                    cw_for_float = MAX(mx - float_sizing_extras, 0);
            }
            if (mnw2 && (mnw2->kind == NS_CSS_V_LENGTH || mnw2->kind == NS_CSS_V_CALC)) {
                double mn = length_resolve(mnw2, cw, 0);
                if (mn > 0 && cw_for_float < mn - float_sizing_extras)
                    cw_for_float = mn - float_sizing_extras;
            }
            double avail = cw_for_float
                + c->padding.left + c->padding.right
                + c->border.left + c->border.right
                + c->margin.left + c->margin.right;
            double float_y = cursor_y;
            if (!clr && inline_line_top >= 0 && inline_line_top < cursor_y)
                float_y = inline_line_top;
            if (clr) {
                double y_after_clear = floats_clear_y(floats, float_y, clr);
                if (y_after_clear > float_y) float_y = y_after_clear;
            }
            double left_off = 0, right_off = 0;
            floats_offsets_at(floats, float_y, &left_off, &right_off);
            while ((avail > cw - left_off - right_off) && floats->len > 0) {
                double next_y = float_y;
                gboolean advanced = FALSE;
                for (guint i = 0; i < floats->len; i++) {
                    const float_ref *f = &g_array_index(floats, float_ref, i);
                    if (f->bottom > float_y &&
                        (!advanced || f->bottom < next_y)) {
                        next_y = f->bottom;
                        advanced = TRUE;
                    }
                }
                if (!advanced) break;
                float_y = next_y;
                floats_offsets_at(floats, float_y, &left_off, &right_off);
            }
            double cw_capped = cw - left_off - right_off
                - c->margin.left - c->margin.right
                - c->padding.left - c->padding.right
                - c->border.left - c->border.right;
            if (cw_for_float > cw_capped && cw_capped > 0)
                cw_for_float = MAX(cw_capped, float_floor_w);
            double tentative_outer = cw_for_float
                + c->padding.left + c->padding.right
                + c->border.left + c->border.right
                + c->margin.left + c->margin.right;
            if (fside == 0)
                c->x = inner_x + left_off;
            else
                c->x = inner_x + cw - right_off - tentative_outer;
            c->y = float_y;
            double saved_cw = c->content_width;
            c->content_width = cw_for_float;
            gboolean explicit_float_w = wv2 &&
                (wv2->kind == NS_CSS_V_LENGTH || wv2->kind == NS_CSS_V_CALC);
            layout_box(c, explicit_float_w
                       ? cw
                       : cw_for_float
                         + c->padding.left + c->padding.right
                         + c->border.left + c->border.right
                         + c->margin.left + c->margin.right,
                       child_inherited);
            (void)saved_cw;
            double actual_outer = c->content_width
                + c->padding.left + c->padding.right
                + c->border.left + c->border.right
                + c->margin.left + c->margin.right;
            if (fside == 1) {
                double aligned_x = inner_x + cw - right_off - actual_outer;
                if (aligned_x != c->x) shift_box_tree(c, aligned_x - c->x, 0);
            }
            float_ref fr = {
                .box = c, .side = fside,
                .top = c->y,
                .bottom = c->y + c->margin.top + c->content_height
                    + c->padding.top + c->padding.bottom
                    + c->border.top + c->border.bottom
                    + c->margin.bottom,
                .outer_w = actual_outer,
            };
            g_array_append_val(floats, fr);
            continue;
        }
        if (c->kind == NS_BOX_BLOCK || c->kind == NS_BOX_TABLE ||
            box_is_block_level_replaced(c)) {
            edges_from_style(c->style, cw,
                             &c->margin, &c->padding, &c->border);
            double mt = c->margin.top;
            double gap = collapsed_margin(mt, prev_margin_bottom);
            double c_margin_above = gap;
            gboolean collapses_through_top =
                collapse_top_with_parent && cursor_y == inner_y;
            if (collapses_through_top) {
                c_margin_above = collapsed_margin(outer_margin, gap);
                gap = c_margin_above - outer_margin;
                floats_shift_placed(floats, gap - float_shift);
                float_shift = gap;
                top_through = gap;
            }
            cursor_y += gap;
            if (clr) {
                double y_after_clear = floats_clear_y(floats, cursor_y, clr);
                if (y_after_clear > cursor_y) cursor_y = y_after_clear;
            }
            double left_off = 0, right_off = 0;
            gboolean lines_wrap_floats = c->kind == NS_BOX_BLOCK &&
                                         floats_max_bottom(floats) > cursor_y &&
                                         !box_establishes_bfc(c);
            if (!lines_wrap_floats) {
                floats_offsets_at(floats, cursor_y, &left_off, &right_off);
                floats_advance_to_readable_width(floats, cw, &cursor_y,
                                                 &left_off, &right_off);
            }
            double cw_avail = cw - left_off - right_off;
            if (cw_avail < 0) cw_avail = 0;
            c->x = inner_x + left_off;
            c->y = cursor_y - mt;
            inherited_floats outer_floats = g_inherited_floats;
            margin_above outer_above = g_margin_above;
            if (lines_wrap_floats)
                g_inherited_floats = (inherited_floats){ c, floats, inner_x, cw };
            g_margin_above = (margin_above){ c, c_margin_above };
            layout_box(c, cw_avail, child_inherited);
            g_inherited_floats = outer_floats;
            g_margin_above = outer_above;
            if (box_establishes_bfc(c) && cw_avail > 0) {
                double span_l = 0, span_r = 0;
                double c_h = c->content_height +
                             c->padding.top + c->padding.bottom +
                             c->border.top + c->border.bottom;
                floats_offsets_over(floats, cursor_y, cursor_y + c_h,
                                    &span_l, &span_r);
                if (span_l > left_off || span_r > right_off) {
                    double retry = cw - MAX(span_l, left_off)
                                      - MAX(span_r, right_off);
                    if (retry > 0 && retry < cw_avail) {
                        left_off = MAX(span_l, left_off);
                        right_off = MAX(span_r, right_off);
                        cw_avail = retry;
                        c->x = inner_x + left_off;
                        c->y = cursor_y - mt;
                        layout_box(c, cw_avail, child_inherited);
                    }
                }
            }
            if (keyword_is(box->style ? box->style->values[NS_CSS_DIRECTION]
                                      : NULL, "rtl") &&
                !style_is_absolute_or_fixed(c->style)) {
                double outer = c->margin.left + c->margin.right +
                               c->content_width + c->padding.left +
                               c->padding.right + c->border.left +
                               c->border.right;
                if (fabs(outer - cw_avail) > 0.01)
                    shift_box_tree(c, cw_avail - outer, 0);
            }
            if (c->kind == NS_BOX_BLOCK)
                legacy_align_block_child(c, inner_x + left_off, cw_avail,
                                         child_inherited);
            collect_escaping_floats(c, floats, 0);
            if (empty_block_collapses_through(c, clr)) {
                cursor_y -= gap;
                if (collapses_through_top) top_through = 0;
                prev_margin_bottom = collapsed_margin(
                    collapsed_margin(prev_margin_bottom, mt),
                    c->margin.bottom);
            } else {
                collapse_top_with_parent = FALSE;
                cursor_y += c->margin_top_through;
                if (collapses_through_top) top_through += c->margin_top_through;
                cursor_y += c->content_height +
                            c->padding.top + c->padding.bottom +
                            c->border.top + c->border.bottom;
                prev_margin_bottom = c->margin.bottom;
            }
            inline_line_top = -1;
        } else {
            cursor_y += prev_margin_bottom;
            prev_margin_bottom = 0;
            double left_off = 0, right_off = 0;
            floats_offsets_at(floats, cursor_y, &left_off, &right_off);
            floats_advance_to_readable_width(floats, cw, &cursor_y,
                                             &left_off, &right_off);
            double cw_avail = cw - left_off - right_off;
            if (cw_avail < 0) cw_avail = 0;
            c->x = inner_x + left_off;
            c->y = cursor_y;
            layout_box(c, cw_avail, child_inherited);
            if (c->kind == NS_BOX_INLINE && c->text && cw_avail < cw) {
                double band_bottom = floats_band_bottom(floats, cursor_y);
                if (band_bottom > cursor_y &&
                    c->y + c->content_height > band_bottom + 0.5) {
                    gsize split = inline_run_break_before(c,
                                                          band_bottom - c->y);
                    if (split > 0 && split < strlen(c->text)) {
                        inline_run_split(c, split);
                        layout_box(c, cw_avail, child_inherited);
                    }
                }
            }
            collect_escaping_floats(c, floats, 0);
            double line_height = ns_layout_inline_line_height(child_inherited);
            inline_line_top = c->y + MAX(0, c->content_height - line_height);
            cursor_y += c->content_height;
        }
        if ((c->kind == NS_BOX_IMAGE || c->kind == NS_BOX_VIDEO ||
             c->kind == NS_BOX_SVG || c->kind == NS_BOX_TABLE) &&
            c->content_width < cw) {
            double outer = c->content_width;
            if (c->kind == NS_BOX_TABLE)
                outer += c->padding.left + c->padding.right +
                         c->border.left + c->border.right +
                         c->margin.left + c->margin.right;
            const ns_css_value *ta = child_inherited
                ? child_inherited->values[NS_CSS_TEXT_ALIGN] : NULL;
            gboolean self_center = FALSE, self_right = FALSE;
            if (c->kind == NS_BOX_TABLE) {
                const char *al = c->dom ? ns_element_get_attr(c->dom, "align") : NULL;
                if (al && g_ascii_strcasecmp(al, "center") == 0) self_center = TRUE;
                else if (al && g_ascii_strcasecmp(al, "right") == 0) self_right = TRUE;
                const ns_css_value *ml = c->style ? c->style->values[NS_CSS_MARGIN_LEFT] : NULL;
                const ns_css_value *mr = c->style ? c->style->values[NS_CSS_MARGIN_RIGHT] : NULL;
                if (keyword_is(ml, "auto") && keyword_is(mr, "auto")) self_center = TRUE;
            }
            if ((keyword_is(ta, "center") || self_center) && outer < cw)
                shift_box_tree(c, inner_x + (cw - outer) / 2.0 - c->x, 0);
            else if ((keyword_is(ta, "right") || keyword_is(ta, "end") || self_right) && outer < cw)
                shift_box_tree(c, inner_x + (cw - outer) - c->x, 0);
        }
    }
    if (top_through != 0) {
        box->margin.top += top_through;
        box->margin_top_through = top_through;
        inner_y += top_through;
    }
    if (collapse_bottom_with_parent) {
        box->margin.bottom = collapsed_margin(box->margin.bottom,
                                              prev_margin_bottom);
    } else {
        cursor_y += prev_margin_bottom;
    }

    if (box_establishes_bfc(box) && floats && floats->len > 0) {
        double fb = floats_max_bottom(floats);
        if (fb > cursor_y) cursor_y = fb;
    }
    g_array_free(floats, TRUE);

    if (n_cols > 1 &&
        !layout_multicol_single_inline(box, inner_x, inner_y, cw, col_gap,
                                       n_cols, child_inherited, &cursor_y)) {
        ns_box *host = column_host ? column_host : box;
        double flow_x = column_host && host->first_child
            ? host->first_child->x : inner_x;
        double flow_y = column_host && host->first_child
            ? host->first_child->y : inner_y;
        double total_h = cursor_y - inner_y;
        if (column_host) {
            total_h = 0;
            for (ns_box *c = host->first_child; c; c = c->next_sibling) {
                if (c->kind == NS_BOX_BLOCK || c->kind == NS_BOX_TABLE)
                    total_h += c->content_height +
                               c->padding.top + c->padding.bottom +
                               c->border.top + c->border.bottom;
                else
                    total_h += c->content_height;
            }
        }
        double target_h = total_h / n_cols;
        double cur_y = 0;
        double max_col_h = 0;
        int cur_col = 0;
        for (ns_box *c = host->first_child; c; c = c->next_sibling) {
            double c_full_h;
            if (c->kind == NS_BOX_BLOCK || c->kind == NS_BOX_TABLE) {
                c_full_h = c->content_height +
                           c->padding.top + c->padding.bottom +
                           c->border.top + c->border.bottom;
            } else {
                c_full_h = c->content_height;
            }
            if (cur_col < n_cols - 1 &&
                cur_y > 0 && cur_y + c_full_h > target_h) {
                cur_col++;
                cur_y = 0;
            }
            double target_x = flow_x + cur_col * (cw + col_gap);
            double target_y = flow_y + cur_y;
            double dx = target_x - c->x;
            double dy = target_y - c->y;
            if (dx != 0 || dy != 0) shift_box_tree(c, dx, dy);
            cur_y += c_full_h;
            if (cur_y > max_col_h) max_col_h = cur_y;
        }
        box->content_width = cw * n_cols + col_gap * (n_cols - 1);
        if (column_host) {
            column_host->content_width = box->content_width;
            column_host->content_height = max_col_h;
            cursor_y = flow_y + max_col_h +
                       column_host->padding.bottom +
                       column_host->border.bottom +
                       column_host->margin.bottom;
        } else {
            cursor_y = inner_y + max_col_h;
        }
    }

flex_done: ;
    const ns_css_value *hv  = box->style ? box->style->values[NS_CSS_HEIGHT]     : NULL;
    const ns_css_value *mxh = box->style ? box->style->values[NS_CSS_MAX_HEIGHT] : NULL;
    const ns_css_value *mnh = box->style ? box->style->values[NS_CSS_MIN_HEIGHT] : NULL;
    const char *ovx = box->style
        ? overflow_axis_keyword(box->style, NS_CSS_OVERFLOW_X) : NULL;
    const char *ovy = box->style
        ? overflow_axis_keyword(box->style, NS_CSS_OVERFLOW_Y) : NULL;
    gboolean overflow_scrolls  = overflow_kw_scrolls(ovy);
    gboolean overflow_scrolls_x = overflow_kw_scrolls(ovx);
    if (legend) box_prepend_child(box, legend);
    double measured = cursor_y - inner_y + legend_extra;
    box->measured_content_height = measured;
    double explicit_h = -1;
    if (hv && (hv->kind == NS_CSS_V_LENGTH || hv->kind == NS_CSS_V_CALC))
        explicit_h = resolve_used_height(box, hv, parent_content_width, -1);
    double stretch_h = -1;
    if (height_keyword_stretches(hv) || height_keyword_stretches(mnh) ||
        height_keyword_stretches(mxh)) {
        double cb_h = containing_block_definite_height(box);
        if (cb_h >= 0) {
            stretch_h = cb_h - box->margin.top - box->margin.bottom
                      - box->border.top - box->border.bottom
                      - box->padding.top - box->padding.bottom;
            if (stretch_h < 0) stretch_h = 0;
        }
    }
    if (explicit_h < 0 && stretch_h >= 0 && height_keyword_stretches(hv)) {
        explicit_h = stretch_h;
        if (border_box) explicit_h += vert_extras;
    }
    if (explicit_h >= 0) {
        if (border_box) {
            explicit_h -= vert_extras;
            if (explicit_h < 0) explicit_h = 0;
        }
        box->content_height = explicit_h;
    } else {
        double ratio = box->style
            ? aspect_ratio_number(box->style->values[NS_CSS_ASPECT_RATIO], NULL) : -1;
        if (ratio > 0 && box->content_width > 0) {
            double aspect_h = box->content_width / ratio;
            gboolean content_floor =
                !(mnh && (mnh->kind == NS_CSS_V_LENGTH ||
                          mnh->kind == NS_CSS_V_CALC)) &&
                !overflow_scrolls &&
                !keyword_is(box->style->values[NS_CSS_OVERFLOW_Y], "hidden");
            box->content_height = content_floor && measured > aspect_h
                                ? measured : aspect_h;
        } else {
            box->content_height = measured;
        }
    }
    double max_h = resolve_used_height(box, mxh, parent_content_width, -1);
    if (border_box && max_h >= 0) {
        max_h -= vert_extras;
        if (max_h < 0) max_h = 0;
    }
    if (max_h < 0 && stretch_h >= 0 && height_keyword_stretches(mxh))
        max_h = stretch_h;
    if (max_h < 0 && size_keyword_is_intrinsic(mxh))
        max_h = measured;
    if (max_h >= 0 && box->content_height > max_h)
        box->content_height = max_h;
    double min_h = resolve_used_height(box, mnh, parent_content_width, -1);
    if (min_h < 0 && stretch_h >= 0 && height_keyword_stretches(mnh))
        min_h = stretch_h + (border_box ? vert_extras : 0);
    if (border_box && min_h >= 0) {
        min_h -= vert_extras;
        if (min_h < 0) min_h = 0;
    }
    if (min_h < 0 && size_keyword_is_intrinsic(mnh))
        min_h = measured;
    if (min_h >= 0 && box->content_height < min_h)
        box->content_height = min_h;
    double align_shift = block_align_content_shift(box, box->content_height - measured);
    if (align_shift != 0) {
        for (ns_box *c = box->first_child; c; c = c->next_sibling) {
            if (!style_is_absolute_or_fixed(c->style))
                shift_box_tree(c, 0, align_shift);
        }
    }
    if (align_shift == 0 &&
        box->dom && box->dom->kind == NS_NODE_ELEMENT && box->dom->name &&
        strcmp(box->dom->name, "input") == 0 &&
        box->content_height > measured + 0.5) {
        double shift = (box->content_height - measured) * 0.5;
        for (ns_box *c = box->first_child; c; c = c->next_sibling)
            c->y += shift;
    }
    if (align_shift == 0 &&
        box->dom && box->dom->kind == NS_NODE_ELEMENT && box->dom->name &&
        strcmp(box->dom->name, "button") == 0 &&
        !keyword_is(box->style ? box->style->values[NS_CSS_APPEARANCE] : NULL,
                    "none") &&
        box->content_height > measured + 0.5) {
        double shift = (box->content_height - measured) * 0.5;
        for (ns_box *c = box->first_child; c; c = c->next_sibling) {
            if (!style_is_absolute_or_fixed(c->style))
                shift_box_tree(c, 0, shift);
        }
    }
    if (overflow_scrolls) {
        box->scrolls = TRUE;
        if (measured > box->content_height)
            box->scroll_max_y = measured - box->content_height;
    }
    if (overflow_scrolls_x) {
        double content_right = inner_x + box->content_width;
        double padding_right = content_right + box->padding.right;
        double max_right = padding_right;
        for (const ns_box *c = box->first_child; c; c = c->next_sibling) {
            if (style_is_absolute_or_fixed(c->style)) continue;
            double right = c->x + c->margin.left + c->content_width +
                           c->padding.left + c->padding.right +
                           c->border.left + c->border.right;
            double flow_right = MIN(right + c->margin.right,
                                    MAX(right, content_right)) +
                                box->padding.right;
            if (right > max_right) max_right = right;
            if (flow_right > max_right) max_right = flow_right;
        }
        if (max_right > padding_right + 1.0) {
            box->scrolls = TRUE;
            box->scroll_max_x = max_right - padding_right;
        }
    }
    if (style_content_visibility_hidden(box->style)) {
        double contained_height = explicit_h >= 0 ? explicit_h : 0;
        if (max_h >= 0 && contained_height > max_h)
            contained_height = max_h;
        if (min_h >= 0 && contained_height < min_h)
            contained_height = min_h;
        box->content_height = contained_height;
        box->scrolls = FALSE;
        box->scroll_max_x = 0;
        box->scroll_max_y = 0;
    }
}

typedef struct {
    double w, h;
} ns_frame_viewport;

static GHashTable *g_frame_viewports;

static void
record_frame_viewports_walk(const ns_box *b)
{
    if (!b) return;
    if (b->dom && b->dom->kind == NS_NODE_ELEMENT && b->dom->name &&
        (strcmp(b->dom->name, "iframe") == 0 ||
         strcmp(b->dom->name, "frame") == 0)) {
        ns_frame_viewport *v = g_new0(ns_frame_viewport, 1);
        v->w = b->content_width > 0 ? b->content_width : 0;
        v->h = b->content_height > 0 ? b->content_height : 0;
        g_hash_table_insert(g_frame_viewports, (gpointer)b->dom, v);
    }
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        record_frame_viewports_walk(c);
}

static void
record_frame_viewports(const ns_box *root)
{
    if (!g_frame_viewports)
        g_frame_viewports = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                                  NULL, g_free);
    else
        g_hash_table_remove_all(g_frame_viewports);
    record_frame_viewports_walk(root);
}

gboolean
ns_layout_frame_viewport(const ns_node *frame, double *w, double *h)
{
    if (!frame || !g_frame_viewports) return FALSE;
    const ns_frame_viewport *v = g_hash_table_lookup(g_frame_viewports, frame);
    if (!v) return FALSE;
    if (w) *w = v->w;
    if (h) *h = v->h;
    return TRUE;
}

static double
control_char_cell_px(const ns_style *s)
{
    NsPangoLayout *layout = make_pango_layout(s);
    if (!layout) return 0;
    NsPangoContext *ctx = ns_pango_layout_get_context(layout);
    const NsPangoFontDescription *fd =
        ns_pango_layout_get_font_description(layout);
    if (!fd) fd = ns_pango_context_get_font_description(ctx);
    NsPangoFontMetrics *fm = fd ? ns_pango_context_get_metrics(ctx, fd, NULL)
                                : NULL;
    double cell = 0;
    if (fm) {
        cell = (double)ns_pango_font_metrics_get_approximate_char_width(fm) /
               NS_PANGO_SCALE;
        ns_pango_font_metrics_unref(fm);
    }
    g_object_unref(layout);
    return cell;
}

static gboolean
node_is_windowed_text_input(const ns_node *n)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    if (strcmp(n->name, "input") != 0) return FALSE;
    const char *type = ns_element_get_attr(n, "type");
    return !type || !*type ||
        g_ascii_strcasecmp(type, "text") == 0 ||
        g_ascii_strcasecmp(type, "search") == 0 ||
        g_ascii_strcasecmp(type, "email") == 0 ||
        g_ascii_strcasecmp(type, "url") == 0 ||
        g_ascii_strcasecmp(type, "tel") == 0 ||
        g_ascii_strcasecmp(type, "number") == 0 ||
        g_ascii_strcasecmp(type, "password") == 0;
}

static glong
text_input_display_cps(const ns_node *n)
{
    const char *v = ns_input_used_value(n);
    if (!v || !*v) v = ns_element_get_attr(n, "placeholder");
    return (v && *v) ? g_utf8_strlen(v, -1) : 0;
}

static gboolean
control_width_is_definite(const ns_style *s)
{
    double fs = length_or(s->values[NS_CSS_FONT_SIZE], 16);
    return control_dim_px_clamped(s, NS_CSS_WIDTH, NS_CSS_MIN_WIDTH,
                                  NS_CSS_MAX_WIDTH, fs, 0) > 0;
}

static gboolean
collect_text_input_columns(const ns_box *b, GHashTable *cols)
{
    if (!b) return FALSE;
    gboolean changed = FALSE;
    if (b->style && b->content_width > 0 &&
        node_is_windowed_text_input(b->dom)) {
        int have = text_input_size_attr(b->dom);
        glong shown = text_input_display_cps(b->dom);
        double cell = shown > 0 ? control_char_cell_px(b->style) : 0;
        if (cell > 0) {
            int overhead = 2 + text_input_leading_spaces(b->style);
            double cells = floor(b->content_width / cell) - overhead;
            int fit = cells < 1 ? 1
                    : cells > G_MAXINT16 ? G_MAXINT16 : (int)cells;
            gboolean grow = fit > have && shown > have;
            gboolean shrink = fit < have && shown > fit &&
                              control_width_is_definite(b->style);
            if (grow || shrink) {
                g_hash_table_insert(cols, (gpointer)b->dom,
                                    GINT_TO_POINTER(fit));
                changed = TRUE;
            }
        }
    }
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        if (collect_text_input_columns(c, cols)) changed = TRUE;
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++)
            if (collect_text_input_columns(
                    g_array_index(b->inline_atomics, ns_inline_atomic, i).box,
                    cols))
                changed = TRUE;
    return changed;
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
    g_counters_for_layout = build_counter_snapshots(doc, styles);
    ns_box *root = ns_layout_build_(doc, styles, viewport_width);
    if (!g_input_columns_for_layout) {
        GHashTable *cols = g_hash_table_new(g_direct_hash, g_direct_equal);
        if (collect_text_input_columns(root, cols)) {
            g_input_columns_for_layout = cols;
            ns_box_free(root);
            ns_image_cache_begin_generation(image_cache);
            root = ns_layout_build_(doc, styles, viewport_width);
            g_input_columns_for_layout = NULL;
        }
        g_hash_table_destroy(cols);
    }
    ns_image_cache_collect(image_cache);
    g_focused_input_for_layout = NULL;
    g_focused_is_contenteditable_for_layout = FALSE;
    g_focused_caret_byte_for_layout = 0;
    g_focused_sel_anchor_byte_for_layout = 0;
    g_image_cache_for_layout = NULL;
    g_base_url_for_layout = NULL;
    g_svg_defs_computed_for_layout = FALSE;
    if (g_counters_for_layout) {
        g_hash_table_destroy(g_counters_for_layout);
        g_counters_for_layout = NULL;
    }
    record_frame_viewports(root);
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
    shift_box_tree(box, dx, dy);
}

static gboolean
box_paint_unbounded(const ns_box *b)
{
    const ns_style *s = b->style;
    if (!s) return FALSE;
    const ns_css_value *tv = s->values[NS_CSS_TRANSFORM];
    if (tv && tv->kind == NS_CSS_V_TRANSFORM && tv->u.transform.n_ops > 0)
        return TRUE;
    if (s->values[NS_CSS_ANIMATION] || s->values[NS_CSS_TRANSITION])
        return TRUE;
    const ns_css_value *pv = s->values[NS_CSS_POSITION];
    if (pv && pv->kind == NS_CSS_V_KEYWORD && pv->u.keyword &&
        (strcmp(pv->u.keyword, "fixed") == 0 ||
         strcmp(pv->u.keyword, "sticky") == 0))
        return TRUE;
    return FALSE;
}

static void
compute_paint_bounds(ns_box *b)
{
    double top = b->y;
    double bottom = b->y + b->margin.top + b->border.top + b->padding.top +
                    b->content_height +
                    b->padding.bottom + b->border.bottom + b->margin.bottom;
    if (box_paint_unbounded(b)) {
        top = -G_MAXDOUBLE;
        bottom = G_MAXDOUBLE;
    }
    for (ns_box *c = b->first_child; c; c = c->next_sibling) {
        compute_paint_bounds(c);
        if (c->paint_top < top) top = c->paint_top;
        if (c->paint_bottom > bottom) bottom = c->paint_bottom;
    }
    if (b->inline_atomics) {
        for (guint i = 0; i < b->inline_atomics->len; i++) {
            const ns_inline_atomic *atomic =
                &g_array_index(b->inline_atomics, ns_inline_atomic, i);
            ns_box *ab = atomic->box;
            if (!ab) continue;
            compute_paint_bounds(ab);
            double dy = b->y + atomic->owner_offset_y + ab->rel_dy - ab->y;
            if (ab->paint_top + dy < top) top = ab->paint_top + dy;
            if (ab->paint_bottom + dy > bottom) bottom = ab->paint_bottom + dy;
        }
    }
    b->paint_top = top;
    b->paint_bottom = bottom;
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
    g_cq_seen_container = FALSE;
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

    layout_block(root, viewport_width, NULL);
    ns_layout_apply_position_offsets(root, viewport_width, root->content_height);
    ns_layout_process_absolute_boxes(root, styles, viewport_width);
    ns_paint_sync_inline_atomic_offsets(root);
    compute_paint_bounds(root);

    g_array_free(g_abs_pending, TRUE);
    g_abs_pending = NULL;
    g_clear_pointer(&g_abs_seen, g_hash_table_destroy);
    g_clear_pointer(&g_abs_ph_set, g_hash_table_destroy);
    g_clear_pointer(&g_abs_static, g_hash_table_destroy);
    g_hash_table_destroy(g_contains_block_media_cache);
    g_contains_block_media_cache = NULL;
    return root;
}

const char *
ns_box_kind_name(ns_box_kind k)
{
    switch (k) {
    case NS_BOX_BLOCK:      return "block";
    case NS_BOX_INLINE:     return "inline";
    case NS_BOX_TEXT:       return "text";
    case NS_BOX_IMAGE:      return "image";
    case NS_BOX_TABLE:      return "table";
    case NS_BOX_TABLE_CAPTION: return "caption";
    case NS_BOX_TABLE_ROW:  return "row";
    case NS_BOX_TABLE_CELL: return "cell";
    case NS_BOX_VIDEO:      return "video";
    case NS_BOX_MATH:       return "math";
    case NS_BOX_SVG:        return "svg";
    }
    return "?";
}

static void
collect_images_walk(const ns_box *b, GPtrArray *out)
{
    if (!b) return;
    if (b->kind == NS_BOX_IMAGE) g_ptr_array_add(out, (gpointer)b);
    if (b->media && (b->media->bg_image_src || b->media->marker_image_src ||
                     b->media->border_image_src))
        g_ptr_array_add(out, (gpointer)b);
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        collect_images_walk(c, out);
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++)
            collect_images_walk(
                g_array_index(b->inline_atomics, ns_inline_atomic, i).box, out);
}

static void
collect_videos_walk(const ns_box *b, GPtrArray *out)
{
    if (!b) return;
    if (b->kind == NS_BOX_VIDEO) g_ptr_array_add(out, (gpointer)b);
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        collect_videos_walk(c, out);
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++)
            collect_videos_walk(
                g_array_index(b->inline_atomics, ns_inline_atomic, i).box, out);
}

void
ns_layout_collect_videos(const ns_box *root, GPtrArray *out_boxes)
{
    collect_videos_walk(root, out_boxes);
}

void
ns_layout_collect_images(const ns_box *root, GPtrArray *out_boxes)
{
    collect_images_walk(root, out_boxes);
}

gboolean
ns_box_tree_has_sticky(const ns_box *root)
{
    if (!root) return FALSE;
    if (root->style) {
        const ns_css_value *v = root->style->values[NS_CSS_POSITION];
        if (v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
            strcmp(v->u.keyword, "sticky") == 0)
            return TRUE;
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling)
        if (ns_box_tree_has_sticky(c)) return TRUE;
    return FALSE;
}

static guint
count_matches_in_text(const char *text, const char *needle,
                      gboolean case_sensitive)
{
    if (!text || !needle || !*needle) return 0;
    gsize needle_len = strlen(needle);
    gsize text_len = strlen(text);
    guint hits = 0;
    for (gsize i = 0; i + needle_len <= text_len; ) {
        gboolean match = case_sensitive
            ? (strncmp(text + i, needle, needle_len) == 0)
            : (g_ascii_strncasecmp(text + i, needle, needle_len) == 0);
        if (match) {
            hits++;
            i += needle_len;
        } else {
            i++;
        }
    }
    return hits;
}

guint
ns_box_count_matches(const ns_box *root, const char *needle,
                     gboolean case_sensitive)
{
    if (!root || !needle || !*needle) return 0;
    guint sum = 0;
    if (root->kind == NS_BOX_INLINE && root->text)
        sum += count_matches_in_text(root->text, needle, case_sensitive);
    for (const ns_box *c = root->first_child; c; c = c->next_sibling)
        sum += ns_box_count_matches(c, needle, case_sensitive);
    return sum;
}

const ns_box *
ns_box_first_match_below(const ns_box *root, const char *needle,
                         double y_threshold, gboolean case_sensitive)
{
    if (!root || !needle || !*needle) return NULL;
    if (root->kind == NS_BOX_INLINE && root->text && root->y > y_threshold) {
        if (count_matches_in_text(root->text, needle, case_sensitive) > 0)
            return root;
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling) {
        const ns_box *m = ns_box_first_match_below(c, needle, y_threshold,
                                                   case_sensitive);
        if (m) return m;
    }
    return NULL;
}

const ns_box *
ns_box_first_match_above(const ns_box *root, const char *needle,
                         double y_threshold, gboolean case_sensitive)
{
    if (!root || !needle || !*needle) return NULL;
    const ns_box *best = NULL;
    if (root->kind == NS_BOX_INLINE && root->text && root->y < y_threshold) {
        if (count_matches_in_text(root->text, needle, case_sensitive) > 0)
            best = root;
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling) {
        const ns_box *m = ns_box_first_match_above(c, needle, y_threshold,
                                                   case_sensitive);
        if (m && (!best || m->y > best->y))
            best = m;
    }
    return best;
}

static gboolean
match_ordinal_walk(const ns_box *root, const char *needle,
                   const ns_box *target, gboolean case_sensitive,
                   guint *acc)
{
    if (!root) return FALSE;
    if (root->kind == NS_BOX_INLINE && root->text) {
        guint here = count_matches_in_text(root->text, needle, case_sensitive);
        if (root == target) {
            if (here > 0) { *acc += 1; return TRUE; }
            return FALSE;
        }
        *acc += here;
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling)
        if (match_ordinal_walk(c, needle, target, case_sensitive, acc))
            return TRUE;
    return FALSE;
}

guint
ns_box_match_ordinal(const ns_box *root, const char *needle,
                     const ns_box *target, gboolean case_sensitive)
{
    if (!root || !needle || !*needle || !target) return 0;
    guint acc = 0;
    if (match_ordinal_walk(root, needle, target, case_sensitive, &acc))
        return acc;
    return 0;
}

typedef struct {
    double   best;
    double   prev;
    gboolean any;
} snap_axis;

typedef struct {
    double left, top, right, bottom;
    double cur_x, cur_y;
    double prev_x, prev_y;
    double max_x, max_y;
} snap_port;

static double
snap_length(const ns_style *s, ns_css_prop p, double basis)
{
    const ns_css_value *v = s ? s->values[p] : NULL;
    if (!v || length_is_auto(v)) return 0;
    double px = length_resolve(v, basis, 0);
    return isfinite(px) ? px : 0;
}

static gboolean
snap_better(const snap_axis *axis, double candidate, double current)
{
    double travel = current - axis->prev;
    gboolean ahead = travel > 0.5 ? candidate > axis->prev + 0.5
                   : travel < -0.5 ? candidate < axis->prev - 0.5
                   : TRUE;
    gboolean best_ahead = travel > 0.5 ? axis->best > axis->prev + 0.5
                        : travel < -0.5 ? axis->best < axis->prev - 0.5
                        : TRUE;
    if (!axis->any) return TRUE;
    if (ahead != best_ahead) return ahead;
    return fabs(candidate - current) < fabs(axis->best - current);
}

static void
snap_consider(snap_axis *axis, double candidate, double current, double max)
{
    if (candidate < 0) candidate = 0;
    if (candidate > max) candidate = max;
    if (snap_better(axis, candidate, current)) {
        axis->best = candidate;
        axis->any = TRUE;
    }
}

static void
snap_collect(ns_box *b, const snap_port *port, snap_axis *ax, snap_axis *ay,
             gboolean want_x, gboolean want_y)
{
    double sp_top = port->top, sp_bottom = port->bottom;
    double sp_left = port->left, sp_right = port->right;
    for (ns_box *c = b->first_child; c; c = c->next_sibling) {
        const char *align = c->style
            ? ns_style_keyword(c->style, NS_CSS_SCROLL_SNAP_ALIGN) : NULL;
        if (align && strcmp(align, "none none") != 0) {
            char block[16] = {0}, inline_kw[16] = {0};
            sscanf(align, "%15s %15s", block, inline_kw);
            double port_w = sp_right - sp_left;
            double port_h = sp_bottom - sp_top;
            double top = c->y + c->margin.top -
                snap_length(c->style, NS_CSS_SCROLL_MARGIN_TOP, port_h);
            double bottom = c->y + c->margin.top + c->content_height +
                c->padding.top + c->padding.bottom +
                c->border.top + c->border.bottom +
                snap_length(c->style, NS_CSS_SCROLL_MARGIN_BOTTOM, port_h);
            double left = c->x + c->margin.left -
                snap_length(c->style, NS_CSS_SCROLL_MARGIN_LEFT, port_w);
            double right = c->x + c->margin.left + c->content_width +
                c->padding.left + c->padding.right +
                c->border.left + c->border.right +
                snap_length(c->style, NS_CSS_SCROLL_MARGIN_RIGHT, port_w);
            if (want_y && strcmp(block, "none") != 0)
                snap_consider(ay,
                    strcmp(block, "end") == 0 ? bottom - sp_bottom
                    : strcmp(block, "center") == 0
                        ? (top + bottom - sp_top - sp_bottom) / 2
                        : top - sp_top,
                    port->cur_y, port->max_y);
            if (want_x && strcmp(inline_kw, "none") != 0)
                snap_consider(ax,
                    strcmp(inline_kw, "end") == 0 ? right - sp_right
                    : strcmp(inline_kw, "center") == 0
                        ? (left + right - sp_left - sp_right) / 2
                        : left - sp_left,
                    port->cur_x, port->max_x);
        }
        if (c->scrolls) continue;
        snap_collect(c, port, ax, ay, want_x, want_y);
    }
}

static gboolean
snap_solve(ns_box *root, const char *type, const snap_port *port,
           double *out_x, double *out_y)
{
    char axis_kw[16] = {0}, strictness[16] = {0};
    sscanf(type, "%15s %15s", axis_kw, strictness);
    gboolean want_x = strcmp(axis_kw, "x") == 0 ||
                      strcmp(axis_kw, "inline") == 0 ||
                      strcmp(axis_kw, "both") == 0;
    gboolean want_y = strcmp(axis_kw, "y") == 0 ||
                      strcmp(axis_kw, "block") == 0 ||
                      strcmp(axis_kw, "both") == 0;
    if (!want_x && !want_y) return FALSE;

    snap_axis ax = { .prev = port->prev_x }, ay = { .prev = port->prev_y };
    snap_collect(root, port, &ax, &ay, want_x, want_y);

    gboolean mandatory = strcmp(strictness, "mandatory") == 0;
    double port_w = port->right - port->left;
    double port_h = port->bottom - port->top;
    gboolean snapped = FALSE;
    if (ay.any && (mandatory || fabs(ay.best - port->cur_y) <= port_h / 2)) {
        *out_y = ay.best;
        snapped = TRUE;
    }
    if (ax.any && (mandatory || fabs(ax.best - port->cur_x) <= port_w / 2)) {
        *out_x = ax.best;
        snapped = TRUE;
    }
    return snapped;
}

void
ns_box_scroll_snap_from(ns_box *scroller, double prev_x, double prev_y)
{
    if (!scroller || !scroller->style) return;
    const char *type = ns_style_keyword(scroller->style,
                                        NS_CSS_SCROLL_SNAP_TYPE);
    if (!type || strcmp(type, "none") == 0) return;

    double port_top = scroller->y + scroller->margin.top + scroller->border.top;
    double port_left = scroller->x + scroller->margin.left +
                       scroller->border.left;
    double port_h = scroller->content_height + scroller->padding.top +
                    scroller->padding.bottom;
    double port_w = scroller->content_width + scroller->padding.left +
                    scroller->padding.right;
    const ns_style *s = scroller->style;
    snap_port port = {
        .top = port_top + snap_length(s, NS_CSS_SCROLL_PADDING_TOP, port_h),
        .bottom = port_top + port_h -
                  snap_length(s, NS_CSS_SCROLL_PADDING_BOTTOM, port_h),
        .left = port_left + snap_length(s, NS_CSS_SCROLL_PADDING_LEFT, port_w),
        .right = port_left + port_w -
                 snap_length(s, NS_CSS_SCROLL_PADDING_RIGHT, port_w),
        .cur_x = scroller->scroll_x, .cur_y = scroller->scroll_y,
        .prev_x = prev_x, .prev_y = prev_y,
        .max_x = scroller->scroll_max_x, .max_y = scroller->scroll_max_y,
    };

    double x = scroller->scroll_x, y = scroller->scroll_y;
    if (snap_solve(scroller, type, &port, &x, &y)) {
        scroller->scroll_x = x;
        scroller->scroll_y = y;
    }
}

gboolean
ns_box_scroll_snap_viewport(ns_box *root, const ns_style *s,
                            double viewport_w, double viewport_h,
                            double max_x, double max_y,
                            double prev_x, double prev_y,
                            double *x, double *y)
{
    if (!root || !s || !x || !y) return FALSE;
    if (!(viewport_w > 0) || !(viewport_h > 0)) return FALSE;
    const char *type = ns_style_keyword(s, NS_CSS_SCROLL_SNAP_TYPE);
    if (!type || strcmp(type, "none") == 0) return FALSE;

    snap_port port = {
        .top = snap_length(s, NS_CSS_SCROLL_PADDING_TOP, viewport_h),
        .bottom = viewport_h -
                  snap_length(s, NS_CSS_SCROLL_PADDING_BOTTOM, viewport_h),
        .left = snap_length(s, NS_CSS_SCROLL_PADDING_LEFT, viewport_w),
        .right = viewport_w -
                 snap_length(s, NS_CSS_SCROLL_PADDING_RIGHT, viewport_w),
        .cur_x = *x, .cur_y = *y,
        .prev_x = prev_x, .prev_y = prev_y,
        .max_x = max_x, .max_y = max_y,
    };
    return snap_solve(root, type, &port, x, y);
}

void
ns_box_scroll_snap(ns_box *scroller)
{
    if (scroller)
        ns_box_scroll_snap_from(scroller, scroller->scroll_x,
                                scroller->scroll_y);
}

static gboolean
box_moves_when_painted(const ns_box *b)
{
    if (!b->style || !box_paint_unbounded(b)) return FALSE;
    const ns_css_value *pos = b->style->values[NS_CSS_POSITION];
    return !keyword_is(pos, "fixed") && !keyword_is(pos, "sticky");
}

static void
subtree_extent_y(const ns_box *b, double off, double *top, double *bottom,
                 gboolean *exact)
{
    if (box_moves_when_painted(b)) *exact = FALSE;
    double t = b->y + off;
    double bt = t + b->margin.top + b->border.top + b->padding.top +
                b->content_height + b->padding.bottom + b->border.bottom +
                b->margin.bottom;
    if (isfinite(t) && t < *top) *top = t;
    if (isfinite(bt) && bt > *bottom) *bottom = bt;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        subtree_extent_y(c, off, top, bottom, exact);
    if (!b->inline_atomics) return;
    for (guint i = 0; i < b->inline_atomics->len; i++) {
        const ns_inline_atomic *a =
            &g_array_index(b->inline_atomics, ns_inline_atomic, i);
        if (a->box)
            subtree_extent_y(a->box, off + b->y + a->owner_offset_y +
                                     a->box->rel_dy - a->box->y,
                             top, bottom, exact);
    }
}

gboolean
ns_box_subtree_extent_y(const ns_box *b, double *top, double *bottom)
{
    gboolean exact = TRUE;
    *top = G_MAXDOUBLE;
    *bottom = -G_MAXDOUBLE;
    if (b) subtree_extent_y(b, 0, top, bottom, &exact);
    if (*top > *bottom) *top = *bottom = 0;
    return exact;
}

const ns_box *
ns_box_find_by_id(const ns_box *root, const char *id)
{
    if (!root || !id) return NULL;
    if (root->dom && root->dom->kind == NS_NODE_ELEMENT) {
        const char *eid = ns_element_get_attr(root->dom, "id");
        if (eid && strcmp(eid, id) == 0) return root;
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling) {
        const ns_box *m = ns_box_find_by_id(c, id);
        if (m) return m;
    }
    return NULL;
}

const ns_box *
ns_box_find_by_id_or_name(const ns_box *root, const char *frag)
{
    if (!root || !frag) return NULL;
    if (root->dom && root->dom->kind == NS_NODE_ELEMENT) {
        const char *eid = ns_element_get_attr(root->dom, "id");
        if (eid && strcmp(eid, frag) == 0) return root;
        if (root->dom->name &&
            g_ascii_strcasecmp(root->dom->name, "a") == 0) {
            const char *nm = ns_element_get_attr(root->dom, "name");
            if (nm && strcmp(nm, frag) == 0) return root;
        }
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling) {
        const ns_box *m = ns_box_find_by_id_or_name(c, frag);
        if (m) return m;
    }
    return NULL;
}

static const ns_inline_attr *
inline_attr_element(const ns_inline_attr *r)
{
    return r->kind == NS_INLINE_ELEMENT ? r : NULL;
}

static void
box_inline_union_for_dom(const ns_box *root, const ns_node *target,
                         double dx, double dy,
                         double *x0, double *y0, double *x1, double *y1,
                         gboolean *any)
{
    if (!root) return;
    if (root->kind == NS_BOX_INLINE && root->attrs && root->text) {
        for (guint i = 0; i < root->attrs->len; i++) {
            const ns_inline_attr *r =
                &g_array_index(root->attrs, ns_inline_attr, i);
            if (r->dom != target || r->len == 0) continue;
            double ex, ey, ew, eh;
            if (!ns_paint_inline_range_extents(root, r->start, r->len,
                                               inline_attr_element(r),
                                               &ex, &ey, &ew, &eh))
                continue;
            double rx0 = root->x + dx + ex;
            double ry0 = root->y + dy + ey;
            double rx1 = rx0 + ew;
            double ry1 = ry0 + eh;
            if (!*any) {
                *x0 = rx0; *y0 = ry0; *x1 = rx1; *y1 = ry1;
                *any = TRUE;
            } else {
                if (rx0 < *x0) *x0 = rx0;
                if (ry0 < *y0) *y0 = ry0;
                if (rx1 > *x1) *x1 = rx1;
                if (ry1 > *y1) *y1 = ry1;
            }
        }
    }
    double cdx = dx - root->scroll_x;
    double cdy = dy - root->scroll_y;
    for (const ns_box *c = root->first_child; c; c = c->next_sibling)
        box_inline_union_for_dom(c, target, cdx, cdy, x0, y0, x1, y1, any);
    if (root->inline_atomics)
        for (guint i = 0; i < root->inline_atomics->len; i++) {
            const ns_inline_atomic *atomic =
                &g_array_index(root->inline_atomics, ns_inline_atomic, i);
            if (!atomic->box) continue;
            double adx = cdx + root->x + atomic->owner_offset_x +
                         atomic->box->rel_dx - atomic->box->x;
            double ady = cdy + root->y + atomic->owner_offset_y +
                         atomic->box->rel_dy - atomic->box->y;
            box_inline_union_for_dom(atomic->box, target, adx, ady,
                                     x0, y0, x1, y1, any);
        }
}

gboolean
ns_box_inline_rect_for_dom(const ns_box *root, const ns_node *target,
                           double *x, double *y, double *w, double *h)
{
    if (!root || !target) return FALSE;
    double x0 = 0, y0 = 0, x1 = 0, y1 = 0;
    gboolean any = FALSE;
    box_inline_union_for_dom(root, target, 0, 0, &x0, &y0, &x1, &y1, &any);
    if (!any) return FALSE;
    if (x) *x = x0;
    if (y) *y = y0;
    if (w) *w = x1 - x0;
    if (h) *h = y1 - y0;
    return TRUE;
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
ns_layout_min_width_of(ns_box *box, const ns_style *parent_style)
{
    return min_width_of(box, parent_style);
}

double
ns_layout_measure_natural_width(ns_box *box, const ns_style *parent_style)
{
    return measure_natural_width(box, parent_style);
}

double
ns_layout_min_content_width_of(ns_box *box, const ns_style *parent_style)
{
    return min_content_width_of(box, parent_style);
}

void
ns_layout_box(ns_box *box, double parent_content_width,
              const ns_style *inherited_style)
{
    layout_box(box, parent_content_width, inherited_style);
}

void
ns_layout_block(ns_box *box, double parent_content_width,
                const ns_style *inherited_style)
{
    layout_block(box, parent_content_width, inherited_style);
}

void
ns_layout_legacy_align_block_child(ns_box *c, double avail_x, double avail_w,
                                   const ns_style *inherited)
{
    legacy_align_block_child(c, avail_x, avail_w, inherited);
}

void
ns_layout_shift_box_tree(ns_box *b, double dx, double dy)
{
    shift_box_tree(b, dx, dy);
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
ns_layout_overflow_establishes_bfc(const ns_style *s)
{
    return overflow_establishes_bfc(s);
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
ns_layout_size_keyword_is_intrinsic(const ns_css_value *v)
{
    return size_keyword_is_intrinsic(v);
}

gboolean
ns_layout_height_keyword_stretches(const ns_css_value *v)
{
    return height_keyword_stretches(v);
}

double
ns_layout_intrinsic_keyword_width(ns_box *box, const char *kw,
                                  const ns_style *mi, double avail)
{
    return intrinsic_keyword_width(box, kw, mi, avail);
}

gboolean
ns_layout_box_is_scroll_container(const ns_box *b)
{
    return box_is_scroll_container(b);
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

const char *
ns_layout_overflow_axis_keyword(const ns_style *s, ns_css_prop axis)
{
    return overflow_axis_keyword(s, axis);
}

gboolean
ns_layout_overflow_kw_scrolls(const char *ov)
{
    return overflow_kw_scrolls(ov);
}

double
ns_layout_aspect_ratio_number(const ns_css_value *v, gboolean *with_auto)
{
    return aspect_ratio_number(v, with_auto);
}

double
ns_layout_gap_px(const ns_css_value *specific, const ns_css_value *shorthand,
                 double basis)
{
    return gap_px(specific, shorthand, basis);
}

gboolean
ns_layout_flex_box_is_border_box(const ns_box *c)
{
    return flex_box_is_border_box(c);
}

double
ns_layout_flex_grow_of(const ns_box *c)
{
    return flex_grow_of(c);
}

double
ns_layout_flex_shrink_of(const ns_box *c)
{
    return flex_shrink_of(c);
}

double
ns_layout_flex_gap_of(const ns_style *s, double basis)
{
    return flex_gap_of(s, basis);
}

gboolean
ns_layout_flex_wraps(const ns_style *s)
{
    return flex_wraps(s);
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
ns_layout_box_clips_children(const ns_box *b)
{
    return box_clips_children(b);
}

gboolean
ns_layout_style_creates_fixed_cb(const ns_style *s)
{
    return style_creates_fixed_cb(s);
}

double
ns_layout_estimate_natural_width(const ns_box *b, double cap)
{
    return estimate_natural_width(b, cap);
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

double
ns_layout_inline_attr_control_width(const ns_inline_attr *r, const ns_box *box)
{
    return inline_attr_control_width(r, box);
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

GArray *
ns_layout_measure_inline_atomics_begin(ns_box *box, const ns_style *parent_style,
                                       gboolean max_content)
{
    return measure_inline_atomics_begin(box, parent_style, max_content);
}

void
ns_layout_measure_inline_atomics_end(GArray *saved)
{
    measure_inline_atomics_end(saved);
}
