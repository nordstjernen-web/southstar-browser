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

