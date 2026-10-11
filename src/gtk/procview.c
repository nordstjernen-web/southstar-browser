/* Southstar — GTK thin client over the out-of-process renderer (rproc). */

#include "procview.h"
#include "procview_state.h"
#include "i18n.h"
#include "pagelayers.h"

#include "libsouthstar.h"
#include "proc_limits.h"
#include "../print.h"
#include "rproc_http.h"
#include "rproc_inproc.h"
#include "net.h"

#include <cairo.h>
#include <glib/gstdio.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
#ifdef G_OS_WIN32
#include <windows.h>
#endif

#define NS_PROC_CARET_BLINK_US (530 * 1000)
#define NS_PV_WHEEL_STEP_PX    100.0
#define NS_PV_WHEEL_TAU_MS     45.0
#define NS_PV_FLING_TAU_MS     400.0
#define NS_PV_FLING_STOP_PX_S  20.0
#define NS_PV_HOVER_AFTER_SCROLL_MS 150
#define NS_PV_TILE_H           256

#ifdef __APPLE__
#define NS_PROC_PRIMARY_MASK (GDK_CONTROL_MASK | GDK_META_MASK)
#else
#define NS_PROC_PRIMARY_MASK GDK_CONTROL_MASK
#endif

#ifndef G_OS_WIN32
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#endif

static int
pv_settle_ms(void)
{
    const char *e = g_getenv(NS_PROC_SETTLE_ENV);
    if (e && *e) {
        int v = atoi(e);
        if (v >= 0 && v <= 10000)
            return v;
    }
    return NS_PROC_SETTLE_MS;
}


enum {
    NS_PV_ZOOM_MIN_PERMILLE = (int)(NS_PROC_ZOOM_MIN * 1000.0 + 0.5),
    NS_PV_ZOOM_MAX_PERMILLE = (int)(NS_PROC_ZOOM_MAX * 1000.0 + 0.5)
};

static NsProcView *pv_ref(NsProcView *v) { g_ref_count_inc(&v->rc); return v; }

static void
set_accessible_label(GtkWidget *w, const char *label)
{
    gtk_accessible_update_property(GTK_ACCESSIBLE(w),
                                   GTK_ACCESSIBLE_PROPERTY_LABEL, label, -1);
}

/* Closed silhouette of both glass bulbs, drawn around centre (16,16) so the
   whole glyph fits a compact 32px cursor with rounded bulbs. */
static void
hourglass_bulbs_path(cairo_t *cr, double ox, double oy)
{
    cairo_move_to(cr, 14.6 + ox, 16.0 + oy);
    cairo_curve_to(cr, 12.0 + ox, 13.2 + oy, 9.9 + ox, 10.6 + oy, 10.1 + ox, 8.0 + oy);
    cairo_curve_to(cr, 10.0 + ox, 6.8 + oy, 10.8 + ox, 6.1 + oy, 12.2 + ox, 6.1 + oy);
    cairo_curve_to(cr, 14.2 + ox, 5.6 + oy, 17.8 + ox, 5.6 + oy, 19.8 + ox, 6.1 + oy);
    cairo_curve_to(cr, 21.2 + ox, 6.1 + oy, 22.0 + ox, 6.8 + oy, 21.9 + ox, 8.0 + oy);
    cairo_curve_to(cr, 22.1 + ox, 10.6 + oy, 20.0 + ox, 13.2 + oy, 17.4 + ox, 16.0 + oy);
    cairo_close_path(cr);
    cairo_move_to(cr, 14.6 + ox, 16.0 + oy);
    cairo_curve_to(cr, 12.0 + ox, 18.8 + oy, 9.9 + ox, 21.4 + oy, 10.1 + ox, 24.0 + oy);
    cairo_curve_to(cr, 10.0 + ox, 25.2 + oy, 10.8 + ox, 25.9 + oy, 12.2 + ox, 25.9 + oy);
    cairo_curve_to(cr, 14.2 + ox, 26.4 + oy, 17.8 + ox, 26.4 + oy, 19.8 + ox, 25.9 + oy);
    cairo_curve_to(cr, 21.2 + ox, 25.9 + oy, 22.0 + ox, 25.2 + oy, 21.9 + ox, 24.0 + oy);
    cairo_curve_to(cr, 22.1 + ox, 21.4 + oy, 20.0 + ox, 18.8 + oy, 17.4 + ox, 16.0 + oy);
    cairo_close_path(cr);
}

static void
hourglass_caps_path(cairo_t *cr, double ox, double oy)
{
    cairo_move_to(cr, 9.7 + ox, 6.2 + oy);
    cairo_line_to(cr, 22.3 + ox, 6.2 + oy);
    cairo_move_to(cr, 9.7 + ox, 25.8 + oy);
    cairo_line_to(cr, 22.3 + ox, 25.8 + oy);
}

static GdkTexture *
hourglass_texture(void)
{
    const int size = 32;
    cairo_surface_t *surface =
        cairo_image_surface_create(CAIRO_FORMAT_ARGB32, size, size);
    if (cairo_surface_status(surface) != CAIRO_STATUS_SUCCESS) {
        cairo_surface_destroy(surface);
        return NULL;
    }

    cairo_t *cr = cairo_create(surface);
    cairo_set_antialias(cr, CAIRO_ANTIALIAS_BEST);
    cairo_set_line_cap(cr, CAIRO_LINE_CAP_ROUND);
    cairo_set_line_join(cr, CAIRO_LINE_JOIN_ROUND);

    cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 0.22);
    cairo_set_line_width(cr, 3.0);
    hourglass_caps_path(cr, 0.7, 0.9);
    cairo_stroke(cr);
    cairo_set_line_width(cr, 2.6);
    hourglass_bulbs_path(cr, 0.7, 0.9);
    cairo_stroke(cr);

    cairo_pattern_t *glass = cairo_pattern_create_linear(16.0, 7.0, 16.0, 26.0);
    cairo_pattern_add_color_stop_rgba(glass, 0.0, 0.97, 1.0, 1.0, 0.55);
    cairo_pattern_add_color_stop_rgba(glass, 0.5, 0.62, 0.80, 0.95, 0.30);
    cairo_pattern_add_color_stop_rgba(glass, 1.0, 0.93, 0.98, 1.0, 0.52);
    cairo_set_source(cr, glass);
    hourglass_bulbs_path(cr, 0.0, 0.0);
    cairo_fill(cr);
    cairo_pattern_destroy(glass);

    cairo_pattern_t *sand = cairo_pattern_create_linear(16.0, 7.0, 16.0, 25.0);
    cairo_pattern_add_color_stop_rgba(sand, 0.0, 1.0, 0.85, 0.40, 0.98);
    cairo_pattern_add_color_stop_rgba(sand, 1.0, 0.90, 0.52, 0.14, 0.98);
    cairo_set_source(cr, sand);
    cairo_move_to(cr, 11.7, 8.0);
    cairo_curve_to(cr, 14.0, 7.6, 18.0, 7.6, 20.3, 8.0);
    cairo_curve_to(cr, 18.9, 10.9, 17.4, 13.3, 16.0, 15.3);
    cairo_curve_to(cr, 14.6, 13.3, 13.1, 10.9, 11.7, 8.0);
    cairo_close_path(cr);
    cairo_fill(cr);
    cairo_move_to(cr, 11.7, 24.0);
    cairo_curve_to(cr, 13.5, 21.6, 14.8, 20.9, 16.0, 20.9);
    cairo_curve_to(cr, 17.2, 20.9, 18.5, 21.6, 20.3, 24.0);
    cairo_curve_to(cr, 17.6, 24.7, 14.4, 24.7, 11.7, 24.0);
    cairo_close_path(cr);
    cairo_fill(cr);
    cairo_set_line_width(cr, 1.1);
    cairo_move_to(cr, 16.0, 15.3);
    cairo_line_to(cr, 16.0, 20.9);
    cairo_stroke(cr);
    cairo_pattern_destroy(sand);

    cairo_set_source_rgba(cr, 1.0, 1.0, 1.0, 0.80);
    cairo_set_line_width(cr, 0.9);
    cairo_move_to(cr, 12.6, 8.2);
    cairo_curve_to(cr, 13.7, 10.4, 14.6, 12.2, 15.3, 13.8);
    cairo_stroke(cr);
    cairo_move_to(cr, 16.7, 18.4);
    cairo_curve_to(cr, 17.6, 20.2, 18.7, 22.2, 19.6, 24.0);
    cairo_stroke(cr);

    cairo_set_source_rgba(cr, 0.10, 0.12, 0.14, 0.98);
    cairo_set_line_width(cr, 2.0);
    hourglass_caps_path(cr, 0.0, 0.0);
    cairo_stroke(cr);
    cairo_set_line_width(cr, 1.5);
    hourglass_bulbs_path(cr, 0.0, 0.0);
    cairo_stroke(cr);

    cairo_set_source_rgba(cr, 0.74, 0.82, 0.90, 0.85);
    cairo_set_line_width(cr, 0.6);
    hourglass_bulbs_path(cr, 0.0, 0.0);
    cairo_stroke(cr);

    cairo_destroy(cr);
    cairo_surface_flush(surface);

    int stride = cairo_image_surface_get_stride(surface);
    unsigned char *data = cairo_image_surface_get_data(surface);
    GBytes *bytes = g_bytes_new(data, (gsize)stride * (gsize)size);
    GdkTexture *texture =
        gdk_memory_texture_new(size, size, GDK_MEMORY_DEFAULT, bytes,
                               (gsize)stride);
    g_bytes_unref(bytes);
    cairo_surface_destroy(surface);
    return texture;
}

static GdkCursor *
busy_hourglass_cursor(NsProcView *v)
{
    if (v->hourglass_cursor)
        return v->hourglass_cursor;

    GdkTexture *texture = hourglass_texture();
    if (texture) {
        GdkCursor *fallback = gdk_cursor_new_from_name("wait", NULL);
        v->hourglass_cursor =
            gdk_cursor_new_from_texture(texture, 16, 16, fallback);
        if (fallback)
            g_object_unref(fallback);
        g_object_unref(texture);
    }
    if (!v->hourglass_cursor)
        v->hourglass_cursor = gdk_cursor_new_from_name("wait", NULL);
    return v->hourglass_cursor;
}

static void
pv_set_named_cursor(GtkWidget *w, const char *name)
{
    if (!name || !*name) {
        gtk_widget_set_cursor(w, NULL);
        return;
    }
    static const struct { const char *name; const char *fallback; } fb[] = {
        { "context-menu",  "default" },
        { "help",          "default" },
        { "progress",      "wait" },
        { "cell",          "crosshair" },
        { "vertical-text", "text" },
        { "alias",         "copy" },
        { "copy",          "default" },
        { "move",          "all-scroll" },
        { "no-drop",       "not-allowed" },
        { "not-allowed",   "default" },
        { "grab",          "all-scroll" },
        { "grabbing",      "all-scroll" },
        { "all-scroll",    "move" },
        { "col-resize",    "ew-resize" },
        { "row-resize",    "ns-resize" },
        { "ne-resize",     "nesw-resize" },
        { "sw-resize",     "nesw-resize" },
        { "nw-resize",     "nwse-resize" },
        { "se-resize",     "nwse-resize" },
        { "nesw-resize",   "crosshair" },
        { "nwse-resize",   "crosshair" },
        { "zoom-in",       "crosshair" },
        { "zoom-out",      "crosshair" },
    };
    static const char *const plain[] = {
        "default", "none", "pointer", "wait", "crosshair", "text",
        "n-resize", "e-resize", "s-resize", "w-resize", "ew-resize",
        "ns-resize",
    };
    const char *fallback = NULL;
    gboolean known = FALSE;
    for (gsize i = 0; i < G_N_ELEMENTS(fb); i++)
        if (strcmp(name, fb[i].name) == 0) {
            fallback = fb[i].fallback;
            known = TRUE;
            break;
        }
    for (gsize i = 0; i < G_N_ELEMENTS(plain) && !known; i++)
        known = strcmp(name, plain[i]) == 0;
    if (!known) {
        gtk_widget_set_cursor(w, NULL);
        return;
    }
    GdkCursor *fb_cur = fallback ? gdk_cursor_new_from_name(fallback, NULL) : NULL;
    GdkCursor *cur = gdk_cursor_new_from_name(name, fb_cur);
    gtk_widget_set_cursor(w, cur ? cur : fb_cur);
    if (cur)
        g_object_unref(cur);
    if (fb_cur)
        g_object_unref(fb_cur);
}

static void
set_busy_cursor(NsProcView *v)
{
    v->busy_cursor = TRUE;
    if (!v->area)
        return;

    GdkCursor *cursor = busy_hourglass_cursor(v);
    if (cursor)
        gtk_widget_set_cursor(v->area, cursor);
    else
        gtk_widget_set_cursor_from_name(v->area, "wait");
}

static void
pv_free(NsProcView *v)
{
    ns_pv_media_free(v->media);
    v->media = NULL;
    if (v->queue) {
        Req *r;
        while ((r = g_async_queue_try_pop(v->queue))) {
            g_free(r->url);
            g_free(r->key);
            g_free(r->code);
            g_free(r->query);
            g_free(r->export_dest);
            g_free(r->paths);
            g_free(r->have);
            g_free(r->hold);
            g_free(r);
        }
        g_async_queue_unref(v->queue);
    }
    g_clear_object(&v->frame);
    ns_page_layers_free(v->layers);
    if (v->favicon)
        g_object_unref(v->favicon);
    v->favicon = NULL;
    if (v->ctx_popover)
        gtk_widget_unparent(v->ctx_popover);
    if (v->ctx_actions)
        g_object_unref(v->ctx_actions);
    g_free(v->ctx_link);
    if (v->history)
        g_ptr_array_unref(v->history);
    g_free(v->renderer_path);
    g_free(v->current_url);
    g_free(v->current_title);
    g_free(v->remote_ip);
    g_free(v->deferred_url);
    g_free(v->perm_origin);
    if (v->hourglass_cursor)
        g_object_unref(v->hourglass_cursor);
    g_mutex_clear(&v->proc_lock);
    g_free(v);
}

static void pv_unref(NsProcView *v) { if (g_ref_count_dec(&v->rc)) pv_free(v); }

/* Atomically install a new renderer handle (worker thread only) and return the
   previous one for the caller to close outside the lock. The lock serialises
   the worker's reassignments against the main thread's close-time interrupt so
   it can never touch a freed handle. */
static ns_rproc_http *
pv_swap_proc(NsProcView *v, ns_rproc_http *newp)
{
    g_mutex_lock(&v->proc_lock);
    ns_rproc_http *old = v->proc;
    v->proc = newp;
    g_mutex_unlock(&v->proc_lock);
    return old;
}

char *
ns_proc_renderer_path(void)
{
    const char *env = g_getenv(NS_PROC_RENDERER_ENV);
    if (env && *env)
        return g_strdup(env);
#ifdef G_OS_WIN32
    const char *name = NS_PROC_RENDERER_NAME ".exe";
#else
    const char *name = NS_PROC_RENDERER_NAME;
#endif
    const char *exe = ns_app_self_exe();
    if (exe) {
        char *dir = g_path_get_dirname(exe);
        char *parent = g_build_filename("..", name, NULL);
        const char *rel[] = { name, parent, NULL };
        for (int i = 0; rel[i]; i++) {
            char *cand = g_build_filename(dir, rel[i], NULL);
            if (g_file_test(cand, G_FILE_TEST_IS_EXECUTABLE)) {
                g_free(parent);
                g_free(dir);
                return cand;
            }
            g_free(cand);
        }
        g_free(parent);
        g_free(dir);
    }
    return g_strdup(name);
}

static void request_render(NsProcView *v);
static void print_run(NsProcView *v, GPtrArray *pages,
                      const ns_print_setup *setup, double raster_scale);
static void request_tick(NsProcView *v);

static GdkTexture *
stage_fill(NsProcView *v, const unsigned char *px, int w, int h, int stride)
{
    (void)v;
    size_t row = (size_t)w * 4u;
    if (w <= 0 || h <= 0 || (size_t)stride < row)
        return NULL;
    GBytes *bytes;
    if ((size_t)stride == row) {
        bytes = g_bytes_new(px, row * (size_t)h);
    } else {
        unsigned char *dst = g_malloc(row * (size_t)h);
        for (int y = 0; y < h; y++)
            memcpy(dst + (size_t)y * row, px + (size_t)y * stride, row);
        bytes = g_bytes_new_take(dst, row * (size_t)h);
    }
    GdkTexture *tex = gdk_memory_texture_new(w, h, GDK_MEMORY_DEFAULT, bytes,
                                             row);
    g_bytes_unref(bytes);
    return tex;
}

static void
post_emit(NsProcView *v, NsProcEvent evt, const char *text)
{
    if (v->notify)
        v->notify(v, evt, text, v->notify_ud);
}

static void
clear_busy_cursor(NsProcView *v)
{
    if (!v->busy_cursor)
        return;
    v->busy_cursor = FALSE;
    if (v->area)
        gtk_widget_set_cursor_from_name(v->area, NULL);
}

static void
finish_loading(NsProcView *v)
{
    if (v->loading) {
        v->loading = FALSE;
        post_emit(v, NS_PROC_EVT_LOADING, "0");
    }
}

void
ns_proc_view_stop(NsProcView *v)
{
    if (!v)
        return;
    v->render_seq++;
    v->render_inflight = FALSE;
    v->render_pending = FALSE;
    finish_loading(v);
    clear_busy_cursor(v);
}

static gboolean on_result(gpointer data);

static void
post(Res *res)
{
    g_idle_add(on_result, res);
}

NsProcView *
ns_proc_view_ref(NsProcView *v)
{
    return pv_ref(v);
}

void
ns_proc_view_unref(NsProcView *v)
{
    pv_unref(v);
}

GtkWidget *
ns_proc_view_area(NsProcView *v)
{
    return v->area;
}

NsPvMedia *
ns_proc_view_media(NsProcView *v)
{
    return v ? v->media : NULL;
}

void
ns_proc_view_request_tick(NsProcView *v)
{
    request_tick(v);
}

void
ns_proc_view_request_render(NsProcView *v)
{
    request_render(v);
}

void
ns_proc_view_push_video_event(NsProcView *v, const char *token,
                              const char *kind)
{
    Req *req = g_new0(Req, 1);
    req->type = REQ_VIDEO_EVENT;
    req->seq = v->load_seq;
    req->key = g_strdup(token);
    req->query = g_strdup(kind);
    g_async_queue_push_front(v->queue, req);
}

static gboolean
pv_video_event_render(gpointer data)
{
    NsProcView *v = data;
    request_tick(v);
    pv_unref(v);
    return G_SOURCE_REMOVE;
}

static gpointer
worker_main(gpointer data)
{
    NsProcView *v = data;
    for (;;) {
        Req *req = g_async_queue_pop(v->queue);
        if (req->type == REQ_QUIT) {
            g_free(req->url);
            g_free(req);
            break;
        }
        if (!v->proc && !v->closed)
            pv_swap_proc(v, ns_rproc_http_spawn_shm_ex(v->renderer_path,
                                     NS_PROC_MAX_WIDTH, NS_PROC_MAX_HEIGHT,
                                     v->private_mode));
        if (v->proc && req->dpr > 0)
            ns_rproc_http_set_device_pixel_ratio(v->proc, req->dpr);

        if (req->type == REQ_LOAD) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_PAGE;
            res->seq = req->seq;
            ns_rproc_http_page pg;
            int settle = pv_settle_ms();
            int rc = v->proc ? ns_rproc_http_open_ex(v->proc, req->url, req->vw,
                                             req->vh, settle, req->history,
                                             req->user_activated, &pg)
                             : -1;
            if (rc != 0 && v->proc && !v->closed) {
                ns_rproc_http_close(pv_swap_proc(v, NULL));
                pv_swap_proc(v, ns_rproc_http_spawn_shm_ex(v->renderer_path,
                                         NS_PROC_MAX_WIDTH, NS_PROC_MAX_HEIGHT,
                                         v->private_mode));
                if (v->proc)
                    ns_rproc_http_set_device_pixel_ratio(v->proc, req->dpr);
                rc = v->proc ? ns_rproc_http_open_ex(v->proc, req->url, req->vw,
                                             req->vh, settle, req->history,
                                             req->user_activated, &pg)
                             : -1;
            }
            if (rc == 0 && pg.ok) {
                res->ok = TRUE;
                res->pw = pg.page_width;
                res->ph = pg.page_height;
                res->title = g_strdup(pg.title ? pg.title : "");
                res->url = g_strdup(pg.url ? pg.url : req->url);
                res->nav = pg.nav ? g_strdup(pg.nav) : NULL;
                res->security = pg.security;
                res->remote_ip = pg.remote_ip ? g_strdup(pg.remote_ip) : NULL;
            }
            if (rc == 0)
                ns_rproc_http_page_clear(&pg);
            post(res);
        } else if (req->type == REQ_RENDER) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_FRAME;
            res->seq = req->seq;
            ns_rproc_http_frame fr;
            req->tiles.have = req->have;
            req->tiles.hold = req->hold;
            gboolean rendered = v->proc &&
                (req->tiles.tile_h > 0
                     ? ns_rproc_http_render_tiles(v->proc, req->w, req->h,
                                                  req->sx, req->sy,
                                                  req->scale,
                                                  req->caret_active,
                                                  &req->wheel, &req->tiles,
                                                  &fr)
                     : ns_rproc_http_render_wheel(v->proc, req->w, req->h,
                                                  req->sx, req->sy,
                                                  req->scale,
                                                  req->caret_active,
                                                  &req->wheel, &fr)) == 0 &&
                fr.ok;
            if (rendered) {
                res->ok = TRUE;
                res->animating = fr.animating ? TRUE : FALSE;
                res->caret_blinking = fr.caret_blinking ? TRUE : FALSE;
                res->wheel_snapped = fr.wheel_snapped ? TRUE : FALSE;
                res->pw = fr.page_w;
                res->ph = fr.page_h;
                res->requested_scroll_y = fr.scroll_y;
                res->requested_scroll_x = fr.scroll_x;
                res->frame_unchanged = fr.unchanged ? TRUE : FALSE;
                if (fr.tiles) {
                    res->layers = ns_layer_update_parse(
                        fr.tiles, fr.pixels, ns_rproc_http_map_size(v->proc));
                    free(fr.tiles);
                    res->frame_unchanged = res->layers == NULL;
                } else if (!fr.unchanged) {
                    res->texture = stage_fill(v, fr.pixels, fr.width,
                                              fr.height, fr.stride);
                    res->texture_scale = req->raster > 0 ? req->raster : 1.0;
                }
                if (fr.nav) {
                    res->nav = g_strdup(fr.nav);
                    free(fr.nav);
                }
                if (fr.webgl) {
                    res->webgl = g_strdup(fr.webgl);
                    free(fr.webgl);
                }
                if (fr.camera) {
                    res->camera = g_strdup(fr.camera);
                    free(fr.camera);
                }
                if (fr.download) {
                    res->download = g_strdup(fr.download);
                    free(fr.download);
                }
                if (fr.audio) {
                    res->audio = g_strdup(fr.audio);
                    free(fr.audio);
                }
                if (fr.window_action) {
                    res->window_action = g_strdup(fr.window_action);
                    free(fr.window_action);
                }
                if (fr.clipboard && v->proc)
                    res->clipboard = ns_rproc_http_clipboard(v->proc);
            } else if (v->proc) {
                ns_rproc_http_close(pv_swap_proc(v, NULL));
            }
            post(res);
        } else if (req->type == REQ_TICK) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_TICK;
            res->seq = req->seq;
            ns_rproc_http_tick tick;
            if (v->proc && ns_rproc_http_tick_page(v->proc, &tick) == 0) {
                res->ok = tick.ok ? TRUE : FALSE;
                res->kind = tick.changed;
                res->animating = tick.animating ? TRUE : FALSE;
                res->pw = tick.page_w;
                res->ph = tick.page_h;
                res->nav = tick.nav ? g_strdup(tick.nav) : NULL;
                res->webgl = tick.webgl ? g_strdup(tick.webgl) : NULL;
                res->camera = tick.camera ? g_strdup(tick.camera) : NULL;
                res->download = tick.download ? g_strdup(tick.download) : NULL;
                res->audio = tick.audio ? g_strdup(tick.audio) : NULL;
                res->window_action = tick.window_action
                    ? g_strdup(tick.window_action) : NULL;
                res->title = tick.title ? g_strdup(tick.title) : NULL;
                res->url = tick.url ? g_strdup(tick.url) : NULL;
                res->url_pushed = tick.url_pushed ? TRUE : FALSE;
                ns_rproc_http_tick_clear(&tick);
            }
            post(res);
        } else if (req->type == REQ_LINK) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_LINK;
            res->seq = req->seq;
            res->action = req->action;
            if (v->proc && req->action == ACT_HOVER)
                res->href = ns_rproc_http_link_cursor_at(v->proc, req->x,
                                                         req->y, &res->cursor);
            else if (v->proc && req->action == ACT_CONTEXT) {
                int prevented = 0;
                ns_rproc_http_contextmenu(v->proc, req->x, req->y, &prevented,
                                          &res->edit);
                res->prevented = prevented;
                if (!prevented)
                    res->href = ns_rproc_http_link_at(v->proc, req->x, req->y);
            }
            else if (v->proc)
                res->href = ns_rproc_http_link_at(v->proc, req->x, req->y);
            post(res);
        } else if (req->type == REQ_CLICK) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_CLICK;
            res->seq = req->seq;
            res->href = v->proc
                ? ns_rproc_http_click(v->proc, req->x, req->y, req->mods)
                : NULL;
            post(res);
        } else if (req->type == REQ_VIEWPORT) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_VIEWPORT;
            res->seq = req->seq;
            ns_rproc_http_page pg;
            if (v->proc &&
                ns_rproc_http_set_viewport(v->proc, req->vw, req->vh, &pg) == 0) {
                res->ok = pg.ok;
                res->pw = pg.page_width;
                res->ph = pg.page_height;
                ns_rproc_http_page_clear(&pg);
            }
            post(res);
        } else if (req->type == REQ_KEY) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_KEY;
            res->seq = req->seq;
            res->kind = req->kind;
            res->fallback_scroll = req->fallback_scroll;
            res->fallback_x = req->fallback_x;
            res->fallback_y = req->fallback_y;
            res->href = v->proc
                ? ns_rproc_http_key_full(v->proc, req->kind, req->key,
                               req->code, req->keycode, req->mods,
                               &res->prevented)
                : NULL;
            post(res);
        } else if (req->type == REQ_SELECT) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = (req->kind == 4 || req->kind == 7) ? RES_COPY
                                                           : RES_SELECT;
            res->seq = req->seq;
            res->kind = req->kind;
            res->href = v->proc
                ? ns_rproc_http_select(v->proc, req->kind, req->x, req->y)
                : NULL;
            post(res);
        } else if (req->type == REQ_HOVER) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_HOVER;
            res->seq = req->seq;
            if (v->proc)
                res->ok = ns_rproc_http_hover_full(v->proc, req->x, req->y,
                                                   &res->href,
                                                   &res->cursor) == 1;
            post(res);
        } else if (req->type == REQ_SCROLLBAR) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_SCROLLBAR;
            res->seq = req->seq;
            res->kind = req->kind;
            res->ok = v->proc
                ? ns_rproc_http_scrollbar(v->proc, req->kind, req->x, req->y)
                : 0;
            post(res);
        } else if (req->type == REQ_DROPFILES) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_DROPFILES;
            res->seq = req->seq;
            if (v->proc && req->paths && *req->paths) {
                char **list = g_strsplit(req->paths, "\n", -1);
                guint count = list ? g_strv_length(list) : 0;
                if (count > 0)
                    res->ok = ns_rproc_http_drop_files(
                        v->proc, req->x, req->y,
                        (const char *const *)list, (int)count) == 1;
                g_strfreev(list);
            }
            post(res);
        } else if (req->type == REQ_RELEASE) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_RELEASE;
            res->seq = req->seq;
            res->href = v->proc
                ? ns_rproc_http_release_full(v->proc, &res->ok)
                : NULL;
            if (v->proc && (!res->href || !*res->href))
                res->media_url = ns_rproc_http_media_at(v->proc, req->x, req->y,
                                                   &res->media_is_video,
                                                   &res->media_stream);
            post(res);
        } else if (req->type == REQ_FIND) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_FIND;
            res->seq = req->seq;
            if (v->proc)
                ns_rproc_http_find(v->proc, req->query, req->find_case,
                              req->find_dir, req->find_from_y,
                              &res->find_total, &res->find_current,
                              &res->find_scroll_y);
            post(res);
        } else if (req->type == REQ_EXPORT) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_EXPORT;
            res->seq = req->seq;
            gboolean ok = FALSE;
            if (v->proc && req->url && req->export_dest &&
                ns_rproc_http_export(v->proc, req->url) == 0) {
                GFile *src = g_file_new_for_path(req->url);
                GFile *dst = g_file_new_for_path(req->export_dest);
                if (g_file_query_file_type(src,
                                           G_FILE_QUERY_INFO_NOFOLLOW_SYMLINKS,
                                           NULL) == G_FILE_TYPE_REGULAR)
                    ok = g_file_copy(src, dst,
                                     G_FILE_COPY_OVERWRITE |
                                         G_FILE_COPY_NOFOLLOW_SYMLINKS,
                                     NULL, NULL, NULL, NULL);
                g_object_unref(src);
                g_object_unref(dst);
            }
            if (req->url)
                g_unlink(req->url);
            res->ok = ok;
            res->url = g_strdup(req->export_dest ? req->export_dest : "");
            post(res);
        } else if (req->type == REQ_CONSOLE) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_CONSOLE;
            res->seq = req->seq;
            res->href = v->proc ? ns_rproc_http_console_poll(v->proc) : NULL;
            post(res);
        } else if (req->type == REQ_EVAL) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_EVAL;
            res->seq = req->seq;
            res->dump_tab = req->dump_tab;
            res->inspect = req->inspect;
            res->href = v->proc ? ns_rproc_http_eval(v->proc, req->query) : NULL;
            post(res);
        } else if (req->type == REQ_DUMP) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_DUMP;
            res->seq = req->seq;
            res->dump_tab = req->dump_tab;
            res->href = v->proc ? ns_rproc_http_dump(v->proc, req->query) : NULL;
            post(res);
        } else if (req->type == REQ_WEBGL) {
            if (v->proc)
                ns_rproc_http_resolve_webgl(v->proc, req->url, req->mods);
        } else if (req->type == REQ_PRINT) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_PRINT;
            res->seq = req->seq;
            if (v->proc)
                res->print_pages = ns_rproc_http_print(v->proc,
                                                       &res->print_setup,
                                                       &res->print_scale);
            res->ok = res->print_pages != NULL;
            post(res);
        } else if (req->type == REQ_CAMERA) {
            if (v->proc)
                ns_rproc_http_resolve_camera(v->proc, req->url, req->mods);
        } else if (req->type == REQ_COLOR_SCHEME) {
            if (v->proc)
                ns_rproc_http_set_color_scheme(v->proc, req->mods);
        } else if (req->type == REQ_FAVICON) {
            Res *res = g_new0(Res, 1);
            res->view = pv_ref(v);
            res->type = RES_FAVICON;
            res->seq = req->seq;
            if (v->proc)
                res->favicon_data = ns_rproc_http_favicon(
                    v->proc, &res->favicon_w, &res->favicon_h,
                    &res->favicon_stride);
            post(res);
        } else if (req->type == REQ_VIDEO_EVENT) {
            int rc = v->proc && req->seq == v->load_seq
                ? ns_rproc_http_video_event(v->proc, req->key, req->query) : -1;
            if (rc == 0)
                g_idle_add(pv_video_event_render, pv_ref(v));
        }
        g_free(req->url);
        g_free(req->key);
        g_free(req->code);
        g_free(req->query);
        g_free(req->export_dest);
        g_free(req->paths);
        g_free(req->have);
        g_free(req->hold);
        g_free(req);
    }
    if (v->proc)
        ns_rproc_http_close(pv_swap_proc(v, NULL));
    pv_unref(v);
    return NULL;
}

static void
request_favicon(NsProcView *v)
{
    Req *req = g_new0(Req, 1);
    req->type = REQ_FAVICON;
    req->seq = v->load_seq;
    ns_pv_push_req(v, req);
}


static void
configure_adjustments(NsProcView *v)
{
    double s = ns_pv_cur_scale(v);
    double cw = ns_pv_viewport_w(v) / s;
    double ch = ns_pv_viewport_h(v) / s;
    gboolean can_scroll_x = v->page_w > cw + 0.5;
    gboolean can_scroll_y = v->page_h > ch + 0.5;
    if (!can_scroll_x) v->scroll_x = 0;
    if (!can_scroll_y) v->scroll_y = 0;
    if (v->hscroll) gtk_widget_set_visible(v->hscroll, can_scroll_x);
    if (v->vscroll) gtk_widget_set_visible(v->vscroll, can_scroll_y);
    double upper_w = v->page_w > cw ? v->page_w : cw;
    double upper_h = v->page_h > ch ? v->page_h : ch;
    gtk_adjustment_configure(v->hadj, v->scroll_x, 0, upper_w, 60, cw, cw);
    gtk_adjustment_configure(v->vadj, v->scroll_y, 0, upper_h, 60, ch, ch);
    v->scroll_x = (int)gtk_adjustment_get_value(v->hadj);
    v->scroll_y = (int)gtk_adjustment_get_value(v->vadj);
}

static void
on_adj_changed(GtkAdjustment *adj, gpointer data)
{
    (void)adj;
    NsProcView *v = data;
    if (v->closed)
        return;
    v->scroll_x = (int)gtk_adjustment_get_value(v->hadj);
    v->scroll_y = (int)gtk_adjustment_get_value(v->vadj);
    if (v->tiles_mode)
        gtk_widget_queue_draw(v->area);
    if (v->opened && !v->adopting_scroll)
        request_render(v);
}

static void start_render(NsProcView *v);

static gboolean
pv_tiles_enabled(void)
{
    static int enabled = -1;
    if (enabled < 0) {
        const char *e = g_getenv("NS_TILES");
        enabled = !(e && strcmp(e, "0") == 0);
    }
    return enabled;
}

static void
pv_tiles_range(NsProcView *v, double *y0, double *y1)
{
    double vh = ns_pv_viewport_h(v) / ns_pv_cur_scale(v);
    double ahead = vh * 1.5, behind = vh * 0.5;
    *y0 = MAX(v->scroll_y - (v->scroll_dir < 0 ? ahead : behind), 0);
    *y1 = v->scroll_y + vh + (v->scroll_dir < 0 ? behind : ahead);
}

static void
pv_tiles_request(NsProcView *v, Req *req)
{
    if (!pv_tiles_enabled())
        return;
    double y0, y1;
    pv_tiles_range(v, &y0, &y1);
    req->tiles.tile_h = NS_PV_TILE_H;
    req->tiles.want_y0 = (int)floor(y0);
    req->tiles.want_y1 = (int)ceil(y1);
    req->tiles.gen = ns_page_layers_gen(v->layers);
    req->tiles.vp_held = ns_page_layers_vp_held(v->layers);
    req->tiles.fill = v->tiles_fill;
    v->tiles_fill = FALSE;
    req->have = ns_page_layers_have(v->layers, y0, y1, FALSE);
    req->hold = ns_page_layers_have(v->layers, y0, y1, TRUE);
}

static gboolean
pv_adjustments_stale(NsProcView *v)
{
    double s = ns_pv_cur_scale(v);
    double cw = ns_pv_viewport_w(v) / s, ch = ns_pv_viewport_h(v) / s;
    return fabs(gtk_adjustment_get_upper(v->hadj) - MAX(v->page_w, cw)) > 0.5 ||
           fabs(gtk_adjustment_get_upper(v->vadj) - MAX(v->page_h, ch)) > 0.5;
}

static void
pv_tiles_apply(NsProcView *v, Res *res)
{
    gboolean changed = ns_page_layers_apply(v->layers, res->layers);
    if (!ns_page_layers_active(v->layers))
        return;
    if (!v->tiles_mode) {
        v->tiles_mode = TRUE;
        g_clear_object(&v->frame);
    }
    v->render_restarts = 0;
    clear_busy_cursor(v);
    double vh = ns_pv_viewport_h(v) / ns_pv_cur_scale(v);
    ns_page_layers_evict(v->layers, v->scroll_y - vh * 1.5,
                         v->scroll_y + vh * 2.5);
    if (res->ph > 0) {
        v->page_h = res->ph;
        if (res->pw > 0) v->page_w = res->pw;
    }
    if (pv_adjustments_stale(v)) {
        v->adopting_scroll = TRUE;
        configure_adjustments(v);
        v->adopting_scroll = FALSE;
    }
    double y0, y1;
    pv_tiles_range(v, &y0, &y1);
    if (changed) {
        gtk_widget_queue_draw(v->area);
        if (ns_page_layers_missing(v->layers, y0, y1, v->scroll_y,
                                   v->scroll_y + vh, v->page_h) &&
            !v->render_pending) {
            v->render_pending = TRUE;
            v->tiles_fill = TRUE;
        }
    }
}

static void
pv_tiles_leave(NsProcView *v)
{
    if (!v->tiles_mode)
        return;
    v->tiles_mode = FALSE;
    ns_page_layers_reset(v->layers);
}

static void
request_tick(NsProcView *v)
{
    if (!v->opened) return;
    if (v->tick_inflight) {
        v->tick_pending = TRUE;
        return;
    }
    v->tick_inflight = TRUE;
    Req *req = g_new0(Req, 1);
    req->type = REQ_TICK;
    req->seq = v->load_seq;
    g_async_queue_push_front(v->queue, req);
}

static void
request_render(NsProcView *v)
{
    if (!v->opened)
        return;
    v->tiles_fill = FALSE;
    if (v->render_inflight) {
        v->render_pending = TRUE;
        return;
    }
    start_render(v);
}

static void
start_render(NsProcView *v)
{
    if (!v->opened)
        return;
    v->render_inflight = TRUE;
    Req *req = g_new0(Req, 1);
    req->type = REQ_RENDER;
    req->seq = ++v->render_seq;
    req->raster = ns_pv_raster_scale(v);
    req->w = (int)ceil(ns_pv_viewport_w(v) * req->raster);
    req->h = (int)ceil(ns_pv_viewport_h(v) * req->raster);
    req->sx = v->scroll_x;
    req->sy = v->scroll_y;
    req->scale = ns_pv_cur_scale(v) * req->raster;
    req->dpr = ns_pv_page_dpr(v);
    req->caret_active = gtk_widget_has_focus(v->area);
    pv_tiles_request(v, req);
    req->wheel.dx = (int)v->wheel_pend_x;
    req->wheel.dy = (int)v->wheel_pend_y;
    if (req->wheel.dx || req->wheel.dy) {
        double s = ns_pv_cur_scale(v);
        v->wheel_pend_x -= req->wheel.dx;
        v->wheel_pend_y -= req->wheel.dy;
        req->wheel.x = v->scroll_x + (int)(v->pointer_x / s);
        req->wheel.y = v->scroll_y + (int)(v->pointer_y / s);
        req->wheel.viewport = v->wheel_viewport;
    }
    ns_pv_push_req(v, req);
}

static gboolean
hover_after_scroll(gpointer data)
{
    NsProcView *v = data;
    v->hover_after_scroll_id = 0;
    if (v->closed || !v->opened)
        return G_SOURCE_REMOVE;
    if (gtk_widget_contains(v->area, v->pointer_x, v->pointer_y)) {
        double s = ns_pv_cur_scale(v);
        ns_pv_request_hover(v, v->scroll_x + (int)(v->pointer_x / s),
                      v->scroll_y + (int)(v->pointer_y / s));
    }
    return G_SOURCE_REMOVE;
}

static void
pv_route_wheel(NsProcView *v, double dx, double dy)
{
    double s = ns_pv_cur_scale(v);
    v->wheel_remote = !v->wheel_viewport &&
        ns_page_layers_scroller_at(v->layers, v->pointer_x / s,
                                   v->pointer_y / s, v->scroll_x,
                                   v->scroll_y, fabs(dy) >= fabs(dx));
}

static int
pv_take_whole(double *pend)
{
    double r = round(*pend);
    int whole = fabs(*pend - r) < 1e-6 ? (int)r : (int)*pend;
    *pend -= whole;
    return whole;
}

static void
pv_scroll_local(NsProcView *v)
{
    int dx = pv_take_whole(&v->wheel_pend_x);
    int dy = pv_take_whole(&v->wheel_pend_y);
    if (!dx && !dy)
        return;
    if (dy)
        v->scroll_dir = dy > 0 ? 1 : -1;
    int want_x = v->scroll_x + dx, want_y = v->scroll_y + dy;
    gtk_adjustment_set_value(v->hadj, want_x);
    gtk_adjustment_set_value(v->vadj, want_y);
    if (v->scroll_x != want_x)
        v->wheel_left_x = v->fling_vx = v->wheel_pend_x = 0;
    if (v->scroll_y != want_y)
        v->wheel_left_y = v->fling_vy = v->wheel_pend_y = 0;
}

static void
queue_wheel_scroll(NsProcView *v, double dx, double dy)
{
    if (!v->opened)
        return;
    if (v->hover_after_scroll_id)
        g_source_remove(v->hover_after_scroll_id);
    else
        pv_route_wheel(v, dx, dy);
    v->hover_after_scroll_id =
        g_timeout_add(NS_PV_HOVER_AFTER_SCROLL_MS, hover_after_scroll, v);
    v->wheel_pend_x += dx;
    v->wheel_pend_y += dy;
    if (v->tiles_mode && !v->wheel_remote)
        pv_scroll_local(v);
    else if (fabs(v->wheel_pend_x) >= 1.0 || fabs(v->wheel_pend_y) >= 1.0)
        request_render(v);
}

static gboolean
wheel_animation_idle(const NsProcView *v)
{
    return v->wheel_left_x == 0 && v->wheel_left_y == 0 &&
           v->fling_vx == 0 && v->fling_vy == 0;
}

static gboolean
pv_wheel_scrolling(const NsProcView *v)
{
    return v->hover_after_scroll_id != 0 || !wheel_animation_idle(v) ||
           v->wheel_pend_x != 0 || v->wheel_pend_y != 0;
}

static gboolean
wheel_tick(GtkWidget *widget, GdkFrameClock *clock, gpointer data)
{
    (void)widget;
    NsProcView *v = data;
    if (v->closed || !v->opened || wheel_animation_idle(v)) {
        v->wheel_tick_id = 0;
        v->wheel_last_us = 0;
        return G_SOURCE_REMOVE;
    }
    gint64 now = gdk_frame_clock_get_frame_time(clock);
    double dt_ms = v->wheel_last_us > 0
        ? (double)(now - v->wheel_last_us) / 1000.0 : 1000.0 / 60.0;
    dt_ms = CLAMP(dt_ms, 1.0, 100.0);
    v->wheel_last_us = now;

    double share = 1.0 - exp(-dt_ms / NS_PV_WHEEL_TAU_MS);
    double mx = v->wheel_left_x * share;
    double my = v->wheel_left_y * share;
    if (fabs(v->wheel_left_x - mx) < 0.5) mx = v->wheel_left_x;
    if (fabs(v->wheel_left_y - my) < 0.5) my = v->wheel_left_y;
    v->wheel_left_x -= mx;
    v->wheel_left_y -= my;

    mx += v->fling_vx * dt_ms / 1000.0;
    my += v->fling_vy * dt_ms / 1000.0;
    double decay = exp(-dt_ms / NS_PV_FLING_TAU_MS);
    v->fling_vx *= decay;
    v->fling_vy *= decay;
    if (hypot(v->fling_vx, v->fling_vy) < NS_PV_FLING_STOP_PX_S)
        v->fling_vx = v->fling_vy = 0;

    queue_wheel_scroll(v, mx, my);
    return G_SOURCE_CONTINUE;
}

static void
arm_wheel_animation(NsProcView *v)
{
    if (v->wheel_tick_id || !v->area || wheel_animation_idle(v))
        return;
    v->wheel_last_us = 0;
    v->wheel_tick_id = gtk_widget_add_tick_callback(v->area, wheel_tick, v,
                                                    NULL);
}

static void
stop_wheel_animation(NsProcView *v)
{
    v->wheel_left_x = v->wheel_left_y = 0;
    v->wheel_pend_x = v->wheel_pend_y = 0;
    v->fling_vx = v->fling_vy = 0;
}

double
ns_pv_scroll_target_x(const NsProcView *v)
{
    return v->scroll_x + v->wheel_pend_x + v->wheel_left_x;
}

double
ns_pv_scroll_target_y(const NsProcView *v)
{
    return v->scroll_y + v->wheel_pend_y + v->wheel_left_y;
}

static void
scroll_view_to(NsProcView *v, double x, double y)
{
    double max_x = gtk_adjustment_get_upper(v->hadj) -
                   gtk_adjustment_get_page_size(v->hadj);
    double max_y = gtk_adjustment_get_upper(v->vadj) -
                   gtk_adjustment_get_page_size(v->vadj);
    x = CLAMP(x, 0, MAX(max_x, 0));
    y = CLAMP(y, 0, MAX(max_y, 0));
    v->fling_vx = v->fling_vy = 0;
    v->wheel_viewport = TRUE;
    v->wheel_left_x += x - ns_pv_scroll_target_x(v);
    v->wheel_left_y += y - ns_pv_scroll_target_y(v);
    arm_wheel_animation(v);
}

static void
on_device_scale_changed(GObject *object, GParamSpec *pspec, gpointer data)
{
    (void)object;
    (void)pspec;
    NsProcView *v = data;
    if (v->closed || !v->opened)
        return;
    if (!ns_pv_maybe_update_viewport(v))
        request_render(v);
}

static void
on_area_realize(GtkWidget *area, gpointer data)
{
    NsProcView *v = data;
    GtkNative *native = gtk_widget_get_native(area);
    GdkSurface *surface = native ? gtk_native_get_surface(native) : NULL;
    if (!surface || surface == v->scale_surface)
        return;
    if (v->scale_surface && v->surface_scale_handler)
        g_signal_handler_disconnect(v->scale_surface, v->surface_scale_handler);
    v->scale_surface = surface;
    v->surface_scale_handler =
        g_signal_connect(surface, "notify::scale",
                         G_CALLBACK(on_device_scale_changed), v);
}

static void
on_area_unrealize(GtkWidget *area, gpointer data)
{
    (void)area;
    NsProcView *v = data;
    if (v->scale_surface && v->surface_scale_handler)
        g_signal_handler_disconnect(v->scale_surface, v->surface_scale_handler);
    v->scale_surface = NULL;
    v->surface_scale_handler = 0;
}

static void
on_area_focus_notify(GObject *object, GParamSpec *pspec, gpointer data)
{
    (void)object;
    (void)pspec;
    NsProcView *v = data;
    v->last_anim_frame_us = 0;
    request_render(v);
}

static gboolean
anim_tick(GtkWidget *widget, GdkFrameClock *clock, gpointer data)
{
    (void)widget;
    (void)clock;
    NsProcView *v = data;
    if (v->closed || !v->opened) {
        v->anim_tick_id = 0;
        v->last_anim_frame_us = 0;
        return G_SOURCE_REMOVE;
    }
    if (ns_pv_media_video_playing(v->media))
        return G_SOURCE_CONTINUE;
    gint64 now = clock ? gdk_frame_clock_get_frame_time(clock) : 0;
    if (now <= 0) now = g_get_monotonic_time();
    gint64 interval = v->page_animating ? G_USEC_PER_SEC / 60
                                        : NS_PROC_CARET_BLINK_US;
    if (v->last_anim_frame_us > 0 && now - v->last_anim_frame_us < interval)
        return G_SOURCE_CONTINUE;
    v->last_anim_frame_us = now;
    if (v->page_animating && !pv_wheel_scrolling(v))
        request_tick(v);
    else
        request_render(v);
    return G_SOURCE_CONTINUE;
}

static void
arm_anim(NsProcView *v)
{
    if (v->anim_tick_id || v->closed || !v->area)
        return;
    v->last_anim_frame_us = 0;
    v->anim_tick_id = gtk_widget_add_tick_callback(v->area, anim_tick, v, NULL);
}

static void
disarm_anim(NsProcView *v)
{
    if (v->anim_tick_id && v->area)
        gtk_widget_remove_tick_callback(v->area, v->anim_tick_id);
    v->anim_tick_id = 0;
    v->last_anim_frame_us = 0;
}

static void show_context_menu(NsProcView *v, const char *href, int edit);
static void build_search_bar(NsProcView *v);
static void console_append(NsProcView *v, const char *text);

static void
push_history(NsProcView *v, const char *url)
{
    if (!url || !*url)
        return;
    if (v->hist_index >= 0 &&
        g_strcmp0(g_ptr_array_index(v->history, v->hist_index), url) == 0)
        return;
    while ((int)v->history->len > v->hist_index + 1)
        g_ptr_array_remove_index(v->history, v->history->len - 1);
    g_ptr_array_add(v->history, g_strdup(url));
    v->hist_index = (int)v->history->len - 1;
    post_emit(v, NS_PROC_EVT_HISTORY, NULL);
}

static void
pv_follow_same_document_state(NsProcView *v, const char *url, gboolean pushed,
                              const char *title)
{
    if (url && *url && g_strcmp0(url, v->current_url) != 0) {
        if (pushed || v->hist_index < 0) {
            push_history(v, url);
        } else {
            g_free(g_ptr_array_index(v->history, v->hist_index));
            g_ptr_array_index(v->history, v->hist_index) = g_strdup(url);
        }
        g_free(v->current_url);
        v->current_url = g_strdup(url);
        post_emit(v, NS_PROC_EVT_URL, v->current_url);
    }
    if (title && g_strcmp0(title, v->current_title) != 0) {
        g_free(v->current_title);
        v->current_title = g_strdup(title);
        post_emit(v, NS_PROC_EVT_TITLE, v->current_title);
    }
}

static void pv_perm_resolve(NsProcView *v, gboolean allow);

static void
do_load(NsProcView *v, const char *url, gboolean record, gboolean history,
        gboolean user_activated)
{
    if (!url || !*url)
        return;
    pv_perm_resolve(v, FALSE);
    ns_pv_media_audio_shutdown(v->media);
    ns_pv_media_video_shutdown(v->media);
    v->pending_record = record;
    int seq = ++v->load_seq;
    ++v->render_seq;
    ++v->link_seq;
    ++v->click_seq;
    ++v->viewport_seq;
    ++v->key_seq;
    ++v->select_seq;
    ++v->hover_seq;
    v->render_pending = FALSE;
    v->render_inflight = FALSE;
    v->tick_pending = FALSE;
    v->tick_inflight = FALSE;
    v->link_inflight = FALSE;
    v->link_pending = FALSE;
    v->link_pending_action = ACT_HOVER;
    v->hover_inflight = FALSE;
    v->hover_pending = FALSE;
    v->has_selection = FALSE;
    ++v->find_seq;
    if (v->search_revealer)
        gtk_revealer_set_reveal_child(GTK_REVEALER(v->search_revealer), FALSE);
    if (v->search_label)
        gtk_label_set_text(GTK_LABEL(v->search_label), "");
    v->opened = FALSE;
    v->page_animating = FALSE;
    v->caret_blinking = FALSE;
    stop_wheel_animation(v);
    disarm_anim(v);
    g_clear_object(&v->frame);
    ns_page_layers_reset(v->layers);
    v->tiles_mode = FALSE;
    gtk_widget_queue_draw(v->area);
    if (!v->loading) {
        v->loading = TRUE;
        post_emit(v, NS_PROC_EVT_LOADING, "1");
    }
    set_busy_cursor(v);
    post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Loading…"));

    int vw = gtk_widget_get_width(v->area);
    int vh = gtk_widget_get_height(v->area);
    if (vw <= 1 || vh <= 1) {
        g_free(v->deferred_url);
        v->deferred_url = g_strdup(url);
        v->deferred_record = record;
        v->deferred_history = history;
        v->deferred_user_activated = user_activated;
        return;
    }
    v->last_vp_w = ns_pv_css_viewport_w(v);
    v->last_vp_h = ns_pv_css_viewport_h(v);
    v->last_vp_dpr = ns_pv_page_dpr(v);

    Req *req = g_new0(Req, 1);
    req->type = REQ_LOAD;
    req->seq = seq;
    req->url = g_strdup(url);
    req->vw = v->last_vp_w;
    req->vh = v->last_vp_h;
    req->dpr = v->last_vp_dpr;
    req->history = history;
    req->user_activated = user_activated;
    ns_pv_push_req(v, req);
}

void
ns_proc_view_load(NsProcView *v, const char *url)
{
    v->render_restarts = 0;
    do_load(v, url, TRUE, FALSE, TRUE);
}

gboolean ns_proc_view_can_back(NsProcView *v) { return v->hist_index > 0; }

gboolean
ns_proc_view_can_forward(NsProcView *v)
{
    return v->hist_index >= 0 && v->hist_index < (int)v->history->len - 1;
}

void
ns_proc_view_back(NsProcView *v)
{
    if (!ns_proc_view_can_back(v))
        return;
    v->hist_index--;
    v->render_restarts = 0;
    post_emit(v, NS_PROC_EVT_HISTORY, NULL);
    do_load(v, g_ptr_array_index(v->history, v->hist_index), FALSE, TRUE, TRUE);
}

void
ns_proc_view_forward(NsProcView *v)
{
    if (!ns_proc_view_can_forward(v))
        return;
    v->hist_index++;
    v->render_restarts = 0;
    post_emit(v, NS_PROC_EVT_HISTORY, NULL);
    do_load(v, g_ptr_array_index(v->history, v->hist_index), FALSE, TRUE, TRUE);
}

void
ns_proc_view_reload(NsProcView *v)
{
    v->render_restarts = 0;
    if (v->hist_index >= 0 && v->hist_index < (int)v->history->len)
        do_load(v, g_ptr_array_index(v->history, v->hist_index), FALSE, FALSE,
                TRUE);
    else if (v->current_url)
        do_load(v, v->current_url, FALSE, FALSE, TRUE);
}

void
ns_proc_view_toggle_console(NsProcView *v)
{
    if (v->opened)
        ns_pv_console_set_open(v, !v->console_open);
}

void
ns_proc_view_exit_fullscreen(NsProcView *v)
{
    if (!v || v->closed || !v->opened) return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_EVAL;
    req->query = g_strdup("document.exitFullscreen()");
    ns_pv_push_req(v, req);
}

const char *ns_proc_view_url(NsProcView *v) { return v->current_url; }
const char *ns_proc_view_title(NsProcView *v) { return v->current_title; }
int ns_proc_view_security(NsProcView *v) { return v ? v->security : 0; }
const char *ns_proc_view_remote_ip(NsProcView *v) { return v ? v->remote_ip : NULL; }
gboolean ns_proc_view_is_loading(NsProcView *v) { return v->loading; }
GdkPaintable *ns_proc_view_favicon(NsProcView *v) { return v ? v->favicon : NULL; }

int
ns_proc_view_renderer_pid(NsProcView *v)
{
    if (!v) return -1;
    g_mutex_lock(&v->proc_lock);
    int pid = v->proc ? ns_rproc_http_pid(v->proc) : -1;
    g_mutex_unlock(&v->proc_lock);
    return pid;
}

int
ns_proc_view_audio_pid(NsProcView *v)
{
    return v ? ns_pv_media_audio_pid(v->media) : -1;
}

int
ns_proc_view_video_pid(NsProcView *v)
{
    return v ? ns_pv_media_video_pid(v->media) : -1;
}

void
ns_proc_view_end_task(NsProcView *v)
{
    if (!v) return;
    g_mutex_lock(&v->proc_lock);
    if (v->proc) {
        ns_rproc_http_interrupt(v->proc);
        ns_rproc_http_terminate(v->proc);
    }
    g_mutex_unlock(&v->proc_lock);
}

void
ns_proc_view_stop_video(NsProcView *v)
{
    if (v) ns_pv_media_video_shutdown(v->media);
}

void
ns_proc_view_stop_audio(NsProcView *v)
{
    if (v) ns_pv_media_audio_shutdown(v->media);
}

void ns_proc_view_focus(NsProcView *v)
{
    if (v->area)
        gtk_widget_grab_focus(v->area);
}

static void
set_zoom(NsProcView *v, double scale)
{
    int permille = (int)(scale * 1000.0 + 0.5);
    if (permille < NS_PV_ZOOM_MIN_PERMILLE)
        permille = NS_PV_ZOOM_MIN_PERMILLE;
    if (permille > NS_PV_ZOOM_MAX_PERMILLE)
        permille = NS_PV_ZOOM_MAX_PERMILLE;
    double clamped = permille / 1000.0;
    if (clamped == ns_pv_cur_scale(v))
        return;
    v->scale = clamped;
    char percent[16];
    g_snprintf(percent, sizeof percent, "%d", permille / 10);
    post_emit(v, NS_PROC_EVT_ZOOM, percent);
    if (v->opened && !ns_pv_maybe_update_viewport(v)) {
        configure_adjustments(v);
        request_render(v);
    }
}

static const int k_zoom_ladder_percent[] = {
    25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300,
    400, 500
};

static void
zoom_step(NsProcView *v, int direction)
{
    int now = (int)(ns_pv_cur_scale(v) * 100.0 + 0.5);
    gsize n = G_N_ELEMENTS(k_zoom_ladder_percent);
    if (direction > 0) {
        for (gsize i = 0; i < n; i++)
            if (k_zoom_ladder_percent[i] > now) {
                set_zoom(v, k_zoom_ladder_percent[i] / 100.0);
                return;
            }
        set_zoom(v, k_zoom_ladder_percent[n - 1] / 100.0);
        return;
    }
    for (gsize i = n; i-- > 0; )
        if (k_zoom_ladder_percent[i] < now) {
            set_zoom(v, k_zoom_ladder_percent[i] / 100.0);
            return;
        }
    set_zoom(v, k_zoom_ladder_percent[0] / 100.0);
}

void ns_proc_view_zoom_in(NsProcView *v)  { zoom_step(v, 1); }
void ns_proc_view_zoom_out(NsProcView *v) { zoom_step(v, -1); }
void ns_proc_view_zoom_reset(NsProcView *v) { set_zoom(v, 1.0); }

int
ns_proc_view_zoom_percent(NsProcView *v)
{
    return v ? (int)(ns_pv_cur_scale(v) * 100.0 + 0.5) : 100;
}

static void
pv_perm_resolve(NsProcView *v, gboolean allow)
{
    if (!v->perm_pending)
        return;
    v->perm_pending = FALSE;
    ReqType kind = v->perm_kind;
    char *origin = v->perm_origin;
    v->perm_origin = NULL;
    if (v->perm_revealer)
        gtk_revealer_set_reveal_child(GTK_REVEALER(v->perm_revealer), FALSE);
    if (!v->closed) {
        Req *req = g_new0(Req, 1);
        req->type = kind;
        req->url = g_strdup(origin);
        req->mods = allow ? 1 : 0;
        ns_pv_push_req(v, req);
        if (kind == REQ_WEBGL && allow && v->current_url)
            do_load(v, v->current_url, FALSE, FALSE, TRUE);
    }
    g_free(origin);
}

static void
on_perm_allow(GtkButton *btn, gpointer data)
{
    (void)btn;
    pv_perm_resolve(data, TRUE);
}

static void
on_perm_deny(GtkButton *btn, gpointer data)
{
    (void)btn;
    pv_perm_resolve(data, FALSE);
}

static void
pv_perm_bar_show(NsProcView *v, ReqType kind, const char *origin)
{
    if (v->perm_pending && v->perm_kind == kind &&
        g_strcmp0(v->perm_origin, origin) == 0)
        return;
    if (v->perm_pending)
        pv_perm_resolve(v, FALSE);
    v->perm_pending = TRUE;
    v->perm_kind = kind;
    v->perm_origin = g_strdup(origin);
    const char *what =
        kind == REQ_WEBGL
            ? ns_i18n("This site wants to use WebGL (3D graphics)")
            : ns_i18n("This site wants to use your camera and microphone");
    char *text = g_strdup_printf("%s — %s", what, origin);
    gtk_label_set_text(GTK_LABEL(v->perm_label), text);
    g_free(text);
    gtk_revealer_set_reveal_child(GTK_REVEALER(v->perm_revealer), TRUE);
}

static void
pv_apply_window_action(NsProcView *v, const char *action)
{
    if (!v || !action) return;
    if (strcmp(action, "fullscreen-enter") == 0 ||
        strcmp(action, "fullscreen-exit") == 0)
        post_emit(v, NS_PROC_EVT_FULLSCREEN, action);
}

static gboolean
on_result(gpointer data)
{
    Res *res = data;
    NsProcView *v = res->view;

    if (v->closed)
        goto done;

    if (res->type == RES_PAGE) {
        if (res->seq != v->load_seq)
            goto done;
        if (!res->ok) {
            post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Failed to load page"));
            finish_loading(v);
            clear_busy_cursor(v);
            goto done;
        }
        if (res->nav && *res->nav) {
            if (v->js_redirects < NS_PROC_MAX_JS_REDIRECTS) {
                v->js_redirects++;
                do_load(v, res->nav, v->pending_record, FALSE, FALSE);
                goto done;
            }
            post_emit(v, NS_PROC_EVT_STATUS,
                      ns_i18n("Stopped after too many redirects"));
        }
        v->js_redirects = 0;
        g_free(v->current_url);
        v->current_url = g_strdup(res->url);
        g_free(v->current_title);
        v->current_title = g_strdup(res->title);
        v->security = res->security;
        g_free(v->remote_ip);
        v->remote_ip = res->remote_ip ? g_strdup(res->remote_ip) : NULL;
        v->page_w = res->pw;
        v->page_h = res->ph;
        v->scroll_x = 0;
        v->scroll_y = 0;
        v->opened = TRUE;
        configure_adjustments(v);
        if (v->pending_record)
            push_history(v, v->current_url);
        post_emit(v, NS_PROC_EVT_URL, v->current_url);
        post_emit(v, NS_PROC_EVT_TITLE, v->current_title);
        post_emit(v, NS_PROC_EVT_STATUS, "");
        finish_loading(v);
        request_render(v);
        request_favicon(v);
    } else if (res->type == RES_TICK) {
        gboolean current = res->seq == v->load_seq;
        gboolean navigated = FALSE;
        if (current && res->ok && res->nav && *res->nav &&
            v->js_redirects < NS_PROC_MAX_JS_REDIRECTS) {
            v->js_redirects++;
            do_load(v, res->nav, FALSE, FALSE, FALSE);
            navigated = TRUE;
        }
        if (current && res->ok && res->webgl && *res->webgl)
            post_emit(v, NS_PROC_EVT_WEBGL, "1");
        if (current && res->ok && res->camera && *res->camera)
            pv_perm_bar_show(v, REQ_CAMERA, res->camera);
        if (current && res->ok && res->download && *res->download)
            post_emit(v, NS_PROC_EVT_DOWNLOAD, res->download);
        if (current && res->ok && res->audio && *res->audio)
            ns_pv_media_pump(v, res->audio);
        if (current && res->ok && res->window_action && *res->window_action)
            pv_apply_window_action(v, res->window_action);
        if (current && res->ok && !navigated)
            pv_follow_same_document_state(v, res->url, res->url_pushed,
                                          res->title);
        if (current && res->ok) {
            v->page_animating = res->animating;
            if (v->page_animating || v->caret_blinking)
                arm_anim(v);
            else
                disarm_anim(v);
            if (res->ph > 0 && res->ph != v->page_h) {
                v->page_h = res->ph;
                if (res->pw > 0) v->page_w = res->pw;
                configure_adjustments(v);
            }
            if (res->kind && !ns_pv_media_video_playing(v->media))
                request_render(v);
        }
        v->tick_inflight = FALSE;
        if (!navigated && v->tick_pending && v->opened) {
            v->tick_pending = FALSE;
            request_tick(v);
        }
    } else if (res->type == RES_FRAME) {
        gboolean current = res->seq == v->render_seq;
        if (res->ok && res->wheel_snapped)
            stop_wheel_animation(v);
        if (current && res->ok) {
            v->page_animating = res->animating;
            v->caret_blinking = res->caret_blinking;
            if (v->page_animating || v->caret_blinking)
                arm_anim(v);
            else
                disarm_anim(v);
            if (res->ph > 0 && res->ph != v->page_h) {
                v->page_h = res->ph;
                if (res->pw > 0) v->page_w = res->pw;
                gtk_widget_queue_draw(v->area);
            }
            if (res->requested_scroll_y >= 0 || res->requested_scroll_x >= 0) {
                v->adopting_scroll = TRUE;
                configure_adjustments(v);
                if (res->requested_scroll_y >= 0)
                    gtk_adjustment_set_value(v->vadj,
                                             res->requested_scroll_y);
                if (res->requested_scroll_x >= 0)
                    gtk_adjustment_set_value(v->hadj,
                                             res->requested_scroll_x);
                v->adopting_scroll = FALSE;
                v->scroll_x = (int)gtk_adjustment_get_value(v->hadj);
                v->scroll_y = (int)gtk_adjustment_get_value(v->vadj);
            }
        }
        if (current && res->ok && res->layers) {
            pv_tiles_apply(v, res);
        } else if (current && res->ok && res->texture) {
            pv_tiles_leave(v);
            g_clear_object(&v->frame);
            v->frame = g_steal_pointer(&res->texture);
            v->frame_scale = res->texture_scale;
            v->render_restarts = 0;
            gtk_widget_queue_draw(v->area);
            clear_busy_cursor(v);
        } else if (current && res->ok && res->frame_unchanged) {
            v->render_restarts = 0;
        }
        if (current && res->ok && res->nav && *res->nav &&
            v->js_redirects < NS_PROC_MAX_JS_REDIRECTS) {
            v->js_redirects++;
            do_load(v, res->nav, FALSE, FALSE, FALSE);
        }
        if (res->ok && res->webgl && *res->webgl)
            post_emit(v, NS_PROC_EVT_WEBGL, "1");
        if (res->ok && res->camera && *res->camera)
            pv_perm_bar_show(v, REQ_CAMERA, res->camera);
        if (res->ok && res->download && *res->download)
            post_emit(v, NS_PROC_EVT_DOWNLOAD, res->download);
        if (res->ok && res->audio && *res->audio)
            ns_pv_media_pump(v, res->audio);
        if (res->ok && res->window_action && *res->window_action)
            pv_apply_window_action(v, res->window_action);
        if (res->ok && res->clipboard && v->area) {
            gdk_clipboard_set_text(gtk_widget_get_clipboard(v->area),
                                   res->clipboard);
            post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Copied to clipboard"));
        }
        v->render_inflight = FALSE;
        if (v->render_pending) {
            v->render_pending = FALSE;
            start_render(v);
        } else if (current && !res->ok && v->current_url) {
            if (v->render_restarts < NS_PROC_MAX_RESTARTS) {
                v->render_restarts++;
                post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Renderer restarted"));
                do_load(v, v->current_url, FALSE, FALSE, FALSE);
            } else {
                post_emit(v, NS_PROC_EVT_STATUS,
                          ns_i18n("This tab's renderer keeps failing — "
                                  "reload to retry"));
                finish_loading(v);
                clear_busy_cursor(v);
            }
        }
    } else if (res->type == RES_VIEWPORT) {
        if (res->seq != v->viewport_seq)
            goto done;
        if (res->ok) {
            v->page_w = res->pw;
            v->page_h = res->ph;
            configure_adjustments(v);
            request_render(v);
        }
    } else if (res->type == RES_SELECT) {
        if (res->seq == v->select_seq)
            request_render(v);
    } else if (res->type == RES_COPY) {
        if (res->href && *res->href && v->area) {
            gdk_clipboard_set_text(gtk_widget_get_clipboard(v->area),
                                   res->href);
            post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Copied selection"));
        }
        if (res->kind == 7)
            request_render(v);
    } else if (res->type == RES_KEY) {
        if (res->seq != v->key_seq)
            goto done;
        if (res->kind == 0 && res->href && *res->href) {
            post_emit(v, NS_PROC_EVT_STATUS, res->href);
            ns_proc_view_load(v, res->href);
        } else {
            if (res->fallback_scroll && !res->prevented)
                scroll_view_to(v, res->fallback_x, res->fallback_y);
            request_render(v);
        }
    } else if (res->type == RES_CLICK) {
        if (res->seq != v->click_seq)
            goto done;
        if (res->href && *res->href) {
            post_emit(v, NS_PROC_EVT_STATUS, res->href);
            ns_proc_view_load(v, res->href);
        } else {
            request_render(v);
        }
    } else if (res->type == RES_LINK) {
        if (res->seq != v->link_seq)
            goto done;
        v->link_inflight = FALSE;
        gboolean navigated = FALSE;
        GtkWidget *area = v->area;
        if (res->action == ACT_CONTEXT) {
            if (res->prevented || res->edit)
                request_render(v);
            if (!res->prevented)
                show_context_menu(v, res->href, res->edit);
            if (v->link_pending) {
                v->link_pending = FALSE;
                LinkAct a = v->link_pending_action;
                v->link_pending_action = ACT_HOVER;
                ns_pv_start_link(v, v->link_pending_x, v->link_pending_y, a);
            }
            goto done;
        }
        if (res->href && *res->href) {
            post_emit(v, NS_PROC_EVT_STATUS, res->href);
            if (!v->busy_cursor)
                pv_set_named_cursor(area, res->cursor ? res->cursor : "pointer");
            if (res->action == ACT_NAVIGATE) {
                navigated = TRUE;
                ns_proc_view_load(v, res->href);
            } else if (res->action == ACT_NEWTAB) {
                post_emit(v, NS_PROC_EVT_NEWTAB, res->href);
            }
        } else {
            post_emit(v, NS_PROC_EVT_STATUS, "");
            if (!v->busy_cursor)
                pv_set_named_cursor(area, res->cursor);
        }
        if (!navigated && v->link_pending) {
            v->link_pending = FALSE;
            LinkAct a = v->link_pending_action;
            v->link_pending_action = ACT_HOVER;
            ns_pv_start_link(v, v->link_pending_x, v->link_pending_y, a);
        }
    } else if (res->type == RES_HOVER) {
        if (res->seq != v->hover_seq)
            goto done;
        v->hover_inflight = FALSE;
        if (res->href && *res->href) {
            post_emit(v, NS_PROC_EVT_STATUS, res->href);
            if (!v->busy_cursor)
                pv_set_named_cursor(v->area, res->cursor ? res->cursor : "pointer");
        } else {
            post_emit(v, NS_PROC_EVT_STATUS, "");
            if (!v->busy_cursor)
                pv_set_named_cursor(v->area, res->cursor);
        }
        if (res->ok)
            request_render(v);
        if (v->hover_pending) {
            v->hover_pending = FALSE;
            ns_pv_start_hover(v, v->hover_pending_x, v->hover_pending_y);
        }
    } else if (res->type == RES_DROPFILES) {
        if (res->ok)
            request_render(v);
    } else if (res->type == RES_SCROLLBAR) {
        if (res->kind == 0) {
            gboolean still = v->sb_probe;
            v->sb_probe = FALSE;
            if (res->ok) {
                request_render(v);
                if (still) {
                    v->sb_dragging = TRUE;
                    if (v->sb_have_last) {
                        double s = ns_pv_cur_scale(v);
                        Req *req = g_new0(Req, 1);
                        req->type = REQ_SCROLLBAR;
                        req->kind = 1;
                        req->x = v->scroll_x + (int)(v->sb_last_x / s);
                        req->y = v->scroll_y + (int)(v->sb_last_y / s);
                        ns_pv_push_req(v, req);
                    }
                }
            } else if (still && v->sb_have_last) {
                double s = ns_pv_cur_scale(v);
                if (!v->multi_click) {
                    ns_pv_start_select(v, 0,
                                 v->scroll_x + (int)(v->drag_start_x / s),
                                 v->scroll_y + (int)(v->drag_start_y / s));
                    v->drag_anchored = TRUE;
                }
                ns_pv_start_select(v, 1, v->scroll_x + (int)(v->sb_last_x / s),
                             v->scroll_y + (int)(v->sb_last_y / s));
                v->has_selection = TRUE;
            }
        } else if (res->kind == 1) {
            if (res->ok)
                request_render(v);
        }
    } else if (res->type == RES_RELEASE) {
        if (res->href && *res->href) {
            post_emit(v, NS_PROC_EVT_STATUS, res->href);
            ns_proc_view_load(v, res->href);
        } else if (res->ok) {
            request_render(v);
        }
    } else if (res->type == RES_FIND) {
        if (res->seq != v->find_seq)
            goto done;
        if (v->search_label) {
            const char *q = v->search_entry
                ? gtk_editable_get_text(GTK_EDITABLE(v->search_entry)) : NULL;
            if (res->find_total > 0) {
                char buf[64];
                g_snprintf(buf, sizeof buf, "%d/%d", res->find_current,
                           res->find_total);
                gtk_label_set_text(GTK_LABEL(v->search_label), buf);
            } else {
                gtk_label_set_text(GTK_LABEL(v->search_label),
                                   (q && *q) ? ns_i18n("No results") : "");
            }
        }
        if (res->find_total > 0) {
            double target = res->find_scroll_y > 40 ? res->find_scroll_y - 40
                                                    : 0;
            gtk_adjustment_set_value(v->vadj, target);
        }
        request_render(v);
    } else if (res->type == RES_EXPORT) {
        if (res->ok && res->url)
            post_emit(v, NS_PROC_EVT_STATUS, res->url);
        else
            post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Could not save page"));
    } else if (res->type == RES_PRINT) {
        if (res->print_pages && res->print_pages->len > 0) {
            print_run(v, res->print_pages, &res->print_setup,
                      res->print_scale);
            res->print_pages = NULL;
        } else {
            post_emit(v, NS_PROC_EVT_STATUS, ns_i18n("Nothing to print"));
        }
        request_render(v);
    } else if (res->type == RES_CONSOLE) {
        if (res->href && *res->href)
            console_append(v, res->href);
    } else if (res->type == RES_EVAL) {
        if (res->inspect) {
            if (v->elements_buffer)
                gtk_text_buffer_set_text(
                    v->elements_buffer,
                    (res->href && *res->href) ? res->href
                                              : ns_i18n("No matching element"),
                    -1);
        } else if (res->href && *res->href) {
            console_append(v, res->href);
            console_append(v, "\n");
        } else {
            console_append(v, "undefined\n");
        }
        request_render(v);
    } else if (res->type == RES_DUMP) {
        GtkTextBuffer *buf = NULL;
        switch (res->dump_tab) {
        case DEV_TAB_NETWORK:     buf = v->net_buffer;      break;
        case DEV_TAB_PERFORMANCE: buf = v->perf_buffer;     break;
        case DEV_TAB_LAYOUT:      buf = v->layout_buffer;   break;
        case DEV_TAB_ELEMENTS:    buf = v->elements_buffer; break;
        default: break;
        }
        if (buf == v->perf_buffer) {
            GString *text = g_string_new(
                (res->href && *res->href) ? res->href : ns_i18n("(empty)"));
            ns_pv_media_append_stats(v->media, text);
            gtk_text_buffer_set_text(buf, text->str, -1);
            g_string_free(text, TRUE);
        } else if (buf) {
            gtk_text_buffer_set_text(
                buf, (res->href && *res->href) ? res->href : ns_i18n("(empty)"),
                -1);
        }
    } else if (res->type == RES_FAVICON) {
        if (res->seq != v->load_seq)
            goto done;
        g_clear_object(&v->favicon);
        if (res->favicon_data && res->favicon_w > 0 && res->favicon_h > 0) {
            gsize len = (gsize)res->favicon_stride * (gsize)res->favicon_h;
            GBytes *bytes = g_bytes_new(res->favicon_data, len);
            GdkTexture *tex = gdk_memory_texture_new(
                res->favicon_w, res->favicon_h,
                GDK_MEMORY_B8G8R8A8_PREMULTIPLIED, bytes, res->favicon_stride);
            g_bytes_unref(bytes);
            v->favicon = GDK_PAINTABLE(tex);
        }
        post_emit(v, NS_PROC_EVT_FAVICON, NULL);
    }

done:
    g_clear_object(&res->texture);
    ns_layer_update_free(res->layers);
    g_free(res->title);
    g_free(res->url);
    g_free(res->nav);
    g_free(res->remote_ip);
    g_free(res->webgl);
    g_free(res->camera);
    g_free(res->download);
    g_free(res->audio);
    free(res->clipboard);
    g_free(res->window_action);
    free(res->href);
    free(res->cursor);
    free(res->media_url);
    free(res->favicon_data);
    pv_unref(res->view);
    g_free(res);
    return G_SOURCE_REMOVE;
}

G_DECLARE_FINAL_TYPE(NsProcViewArea, ns_proc_view_area, NS, PROC_VIEW_AREA,
                     GtkDrawingArea)

struct _NsProcViewArea {
    GtkDrawingArea parent_instance;
    NsProcView    *view;
};

G_DEFINE_TYPE(NsProcViewArea, ns_proc_view_area, GTK_TYPE_DRAWING_AREA)

static gboolean
pv_frame_covers(NsProcView *v, double fs, const graphene_rect_t *area)
{
    return v->frame &&
        gdk_texture_get_width(v->frame) / fs >= area->size.width - 0.5 &&
        gdk_texture_get_height(v->frame) / fs >= area->size.height - 0.5;
}

static void
pv_snapshot_page_video(GtkSnapshot *snapshot, double offset_x,
                       double offset_y, gpointer data)
{
    NsProcView *v = data;
    if (!ns_pv_media_on_page(v->media))
        return;
    double fs = ns_pv_raster_scale(v);
    gtk_snapshot_save(snapshot);
    gtk_snapshot_translate(snapshot,
                           &GRAPHENE_POINT_INIT((float)(-offset_x / fs),
                                                (float)(-offset_y / fs)));
    ns_pv_media_snapshot(v->media, fs, snapshot);
    gtk_snapshot_restore(snapshot);
}

static void
ns_proc_view_area_snapshot(GtkWidget *widget, GtkSnapshot *snapshot)
{
    NsProcView *v = NS_PROC_VIEW_AREA(widget)->view;
    if (!v)
        return;
    if (v->tiles_mode) {
        ns_page_layers_snapshot(v->layers, snapshot,
                                gtk_widget_get_width(widget),
                                gtk_widget_get_height(widget),
                                ns_pv_raster_scale(v), v->scroll_x, v->scroll_y,
                                pv_snapshot_page_video, v);
        return;
    }
    double fs = v->frame_scale > 0 ? v->frame_scale : 1.0;
    graphene_rect_t area = GRAPHENE_RECT_INIT(
        0, 0, gtk_widget_get_width(widget), gtk_widget_get_height(widget));
    if (!pv_frame_covers(v, fs, &area)) {
        GdkRGBA white = { 1.0f, 1.0f, 1.0f, 1.0f };
        gtk_snapshot_append_color(snapshot, &white, &area);
    }
    if (!ns_pv_media_on_page(v->media))
        ns_pv_media_snapshot(v->media, fs, snapshot);
    if (!v->frame)
        return;
    graphene_rect_t bounds = GRAPHENE_RECT_INIT(
        0, 0, gdk_texture_get_width(v->frame) / fs,
        gdk_texture_get_height(v->frame) / fs);
    gtk_snapshot_push_clip(snapshot, &area);
    gtk_snapshot_append_texture(snapshot, v->frame, &bounds);
    gtk_snapshot_pop(snapshot);
}

static void
ns_proc_view_area_class_init(NsProcViewAreaClass *klass)
{
    GTK_WIDGET_CLASS(klass)->snapshot = ns_proc_view_area_snapshot;
}

static void
ns_proc_view_area_init(NsProcViewArea *self)
{
    self->view = NULL;
}

static void
on_resize(GtkDrawingArea *area, int width, int height, gpointer data)
{
    (void)area;
    NsProcView *v = data;
    if (v->deferred_url && width > 1 && height > 1) {
        char *u = v->deferred_url;
        gboolean rec = v->deferred_record;
        gboolean hist = v->deferred_history;
        gboolean activated = v->deferred_user_activated;
        v->deferred_url = NULL;
        do_load(v, u, rec, hist, activated);
        g_free(u);
        return;
    }
    if (v->opened) {
        if (ns_pv_maybe_update_viewport(v))
            return;
        configure_adjustments(v);
        request_render(v);
    }
}

static gboolean
on_scroll(GtkEventControllerScroll *ctrl, double dx, double dy, gpointer data)
{
    NsProcView *v = data;
    if (!v->opened)
        return FALSE;
    GdkModifierType mods =
        gtk_event_controller_get_current_event_state(
            GTK_EVENT_CONTROLLER(ctrl));
    GdkEvent *ev =
        gtk_event_controller_get_current_event(GTK_EVENT_CONTROLLER(ctrl));
    if (ev)
        mods |= gdk_event_get_modifier_state(ev);
    if (mods & GDK_CONTROL_MASK) {
        double delta = dy != 0.0 ? dy : dx;
        if (delta < 0)
            ns_proc_view_zoom_in(v);
        else if (delta > 0)
            ns_proc_view_zoom_out(v);
        return TRUE;
    }
    double s = ns_pv_cur_scale(v);
    v->fling_vx = v->fling_vy = 0;
    v->wheel_viewport = FALSE;
    if (gtk_event_controller_scroll_get_unit(ctrl) == GDK_SCROLL_UNIT_SURFACE) {
        queue_wheel_scroll(v, dx / s, dy / s);
        return TRUE;
    }
    v->wheel_left_x += dx * NS_PV_WHEEL_STEP_PX / s;
    v->wheel_left_y += dy * NS_PV_WHEEL_STEP_PX / s;
    arm_wheel_animation(v);
    return TRUE;
}

static void
on_scroll_decelerate(GtkEventControllerScroll *ctrl, double vel_x,
                     double vel_y, gpointer data)
{
    NsProcView *v = data;
    if (!v->opened ||
        (gtk_event_controller_get_current_event_state(
             GTK_EVENT_CONTROLLER(ctrl)) & GDK_CONTROL_MASK))
        return;
    double s = ns_pv_cur_scale(v);
    v->fling_vx = vel_x / s;
    v->fling_vy = vel_y / s;
    v->wheel_viewport = FALSE;
    arm_wheel_animation(v);
}

typedef struct {
    GPtrArray      *pages;
    ns_print_setup  setup;
    double          raster_scale;
    char           *title;
} PrintJob;

static void
print_job_free(gpointer data)
{
    PrintJob *job = data;
    for (guint i = 0; i < job->pages->len; i++)
        cairo_surface_destroy(g_ptr_array_index(job->pages, i));
    g_ptr_array_free(job->pages, TRUE);
    g_free(job->title);
    g_free(job);
}

static void
on_print_begin(GtkPrintOperation *op, GtkPrintContext *ctx, gpointer data)
{
    (void)ctx;
    PrintJob *job = data;
    gtk_print_operation_set_n_pages(op, (int)job->pages->len);
}

static void
on_print_draw(GtkPrintOperation *op, GtkPrintContext *ctx, int page_nr,
              gpointer data)
{
    (void)op;
    PrintJob *job = data;
    if (page_nr < 0 || (guint)page_nr >= job->pages->len)
        return;
    cairo_t *cr = gtk_print_context_get_cairo_context(ctx);
    double surface_w = gtk_print_context_get_width(ctx);
    double scale = job->setup.width > 0 ? surface_w / job->setup.width : 1.0;
    scale /= job->raster_scale > 0 ? job->raster_scale : 1.0;
    cairo_save(cr);
    cairo_scale(cr, scale, scale);
    cairo_set_source_surface(cr, g_ptr_array_index(job->pages, page_nr), 0, 0);
    cairo_paint(cr);
    cairo_restore(cr);
}

static void
print_run(NsProcView *v, GPtrArray *pages, const ns_print_setup *setup,
          double raster_scale)
{
    PrintJob *job = g_new0(PrintJob, 1);
    job->pages = pages;
    job->setup = *setup;
    job->raster_scale = raster_scale > 0 ? raster_scale : 1.0;
    job->title = g_strdup((v->current_title && *v->current_title)
                          ? v->current_title : "page");

    GtkPageSetup *page_setup = gtk_page_setup_new();
    GtkPaperSize *paper = gtk_paper_size_new_custom(
        "southstar", "Southstar", setup->width * 72.0 / 96.0,
        setup->height * 72.0 / 96.0, GTK_UNIT_POINTS);
    gtk_page_setup_set_paper_size(page_setup, paper);
    gtk_page_setup_set_orientation(page_setup,
        setup->width > setup->height ? GTK_PAGE_ORIENTATION_LANDSCAPE
                                     : GTK_PAGE_ORIENTATION_PORTRAIT);
    gtk_paper_size_free(paper);

    GtkPrintOperation *op = gtk_print_operation_new();
    gtk_print_operation_set_default_page_setup(op, page_setup);
    gtk_print_operation_set_use_full_page(op, TRUE);
    gtk_print_operation_set_unit(op, GTK_UNIT_POINTS);
    gtk_print_operation_set_job_name(op, job->title);
    gtk_print_operation_set_embed_page_setup(op, TRUE);
    g_signal_connect(op, "begin-print", G_CALLBACK(on_print_begin), job);
    g_signal_connect(op, "draw-page", G_CALLBACK(on_print_draw), job);
    g_object_set_data_full(G_OBJECT(op), "ns-print-job", job, print_job_free);

    GtkRoot *root = v->area ? gtk_widget_get_root(v->area) : NULL;
    GError *err = NULL;
    gtk_print_operation_run(op, GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG,
                            GTK_WINDOW(root), &err);
    if (err) {
        post_emit(v, NS_PROC_EVT_STATUS, err->message);
        g_error_free(err);
    }
    g_object_unref(page_setup);
    g_object_unref(op);
}

typedef struct { NsProcView *view; gboolean pdf; } ExportCtx;

static void
on_save_dialog_done(GObject *src, GAsyncResult *res, gpointer ud)
{
    ExportCtx *c = ud;
    NsProcView *v = c->view;
    GError *err = NULL;
    GFile *file = gtk_file_dialog_save_finish(GTK_FILE_DIALOG(src), res, &err);
    if (file) {
        char *dest = g_file_get_path(file);
        if (dest && v->opened) {
            static int export_counter = 0;
            char *base = g_strdup_printf(
                "southstar-export-%" G_GINT64_FORMAT "-%d.%s",
                g_get_monotonic_time(), ++export_counter,
                c->pdf ? "pdf" : "png");
            Req *req = g_new0(Req, 1);
            req->type = REQ_EXPORT;
            req->url = g_build_filename(g_get_user_runtime_dir(), base, NULL);
            req->export_dest = g_strdup(dest);
            ns_pv_push_req(v, req);
            g_free(base);
        }
        g_free(dest);
        g_object_unref(file);
    }
    g_clear_error(&err);
    pv_unref(v);
    g_free(c);
}

void
ns_pv_view_save(NsProcView *v, gboolean pdf)
{
    if (!v->opened || !v->area)
        return;
    GtkRoot *root = gtk_widget_get_root(v->area);
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog,
        pdf ? ns_i18n("Save page as PDF")
                                      : ns_i18n("Save page as PNG"));
    const char *t = (v->current_title && *v->current_title)
        ? v->current_title : "page";
    char *name = g_strdup_printf("%s.%s", t, pdf ? "pdf" : "png");
    g_strdelimit(name, "/", '_');
    gtk_file_dialog_set_initial_name(dialog, name);
    ExportCtx *c = g_new0(ExportCtx, 1);
    c->view = pv_ref(v);
    c->pdf = pdf;
    gtk_file_dialog_save(dialog, GTK_WINDOW(root), NULL,
                         on_save_dialog_done, c);
    g_object_unref(dialog);
    g_free(name);
}

typedef enum {
    EDIT_NONE, EDIT_CUT, EDIT_COPY, EDIT_PASTE, EDIT_SELECT_ALL
} EditOp;

static void
ctx_set_clipboard(NsProcView *v, const char *text, const char *status)
{
    if (!text || !*text || !v->area)
        return;
    gdk_clipboard_set_text(gtk_widget_get_clipboard(v->area), text);
    post_emit(v, NS_PROC_EVT_STATUS, status);
}

static void
on_ctx_back(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_back(ud); }

static void
on_ctx_forward(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_forward(ud); }

static void
on_ctx_reload(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_reload(ud); }

static void
on_ctx_copy_url(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    ctx_set_clipboard(v, v->current_url, ns_i18n("Copied page address"));
}

static void
on_ctx_open_newtab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    if (v->ctx_link && *v->ctx_link)
        post_emit(v, NS_PROC_EVT_NEWTAB, v->ctx_link);
}

static void
on_ctx_open_link(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    if (v->ctx_link && *v->ctx_link)
        ns_proc_view_load(v, v->ctx_link);
}

static void
on_ctx_copy_link(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    ctx_set_clipboard(v, v->ctx_link, ns_i18n("Copied link address"));
}

static void
on_ctx_cut(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_CUT); }

static void
on_ctx_copy_sel(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_COPY); }

static void
on_ctx_paste(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_PASTE); }

static void
on_ctx_select_all(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    v->has_selection = TRUE;
    ns_pv_start_select(v, 3, 0, 0);
}

void
ns_proc_view_print(NsProcView *view)
{
    if (!view || !view->opened)
        return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_PRINT;
    ns_pv_push_req(view, req);
}

void
ns_proc_view_save_pdf(NsProcView *view)
{ if (view) ns_pv_view_save(view, TRUE); }

void
ns_proc_view_save_image(NsProcView *view)
{ if (view) ns_pv_view_save(view, FALSE); }

static void
on_ctx_save_pdf(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_view_save(ud, TRUE); }

static void
on_ctx_save_png(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_view_save(ud, FALSE); }

static void
ctx_action_enable(NsProcView *v, const char *name, gboolean on)
{
    GAction *act = g_action_map_lookup_action(G_ACTION_MAP(v->ctx_actions),
                                              name);
    if (act)
        g_simple_action_set_enabled(G_SIMPLE_ACTION(act), on);
}

static void
ctx_install_actions(NsProcView *v)
{
    static const GActionEntry entries[] = {
        { "back",        on_ctx_back,        NULL, NULL, NULL, {0} },
        { "forward",     on_ctx_forward,     NULL, NULL, NULL, {0} },
        { "reload",      on_ctx_reload,      NULL, NULL, NULL, {0} },
        { "copy-url",    on_ctx_copy_url,    NULL, NULL, NULL, {0} },
        { "open-link",   on_ctx_open_link,   NULL, NULL, NULL, {0} },
        { "open-newtab", on_ctx_open_newtab, NULL, NULL, NULL, {0} },
        { "copy-link",   on_ctx_copy_link,   NULL, NULL, NULL, {0} },
        { "cut",         on_ctx_cut,         NULL, NULL, NULL, {0} },
        { "copy-sel",    on_ctx_copy_sel,    NULL, NULL, NULL, {0} },
        { "paste",       on_ctx_paste,       NULL, NULL, NULL, {0} },
        { "select-all",  on_ctx_select_all,  NULL, NULL, NULL, {0} },
        { "save-pdf",    on_ctx_save_pdf,    NULL, NULL, NULL, {0} },
        { "save-png",    on_ctx_save_png,    NULL, NULL, NULL, {0} },
    };
    v->ctx_actions = g_simple_action_group_new();
    g_action_map_add_action_entries(G_ACTION_MAP(v->ctx_actions), entries,
                                    G_N_ELEMENTS(entries), v);
    gtk_widget_insert_action_group(v->area, "ctx",
                                   G_ACTION_GROUP(v->ctx_actions));
}

static gboolean
popover_present_idle(gpointer data)
{
    GtkWidget *pop = data;
    if (gtk_widget_get_mapped(pop))
        gtk_popover_present(GTK_POPOVER(pop));
    g_object_unref(pop);
    return G_SOURCE_REMOVE;
}

static void
on_popover_menu_mapped(GtkWidget *pop, gpointer user_data)
{
    (void)user_data;
    g_idle_add(popover_present_idle, g_object_ref(pop));
}

void
ns_popover_menu_fit(GtkWidget *popover)
{
    if (!GTK_IS_POPOVER(popover)) return;
    g_signal_connect(popover, "map", G_CALLBACK(on_popover_menu_mapped), NULL);
}

static GMenu *
ctx_field_menu(NsProcView *v, int edit)
{
    gboolean selected = (edit & NS_BROWSER_EDIT_SELECTION) != 0;
    gboolean writable = (edit & NS_BROWSER_EDIT_WRITABLE) != 0;
    ctx_action_enable(v, "cut", writable && selected);
    ctx_action_enable(v, "copy-sel", selected);
    ctx_action_enable(v, "paste", writable);

    GMenu *menu = g_menu_new();
    GMenu *clip = g_menu_new();
    g_menu_append(clip, ns_i18n("Cut"), "ctx.cut");
    g_menu_append(clip, ns_i18n("Copy"), "ctx.copy-sel");
    g_menu_append(clip, ns_i18n("Paste"), "ctx.paste");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(clip));
    g_object_unref(clip);
    GMenu *all = g_menu_new();
    g_menu_append(all, ns_i18n("Select All"), "ctx.select-all");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(all));
    g_object_unref(all);
    return menu;
}

static GMenu *
ctx_page_menu(NsProcView *v)
{
    ctx_action_enable(v, "back", ns_proc_view_can_back(v));
    ctx_action_enable(v, "forward", ns_proc_view_can_forward(v));
    ctx_action_enable(v, "open-link", v->ctx_link != NULL);
    ctx_action_enable(v, "open-newtab", v->ctx_link != NULL);
    ctx_action_enable(v, "copy-link", v->ctx_link != NULL);
    ctx_action_enable(v, "copy-sel", v->has_selection);

    GMenu *menu = g_menu_new();
    if (v->ctx_link) {
        GMenu *s = g_menu_new();
        g_menu_append(s, ns_i18n("Open Link"), "ctx.open-link");
        if (!ns_rproc_single_process_enabled())
            g_menu_append(s, ns_i18n("Open Link in New Tab"), "ctx.open-newtab");
        g_menu_append(s, ns_i18n("Copy Link Address"), "ctx.copy-link");
        g_menu_append_section(menu, NULL, G_MENU_MODEL(s));
        g_object_unref(s);
    }
    if (v->has_selection) {
        GMenu *s = g_menu_new();
        g_menu_append(s, ns_i18n("Copy"), "ctx.copy-sel");
        g_menu_append_section(menu, NULL, G_MENU_MODEL(s));
        g_object_unref(s);
    }
    GMenu *nav = g_menu_new();
    g_menu_append(nav, ns_i18n("Back"), "ctx.back");
    g_menu_append(nav, ns_i18n("Forward"), "ctx.forward");
    g_menu_append(nav, ns_i18n("Reload"), "ctx.reload");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(nav));
    g_object_unref(nav);
    GMenu *page = g_menu_new();
    g_menu_append(page, ns_i18n("Select All"), "ctx.select-all");
    g_menu_append(page, ns_i18n("Copy Page Address"), "ctx.copy-url");
    g_menu_append(page, ns_i18n("Save Page as PDF…"), "ctx.save-pdf");
    g_menu_append(page, ns_i18n("Save Page as Image…"), "ctx.save-png");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(page));
    g_object_unref(page);
    return menu;
}

static void
show_context_menu(NsProcView *v, const char *href, int edit)
{
    g_free(v->ctx_link);
    v->ctx_link = (href && *href) ? g_strdup(href) : NULL;

    GMenu *menu = (edit & NS_BROWSER_EDIT_FIELD) ? ctx_field_menu(v, edit)
                                                 : ctx_page_menu(v);
    if (v->ctx_popover)
        gtk_widget_unparent(v->ctx_popover);
    v->ctx_popover = gtk_popover_menu_new_from_model(G_MENU_MODEL(menu));
    g_object_unref(menu);
    gtk_widget_set_parent(v->ctx_popover, v->area);
    ns_popover_menu_fit(v->ctx_popover);
    gtk_popover_set_has_arrow(GTK_POPOVER(v->ctx_popover), FALSE);
    gtk_popover_set_pointing_to(GTK_POPOVER(v->ctx_popover),
        &(GdkRectangle){ (int)v->ctx_x, (int)v->ctx_y, 1, 1 });
    gtk_popover_popup(GTK_POPOVER(v->ctx_popover));
}

void
ns_pv_request_find(NsProcView *v, int direction)
{
    if (!v->opened || !v->search_entry)
        return;
    const char *q = gtk_editable_get_text(GTK_EDITABLE(v->search_entry));
    Req *req = g_new0(Req, 1);
    req->type = REQ_FIND;
    req->seq = ++v->find_seq;
    req->query = g_strdup(q ? q : "");
    req->find_dir = direction;
    req->find_from_y = v->scroll_y;
    req->find_case = v->find_case;
    ns_pv_push_req(v, req);
}

void
ns_pv_search_open(NsProcView *v)
{
    if (!v->search_revealer)
        return;
    gtk_revealer_set_reveal_child(GTK_REVEALER(v->search_revealer), TRUE);
    gtk_widget_grab_focus(v->search_entry);
    gtk_editable_select_region(GTK_EDITABLE(v->search_entry), 0, -1);
    const char *q = gtk_editable_get_text(GTK_EDITABLE(v->search_entry));
    if (q && *q)
        ns_pv_request_find(v, 0);
}

static void
search_close(NsProcView *v)
{
    if (!v->search_revealer)
        return;
    gtk_revealer_set_reveal_child(GTK_REVEALER(v->search_revealer), FALSE);
    gtk_label_set_text(GTK_LABEL(v->search_label), "");
    Req *req = g_new0(Req, 1);
    req->type = REQ_FIND;
    req->seq = ++v->find_seq;
    req->query = g_strdup("");
    req->find_from_y = v->scroll_y;
    ns_pv_push_req(v, req);
    gtk_widget_grab_focus(v->area);
}

void
ns_proc_view_find_open(NsProcView *v)
{
    ns_pv_search_open(v);
}

void
ns_proc_view_set_color_scheme(NsProcView *v, gboolean dark)
{
    if (!v) return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_COLOR_SCHEME;
    req->mods = dark ? 1 : 0;
    ns_pv_push_req(v, req);
}

static void
on_search_changed(GtkSearchEntry *e, gpointer data)
{ (void)e; ns_pv_request_find(data, 0); }

static void
on_search_next(GtkWidget *w, gpointer data)
{ (void)w; ns_pv_request_find(data, 1); }

static void
on_search_prev(GtkWidget *w, gpointer data)
{ (void)w; ns_pv_request_find(data, 2); }

gboolean
ns_proc_view_find_close(NsProcView *view)
{
    if (!view || !view->search_revealer ||
        !gtk_revealer_get_reveal_child(GTK_REVEALER(view->search_revealer)))
        return FALSE;
    search_close(view);
    return TRUE;
}

static void
on_search_stop(GtkSearchEntry *e, gpointer data)
{ (void)e; search_close(data); }

static void
on_search_close_clicked(GtkButton *b, gpointer data)
{ (void)b; search_close(data); }

static void
on_area_destroy(GtkWidget *widget, gpointer data)
{
    NsProcView *v = data;
    NS_PROC_VIEW_AREA(widget)->view = NULL;
    v->closed = TRUE;
    g_clear_object(&v->im);
    disarm_anim(v);
    if (v->hover_after_scroll_id) {
        g_source_remove(v->hover_after_scroll_id);
        v->hover_after_scroll_id = 0;
    }
    if (v->console_poll_id) {
        g_source_remove(v->console_poll_id);
        v->console_poll_id = 0;
    }
    if (v->console_window) {
        GtkWidget *win = v->console_window;
        v->console_window = NULL;
        v->console_notebook = NULL;
        v->console_entry = NULL;
        v->console_view = NULL;
        v->console_buffer = NULL;
        v->net_view = NULL;
        v->net_buffer = NULL;
        v->perf_view = NULL;
        v->perf_buffer = NULL;
        v->layout_view = NULL;
        v->layout_buffer = NULL;
        v->elements_view = NULL;
        v->elements_buffer = NULL;
        v->inspect_entry = NULL;
        gtk_window_destroy(GTK_WINDOW(win));
    }
    if (v->ctx_popover) {
        gtk_widget_unparent(v->ctx_popover);
        v->ctx_popover = NULL;
    }
    v->area = NULL;
    v->perm_revealer = NULL;
    v->perm_label = NULL;
    Req *req = g_new0(Req, 1);
    req->type = REQ_QUIT;
    ns_pv_push_req(v, req);
    /* Unblock the worker if it is mid-request to a wedged renderer, so the join
     * below can't stall the main loop for up to the 30 s IPC read timeout (which
     * would also trip the watchdog heartbeat and restart the whole shell). */
    g_mutex_lock(&v->proc_lock);
    if (v->proc)
        ns_rproc_http_interrupt(v->proc);
    g_mutex_unlock(&v->proc_lock);
    if (v->thread) {
        g_thread_join(v->thread);
        v->thread = NULL;
    }
    pv_unref(v);
}

static void
console_append(NsProcView *v, const char *text)
{
    if (!v->console_buffer || !text || !*text)
        return;
    GtkTextIter end;
    gtk_text_buffer_get_end_iter(v->console_buffer, &end);
    gtk_text_buffer_insert(v->console_buffer, &end, text, -1);
    if (v->console_view) {
        gtk_text_buffer_get_end_iter(v->console_buffer, &end);
        GtkTextMark *m = gtk_text_buffer_create_mark(v->console_buffer, NULL,
                                                     &end, FALSE);
        gtk_text_view_scroll_mark_onscreen(GTK_TEXT_VIEW(v->console_view), m);
        gtk_text_buffer_delete_mark(v->console_buffer, m);
    }
}

static gboolean
console_poll_cb(gpointer data)
{
    NsProcView *v = data;
    if (!v->console_open || !v->opened)
        return G_SOURCE_CONTINUE;
    Req *req = g_new0(Req, 1);
    req->type = REQ_CONSOLE;
    ns_pv_push_req(v, req);
    return G_SOURCE_CONTINUE;
}

static void
on_console_eval(GtkEntry *entry, gpointer data)
{
    NsProcView *v = data;
    const char *src = gtk_editable_get_text(GTK_EDITABLE(entry));
    if (!src || !*src || !v->opened)
        return;
    char *echo = g_strdup_printf("> %s\n", src);
    console_append(v, echo);
    g_free(echo);
    Req *req = g_new0(Req, 1);
    req->type = REQ_EVAL;
    req->query = g_strdup(src);
    ns_pv_push_req(v, req);
    gtk_editable_set_text(GTK_EDITABLE(entry), "");
}

static void build_console_window(NsProcView *v);

void
ns_pv_console_set_open(NsProcView *v, gboolean open)
{
    if (open && !v->console_window)
        build_console_window(v);
    if (!v->console_window)
        return;
    v->console_open = open;
    if (open) {
        GtkRoot *root = gtk_widget_get_root(v->area);
        if (GTK_IS_WINDOW(root))
            gtk_window_set_transient_for(GTK_WINDOW(v->console_window),
                                         GTK_WINDOW(root));
        gtk_window_present(GTK_WINDOW(v->console_window));
        if (!v->console_poll_id)
            v->console_poll_id = g_timeout_add(NS_PROC_CONSOLE_POLL_MS, console_poll_cb, v);
        gtk_widget_grab_focus(v->console_entry);
    } else {
        gtk_widget_set_visible(v->console_window, FALSE);
        if (v->console_poll_id) {
            g_source_remove(v->console_poll_id);
            v->console_poll_id = 0;
        }
        if (v->area)
            gtk_widget_grab_focus(v->area);
    }
}

static void
console_request_dump(NsProcView *v, int tab)
{
    const char *kind = NULL;
    switch (tab) {
    case DEV_TAB_NETWORK:     kind = "network";     break;
    case DEV_TAB_PERFORMANCE: kind = "performance"; break;
    case DEV_TAB_LAYOUT:      kind = "layout";      break;
    case DEV_TAB_ELEMENTS:    kind = "dom";         break;
    default: return;
    }
    if (!v->opened)
        return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_DUMP;
    req->dump_tab = tab;
    req->query = g_strdup(kind);
    ns_pv_push_req(v, req);
}

static int
console_current_tab(NsProcView *v)
{
    if (!v->console_notebook)
        return DEV_TAB_CONSOLE;
    return gtk_notebook_get_current_page(GTK_NOTEBOOK(v->console_notebook));
}

static void
on_console_clear(GtkButton *b, gpointer data)
{
    (void)b;
    NsProcView *v = data;
    GtkTextBuffer *buf = NULL;
    switch (console_current_tab(v)) {
    case DEV_TAB_CONSOLE:     buf = v->console_buffer;  break;
    case DEV_TAB_NETWORK:     buf = v->net_buffer;      break;
    case DEV_TAB_PERFORMANCE: buf = v->perf_buffer;     break;
    case DEV_TAB_LAYOUT:      buf = v->layout_buffer;   break;
    case DEV_TAB_ELEMENTS:    buf = v->elements_buffer; break;
    default: break;
    }
    if (buf)
        gtk_text_buffer_set_text(buf, "", 0);
}

static void
on_console_refresh(GtkButton *b, gpointer data)
{
    (void)b;
    NsProcView *v = data;
    console_request_dump(v, console_current_tab(v));
}

static void
on_console_notebook_switch(GtkNotebook *nb, GtkWidget *page, guint num,
                           gpointer data)
{
    (void)nb;
    (void)page;
    NsProcView *v = data;
    console_request_dump(v, (int)num);
}

static void
on_inspect_activate(GtkEntry *entry, gpointer data)
{
    NsProcView *v = data;
    const char *sel = gtk_editable_get_text(GTK_EDITABLE(entry));
    if (!sel || !*sel || !v->opened)
        return;
    char *esc = g_strescape(sel, NULL);
    char *src = g_strdup_printf(
        "(function(){try{var e=document.querySelector(\"%s\");"
        "if(!e)return \"\";var s=e.outerHTML;"
        "return s.length>20000?s.slice(0,20000)+\"\\n\\u2026(truncated)\":s;}"
        "catch(err){return \"Error: \"+err;}})()",
        esc);
    g_free(esc);
    Req *req = g_new0(Req, 1);
    req->type = REQ_EVAL;
    req->inspect = TRUE;
    req->dump_tab = DEV_TAB_ELEMENTS;
    req->query = src;
    ns_pv_push_req(v, req);
}

static gboolean
on_console_close_request(GtkWindow *win, gpointer data)
{
    (void)win;
    ns_pv_console_set_open(data, FALSE);
    return TRUE;
}

static GtkWidget *
console_make_view(GtkWidget **out_view, GtkTextBuffer **out_buffer,
                  gboolean wrap)
{
    GtkWidget *view = gtk_text_view_new();
    gtk_text_view_set_editable(GTK_TEXT_VIEW(view), FALSE);
    gtk_text_view_set_monospace(GTK_TEXT_VIEW(view), TRUE);
    gtk_text_view_set_cursor_visible(GTK_TEXT_VIEW(view), FALSE);
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(view),
                                wrap ? GTK_WRAP_WORD_CHAR : GTK_WRAP_NONE);
    *out_view = view;
    *out_buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));
    GtkWidget *scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), view);
    gtk_widget_set_vexpand(scroll, TRUE);
    return scroll;
}

static void
console_add_tab(NsProcView *v, GtkWidget *child, const char *title)
{
    gtk_notebook_append_page(GTK_NOTEBOOK(v->console_notebook), child,
                             gtk_label_new(ns_i18n(title)));
}

static void
build_console_window(NsProcView *v)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_add_css_class(header, "toolbar");
    GtkWidget *spacer = gtk_label_new("");
    gtk_widget_set_hexpand(spacer, TRUE);
    GtkWidget *refresh = gtk_button_new_from_icon_name("view-refresh-symbolic");
    gtk_widget_set_tooltip_text(refresh, ns_i18n("Refresh"));
    set_accessible_label(refresh, ns_i18n("Refresh"));
    g_signal_connect(refresh, "clicked", G_CALLBACK(on_console_refresh), v);
    GtkWidget *clear = gtk_button_new_from_icon_name("edit-clear-symbolic");
    gtk_widget_set_tooltip_text(clear, ns_i18n("Clear"));
    set_accessible_label(clear, ns_i18n("Clear"));
    g_signal_connect(clear, "clicked", G_CALLBACK(on_console_clear), v);
    gtk_box_append(GTK_BOX(header), spacer);
    gtk_box_append(GTK_BOX(header), refresh);
    gtk_box_append(GTK_BOX(header), clear);

    v->console_notebook = gtk_notebook_new();
    gtk_widget_set_vexpand(v->console_notebook, TRUE);

    GtkWidget *console_page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *console_scroll =
        console_make_view(&v->console_view, &v->console_buffer, TRUE);
    v->console_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(v->console_entry),
                                   ns_i18n("Evaluate JavaScript and press Enter"));
    g_signal_connect(v->console_entry, "activate",
                     G_CALLBACK(on_console_eval), v);
    gtk_box_append(GTK_BOX(console_page), console_scroll);
    gtk_box_append(GTK_BOX(console_page), v->console_entry);
    console_add_tab(v, console_page, "Console");

    console_add_tab(v, console_make_view(&v->net_view, &v->net_buffer, FALSE),
                    "Network");
    console_add_tab(v, console_make_view(&v->perf_view, &v->perf_buffer, FALSE),
                    "Performance");
    console_add_tab(v,
                    console_make_view(&v->layout_view, &v->layout_buffer, FALSE),
                    "Layout");

    GtkWidget *elements_page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *elements_scroll =
        console_make_view(&v->elements_view, &v->elements_buffer, FALSE);
    v->inspect_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(
        GTK_ENTRY(v->inspect_entry),
        ns_i18n("Inspect: CSS selector, then Enter"));
    g_signal_connect(v->inspect_entry, "activate",
                     G_CALLBACK(on_inspect_activate), v);
    gtk_box_append(GTK_BOX(elements_page), elements_scroll);
    gtk_box_append(GTK_BOX(elements_page), v->inspect_entry);
    console_add_tab(v, elements_page, "Elements");

    g_signal_connect(v->console_notebook, "switch-page",
                     G_CALLBACK(on_console_notebook_switch), v);

    gtk_box_append(GTK_BOX(box), header);
    gtk_box_append(GTK_BOX(box), v->console_notebook);

    v->console_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(v->console_window),
                         ns_i18n("Developer Tools"));
    gtk_window_set_default_size(GTK_WINDOW(v->console_window), 720, 420);
    gtk_window_set_destroy_with_parent(GTK_WINDOW(v->console_window), TRUE);
    gtk_window_set_child(GTK_WINDOW(v->console_window), box);
    g_signal_connect(v->console_window, "close-request",
                     G_CALLBACK(on_console_close_request), v);
}

static void
build_search_bar(NsProcView *v)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_add_css_class(box, "toolbar");

    v->search_entry = gtk_search_entry_new();
    gtk_widget_set_size_request(v->search_entry, 220, -1);
    set_accessible_label(v->search_entry, ns_i18n("Find in page"));
    g_signal_connect(v->search_entry, "search-changed",
                     G_CALLBACK(on_search_changed), v);
    g_signal_connect(v->search_entry, "activate",
                     G_CALLBACK(on_search_next), v);
    g_signal_connect(v->search_entry, "next-match",
                     G_CALLBACK(on_search_next), v);
    g_signal_connect(v->search_entry, "previous-match",
                     G_CALLBACK(on_search_prev), v);
    g_signal_connect(v->search_entry, "stop-search",
                     G_CALLBACK(on_search_stop), v);

    v->search_label = gtk_label_new("");
    gtk_widget_set_size_request(v->search_label, 56, -1);

    GtkWidget *prev = gtk_button_new_from_icon_name("go-up-symbolic");
    GtkWidget *next = gtk_button_new_from_icon_name("go-down-symbolic");
    GtkWidget *close = gtk_button_new_from_icon_name("window-close-symbolic");
    gtk_widget_set_tooltip_text(prev, ns_i18n("Previous match (Shift+Enter)"));
    gtk_widget_set_tooltip_text(next, ns_i18n("Next match (Enter)"));
    gtk_widget_set_tooltip_text(close, ns_i18n("Close (Esc)"));
    set_accessible_label(prev, ns_i18n("Previous match"));
    set_accessible_label(next, ns_i18n("Next match"));
    set_accessible_label(close, ns_i18n("Close search"));
    g_signal_connect(prev, "clicked", G_CALLBACK(on_search_prev), v);
    g_signal_connect(next, "clicked", G_CALLBACK(on_search_next), v);
    g_signal_connect(close, "clicked", G_CALLBACK(on_search_close_clicked), v);

    gtk_box_append(GTK_BOX(box), v->search_entry);
    gtk_box_append(GTK_BOX(box), v->search_label);
    gtk_box_append(GTK_BOX(box), prev);
    gtk_box_append(GTK_BOX(box), next);
    gtk_box_append(GTK_BOX(box), close);

    GtkWidget *frame = gtk_frame_new(NULL);
    gtk_widget_add_css_class(frame, "ns-findbar");
    gtk_frame_set_child(GTK_FRAME(frame), box);

    v->search_revealer = gtk_revealer_new();
    gtk_revealer_set_transition_type(GTK_REVEALER(v->search_revealer),
                                     GTK_REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    gtk_revealer_set_child(GTK_REVEALER(v->search_revealer), frame);
    gtk_widget_set_halign(v->search_revealer, GTK_ALIGN_END);
    gtk_widget_set_valign(v->search_revealer, GTK_ALIGN_START);
    gtk_widget_set_margin_top(v->search_revealer, 8);
    gtk_widget_set_margin_end(v->search_revealer, 20);
    gtk_overlay_add_overlay(GTK_OVERLAY(v->overlay), v->search_revealer);
}

static void
build_perm_bar(NsProcView *v)
{
    GtkWidget *bar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_margin_top(bar, 4);
    gtk_widget_set_margin_bottom(bar, 4);
    gtk_widget_set_margin_start(bar, 8);
    gtk_widget_set_margin_end(bar, 8);

    GtkWidget *icon = gtk_image_new_from_icon_name("dialog-question-symbolic");

    v->perm_label = gtk_label_new("");
    gtk_label_set_xalign(GTK_LABEL(v->perm_label), 0.0);
    gtk_label_set_ellipsize(GTK_LABEL(v->perm_label), PANGO_ELLIPSIZE_MIDDLE);
    gtk_widget_set_hexpand(v->perm_label, TRUE);

    GtkWidget *allow = gtk_button_new_with_label(ns_i18n("Allow"));
    gtk_widget_add_css_class(allow, "suggested-action");
    gtk_widget_set_tooltip_text(allow, ns_i18n("Allow and trust this site"));
    GtkWidget *deny = gtk_button_new_with_label(ns_i18n("Not now"));
    GtkWidget *close = gtk_button_new_from_icon_name("window-close-symbolic");
    gtk_button_set_has_frame(GTK_BUTTON(close), FALSE);
    set_accessible_label(close, ns_i18n("Dismiss"));
    g_signal_connect(allow, "clicked", G_CALLBACK(on_perm_allow), v);
    g_signal_connect(deny, "clicked", G_CALLBACK(on_perm_deny), v);
    g_signal_connect(close, "clicked", G_CALLBACK(on_perm_deny), v);

    gtk_box_append(GTK_BOX(bar), icon);
    gtk_box_append(GTK_BOX(bar), v->perm_label);
    gtk_box_append(GTK_BOX(bar), allow);
    gtk_box_append(GTK_BOX(bar), deny);
    gtk_box_append(GTK_BOX(bar), close);

    GtkWidget *frame = gtk_frame_new(NULL);
    gtk_frame_set_child(GTK_FRAME(frame), bar);

    v->perm_revealer = gtk_revealer_new();
    gtk_revealer_set_transition_type(GTK_REVEALER(v->perm_revealer),
                                     GTK_REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    gtk_revealer_set_child(GTK_REVEALER(v->perm_revealer), frame);
}

NsProcView *
ns_proc_view_new(void)
{
    ns_pv_check_layout();
    NsProcView *v = g_new0(NsProcView, 1);
    g_ref_count_init(&v->rc);
    g_mutex_init(&v->proc_lock);
    v->media = ns_pv_media_new();
    v->renderer_path = ns_proc_renderer_path();
    v->queue = g_async_queue_new();
    v->history = g_ptr_array_new_with_free_func(g_free);
    v->hist_index = -1;
    v->link_pending_action = ACT_HOVER;
    v->pending_record = TRUE;
    v->scale = 1.0;

    v->hadj = gtk_adjustment_new(0, 0, 1, 60, 60, 1);
    v->vadj = gtk_adjustment_new(0, 0, 1, 60, 60, 1);
    g_signal_connect(v->hadj, "value-changed", G_CALLBACK(on_adj_changed), v);
    g_signal_connect(v->vadj, "value-changed", G_CALLBACK(on_adj_changed), v);

    v->layers = ns_page_layers_new();
    v->area = g_object_new(ns_proc_view_area_get_type(), NULL);
    NS_PROC_VIEW_AREA(v->area)->view = v;
    gtk_widget_set_hexpand(v->area, TRUE);
    gtk_widget_set_vexpand(v->area, TRUE);
    gtk_widget_set_focusable(v->area, TRUE);
    g_signal_connect(v->area, "resize", G_CALLBACK(on_resize), v);
    g_signal_connect(v->area, "notify::has-focus",
                     G_CALLBACK(on_area_focus_notify), v);
    g_signal_connect(v->area, "notify::scale-factor",
                     G_CALLBACK(on_device_scale_changed), v);
    g_signal_connect(v->area, "realize", G_CALLBACK(on_area_realize), v);
    g_signal_connect(v->area, "unrealize", G_CALLBACK(on_area_unrealize), v);

    GtkWidget *grid = gtk_grid_new();
    v->vscroll =
        gtk_scrollbar_new(GTK_ORIENTATION_VERTICAL, v->vadj);
    v->hscroll =
        gtk_scrollbar_new(GTK_ORIENTATION_HORIZONTAL, v->hadj);
    gtk_grid_attach(GTK_GRID(grid), v->area, 0, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(grid), v->vscroll, 1, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(grid), v->hscroll, 0, 1, 1, 1);
    gtk_widget_set_visible(v->vscroll, FALSE);
    gtk_widget_set_visible(v->hscroll, FALSE);

    v->overlay = gtk_overlay_new();
    gtk_overlay_set_child(GTK_OVERLAY(v->overlay), grid);
    gtk_widget_set_vexpand(v->overlay, TRUE);
    build_perm_bar(v);
    v->root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_box_append(GTK_BOX(v->root), v->perm_revealer);
    gtk_box_append(GTK_BOX(v->root), v->overlay);
    gtk_widget_set_vexpand(v->root, TRUE);
    build_search_bar(v);

    ns_pv_input_install_pointer(v);

    ctx_install_actions(v);
    ns_pv_input_install_secondary(v);

    GtkEventController *scroll = gtk_event_controller_scroll_new(
        GTK_EVENT_CONTROLLER_SCROLL_BOTH_AXES |
        GTK_EVENT_CONTROLLER_SCROLL_KINETIC);
    g_signal_connect(scroll, "scroll", G_CALLBACK(on_scroll), v);
    g_signal_connect(scroll, "decelerate", G_CALLBACK(on_scroll_decelerate), v);
    gtk_widget_add_controller(v->area, scroll);

    ns_pv_input_install_keys(v);

    g_signal_connect(v->area, "destroy", G_CALLBACK(on_area_destroy), v);

    v->thread = g_thread_new("ns-proc-view", worker_main, pv_ref(v));
    return v;
}

GtkWidget *ns_proc_view_widget(NsProcView *v) { return v->root; }

void
ns_proc_view_set_notify(NsProcView *v, NsProcNotify cb, gpointer ud)
{
    v->notify = cb;
    v->notify_ud = ud;
}

void
ns_proc_view_set_private(NsProcView *v, gboolean private_mode)
{
    if (v)
        v->private_mode = private_mode;
}

gboolean
ns_proc_view_is_private(NsProcView *v)
{
    return v && v->private_mode;
}

void
ns_proc_view_layout(size_t *out, size_t count)
{
    const size_t layout[] = {
        sizeof(Req),
        G_STRUCT_OFFSET(Req, type),
        G_STRUCT_OFFSET(Req, seq),
        G_STRUCT_OFFSET(Req, url),
        G_STRUCT_OFFSET(Req, vw),
        G_STRUCT_OFFSET(Req, vh),
        G_STRUCT_OFFSET(Req, w),
        G_STRUCT_OFFSET(Req, h),
        G_STRUCT_OFFSET(Req, sx),
        G_STRUCT_OFFSET(Req, sy),
        G_STRUCT_OFFSET(Req, scale),
        G_STRUCT_OFFSET(Req, dpr),
        G_STRUCT_OFFSET(Req, raster),
        G_STRUCT_OFFSET(Req, wheel),
        G_STRUCT_OFFSET(Req, x),
        G_STRUCT_OFFSET(Req, y),
        G_STRUCT_OFFSET(Req, dx),
        G_STRUCT_OFFSET(Req, dy),
        G_STRUCT_OFFSET(Req, mods),
        G_STRUCT_OFFSET(Req, kind),
        G_STRUCT_OFFSET(Req, keycode),
        G_STRUCT_OFFSET(Req, fallback_scroll),
        G_STRUCT_OFFSET(Req, fallback_x),
        G_STRUCT_OFFSET(Req, fallback_y),
        G_STRUCT_OFFSET(Req, key),
        G_STRUCT_OFFSET(Req, code),
        G_STRUCT_OFFSET(Req, action),
        G_STRUCT_OFFSET(Req, query),
        G_STRUCT_OFFSET(Req, find_dir),
        G_STRUCT_OFFSET(Req, find_from_y),
        G_STRUCT_OFFSET(Req, find_case),
        G_STRUCT_OFFSET(Req, export_dest),
        G_STRUCT_OFFSET(Req, paths),
        G_STRUCT_OFFSET(Req, dump_tab),
        G_STRUCT_OFFSET(Req, inspect),
        G_STRUCT_OFFSET(Req, history),
        G_STRUCT_OFFSET(Req, user_activated),
        G_STRUCT_OFFSET(Req, caret_active),
        G_STRUCT_OFFSET(Req, tiles),
        G_STRUCT_OFFSET(Req, have),
        G_STRUCT_OFFSET(Req, hold),
        sizeof(Res),
        G_STRUCT_OFFSET(Res, view),
        G_STRUCT_OFFSET(Res, type),
        G_STRUCT_OFFSET(Res, seq),
        G_STRUCT_OFFSET(Res, ok),
        G_STRUCT_OFFSET(Res, pw),
        G_STRUCT_OFFSET(Res, ph),
        G_STRUCT_OFFSET(Res, title),
        G_STRUCT_OFFSET(Res, url),
        G_STRUCT_OFFSET(Res, url_pushed),
        G_STRUCT_OFFSET(Res, nav),
        G_STRUCT_OFFSET(Res, security),
        G_STRUCT_OFFSET(Res, remote_ip),
        G_STRUCT_OFFSET(Res, webgl),
        G_STRUCT_OFFSET(Res, camera),
        G_STRUCT_OFFSET(Res, download),
        G_STRUCT_OFFSET(Res, audio),
        G_STRUCT_OFFSET(Res, window_action),
        G_STRUCT_OFFSET(Res, clipboard),
        G_STRUCT_OFFSET(Res, texture),
        G_STRUCT_OFFSET(Res, texture_scale),
        G_STRUCT_OFFSET(Res, layers),
        G_STRUCT_OFFSET(Res, href),
        G_STRUCT_OFFSET(Res, cursor),
        G_STRUCT_OFFSET(Res, action),
        G_STRUCT_OFFSET(Res, kind),
        G_STRUCT_OFFSET(Res, prevented),
        G_STRUCT_OFFSET(Res, edit),
        G_STRUCT_OFFSET(Res, fallback_scroll),
        G_STRUCT_OFFSET(Res, fallback_x),
        G_STRUCT_OFFSET(Res, fallback_y),
        G_STRUCT_OFFSET(Res, animating),
        G_STRUCT_OFFSET(Res, caret_blinking),
        G_STRUCT_OFFSET(Res, wheel_snapped),
        G_STRUCT_OFFSET(Res, frame_unchanged),
        G_STRUCT_OFFSET(Res, requested_scroll_y),
        G_STRUCT_OFFSET(Res, requested_scroll_x),
        G_STRUCT_OFFSET(Res, find_total),
        G_STRUCT_OFFSET(Res, find_current),
        G_STRUCT_OFFSET(Res, find_scroll_y),
        G_STRUCT_OFFSET(Res, media_url),
        G_STRUCT_OFFSET(Res, media_is_video),
        G_STRUCT_OFFSET(Res, media_stream),
        G_STRUCT_OFFSET(Res, favicon_data),
        G_STRUCT_OFFSET(Res, favicon_w),
        G_STRUCT_OFFSET(Res, favicon_h),
        G_STRUCT_OFFSET(Res, favicon_stride),
        G_STRUCT_OFFSET(Res, dump_tab),
        G_STRUCT_OFFSET(Res, inspect),
        G_STRUCT_OFFSET(Res, print_pages),
        G_STRUCT_OFFSET(Res, print_setup),
        G_STRUCT_OFFSET(Res, print_scale),
        sizeof(NsProcView),
        G_STRUCT_OFFSET(NsProcView, rc),
        G_STRUCT_OFFSET(NsProcView, root),
        G_STRUCT_OFFSET(NsProcView, area),
        G_STRUCT_OFFSET(NsProcView, hscroll),
        G_STRUCT_OFFSET(NsProcView, vscroll),
        G_STRUCT_OFFSET(NsProcView, im),
        G_STRUCT_OFFSET(NsProcView, hadj),
        G_STRUCT_OFFSET(NsProcView, vadj),
        G_STRUCT_OFFSET(NsProcView, closed),
        G_STRUCT_OFFSET(NsProcView, thread),
        G_STRUCT_OFFSET(NsProcView, queue),
        G_STRUCT_OFFSET(NsProcView, proc),
        G_STRUCT_OFFSET(NsProcView, proc_lock),
        G_STRUCT_OFFSET(NsProcView, renderer_path),
        G_STRUCT_OFFSET(NsProcView, private_mode),
        G_STRUCT_OFFSET(NsProcView, media),
        G_STRUCT_OFFSET(NsProcView, notify),
        G_STRUCT_OFFSET(NsProcView, notify_ud),
        G_STRUCT_OFFSET(NsProcView, current_url),
        G_STRUCT_OFFSET(NsProcView, current_title),
        G_STRUCT_OFFSET(NsProcView, security),
        G_STRUCT_OFFSET(NsProcView, remote_ip),
        G_STRUCT_OFFSET(NsProcView, page_w),
        G_STRUCT_OFFSET(NsProcView, page_h),
        G_STRUCT_OFFSET(NsProcView, scroll_x),
        G_STRUCT_OFFSET(NsProcView, scroll_y),
        G_STRUCT_OFFSET(NsProcView, opened),
        G_STRUCT_OFFSET(NsProcView, frame),
        G_STRUCT_OFFSET(NsProcView, frame_scale),
        G_STRUCT_OFFSET(NsProcView, layers),
        G_STRUCT_OFFSET(NsProcView, tiles_mode),
        G_STRUCT_OFFSET(NsProcView, tiles_fill),
        G_STRUCT_OFFSET(NsProcView, wheel_remote),
        G_STRUCT_OFFSET(NsProcView, scroll_dir),
        G_STRUCT_OFFSET(NsProcView, surface_scale_handler),
        G_STRUCT_OFFSET(NsProcView, scale_surface),
        G_STRUCT_OFFSET(NsProcView, stage),
        G_STRUCT_OFFSET(NsProcView, stage_next),
        G_STRUCT_OFFSET(NsProcView, favicon),
        G_STRUCT_OFFSET(NsProcView, render_inflight),
        G_STRUCT_OFFSET(NsProcView, render_pending),
        G_STRUCT_OFFSET(NsProcView, tick_inflight),
        G_STRUCT_OFFSET(NsProcView, tick_pending),
        G_STRUCT_OFFSET(NsProcView, render_restarts),
        G_STRUCT_OFFSET(NsProcView, link_inflight),
        G_STRUCT_OFFSET(NsProcView, link_pending),
        G_STRUCT_OFFSET(NsProcView, link_pending_x),
        G_STRUCT_OFFSET(NsProcView, link_pending_y),
        G_STRUCT_OFFSET(NsProcView, link_pending_action),
        G_STRUCT_OFFSET(NsProcView, hover_inflight),
        G_STRUCT_OFFSET(NsProcView, hover_pending),
        G_STRUCT_OFFSET(NsProcView, hover_pending_x),
        G_STRUCT_OFFSET(NsProcView, hover_pending_y),
        G_STRUCT_OFFSET(NsProcView, has_selection),
        G_STRUCT_OFFSET(NsProcView, multi_click),
        G_STRUCT_OFFSET(NsProcView, ctx_x),
        G_STRUCT_OFFSET(NsProcView, ctx_y),
        G_STRUCT_OFFSET(NsProcView, ctx_link),
        G_STRUCT_OFFSET(NsProcView, ctx_popover),
        G_STRUCT_OFFSET(NsProcView, ctx_actions),
        G_STRUCT_OFFSET(NsProcView, search_revealer),
        G_STRUCT_OFFSET(NsProcView, search_entry),
        G_STRUCT_OFFSET(NsProcView, search_label),
        G_STRUCT_OFFSET(NsProcView, find_seq),
        G_STRUCT_OFFSET(NsProcView, find_case),
        G_STRUCT_OFFSET(NsProcView, overlay),
        G_STRUCT_OFFSET(NsProcView, perm_revealer),
        G_STRUCT_OFFSET(NsProcView, perm_label),
        G_STRUCT_OFFSET(NsProcView, perm_kind),
        G_STRUCT_OFFSET(NsProcView, perm_pending),
        G_STRUCT_OFFSET(NsProcView, perm_origin),
        G_STRUCT_OFFSET(NsProcView, console_window),
        G_STRUCT_OFFSET(NsProcView, console_notebook),
        G_STRUCT_OFFSET(NsProcView, console_entry),
        G_STRUCT_OFFSET(NsProcView, console_view),
        G_STRUCT_OFFSET(NsProcView, console_buffer),
        G_STRUCT_OFFSET(NsProcView, net_view),
        G_STRUCT_OFFSET(NsProcView, net_buffer),
        G_STRUCT_OFFSET(NsProcView, perf_view),
        G_STRUCT_OFFSET(NsProcView, perf_buffer),
        G_STRUCT_OFFSET(NsProcView, layout_view),
        G_STRUCT_OFFSET(NsProcView, layout_buffer),
        G_STRUCT_OFFSET(NsProcView, elements_view),
        G_STRUCT_OFFSET(NsProcView, elements_buffer),
        G_STRUCT_OFFSET(NsProcView, inspect_entry),
        G_STRUCT_OFFSET(NsProcView, console_open),
        G_STRUCT_OFFSET(NsProcView, console_poll_id),
        G_STRUCT_OFFSET(NsProcView, history),
        G_STRUCT_OFFSET(NsProcView, hist_index),
        G_STRUCT_OFFSET(NsProcView, pending_record),
        G_STRUCT_OFFSET(NsProcView, deferred_url),
        G_STRUCT_OFFSET(NsProcView, deferred_record),
        G_STRUCT_OFFSET(NsProcView, deferred_history),
        G_STRUCT_OFFSET(NsProcView, deferred_user_activated),
        G_STRUCT_OFFSET(NsProcView, js_redirects),
        G_STRUCT_OFFSET(NsProcView, scale),
        G_STRUCT_OFFSET(NsProcView, loading),
        G_STRUCT_OFFSET(NsProcView, busy_cursor),
        G_STRUCT_OFFSET(NsProcView, hourglass_cursor),
        G_STRUCT_OFFSET(NsProcView, load_seq),
        G_STRUCT_OFFSET(NsProcView, render_seq),
        G_STRUCT_OFFSET(NsProcView, link_seq),
        G_STRUCT_OFFSET(NsProcView, click_seq),
        G_STRUCT_OFFSET(NsProcView, viewport_seq),
        G_STRUCT_OFFSET(NsProcView, key_seq),
        G_STRUCT_OFFSET(NsProcView, select_seq),
        G_STRUCT_OFFSET(NsProcView, hover_seq),
        G_STRUCT_OFFSET(NsProcView, last_vp_w),
        G_STRUCT_OFFSET(NsProcView, last_vp_h),
        G_STRUCT_OFFSET(NsProcView, last_vp_dpr),
        G_STRUCT_OFFSET(NsProcView, wheel_left_x),
        G_STRUCT_OFFSET(NsProcView, wheel_left_y),
        G_STRUCT_OFFSET(NsProcView, wheel_viewport),
        G_STRUCT_OFFSET(NsProcView, wheel_pend_x),
        G_STRUCT_OFFSET(NsProcView, wheel_pend_y),
        G_STRUCT_OFFSET(NsProcView, fling_vx),
        G_STRUCT_OFFSET(NsProcView, fling_vy),
        G_STRUCT_OFFSET(NsProcView, wheel_tick_id),
        G_STRUCT_OFFSET(NsProcView, wheel_last_us),
        G_STRUCT_OFFSET(NsProcView, adopting_scroll),
        G_STRUCT_OFFSET(NsProcView, hover_after_scroll_id),
        G_STRUCT_OFFSET(NsProcView, drag_start_x),
        G_STRUCT_OFFSET(NsProcView, drag_start_y),
        G_STRUCT_OFFSET(NsProcView, pointer_x),
        G_STRUCT_OFFSET(NsProcView, pointer_y),
        G_STRUCT_OFFSET(NsProcView, drag_anchored),
        G_STRUCT_OFFSET(NsProcView, sb_probe),
        G_STRUCT_OFFSET(NsProcView, sb_dragging),
        G_STRUCT_OFFSET(NsProcView, sb_have_last),
        G_STRUCT_OFFSET(NsProcView, sb_last_x),
        G_STRUCT_OFFSET(NsProcView, sb_last_y),
        G_STRUCT_OFFSET(NsProcView, anim_tick_id),
        G_STRUCT_OFFSET(NsProcView, last_anim_frame_us),
        G_STRUCT_OFFSET(NsProcView, page_animating),
        G_STRUCT_OFFSET(NsProcView, caret_blinking),
    };
    for (size_t i = 0; i < count && i < G_N_ELEMENTS(layout); i++)
        out[i] = layout[i];
}
