/* Southstar — internal JS engine declarations shared between
 * js.c and js_canvas.c. Not a public API.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_JS_INTERNAL_H
#define NS_JS_INTERNAL_H

#include <glib.h>
#include <cairo.h>
#include "ns_pango.h"
#include "ns_quickjs.h"

#include "js.h"
#include "dom.h"
#include "image.h"
#include "layout.h"

typedef struct ns_worker_host ns_worker_host;
typedef enum ns_ho_kind {
    NS_HO_NONE,
    NS_HO_ABORT_CONTROLLER,
    NS_HO_ABORT_SIGNAL,
    NS_HO_BROADCAST_CHANNEL,
    NS_HO_FILE_READER,
    NS_HO_FORM_DATA,
    NS_HO_MESSAGE_CHANNEL,
    NS_HO_MESSAGE_PORT,
    NS_HO_TEXT_ENCODER,
    NS_HO_TEXT_DECODER,
    NS_HO_XHR,
    NS_HO_XHR_UPLOAD,
    NS_HO_KIND_COUNT
} ns_ho_kind;

typedef struct ns_canvas_state {
    int w, h;
    cairo_surface_t *surf;
    cairo_t         *cr;
    double fill_r, fill_g, fill_b, fill_a;
    double stroke_r, stroke_g, stroke_b, stroke_a;
    double line_width;
    char  *font;
    cairo_pattern_t *fill_pattern;
    cairo_pattern_t *stroke_pattern;
    double shadow_r, shadow_g, shadow_b, shadow_a;
    double shadow_blur, shadow_ox, shadow_oy;
    gboolean origin_clean;
    JSValue ctx2d;
    JSContext *jsctx;
    JSRuntime *rt;
    ns_node *owned_node;
    int context_kind;
} ns_canvas_state;
#if GLIB_SIZEOF_VOID_P == 8
G_STATIC_ASSERT(sizeof(ns_canvas_state) == 232);
G_STATIC_ASSERT(G_STRUCT_OFFSET(ns_canvas_state, ctx2d) == 184);
#endif

struct ns_js {
    JSRuntime    *rt;
    JSContext    *ctx;
    JSContext    *main_realm_ctx;
    JSContext    *module_ctx;
    GPtrArray    *frame_ctxs;
    GHashTable   *frame_contexts;
    GHashTable   *frame_windows;
    GArray       *font_ready_resolvers;
    ns_js_log_cb  log_cb;
    gpointer      log_user_data;
    ns_js_mutated_cb mut_cb;
    gpointer      mut_user_data;
    ns_js_navigate_cb nav_cb;
    gpointer      nav_user_data;
    ns_js_download_cb download_cb;
    gpointer      download_user_data;
    ns_js_audio_cb audio_cb;
    gpointer      audio_user_data;
    ns_js_media_seek_cb media_seek_cb;
    gpointer      media_seek_user_data;
    ns_js_media_play_cb media_play_cb;
    gpointer      media_play_user_data;
    ns_js_media_muted_cb media_muted_cb;
    gpointer      media_muted_user_data;
    ns_js_mse_cb  mse_cb;
    gpointer      mse_user_data;
    ns_js_mse_buffered_cb mse_buffered_cb;
    gpointer      mse_buffered_user_data;
    ns_js_mse_remove_cb mse_remove_cb;
    gpointer      mse_remove_user_data;
    ns_js_mse_bytes_cb mse_bytes_cb;
    gpointer      mse_bytes_user_data;
    ns_js_media_volume_cb media_volume_cb;
    gpointer      media_volume_user_data;
    guint         next_audio_token;
    ns_js_scroll_to_cb scroll_to_cb;
    gpointer      scroll_to_user_data;
    ns_js_fragment_nav_cb fragment_nav_cb;
    gpointer      fragment_nav_user_data;
    ns_js_form_submit_cb form_submit_cb;
    gpointer      form_submit_user_data;
    ns_js_soft_nav_cb soft_nav_cb;
    gpointer      soft_nav_user_data;
    ns_js_repaint_cb repaint_cb;
    gpointer      repaint_user_data;
    ns_js_layout_flush_cb layout_flush_cb;
    gpointer      layout_flush_user_data;
    ns_js_viewport_scroll_cb viewport_scroll_cb;
    gpointer      viewport_scroll_user_data;
    gboolean    (*load_delay_cb)(gpointer user_data);
    gpointer      load_delay_user_data;
    gboolean      in_layout_flush;
    guint64       task_epoch;
    ns_js_clipboard_write_cb clipboard_write_cb;
    ns_js_selection_cmd_cb selection_cmd_cb;
    gpointer      selection_cmd_user_data;
    gpointer      clipboard_write_user_data;
    ns_js_window_action_cb window_action_cb;
    gpointer      window_action_user_data;
    const ns_node *pending_fullscreen_event_target;
    JSValue       pending_fullscreen_resolve;
    char         *current_url;
    char         *document_origin;
    ns_node       *current_doc;
    GHashTable    *frame_urls;
    GHashTable    *frame_referrers;
    gpointer       realm_scope_base;
    /* While a frame's code runs, current_url is the frame's URL and the
     * top-level document's URL is held in the slot this points at. */
    char         **top_url_slot;
    ns_node       *current_script;
    char         *early_inject_src;
    gboolean      mutated;
    GHashTable   *timers;
    GMainContext *main_context;
    GPtrArray    *workers;
    ns_worker_host *worker_host;
    int           next_timer_id;
    int           timer_nesting_level;
    int           n_immediate_timers;
    gboolean      running_due_timers;
    GArray       *raf_pending;
    int           next_raf_id;
    gint64        raf_last_us;
    ns_node      *raf_frame_ctx;
    JSValue       pristine_promise;
    GHashTable   *style_table;
    const struct ns_box *layout_root;
    GHashTable   *box_lookup_cache;
    const void   *box_lookup_cache_root;
    const void   *box_lookup_pending_root;
    int           box_lookup_pending_count;
    const ns_node *focused_node;
    const ns_node *change_pending;
    char          *change_baseline;
    /* The document that has focus; NULL means the top-level document. */
    const ns_node *focused_doc;
    /* The innermost focus change in progress (ns_focus_guard in js.c); the
     * node-free hook clears freed nodes from every one. */
    gpointer       focus_guard;
    /* The innermost parser-blocking script run's held-back nodes
     * (ns_parser_hold in js.c); the node-free hook clears freed ones. */
    gpointer       parser_hold;
    gboolean      pointer_input;
    gboolean      autofocus_processed;
    const ns_node *focus_nav_start;
    const ns_node *active_modal;
    GPtrArray     *close_watchers;
    GPtrArray     *modal_dialogs;
    ns_node       *dialog_pointerdown;
    GPtrArray     *popover_auto;
    GPtrArray     *popover_hint;
    GHashTable    *popover_info;
    ns_node       *popover_hint_parent;
    ns_node       *popover_pointerdown;
    gboolean       popover_showing;
    int            popover_hiding_count;
    GArray        *attr_element_refs;
    const ns_node *pointer_lock_element;
    double         last_mouse_x[2];
    double         last_mouse_y[2];
    gboolean       has_last_mouse[2];
    gint64         user_activation_us;
    gboolean       user_ever_activated;
    ns_image_cache *image_cache;
    struct ns_anim *anim;
    GHashTable   *js_image_loads;
    GHashTable   *orphan_nodes;
    GPtrArray    *listeners;
    GHashTable   *listener_index;
    GHashTable   *pinned_wrappers_set;
    GPtrArray    *attr_wrappers;
    GHashTable   *attribute_maps;
    GPtrArray    *pending_xhrs;
    GPtrArray    *pending_ws;
    GPtrArray    *filereader_idles;
    GHashTable   *local_storage;
    GHashTable   *session_storage;
    char         *local_storage_origin;
    char         *local_storage_path;
    gboolean      local_storage_dirty;
    guint         local_storage_flush_source;
    gboolean      local_storage_disabled;
    char         *cookie_value;
    GHashTable   *session_storage_buckets;
    GHashTable   *cookie_buckets;
    char         *partition_key;
    guint64       opaque_counter;
    char         *referrer;
    int           ready_state;
    GHashTable   *doc_ready_states;
    GHashTable   *initial_blank_realms;
    GHashTable   *window_forwards;
    GHashTable   *window_outwards;
    GQueue       *message_tasks;
    GHashTable   *realm_cloners;
    JSValue       navigator_brand;   /* WeakSet of the frame realms'
                                        navigators the Navigator getters
                                        accept */
    guint         message_task_source;
    guint         lifecycle_source;
    GArray       *lifecycle_tasks;
    ns_node      *lifecycle_doc;
    char         *lifecycle_origin;
    int           lifecycle_phase;
    gint64        lifecycle_start_us;
    gint64        eval_deadline_us;
    gint64        js_monitor_deadline_us;
    gboolean      halted;
    gboolean      in_pump;
    gboolean      in_scroll_dispatch;
    GPtrArray    *pending_scrollend;
    gboolean      pending_scrollend_doc;
    int           eval_depth;
    GString      *document_write_buffer;
    ns_node      *document_write_script;
    gboolean      document_write_parser_open;
    GPtrArray    *deferred_script_roots;
    GPtrArray    *async_script_roots;
    guint         async_script_source;
    GPtrArray    *pending_iframe_loads;
    GPtrArray    *deferred_iframe_loads;
    GHashTable   *pending_rejections;
    GHashTable   *reported_rejections;
    GHashTable   *iframe_globals;
    int           iframe_load_depth;
    GArray       *pending_storage_events;
    gboolean      storage_events_draining;
    gint64        last_pump_us;
    gint64        last_orphan_sweep_us;
    int           dispatch_depth;
    /* listener lists copied for a dispatch in progress (kept from sweeps) */
    int           listener_snapshots;
    guint         listener_tombstones;
    int           callback_depth;
    int           synthetic_click_depth;
    GPtrArray    *media_query_lists;
    guint         observer_tick_source;
    guint         raf_tick_source;
    gint64        raf_host_us;
    gboolean      raf_host_driven;
    JSValue       iframe_doc;
    int           iframe_doc_set;
    ns_csp *csp;
    char         *selection_text;
    gboolean      selection_has_range;
    double        selection_x, selection_y, selection_w, selection_h;
    int           module_load_count;
    gsize         module_load_bytes;
    gint64        module_load_deadline_us;
    gboolean      module_load_capped;
    GPtrArray    *import_map;
    gint64        time_origin_us;
    double        time_origin_real_ms;
    ns_js_navigation_timing navigation_timing;
    GPtrArray    *node_iters;
    GHashTable   *console_counts;
    GHashTable   *console_timers;
    GHashTable   *blob_urls;
    GHashTable   *ce_registry;
    const ns_node *ce_main_doc;
    GHashTable   *ce_pending;
    GHashTable   *platform_globals;
    GHashTable   *ce_under_construction;
    ns_node      *ce_upgrading;
    void         *ce_upgrading_wrapper;
    int           ce_in_attr_callback;
    int           ce_defer_upgrades;
    int           throw_on_dynamic_markup;
    int           ignore_destructive_writes;
    int           in_error_report;
    JSValue       nodelist_decorator;
    int           nodelist_decorator_set;
    JSValue       live_html_proto;
    JSValue       live_node_proto;
    JSValue       live_radionode_proto;
    int           live_protos_set;
    JSValue       computed_style_proxy;
    int           computed_style_proxy_set;
    JSValue       url_helper;
    int           url_helper_set;
    JSValue       search_params_helper;
    int           search_params_helper_set;
    JSValue       form_data_helper;
    int           form_data_helper_set;
    JSAtom        atom_capture;
    JSAtom        atom_once;
    JSAtom        atom_signal;
    JSAtom        atom_passive;
    JSAtom        atom_aborted;
    JSAtom        atom_immediate_stopped;
    JSAtom        atom_propagation_stopped;
    int           listener_atoms_set;
    guint64       dom_gen;
    JSValue       proto_node;
    JSValue       proto_element;
    JSValue       proto_htmlelement;
    JSValue       proto_htmlunknownelement;
    JSValue       proto_svgelement;
    JSValue       proto_svgaelement;
    JSValue       proto_mathmlelement;
    JSValue       proto_chardata;
    JSValue       proto_text;
    JSValue       proto_comment;
    JSValue       proto_cdata;
    JSValue       proto_pi;
    JSValue       proto_doctype;
    JSValue       proto_docfrag;
    JSValue       proto_document;
    GHashTable   *per_tag_protos;
    int           dom_protos_set;
    struct {
        const void *root;
        char        kind;
        char       *key;
        guint64     gen;
        JSValue     value;
        int         set;
    } qcache[16];
    int           qcache_next;
    GPtrArray    *current_dispatch_path;
    gboolean      current_dispatch_window;
    gboolean      in_hashchange;
};

static inline ns_js *
js_from_ctx(JSContext *ctx)
{
    return ctx ? (ns_js *)JS_GetContextOpaque(ctx) : NULL;
}

typedef void (*ns_ctx_drawfn)(cairo_t *cr, void *ud);

/* Helpers defined in js.c, used by js_canvas.c */
double ns_arg_d(JSContext *ctx, JSValueConst v);
void ns_bind_fn(JSContext *ctx, JSValueConst obj, const char *name, JSCFunction *fn, int argc);
const ns_box *ns_box_find_by_dom(const ns_box *root, const ns_node *target);
uint32_t ns_js_array_length(JSContext *ctx, JSValueConst arr);
gboolean ns_webaudio_render_offline(JSContext *ctx, JSValueConst destination,
                                    uint32_t frames, double rate, float *out);
void ns_js_promise_reject(JSContext *ctx, JSValue resolvers[2], const char *message);
JSValue ns_make_element(JSContext *ctx, const ns_node *cnode);
const ns_node *ns_unwrap_element(JSValueConst val);

/* Canvas API implemented in js_canvas.c */
JSValue
ns_image_bitmap_make(JSContext *ctx, cairo_surface_t *surf, int w, int h,
                     gboolean origin_clean);
gboolean
ns_image_bitmap_is(JSValueConst v);
JSValue
ns_canvas_clone_object(JSContext *ctx, JSValueConst v);

/* Structured cloning and the worker wire graph (rust/js-clone). */
JSValue ns_structured_clone_transfer(JSContext *ctx, JSValueConst value,
                                     JSValue transfer, JSValueConst seed_from,
                                     JSValueConst seed_to);
JSValue ns_sc_fail(JSContext *ctx);
gboolean ns_sc_buffer_detached(JSContext *ctx, JSValueConst buffer);
JSValue ns_wire_encode_value(JSContext *ctx, JSValueConst value, JSValueConst ports);
JSValue ns_wire_decode_value(JSContext *ctx, JSValueConst wire, JSValueConst ports);
JSValue ns_throw_dom_exception(JSContext *ctx, const char *name, int code,
                               const char *message);
gboolean ns_js_is_host_object(JSValueConst v);
gboolean ns_worker_transfer_is_port(JSContext *ctx, JSValueConst v);

/* FormData, constraint validation, form submission and reset (rust/js-forms). */
JSValue ns_window_form_data_ctor(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv);
void    ns_net_install_form_data(JSContext *ctx, JSValueConst global);
char   *ns_js_form_data_serialize(JSContext *ctx, JSValueConst fd,
                                  gsize *out_len, char **out_content_type);
void    ns_form_listed_controls(const ns_node *form, gboolean include_image,
                                GPtrArray *out);
JSValue ns_validity_get_valid(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_validity(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_validation_message(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_will_validate(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_check_validity(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv);
JSValue ns_element_setCustomValidity(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);
gboolean ns_node_is_submit_trigger(const ns_node *el);
gboolean ns_node_is_reset_trigger(const ns_node *el);
JSValue ns_js_request_submit_form(JSContext *ctx, const ns_node *form,
                                  const ns_node *submitter);
JSValue ns_js_reset_form(JSContext *ctx, ns_node *form);
JSValue ns_element_form_requestSubmit(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_element_form_submit(JSContext *ctx, JSValueConst this_val,
                               int argc, JSValueConst *argv);
JSValue ns_element_form_reset(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv);
JSValue ns_submit_event_ctor(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv);
gboolean ns_js_value_is_form_data(JSContext *ctx, JSValueConst v);
JSValue ns_form_data_construct(JSContext *ctx, JSValueConst new_target);
char   *ns_blob_bytes_as_string(JSContext *ctx, JSValueConst blob, gsize *out_len);
JSContext *ns_js_pattern_context(void);
const ns_node *ns_js_current_document(const ns_js *js);
gboolean ns_js_events_suspended(const ns_js *js);
gboolean ns_js_node_in_page(ns_js *js, const ns_node *node);
gboolean ns_node_sandbox_blocks_forms(const ns_node *node);
void    ns_js_submit_form(ns_js *js, const ns_node *form, const ns_node *submitter);
void    ns_js_clear_children(ns_js *js, ns_node *n);
JSValue ns_make_event(JSContext *ctx, const char *type, const ns_node *target);
JSValue ns_event_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                      JSValueConst *argv);
gboolean ns_js_dispatch_built_event(ns_js *js, const ns_node *target,
                                    const char *type, JSValue event,
                                    gboolean *default_prevented);
JSValue
ns_canvas_throw_dom(JSContext *ctx, const char *name, const char *msg);
int
ns_canvas_dim_from_attr(const ns_node *el, const char *name, int defv);
gboolean
ns_canvas_parse_color(const char *s, double *r, double *g, double *b, double *a);
ns_canvas_state *
ns_canvas_state_for(ns_js *js, const ns_node *el);
ns_canvas_state *
ns_ctx_state(JSContext *ctx, JSValueConst this_val);
gboolean
ns_js_resource_origin_clean(ns_js *js, JSContext *ctx, const char *url,
                            const char *cors_allow_origin);
double
ns_ctx_global_alpha(JSContext *ctx, JSValueConst this_val);
cairo_operator_t
ns_ctx_parse_composite(const char *s);
void
ns_ctx_apply_composite(JSContext *ctx, JSValueConst this_val, cairo_t *cr);
gboolean
ns_ctx_image_smoothing(JSContext *ctx, JSValueConst this_val);
void
ns_ctx_sync_styles(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st);
gboolean
ns_ctx_has_shadow(const ns_canvas_state *st);
void
ns_box_blur_argb(uint8_t *data, int w, int h, int stride, int radius);
void
ns_ctx_with_shadow(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st,
                   ns_ctx_drawfn draw, void *ud);
void
ns_ctx_set_fill_source(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st);
void
ns_ctx_set_stroke_source(JSContext *ctx, JSValueConst this_val, ns_canvas_state *st);
gboolean
ns_value_is_path2d(JSValueConst v);
void
ns_replay_path2d(cairo_t *target, JSValueConst path_v);
cairo_fill_rule_t
ns_parse_fill_rule(const char *s);
cairo_path_t *
ns_ctx_prepare_path_and_rule(JSContext *ctx, cairo_t *cr,
                             int argc, JSValueConst *argv);
void
ns_ctx_restore_path(cairo_t *cr, cairo_path_t *saved);
void
ns_round_rect_subpath(cairo_t *cr, double x, double y, double w, double h,
                      double rtl, double rtr, double rbr, double rbl);
gboolean
ns_extract_radii(JSContext *ctx, JSValueConst v,
                 double *rtl, double *rtr, double *rbr, double *rbl);
void
ns_path2d_parse_svg(cairo_t *cr, const char *d);
JSValue
ns_dommatrix_make(JSContext *ctx, double a, double b, double c, double d,
                  double e, double f);
JSValue
ns_element_getContext(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv);
cairo_status_t
ns_canvas_png_write(void *closure, const unsigned char *data, unsigned int length);


/* The canvas objects' WebIDL surface (js_canvas_api.c). */
enum {
    NS_HK_CTX2D,
    NS_HK_OFFSCREEN_CTX2D,
    NS_HK_GRADIENT,
    NS_HK_PATTERN,
    NS_HK_IMAGEDATA,
    NS_HK_TEXTMETRICS,
    NS_HK_OFFSCREEN,
    NS_HK_ACTIVEINFO,
    NS_HK_PRECISION,
};
void ns_canvas_register_classes(JSRuntime *rt);
void ns_canvas_state_adopt_node(ns_js *js, ns_node *el);
void ns_canvas_states_teardown(ns_js *js);
gpointer ns_hidden_ptr(JSValueConst v);
void ns_hidden_set_ptr(JSValueConst v, gpointer ptr);
JSValue ns_hidden_new(JSContext *realm, int kind, JSValueConst proto);
gboolean ns_hidden_is(JSValueConst v, int kind);
JSValue ns_hget(JSContext *ctx, JSValueConst obj, const char *name);
void ns_hset(JSContext *ctx, JSValueConst obj, const char *name, JSValue val);
gboolean ns_ctx2d_is(JSValueConst v);
JSContext *ns_canvas_realm(JSContext *ctx, const ns_node *el);
JSContext *ns_ctx_realm(JSContext *ctx, JSValueConst this_val);
JSContext *ns_js_realm_for_node(ns_js *js, const ns_node *node);
char *ns_js_computed_text(JSContext *ctx, const ns_node *node, const char *name);
JSValue ns_api_proto(JSContext *realm, const char *iface);
JSValue ns_api_proto_of_ctor(JSContext *ctx, JSValueConst new_target,
                             const char *iface);
JSValue ns_api_throw_new_required(JSContext *ctx, const char *iface);
JSValue ns_api_interface(JSContext *ctx, JSValueConst global, const char *name,
                         JSValue ctor, const char *parent);
char *ns_canvas_color_string(const char *css);
gboolean ns_canvas_filter_valid(const char *s);
gboolean ns_canvas_length_valid(const char *s);
char *ns_canvas_font_string(const char *css);
void ns_ctx2d_init_state(JSContext *ctx, JSValueConst obj);
JSValue ns_ctx2d_new(JSContext *ctx, const ns_node *el, JSValueConst canvas_obj,
                     gboolean offscreen, JSValue attrs);
JSValue ns_gradient_new(JSContext *ctx, JSContext *realm, const char *type);
JSValue ns_pattern_new(JSContext *ctx, JSContext *realm, JSValueConst source,
                       const char *repetition);
JSValue ns_textmetrics_new(JSContext *ctx, JSContext *realm, const double v[10]);
JSValue ns_imagedata_wrap(JSContext *ctx, JSContext *realm, JSValueConst proto,
                          int w, int h, JSValue data, const char *color_space);
JSValue ns_imagedata_new(JSContext *ctx, JSContext *realm, int w, int h,
                         const uint8_t *rgba);
JSValue ns_imagedata_construct(JSContext *ctx, JSValueConst new_target, int argc,
                               JSValueConst *argv);
void ns_offscreen_sync_size(JSContext *ctx, JSValueConst obj);
JSValue ns_offscreen_construct(JSContext *ctx, JSValueConst new_target, int argc,
                               JSValueConst *argv);
void ns_canvas_install(JSContext *ctx, JSValueConst global, gboolean window);

/* Performance API (js_perf.c) and the js.c helpers it shares. */
void ns_bind_fn_if_not_callable(JSContext *ctx, JSValueConst obj, const char *name,
                                JSCFunction *fn, int argc);
JSValue ns_own_data_props_toJSON(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv);
gboolean ns_js_get_bool_prop(JSContext *ctx, JSValueConst obj, const char *key,
                             gboolean *was_set);

double ns_perf_now_ms(const ns_js *js);
gint64 ns_js_time_origin_us(const ns_js *js, gconstpointer realm);
double ns_js_time_origin_real_ms(const ns_js *js, gconstpointer realm);
double ns_perf_realm_now_ms(JSContext *ctx);
void   ns_js_start_frame_clock(ns_js *js, gconstpointer frame);
void   ns_js_adopt_frame_clock(ns_js *js, gconstpointer frame, JSContext *ctx);
void   ns_js_clear_frame_clocks(ns_js *js, gboolean destroy);
double ns_perf_relative_ms(gint64 now_us, gint64 origin_us);
/* What a resource timing entry needs beyond its times and response.
 * timeline names the performance timeline that gets the entry: a frame
 * realm's JSContext, an opaque key for a frame without a realm, or NULL
 * for the page's own.  document_url is that document's URL, for the
 * same-origin and Timing-Allow-Origin checks.  Without a response (an
 * image the image cache fetched) the protocol, the Timing-Allow-Origin
 * value, the status and the size come from here. */
typedef struct ns_perf_resource_info {
    gconstpointer timeline;
    const char   *document_url;
    gboolean      render_blocking;
    gboolean      cors_mode;
    const char   *next_hop_protocol;
    const char   *timing_allow_origin;
    long          status;
    gint64        body_size;
} ns_perf_resource_info;
G_STATIC_ASSERT(sizeof(ns_perf_resource_info) == 56);
void ns_perf_init(ns_js *js);
void ns_perf_reset_observers(ns_js *js);
void ns_perf_teardown(ns_js *js);
gint64 ns_js_page_time_origin_us(const ns_js *js);
double ns_js_page_time_origin_real_ms(const ns_js *js);
JSContext *ns_js_main_realm(const ns_js *js);
JSContext *ns_js_main_context(const ns_js *js);
gboolean ns_js_is_worker(const ns_js *js);
void ns_js_mark_mutated(ns_js *js);
const ns_js_navigation_timing *ns_js_page_navigation_timing(const ns_js *js);
void ns_js_log_line(ns_js *js, const char *line);
struct ns_response;
void ns_perf_add_resource_timed(ns_js *js, const ns_perf_resource_info *info,
                                const char *url, const char *initiator,
                                gint64 start_us, gint64 end_us,
                                const struct ns_response *resp);
gboolean ns_perf_has_resource(ns_js *js, gconstpointer timeline,
                              const char *url, const char *initiator);
void ns_perf_move_timeline(ns_js *js, gconstpointer from, gconstpointer to);
JSValue ns_perf_new_performance_object(JSContext *ctx);
void    ns_perf_set_performance_objects(JSContext *ctx, JSValueConst perf,
                                        JSValue timing, JSValue navigation,
                                        JSValue event_counts);
JSValue ns_window_performance_time_origin_get(JSContext *ctx,
                                              JSValueConst this_val,
                                              int argc, JSValueConst *argv);
JSValue ns_window_performance_object_get(JSContext *ctx,
                                         JSValueConst this_val, int argc,
                                         JSValueConst *argv, int magic);
JSValue ns_perf_supported_entry_types(JSContext *ctx);
void ns_perf_install_entry_list(JSContext *ctx, JSValueConst global);
JSValue ns_perf_observer_ctor(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv);
JSValue ns_perf_observer_observe(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv);
JSValue ns_perf_observer_disconnect(JSContext *ctx, JSValueConst this_val,
                                    int argc, JSValueConst *argv);
JSValue ns_perf_observer_takeRecords(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);
JSValue ns_window_performance_now(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv);
JSValue ns_window_performance_mark(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
JSValue ns_window_performance_measure(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_window_performance_clearMarks(JSContext *ctx, JSValueConst this_val,
                                         int argc, JSValueConst *argv);
JSValue ns_window_performance_clearMeasures(JSContext *ctx, JSValueConst this_val,
                                            int argc, JSValueConst *argv);
JSValue ns_window_performance_clearResourceTimings(JSContext *ctx,
                                                   JSValueConst this_val,
                                                   int argc, JSValueConst *argv);
JSValue ns_window_performance_getEntries(JSContext *ctx, JSValueConst this_val,
                                         int argc, JSValueConst *argv);
JSValue ns_window_performance_getEntriesByName(JSContext *ctx, JSValueConst this_val,
                                               int argc, JSValueConst *argv);
JSValue ns_window_performance_getEntriesByType(JSContext *ctx, JSValueConst this_val,
                                               int argc, JSValueConst *argv);
JSValue ns_window_performance_memory_get(JSContext *ctx, JSValueConst this_val,
                                         int argc, JSValueConst *argv);

/* MutationObserver, IntersectionObserver and ResizeObserver
 * (rust/js-observers) and the js.c and layout helpers they call. */
JSValue ns_mutation_observer_ctor(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv);
JSValue ns_mutation_observer_observe(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);
JSValue ns_mutation_observer_disconnect(JSContext *ctx, JSValueConst this_val,
                                        int argc, JSValueConst *argv);
JSValue ns_mutation_observer_takeRecords(JSContext *ctx, JSValueConst this_val,
                                         int argc, JSValueConst *argv);
JSValue ns_intersection_observer_ctor(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_intersection_observer_observe(JSContext *ctx, JSValueConst this_val,
                                         int argc, JSValueConst *argv);
JSValue ns_intersection_observer_unobserve(JSContext *ctx, JSValueConst this_val,
                                           int argc, JSValueConst *argv);
JSValue ns_intersection_observer_disconnect(JSContext *ctx, JSValueConst this_val,
                                            int argc, JSValueConst *argv);
JSValue ns_intersection_observer_takeRecords(JSContext *ctx, JSValueConst this_val,
                                             int argc, JSValueConst *argv);
JSValue ns_resize_observer_ctor(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_resize_observer_observe(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
JSValue ns_resize_observer_unobserve(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);
JSValue ns_resize_observer_disconnect(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
void ns_mut_record_emit(ns_js *js, const char *type, ns_node *target,
                        ns_node *added, ns_node *removed,
                        ns_node *previous_sibling, ns_node *next_sibling,
                        const char *attr_name, const char *attr_namespace,
                        const char *old_value);
void ns_mut_record_emit_child_list_arrays(ns_js *js, ns_node *target,
                                          GPtrArray *added_nodes,
                                          GPtrArray *removed_nodes,
                                          ns_node *previous_sibling,
                                          ns_node *next_sibling);
void ns_mut_scrub_node(ns_js *js, ns_node *n);
gboolean ns_mutation_drain_pending(const ns_js *js);
void ns_intersection_observers_tick(ns_js *js);
void ns_resize_observers_tick(ns_js *js);
void ns_observers_reset(ns_js *js);
void ns_observers_teardown(ns_js *js);
void ns_observer_schedule_tick(ns_js *js);
JSValue ns_js_call_observer(ns_js *js, JSContext *ctx, JSValueConst cb,
                            JSValueConst this_val, int argc, JSValueConst *argv,
                            const char *report_type, gboolean fresh_budget);
void ns_node_arm_js_invalidate(ns_node *n);
const ns_box *ns_js_layout_root(const ns_js *js);
const ns_box *ns_box_for_this(JSContext *ctx, JSValueConst this_val);
void ns_box_border_box(const ns_box *b, double *x, double *y, double *w, double *h);
void ns_box_visual_border_box(const ns_box *box,
                              double *x, double *y, double *w, double *h);
void ns_box_visual_padding_box(const ns_box *box,
                               double *x, double *y, double *w, double *h);
JSValue ns_make_dom_rect(JSContext *ctx, double x, double y, double w, double h);

/* History, the Navigation API and navigation between windows and frames
 * (rust/js-window) and the js.c helpers they call. */
void ns_window_install_history(JSContext *ctx, JSValueConst global);
void ns_window_history_teardown(ns_js *js);
void ns_js_set_current_url(ns_js *js, const char *url);
void ns_js_soft_navigate(ns_js *js, const char *url, gboolean replace);
gboolean ns_js_navigate(ns_js *js, const char *url, gboolean reload);
gboolean ns_js_window_events_blocked(const ns_js *js);
void ns_js_dispatch_document_window_event(ns_js *js, const char *type,
                                          JSValue event);
JSValue ns_window_structured_clone(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
JSValue ns_target_make_event(JSContext *ctx, JSValueConst target,
                             const char *type);
void ns_target_dispatch_with_event(JSContext *ctx, JSValueConst obj,
                                   const char *type, JSValueConst ev);
void ns_bind_event_target_listeners(JSContext *ctx, JSValueConst obj);
JSValue ns_make_window_event(JSContext *ctx, const char *type);
JSValue ns_window_make_location(JSContext *ctx);
JSValue ns_window_open_method(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv);
gboolean ns_js_has_transient_activation(ns_js *js);
void ns_js_consume_user_activation(ns_js *js);
const char *ns_js_top_url(ns_js *js);
void ns_js_set_top_url(ns_js *js, const char *url);
gboolean ns_js_url_parses(ns_js *js, const char *url);
gboolean ns_js_anchor_fragment_navigate(ns_js *js, const char *abs_url);
gboolean ns_js_in_frame_load(const ns_js *js);
gboolean ns_js_can_navigate(const ns_js *js);
void ns_js_fragment_navigated(ns_js *js, const char *url);

/* MessagePort, MessageChannel and BroadcastChannel (rust/js-workers) and the
 * js.c helpers they call. */
typedef struct {
    JSValue  phase;
    JSValue  current;
    gboolean nested;
} ns_event_at_target;
void ns_event_at_target_begin(JSContext *ctx, JSValueConst ev, JSValueConst target,
                              ns_event_at_target *st);
void ns_event_at_target_end(JSContext *ctx, JSValueConst ev, ns_event_at_target *st);
JSValue ns_port_deliver_job(JSContext *ctx, int argc, JSValueConst *argv);
JSValue ns_port_new(JSContext *ctx);
guint64 ns_port_bridge_id(JSContext *ctx, JSValueConst port);
int     ns_port_transfer_prepare(JSContext *ctx, JSValueConst transfer,
                                 JSValueConst source_port, JSContext *realm,
                                 JSValue *old_ports, JSValue *new_ports);
void    ns_port_transfer_commit(JSContext *ctx, JSValueConst old_ports,
                                JSValueConst new_ports);
JSValue ns_window_message_channel(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv);
JSValue ns_window_broadcast_channel(JSContext *ctx, JSValueConst this_val,
                                    int argc, JSValueConst *argv);
JSValue ns_port_add_event_listener(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
JSValue ns_port_remove_event_listener(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
void    ns_net_install_ports(JSContext *ctx, JSValueConst global);
JSValue ns_port_bridge_send(JSContext *ctx, JSValueConst port, guint64 id,
                            JSValueConst data);
JSContext *ns_target_handler_realm(JSContext *ctx, JSValueConst obj,
                                   const char *type, const char *listener_key);
JSValue ns_event_new(JSContext *ctx);
void    ns_event_define_cancel_bubble(JSContext *ctx, JSValueConst ev);
gint64  ns_js_budget_enter(ns_js *js);
void    ns_js_budget_leave(ns_js *js, gint64 saved);
void    ns_js_queue_message_task(JSContext *ctx, JSJobFunc *func, int argc,
                                 JSValueConst *argv);
gboolean ns_js_value_is_message_port(JSValueConst v);
gboolean ns_js_value_is_broadcast_channel(JSValueConst v);
JSValue ns_message_port_state(JSContext *ctx, JSValueConst v);
JSValue ns_message_port_new_object(JSContext *ctx);
JSValue ns_message_channel_construct(JSContext *ctx, JSValueConst new_target);
JSValue ns_broadcast_channel_construct(JSContext *ctx, JSValueConst new_target);
gboolean ns_listener_parse_options(JSContext *ctx, JSValueConst opts,
                                   gboolean *capture, gboolean *once,
                                   gboolean *passive, gboolean *passive_set,
                                   JSValue *signal_out, gboolean strict_signal);
void    ns_listeners_compact_dead(JSContext *ctx, JSValueConst owner);

/* navigator and the other window services (rust/js-services) and the
 * js.c helpers they call. */
JSValue ns_services_window_navigator(JSContext *ctx);
JSValue ns_services_worker_navigator(JSContext *ctx);
gboolean ns_services_chrome_compat(void);
JSValue ns_target_dispatchEvent(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_navigator_sendBeacon(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_eme_request_access(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv);
JSValue ns_media_capabilities_info(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
gboolean ns_js_user_activation_state(ns_js *js, gboolean *ever_activated);
int ns_js_clipboard_write(ns_js *js, const char *text);

/* fetch, Request, Response and AbortController (rust/js-net), and the js.c
 * helpers they call. */
JSValue ns_js_fetch(JSContext *ctx, JSValueConst this_val, int argc,
                    JSValueConst *argv);
JSValue ns_window_response_ctor(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_window_request_ctor(JSContext *ctx, JSValueConst this_val,
                               int argc, JSValueConst *argv);
void    ns_fetch_install_interfaces(JSContext *ctx, JSValueConst global);
JSValue ns_window_abort_controller_ctor(JSContext *ctx, JSValueConst this_val,
                                        int argc, JSValueConst *argv);
void    ns_install_abort_signal_interface(JSContext *ctx, JSValueConst global);
void    ns_js_net_reset(ns_js *js);
void    ns_js_net_teardown(ns_js *js);
guint   ns_js_net_pending_fetches(const ns_js *js);
void    ns_js_net_sw_fetch_result(ns_js *js, guint id, int outcome, long status,
                                  const char *content_type,
                                  const char *raw_headers, const guint8 *body,
                                  gsize body_len, const char *error);
gboolean ns_cors_allows(const char *doc_url, const char *resp_url,
                        const char *cors_header);
gboolean ns_header_value_is_safe(const char *value);
const char *ns_js_net_page_url(const ns_js *js);
gboolean ns_js_net_csp_allows_connect(const ns_js *js, const char *url,
                                      const char *page);
gboolean ns_js_net_host_is(JSValueConst v, int kind);
void     ns_drain_mutations(ns_js *js);
guint    ns_js_attach_idle(ns_js *js, GSourceFunc func, gpointer data);
guint    ns_js_attach_timeout(ns_js *js, guint ms, GSourceFunc func, gpointer data);
GBytes  *ns_js_blob_url_lookup(ns_js *js, const char *url, char **out_type);
ns_worker_host *ns_sw_controller_for(ns_js *js, const char *abs_url);
void     ns_sw_post_fetch_request(ns_worker_host *host, guint id,
                                  const char *url, const char *method,
                                  const char *const *headers,
                                  const guint8 *body, gsize body_len);
const char *ns_js_realm_url(ns_js *js, JSContext *realm);
const char *ns_js_realm_document_url(ns_js *js, JSContext *realm);
char    *ns_js_body_bytes(JSContext *ctx, JSValueConst value, gsize *out_len);
gboolean ns_js_value_is_url_search_params(JSContext *ctx, JSValueConst v);
char    *ns_js_usp_serialize(JSContext *ctx, JSValueConst usp, gsize *out_len,
                             char **out_content_type);
void     ns_target_fire_event(JSContext *ctx, JSValueConst obj, const char *type);
JSValue  ns_ho_construct(JSContext *ctx, JSValueConst new_target, ns_ho_kind kind);
JSValue  ns_ho_new_default(JSContext *ctx, ns_ho_kind kind);
void     ns_bind_ctor(JSContext *ctx, JSValueConst obj, const char *name,
                      JSCFunction *fn, int argc);
JSValue  ns_illegal_constructor(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue  ns_make_abort_error(JSContext *ctx);
#endif
