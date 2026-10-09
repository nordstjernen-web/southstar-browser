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

