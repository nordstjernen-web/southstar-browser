/* Southstar — the tab view's state, shared by procview.c and rust/shell while the view moves to Rust. */

#ifndef SOUTHSTAR_GTK_PROCVIEW_STATE_H
#define SOUTHSTAR_GTK_PROCVIEW_STATE_H

#include "procview.h"
#include "pagelayers.h"
#include "rproc_http.h"
#include "../print.h"

G_BEGIN_DECLS

typedef enum {
    REQ_LOAD, REQ_RENDER, REQ_LINK, REQ_CLICK, REQ_VIEWPORT, REQ_KEY,
    REQ_SELECT, REQ_HOVER, REQ_RELEASE, REQ_FIND, REQ_EXPORT, REQ_CONSOLE,
    REQ_EVAL, REQ_DUMP, REQ_DROPFILES, REQ_SCROLLBAR,
    REQ_WEBGL, REQ_CAMERA, REQ_COLOR_SCHEME, REQ_FAVICON, REQ_VIDEO_EVENT,
    REQ_TICK, REQ_PRINT, REQ_QUIT
} ReqType;
typedef enum { ACT_HOVER, ACT_NAVIGATE, ACT_NEWTAB, ACT_CONTEXT } LinkAct;

enum {
    DEV_TAB_CONSOLE = 0,
    DEV_TAB_NETWORK,
    DEV_TAB_PERFORMANCE,
    DEV_TAB_LAYOUT,
    DEV_TAB_ELEMENTS
};

typedef struct {
    ReqType type;
    int     seq;
    char   *url;
    int     vw;
    int     vh;
    int     w, h, sx, sy;
    double  scale;
    double  dpr;
    double  raster;
    ns_rproc_http_wheel wheel;
    int     x, y;
    int     dx, dy;
    int     mods;
    int     kind;
    int     keycode;
    int     fallback_scroll;
    double  fallback_x;
    double  fallback_y;
    char   *key;
    char   *code;
    LinkAct action;
    char   *query;
    int     find_dir;
    int     find_from_y;
    int     find_case;
    char   *export_dest;
    char   *paths;
    int     dump_tab;
    gboolean inspect;
    gboolean history;
    gboolean user_activated;
    gboolean caret_active;
    ns_rproc_http_tiles_req tiles;
    char   *have;
    char   *hold;
} Req;

typedef enum {
    RES_PAGE, RES_FRAME, RES_LINK, RES_CLICK, RES_VIEWPORT, RES_KEY,
    RES_SELECT, RES_COPY, RES_HOVER, RES_RELEASE, RES_FIND, RES_EXPORT,
    RES_CONSOLE, RES_EVAL, RES_DUMP, RES_FAVICON, RES_DROPFILES,
    RES_SCROLLBAR, RES_TICK, RES_PRINT
} ResType;

typedef struct {
    NsProcView      *view;
    ResType          type;
    int              seq;
    gboolean         ok;
    int              pw, ph;
    char            *title;
    char            *url;
    gboolean         url_pushed;
    char            *nav;
    int              security;
    char            *remote_ip;
    char            *webgl;
    char            *camera;
    char            *download;
    char            *audio;
    char            *window_action;
    char            *clipboard;
    GdkTexture      *texture;
    double           texture_scale;
    NsLayerUpdate   *layers;
    char            *href;
    char            *cursor;
    LinkAct          action;
    int              kind;
    int              prevented;
    int              edit;
    int              fallback_scroll;
    double           fallback_x;
    double           fallback_y;
    gboolean         animating;
    gboolean         caret_blinking;
    gboolean         wheel_snapped;
    gboolean         frame_unchanged;
    int              requested_scroll_y, requested_scroll_x;
    int              find_total, find_current, find_scroll_y;
    char            *media_url;
    int              media_is_video, media_stream;
    unsigned char   *favicon_data;
    int              favicon_w, favicon_h, favicon_stride;
    int              dump_tab;
    gboolean         inspect;
    GPtrArray       *print_pages;
    ns_print_setup   print_setup;
    double           print_scale;
} Res;

struct NsProcView {
    grefcount   rc;

    GtkWidget     *root;
    GtkWidget     *area;
    GtkWidget     *hscroll;
    GtkWidget     *vscroll;
    GtkIMContext  *im;
    GtkAdjustment *hadj;
    GtkAdjustment *vadj;
    gboolean    closed;

    GThread    *thread;
    GAsyncQueue *queue;
    ns_rproc_http *proc;
    GMutex      proc_lock;
    char       *renderer_path;
    gboolean    private_mode;

    NsPvMedia    *media;

    NsProcNotify notify;
    gpointer     notify_ud;

    char       *current_url;
    char       *current_title;
    int         security;
    char       *remote_ip;
    int         page_w, page_h;
    int         scroll_x, scroll_y;
    gboolean    opened;

    GdkTexture      *frame;
    double           frame_scale;
    NsPageLayers    *layers;
    gboolean         tiles_mode;
    gboolean         tiles_fill;
    gboolean         wheel_remote;
    int              scroll_dir;
    gulong           surface_scale_handler;
    GdkSurface      *scale_surface;
    cairo_surface_t *stage[2];
    int              stage_next;

    GdkPaintable    *favicon;

    gboolean    render_inflight;
    gboolean    render_pending;
    gboolean    tick_inflight;
    gboolean    tick_pending;
    int         render_restarts;

    gboolean    link_inflight;
    gboolean    link_pending;
    int         link_pending_x, link_pending_y;
    LinkAct     link_pending_action;

    gboolean    hover_inflight;
    gboolean    hover_pending;
    int         hover_pending_x, hover_pending_y;

    gboolean    has_selection;
    gboolean    multi_click;
    double      ctx_x, ctx_y;
    char       *ctx_link;
    GtkWidget  *ctx_popover;
    GSimpleActionGroup *ctx_actions;

    GtkWidget  *search_revealer;
    GtkWidget  *search_entry;
    GtkWidget  *search_label;
    int         find_seq;
    gboolean    find_case;

    GtkWidget  *overlay;
    GtkWidget  *perm_revealer;
    GtkWidget  *perm_label;
    ReqType     perm_kind;
    gboolean    perm_pending;
    char       *perm_origin;

    GtkWidget    *console_window;
    GtkWidget    *console_notebook;
    GtkWidget    *console_entry;
    GtkWidget    *console_view;
    GtkTextBuffer *console_buffer;
    GtkWidget    *net_view;
    GtkTextBuffer *net_buffer;
    GtkWidget    *perf_view;
    GtkTextBuffer *perf_buffer;
    GtkWidget    *layout_view;
    GtkTextBuffer *layout_buffer;
    GtkWidget    *elements_view;
    GtkTextBuffer *elements_buffer;
    GtkWidget    *inspect_entry;
    gboolean      console_open;
    guint         console_poll_id;

    GPtrArray  *history;
    int         hist_index;
    gboolean    pending_record;

    char       *deferred_url;
    gboolean    deferred_record;
    gboolean    deferred_history;
    gboolean    deferred_user_activated;

    int         js_redirects;

    double      scale;
    gboolean    loading;
    gboolean    busy_cursor;
    GdkCursor  *hourglass_cursor;

    int         load_seq, render_seq, link_seq, click_seq, viewport_seq;
    int         key_seq, select_seq, hover_seq;
    int         last_vp_w, last_vp_h;
    double      last_vp_dpr;
    double      wheel_left_x, wheel_left_y;
    gboolean    wheel_viewport;
    double      wheel_pend_x, wheel_pend_y;
    double      fling_vx, fling_vy;
    guint       wheel_tick_id;
    gint64      wheel_last_us;
    gboolean    adopting_scroll;
    guint       hover_after_scroll_id;
    double      drag_start_x, drag_start_y;
    double      pointer_x, pointer_y;
    gboolean    drag_anchored;
    gboolean    sb_probe, sb_dragging, sb_have_last;
    double      sb_last_x, sb_last_y;

    guint       anim_tick_id;
    gint64      last_anim_frame_us;
    gboolean    page_animating;
    gboolean    caret_blinking;
};

void ns_proc_view_layout(size_t *out, size_t count);
void ns_pv_check_layout(void);

void     ns_pv_input_install_pointer(NsProcView *v);
void     ns_pv_input_install_secondary(NsProcView *v);
void     ns_pv_input_install_keys(NsProcView *v);
void     ns_pv_push_req(NsProcView *v, Req *req);
void     ns_pv_request_link(NsProcView *v, int x, int y, LinkAct action);
void     ns_pv_start_link(NsProcView *v, int x, int y, LinkAct action);
void     ns_pv_request_hover(NsProcView *v, int x, int y);
void     ns_pv_start_hover(NsProcView *v, int x, int y);
void     ns_pv_start_select(NsProcView *v, int kind, int x, int y);
void     ns_pv_start_key_text(NsProcView *v, int kind, const char *text);
void     ns_pv_run_edit_op(NsProcView *v, int op);
gboolean ns_pv_maybe_update_viewport(NsProcView *v);
int      ns_pv_viewport_w(NsProcView *v);
int      ns_pv_viewport_h(NsProcView *v);
double   ns_pv_cur_scale(NsProcView *v);
double   ns_pv_raster_scale(NsProcView *v);
double   ns_pv_page_dpr(NsProcView *v);
int      ns_pv_css_viewport_w(NsProcView *v);
int      ns_pv_css_viewport_h(NsProcView *v);

double   ns_pv_scroll_target_x(const NsProcView *v);
double   ns_pv_scroll_target_y(const NsProcView *v);

GtkWidget  *ns_pv_area_new(NsProcView *v);
void        ns_pv_area_set_view(GtkWidget *area, NsProcView *v);
void        ns_pv_install_view_signals(NsProcView *v);
void        ns_pv_install_area_signals(NsProcView *v);
void        ns_pv_install_scroll(NsProcView *v);
GdkTexture *ns_pv_stage_fill(const unsigned char *px, int w, int h, int stride);
void        ns_pv_set_busy_cursor(NsProcView *v);
void        ns_pv_clear_busy_cursor(NsProcView *v);
void        ns_pv_set_named_cursor(GtkWidget *widget, const char *name);
void        ns_pv_configure_adjustments(NsProcView *v);
void        ns_pv_tiles_apply(NsProcView *v, const Res *res);
void        ns_pv_tiles_leave(NsProcView *v);
void        ns_pv_request_tick(NsProcView *v);
void        ns_pv_request_render(NsProcView *v);
void        ns_pv_start_render(NsProcView *v);
void        ns_pv_stop_wheel_animation(NsProcView *v);
gboolean    ns_pv_wheel_scrolling(NsProcView *v);
void        ns_pv_scroll_view_to(NsProcView *v, double x, double y);
void        ns_pv_arm_anim(NsProcView *v);
void        ns_pv_disarm_anim(NsProcView *v);
void        ns_pv_do_load(NsProcView *v, const char *url, gboolean record,
                          gboolean history, gboolean user_activated);
void     ns_pv_search_open(NsProcView *v);
void     ns_pv_request_find(NsProcView *v, int direction);
void     ns_pv_view_save(NsProcView *v, gboolean pdf);
void     ns_pv_console_set_open(NsProcView *v, gboolean open);

GThread *ns_pv_start_worker(NsProcView *v);
void     ns_pv_perm_resolve(NsProcView *v, gboolean allow);
void     ns_pv_post_emit(NsProcView *v, NsProcEvent evt, const char *text);
void     ns_pv_show_context_menu(NsProcView *v, const char *href, int edit);
void     ns_pv_console_append(NsProcView *v, const char *text);
void     ns_pv_print_run(NsProcView *v, GPtrArray *pages,
                         const ns_print_setup *setup, double raster_scale);

G_END_DECLS

#endif
