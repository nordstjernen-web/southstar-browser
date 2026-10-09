/* Southstar — HTML canvas 2D, Path2D, ImageBitmap (QuickJS).
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */
#include "js_internal.h"
#include "js_classid.h"

#include <math.h>
#include <string.h>

#include <gio/gio.h>
#include "ns_pango.h"

#include "css.h"
#include "net.h"
#include "paint.h"
#include "video.h"
#include "texture.h"
#include "image.h"
#include "webgl.h"
#ifdef ND_HAVE_WEBGPU
#include "webgpu.h"
#endif

typedef struct { double pos, r, g, b, a; } ns_conic_stop;

cairo_pattern_t *ns_canvas_conic_pattern(double cx, double cy, double angle,
                                         ns_conic_stop *stops, guint n);

static cairo_pattern_t *
ns_ctx_build_conic_pattern(JSContext *ctx, JSValueConst obj)
{
    double cx = 0, cy = 0, angle = 0;
    JSValue v;
    v = ns_hget(ctx, obj, "_x0"); JS_ToFloat64(ctx, &cx, v); JS_FreeValue(ctx, v);
    v = ns_hget(ctx, obj, "_y0"); JS_ToFloat64(ctx, &cy, v); JS_FreeValue(ctx, v);
    v = ns_hget(ctx, obj, "_angle"); JS_ToFloat64(ctx, &angle, v); JS_FreeValue(ctx, v);

    GArray *sa = g_array_new(FALSE, FALSE, sizeof(ns_conic_stop));
    JSValue stops = ns_hget(ctx, obj, "_stops");
    if (JS_IsArray(stops)) {
        JSValue lenv = JS_GetPropertyStr(ctx, stops, "length");
        uint32_t n = 0; JS_ToUint32(ctx, &n, lenv); JS_FreeValue(ctx, lenv);
        for (uint32_t i = 0; i < n; i++) {
            JSValue s = JS_GetPropertyUint32(ctx, stops, i);
            if (JS_IsObject(s)) {
                ns_conic_stop cs = { 0, 0, 0, 0, 1 };
                JSValue f;
                f = JS_GetPropertyStr(ctx, s, "pos"); JS_ToFloat64(ctx, &cs.pos, f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "r");   JS_ToFloat64(ctx, &cs.r,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "g");   JS_ToFloat64(ctx, &cs.g,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "b");   JS_ToFloat64(ctx, &cs.b,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "a");   JS_ToFloat64(ctx, &cs.a,   f); JS_FreeValue(ctx, f);
                g_array_append_val(sa, cs);
            }
            JS_FreeValue(ctx, s);
        }
    }
    JS_FreeValue(ctx, stops);
    if (sa->len == 0) { g_array_free(sa, TRUE); return NULL; }
    cairo_pattern_t *pat = ns_canvas_conic_pattern(cx, cy, angle,
        (ns_conic_stop *)(void *)sa->data, sa->len);
    g_array_free(sa, TRUE);
    return pat;
}

cairo_pattern_t *
ns_ctx_build_pattern(JSContext *ctx, JSValueConst obj, gboolean *origin_clean)
{
    *origin_clean = TRUE;
    if (!JS_IsObject(obj)) return NULL;
    JSValue t = ns_hget(ctx, obj, "_type");
    if (!JS_IsString(t)) { JS_FreeValue(ctx, t); return NULL; }
    const char *type = JS_ToCString(ctx, t);
    JS_FreeValue(ctx, t);
    if (!type) return NULL;
    cairo_pattern_t *pat = NULL;
    if (strcmp(type, "linear") == 0) {
        double x0 = 0, y0 = 0, x1 = 0, y1 = 0;
        JSValue v;
        v = ns_hget(ctx, obj, "_x0"); JS_ToFloat64(ctx, &x0, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_y0"); JS_ToFloat64(ctx, &y0, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_x1"); JS_ToFloat64(ctx, &x1, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_y1"); JS_ToFloat64(ctx, &y1, v); JS_FreeValue(ctx, v);
        pat = cairo_pattern_create_linear(x0, y0, x1, y1);
    } else if (strcmp(type, "pattern") == 0) {
        JS_FreeCString(ctx, type);
        JSValue node_v = ns_hget(ctx, obj, "_node");
        int iw = 0, ih = 0;
        cairo_surface_t *img =
            ns_ctx_drawimage_source(ctx, node_v, &iw, &ih, origin_clean);
        JS_FreeValue(ctx, node_v);
        if (!img) return NULL;
        pat = cairo_pattern_create_for_surface(img);
        cairo_surface_destroy(img);
        cairo_extend_t ext = CAIRO_EXTEND_REPEAT;
        JSValue rep_v = ns_hget(ctx, obj, "_rep");
        if (JS_IsString(rep_v)) {
            const char *r = JS_ToCString(ctx, rep_v);
            if (r) {
                if      (!strcmp(r, "repeat-x"))  ext = CAIRO_EXTEND_REPEAT;
                else if (!strcmp(r, "repeat-y"))  ext = CAIRO_EXTEND_REPEAT;
                else if (!strcmp(r, "no-repeat")) ext = CAIRO_EXTEND_NONE;
                else                              ext = CAIRO_EXTEND_REPEAT;
                JS_FreeCString(ctx, r);
            }
        }
        JS_FreeValue(ctx, rep_v);
        cairo_pattern_set_extend(pat, ext);
        JSValue m = ns_hget(ctx, obj, "_matrix");
        if (JS_IsArray(m)) {
            double a = 1, b = 0, c = 0, d = 1, e = 0, f = 0;
            JSValue vv;
            vv = JS_GetPropertyUint32(ctx, m, 0); JS_ToFloat64(ctx, &a, vv); JS_FreeValue(ctx, vv);
            vv = JS_GetPropertyUint32(ctx, m, 1); JS_ToFloat64(ctx, &b, vv); JS_FreeValue(ctx, vv);
            vv = JS_GetPropertyUint32(ctx, m, 2); JS_ToFloat64(ctx, &c, vv); JS_FreeValue(ctx, vv);
            vv = JS_GetPropertyUint32(ctx, m, 3); JS_ToFloat64(ctx, &d, vv); JS_FreeValue(ctx, vv);
            vv = JS_GetPropertyUint32(ctx, m, 4); JS_ToFloat64(ctx, &e, vv); JS_FreeValue(ctx, vv);
            vv = JS_GetPropertyUint32(ctx, m, 5); JS_ToFloat64(ctx, &f, vv); JS_FreeValue(ctx, vv);
            cairo_matrix_t cm;
            cairo_matrix_init(&cm, a, b, c, d, e, f);
            cairo_matrix_t inv = cm;
            if (cairo_matrix_invert(&inv) == CAIRO_STATUS_SUCCESS)
                cairo_pattern_set_matrix(pat, &inv);
        }
        JS_FreeValue(ctx, m);
        return pat;
    } else if (strcmp(type, "radial") == 0) {
        double x0 = 0, y0 = 0, r0 = 0, x1 = 0, y1 = 0, r1 = 0;
        JSValue v;
        v = ns_hget(ctx, obj, "_x0"); JS_ToFloat64(ctx, &x0, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_y0"); JS_ToFloat64(ctx, &y0, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_r0"); JS_ToFloat64(ctx, &r0, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_x1"); JS_ToFloat64(ctx, &x1, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_y1"); JS_ToFloat64(ctx, &y1, v); JS_FreeValue(ctx, v);
        v = ns_hget(ctx, obj, "_r1"); JS_ToFloat64(ctx, &r1, v); JS_FreeValue(ctx, v);
        pat = cairo_pattern_create_radial(x0, y0, r0, x1, y1, r1);
    } else if (strcmp(type, "conic") == 0) {
        JS_FreeCString(ctx, type);
        return ns_ctx_build_conic_pattern(ctx, obj);
    }
    JS_FreeCString(ctx, type);
    if (!pat) return NULL;
    JSValue stops = ns_hget(ctx, obj, "_stops");
    if (JS_IsArray(stops)) {
        JSValue lenv = JS_GetPropertyStr(ctx, stops, "length");
        uint32_t n = 0; JS_ToUint32(ctx, &n, lenv); JS_FreeValue(ctx, lenv);
        for (uint32_t i = 0; i < n; i++) {
            JSValue s = JS_GetPropertyUint32(ctx, stops, i);
            if (JS_IsObject(s)) {
                double pos = 0, r = 0, g = 0, b = 0, a = 1;
                JSValue f;
                f = JS_GetPropertyStr(ctx, s, "pos"); JS_ToFloat64(ctx, &pos, f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "r");   JS_ToFloat64(ctx, &r,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "g");   JS_ToFloat64(ctx, &g,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "b");   JS_ToFloat64(ctx, &b,   f); JS_FreeValue(ctx, f);
                f = JS_GetPropertyStr(ctx, s, "a");   JS_ToFloat64(ctx, &a,   f); JS_FreeValue(ctx, f);
                cairo_pattern_add_color_stop_rgba(pat, pos, r, g, b, a);
            }
            JS_FreeValue(ctx, s);
        }
    }
    JS_FreeValue(ctx, stops);
    return pat;
}

double
ns_ctx_global_alpha(JSContext *ctx, JSValueConst this_val)
{
    JSValue v = ns_hget(ctx, this_val, "globalAlpha");
    double ga = 1.0;
    JS_ToFloat64(ctx, &ga, v);
    JS_FreeValue(ctx, v);
    if (ga < 0) ga = 0;
    if (ga > 1) ga = 1;
    return ga;
}

void
ns_ctx_apply_composite(JSContext *ctx, JSValueConst this_val, cairo_t *cr)
{
    JSValue v = ns_hget(ctx, this_val, "globalCompositeOperation");
    cairo_operator_t op = CAIRO_OPERATOR_OVER;
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) { op = ns_ctx_parse_composite(s); JS_FreeCString(ctx, s); }
    }
    JS_FreeValue(ctx, v);
    cairo_set_operator(cr, op);
}

gboolean
ns_ctx_image_smoothing(JSContext *ctx, JSValueConst this_val)
{
    JSValue v = ns_hget(ctx, this_val, "imageSmoothingEnabled");
    gboolean on = TRUE;
    if (!JS_IsUndefined(v) && !JS_IsNull(v))
        on = JS_ToBool(ctx, v) ? TRUE : FALSE;
    JS_FreeValue(ctx, v);
    return on;
}

void
ns_ctx_sync_styles(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st)
{
    JSValue v;
    if (st->fill_pattern) { cairo_pattern_destroy(st->fill_pattern); st->fill_pattern = NULL; }
    if (st->stroke_pattern) { cairo_pattern_destroy(st->stroke_pattern); st->stroke_pattern = NULL; }
    v = ns_hget(ctx, this_val, "fillStyle");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) {
            double r, g, b, a;
            if (ns_canvas_parse_color(s, &r, &g, &b, &a)) {
                st->fill_r = r; st->fill_g = g; st->fill_b = b; st->fill_a = a;
            }
            JS_FreeCString(ctx, s);
        }
    } else if (JS_IsObject(v)) {
        gboolean clean = TRUE;
        st->fill_pattern = ns_ctx_build_pattern(ctx, v, &clean);
        if (!clean) st->origin_clean = FALSE;
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "strokeStyle");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) {
            double r, g, b, a;
            if (ns_canvas_parse_color(s, &r, &g, &b, &a)) {
                st->stroke_r = r; st->stroke_g = g; st->stroke_b = b; st->stroke_a = a;
            }
            JS_FreeCString(ctx, s);
        }
    } else if (JS_IsObject(v)) {
        gboolean clean = TRUE;
        st->stroke_pattern = ns_ctx_build_pattern(ctx, v, &clean);
        if (!clean) st->origin_clean = FALSE;
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "lineWidth");
    double lw;
    if (JS_ToFloat64(ctx, &lw, v) == 0 && lw > 0) st->line_width = lw;
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "font");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) { g_free(st->font); st->font = g_strdup(s); JS_FreeCString(ctx, s); }
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "lineCap");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) {
            if      (strcmp(s, "round")  == 0) cairo_set_line_cap(st->cr, CAIRO_LINE_CAP_ROUND);
            else if (strcmp(s, "square") == 0) cairo_set_line_cap(st->cr, CAIRO_LINE_CAP_SQUARE);
            else                                cairo_set_line_cap(st->cr, CAIRO_LINE_CAP_BUTT);
            JS_FreeCString(ctx, s);
        }
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "lineJoin");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) {
            if      (strcmp(s, "round") == 0) cairo_set_line_join(st->cr, CAIRO_LINE_JOIN_ROUND);
            else if (strcmp(s, "bevel") == 0) cairo_set_line_join(st->cr, CAIRO_LINE_JOIN_BEVEL);
            else                              cairo_set_line_join(st->cr, CAIRO_LINE_JOIN_MITER);
            JS_FreeCString(ctx, s);
        }
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "miterLimit");
    double ml;
    if (JS_ToFloat64(ctx, &ml, v) == 0 && ml > 0) cairo_set_miter_limit(st->cr, ml);
    JS_FreeValue(ctx, v);
    st->shadow_r = st->shadow_g = st->shadow_b = 0;
    st->shadow_a = 0;
    st->shadow_blur = st->shadow_ox = st->shadow_oy = 0;
    v = ns_hget(ctx, this_val, "shadowColor");
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) {
            double r, g, b, a;
            if (ns_canvas_parse_color(s, &r, &g, &b, &a)) {
                st->shadow_r = r; st->shadow_g = g;
                st->shadow_b = b; st->shadow_a = a;
            }
            JS_FreeCString(ctx, s);
        }
    }
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "shadowBlur");
    double sb = 0;
    if (JS_ToFloat64(ctx, &sb, v) == 0 && sb >= 0) st->shadow_blur = sb;
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "shadowOffsetX");
    double sox = 0;
    if (JS_ToFloat64(ctx, &sox, v) == 0) st->shadow_ox = sox;
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "shadowOffsetY");
    double soy = 0;
    if (JS_ToFloat64(ctx, &soy, v) == 0) st->shadow_oy = soy;
    JS_FreeValue(ctx, v);
    double dash_offset = 0;
    v = ns_hget(ctx, this_val, "lineDashOffset");
    JS_ToFloat64(ctx, &dash_offset, v);
    JS_FreeValue(ctx, v);
    v = ns_hget(ctx, this_val, "_dashes");
    if (JS_IsArray(v)) {
        uint32_t n = ns_js_array_length(ctx, v);
        if (n == 0) {
            cairo_set_dash(st->cr, NULL, 0, 0);
        } else {
            if (n > 64) n = 64;
            double dashes[64];
            double dash_sum = 0;
            for (uint32_t i = 0; i < n; i++) {
                JSValue e = JS_GetPropertyUint32(ctx, v, i);
                double d = 0; JS_ToFloat64(ctx, &d, e); JS_FreeValue(ctx, e);
                if (d < 0) d = 0;
                dashes[i] = d;
                dash_sum += d;
            }
            if (dash_sum > 0)
                cairo_set_dash(st->cr, dashes, (int)n, dash_offset);
            else
                cairo_set_dash(st->cr, NULL, 0, 0);
        }
    } else {
        cairo_set_dash(st->cr, NULL, 0, 0);
    }
    JS_FreeValue(ctx, v);
}

gboolean
ns_ctx_has_shadow(const ns_canvas_state *st)
{
    if (!st || st->shadow_a <= 0) return FALSE;
    return st->shadow_ox != 0 || st->shadow_oy != 0 || st->shadow_blur > 0;
}

void
ns_ctx_with_shadow(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st,
                   ns_ctx_drawfn draw, void *ud)
{
    if (!ns_ctx_has_shadow(st)) {
        draw(st->cr, ud);
        return;
    }
    int w = st->w, h = st->h;
    if (w <= 0 || h <= 0) { draw(st->cr, ud); return; }
    cairo_surface_t *off = cairo_image_surface_create(CAIRO_FORMAT_ARGB32, w, h);
    if (cairo_surface_status(off) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(off);
        draw(st->cr, ud);
        return;
    }
    cairo_t *ocr = cairo_create(off);
    cairo_matrix_t m;
    cairo_get_matrix(st->cr, &m);
    cairo_set_matrix(ocr, &m);
    draw(ocr, ud);
    cairo_destroy(ocr);
    cairo_surface_flush(off);
    uint8_t *data = cairo_image_surface_get_data(off);
    int stride = cairo_image_surface_get_stride(off);
    int sw = cairo_image_surface_get_width(off);
    int sh = cairo_image_surface_get_height(off);
    if (data) {
        for (int y = 0; y < sh; y++) {
            uint8_t *row = data + y * stride;
            for (int x = 0; x < sw; x++) {
                uint8_t a = row[x * 4 + 3];
                uint8_t na = (uint8_t)(a * st->shadow_a);
                row[x * 4 + 0] = (uint8_t)(st->shadow_b * na);
                row[x * 4 + 1] = (uint8_t)(st->shadow_g * na);
                row[x * 4 + 2] = (uint8_t)(st->shadow_r * na);
                row[x * 4 + 3] = na;
            }
        }
        double half_blur = st->shadow_blur * 0.5 + 0.5;
        int radius = half_blur < 64 ? (int)half_blur : 64;
        if (radius > 0)
            ns_box_blur_argb(data, sw, sh, stride, radius);
        cairo_surface_mark_dirty(off);
    }
    cairo_save(st->cr);
    cairo_identity_matrix(st->cr);
    cairo_set_source_surface(st->cr, off, st->shadow_ox, st->shadow_oy);
    cairo_paint(st->cr);
    cairo_restore(st->cr);
    cairo_surface_destroy(off);
    draw(st->cr, ud);
    (void)ctx; (void)this_val;
}

void
ns_ctx_set_fill_source(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st)
{
    double ga = ns_ctx_global_alpha(ctx, this_val);
    if (st->fill_pattern) cairo_set_source(st->cr, st->fill_pattern);
    else cairo_set_source_rgba(st->cr, st->fill_r, st->fill_g, st->fill_b,
                               st->fill_a * ga);
}

void
ns_ctx_set_stroke_source(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st)
{
    double ga = ns_ctx_global_alpha(ctx, this_val);
    if (st->stroke_pattern) cairo_set_source(st->cr, st->stroke_pattern);
    else cairo_set_source_rgba(st->cr, st->stroke_r, st->stroke_g, st->stroke_b,
                               st->stroke_a * ga);
}

void
ns_draw_fillrect(cairo_t *cr, void *vud)
{
    ns_draw_rect_ud *u = vud;
    if (cr == u->st->cr) ns_ctx_set_fill_source(u->ctx, u->this_val, u->st);
    else {
        double ga = ns_ctx_global_alpha(u->ctx, u->this_val);
        cairo_set_source_rgba(cr, u->st->fill_r, u->st->fill_g,
                              u->st->fill_b, u->st->fill_a * ga);
    }
    ns_ctx_apply_composite(u->ctx, u->this_val, cr);
    cairo_rectangle(cr, u->x, u->y, u->w, u->h);
    cairo_fill(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
}

void
ns_draw_strokerect(cairo_t *cr, void *vud)
{
    ns_draw_rect_ud *u = vud;
    if (cr == u->st->cr) ns_ctx_set_stroke_source(u->ctx, u->this_val, u->st);
    else {
        double ga = ns_ctx_global_alpha(u->ctx, u->this_val);
        cairo_set_source_rgba(cr, u->st->stroke_r, u->st->stroke_g,
                              u->st->stroke_b, u->st->stroke_a * ga);
    }
    ns_ctx_apply_composite(u->ctx, u->this_val, cr);
    cairo_set_line_width(cr, u->lw);
    cairo_rectangle(cr, u->x, u->y, u->w, u->h);
    cairo_stroke(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
}

JSValue
ns_ctx_fillRect(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 4) return JS_UNDEFINED;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    ns_draw_rect_ud u = {
        .x = ns_arg_d(ctx, argv[0]), .y = ns_arg_d(ctx, argv[1]),
        .w = ns_arg_d(ctx, argv[2]), .h = ns_arg_d(ctx, argv[3]),
        .lw = st->line_width, .ctx = ctx, .this_val = this_val, .st = st,
    };
    ns_ctx_with_shadow(ctx, this_val, st, ns_draw_fillrect, &u);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_strokeRect(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 4) return JS_UNDEFINED;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    ns_draw_rect_ud u = {
        .x = ns_arg_d(ctx, argv[0]), .y = ns_arg_d(ctx, argv[1]),
        .w = ns_arg_d(ctx, argv[2]), .h = ns_arg_d(ctx, argv[3]),
        .lw = st->line_width, .ctx = ctx, .this_val = this_val, .st = st,
    };
    ns_ctx_with_shadow(ctx, this_val, st, ns_draw_strokerect, &u);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_clearRect(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 4) return JS_UNDEFINED;
    double x = ns_arg_d(ctx, argv[0]), y = ns_arg_d(ctx, argv[1]);
    double w = ns_arg_d(ctx, argv[2]), h = ns_arg_d(ctx, argv[3]);
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    cairo_save(st->cr);
    cairo_set_operator(st->cr, CAIRO_OPERATOR_CLEAR);
    cairo_rectangle(st->cr, x, y, w, h);
    cairo_fill(st->cr);
    cairo_restore(st->cr);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

cairo_path_t *
ns_ctx_prepare_path_and_rule(JSContext *ctx, cairo_t *cr,
                             int argc, JSValueConst *argv)
{
    JSValueConst path_v = JS_UNDEFINED;
    const char *rule_s = NULL;
    if (argc >= 1 && ns_value_is_path2d(argv[0])) {
        path_v = argv[0];
        if (argc >= 2 && JS_IsString(argv[1]))
            rule_s = JS_ToCString(ctx, argv[1]);
    } else if (argc >= 1 && JS_IsString(argv[0])) {
        rule_s = JS_ToCString(ctx, argv[0]);
    }
    cairo_path_t *saved = NULL;
    if (!JS_IsUndefined(path_v)) {
        saved = cairo_copy_path(cr);
        ns_replay_path2d(cr, path_v);
    }
    cairo_set_fill_rule(cr, ns_parse_fill_rule(rule_s));
    if (rule_s) JS_FreeCString(ctx, rule_s);
    return saved;
}

void
ns_ctx_restore_path(cairo_t *cr, cairo_path_t *saved)
{
    if (!saved) return;
    cairo_new_path(cr);
    cairo_append_path(cr, saved);
    cairo_path_destroy(saved);
}

void
ns_draw_fillpath(cairo_t *cr, void *vud)
{
    ns_draw_path_ud *u = vud;
    if (cr != u->st->cr) {
        cairo_new_path(cr);
        if (u->snapshot) cairo_append_path(cr, u->snapshot);
        cairo_set_source_rgba(cr, u->st->fill_r, u->st->fill_g,
                              u->st->fill_b, u->st->fill_a *
                              ns_ctx_global_alpha(u->ctx, u->this_val));
    } else {
        ns_ctx_set_fill_source(u->ctx, u->this_val, u->st);
    }
    cairo_set_fill_rule(cr, u->fill_rule);
    ns_ctx_apply_composite(u->ctx, u->this_val, cr);
    cairo_fill_preserve(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
}

void
ns_draw_strokepath(cairo_t *cr, void *vud)
{
    ns_draw_path_ud *u = vud;
    if (cr != u->st->cr) {
        cairo_new_path(cr);
        if (u->snapshot) cairo_append_path(cr, u->snapshot);
        cairo_set_source_rgba(cr, u->st->stroke_r, u->st->stroke_g,
                              u->st->stroke_b, u->st->stroke_a *
                              ns_ctx_global_alpha(u->ctx, u->this_val));
    } else {
        ns_ctx_set_stroke_source(u->ctx, u->this_val, u->st);
    }
    ns_ctx_apply_composite(u->ctx, u->this_val, cr);
    cairo_set_line_width(cr, u->lw);
    cairo_stroke_preserve(cr);
    cairo_set_operator(cr, CAIRO_OPERATOR_OVER);
}

JSValue
ns_ctx_fill(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    cairo_path_t *saved = ns_ctx_prepare_path_and_rule(ctx, st->cr, argc, argv);
    cairo_path_t *snap = ns_ctx_has_shadow(st) ? cairo_copy_path(st->cr) : NULL;
    ns_draw_path_ud u = {
        .ctx = ctx, .this_val = this_val, .st = st,
        .lw = st->line_width, .snapshot = snap,
        .fill_rule = cairo_get_fill_rule(st->cr),
    };
    ns_ctx_with_shadow(ctx, this_val, st, ns_draw_fillpath, &u);
    if (snap) cairo_path_destroy(snap);
    ns_ctx_restore_path(st->cr, saved);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_stroke(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    cairo_path_t *saved = NULL;
    if (argc >= 1 && ns_value_is_path2d(argv[0])) {
        saved = cairo_copy_path(st->cr);
        ns_replay_path2d(st->cr, argv[0]);
    }
    cairo_path_t *snap = ns_ctx_has_shadow(st) ? cairo_copy_path(st->cr) : NULL;
    ns_draw_path_ud u = {
        .ctx = ctx, .this_val = this_val, .st = st,
        .lw = st->line_width, .snapshot = snap,
        .fill_rule = cairo_get_fill_rule(st->cr),
    };
    ns_ctx_with_shadow(ctx, this_val, st, ns_draw_strokepath, &u);
    if (snap) cairo_path_destroy(snap);
    ns_ctx_restore_path(st->cr, saved);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

static const char *ns_ctx_savable_props[] = {
    "fillStyle", "strokeStyle", "font", "textAlign", "textBaseline",
    "direction", "globalAlpha", "globalCompositeOperation",
    "shadowColor", "shadowBlur", "shadowOffsetX", "shadowOffsetY",
    "imageSmoothingEnabled", "imageSmoothingQuality",
    "lineWidth", "lineCap", "lineJoin", "miterLimit", "lineDashOffset",
    "filter", "letterSpacing", "wordSpacing",
    "fontKerning", "fontStretch", "fontVariantCaps", "textRendering",
    "_dashes",
};

JSValue
ns_ctx_save(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    cairo_save(st->cr);
    JSValue stack = ns_hget(ctx, this_val, "_stateStack");
    if (!JS_IsArray(stack)) {
        JS_FreeValue(ctx, stack);
        stack = JS_NewArray(ctx);
        ns_hset(ctx, this_val, "_stateStack", JS_DupValue(ctx, stack));
    }
    JSValue snap = JS_NewObjectProto(ctx, JS_NULL);
    for (gsize i = 0; i < G_N_ELEMENTS(ns_ctx_savable_props); i++) {
        JSValue v = ns_hget(ctx, this_val, ns_ctx_savable_props[i]);
        JS_DefinePropertyValueStr(ctx, snap, ns_ctx_savable_props[i], v,
                                  JS_PROP_C_W_E);
    }
    uint32_t n = ns_js_array_length(ctx, stack);
    JS_DefinePropertyValueUint32(ctx, stack, n, snap, JS_PROP_C_W_E);
    JS_FreeValue(ctx, stack);
    return JS_UNDEFINED;
}

JSValue
ns_ctx_restore(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    JSValue stack = ns_hget(ctx, this_val, "_stateStack");
    if (!JS_IsArray(stack)) { JS_FreeValue(ctx, stack); return JS_UNDEFINED; }
    uint32_t n = ns_js_array_length(ctx, stack);
    if (n == 0) { JS_FreeValue(ctx, stack); return JS_UNDEFINED; }
    JSValue snap = JS_GetPropertyUint32(ctx, stack, n - 1);
    for (gsize i = 0; i < G_N_ELEMENTS(ns_ctx_savable_props); i++) {
        JSValue v = JS_GetPropertyStr(ctx, snap, ns_ctx_savable_props[i]);
        ns_hset(ctx, this_val, ns_ctx_savable_props[i], v);
    }
    JS_FreeValue(ctx, snap);
    JSAtom len_atom = JS_NewAtom(ctx, "length");
    JS_SetProperty(ctx, stack, len_atom, JS_NewUint32(ctx, n - 1));
    JS_FreeAtom(ctx, len_atom);
    JS_FreeValue(ctx, stack);
    cairo_restore(st->cr);
    return JS_UNDEFINED;
}

NsPangoFontDescription *
ns_canvas_font_desc(const char *css_font)
{
    const char *src = css_font && *css_font ? css_font : "10px sans-serif";
    double size_px = 10.0;
    const char *p = src;
    GString *rest = g_string_new(NULL);
    gboolean found_size = FALSE;
    while (*p) {
        while (*p && g_ascii_isspace(*p)) p++;
        const char *start = p;
        while (*p && !g_ascii_isspace(*p)) p++;
        gsize len = (gsize)(p - start);
        if (len == 0) continue;
        if (!found_size && len >= 3 && g_ascii_isdigit(start[0])) {
            char *endp = NULL;
            double v = g_ascii_strtod(start, &endp);
            if (endp && endp > start) {
                gsize used = (gsize)(endp - start);
                if (used + 2 <= len &&
                    (g_ascii_strncasecmp(endp, "px", 2) == 0 ||
                     g_ascii_strncasecmp(endp, "pt", 2) == 0)) {
                    if (g_ascii_strncasecmp(endp, "pt", 2) == 0)
                        v = v * 96.0 / 72.0;
                    size_px = v;
                    found_size = TRUE;
                    continue;
                }
                if (used == len) {
                    size_px = v;
                    found_size = TRUE;
                    continue;
                }
            }
        }
        if (rest->len) g_string_append_c(rest, ' ');
        g_string_append_len(rest, start, len);
    }
    NsPangoFontDescription *desc = ns_pango_font_description_from_string(
        rest->len ? rest->str : "sans-serif");
    g_string_free(rest, TRUE);
    if (size_px <= 0) size_px = 10;
    ns_pango_font_description_set_absolute_size(
        desc, ns_paint_pango_font_size(size_px));
    return desc;
}

gboolean
ns_ctx_direction_is_rtl(JSContext *ctx, JSValueConst this_val)
{
    JSValue v = ns_hget(ctx, this_val, "direction");
    gboolean rtl = FALSE;
    if (JS_IsString(v)) {
        const char *s = JS_ToCString(ctx, v);
        if (s) { rtl = strcmp(s, "rtl") == 0; JS_FreeCString(ctx, s); }
    }
    JS_FreeValue(ctx, v);
    return rtl;
}

void
ns_ctx_paint_text(JSContext *ctx, JSValueConst this_val,
                  ns_canvas_state *st, const char *text,
                  double x, double y, double max_width,
                  gboolean stroke)
{
    NsPangoLayout *layout = ns_pango_cairo_create_layout(st->cr);
    NsPangoFontDescription *desc = ns_canvas_font_desc(st->font);
    ns_pango_layout_set_font_description(layout, desc);
    ns_pango_layout_set_text(layout, text, -1);
    NsPangoRectangle ink, logical;
    ns_pango_layout_get_extents(layout, &ink, &logical);
    double baseline_offset =
        (double)ns_pango_layout_get_baseline(layout) / NS_PANGO_SCALE;
    JSValue baseline_v = ns_hget(ctx, this_val, "textBaseline");
    double dy = 0;
    if (JS_IsString(baseline_v)) {
        const char *bs = JS_ToCString(ctx, baseline_v);
        if (bs) {
            if      (!strcmp(bs, "top"))         dy = 0;
            else if (!strcmp(bs, "hanging"))     dy = -baseline_offset * 0.2;
            else if (!strcmp(bs, "middle"))      dy = -baseline_offset * 0.5;
            else if (!strcmp(bs, "ideographic")) dy = -(double)(logical.y + logical.height) / NS_PANGO_SCALE;
            else                                  dy = -baseline_offset;
            JS_FreeCString(ctx, bs);
        }
    } else {
        dy = -baseline_offset;
    }
    JS_FreeValue(ctx, baseline_v);
    gboolean rtl = ns_ctx_direction_is_rtl(ctx, this_val);
    JSValue align_v = ns_hget(ctx, this_val, "textAlign");
    double dx = 0;
    double tw = (double)logical.width / NS_PANGO_SCALE;
    const char *align = "start";
    char align_buf[16];
    if (JS_IsString(align_v)) {
        const char *as = JS_ToCString(ctx, align_v);
        if (as) {
            g_strlcpy(align_buf, as, sizeof align_buf);
            align = align_buf;
            JS_FreeCString(ctx, as);
        }
    }
    JS_FreeValue(ctx, align_v);
    if      (!strcmp(align, "center"))                       dx = -tw / 2;
    else if (!strcmp(align, "right"))                        dx = -tw;
    else if (!strcmp(align, "left"))                         dx = 0;
    else if (!strcmp(align, "end")   && !rtl)                dx = -tw;
    else if (!strcmp(align, "end")   &&  rtl)                dx = 0;
    else if (!strcmp(align, "start") &&  rtl)                dx = -tw;
    double xscale = 1.0;
    if (max_width > 0 && tw > max_width) xscale = max_width / tw;
    cairo_save(st->cr);
    ns_ctx_apply_composite(ctx, this_val, st->cr);
    cairo_translate(st->cr, x + dx * xscale, y + dy);
    if (xscale < 1.0) cairo_scale(st->cr, xscale, 1.0);
    if (stroke) {
        ns_ctx_set_stroke_source(ctx, this_val, st);
        cairo_set_line_width(st->cr, st->line_width);
        cairo_move_to(st->cr, 0, 0);
        ns_pango_cairo_layout_path(st->cr, layout);
        cairo_stroke(st->cr);
    } else {
        ns_ctx_set_fill_source(ctx, this_val, st);
        cairo_move_to(st->cr, 0, 0);
        ns_pango_cairo_show_layout(st->cr, layout);
    }
    cairo_restore(st->cr);
    ns_pango_font_description_free(desc);
    g_object_unref(layout);
}

JSValue
ns_ctx_fillText(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 3) return JS_UNDEFINED;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    const char *text = JS_ToCString(ctx, argv[0]);
    if (!text) return JS_UNDEFINED;
    double x = ns_arg_d(ctx, argv[1]);
    double y = ns_arg_d(ctx, argv[2]);
    double mw = argc >= 4 ? ns_arg_d(ctx, argv[3]) : 0;
    ns_ctx_paint_text(ctx, this_val, st, text, x, y, mw, FALSE);
    JS_FreeCString(ctx, text);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_measureText(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    double width = 0, ascent = 0, descent = 0;
    double font_ascent = 0, font_descent = 0;
    ns_canvas_state *st = argc >= 1 ? ns_ctx_state(ctx, this_val) : NULL;
    const char *text = argc >= 1 ? JS_ToCString(ctx, argv[0]) : NULL;
    if (text && st) {
        ns_ctx_sync_styles(ctx, this_val, st);
        NsPangoLayout *layout = ns_pango_cairo_create_layout(st->cr);
        NsPangoFontDescription *desc = ns_canvas_font_desc(st->font);
        ns_pango_layout_set_font_description(layout, desc);
        ns_pango_layout_set_text(layout, text, -1);
        NsPangoRectangle ink, logical;
        ns_pango_layout_get_extents(layout, &ink, &logical);
        double baseline_y = (double)ns_pango_layout_get_baseline(layout);
        width = (double)logical.width / NS_PANGO_SCALE;
        ascent  = (baseline_y - (double)ink.y) / NS_PANGO_SCALE;
        descent = ((double)(ink.y + ink.height) - baseline_y) / NS_PANGO_SCALE;
        if (ascent < 0) ascent = 0;
        if (descent < 0) descent = 0;
        NsPangoContext *pctx = ns_pango_layout_get_context(layout);
        NsPangoFontMetrics *fm = ns_pango_context_get_metrics(pctx, desc, NULL);
        if (fm) {
            font_ascent  = (double)ns_pango_font_metrics_get_ascent(fm)  / NS_PANGO_SCALE;
            font_descent = (double)ns_pango_font_metrics_get_descent(fm) / NS_PANGO_SCALE;
            ns_pango_font_metrics_unref(fm);
        }
        ns_pango_font_description_free(desc);
        g_object_unref(layout);
    }
    if (text) JS_FreeCString(ctx, text);
    const double metrics[10] = {
        width, 0, width, ascent, descent, font_ascent, font_descent,
        font_ascent * 0.8, 0, -font_descent,
    };
    return ns_textmetrics_new(ctx, ns_ctx_realm(ctx, this_val), metrics);
}

JSValue
ns_ctx_clip(JSContext *ctx, JSValueConst this_val,
            int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    cairo_path_t *saved = ns_ctx_prepare_path_and_rule(ctx, st->cr, argc, argv);
    cairo_clip_preserve(st->cr);
    ns_ctx_restore_path(st->cr, saved);
    return JS_UNDEFINED;
}

JSValue
ns_ctx_setLineDash(JSContext *ctx, JSValueConst this_val,
                   int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    JSValue stored = JS_NewArray(ctx);
    if (argc >= 1 && JS_IsArray(argv[0])) {
        uint32_t n = ns_js_array_length(ctx, argv[0]);
        uint32_t out = 0;
        gboolean dup = (n % 2 == 1);
        for (uint32_t pass = 0; pass < (dup ? 2u : 1u); pass++) {
            for (uint32_t i = 0; i < n; i++) {
                JSValue e = JS_GetPropertyUint32(ctx, argv[0], i);
                double d = 0;
                JS_ToFloat64(ctx, &d, e);
                JS_FreeValue(ctx, e);
                if (!isfinite(d) || d < 0) d = 0;
                JS_SetPropertyUint32(ctx, stored, out++, JS_NewFloat64(ctx, d));
            }
        }
    }
    ns_hset(ctx, this_val, "_dashes", stored);
    return JS_UNDEFINED;
}

JSValue
ns_ctx_getLineDash(JSContext *ctx, JSValueConst this_val,
                   int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue cur = ns_hget(ctx, this_val, "_dashes");
    if (!JS_IsArray(cur)) {
        JS_FreeValue(ctx, cur);
        return JS_NewArray(ctx);
    }
    uint32_t n = ns_js_array_length(ctx, cur);
    JSValue out = JS_NewArray(ctx);
    for (uint32_t i = 0; i < n; i++) {
        JSValue e = JS_GetPropertyUint32(ctx, cur, i);
        JS_SetPropertyUint32(ctx, out, i, e);
    }
    JS_FreeValue(ctx, cur);
    return out;
}

JSValue
ns_ctx_gradient_addColorStop(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    if (argc < 2) return JS_UNDEFINED;
    double pos = ns_arg_d(ctx, argv[0]);
    if (!(pos >= 0.0 && pos <= 1.0))
        return ns_canvas_throw_dom(ctx, "IndexSizeError",
            "addColorStop offset must be in the range [0, 1]");
    const char *col = JS_ToCString(ctx, argv[1]);
    if (!col)
        return ns_canvas_throw_dom(ctx, "SyntaxError",
            "addColorStop color could not be parsed");
    double r, g, b, a;
    gboolean ok = ns_canvas_parse_color(col, &r, &g, &b, &a);
    JS_FreeCString(ctx, col);
    if (!ok)
        return ns_canvas_throw_dom(ctx, "SyntaxError",
            "addColorStop color could not be parsed");
    JSValue stops = ns_hget(ctx, this_val, "_stops");
    if (!JS_IsArray(stops)) {
        JS_FreeValue(ctx, stops);
        stops = JS_NewArray(ctx);
        ns_hset(ctx, this_val, "_stops", JS_DupValue(ctx, stops));
    }
    JSValue lenv = JS_GetPropertyStr(ctx, stops, "length");
    uint32_t n = 0; JS_ToUint32(ctx, &n, lenv); JS_FreeValue(ctx, lenv);
    JSValue entry = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, entry, "pos", JS_NewFloat64(ctx, pos));
    JS_SetPropertyStr(ctx, entry, "r",   JS_NewFloat64(ctx, r));
    JS_SetPropertyStr(ctx, entry, "g",   JS_NewFloat64(ctx, g));
    JS_SetPropertyStr(ctx, entry, "b",   JS_NewFloat64(ctx, b));
    JS_SetPropertyStr(ctx, entry, "a",   JS_NewFloat64(ctx, a));
    JS_SetPropertyUint32(ctx, stops, n, entry);
    JS_FreeValue(ctx, stops);
    return JS_UNDEFINED;
}

cairo_surface_t *
ns_ctx_drawimage_source(JSContext *ctx, JSValueConst src, int *out_w, int *out_h,
                        gboolean *origin_clean)
{
    *origin_clean = TRUE;
    if (!JS_IsObject(src)) return NULL;
    int clean = TRUE;
    cairo_surface_t *bitmap = ns_image_bitmap_surface(src, out_w, out_h, &clean);
    if (bitmap) {
        *origin_clean = clean;
        return bitmap;
    }
    const ns_node *n = ns_offscreen_node(src);
    if (!n) n = ns_unwrap_element(src);
    if (!n || !n->name) return NULL;
    ns_js *js = js_from_ctx(ctx);
    if (!js) return NULL;
    if (strcmp(n->name, "canvas") == 0) {
        ns_canvas_state *st = ns_canvas_state_for(js, n);
        if (st && st->surf) {
            *out_w = st->w;
            *out_h = st->h;
            *origin_clean = st->origin_clean;
            return cairo_surface_reference(st->surf);
        }
        return NULL;
    }
    ns_texture *tex = NULL;
    const char *source_url = NULL;
    const char *cors_allow_origin = NULL;
    gboolean cors_requested = ns_element_get_attr(n, "crossorigin") != NULL;
    if (js->layout_root) {
        const ns_box *b = ns_box_find_by_dom(js->layout_root, n);
        if (b && b->media) {
            if (strcmp(n->name, "img") == 0 && b->media->image) {
                const ns_image *im = (const ns_image *)b->media->image;
                if (im->texture) {
                    tex = im->texture;
                    source_url = im->final_url ? im->final_url : im->url;
                    cors_allow_origin = im->cors_allow_origin;
                }
            } else if (strcmp(n->name, "video") == 0 && b->media->video) {
                const ns_video *v = (const ns_video *)b->media->video;
                tex = v->poster_texture;
                source_url = v->poster_url;
            }
        }
    }
    ns_image *im_cache = NULL;
    if (!tex && strcmp(n->name, "img") == 0) {
        const ns_image *im = ns_js_image_for_node(js, n);
        if (im && im->texture) {
            tex = im->texture;
            source_url = im->final_url ? im->final_url : im->url;
            cors_allow_origin = im->cors_allow_origin;
            if (!im->anim_frames) im_cache = (ns_image *)im;
        }
    }
    if (!tex) return NULL;
    *origin_clean = ns_js_resource_origin_clean(js, ctx, source_url,
        cors_requested ? cors_allow_origin : NULL);
    if (im_cache && im_cache->render_surface) {
        cairo_surface_t *cached = im_cache->render_surface;
        *out_w = cairo_image_surface_get_width(cached);
        *out_h = cairo_image_surface_get_height(cached);
        return cairo_surface_reference(cached);
    }
    int iw = ns_texture_get_width(tex);
    int ih = ns_texture_get_height(tex);
    if (iw <= 0 || ih <= 0) return NULL;
    cairo_surface_t *surf =
        cairo_image_surface_create(CAIRO_FORMAT_ARGB32, iw, ih);
    if (cairo_surface_status(surf) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(surf);
        return NULL;
    }
    guchar *dst = cairo_image_surface_get_data(surf);
    int dst_stride = cairo_image_surface_get_stride(surf);
    ns_texture_download(tex, dst, (gsize)dst_stride);
    cairo_surface_mark_dirty(surf);
    *out_w = iw;
    *out_h = ih;
    if (im_cache)
        im_cache->render_surface = cairo_surface_reference(surf);
    return surf;
}

cairo_surface_t *
ns_js_drawimage_source_surface(JSContext *ctx, JSValueConst src,
                               int *out_w, int *out_h, gboolean *threw)
{
    *out_w = 0;
    *out_h = 0;
    *threw = FALSE;
    gboolean origin_clean = TRUE;
    cairo_surface_t *surf = ns_ctx_drawimage_source(ctx, src, out_w, out_h,
                                                    &origin_clean);
    if (!surf || origin_clean) return surf;
    cairo_surface_destroy(surf);
    *out_w = 0;
    *out_h = 0;
    *threw = TRUE;
    ns_canvas_throw_dom(ctx, "SecurityError",
                        "The image source is not origin-clean.");
    return NULL;
}

JSValue
ns_ctx_drawImage(JSContext *ctx, JSValueConst this_val,
                 int argc, JSValueConst *argv)
{
    if (argc < 1 || !JS_IsObject(argv[0]))
        return JS_ThrowTypeError(ctx,
            "Failed to execute 'drawImage' on 'CanvasRenderingContext2D': "
            "argument 1 is not a valid image source.");
    if (argc < 3) return JS_UNDEFINED;
    int sw_total = 0, sh_total = 0;
    gboolean origin_clean = TRUE;
    cairo_surface_t *src = ns_ctx_drawimage_source(ctx, argv[0],
                                                   &sw_total, &sh_total,
                                                   &origin_clean);
    if (!src || sw_total <= 0 || sh_total <= 0) {
        if (src) cairo_surface_destroy(src);
        return JS_UNDEFINED;
    }
    double sx, sy, sw, sh, dx, dy, dw, dh;
    if (argc >= 9) {
        sx = ns_arg_d(ctx, argv[1]);
        sy = ns_arg_d(ctx, argv[2]);
        sw = ns_arg_d(ctx, argv[3]);
        sh = ns_arg_d(ctx, argv[4]);
        dx = ns_arg_d(ctx, argv[5]);
        dy = ns_arg_d(ctx, argv[6]);
        dw = ns_arg_d(ctx, argv[7]);
        dh = ns_arg_d(ctx, argv[8]);
    } else if (argc >= 5) {
        sx = 0; sy = 0; sw = sw_total; sh = sh_total;
        dx = ns_arg_d(ctx, argv[1]);
        dy = ns_arg_d(ctx, argv[2]);
        dw = ns_arg_d(ctx, argv[3]);
        dh = ns_arg_d(ctx, argv[4]);
    } else {
        sx = 0; sy = 0; sw = sw_total; sh = sh_total;
        dx = ns_arg_d(ctx, argv[1]);
        dy = ns_arg_d(ctx, argv[2]);
        dw = sw_total; dh = sh_total;
    }
    if (sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0) {
        cairo_surface_destroy(src);
        return JS_UNDEFINED;
    }
    double ga = ns_ctx_global_alpha(ctx, this_val);
    gboolean smooth = ns_ctx_image_smoothing(ctx, this_val);
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) {
        cairo_surface_destroy(src);
        return JS_UNDEFINED;
    }
    cairo_save(st->cr);
    ns_ctx_apply_composite(ctx, this_val, st->cr);
    cairo_translate(st->cr, dx, dy);
    cairo_scale(st->cr, dw / sw, dh / sh);
    cairo_translate(st->cr, -sx, -sy);
    cairo_rectangle(st->cr, sx, sy, sw, sh);
    cairo_clip(st->cr);
    cairo_set_source_surface(st->cr, src, 0, 0);
    cairo_pattern_set_filter(cairo_get_source(st->cr),
                             smooth ? CAIRO_FILTER_BILINEAR
                                    : CAIRO_FILTER_NEAREST);
    if (ga < 1.0 - 1e-6) cairo_paint_with_alpha(st->cr, ga);
    else                  cairo_paint(st->cr);
    cairo_restore(st->cr);
    cairo_surface_destroy(src);
    if (!origin_clean) st->origin_clean = FALSE;
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_createPattern(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    if (argc < 1 || !JS_IsObject(argv[0])) return JS_NULL;
    int iw = 0, ih = 0;
    gboolean origin_clean = TRUE;
    cairo_surface_t *probe = ns_ctx_drawimage_source(ctx, argv[0], &iw, &ih,
                                                     &origin_clean);
    if (!probe) return JS_NULL;
    cairo_surface_destroy(probe);
    const char *rep = "repeat";
    const char *r = argc >= 2 && JS_IsString(argv[1]) ? JS_ToCString(ctx, argv[1]) : NULL;
    if (r && *r) rep = r;
    JSValue obj = ns_pattern_new(ctx, ns_ctx_realm(ctx, this_val), argv[0], rep);
    if (r) JS_FreeCString(ctx, r);
    return obj;
}

JSValue
ns_ctx_createLinearGradient(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv)
{
    if (argc < 4) return JS_NULL;
    JSValue obj = ns_gradient_new(ctx, ns_ctx_realm(ctx, this_val), "linear");
    ns_hset(ctx, obj, "_x0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[0])));
    ns_hset(ctx, obj, "_y0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[1])));
    ns_hset(ctx, obj, "_x1", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[2])));
    ns_hset(ctx, obj, "_y1", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[3])));
    return obj;
}

JSValue
ns_ctx_createRadialGradient(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv)
{
    if (argc < 6) return JS_NULL;
    double r0 = ns_arg_d(ctx, argv[2]);
    double r1 = ns_arg_d(ctx, argv[5]);
    if (r0 < 0.0 || r1 < 0.0)
        return ns_canvas_throw_dom(ctx, "IndexSizeError",
            "createRadialGradient radius must not be negative");
    JSValue obj = ns_gradient_new(ctx, ns_ctx_realm(ctx, this_val), "radial");
    ns_hset(ctx, obj, "_x0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[0])));
    ns_hset(ctx, obj, "_y0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[1])));
    ns_hset(ctx, obj, "_r0", JS_NewFloat64(ctx, r0));
    ns_hset(ctx, obj, "_x1", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[3])));
    ns_hset(ctx, obj, "_y1", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[4])));
    ns_hset(ctx, obj, "_r1", JS_NewFloat64(ctx, r1));
    return obj;
}

JSValue
ns_ctx_createConicGradient(JSContext *ctx, JSValueConst this_val,
                           int argc, JSValueConst *argv)
{
    if (argc < 3) return JS_NULL;
    JSValue obj = ns_gradient_new(ctx, ns_ctx_realm(ctx, this_val), "conic");
    ns_hset(ctx, obj, "_angle", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[0])));
    ns_hset(ctx, obj, "_x0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[1])));
    ns_hset(ctx, obj, "_y0", JS_NewFloat64(ctx, ns_arg_d(ctx, argv[2])));
    return obj;
}

JSValue
ns_ctx_createImageData(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    int w = 0, h = 0;
    if (JS_IsObject(argv[0]) && !JS_IsNumber(argv[0])) {
        JSValue wv = JS_GetPropertyStr(ctx, argv[0], "width");
        JSValue hv = JS_GetPropertyStr(ctx, argv[0], "height");
        JS_ToInt32(ctx, &w, wv); JS_ToInt32(ctx, &h, hv);
        JS_FreeValue(ctx, wv); JS_FreeValue(ctx, hv);
    } else if (argc >= 2) {
        JS_ToInt32(ctx, &w, argv[0]);
        JS_ToInt32(ctx, &h, argv[1]);
    } else {
        return JS_ThrowTypeError(ctx,
            "Failed to execute 'createImageData' on 'CanvasRenderingContext2D': "
            "2 arguments required, but only 1 present.");
    }
    int64_t aw = w < 0 ? -(int64_t)w : (int64_t)w;
    int64_t ah = h < 0 ? -(int64_t)h : (int64_t)h;
    if (aw == 0 || ah == 0)
        return ns_canvas_throw_dom(ctx, "IndexSizeError", aw == 0
            ? "The source width is zero or not a number."
            : "The source height is zero or not a number.");
    if (aw > 32767 || ah > 32767) return JS_ThrowRangeError(ctx, "ImageData too large");
    return ns_imagedata_new(ctx, ns_ctx_realm(ctx, this_val), (int)aw, (int)ah, NULL);
}

JSValue
ns_ctx_getImageData(JSContext *ctx, JSValueConst this_val,
                    int argc, JSValueConst *argv)
{
    (void)argc;
    int sx = 0, sy = 0, sw = 0, sh = 0;
    JS_ToInt32(ctx, &sx, argv[0]);
    JS_ToInt32(ctx, &sy, argv[1]);
    JS_ToInt32(ctx, &sw, argv[2]);
    JS_ToInt32(ctx, &sh, argv[3]);
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (st && !st->origin_clean)
        return ns_canvas_throw_dom(ctx, "SecurityError",
            "The canvas has been tainted by cross-origin data.");
    int64_t ox = sx, oy = sy, rw = sw, rh = sh;
    if (rw < 0) { ox += rw; rw = -rw; }
    if (rh < 0) { oy += rh; rh = -rh; }
    if (rw == 0 || rh == 0)
        return ns_canvas_throw_dom(ctx, "IndexSizeError", rw == 0
            ? "The source width is 0." : "The source height is 0.");
    if (rw > 32767 || rh > 32767)
        return JS_ThrowRangeError(ctx, "getImageData region too large");
    int dw = (int)rw, dh = (int)rh;
    uint8_t *out = g_try_malloc0((size_t)dw * (size_t)dh * 4u);
    if (!out) return JS_ThrowRangeError(ctx, "getImageData allocation failed");
    /* A canvas with no backing surface (never drawn to) reads as
       transparent black, not null. */
    cairo_surface_t *surf = (st && st->surf) ? st->surf : NULL;
    const uint8_t *cd = NULL;
    int cw = 0, ch = 0, cs = 0;
    if (surf) {
        cw = cairo_image_surface_get_width(surf);
        ch = cairo_image_surface_get_height(surf);
        cs = cairo_image_surface_get_stride(surf);
        cairo_surface_flush(surf);
        cd = cairo_image_surface_get_data(surf);
    }
    if (cd)
    for (int y = 0; y < dh; y++) {
        int64_t srcy = oy + y;
        for (int x = 0; x < dw; x++) {
            int64_t srcx = ox + x;
            uint8_t *dst = out + ((size_t)y * (size_t)dw + (size_t)x) * 4u;
            if (srcx < 0 || srcy < 0 || srcx >= cw || srcy >= ch) continue;
            const uint8_t *p = cd + (size_t)srcy * (size_t)cs + (size_t)srcx * 4u;
            uint8_t b = p[0], g = p[1], r = p[2], a = p[3];
            if (a == 0) {
                dst[0] = 0; dst[1] = 0; dst[2] = 0; dst[3] = 0;
            } else if (a == 255) {
                dst[0] = r; dst[1] = g; dst[2] = b; dst[3] = 255;
            } else {
                dst[0] = (uint8_t)((r * 255 + a / 2) / a);
                dst[1] = (uint8_t)((g * 255 + a / 2) / a);
                dst[2] = (uint8_t)((b * 255 + a / 2) / a);
                dst[3] = a;
            }
        }
    }
    JSValue result = ns_imagedata_new(ctx, ns_ctx_realm(ctx, this_val), dw, dh, out);
    g_free(out);
    return result;
}

JSValue
ns_ctx_putImageData(JSContext *ctx, JSValueConst this_val,
                    int argc, JSValueConst *argv)
{
    if (argc < 3 || !JS_IsObject(argv[0])) return JS_UNDEFINED;
    JSValue wv = JS_GetPropertyStr(ctx, argv[0], "width");
    JSValue hv = JS_GetPropertyStr(ctx, argv[0], "height");
    JSValue dv = JS_GetPropertyStr(ctx, argv[0], "data");
    int iw = 0, ih = 0;
    JS_ToInt32(ctx, &iw, wv); JS_ToInt32(ctx, &ih, hv);
    JS_FreeValue(ctx, wv); JS_FreeValue(ctx, hv);
    if (iw <= 0 || ih <= 0) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }
    if (iw > 32767 || ih > 32767) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }
    int dx = 0, dy = 0;
    JS_ToInt32(ctx, &dx, argv[1]);
    JS_ToInt32(ctx, &dy, argv[2]);
    int64_t rx = 0, ry = 0, rw = iw, rh = ih;
    if (argc >= 7) {
        int arx = 0, ary = 0, arw = 0, arh = 0;
        JS_ToInt32(ctx, &arx, argv[3]);
        JS_ToInt32(ctx, &ary, argv[4]);
        JS_ToInt32(ctx, &arw, argv[5]);
        JS_ToInt32(ctx, &arh, argv[6]);
        rx = arx; ry = ary; rw = arw; rh = arh;
    }
    if (rw < 0) { rx += rw; rw = -rw; }
    if (rh < 0) { ry += rh; rh = -rh; }
    if (rx < 0)        { rw += rx; rx = 0; }
    if (ry < 0)        { rh += ry; ry = 0; }
    if (rx + rw > iw)  { rw = iw - rx; }
    if (ry + rh > ih)  { rh = ih - ry; }
    if (rw <= 0 || rh <= 0) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }

    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st || !st->surf) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }
    int cw = cairo_image_surface_get_width(st->surf);
    int ch = cairo_image_surface_get_height(st->surf);
    int cs = cairo_image_surface_get_stride(st->surf);
    cairo_surface_flush(st->surf);
    uint8_t *cd = cairo_image_surface_get_data(st->surf);
    if (!cd) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }

    size_t byte_offset = 0, byte_len = 0, bpe = 0;
    JSValue ab = JS_GetTypedArrayBuffer(ctx, dv, &byte_offset, &byte_len, &bpe);
    if (JS_IsException(ab)) { JS_FreeValue(ctx, dv); return JS_UNDEFINED; }
    size_t ab_len = 0;
    uint8_t *src = JS_GetArrayBuffer(ctx, &ab_len, ab);
    if (!src || byte_len < (size_t)iw * (size_t)ih * 4u ||
        byte_offset + byte_len > ab_len) {
        JS_FreeValue(ctx, ab); JS_FreeValue(ctx, dv);
        return JS_UNDEFINED;
    }
    src += byte_offset;
    for (int y = 0; y < rh; y++) {
        int64_t dst_y = (int64_t)dy + y;
        if (dst_y < 0 || dst_y >= ch) continue;
        for (int x = 0; x < rw; x++) {
            int64_t dst_x = (int64_t)dx + x;
            if (dst_x < 0 || dst_x >= cw) continue;
            const uint8_t *s = src + ((size_t)(ry + y) * (size_t)iw +
                                      (size_t)(rx + x)) * 4u;
            uint8_t r = s[0], g = s[1], b = s[2], a = s[3];
            uint8_t pr, pg, pb;
            if (a == 0)        { pr = 0; pg = 0; pb = 0; }
            else if (a == 255) { pr = r; pg = g; pb = b; }
            else {
                pr = (uint8_t)((r * a + 127) / 255);
                pg = (uint8_t)((g * a + 127) / 255);
                pb = (uint8_t)((b * a + 127) / 255);
            }
            uint8_t *p = cd + (size_t)dst_y * (size_t)cs +
                              (size_t)dst_x * 4u;
            p[0] = pb; p[1] = pg; p[2] = pr; p[3] = a;
        }
    }
    cairo_surface_mark_dirty(st->surf);
    JS_FreeValue(ctx, ab);
    JS_FreeValue(ctx, dv);
    ns_js *_j = js_from_ctx(ctx);
    if (_j) _j->mutated = TRUE;
    return JS_UNDEFINED;
}

JSValue
ns_ctx_strokeText(JSContext *ctx, JSValueConst this_val,
                  int argc, JSValueConst *argv)
{
    if (argc < 3) return JS_UNDEFINED;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    ns_ctx_sync_styles(ctx, this_val, st);
    const char *text = JS_ToCString(ctx, argv[0]);
    if (!text) return JS_UNDEFINED;
    double x = ns_arg_d(ctx, argv[1]);
    double y = ns_arg_d(ctx, argv[2]);
    double mw = argc >= 4 ? ns_arg_d(ctx, argv[3]) : 0;
    ns_ctx_paint_text(ctx, this_val, st, text, x, y, mw, TRUE);
    JS_FreeCString(ctx, text);
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_reset(JSContext *ctx, JSValueConst this_val,
             int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st) return JS_UNDEFINED;
    cairo_save(st->cr);
    cairo_identity_matrix(st->cr);
    cairo_set_operator(st->cr, CAIRO_OPERATOR_CLEAR);
    cairo_paint(st->cr);
    cairo_restore(st->cr);
    cairo_identity_matrix(st->cr);
    cairo_new_path(st->cr);
    cairo_reset_clip(st->cr);
    cairo_set_dash(st->cr, NULL, 0, 0);
    cairo_set_line_width(st->cr, 1);
    cairo_set_line_cap(st->cr, CAIRO_LINE_CAP_BUTT);
    cairo_set_line_join(st->cr, CAIRO_LINE_JOIN_MITER);
    cairo_set_miter_limit(st->cr, 10);
    ns_ctx2d_init_state(ctx, this_val);
    g_free(st->font);
    st->font = g_strdup("10px sans-serif");
    st->fill_r = st->fill_g = st->fill_b = 0; st->fill_a = 1;
    st->stroke_r = st->stroke_g = st->stroke_b = 0; st->stroke_a = 1;
    st->line_width = 1;
    if (st->fill_pattern)   { cairo_pattern_destroy(st->fill_pattern);   st->fill_pattern = NULL; }
    if (st->stroke_pattern) { cairo_pattern_destroy(st->stroke_pattern); st->stroke_pattern = NULL; }
    st->shadow_r = st->shadow_g = st->shadow_b = 0;
    st->shadow_a = 0; st->shadow_blur = 0;
    st->shadow_ox = st->shadow_oy = 0;
    { ns_js *_j = js_from_ctx(ctx); if (_j) _j->mutated = TRUE; }
    return JS_UNDEFINED;
}

JSValue
ns_ctx_isPointInPath(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st || argc < 2) return JS_FALSE;
    int i = 0;
    cairo_path_t *saved = NULL;
    if (ns_value_is_path2d(argv[0])) {
        if (argc < 3) return JS_FALSE;
        saved = cairo_copy_path(st->cr);
        ns_replay_path2d(st->cr, argv[0]);
        i = 1;
    }
    double x = ns_arg_d(ctx, argv[i]);
    double y = ns_arg_d(ctx, argv[i + 1]);
    const char *rule_s = NULL;
    if (argc > i + 2 && JS_IsString(argv[i + 2]))
        rule_s = JS_ToCString(ctx, argv[i + 2]);
    cairo_fill_rule_t prev_rule = cairo_get_fill_rule(st->cr);
    cairo_set_fill_rule(st->cr, ns_parse_fill_rule(rule_s));
    if (rule_s) JS_FreeCString(ctx, rule_s);
    cairo_bool_t in = cairo_in_fill(st->cr, x, y);
    cairo_set_fill_rule(st->cr, prev_rule);
    ns_ctx_restore_path(st->cr, saved);
    return in ? JS_TRUE : JS_FALSE;
}

JSValue
ns_ctx_isPointInStroke(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    ns_canvas_state *st = ns_ctx_state(ctx, this_val);
    if (!st || argc < 2) return JS_FALSE;
    ns_ctx_sync_styles(ctx, this_val, st);
    int i = 0;
    cairo_path_t *saved = NULL;
    if (ns_value_is_path2d(argv[0])) {
        if (argc < 3) return JS_FALSE;
        saved = cairo_copy_path(st->cr);
        ns_replay_path2d(st->cr, argv[0]);
        i = 1;
    }
    double x = ns_arg_d(ctx, argv[i]);
    double y = ns_arg_d(ctx, argv[i + 1]);
    cairo_set_line_width(st->cr, st->line_width);
    cairo_bool_t in = cairo_in_stroke(st->cr, x, y);
    ns_ctx_restore_path(st->cr, saved);
    return in ? JS_TRUE : JS_FALSE;
}
