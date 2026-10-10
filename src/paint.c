/* Southstar — Cairo paint.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "paint.h"
#include "paint_internal.h"

static int g_dbg_paint_x = -2, g_dbg_paint_y = -2;

#include <math.h>
#include "ns_pango.h"
#include <stdlib.h>
#include <string.h>

#include "anim.h"
#include "css.h"
#include "dom.h"
#include "spellcheck.h"
#include "font.h"
#include "image.h"
#include "mathml.h"
#include "selection.h"
#include "svg.h"
#include "video.h"

typedef struct rgba {
    double r, g, b, a;
} rgba;

typedef struct paint_video_hole {
    double x0, y0, x1, y1;
} paint_video_hole;

static int            g_paint_no_cull;
static GPtrArray     *g_paint_deferred_list;

static int            g_paint_defer_depth;
static const ns_box  *g_paint_flush_box;

enum {
    PAINT_LAYERS_OFF,
    PAINT_LAYERS_PLAN,
    PAINT_LAYERS_DOC,
};

typedef struct paint_layers_state {
    int mode;
    const ns_box *root;
    const ns_box *owner;
    gboolean flush_layered;
    int vp_seen;
    cairo_matrix_t base;
    cairo_t *doc;
    GHashTable *kinds;
    GArray *found;
    ns_paint_upper_fn upper;
    gpointer upper_data;
    int n_upper;
    gboolean video_above;
} paint_layers_state;

static paint_layers_state g_layers;
static gboolean g_paint_have_viewport;
static double g_paint_vp_x0, g_paint_vp_y0;
static GHashTable *g_paint_sel_runs;
static const ns_box *g_paint_sticky_static_box;

static GArray        *g_paint_video_holes;

void
ns_paint_video_hole_record(cairo_t *cr, double x, double y, double w, double h)
{
    paint_video_hole hole = { x, y, x + w, y + h };
    cairo_user_to_device(cr, &hole.x0, &hole.y0);
    cairo_user_to_device(cr, &hole.x1, &hole.y1);
    if (hole.x0 > hole.x1) {
        double swap = hole.x0;
        hole.x0 = hole.x1;
        hole.x1 = swap;
    }
    if (hole.y0 > hole.y1) {
        double swap = hole.y0;
        hole.y0 = hole.y1;
        hole.y1 = swap;
    }
    if (!g_paint_video_holes)
        g_paint_video_holes = g_array_new(FALSE, FALSE,
                                           sizeof(paint_video_hole));
    g_array_append_val(g_paint_video_holes, hole);
}

static void
paint_group_video_holes(cairo_t *cr, cairo_pattern_t *source,
                        cairo_pattern_t *mask, double opacity,
                        guint first_hole)
{
    if (!g_paint_video_holes || first_hole >= g_paint_video_holes->len)
        return;
    cairo_save(cr);
    cairo_new_path(cr);
    for (guint i = first_hole; i < g_paint_video_holes->len; i++) {
        paint_video_hole hole =
            g_array_index(g_paint_video_holes, paint_video_hole, i);
        cairo_device_to_user(cr, &hole.x0, &hole.y0);
        cairo_device_to_user(cr, &hole.x1, &hole.y1);
        cairo_rectangle(cr, hole.x0, hole.y0,
                        hole.x1 - hole.x0, hole.y1 - hole.y0);
    }
    cairo_clip(cr);
    cairo_set_source(cr, source);
    cairo_set_operator(cr, CAIRO_OPERATOR_SOURCE);
    if (mask)
        cairo_mask(cr, mask);
    else if (opacity < 0.999)
        cairo_paint_with_alpha(cr, opacity);
    else
        cairo_paint(cr);
    cairo_restore(cr);
}

static rgba
rgba_of(const ns_css_value *v, double dr, double dg, double db, double da)
{
    rgba c = { dr, dg, db, da };
    if (!v || v->kind != NS_CSS_V_COLOR) return c;
    c.r = v->u.color.r / 255.0;
    c.g = v->u.color.g / 255.0;
    c.b = v->u.color.b / 255.0;
    c.a = v->u.color.a / 255.0;
    return c;
}

static inline void
set_source_rgba(cairo_t *cr, rgba c)
{
    cairo_set_source_rgba(cr, c.r, c.g, c.b, c.a);
}

static gboolean
overflow_kw_clips(const char *ov)
{
    return ov && (g_ascii_strcasecmp(ov, "hidden") == 0 ||
                  g_ascii_strcasecmp(ov, "clip")   == 0 ||
                  g_ascii_strcasecmp(ov, "auto")   == 0 ||
                  g_ascii_strcasecmp(ov, "scroll") == 0);
}

#define length_or ns_css_length_or

#define keyword_is ns_css_keyword_is

static void paint_walk(cairo_t *cr, const ns_box *b, const char *highlight);

static gboolean
box_is_hidden(const ns_box *b)
{
    const ns_style *s = b ? b->style : NULL;
    if (!s) return FALSE;
    const ns_css_value *v = s->values[NS_CSS_VISIBILITY];
    if (v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
        (strcmp(v->u.keyword, "hidden") == 0 ||
         strcmp(v->u.keyword, "collapse") == 0))
        return TRUE;
    return FALSE;
}

static gboolean
box_skips_contents(const ns_box *b)
{
    const ns_style *s = b ? b->style : NULL;
    const ns_css_value *v = s ? s->values[NS_CSS_CONTENT_VISIBILITY] : NULL;
    return v && v->kind == NS_CSS_V_KEYWORD && v->u.keyword &&
           strcmp(v->u.keyword, "hidden") == 0;
}

static double
box_opacity(const ns_box *b)
{
    if (b && ns_paint_anim()) {
        double anim_o;
        if (ns_anim_get_opacity(ns_paint_anim(), b->dom, &anim_o)) {
            if (anim_o < 0) anim_o = 0;
            if (anim_o > 1) anim_o = 1;
            return anim_o;
        }
    }
    const ns_style *s = b ? b->style : NULL;
    if (!s) return 1.0;
    const ns_css_value *v = s->values[NS_CSS_OPACITY];
    if (!v) return 1.0;
    if (v->kind == NS_CSS_V_LENGTH) {
        double o = v->u.length.v;
        if (o < 0) o = 0;
        if (o > 1) o = 1;
        return o;
    }
    return 1.0;
}

static gboolean
box_is_positioned(const ns_box *b)
{
    const ns_style *s = b ? b->style : NULL;
    if (!s) return FALSE;
    const ns_css_value *v = s->values[NS_CSS_POSITION];
    if (!v || v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return FALSE;
    const char *kw = v->u.keyword;
    return strcmp(kw, "relative") == 0 || strcmp(kw, "absolute") == 0 ||
           strcmp(kw, "fixed") == 0    || strcmp(kw, "sticky") == 0;
}

static gboolean
box_clip_hides(const ns_box *b)
{
    const ns_style *s = b ? b->style : NULL;
    if (!s) return FALSE;
    const ns_css_value *cv = s->values[NS_CSS_CLIP];
    if (!cv || cv->kind != NS_CSS_V_RECT) return FALSE;
    const ns_css_value *pv = s->values[NS_CSS_POSITION];
    if (!pv || pv->kind != NS_CSS_V_KEYWORD || !pv->u.keyword) return FALSE;
    if (strcmp(pv->u.keyword, "absolute") != 0 &&
        strcmp(pv->u.keyword, "fixed") != 0) return FALSE;
    double bw = b->content_width + b->padding.left + b->padding.right +
                b->border.left + b->border.right;
    double bh = b->content_height + b->padding.top + b->padding.bottom +
                b->border.top + b->border.bottom;
    double top    = cv->u.rect.is_auto[0] ? 0  : cv->u.rect.v[0];
    double right  = cv->u.rect.is_auto[1] ? bw : cv->u.rect.v[1];
    double bottom = cv->u.rect.is_auto[2] ? bh : cv->u.rect.v[2];
    double left   = cv->u.rect.is_auto[3] ? 0  : cv->u.rect.v[3];
    return (right - left) <= 0 || (bottom - top) <= 0;
}

static int
box_z_index(const ns_box *b)
{
    const ns_style *s = b ? b->style : NULL;
    if (!s) return 0;
    const ns_css_value *v = s->values[NS_CSS_Z_INDEX];
    if (!v || v->kind != NS_CSS_V_LENGTH) return 0;
    return (int)v->u.length.v;
}

typedef struct paint_entry {
    const ns_box *box;
    int key;
    guint order;
} paint_entry;

static int
paint_entry_cmp(const void *a, const void *b)
{
    const paint_entry *pa = a;
    const paint_entry *pb = b;
    if (pa->key != pb->key) return pa->key < pb->key ? -1 : 1;
    if (pa->order != pb->order) return pa->order < pb->order ? -1 : 1;
    return 0;
}

static gboolean
box_z_index_is_auto(const ns_box *b)
{
    const ns_css_value *v = b && b->style ? b->style->values[NS_CSS_Z_INDEX]
                                          : NULL;
    return !v || v->kind != NS_CSS_V_LENGTH;
}

static gboolean
box_is_flex_or_grid_item(const ns_box *b)
{
    const ns_box *p = b ? b->parent : NULL;
    while (p && !p->style) p = p->parent;
    if (!p) return FALSE;
    ns_display d = ns_css_display_of(p->style);
    return ns_display_is_flex_container(d) || ns_display_is_grid_container(d);
}

static gboolean
box_defers_to_positioned_layer(const ns_box *b)
{
    if (box_z_index(b) < 0) return FALSE;
    return box_is_positioned(b) ||
           (!box_z_index_is_auto(b) && box_is_flex_or_grid_item(b));
}

static gboolean
box_isolates_positioned_descendants(const ns_box *b)
{
    if (!box_z_index_is_auto(b)) return TRUE;
    const ns_style *s = b->style;
    if (!s) return FALSE;
    const ns_css_value *pos = s->values[NS_CSS_POSITION];
    if (keyword_is(pos, "fixed") || keyword_is(pos, "sticky")) return TRUE;
    const ns_css_value *filter = s->values[NS_CSS_FILTER];
    return filter && !keyword_is(filter, "none");
}

static int
dom_tree_order_cmp(const ns_node *a, const ns_node *b)
{
    if (!a || !b || a == b) return 0;
    const ns_node *pa[128], *pb[128];
    int na = 0, nb = 0;
    for (const ns_node *n = a; n && na < 128; n = n->parent) pa[na++] = n;
    for (const ns_node *n = b; n && nb < 128; n = n->parent) pb[nb++] = n;
    if (na >= 128 || nb >= 128) return 0;
    int ia = na - 1, ib = nb - 1;
    while (ia >= 0 && ib >= 0 && pa[ia] == pb[ib]) { ia--; ib--; }
    if (ia < 0) return -1;
    if (ib < 0) return 1;
    if (pa[ia]->parent != pb[ib]->parent) return 0;
    for (const ns_node *s = pa[ia]->next_sibling; s; s = s->next_sibling)
        if (s == pb[ib]) return -1;
    return 1;
}

typedef struct deferred_capture {
    const ns_box *box;
    double dev_x, dev_y;
    guint seq;
} deferred_capture;

static guint g_paint_capture_seq;

static int
deferred_capture_cmp(const void *va, const void *vb)
{
    const deferred_capture *a = *(deferred_capture *const *)va;
    const deferred_capture *b = *(deferred_capture *const *)vb;
    const ns_box *ab = a->box, *bb = b->box;
    if (!ab || !bb) return ab ? 1 : bb ? -1 : 0;
    int za = box_z_index(ab), zb = box_z_index(bb);
    if (za != zb) return za < zb ? -1 : 1;
    int c = dom_tree_order_cmp(ab->dom, bb->dom);
    if (c) return c;
    return a->seq < b->seq ? -1 : a->seq > b->seq ? 1 : 0;
}

static void
layers_record(cairo_t *cr, const ns_box *box, int kind, double dx, double dy)
{
    ns_paint_vp_capture layer = { .box = box, .kind = kind };
    cairo_matrix_t m, inv = g_layers.base;
    cairo_get_matrix(cr, &m);
    cairo_matrix_translate(&m, dx, dy);
    if (cairo_matrix_invert(&inv) != CAIRO_STATUS_SUCCESS)
        cairo_matrix_init_identity(&inv);
    cairo_matrix_multiply(&layer.rel, &m, &inv);
    g_array_append_val(g_layers.found, layer);
}

static cairo_t *
layers_target(cairo_t *cr, const deferred_capture *cap, double dx, double dy)
{
    int kind = GPOINTER_TO_INT(g_hash_table_lookup(g_layers.kinds, cap->box));
    if (kind) {
        if (g_layers.mode == PAINT_LAYERS_PLAN)
            layers_record(cr, cap->box, kind, dx, dy);
        g_layers.vp_seen++;
        return NULL;
    }
    if (g_layers.mode == PAINT_LAYERS_PLAN) return cr;
    if (g_layers.vp_seen == 0 || g_layers.n_upper == 0) return g_layers.doc;
    int i = MIN(g_layers.vp_seen, g_layers.n_upper) - 1;
    cairo_t *upper = g_layers.upper(i, g_layers.upper_data);
    return upper ? upper : g_layers.doc;
}

static void
paint_flush_deferred(cairo_t *cr, GPtrArray *list, const char *highlight)
{
    gboolean layered = g_layers.mode != PAINT_LAYERS_OFF &&
                       g_layers.flush_layered;
    g_layers.flush_layered = FALSE;
    if (!list || list->len == 0) return;
    GPtrArray *queue = g_ptr_array_sized_new(list->len);
    GPtrArray *adopted =
        g_ptr_array_new_with_free_func((GDestroyNotify)g_ptr_array_unref);
    for (guint i = 0; i < list->len; i++)
        g_ptr_array_add(queue, g_ptr_array_index(list, i));
    qsort(queue->pdata, queue->len, sizeof(gpointer), deferred_capture_cmp);
    const ns_box *saved_flush = g_paint_flush_box;
    for (guint i = 0; i < queue->len; i++) {
        const deferred_capture *cap = g_ptr_array_index(queue, i);
        double cur_x = 0, cur_y = 0;
        cairo_user_to_device(cr, &cur_x, &cur_y);
        double dx = cap->dev_x - cur_x;
        double dy = cap->dev_y - cur_y;
        if (isnan(dx) || isnan(dy)) dx = dy = 0;
        cairo_t *target = layered ? layers_target(cr, cap, dx, dy) : cr;
        if (!target) continue;
        if (layered) g_layers.owner = cap->box;
        cairo_save(target);
        if (target != cr) {
            cairo_matrix_t m;
            cairo_get_matrix(cr, &m);
            cairo_set_matrix(target, &m);
        }
        if (dx != 0 || dy != 0) cairo_translate(target, dx, dy);
        g_paint_flush_box = cap->box;
        if (g_dbg_paint_x >= 0 && cap->box->dom) {
            double gx0, gy0, gx1, gy1;
            cairo_clip_extents(cr, &gx0, &gy0, &gx1, &gy1);
            g_printerr("[flush-one] <%s#%s y=%.0f h=%.0f> d=%.0f,%.0f "
                       "clip=%.0f,%.0f..%.0f,%.0f\n",
                       cap->box->dom->name ? cap->box->dom->name : "?",
                       ns_element_get_attr(cap->box->dom, "id")
                           ? ns_element_get_attr(cap->box->dom, "id")
                           : "",
                       cap->box->y, cap->box->content_height,
                       dx, dy, gx0, gy0, gx1, gy1);
        }
        gboolean flat = !box_isolates_positioned_descendants(cap->box);
        GPtrArray *saved_list = g_paint_deferred_list;
        if (flat) {
            g_paint_deferred_list = NULL;
            g_paint_defer_depth++;
        }
        paint_walk(target, cap->box, highlight);
        if (flat) {
            GPtrArray *found = g_paint_deferred_list;
            g_paint_deferred_list = saved_list;
            g_paint_defer_depth--;
            if (found) {
                for (guint k = 0; k < found->len; k++)
                    g_ptr_array_add(queue, g_ptr_array_index(found, k));
                qsort(queue->pdata + i + 1, queue->len - i - 1,
                      sizeof(gpointer), deferred_capture_cmp);
                g_ptr_array_add(adopted, found);
            }
        }
        cairo_restore(target);
    }
    g_paint_flush_box = saved_flush;
    g_ptr_array_free(queue, TRUE);
    g_ptr_array_free(adopted, TRUE);
}

static double g_paint_anchor_dx, g_paint_anchor_dy;

static void
paint_anchor_leave(cairo_t *cr, double saved_dx, double saved_dy)
{
    cairo_restore(cr);
    g_paint_anchor_dx = saved_dx;
    g_paint_anchor_dy = saved_dy;
}

static void
compute_sticky_offset(const ns_box *b, cairo_t *cr,
                      double *out_dx, double *out_dy)
{
    *out_dx = 0;
    *out_dy = 0;
    if (!b || !b->style || b == g_paint_sticky_static_box) return;
    if (ns_box_is_fixed(b)) {
        if (g_paint_have_viewport) {
            *out_dx = g_paint_vp_x0;
            *out_dy = g_paint_vp_y0;
        }
        return;
    }
    if (!keyword_is(b->style->values[NS_CSS_POSITION], "sticky")) return;
    double clip_x1, clip_y1, clip_x2, clip_y2;
    cairo_clip_extents(cr, &clip_x1, &clip_y1, &clip_x2, &clip_y2);
    ns_box_sticky_offset(b, clip_x1, clip_y1, clip_x2, clip_y2,
                         out_dx, out_dy);
}

static cairo_operator_t
blend_mode_operator(const ns_style *s)
{
    if (!s || !s->values[NS_CSS_MIX_BLEND_MODE]) return CAIRO_OPERATOR_OVER;
    const ns_css_value *v = s->values[NS_CSS_MIX_BLEND_MODE];
    if (v->kind != NS_CSS_V_KEYWORD || !v->u.keyword) return CAIRO_OPERATOR_OVER;
    const char *k = v->u.keyword;
    if (strcmp(k, "multiply")    == 0) return CAIRO_OPERATOR_MULTIPLY;
    if (strcmp(k, "screen")      == 0) return CAIRO_OPERATOR_SCREEN;
    if (strcmp(k, "overlay")     == 0) return CAIRO_OPERATOR_OVERLAY;
    if (strcmp(k, "darken")      == 0) return CAIRO_OPERATOR_DARKEN;
    if (strcmp(k, "lighten")     == 0) return CAIRO_OPERATOR_LIGHTEN;
    if (strcmp(k, "color-dodge") == 0) return CAIRO_OPERATOR_COLOR_DODGE;
    if (strcmp(k, "color-burn")  == 0) return CAIRO_OPERATOR_COLOR_BURN;
    if (strcmp(k, "hard-light")  == 0) return CAIRO_OPERATOR_HARD_LIGHT;
    if (strcmp(k, "soft-light")  == 0) return CAIRO_OPERATOR_SOFT_LIGHT;
    if (strcmp(k, "difference")  == 0) return CAIRO_OPERATOR_DIFFERENCE;
    if (strcmp(k, "exclusion")   == 0) return CAIRO_OPERATOR_EXCLUSION;
    if (strcmp(k, "hue")         == 0) return CAIRO_OPERATOR_HSL_HUE;
    if (strcmp(k, "saturation")  == 0) return CAIRO_OPERATOR_HSL_SATURATION;
    if (strcmp(k, "color")       == 0) return CAIRO_OPERATOR_HSL_COLOR;
    if (strcmp(k, "luminosity")  == 0) return CAIRO_OPERATOR_HSL_LUMINOSITY;
    return CAIRO_OPERATOR_OVER;
}

static const ns_box *g_paint_skip_box;
static ns_paint_stats g_paint_stats;
static gboolean g_paint_collect_stats;
static gboolean g_paint_have_clip;
static double g_paint_clip_y0, g_paint_clip_y1;
static double g_paint_cull_margin = 400.0;

static void
paint_cache_clip(cairo_t *cr)
{
    double x0, x1;
    cairo_clip_extents(cr, &x0, &g_paint_clip_y0, &x1, &g_paint_clip_y1);
    g_paint_have_clip = TRUE;
    g_paint_vp_x0 = x0;
    g_paint_vp_y0 = g_paint_clip_y0;
    g_paint_have_viewport = isfinite(x0) && isfinite(g_paint_clip_y0) &&
                            (x0 != 0 || g_paint_clip_y0 != 0);
    g_paint_anchor_dx = 0;
    g_paint_anchor_dy = 0;
}

static const ns_box *g_paint_tex_root;

typedef struct ns_quad3 {
    const ns_box *box;
    ns_mat4 m;
    double bx, by, bw, bh;
    double depth;
    guint seq;
    gboolean own_only;
} ns_quad3;

typedef struct quad_tex_entry {
    cairo_surface_t *surf;
    int tw, th;
} quad_tex_entry;

static GHashTable *g_paint_3d_tex;

static void
quad_tex_entry_free(gpointer p)
{
    quad_tex_entry *e = p;
    cairo_surface_destroy(e->surf);
    g_free(e);
}

static void
box_border_rect(const ns_box *b, double *bx, double *by, double *bw, double *bh)
{
    *bx = b->x + b->margin.left;
    *by = b->y + b->margin.top;
    *bw = b->content_width + b->padding.left + b->padding.right +
          b->border.left + b->border.right;
    *bh = b->content_height + b->padding.top + b->padding.bottom +
          b->border.top + b->border.bottom;
}

static gboolean
box_preserve3d(const ns_box *b)
{
    if (!b->style || !b->style->values[NS_CSS_TRANSFORM_STYLE]) return FALSE;
    const char *kw = ns_style_keyword(b->style, NS_CSS_TRANSFORM_STYLE);
    return kw && strcmp(kw, "preserve-3d") == 0;
}

static double
box_perspective_px(const ns_box *b)
{
    const ns_css_value *v =
        b->style ? b->style->values[NS_CSS_PERSPECTIVE] : NULL;
    if (v && v->kind == NS_CSS_V_LENGTH && v->u.length.v > 0)
        return v->u.length.v;
    return 0;
}

static gboolean
box_establishes_3d(const ns_box *b)
{
    if (box_perspective_px(b) > 0) return TRUE;
    if (!box_preserve3d(b)) return FALSE;
    return !(b->parent && box_preserve3d(b->parent));
}

static gboolean
box_has_own_decor(const ns_box *b)
{
    const ns_style *st = b->style;
    if (!st) return FALSE;
    const ns_css_value *bg = st->values[NS_CSS_BACKGROUND_COLOR];
    if (bg && bg->kind == NS_CSS_V_COLOR && bg->u.color.a > 0) return TRUE;
    const ns_css_value *bi = st->values[NS_CSS_BACKGROUND_IMAGE];
    if (bi && (bi->kind == NS_CSS_V_URL || bi->kind == NS_CSS_V_GRADIENT))
        return TRUE;
    if (b->border.left > 0 || b->border.right > 0 ||
        b->border.top > 0 || b->border.bottom > 0)
        return TRUE;
    const ns_css_value *ow = st->values[NS_CSS_OUTLINE_WIDTH];
    const ns_css_value *os = st->values[NS_CSS_OUTLINE_STYLE];
    if (ow && os && os->kind == NS_CSS_V_KEYWORD && os->u.keyword &&
        strcmp(os->u.keyword, "none") != 0 && length_or(ow, 0) > 0)
        return TRUE;
    return FALSE;
}

static gboolean
box_subtree_paints(const ns_box *b)
{
    if (box_is_hidden(b)) return FALSE;
    if (b->kind == NS_BOX_IMAGE || b->kind == NS_BOX_VIDEO ||
        b->kind == NS_BOX_INLINE || b->kind == NS_BOX_TEXT)
        return TRUE;
    if (box_has_own_decor(b)) return TRUE;
    if (box_skips_contents(b)) return FALSE;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        if (box_subtree_paints(c)) return TRUE;
    return FALSE;
}

static void
box_transform_origin(const ns_box *b, double bx, double by, double bw,
                     double bh, ns_css_prop prop,
                     double *ox, double *oy, double *oz)
{
    *ox = bx + bw / 2.0;
    *oy = by + bh / 2.0;
    *oz = 0;
    const ns_css_value *origin = b->style ? b->style->values[prop] : NULL;
    if (origin && origin->kind == NS_CSS_V_TRANSFORM &&
        origin->u.transform.n_ops > 0) {
        const ns_css_transform_op *o = &origin->u.transform.ops[0];
        *ox = bx + (o->a_is_percent ? o->a / 100.0 * bw : o->a);
        *oy = by + (o->b_is_percent ? o->b / 100.0 * bh : o->b);
        *oz = o->c;
    }
}

static void
collect_3d_quads(const ns_box *b, const ns_mat4 *pm, GArray *quads)
{
    if (box_is_hidden(b) || b == g_paint_skip_box) return;
    double bx, by, bw, bh;
    box_border_rect(b, &bx, &by, &bw, &bh);
    ns_mat4 m = *pm;
    const ns_css_transform *anim_tf =
        ns_paint_anim() ? ns_anim_get_transform(ns_paint_anim(), b->dom) : NULL;
    ns_css_transform eff;
    eff.n_ops = 0;
    if (anim_tf ||
        (b->style && (b->style->values[NS_CSS_TRANSFORM] ||
                      b->style->values[NS_CSS_TRANSLATE] ||
                      b->style->values[NS_CSS_ROTATE] ||
                      b->style->values[NS_CSS_SCALE])))
        ns_css_style_effective_transform(b->style, anim_tf, &eff);
    if (eff.n_ops > 0) {
        double ox, oy, oz;
        box_transform_origin(b, bx, by, bw, bh, NS_CSS_TRANSFORM_ORIGIN,
                             &ox, &oy, &oz);
        ns_mat4 tm;
        ns_css_transform_to_mat4(&eff, bw, bh, &tm);
        ns_mat4_translate(&m, ox, oy, oz);
        ns_mat4_multiply(&m, &tm, &m);
        ns_mat4_translate(&m, -ox, -oy, -oz);
    }
    gboolean p3d = box_preserve3d(b);
    if (!p3d || !b->first_child) {
        if (box_subtree_paints(b)) {
            ns_quad3 q = { b, m, bx, by, bw, bh, 0, quads->len, FALSE };
            g_array_append_val(quads, q);
        }
        return;
    }
    if (box_has_own_decor(b)) {
        ns_quad3 q = { b, m, bx, by, bw, bh, 0, quads->len, TRUE };
        g_array_append_val(quads, q);
    }
    ns_mat4 cm = m;
    double d = box_perspective_px(b);
    if (d > 0) {
        double pox, poy, poz;
        box_transform_origin(b, bx, by, bw, bh, NS_CSS_PERSPECTIVE_ORIGIN,
                             &pox, &poy, &poz);
        ns_mat4_translate(&cm, pox, poy, 0);
        ns_mat4_perspective(&cm, d);
        ns_mat4_translate(&cm, -pox, -poy, 0);
    }
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        collect_3d_quads(c, &cm, quads);
}

static void
paint_quad3(cairo_t *cr, const ns_quad3 *q, const char *highlight)
{
    if (q->bw < 0.5 || q->bh < 0.5) return;
    const double weps = 0.02;
    double px[4], py[4], pws[4];
    const double cxs[4] = { q->bx, q->bx + q->bw, q->bx + q->bw, q->bx };
    const double cys[4] = { q->by, q->by, q->by + q->bh, q->by + q->bh };
    int behind = 0;
    for (int i = 0; i < 4; i++) {
        double ox, oy, oz, ow;
        ns_mat4_apply(&q->m, cxs[i], cys[i], 0, &ox, &oy, &oz, &ow);
        pws[i] = ow;
        if (ow < weps) {
            behind++;
            px[i] = 0;
            py[i] = 0;
            continue;
        }
        px[i] = ox / ow;
        py[i] = oy / ow;
    }
    if (behind == 4) return;
    gboolean clipped = behind > 0;
    double clx0, cly0, clx1, cly1;
    cairo_clip_extents(cr, &clx0, &cly0, &clx1, &cly1);
    double minx = 0, maxx = 0, miny = 0, maxy = 0;
    if (!clipped) {
        double area2 = (px[1] - px[0]) * (py[3] - py[0]) -
                       (px[3] - px[0]) * (py[1] - py[0]);
        if (fabs(area2) < 0.01) return;
        if (area2 < 0 && q->box->style) {
            const char *bfv =
                ns_style_keyword(q->box->style, NS_CSS_BACKFACE_VISIBILITY);
            if (bfv && strcmp(bfv, "hidden") == 0) return;
        }
        minx = px[0]; maxx = px[0]; miny = py[0]; maxy = py[0];
        for (int i = 1; i < 4; i++) {
            minx = MIN(minx, px[i]);
            maxx = MAX(maxx, px[i]);
            miny = MIN(miny, py[i]);
            maxy = MAX(maxy, py[i]);
        }
        if (maxx < clx0 || minx > clx1 || maxy < cly0 || miny > cly1) return;
    } else {
        minx = clx0; maxx = clx1; miny = cly0; maxy = cly1;
    }

    double k = clipped ? 2.0
                       : MAX((maxx - minx) / MAX(q->bw, 1.0),
                             (maxy - miny) / MAX(q->bh, 1.0));
    k = CLAMP(k, 1.0, 3.0);
    double maxdim = MAX(q->bw, q->bh);
    if (maxdim > 0.0 && maxdim * k > 4096.0)
        k = 4096.0 / maxdim;
    int tw = (int)ceil(q->bw * k);
    int th = (int)ceil(q->bh * k);
    if (tw < 1 || th < 1) return;
    if (tw > 4096) { k *= 4096.0 / tw; tw = 4096; th = (int)ceil(q->bh * k); }
    if (th > 4096) { k *= 4096.0 / th; th = 4096; tw = (int)ceil(q->bw * k); }
    gboolean cacheable = q->box->dom && !q->own_only &&
                         !q->box->first_child &&
                         q->box->kind == NS_BOX_BLOCK &&
                         !(q->box->dom->name &&
                           strcmp(q->box->dom->name, "canvas") == 0);
    if (cacheable && ns_paint_anim()) {
        double anim_op;
        guint8 anim_col[4];
        if (ns_anim_get_opacity(ns_paint_anim(), q->box->dom, &anim_op) ||
            ns_anim_get_color(ns_paint_anim(), q->box->dom,
                              NS_CSS_ANIM_TARGET_COLOR, anim_col) ||
            ns_anim_get_color(ns_paint_anim(), q->box->dom,
                              NS_CSS_ANIM_TARGET_BG_COLOR, anim_col))
            cacheable = FALSE;
    }
    cairo_surface_t *tex = NULL;
    if (cacheable && g_paint_3d_tex) {
        quad_tex_entry *e =
            g_hash_table_lookup(g_paint_3d_tex, (gpointer)q->box);
        if (e && e->tw == tw && e->th == th)
            tex = cairo_surface_reference(e->surf);
    }
    if (!tex) {
        tex = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, tw, th);
        if (cairo_surface_status(tex) != CAIRO_STATUS_SUCCESS) {
            cairo_surface_destroy(tex);
            return;
        }
        cairo_t *tcr = cairo_create(tex);
        cairo_scale(tcr, k, k);
        cairo_translate(tcr, -q->bx, -q->by);
        const ns_box *saved_root = g_paint_tex_root;
        const ns_box *saved_flush = g_paint_flush_box;
        g_paint_tex_root = q->box;
        g_paint_flush_box = q->box;
        g_paint_no_cull++;
        if (q->own_only)
            ns_paint_block(tcr, q->box);
        else
            paint_walk(tcr, q->box, highlight);
        g_paint_no_cull--;
        g_paint_tex_root = saved_root;
        g_paint_flush_box = saved_flush;
        cairo_destroy(tcr);
        const ns_style *st = q->box->style;
        const char *filter_kw = st && st->values[NS_CSS_FILTER] &&
            st->values[NS_CSS_FILTER]->kind == NS_CSS_V_KEYWORD
            ? st->values[NS_CSS_FILTER]->u.keyword : NULL;
        if (filter_kw && ns_paint_filter_has_bitmap_effect(filter_kw)) {
            cairo_surface_flush(tex);
            ns_paint_apply_image_filter(cairo_image_surface_get_data(tex),
                               cairo_image_surface_get_stride(tex),
                               tw, th, filter_kw);
            cairo_surface_mark_dirty(tex);
        }
        if (cacheable) {
            if (!g_paint_3d_tex)
                g_paint_3d_tex =
                    g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                          NULL, quad_tex_entry_free);
            quad_tex_entry *e = g_new(quad_tex_entry, 1);
            e->surf = cairo_surface_reference(tex);
            e->tw = tw;
            e->th = th;
            g_hash_table_replace(g_paint_3d_tex, (gpointer)q->box, e);
        }
    }

    double wmin = pws[0], wmax = pws[0];
    for (int i = 1; i < 4; i++) {
        wmin = MIN(wmin, pws[i]);
        wmax = MAX(wmax, pws[i]);
    }
    int n = 1;
    if (clipped) {
        n = 32;
    } else if ((wmax - wmin) / MAX(wmin, 1e-9) > 0.02) {
        double dim = MAX(maxx - minx, maxy - miny);
        n = (int)(dim / 24.0);
        n = CLAMP(n, 2, 32);
    }
    int np = n + 1;
    double *gx = g_new(double, (gsize)np * np * 3);
    double *gy = gx + np * np;
    double *gw = gy + np * np;
    for (int j = 0; j <= n; j++)
        for (int i = 0; i <= n; i++) {
            double ox, oy, oz, ow;
            ns_mat4_apply(&q->m,
                          q->bx + q->bw * i / n,
                          q->by + q->bh * j / n, 0,
                          &ox, &oy, &oz, &ow);
            gw[j * np + i] = ow;
            if (ow < weps) {
                gx[j * np + i] = 0;
                gy[j * np + i] = 0;
            } else {
                gx[j * np + i] = ox / ow;
                gy[j * np + i] = oy / ow;
            }
        }
    cairo_pattern_t *pat = cairo_pattern_create_for_surface(tex);
    cairo_pattern_set_extend(pat, CAIRO_EXTEND_PAD);
    cairo_pattern_set_filter(pat, CAIRO_FILTER_GOOD);
    for (int j = 0; j < n; j++) {
        for (int i = 0; i < n; i++) {
            double cws[4] = { gw[j * np + i], gw[j * np + i + 1],
                              gw[(j + 1) * np + i + 1], gw[(j + 1) * np + i] };
            int cell_behind = 0;
            for (int c2 = 0; c2 < 4; c2++)
                if (cws[c2] < weps) cell_behind++;
            if (cell_behind == 4) continue;
            double sx[8], sy[8], ss[8], st[8];
            int nv;
            if (cell_behind == 0) {
                sx[0] = gx[j * np + i];       sy[0] = gy[j * np + i];
                sx[1] = gx[j * np + i + 1];   sy[1] = gy[j * np + i + 1];
                sx[2] = gx[(j + 1) * np + i + 1];
                sy[2] = gy[(j + 1) * np + i + 1];
                sx[3] = gx[(j + 1) * np + i]; sy[3] = gy[(j + 1) * np + i];
                ss[0] = 0; st[0] = 0; ss[1] = 1; st[1] = 0;
                ss[2] = 1; st[2] = 1; ss[3] = 0; st[3] = 1;
                nv = 4;
            } else {
                const double cs[4] = { 0, 1, 1, 0 };
                const double ct[4] = { 0, 0, 1, 1 };
                nv = 0;
                for (int c2 = 0; c2 < 4; c2++) {
                    int c3 = (c2 + 1) & 3;
                    gboolean in_a = cws[c2] >= weps;
                    gboolean in_b = cws[c3] >= weps;
                    if (in_a) {
                        ss[nv] = cs[c2];
                        st[nv] = ct[c2];
                        nv++;
                    }
                    if (in_a != in_b) {
                        double t = (weps - cws[c2]) / (cws[c3] - cws[c2]);
                        ss[nv] = cs[c2] + (cs[c3] - cs[c2]) * t;
                        st[nv] = ct[c2] + (ct[c3] - ct[c2]) * t;
                        nv++;
                    }
                }
                if (nv < 3) continue;
                for (int v = 0; v < nv; v++) {
                    double ox, oy, oz, ow;
                    ns_mat4_apply(&q->m,
                                  q->bx + q->bw * (i + ss[v]) / n,
                                  q->by + q->bh * (j + st[v]) / n, 0,
                                  &ox, &oy, &oz, &ow);
                    if (ow < weps * 0.5) { nv = 0; break; }
                    sx[v] = ox / ow;
                    sy[v] = oy / ow;
                }
                if (nv < 3) continue;
            }
            double d_s1 = ss[1] - ss[0], d_t1 = st[1] - st[0];
            double d_s2 = ss[2] - ss[0], d_t2 = st[2] - st[0];
            double pdet = d_s1 * d_t2 - d_s2 * d_t1;
            if (fabs(pdet) < 1e-9) continue;
            double ma = ((sx[1] - sx[0]) * d_t2 - (sx[2] - sx[0]) * d_t1) / pdet;
            double mc = ((sx[2] - sx[0]) * d_s1 - (sx[1] - sx[0]) * d_s2) / pdet;
            double mb = ((sy[1] - sy[0]) * d_t2 - (sy[2] - sy[0]) * d_t1) / pdet;
            double md = ((sy[2] - sy[0]) * d_s1 - (sy[1] - sy[0]) * d_s2) / pdet;
            double me = sx[0] - ma * ss[0] - mc * st[0];
            double mf = sy[0] - mb * ss[0] - md * st[0];
            double ccx = 0, ccy = 0;
            for (int v = 0; v < nv; v++) { ccx += sx[v]; ccy += sy[v]; }
            ccx /= nv;
            ccy /= nv;
            double pad = n > 1 ? 0.35 : 0.0;
            cairo_save(cr);
            cairo_new_path(cr);
            for (int v = 0; v < nv; v++) {
                double dx2 = sx[v] - ccx, dy2 = sy[v] - ccy;
                double dlen = sqrt(dx2 * dx2 + dy2 * dy2);
                double ex = sx[v], ey = sy[v];
                if (dlen > 1e-9) {
                    ex += dx2 / dlen * pad;
                    ey += dy2 / dlen * pad;
                }
                if (v == 0) cairo_move_to(cr, ex, ey);
                else        cairo_line_to(cr, ex, ey);
            }
            cairo_close_path(cr);
            cairo_clip(cr);
            cairo_matrix_t cm2;
            cairo_matrix_init(&cm2, ma, mb, mc, md, me, mf);
            cairo_transform(cr, &cm2);
            cairo_matrix_t pm2;
            cairo_matrix_init(&pm2, (double)tw / n, 0, 0, (double)th / n,
                              (double)tw * i / n, (double)th * j / n);
            cairo_pattern_set_matrix(pat, &pm2);
            cairo_set_source(cr, pat);
            cairo_rectangle(cr, -0.5, -0.5, 2.0, 2.0);
            cairo_fill(cr);
            cairo_restore(cr);
        }
    }
    cairo_pattern_destroy(pat);
    cairo_surface_destroy(tex);
    g_free(gx);
}

static int
quad3_cmp(gconstpointer pa, gconstpointer pb)
{
    const ns_quad3 *a = pa, *b = pb;
    if (a->depth < b->depth) return -1;
    if (a->depth > b->depth) return 1;
    if (a->seq < b->seq) return -1;
    if (a->seq > b->seq) return 1;
    return 0;
}

static GHashTable *g_paint_3d_reg;

static void
paint_3d_reg_value_free(gpointer v)
{
    g_array_free(v, TRUE);
}

void
ns_paint_3d_invalidate(void)
{
    if (g_paint_3d_reg) g_hash_table_remove_all(g_paint_3d_reg);
    if (g_paint_3d_tex) g_hash_table_remove_all(g_paint_3d_tex);
}

gboolean
ns_paint_3d_registered(const ns_box *b)
{
    if (g_paint_3d_reg && g_hash_table_contains(g_paint_3d_reg, (gpointer)b))
        return TRUE;
    return box_establishes_3d(b);
}

static void
paint_3d_register(const ns_box *root, const GArray *quads)
{
    if (!g_paint_3d_reg)
        g_paint_3d_reg = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                               NULL, paint_3d_reg_value_free);
    GArray *snap = g_array_sized_new(FALSE, FALSE, sizeof(ns_quad3),
                                     quads->len);
    g_array_append_vals(snap, quads->data, quads->len);
    g_hash_table_replace(g_paint_3d_reg, (gpointer)root, snap);
}

static gboolean
quad3_unproject(const ns_quad3 *q, double sx, double sy,
                double *u_out, double *v_out, double *depth_out)
{
    const double *m = q->m.m;
    double a = m[0] * q->bw, b = m[1] * q->bh;
    double c = m[0] * q->bx + m[1] * q->by + m[3];
    double d = m[4] * q->bw, e = m[5] * q->bh;
    double f = m[4] * q->bx + m[5] * q->by + m[7];
    double g = m[12] * q->bw, h = m[13] * q->bh;
    double k = m[12] * q->bx + m[13] * q->by + m[15];
    double i0 = e * k - f * h;
    double i1 = c * h - b * k;
    double i2 = b * f - c * e;
    double i3 = f * g - d * k;
    double i4 = a * k - c * g;
    double i5 = c * d - a * f;
    double i6 = d * h - e * g;
    double i7 = b * g - a * h;
    double i8 = a * e - b * d;
    double det = a * i0 + b * i3 + c * i6;
    if (fabs(det) < 1e-12) return FALSE;
    double tu = i0 * sx + i1 * sy + i2;
    double tv = i3 * sx + i4 * sy + i5;
    double tw = i6 * sx + i7 * sy + i8;
    if (fabs(tw) < 1e-12) return FALSE;
    double u = tu / tw, v = tv / tw;
    if (u < -0.002 || u > 1.002 || v < -0.002 || v > 1.002) return FALSE;
    double px = q->bx + u * q->bw, py = q->by + v * q->bh;
    double w = m[12] * px + m[13] * py + m[15];
    if (w < 1e-6) return FALSE;
    double z = m[8] * px + m[9] * py + m[11];
    *u_out = u;
    *v_out = v;
    *depth_out = z / w;
    return TRUE;
}

typedef struct quad_pick_cand {
    guint idx;
    double depth, u, v;
} quad_pick_cand;

static int
quad_pick_cand_cmp(gconstpointer pa, gconstpointer pb)
{
    const quad_pick_cand *a = pa, *b = pb;
    if (a->depth > b->depth) return -1;
    if (a->depth < b->depth) return 1;
    if (a->idx > b->idx) return -1;
    if (a->idx < b->idx) return 1;
    return 0;
}

static GArray *collect_root_quads(const ns_box *b);

const ns_box *
ns_paint_3d_pick(const ns_box *root3d, double x, double y)
{
    GArray *quads = g_paint_3d_reg
        ? g_hash_table_lookup(g_paint_3d_reg, (gpointer)root3d) : NULL;
    if (!quads) {
        GArray *fresh = collect_root_quads(root3d);
        paint_3d_register(root3d, fresh);
        g_array_free(fresh, TRUE);
        quads = g_hash_table_lookup(g_paint_3d_reg, (gpointer)root3d);
        if (!quads) return NULL;
    }
    GArray *cands = g_array_new(FALSE, FALSE, sizeof(quad_pick_cand));
    for (guint i = 0; i < quads->len; i++) {
        const ns_quad3 *q = &g_array_index(quads, ns_quad3, i);
        quad_pick_cand c = { i, 0, 0, 0 };
        if (quad3_unproject(q, x, y, &c.u, &c.v, &c.depth))
            g_array_append_val(cands, c);
    }
    g_array_sort(cands, quad_pick_cand_cmp);
    const ns_box *result = NULL;
    for (guint i = 0; i < cands->len && !result; i++) {
        const quad_pick_cand *c = &g_array_index(cands, quad_pick_cand, i);
        const ns_quad3 *q = &g_array_index(quads, ns_quad3, c->idx);
        double lx = q->bx + c->u * q->bw;
        double ly = q->by + c->v * q->bh;
        result = ns_box_hit_test(q->box, lx, ly);
    }
    g_array_free(cands, TRUE);
    return result;
}

static GArray *
collect_root_quads(const ns_box *b)
{
    GArray *quads = g_array_new(FALSE, FALSE, sizeof(ns_quad3));
    ns_mat4 root;
    ns_mat4_identity(&root);
    if (box_establishes_3d(b)) {
        double d = box_perspective_px(b);
        if (d > 0) {
            double bx, by, bw, bh;
            box_border_rect(b, &bx, &by, &bw, &bh);
            double pox, poy, poz;
            box_transform_origin(b, bx, by, bw, bh,
                                 NS_CSS_PERSPECTIVE_ORIGIN,
                                 &pox, &poy, &poz);
            ns_mat4_translate(&root, pox, poy, 0);
            ns_mat4_perspective(&root, d);
            ns_mat4_translate(&root, -pox, -poy, 0);
        }
        for (const ns_box *c = b->first_child; c; c = c->next_sibling)
            collect_3d_quads(c, &root, quads);
    } else {
        collect_3d_quads(b, &root, quads);
    }
    for (guint i = 0; i < quads->len; i++) {
        ns_quad3 *q = &g_array_index(quads, ns_quad3, i);
        double ox, oy, oz, ow;
        ns_mat4_apply(&q->m, q->bx + q->bw / 2.0, q->by + q->bh / 2.0, 0,
                      &ox, &oy, &oz, &ow);
        if (ow > 1e-6) {
            q->depth = oz / ow;
        } else {
            q->depth = -1e30;
            const double cxs[4] = { q->bx, q->bx + q->bw,
                                    q->bx + q->bw, q->bx };
            const double cys[4] = { q->by, q->by,
                                    q->by + q->bh, q->by + q->bh };
            for (int c = 0; c < 4; c++) {
                ns_mat4_apply(&q->m, cxs[c], cys[c], 0, &ox, &oy, &oz, &ow);
                if (ow > 1e-6 && oz / ow > q->depth) q->depth = oz / ow;
            }
        }
    }
    g_array_sort(quads, quad3_cmp);
    return quads;
}

static void
paint_3d_root(cairo_t *cr, const ns_box *b, const char *highlight)
{
    if (box_establishes_3d(b) &&
        (b->kind == NS_BOX_BLOCK || b->kind == NS_BOX_TABLE ||
         b->kind == NS_BOX_TABLE_CAPTION || b->kind == NS_BOX_TABLE_CELL))
        ns_paint_block(cr, b);
    GArray *quads = collect_root_quads(b);
    paint_3d_register(b, quads);
    if (g_getenv("NS_3D_DEBUG")) {
        fprintf(stderr, "3d root <%s class=%s> %u quads\n",
                b->dom && b->dom->name ? b->dom->name : "?",
                b->dom ? (ns_element_get_attr(b->dom, "class") ?: "") : "",
                quads->len);
        for (guint i = 0; i < quads->len; i++) {
            const ns_quad3 *q = &g_array_index(quads, ns_quad3, i);
            double cxs[4] = { q->bx, q->bx + q->bw, q->bx + q->bw, q->bx };
            double cys[4] = { q->by, q->by, q->by + q->bh, q->by + q->bh };
            fprintf(stderr, "  quad <%s class=%s> rect %.0f,%.0f %gx%g depth %.2f corners",
                    q->box->dom && q->box->dom->name ? q->box->dom->name : "?",
                    q->box->dom ? (ns_element_get_attr(q->box->dom, "class") ?: "") : "",
                    q->bx, q->by, q->bw, q->bh, q->depth);
            for (int c = 0; c < 4; c++) {
                double ox, oy, oz, ow;
                ns_mat4_apply(&q->m, cxs[c], cys[c], 0, &ox, &oy, &oz, &ow);
                if (ow > 1e-6)
                    fprintf(stderr, " (%.1f,%.1f)", ox / ow, oy / ow);
                else
                    fprintf(stderr, " (w=%.3f!)", ow);
            }
            fprintf(stderr, "\n");
        }
    }
    g_paint_no_cull++;
    for (guint i = 0; i < quads->len; i++) {
        double w0 = 0, w1 = 0, w2 = 0, w3 = 0;
        if (g_dbg_paint_x >= 0)
            cairo_clip_extents(cr, &w0, &w1, &w2, &w3);
        paint_quad3(cr, &g_array_index(quads, ns_quad3, i), highlight);
        if (g_dbg_paint_x >= 0) {
            double v0, v1, v2, v3;
            cairo_clip_extents(cr, &v0, &v1, &v2, &v3);
            if (fabs(v0 - w0) > 0.5 || fabs(v1 - w1) > 0.5 ||
                fabs(v2 - w2) > 0.5 || fabs(v3 - w3) > 0.5) {
                const ns_quad3 *q = &g_array_index(quads, ns_quad3, i);
                g_printerr("[quad3-LEAK] <%s class=%s> rect %.0f,%.0f "
                           "%gx%g\n",
                           q->box->dom && q->box->dom->name
                               ? q->box->dom->name : "?",
                           q->box->dom
                               ? (ns_element_get_attr(q->box->dom, "class")
                                      ?: "")
                               : "",
                           q->bx, q->by, q->bw, q->bh);
            }
        }
    }
    g_paint_no_cull--;
    g_array_free(quads, TRUE);
}

static void
ns_dbg_paint_probe(cairo_t *cr, const ns_box *b)
{
    if (g_dbg_paint_x == -2) {
        const char *s = g_getenv("NS_DBG_PAINT_AT");
        g_dbg_paint_x = g_dbg_paint_y = -1;
        if (s) sscanf(s, "%d,%d", &g_dbg_paint_x, &g_dbg_paint_y);
    }
    if (g_dbg_paint_x < 0) return;
    if (isnan(b->x) || isnan(b->y) ||
        isnan(b->content_width) || isnan(b->content_height)) {
        GString *ch = g_string_new("[paint-NAN]");
        for (const ns_box *p2 = b; p2; p2 = p2->parent) {
            const char *nm = p2->dom && p2->dom->name ? p2->dom->name : "?";
            const char *id = p2->dom && p2->dom->kind == NS_NODE_ELEMENT
                           ? ns_element_get_attr(p2->dom, "id") : NULL;
            g_string_append_printf(ch, " <%s#%s%s>", nm, id ? id : "",
                                   isnan(p2->x) ? " NAN" : "");
        }
        g_printerr("%s\n", ch->str);
        g_string_free(ch, TRUE);
    }
    if (b->dom && b->dom->name &&
        strcmp(b->dom->name, "ytd-watch-metadata") == 0) {
        GString *chain = g_string_new("[paint-chain]");
        for (const ns_box *p2 = b; p2; p2 = p2->parent) {
            const char *nm = p2->dom && p2->dom->name ? p2->dom->name : "?";
            const char *id = p2->dom && p2->dom->kind == NS_NODE_ELEMENT
                           ? ns_element_get_attr(p2->dom, "id") : NULL;
            g_string_append_printf(chain, " <%s#%s y=%.0f h=%.0f>",
                                   nm, id ? id : "", p2->y,
                                   p2->content_height);
        }
        g_printerr("%s\n", chain->str);
        g_string_free(chain, TRUE);
    }
    double x0 = b->x, y0 = b->y;
    double x1 = b->x + b->content_width, y1 = b->y + b->content_height;
    cairo_user_to_device(cr, &x0, &y0);
    cairo_user_to_device(cr, &x1, &y1);
    if (isnan(x0) && !isnan(b->x)) {
        GString *ch = g_string_new("[paint-CTM-NAN]");
        for (const ns_box *p2 = b; p2; p2 = p2->parent) {
            const char *nm = p2->dom && p2->dom->name ? p2->dom->name : "?";
            const char *id = p2->dom && p2->dom->kind == NS_NODE_ELEMENT
                           ? ns_element_get_attr(p2->dom, "id") : NULL;
            g_string_append_printf(ch, " <%s#%s sx=%.0f sy=%.0f>",
                                   nm, id ? id : "",
                                   p2->scroll_x, p2->scroll_y);
        }
        g_printerr("%s\n", ch->str);
        g_string_free(ch, TRUE);
    }
    if (g_dbg_paint_x < x0 || g_dbg_paint_x > x1 ||
        g_dbg_paint_y < y0 || g_dbg_paint_y > y1)
        return;
    const ns_css_value *bgv =
        b->style ? b->style->values[NS_CSS_BACKGROUND_COLOR] : NULL;
    char bg[64] = "-";
    if (bgv && bgv->kind == NS_CSS_V_COLOR)
        g_snprintf(bg, sizeof bg, "rgba(%d,%d,%d,%d)",
                   (int)bgv->u.color.r, (int)bgv->u.color.g,
                   (int)bgv->u.color.b, (int)bgv->u.color.a);
    double kx0, ky0, kx1, ky1;
    cairo_clip_extents(cr, &kx0, &ky0, &kx1, &ky1);
    g_printerr("[paint-at] <%s> %.0f,%.0f %.0fx%.0f bg=%s clip=%.0f,%.0f..%.0f,%.0f\n",
               b->dom && b->dom->name ? b->dom->name : "?",
               x0, y0, x1 - x0, y1 - y0, bg, kx0, ky0, kx1, ky1);
}

static void
paint_walk(cairo_t *cr, const ns_box *b, const char *highlight)
{
    if (!b) return;
    if (g_paint_collect_stats) g_paint_stats.boxes_seen++;
    if (box_is_hidden(b)) {
        if (g_paint_collect_stats) g_paint_stats.hidden++;
        return;
    }
    ns_dbg_paint_probe(cr, b);
    if (box_clip_hides(b)) {
        if (g_paint_collect_stats) g_paint_stats.hidden++;
        return;
    }
    if (b == g_paint_skip_box) {
        if (g_paint_collect_stats) g_paint_stats.skipped_top++;
        return;
    }
    if (g_paint_defer_depth > 0 && b != g_paint_flush_box &&
        box_defers_to_positioned_layer(b)) {
        if (!g_paint_deferred_list)
            g_paint_deferred_list = g_ptr_array_new_with_free_func(g_free);
        deferred_capture *cap = g_new0(deferred_capture, 1);
        cap->box = b;
        cap->seq = g_paint_capture_seq++;
        cairo_user_to_device(cr, &cap->dev_x, &cap->dev_y);
        g_ptr_array_add(g_paint_deferred_list, cap);
        if (g_dbg_paint_x >= 0 && b->dom && b->dom->name)
            g_printerr("[paint-defer] <%s#%s> y=%.0f h=%.0f\n",
                       b->dom->name,
                       ns_element_get_attr(b->dom, "id")
                           ? ns_element_get_attr(b->dom, "id") : "",
                       b->y, b->content_height);
        return;
    }
    if (!g_paint_no_cull && g_paint_have_clip &&
        b->paint_bottom > b->paint_top) {
        double top = b->paint_top + g_paint_anchor_dy;
        double bottom = b->paint_bottom + g_paint_anchor_dy;
        if (bottom < g_paint_clip_y0 - g_paint_cull_margin ||
            top > g_paint_clip_y1 + g_paint_cull_margin) {
            if (g_paint_collect_stats) g_paint_stats.culled_bounds++;
            return;
        }
    }
    double dbg_e0 = 0, dbg_e1 = 0, dbg_e2 = 0, dbg_e3 = 0;
    if (g_dbg_paint_x >= 0)
        cairo_clip_extents(cr, &dbg_e0, &dbg_e1, &dbg_e2, &dbg_e3);
    const ns_style *style = b->style;
    gboolean skip_contents = box_skips_contents(b);
    double op = box_opacity(b);
    cairo_operator_t blend = blend_mode_operator(style);
    gboolean mask_grad = ns_paint_mask_layers_paintable(style);
    gboolean grouped = op < 0.999 || blend != CAIRO_OPERATOR_OVER || mask_grad;
    double sticky_dx = 0, sticky_dy = 0;
    compute_sticky_offset(b, cr, &sticky_dx, &sticky_dy);
    if (isnan(sticky_dx) || isnan(sticky_dy)) {
        if (g_dbg_paint_x >= 0)
            g_printerr("[paint-nan-guard] sticky <%s>\n",
                       b->dom && b->dom->name ? b->dom->name : "?");
        sticky_dx = sticky_dy = 0;
    }
    gboolean has_sticky = (sticky_dx != 0 || sticky_dy != 0);
    double saved_anchor_dx = g_paint_anchor_dx;
    double saved_anchor_dy = g_paint_anchor_dy;
    if (has_sticky) {
        cairo_save(cr);
        cairo_translate(cr, sticky_dx, sticky_dy);
        g_paint_anchor_dx += sticky_dx;
        g_paint_anchor_dy += sticky_dy;
    }
    const ns_css_transform *anim_tf =
        ns_paint_anim() ? ns_anim_get_transform(ns_paint_anim(), b->dom) : NULL;
    ns_css_transform eff_tf;
    eff_tf.n_ops = 0;
    if (anim_tf ||
        (style && (style->values[NS_CSS_TRANSFORM] ||
                   style->values[NS_CSS_TRANSLATE] ||
                   style->values[NS_CSS_ROTATE] ||
                   style->values[NS_CSS_SCALE])))
        ns_css_style_effective_transform(style, anim_tf, &eff_tf);
    gboolean has_transform = eff_tf.n_ops > 0;
    if (b == g_paint_tex_root) {
        has_transform = FALSE;
    } else if (box_establishes_3d(b) ||
               (has_transform && ns_css_transform_is_3d(&eff_tf))) {
        double u0 = 0, u1 = 0, u2 = 0, u3 = 0;
        if (g_dbg_paint_x >= 0)
            cairo_clip_extents(cr, &u0, &u1, &u2, &u3);
        paint_3d_root(cr, b, highlight);
        if (g_dbg_paint_x >= 0) {
            double z0, z1, z2, z3;
            cairo_clip_extents(cr, &z0, &z1, &z2, &z3);
            if (fabs(z0 - u0) > 0.5 || fabs(z1 - u1) > 0.5 ||
                fabs(z2 - u2) > 0.5 || fabs(z3 - u3) > 0.5)
                g_printerr("[3droot-LEAK] <%s class=%.60s>\n",
                           b->dom && b->dom->name ? b->dom->name : "?",
                           b->dom ? (ns_element_get_attr(b->dom, "class")
                                         ?: "")
                                  : "");
        }
        if (has_sticky) paint_anchor_leave(cr, saved_anchor_dx, saved_anchor_dy);
        return;
    }

    gboolean box_offscreen = FALSE;
    if (!has_transform && !has_sticky && !g_paint_no_cull) {
        const ns_css_value *posv = style ? style->values[NS_CSS_POSITION] : NULL;
        gboolean is_fixed = posv && posv->kind == NS_CSS_V_KEYWORD &&
                            posv->u.keyword && strcmp(posv->u.keyword, "fixed") == 0;
        if (!is_fixed) {
            double by = b->y + b->margin.top + g_paint_anchor_dy;
            double bh = b->content_height + b->padding.top + b->padding.bottom +
                        b->border.top + b->border.bottom;
            if (g_paint_have_clip &&
                (by + bh < g_paint_clip_y0 - g_paint_cull_margin ||
                 by > g_paint_clip_y1 + g_paint_cull_margin))
                box_offscreen = TRUE;
        }
    }
    if (box_offscreen && g_paint_collect_stats) g_paint_stats.offscreen++;

    guint first_group_hole = g_paint_video_holes
        ? g_paint_video_holes->len : 0;
    if (grouped) {
        if (g_paint_collect_stats) g_paint_stats.grouped++;
        cairo_push_group(cr);
    }
    if (has_transform) {
        cairo_save(cr);
        double bx, by, bw, bh;
        box_border_rect(b, &bx, &by, &bw, &bh);
        double ox, oy, oz;
        box_transform_origin(b, bx, by, bw, bh, NS_CSS_TRANSFORM_ORIGIN,
                             &ox, &oy, &oz);
        ns_mat4 m;
        ns_css_transform_to_mat4(&eff_tf, bw, bh, &m);
        cairo_matrix_t cm;
        cairo_matrix_init(&cm, m.m[0], m.m[4], m.m[1], m.m[5],
                          m.m[3], m.m[7]);
        if (isnan(m.m[0]) || isnan(m.m[4]) || isnan(m.m[1]) ||
            isnan(m.m[5]) || isnan(m.m[3]) || isnan(m.m[7]) ||
            isnan(ox) || isnan(oy)) {
            if (g_dbg_paint_x >= 0)
                g_printerr("[paint-nan-guard] transform <%s>\n",
                           b->dom && b->dom->name ? b->dom->name : "?");
        } else if (fabs(m.m[0] * m.m[5] - m.m[1] * m.m[4]) < 1e-6) {
            cairo_restore(cr);
            if (grouped)
                cairo_pattern_destroy(cairo_pop_group(cr));
            if (has_sticky) paint_anchor_leave(cr, saved_anchor_dx, saved_anchor_dy);
            return;
        } else {
            cairo_translate(cr, ox, oy);
            cairo_transform(cr, &cm);
            cairo_translate(cr, -ox, -oy);
        }
    }
    gboolean has_path_clip = FALSE;
    if ((b->kind == NS_BOX_BLOCK || b->kind == NS_BOX_TABLE ||
         b->kind == NS_BOX_TABLE_CAPTION || b->kind == NS_BOX_TABLE_CELL) &&
        style && style->values[NS_CSS_CLIP_PATH] &&
        style->values[NS_CSS_CLIP_PATH]->kind == NS_CSS_V_KEYWORD &&
        style->values[NS_CSS_CLIP_PATH]->u.keyword &&
        strcmp(style->values[NS_CSS_CLIP_PATH]->u.keyword, "none") != 0) {
        cairo_save(cr);
        if (ns_paint_box_content_clip(cr, b)) has_path_clip = TRUE;
        else                                cairo_restore(cr);
    }
    if (!box_offscreen) {
        double s0 = 0, s1 = 0, s2 = 0, s3 = 0;
        if (g_dbg_paint_x >= 0)
            cairo_clip_extents(cr, &s0, &s1, &s2, &s3);
        if (b->kind == NS_BOX_BLOCK || b->kind == NS_BOX_TABLE ||
            b->kind == NS_BOX_TABLE_CAPTION ||
            b->kind == NS_BOX_TABLE_ROW || b->kind == NS_BOX_TABLE_CELL ||
            b->kind == NS_BOX_IMAGE || b->kind == NS_BOX_VIDEO ||
            b->kind == NS_BOX_MATH || b->kind == NS_BOX_SVG) {
            if (g_paint_collect_stats) g_paint_stats.blocks++;
            ns_paint_block(cr, b);
            if (g_dbg_paint_x >= 0) {
                double t0, t1, t2, t3;
                cairo_clip_extents(cr, &t0, &t1, &t2, &t3);
                if (fabs(t0 - s0) > 0.5 || fabs(t1 - s1) > 0.5 ||
                    fabs(t2 - s2) > 0.5 || fabs(t3 - s3) > 0.5)
                    g_printerr("[block-LEAK] <%s class=%.60s>\n",
                               b->dom && b->dom->name ? b->dom->name : "?",
                               b->dom
                                   ? (ns_element_get_attr(b->dom, "class")
                                          ?: "")
                                   : "");
            }
        }
        if (b->kind == NS_BOX_BLOCK) {
            ns_paint_marker(cr, b);
            ns_paint_hr(cr, b);
        }
        if (b->kind == NS_BOX_INLINE && !skip_contents) {
            if (g_paint_collect_stats) g_paint_stats.inlines++;
            ns_paint_inline(cr, b, highlight);
            if (g_dbg_paint_x >= 0) {
                double t0, t1, t2, t3;
                cairo_clip_extents(cr, &t0, &t1, &t2, &t3);
                if (fabs(t0 - s0) > 0.5 || fabs(t1 - s1) > 0.5 ||
                    fabs(t2 - s2) > 0.5 || fabs(t3 - s3) > 0.5)
                    g_printerr("[inline-LEAK] <%s> text=%.30s\n",
                               b->dom && b->dom->name ? b->dom->name : "?",
                               b->text ? b->text : "");
            }
        }
        if (b->kind == NS_BOX_IMAGE && !skip_contents) {
            if (g_paint_collect_stats) g_paint_stats.images++;
            ns_paint_image(cr, b);
        }
        if (b->kind == NS_BOX_VIDEO && !skip_contents) {
            if (g_paint_collect_stats) g_paint_stats.videos++;
            ns_paint_video(cr, b);
        }
        if (b->kind == NS_BOX_MATH && !skip_contents)
            ns_paint_math(cr, b);
        if (b->kind == NS_BOX_SVG)
            ns_paint_svg(cr, b);
    }
    if (!skip_contents && ns_node_is_element_named(b->dom, "canvas") &&
        ns_paint_js()) {
        if (g_paint_collect_stats) g_paint_stats.canvases++;
        cairo_surface_t *surf = ns_js_canvas_surface(ns_paint_js(), b->dom);
        if (surf) {
            int sw = cairo_image_surface_get_width(surf);
            int sh = cairo_image_surface_get_height(surf);
            if (sw > 0 && sh > 0) {
                double dx = b->x + b->margin.left + b->border.left + b->padding.left;
                double dy = b->y + b->margin.top  + b->border.top  + b->padding.top;
                double dw = b->content_width > 0 ? b->content_width : sw;
                double dh = b->content_height > 0 ? b->content_height : sh;
                cairo_save(cr);
                cairo_translate(cr, dx, dy);
                cairo_scale(cr, dw / sw, dh / sh);
                cairo_set_source_surface(cr, surf, 0, 0);
                cairo_paint(cr);
                cairo_restore(cr);
            }
        }
    }

    guint n_children = 0;
    if (!skip_contents)
        for (const ns_box *c = b->first_child; c; c = c->next_sibling)
            n_children++;
    paint_entry entries_buf[64];
    paint_entry *entries = n_children <= G_N_ELEMENTS(entries_buf)
        ? entries_buf : g_new(paint_entry, n_children);
    guint order = 0;
    gboolean any_z = FALSE;
    if (!skip_contents) {
        for (const ns_box *c = b->first_child; c; c = c->next_sibling) {
            paint_entry e;
            e.box = c;
            e.order = order++;
            if (box_is_positioned(c)) {
                e.key = box_z_index(c);
                if (e.key != 0) any_z = TRUE;
            } else {
                e.key = 0;
            }
            entries[e.order] = e;
        }
    }
    if (any_z) {
        if (g_paint_collect_stats) {
            g_paint_stats.sorted_parents++;
            g_paint_stats.sorted_children += n_children;
        }
        qsort(entries, n_children, sizeof(paint_entry), paint_entry_cmp);
    }
    const char *ovx = b->style ? ns_style_keyword(b->style, NS_CSS_OVERFLOW_X) : NULL;
    const char *ovy = b->style ? ns_style_keyword(b->style, NS_CSS_OVERFLOW_Y) : NULL;
    const char *ovs = b->style ? ns_style_keyword(b->style, NS_CSS_OVERFLOW) : NULL;
    if (!ovx) ovx = ovs;
    if (!ovy) ovy = ovs;
    gboolean is_root = (b->parent == NULL) ||
                       (b->dom && b->dom->name &&
                        (strcmp(b->dom->name, "html") == 0 ||
                         strcmp(b->dom->name, "body") == 0));
    gboolean clip_overflow = !is_root &&
                             (overflow_kw_clips(ovx) || overflow_kw_clips(ovy));
    if (clip_overflow &&
        (b->kind == NS_BOX_BLOCK || b->kind == NS_BOX_TABLE_CAPTION ||
         b->kind == NS_BOX_TABLE_CELL)) {
        double px = b->x + b->margin.left + b->border.left;
        double py = b->y + b->margin.top  + b->border.top;
        double pw = b->content_width + b->padding.left + b->padding.right;
        double ph = b->content_height + b->padding.top + b->padding.bottom;
        if (pw < 0) pw = 0;
        if (ph < 0) ph = 0;
        if (isnan(px) || isnan(py) || isnan(pw) || isnan(ph)) {
            if (g_dbg_paint_x >= 0)
                g_printerr("[paint-nan-guard] overflow-clip <%s>\n",
                           b->dom && b->dom->name ? b->dom->name : "?");
            px = py = 0; pw = ph = 0;
        }
        const ns_style *bs = b->style;
        gboolean explicit_h = bs &&
            ((bs->values[NS_CSS_MAX_HEIGHT] &&
              (bs->values[NS_CSS_MAX_HEIGHT]->kind == NS_CSS_V_LENGTH ||
               bs->values[NS_CSS_MAX_HEIGHT]->kind == NS_CSS_V_CALC)) ||
             (bs->values[NS_CSS_HEIGHT] &&
              (bs->values[NS_CSS_HEIGHT]->kind == NS_CSS_V_LENGTH ||
               bs->values[NS_CSS_HEIGHT]->kind == NS_CSS_V_CALC)));
        gboolean explicit_w = bs &&
            ((bs->values[NS_CSS_MAX_WIDTH] &&
              (bs->values[NS_CSS_MAX_WIDTH]->kind == NS_CSS_V_LENGTH ||
               bs->values[NS_CSS_MAX_WIDTH]->kind == NS_CSS_V_CALC)) ||
             (bs->values[NS_CSS_WIDTH] &&
              (bs->values[NS_CSS_WIDTH]->kind == NS_CSS_V_LENGTH ||
               bs->values[NS_CSS_WIDTH]->kind == NS_CSS_V_CALC)));
        gboolean sized_by_container = box_is_flex_or_grid_item(b);
        if ((pw > 0 || explicit_w || sized_by_container) &&
            (ph > 0 || explicit_h || sized_by_container)) {
            cairo_save(cr);
            ns_paint_box_radii_path(cr, b, px, py, pw, ph);
            cairo_clip(cr);
            if (g_dbg_paint_x >= 0) {
                double ex0, ey0, ex1, ey1;
                cairo_clip_extents(cr, &ex0, &ey0, &ex1, &ey1);
                g_printerr("[paint-clip%s] <%s#%s> rect %.0f,%.0f %.0fx%.0f"
                           " -> clip %.0f,%.0f..%.0f,%.0f\n",
                           (ey1 - ey0 < 1 || ex1 - ex0 < 1) ? "-EMPTY" : "",
                           b->dom && b->dom->name ? b->dom->name : "?",
                           b->dom ? (ns_element_get_attr(b->dom, "id")
                                     ? ns_element_get_attr(b->dom, "id") : "")
                                  : "",
                           px, py, pw, ph, ex0, ey0, ex1, ey1);
            }
            if ((b->scroll_x != 0 || b->scroll_y != 0) &&
                !isnan(b->scroll_x) && !isnan(b->scroll_y))
                cairo_translate(cr, -b->scroll_x, -b->scroll_y);
            if (g_paint_collect_stats) g_paint_stats.overflow_clips++;
        } else {
            clip_overflow = FALSE;
        }
    } else {
        clip_overflow = FALSE;
    }
    gboolean own_layer_scope = b->parent == NULL || grouped || has_transform ||
                               clip_overflow || has_path_clip ||
                               b == g_paint_tex_root ||
                               (b == g_paint_flush_box &&
                                box_isolates_positioned_descendants(b));
    GPtrArray *saved_layer_list = NULL;
    if (own_layer_scope) {
        saved_layer_list = g_paint_deferred_list;
        g_paint_deferred_list = NULL;
        g_paint_defer_depth++;
    }
    if (has_transform || has_sticky) g_paint_no_cull++;
    for (guint i = 0; i < n_children; i++)
        paint_walk(cr, entries[i].box, highlight);
    if (has_transform || has_sticky) g_paint_no_cull--;
    GPtrArray *deferred_mine = NULL;
    if (own_layer_scope) {
        deferred_mine = g_paint_deferred_list;
        g_paint_deferred_list = saved_layer_list;
        g_paint_defer_depth--;
        if (deferred_mine) {
            if (g_dbg_paint_x >= 0) {
                double fx0, fy0, fx1, fy1;
                cairo_clip_extents(cr, &fx0, &fy0, &fx1, &fy1);
                g_printerr("[paint-flush] owner=<%s#%s> n=%u "
                           "clip=%.0f,%.0f..%.0f,%.0f\n",
                           b->dom && b->dom->name ? b->dom->name : "?",
                           b->dom && ns_element_get_attr(b->dom, "id")
                               ? ns_element_get_attr(b->dom, "id") : "",
                           deferred_mine->len, fx0, fy0, fx1, fy1);
            }
            if (has_transform || has_sticky) g_paint_no_cull++;
            g_layers.flush_layered =
                b == g_layers.root ||
                (b == g_layers.owner && !grouped && !has_transform &&
                 !clip_overflow && !has_path_clip);
            paint_flush_deferred(cr, deferred_mine, highlight);
            if (has_transform || has_sticky) g_paint_no_cull--;
            g_ptr_array_free(deferred_mine, TRUE);
            deferred_mine = NULL;
        }
    }
    const char *sbw_kw = b->style && b->style->values[NS_CSS_SCROLLBAR_WIDTH] &&
        b->style->values[NS_CSS_SCROLLBAR_WIDTH]->kind == NS_CSS_V_KEYWORD
        ? b->style->values[NS_CSS_SCROLLBAR_WIDTH]->u.keyword : NULL;
    gboolean sb_hidden = sbw_kw && strcmp(sbw_kw, "none") == 0;
    double sb_size = (sbw_kw && strcmp(sbw_kw, "thin") == 0) ? 5.0 : 8.0;
    double th_r = 0, th_g = 0, th_b = 0, th_a = 0.40;
    double tk_r = 0, tk_g = 0, tk_b = 0, tk_a = 0.06;
    const char *sbc_kw = b->style && b->style->values[NS_CSS_SCROLLBAR_COLOR] &&
        b->style->values[NS_CSS_SCROLLBAR_COLOR]->kind == NS_CSS_V_KEYWORD
        ? b->style->values[NS_CSS_SCROLLBAR_COLOR]->u.keyword : NULL;
    if (sbc_kw && g_ascii_strcasecmp(sbc_kw, "auto") != 0) {
        char **ct = g_strsplit_set(sbc_kw, " \t", -1);
        int idx = 0;
        for (int i = 0; ct[i] && idx < 2; i++) {
            char *t = g_strstrip(ct[i]);
            guint8 r, g, bb, a;
            if (*t && ns_css_parse_color(t, &r, &g, &bb, &a)) {
                if (idx == 0) { th_r = r/255.0; th_g = g/255.0; th_b = bb/255.0; th_a = a/255.0; }
                else          { tk_r = r/255.0; tk_g = g/255.0; tk_b = bb/255.0; tk_a = a/255.0; }
                idx++;
            }
        }
        g_strfreev(ct);
    }
    if (clip_overflow && b->scrolls && !sb_hidden &&
        (b->scroll_max_x > 0 || b->scroll_max_y > 0)) {
        double px = b->x + b->margin.left + b->border.left;
        double py = b->y + b->margin.top  + b->border.top;
        double pw = b->content_width + b->padding.left + b->padding.right;
        double ph = b->content_height + b->padding.top + b->padding.bottom;
        if (b->scroll_x != 0 || b->scroll_y != 0)
            cairo_translate(cr, b->scroll_x, b->scroll_y);
        if (b->scroll_max_y > 0 && ph > 16) {
            double track_w = sb_size;
            double track_x = px + pw - track_w - 1.0;
            double track_y = py + 1.0;
            double track_h = ph - 2.0;
            double total_h = ph + b->scroll_max_y;
            double thumb_h = track_h * (ph / total_h);
            if (thumb_h < 16.0) thumb_h = 16.0;
            if (thumb_h > track_h) thumb_h = track_h;
            double thumb_y = track_y +
                (track_h - thumb_h) * (b->scroll_y / b->scroll_max_y);
            cairo_save(cr);
            cairo_set_source_rgba(cr, tk_r, tk_g, tk_b, tk_a);
            cairo_rectangle(cr, track_x, track_y, track_w, track_h);
            cairo_fill(cr);
            cairo_set_source_rgba(cr, th_r, th_g, th_b, th_a);
            cairo_rectangle(cr, track_x + 1, thumb_y, track_w - 2, thumb_h);
            cairo_fill(cr);
            cairo_restore(cr);
        }
        if (b->scroll_max_x > 0 && pw > 16) {
            double track_h = sb_size;
            double track_x = px + 1.0;
            double track_y = py + ph - track_h - 1.0;
            double track_w = pw - 2.0 -
                (b->scroll_max_y > 0 ? sb_size : 0.0);
            double total_w = pw + b->scroll_max_x;
            double thumb_w = track_w * (pw / total_w);
            if (thumb_w < 16.0) thumb_w = 16.0;
            if (thumb_w > track_w) thumb_w = track_w;
            double thumb_x = track_x +
                (track_w - thumb_w) * (b->scroll_x / b->scroll_max_x);
            cairo_save(cr);
            cairo_set_source_rgba(cr, tk_r, tk_g, tk_b, tk_a);
            cairo_rectangle(cr, track_x, track_y, track_w, track_h);
            cairo_fill(cr);
            cairo_set_source_rgba(cr, th_r, th_g, th_b, th_a);
            cairo_rectangle(cr, thumb_x, track_y + 1, thumb_w, track_h - 2);
            cairo_fill(cr);
            cairo_restore(cr);
        }
    }

    if (b->kind == NS_BOX_BLOCK && b->style && b->columns >= 2) {
        double col_gap = 16;
        int n_cols = b->columns;
        (void)ns_css_used_column_count(b->style, b->content_width, &col_gap);
        {
            double rule_w = length_or(b->style->values[NS_CSS_COLUMN_RULE_WIDTH], 0);
            const ns_css_value *rstyle =
                b->style->values[NS_CSS_COLUMN_RULE_STYLE];
            gboolean rdrawable = rstyle && rstyle->kind == NS_CSS_V_KEYWORD &&
                rstyle->u.keyword && strcmp(rstyle->u.keyword, "none") != 0 &&
                strcmp(rstyle->u.keyword, "hidden") != 0;
            if (rule_w > 0 && rdrawable) {
                rgba rc = rgba_of(b->style->values[NS_CSS_COLUMN_RULE_COLOR],
                                  0.50, 0.50, 0.50, 1.0);
                double inx = b->x + b->margin.left + b->border.left + b->padding.left;
                double iny = b->y + b->margin.top  + b->border.top  + b->padding.top;
                double cw = b->content_width;
                double col_w = (cw - col_gap * (n_cols - 1)) / n_cols;
                cairo_save(cr);
                set_source_rgba(cr, rc);
                cairo_set_line_width(cr, rule_w);
                if (strcmp(rstyle->u.keyword, "dashed") == 0) {
                    double dashes[] = { rule_w * 3, rule_w * 2 };
                    cairo_set_dash(cr, dashes, 2, 0);
                } else if (strcmp(rstyle->u.keyword, "dotted") == 0) {
                    double dashes[] = { rule_w, rule_w };
                    cairo_set_dash(cr, dashes, 2, 0);
                }
                for (int i = 0; i < n_cols - 1; i++) {
                    double rx = inx + col_w * (i + 1) + col_gap * i + col_gap / 2.0;
                    cairo_move_to(cr, rx, iny);
                    cairo_line_to(cr, rx, iny + b->content_height);
                    cairo_stroke(cr);
                }
                cairo_restore(cr);
            }
        }
    }
    if (clip_overflow) cairo_restore(cr);
    if (entries != entries_buf) g_free(entries);

    if (has_path_clip) cairo_restore(cr);
    if (deferred_mine) {
        if (g_dbg_paint_x >= 0) {
            double fx0, fy0, fx1, fy1;
            cairo_clip_extents(cr, &fx0, &fy0, &fx1, &fy1);
            g_printerr("[paint-flush-postclip] owner=<%s#%s> n=%u "
                       "clip=%.0f,%.0f..%.0f,%.0f\n",
                       b->dom && b->dom->name ? b->dom->name : "?",
                       b->dom && ns_element_get_attr(b->dom, "id")
                           ? ns_element_get_attr(b->dom, "id") : "",
                       deferred_mine->len, fx0, fy0, fx1, fy1);
        }
        if (has_transform || has_sticky) g_paint_no_cull++;
        g_layers.flush_layered =
            b == g_layers.root ||
            (b == g_layers.owner && !grouped && !has_transform &&
             !has_path_clip);
        paint_flush_deferred(cr, deferred_mine, highlight);
        if (has_transform || has_sticky) g_paint_no_cull--;
        g_ptr_array_free(deferred_mine, TRUE);
    }

    if (has_transform) cairo_restore(cr);

    if (grouped) {
        cairo_pop_group_to_source(cr);
        cairo_pattern_t *group_source =
            cairo_pattern_reference(cairo_get_source(cr));
        cairo_operator_t saved_op = cairo_get_operator(cr);
        if (blend != CAIRO_OPERATOR_OVER) cairo_set_operator(cr, blend);
        cairo_pattern_t *mp = NULL;
        if (mask_grad) mp = ns_paint_mask_layers_pattern(cr, b);
        if (mp) {
            cairo_mask(cr, mp);
        } else {
            cairo_paint_with_alpha(cr, op);
        }
        if (blend != CAIRO_OPERATOR_OVER) cairo_set_operator(cr, saved_op);
        paint_group_video_holes(cr, group_source, mp, op, first_group_hole);
        if (mp) cairo_pattern_destroy(mp);
        cairo_pattern_destroy(group_source);
    }

    if (has_sticky) paint_anchor_leave(cr, saved_anchor_dx, saved_anchor_dy);
    if (g_dbg_paint_x >= 0) {
        double q0, q1, q2, q3;
        cairo_clip_extents(cr, &q0, &q1, &q2, &q3);
        if (fabs(q0 - dbg_e0) > 0.5 || fabs(q1 - dbg_e1) > 0.5 ||
            fabs(q2 - dbg_e2) > 0.5 || fabs(q3 - dbg_e3) > 0.5)
            g_printerr("[clip-LEAK] <%s#%s class=%.70s kind=%d grp=%d "
                       "tf=%d ov=%d pc=%d st=%d> "
                       "entry=%.0f,%.0f..%.0f,%.0f exit=%.0f,%.0f..%.0f,%.0f\n",
                       b->dom && b->dom->name ? b->dom->name : "?",
                       b->dom && b->dom->kind == NS_NODE_ELEMENT &&
                       ns_element_get_attr(b->dom, "id")
                           ? ns_element_get_attr(b->dom, "id") : "",
                       b->dom && b->dom->kind == NS_NODE_ELEMENT
                           ? (ns_element_get_attr(b->dom, "class") ?: "")
                           : "",
                       (int)b->kind, grouped, has_transform, clip_overflow,
                       has_path_clip, has_sticky,
                       dbg_e0, dbg_e1, dbg_e2, dbg_e3, q0, q1, q2, q3);
    }
}

gboolean
ns_paint_viewport_origin(double *x, double *y)
{
    *x = g_paint_vp_x0;
    *y = g_paint_vp_y0;
    return g_paint_have_viewport;
}

GHashTable *
ns_paint_selection_runs(void)
{
    return g_paint_sel_runs;
}

int
ns_paint_layers_mode(void)
{
    return g_layers.mode;
}

void
ns_paint_layers_note_video(cairo_t *cr)
{
    if (g_layers.mode == PAINT_LAYERS_DOC && cr != g_layers.doc)
        g_layers.video_above = TRUE;
}

void
ns_paint_walk_atomic(cairo_t *cr, const ns_box *box, const char *highlight)
{
    g_paint_no_cull++;
    const ns_box *saved_flush = g_paint_flush_box;
    g_paint_flush_box = box;
    paint_walk(cr, box, highlight);
    g_paint_flush_box = saved_flush;
    g_paint_no_cull--;
}

static const ns_box *
find_element_box_named(const ns_box *b, const char *name)
{
    if (!b) return NULL;
    if (b->dom && b->dom->kind == NS_NODE_ELEMENT && b->dom->name &&
        strcmp(b->dom->name, name) == 0)
        return b;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling) {
        const ns_box *hit = find_element_box_named(c, name);
        if (hit) return hit;
    }
    return NULL;
}

static gboolean
box_solid_background(const ns_box *b, rgba *out)
{
    const ns_style *s = b ? b->style : NULL;
    if (s && s->values[NS_CSS_BACKGROUND_COLOR] &&
        s->values[NS_CSS_BACKGROUND_COLOR]->kind == NS_CSS_V_COLOR &&
        s->values[NS_CSS_BACKGROUND_COLOR]->u.color.a > 0) {
        *out = rgba_of(s->values[NS_CSS_BACKGROUND_COLOR], 1, 1, 1, 1);
        return TRUE;
    }
    return FALSE;
}

static gboolean
canvas_background_of(const ns_box *root, rgba *out)
{
    if (!root) return FALSE;
    const ns_box *html = find_element_box_named(root, "html");
    if (box_solid_background(html, out)) return TRUE;
    const ns_box *body = find_element_box_named(root, "body");
    if (box_solid_background(body, out)) return TRUE;
    return FALSE;
}

static const ns_box *
find_box_for_node(const ns_box *b, const ns_node *node)
{
    if (!b) return NULL;
    if (b->dom == node) return b;
    for (const ns_box *c = b->first_child; c; c = c->next_sibling) {
        const ns_box *hit = find_box_for_node(c, node);
        if (hit) return hit;
    }
    return NULL;
}

static const ns_box *
top_layer_box(const ns_box *root)
{
    const ns_node *modal = ns_dom_active_modal();
    if (!modal) return NULL;
    return find_box_for_node(root, modal);
}

static void
paint_top_layer(cairo_t *cr, const ns_box *root, const char *highlight)
{
    const ns_box *top = top_layer_box(root);
    if (!top) return;
    const ns_style *s = top->style;
    const ns_style *bd = s ? s->backdrop : NULL;
    if (bd && bd->values[NS_CSS_BACKGROUND_COLOR]) {
        rgba c = rgba_of(bd->values[NS_CSS_BACKGROUND_COLOR], 0, 0, 0, 0);
        double x1, y1, x2, y2;
        cairo_clip_extents(cr, &x1, &y1, &x2, &y2);
        cairo_save(cr);
        set_source_rgba(cr, c);
        cairo_rectangle(cr, x1, y1, x2 - x1, y2 - y1);
        cairo_fill(cr);
        cairo_restore(cr);
    }
    const ns_box *saved = g_paint_skip_box;
    const ns_box *saved_flush = g_paint_flush_box;
    g_paint_skip_box = NULL;
    g_paint_flush_box = top;
    paint_walk(cr, top, highlight);
    g_paint_flush_box = saved_flush;
    g_paint_skip_box = saved;
}

static void
paint_document(cairo_t *cr, const ns_box *root, const char *highlight_query,
               const struct ns_selection *sel)
{
    ns_paint_list_ordinals_begin();
    if (g_paint_video_holes) g_array_set_size(g_paint_video_holes, 0);
    rgba bg = { 254.0 / 255, 254.0 / 255, 254.0 / 255, 1 };
    canvas_background_of(root, &bg);
    cairo_save(cr);
    set_source_rgba(cr, bg);
    cairo_paint(cr);
    cairo_restore(cr);
    paint_cache_clip(cr);
    if (sel)
        g_paint_sel_runs = ns_selection_ranges(root, sel);
    g_paint_skip_box = top_layer_box(root);
    paint_walk(cr, root, highlight_query);
    g_paint_skip_box = NULL;
    paint_top_layer(cr, root, highlight_query);
    g_clear_pointer(&g_paint_sel_runs, g_hash_table_destroy);
    g_paint_have_clip = FALSE;
    if (g_paint_video_holes) g_array_set_size(g_paint_video_holes, 0);
    g_paint_have_viewport = FALSE;
    ns_paint_list_ordinals_end();
}

void
ns_paint(cairo_t *cr, const ns_box *root, const char *highlight_query)
{
    paint_document(cr, root, highlight_query, NULL);
}

void
ns_paint_with_selection(cairo_t *cr, const ns_box *root,
                        const char *highlight_query,
                        const struct ns_selection *sel)
{
    paint_document(cr, root, highlight_query, sel);
}

gboolean
ns_paint_canvas_color(const ns_box *root, double rgba_out[4])
{
    rgba bg = { 254.0 / 255, 254.0 / 255, 254.0 / 255, 1 };
    gboolean found = canvas_background_of(root, &bg);
    rgba_out[0] = bg.r;
    rgba_out[1] = bg.g;
    rgba_out[2] = bg.b;
    rgba_out[3] = bg.a;
    return found;
}

static gboolean
layers_box_bg_fixed(const ns_box *b)
{
    const ns_css_value *att = b->style
        ? b->style->values[NS_CSS_BACKGROUND_ATTACHMENT] : NULL;
    const ns_css_value *img = b->style
        ? b->style->values[NS_CSS_BACKGROUND_IMAGE] : NULL;
    if (!att || !img || keyword_is(img, "none")) return FALSE;
    int n = MAX(ns_css_value_layer_count(att), 1);
    for (int i = 0; i < n; i++)
        if (keyword_is(ns_css_value_layer(att, i), "fixed")) return TRUE;
    return FALSE;
}

static gboolean
layers_box_video_composited(const ns_box *b)
{
    return b->kind == NS_BOX_VIDEO && b->media && b->media->video &&
           ns_video_helper_composited(b->media->video);
}

static int
layers_vp_kind(const ns_box *b)
{
    const ns_css_value *pos = b->style ? b->style->values[NS_CSS_POSITION]
                                       : NULL;
    if (keyword_is(pos, "fixed"))
        return ns_box_is_fixed(b) ? NS_PAINT_VP_FIXED : 0;
    if (keyword_is(pos, "sticky") && !ns_box_in_scroller(b))
        return NS_PAINT_VP_STICKY;
    return 0;
}

static gboolean
layers_box_needs_frames(const ns_box *b, int under)
{
    return (under && layers_box_video_composited(b)) ||
           layers_box_bg_fixed(b);
}

static int
layers_scan_kind(const ns_box *b, int under, gboolean in_atomic)
{
    if (layers_box_needs_frames(b, under)) return -1;
    int kind = layers_vp_kind(b);
    if (!kind) return 0;
    gboolean nested_sticky = under == NS_PAINT_VP_STICKY ||
                             (under && kind == NS_PAINT_VP_STICKY);
    if (in_atomic || nested_sticky || box_z_index(b) < 0) return -1;
    return under ? 0 : kind;
}

static gboolean
layers_scan(const ns_box *b, int under, gboolean in_atomic, GHashTable *kinds,
            int *roots);

static gboolean
layers_scan_children(const ns_box *b, int under, gboolean in_atomic,
                     GHashTable *kinds, int *roots)
{
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        if (!layers_scan(c, under, in_atomic, kinds, roots)) return FALSE;
    guint n = b->inline_atomics ? b->inline_atomics->len : 0;
    for (guint i = 0; i < n; i++) {
        const ns_box *ab =
            g_array_index(b->inline_atomics, ns_inline_atomic, i).box;
        if (ab && !layers_scan(ab, under, TRUE, kinds, roots)) return FALSE;
    }
    return TRUE;
}

static gboolean
layers_scan(const ns_box *b, int under, gboolean in_atomic, GHashTable *kinds,
            int *roots)
{
    if (box_is_hidden(b) || box_clip_hides(b)) return TRUE;
    int kind = layers_scan_kind(b, under, in_atomic);
    if (kind < 0) return FALSE;
    if (kind > 0) {
        g_hash_table_insert(kinds, (gpointer)b, GINT_TO_POINTER(kind));
        (*roots)++;
        under = kind;
    }
    return box_skips_contents(b) ||
           layers_scan_children(b, under, in_atomic, kinds, roots);
}

void
ns_paint_layer_plan_init(ns_paint_layer_plan *plan)
{
    plan->dynamic = FALSE;
    plan->kinds = g_hash_table_new(g_direct_hash, g_direct_equal);
    plan->vp = g_array_new(FALSE, TRUE, sizeof(ns_paint_vp_capture));
}

void
ns_paint_layer_plan_clear(ns_paint_layer_plan *plan)
{
    g_clear_pointer(&plan->kinds, g_hash_table_destroy);
    if (plan->vp) g_array_free(plan->vp, TRUE);
    plan->vp = NULL;
}

void
ns_paint_plan_layers(cairo_t *cr, const ns_box *root,
                     ns_paint_layer_plan *plan)
{
    plan->dynamic = FALSE;
    g_hash_table_remove_all(plan->kinds);
    g_array_set_size(plan->vp, 0);
    if (!root) return;
    int roots = 0;
    if (top_layer_box(root) ||
        !layers_scan(root, 0, FALSE, plan->kinds, &roots)) {
        plan->dynamic = TRUE;
        return;
    }
    if (roots == 0) return;
    g_layers.mode = PAINT_LAYERS_PLAN;
    g_layers.root = root;
    g_layers.kinds = plan->kinds;
    g_layers.found = plan->vp;
    cairo_get_matrix(cr, &g_layers.base);
    paint_document(cr, root, NULL, NULL);
    memset(&g_layers, 0, sizeof g_layers);
    if ((int)plan->vp->len != roots) plan->dynamic = TRUE;
}

gboolean
ns_paint_doc_layers(cairo_t *cr, ns_paint_upper_fn upper, gpointer upper_data,
                    const ns_box *root, const char *highlight_query,
                    const struct ns_selection *sel,
                    const ns_paint_layer_plan *plan)
{
    g_layers.mode = PAINT_LAYERS_DOC;
    g_layers.root = root;
    g_layers.doc = cr;
    g_layers.kinds = plan->kinds;
    g_layers.upper = upper;
    g_layers.upper_data = upper_data;
    g_layers.n_upper = (int)plan->vp->len;
    paint_document(cr, root, highlight_query, sel);
    gboolean ok = !g_layers.video_above;
    memset(&g_layers, 0, sizeof g_layers);
    return ok;
}

void
ns_paint_vp_layer(cairo_t *cr, const ns_box *root,
                  const ns_paint_vp_capture *layer, double vp_x, double vp_y,
                  const char *highlight_query,
                  const struct ns_selection *sel)
{
    ns_paint_list_ordinals_begin();
    if (g_paint_video_holes) g_array_set_size(g_paint_video_holes, 0);
    paint_cache_clip(cr);
    g_paint_vp_x0 = vp_x;
    g_paint_vp_y0 = vp_y;
    g_paint_have_viewport = vp_x != 0 || vp_y != 0;
    if (sel)
        g_paint_sel_runs = ns_selection_ranges(root, sel);
    if (layer->kind == NS_PAINT_VP_STICKY)
        g_paint_sticky_static_box = layer->box;
    const ns_box *saved_flush = g_paint_flush_box;
    cairo_save(cr);
    cairo_matrix_t base, m;
    cairo_get_matrix(cr, &base);
    cairo_matrix_multiply(&m, &layer->rel, &base);
    cairo_set_matrix(cr, &m);
    g_paint_flush_box = layer->box;
    paint_walk(cr, layer->box, highlight_query);
    g_paint_flush_box = saved_flush;
    cairo_restore(cr);
    g_paint_sticky_static_box = NULL;
    g_clear_pointer(&g_paint_sel_runs, g_hash_table_destroy);
    g_paint_have_clip = FALSE;
    if (g_paint_video_holes) g_array_set_size(g_paint_video_holes, 0);
    g_paint_have_viewport = FALSE;
    ns_paint_list_ordinals_end();
}
