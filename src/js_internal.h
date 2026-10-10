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
    GMainContext *main_context;
    ns_worker_host *worker_host;
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
    GHashTable   *attribute_maps;
    GPtrArray    *filereader_idles;
    char         *cookie_value;
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
    gint64        last_pump_us;
    gint64        last_orphan_sweep_us;
    int           dispatch_depth;
    /* listener lists copied for a dispatch in progress (kept from sweeps) */
    int           listener_snapshots;
    guint         listener_tombstones;
    int           callback_depth;
    int           synthetic_click_depth;
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
    GHashTable   *blob_urls;
    const ns_node *ce_main_doc;
    GHashTable   *platform_globals;
    int           throw_on_dynamic_markup;
    int           ignore_destructive_writes;
    int           in_error_report;
    JSValue       nodelist_decorator;
    int           nodelist_decorator_set;
    JSValue       live_html_proto;
    JSValue       live_node_proto;
    JSValue       live_radionode_proto;
    int           live_protos_set;
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
typedef struct {
    gboolean checked;
    gboolean indeterminate;
    ns_node *checked_radio;
} ns_checkable_click_state;
typedef enum {
    NS_RANGE_MIN,
    NS_RANGE_MAX,
    NS_RANGE_LOW,
    NS_RANGE_HIGH,
    NS_RANGE_OPTIMUM,
} ns_range_number_prop;
JSValue ns_element_get_label_control(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selection_dir(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_default_value(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_default_checked(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_default_selected(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selected(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_value_as_number(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_value_as_date(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_checked(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_indeterminate(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_progress_position(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_value_prop(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_label_prop(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selectedIndex(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_options(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selectedOptions(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selection_start(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_get_selection_end(JSContext *ctx, JSValueConst this_val);
JSValue ns_text_control_get_text_length(JSContext *ctx, JSValueConst this_val);
JSValue ns_element_set_default_value(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_default_checked(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_default_selected(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_selected(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_value_as_number(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_value_as_date(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_checked(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_indeterminate(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_value_prop(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_label_prop(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_selectedIndex(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_selection_start(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_selection_end(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_set_selection_dir(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue ns_element_range_number_getter(JSContext *ctx, JSValueConst this_val, int magic);
JSValue ns_element_range_number_setter(JSContext *ctx, JSValueConst this_val,
                                       JSValueConst val, int magic);
JSValue ns_input_select(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue ns_input_setSelectionRange(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
JSValue ns_input_setRangeText(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv);
JSValue ns_input_stepUp(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue ns_input_stepDown(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue ns_options_item(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue ns_options_namedItem(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv);
JSValue ns_select_add(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
void     ns_input_resanitize_value(ns_node *el);
gboolean ns_js_node_is_labelable(const ns_node *n);
const ns_node *ns_js_first_labelable_descendant(const ns_node *n, int depth);
const ns_node *ns_label_associated_control(const ns_node *label);
int      ns_checkable_input_kind(const ns_node *el);
void     ns_checkable_pre_click(ns_js *js, ns_node *el, int kind,
                                ns_checkable_click_state *state);
void     ns_checkable_post_click(ns_js *js, ns_node *el, int kind,
                                 const ns_checkable_click_state *state,
                                 gboolean prevented);
void     ns_js_set_checkedness(ns_js *js, ns_node *n, gboolean checked);
JSValue  ns_array_item(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_array_namedItem(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv);
void     ns_element_insert_before_single(ns_js *js, ns_node *parent, ns_node *newc,
                                         ns_node *ref);
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
void ns_window_bind_post_message(JSContext *ctx, JSValueConst global);
JSValue ns_window_make_post_message(JSContext *ctx, JSValueConst window);
void ns_window_links_clear(ns_js *js, gboolean destroy);
void ns_window_link_outward(ns_js *js, JSValueConst outward,
                            JSValueConst realm_window);
JSValue ns_window_forward_of(ns_js *js, JSValueConst outward);
ns_node *ns_window_frame_node(ns_js *js, JSValueConst win);
JSValue ns_iframe_cross_origin_window(JSContext *ctx, JSValue target);
JSContext *ns_js_main_realm_context(const ns_js *js);
gboolean ns_iframe_origin_is_opaque(ns_node *frame);
const char *ns_js_document_origin(const ns_js *js);
const char *ns_js_frame_url(const ns_js *js, const ns_node *frame);
JSContext *ns_js_frame_context(const ns_js *js, const ns_node *frame);
void *ns_js_realm_url_enter(ns_js *js, JSContext *realm);
void ns_js_realm_url_leave(ns_js *js, void *token);
void ns_js_dispatch_main_window_event(ns_js *js, const char *type,
                                      JSValue event);

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
JSValue ns_media_set_media_keys(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
void ns_media_install_audio(JSContext *ctx, JSValueConst global);
JSValue ns_media_capabilities_info(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
void ns_media_init(ns_js *js);
void ns_media_teardown(ns_js *js);
guint ns_media_native_formats(void);
JSValue ns_media_canPlayType(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv);
JSValue ns_media_source_is_type_supported(JSContext *ctx, JSValueConst this_val,
                                          int argc, JSValueConst *argv);
JSValue ns_media_play(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv);
JSValue ns_media_pause(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv);
JSValue ns_media_load(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv);
JSValue ns_media_fast_seek(JSContext *ctx, JSValueConst this_val,
                           int argc, JSValueConst *argv);
JSValue ns_media_get_video_playback_quality(JSContext *ctx, JSValueConst this_val,
                                            int argc, JSValueConst *argv);
JSValue ns_window_mse_append(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv);
JSValue ns_window_mse_eos(JSContext *ctx, JSValueConst this_val,
                          int argc, JSValueConst *argv);
JSValue ns_window_mse_buffered(JSContext *ctx, JSValueConst this_val,
                               int argc, JSValueConst *argv);
JSValue ns_window_mse_buffered_start(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);
JSValue ns_window_mse_remove(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv);
JSValue ns_window_mse_bytes(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv);
JSValue ns_media_get_paused(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_ended(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_seeking(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_readyState(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_networkState(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_current_time(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_duration(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_error(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_seekable_ranges(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_buffered_ranges(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_played_ranges(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_playbackRate(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_defaultPlaybackRate(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_volume(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_muted(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_get_srcObject(JSContext *ctx, JSValueConst this_val);
JSValue ns_media_set_current_time(JSContext *ctx, JSValueConst this_val,
                                  JSValueConst val);
JSValue ns_media_set_playbackRate(JSContext *ctx, JSValueConst this_val,
                                  JSValueConst val);
JSValue ns_media_set_defaultPlaybackRate(JSContext *ctx, JSValueConst this_val,
                                         JSValueConst val);
JSValue ns_media_set_volume(JSContext *ctx, JSValueConst this_val,
                            JSValueConst val);
JSValue ns_media_set_muted(JSContext *ctx, JSValueConst this_val,
                           JSValueConst val);
JSValue ns_media_set_srcObject(JSContext *ctx, JSValueConst this_val,
                               JSValueConst val);
gboolean ns_js_user_activation_state(ns_js *js, gboolean *ever_activated);
int ns_js_clipboard_write(ns_js *js, const char *text);
JSValue ns_services_alert(JSContext *ctx, JSValueConst this_val,
                          int argc, JSValueConst *argv);
JSValue ns_services_queue_microtask(JSContext *ctx, JSValueConst this_val,
                                    int argc, JSValueConst *argv);
JSValue ns_services_notification_ctor(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_services_match_media(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
void ns_services_install_console(JSContext *ctx, JSValueConst global);
void ns_services_install_screen(JSContext *ctx, JSValueConst global);
void ns_services_install_rtc(JSContext *ctx, JSValueConst global);
void ns_services_install_clipboard_item(JSContext *ctx, JSValueConst global);
void ns_services_console_emit(ns_js *js, const char *prefix, JSContext *ctx,
                              int argc, JSValueConst *argv);
void ns_services_screen_metrics(int *width, int *height,
                                int *avail_width, int *avail_height,
                                int *avail_left, int *avail_top);
void ns_services_reeval_media_queries(ns_js *js);
void ns_services_reset(ns_js *js);
void ns_services_teardown(ns_js *js);
gboolean ns_js_log_enabled(const ns_js *js);
JSValue ns_services_set_timeout(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_services_set_interval(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv);
JSValue ns_services_clear_timer(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv);
JSValue ns_services_request_idle_callback(JSContext *ctx, JSValueConst this_val,
                                          int argc, JSValueConst *argv);
void ns_services_install_idle_deadline(JSContext *ctx, JSValueConst proto);
void ns_services_run_due_timers(ns_js *js);
gboolean ns_services_timers_pending(const ns_js *js, gboolean include_idle);
guint ns_services_timer_count(const ns_js *js);
void ns_services_purge_frame_timers(ns_js *js, const ns_node *frame);
void ns_services_timer_remove(ns_js *js, int id);

enum { NS_TIMER_RUN, NS_TIMER_WAIT, NS_TIMER_DROP };
typedef struct ns_timer_scope ns_timer_scope;
gboolean ns_js_due_timers_allowed(const ns_js *js);
int ns_js_timer_gate(ns_js *js, ns_node *frame, gboolean idle_expired);
ns_timer_scope *ns_js_timer_scope_enter(ns_js *js, JSContext *ctx, ns_node *frame);
JSContext *ns_js_timer_scope_context(const ns_timer_scope *scope);
void ns_js_timer_scope_leave(ns_timer_scope *scope, gboolean threw,
                             JSValueConst exception);
gboolean ns_timer_this_is_detached_window(ns_js *js, JSContext *ctx,
                                          JSValueConst this_val);
ns_node *ns_js_context_frame(ns_js *js, JSContext *ctx);
gint64 ns_js_idle_frame_end(const ns_js *js, gint64 now, gint64 end);
void ns_js_source_remove(ns_js *js, guint id);
GMainContext *ns_js_glib_context(const ns_js *js);
void ns_event_adopt_interface(JSContext *ctx, JSValueConst ev, const char *iface);

/* fetch, Request, Response, AbortController, XMLHttpRequest, WebSocket,
 * EventSource and the network interfaces (rust/js-net), and the js.c helpers
 * they call. */
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
guint   ns_js_net_pending_xhrs(const ns_js *js);
guint   ns_js_net_pending_sockets(const ns_js *js);
JSValue ns_window_websocket_ctor(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv);
JSValue ns_window_eventsource_ctor(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv);
void    ns_js_net_install_sockets(JSContext *ctx, JSValueConst global);
void    ns_net_install_interfaces(JSContext *ctx, JSValueConst global);
gpointer ns_js_net_enter_handler_realm(JSContext *ctx, JSValueConst obj,
                                       const char *type);
void     ns_js_net_leave_handler_realm(JSContext *ctx, gpointer scope);
void     ns_ho_install_attrs(JSContext *ctx, JSValueConst global);
void     ns_net_install_file_reader(JSContext *ctx, JSValueConst global);
JSValue ns_window_xhr_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                           JSValueConst *argv);
void    ns_xhr_install_interface(JSContext *ctx, JSValueConst global);
JSValue ns_js_net_host_state(JSContext *ctx, JSValueConst v, int kind);
gboolean ns_js_net_pump_iteration(ns_js *js);
void     ns_xhr_fire_progress_event(JSContext *ctx, JSValueConst target,
                                    const char *type, double loaded,
                                    double total, gboolean length_computable);
void     ns_js_credit_pumped_time(ns_js *js, gint64 pump_start_us);
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

/* Dedicated and service workers (rust/js-workers) and the js.c realm
 * installers and helpers they call. */
typedef struct ns_worker_realm {
    ns_worker_host *host;
    GMainContext   *context;
    const char     *url;
    const char     *base_url;
    const char     *origin;
    const char     *name;
    gboolean        is_service_worker;
} ns_worker_realm;
ns_js  *ns_worker_js_new(const ns_worker_realm *p);
int     ns_worker_js_eval(ns_js *js, const char *src, gsize len, const char *url,
                          gboolean module, JSValue *exception);
ns_worker_host *ns_js_worker_host(const ns_js *js);
void    ns_js_halt(ns_js *js);
gboolean ns_js_csp_allows_worker(ns_js *js, const char *url);
void    ns_js_report_error_event_in(ns_js *js, JSContext *ctx, const char *message,
                                    const char *filename, int lineno, int colno);
void    ns_js_dispatch_engine_event(JSContext *ctx, JSValueConst target,
                                    JSValueConst ev);
void    ns_drain_microtasks(ns_js *js);
char   *ns_js_exception_message(JSContext *ctx, JSValueConst ex);
JSValue ns_make_ctor(JSContext *ctx, JSCFunction *fn, const char *name, int argc);
void    ns_install_namespace_object(JSContext *ctx, JSValueConst global,
                                    const char *name, JSValue obj, const char *tag);
void    ns_js_link_interface_ctors(JSContext *ctx);
void    ns_js_lock_global_prototypes(JSContext *ctx);
char   *ns_js_doc_base_url(ns_js *js);
char   *ns_js_decode_data_url(const char *url, gsize *out_len);
JSValue ns_event_prevent_default(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue ns_event_stop_propagation(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue ns_event_stop_immediate(JSContext *ctx, JSValueConst this_val, int argc,
                                JSValueConst *argv);
JSValue ns_event_composed_path(JSContext *ctx, JSValueConst this_val, int argc,
                               JSValueConst *argv);
JSValue ns_target_addEventListener(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue ns_target_removeEventListener(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_returns_resolved_undefined(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_returns_resolved_false(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv);
JSValue ns_returns_resolved_empty_array(JSContext *ctx, JSValueConst this_val,
                                        int argc, JSValueConst *argv);
JSValue ns_cache_open(JSContext *ctx, JSValueConst this_val, int argc,
                      JSValueConst *argv);
JSValue ns_window_event_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                             JSValueConst *argv);
void    ns_worker_install_constructor(JSContext *ctx, JSValueConst global);
void    ns_sw_install_container(JSContext *ctx, JSValueConst navigator);
gboolean ns_worker_report_exception(ns_js *js, JSValueConst ex);
gboolean ns_worker_host_closing(const ns_worker_host *host);
const char *ns_worker_host_base_url(const ns_worker_host *host);
gboolean ns_workers_pending(const ns_js *js);
void    ns_workers_teardown(ns_js *js);
void    ns_worker_log_cb(const char *line, gpointer user_data);
void    ns_worker_promise_rejection_tracker(JSContext *ctx, JSValueConst promise,
                                            JSValueConst reason,
                                            ns_js_bool is_handled, void *opaque);
JSValue ns_worker_global_post_message(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_worker_global_close(JSContext *ctx, JSValueConst this_val, int argc,
                               JSValueConst *argv);
JSValue ns_worker_import_scripts(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue ns_worker_report_error(JSContext *ctx, JSValueConst this_val, int argc,
                               JSValueConst *argv);
JSValue ns_worker_performance_now(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue ns_worker_performance_entries(JSContext *ctx, JSValueConst this_val,
                                      int argc, JSValueConst *argv);
JSValue ns_worker_performance_clear(JSContext *ctx, JSValueConst this_val,
                                    int argc, JSValueConst *argv);
void    ns_worker_install_console(JSContext *ctx, JSValueConst global);
void    ns_worker_install_location(JSContext *ctx, JSValueConst global,
                                   const char *url);
void    ns_sw_install_scope(JSContext *ctx, JSValueConst global);
void    ns_worker_shape_global(JSContext *ctx, gboolean service_worker);

void    ns_crypto_install_window(JSContext *ctx, JSValueConst global);
void    ns_crypto_install_window_subtle(JSContext *ctx, JSValueConst global);
void    ns_crypto_install_worker(JSContext *ctx, JSValueConst global);

/* getComputedStyle, element.style, the CSS namespace and the Web Animations
 * hooks (rust/js-cssom) and the js.c helpers they call. */
void     ns_cssom_install_window(JSContext *ctx, JSValueConst global);
void     ns_cssom_install_css(JSContext *ctx, JSValueConst global);
void     ns_cssom_install_style_proto(JSContext *ctx, JSValueConst proto);
char    *ns_cssom_style_own_value(const ns_node *node, const char *name,
                                  gboolean *writable);
void     ns_cssom_style_set(JSContext *ctx, ns_node *node, const char *name,
                            JSValueConst value);
void     ns_cssom_teardown(ns_js *js);
ns_node *ns_style_decl_node(JSValueConst this_val);
JSValue  ns_style_decl_proto(JSContext *ctx);
gboolean ns_js_element_is_rendered(JSContext *ctx, JSValueConst v);
GHashTable *ns_js_style_table(const ns_js *js);
struct ns_anim *ns_js_anim(const ns_js *js);
void     ns_js_flush_layout(ns_js *js);
void     ns_js_flush_style(ns_js *js);
void     ns_js_set_attr_recorded(ns_js *js, ns_node *n, const char *name,
                                 const char *value);

/* The event core (rust/js-events) and the js.c helpers it calls. */
JSValue ns_event_new_proto(JSContext *ctx, JSValueConst proto);
JSValue ns_event_state(JSValueConst v);
const ns_node *const *ns_js_dispatch_path(ns_js *js, guint *len, gboolean *window);
void    ns_event_mark_default_prevented(JSContext *ctx, JSValueConst event);
void    ns_event_define_source(JSContext *ctx, JSValueConst ev, JSValue source);
void    ns_install_event_attribute_getters(JSContext *ctx, JSValueConst global);
void    ns_events_install_window_base(JSContext *ctx, JSValueConst global);
void    ns_events_install_window(JSContext *ctx, JSValueConst global);
void    ns_events_install_worker(JSContext *ctx, JSValueConst global);

JSValue  ns_ce_define(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_ce_get(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_ce_when_defined(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_ce_upgrade(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_ce_get_name(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
gboolean ns_ce_name_valid(const char *name);
JSValue  ns_ce_html_element_construct(JSContext *ctx, JSValueConst new_target);
JSValue  ns_ce_class_for_node(ns_js *js, const ns_node *node);
void     ns_ce_upgrade_element(ns_js *js, ns_node *node);
void     ns_ce_upgrade_subtree_all(ns_js *js, ns_node *root);
void     ns_ce_upgrade_subtree_detached(ns_js *js, ns_node *root);
void     ns_ce_disconnect_subtree(ns_js *js, ns_node *root);
void     ns_ce_attribute_changed(ns_js *js, ns_node *node, const char *attr,
                                 const char *old_value, const char *new_value);
gboolean ns_ce_has_pending(const ns_js *js);
gboolean ns_ce_upgrading(const ns_js *js);
void     ns_ce_reset(ns_js *js);
void     ns_ce_teardown(ns_js *js);
const ns_node *ns_js_ce_main_doc(const ns_js *js);
JSValue  ns_js_node_wrapper(JSContext *ctx, const ns_node *node);
gboolean ns_js_wrapper_pinned(const ns_js *js, const ns_node *node);
void     ns_js_popover_removing(ns_js *js, ns_node *el);
JSValue  ns_js_new_orphan_element(JSContext *ctx, const char *name);

JSValue  ns_element_showPopover(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_hidePopover(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_togglePopover(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_show(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_showModal(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_close(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_requestClose(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_element_get_popover(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_set_popover(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_element_get_popoverTargetElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_set_popoverTargetElement(JSContext *ctx, JSValueConst this_val,
                                             JSValueConst val);
JSValue  ns_dialog_get_returnValue(JSContext *ctx, JSValueConst this_val);
JSValue  ns_dialog_set_returnValue(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_dialog_get_closedBy(JSContext *ctx, JSValueConst this_val);
JSValue  ns_dialog_set_closedBy(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_button_get_command(JSContext *ctx, JSValueConst this_val);
JSValue  ns_button_set_command(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_button_get_commandForElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_button_set_commandForElement(JSContext *ctx, JSValueConst this_val,
                                         JSValueConst val);
void     ns_top_layer_forget_node(ns_js *js, const ns_node *n);
void     ns_top_layer_clear(ns_js *js);
void     ns_popover_attr_changed(ns_js *js, ns_node *el, const char *attr,
                                 const char *old_value, const char *new_value);
void     ns_js_popover_light_dismiss(ns_js *js, const ns_node *target, gboolean up);
void     ns_js_dialog_light_dismiss(ns_js *js, const ns_node *target, gboolean up);
ns_node *ns_summary_toggle_target(const ns_node *el);
gboolean ns_node_is_button(const ns_node *el);
void     ns_button_activation(ns_js *js, ns_node *button, const ns_node *event_target);
void     ns_popover_target_activation(ns_js *js, ns_node *el, const ns_node *event_target);
void     ns_js_flush_autofocus(ns_js *js);
gboolean ns_js_fire_toggle_event(ns_js *js, const ns_node *target, const char *type,
                                 const char *old_state, const char *new_state,
                                 gboolean cancelable, const ns_node *source,
                                 gboolean *default_prevented);
void     ns_js_set_attr_recorded_len(ns_js *js, ns_node *n, const char *name,
                                     const char *value, gssize len);
void     ns_js_remove_attr_recorded(ns_js *js, ns_node *n, const char *name);
ns_node *ns_node_assigned_slot_node(const ns_node *n);
gboolean ns_node_tabindex(const ns_node *el, int *out);
gboolean ns_js_autofocus_processed(const ns_js *js);
void     ns_js_set_autofocus_processed(ns_js *js);
int      ns_js_ready_state(const ns_js *js);
void     ns_js_set_active_modal(ns_js *js, const ns_node *modal);

void    ns_storage_init(ns_js *js);
void    ns_storage_teardown(ns_js *js);
void    ns_storage_flush(ns_js *js);
void    ns_storage_schedule_flush(ns_js *js);
void    ns_storage_drain_deferred_events(ns_js *js);
void    ns_storage_free_deferred_events(ns_js *js);
void    ns_storage_switch_session(ns_js *js, const char *old_partition,
                                  const char *new_partition);
void    ns_storage_load_local(ns_js *js, const char *url);
void    ns_storage_install_proto(JSContext *ctx, JSValueConst proto);
void    ns_storage_install_window(JSContext *ctx, JSValueConst global);
char   *ns_storage_named_value(JSContext *ctx, JSValueConst obj, const char *name);
int     ns_storage_named_set(JSContext *ctx, JSValueConst obj, const char *name,
                             JSValueConst value);
void    ns_storage_named_delete(JSContext *ctx, JSValueConst obj, const char *name);
char  **ns_storage_names(JSContext *ctx, JSValueConst obj);
int     ns_storage_area_of(JSValueConst obj);
JSValue ns_storage_new(JSContext *ctx, int area);
gboolean ns_js_halted(const ns_js *js);
void    ns_js_in_main_realm(ns_js *js, void (*fn)(void *data), void *data);
JSValue ns_js_frame_realm_window(ns_js *js, const ns_node *frame);
gboolean ns_js_inline_handlers_allowed(const ns_js *js);
void    ns_js_fire_element_handlers(ns_js *js, const ns_node *element,
                                    const char *type, JSValueConst event);

JSValue  ns_window_btoa(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_window_atob(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_window_url_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                            JSValueConst *argv);
JSValue  ns_window_url_can_parse(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue  ns_window_url_parse_static(JSContext *ctx, JSValueConst this_val, int argc,
                                    JSValueConst *argv);
JSValue  ns_window_url_create_object(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_window_url_update_object(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_window_url_revoke_object(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_window_usp_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                            JSValueConst *argv);
JSValue  ns_window_text_encoder_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_window_text_decoder_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_window_filereader_ctor(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
void     ns_url_install_interface(JSContext *ctx);
void     ns_usp_install_interface(JSContext *ctx);
void     ns_net_install_text_codecs(JSContext *ctx, JSValueConst global);
void     ns_filereader_run(JSContext *ctx, JSValueConst self, gint64 gen);
void     ns_url_teardown(ns_js *js);
void     ns_js_filereader_schedule(JSContext *ctx, JSValueConst self, gint64 gen);
void     ns_js_blob_urls_put(ns_js *js, const char *url, const guint8 *bytes, gsize len,
                             const char *type);
void     ns_js_blob_urls_remove(ns_js *js, const char *url);
gboolean ns_js_bytes_view(JSContext *ctx, JSValueConst value, const uint8_t **out_data,
                          size_t *out_len, JSValue *out_holder);
void     ns_js_orphan_node(ns_js *js, ns_node *n);
void     ns_js_record_character_data(ns_js *js, ns_node *target, const char *old_value);
void     ns_js_record_child_change(ns_js *js, ns_node *parent,
                                   ns_node *added, ns_node *removed,
                                   ns_node *previous_sibling, ns_node *next_sibling);
void     ns_insert_sibling_before(ns_node *ref, ns_node *newc);
JSValue  ns_element_get_innerText(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_set_innerText(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_element_set_outerText(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_element_get_wholeText(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_normalize(JSContext *ctx, JSValueConst this_val, int argc,
                              JSValueConst *argv);
JSValue  ns_element_substring_data(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_element_append_data(JSContext *ctx, JSValueConst this_val, int argc,
                                JSValueConst *argv);
JSValue  ns_element_delete_data(JSContext *ctx, JSValueConst this_val, int argc,
                                JSValueConst *argv);
JSValue  ns_element_insert_data(JSContext *ctx, JSValueConst this_val, int argc,
                                JSValueConst *argv);
JSValue  ns_element_replace_data(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue  ns_element_split_text(JSContext *ctx, JSValueConst this_val, int argc,
                               JSValueConst *argv);

const char    *ns_js_cookie_value(const ns_js *js);
void           ns_js_set_cookie_value(ns_js *js, const char *value);
const char    *ns_js_partition_key(const ns_js *js);
const char    *ns_js_referrer(const ns_js *js);
const char    *ns_js_frame_referrer_for(const ns_js *js, const ns_node *frame);
int            ns_js_doc_ready_state(const ns_js *js, const ns_node *doc);
const ns_node *ns_js_current_script(const ns_js *js);
const ns_node *ns_js_focused_doc(const ns_js *js);
void           ns_js_clear_focused_node(ns_js *js);
void           ns_js_unorphan_node(ns_js *js, ns_node *n);
void     ns_element_replace_all_recorded(ns_js *js, ns_node *n, ns_node *added);
gboolean ns_document_is_realm_document(JSContext *ctx, JSValueConst doc);
void     ns_js_seed_cookies_from_jar(ns_js *js);
JSValue  ns_document_get_documentElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_body(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_set_body(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_document_get_head(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_scrollingElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_activeElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_currentScript(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_scripts(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_anchors(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_embeds(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_plugins(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_applets(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_title(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_set_title(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_document_get_dir(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_set_dir(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_document_get_color(JSContext *ctx, JSValueConst this_val, int magic);
JSValue  ns_document_set_color(JSContext *ctx, JSValueConst this_val, JSValueConst val,
                               int magic);
JSValue  ns_document_get_cookie(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_set_cookie(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_document_get_referrer(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_readyState(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_designMode(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_lastModified(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_xmlVersion(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_hidden(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_visibilityState(JSContext *ctx, JSValueConst this_val);
JSValue  ns_document_get_compatMode(JSContext *ctx, JSValueConst this_val);

JSValue  ns_window_get_selection(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue  ns_document_create_range(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue  ns_native_range(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_document_create_tree_walker(JSContext *ctx, JSValueConst this_val, int argc,
                                        JSValueConst *argv);
JSValue  ns_document_create_node_iterator(JSContext *ctx, JSValueConst this_val, int argc,
                                          JSValueConst *argv);
void     ns_tree_walker_install_proto(JSContext *ctx, JSValueConst proto);
void     ns_node_iters_pre_remove(ns_js *js, ns_node *removed);
void     ns_traversal_teardown(ns_js *js);
gboolean ns_js_selection_state(const ns_js *js, const char **text, double rect[4]);
void     ns_js_track_orphan(ns_js *js, ns_node *node);

void     ns_js_set_attr_ns_recorded(ns_js *js, ns_node *n, const char *namespace_uri,
                                    const char *prefix, const char *local_name,
                                    const char *name, const char *value);
void     ns_js_remove_attr_ns_recorded(ns_js *js, ns_node *n, const char *namespace_uri,
                                       const char *local_name);
gboolean ns_valid_element_local_name(const char *s);
ns_node *ns_token_list_node(JSValueConst this_val, const char **out_attr);
ns_node *ns_dataset_node(JSValueConst obj);
ns_node *ns_namedmap_owner(JSValueConst this_val);
void    *ns_attr_opaque(JSValueConst value);
JSValue  ns_attr_new_object(JSContext *ctx, void *state);
void     ns_attr_apply_proto(JSContext *ctx, JSValueConst obj);
JSValue  ns_tlist_contains(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_add(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_remove(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_toggle(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_replace(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_item(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_supports(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_toString(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_tlist_get_length(JSContext *ctx, JSValueConst this_val);
JSValue  ns_tlist_get_value(JSContext *ctx, JSValueConst this_val);
JSValue  ns_tlist_set_value(JSContext *ctx, JSValueConst this_val, JSValueConst val);
char    *ns_tlist_named_token(JSValueConst obj, const char *name);
char    *ns_dataset_named_value(JSValueConst obj, const char *name);
char   **ns_dataset_names(JSValueConst obj);
int      ns_dataset_named_set(JSContext *ctx, JSValueConst obj, const char *name,
                              JSValueConst val);
void     ns_dataset_named_delete(JSContext *ctx, JSValueConst obj, const char *name);
gboolean ns_element_has_dataset(const ns_node *el);
JSValue  ns_namedmap_getNamedItem(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue  ns_namedmap_getNamedItemNS(JSContext *ctx, JSValueConst this_val, int argc,
                                    JSValueConst *argv);
JSValue  ns_namedmap_setNamedItem(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue  ns_namedmap_removeNamedItem(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_namedmap_removeNamedItemNS(JSContext *ctx, JSValueConst this_val, int argc,
                                       JSValueConst *argv);
JSValue  ns_namedmap_item(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
JSValue  ns_namedmap_get_length(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_getAttributeNode(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_element_getAttributeNodeNS(JSContext *ctx, JSValueConst this_val, int argc,
                                       JSValueConst *argv);
JSValue  ns_element_setAttributeNode(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_element_removeAttributeNode(JSContext *ctx, JSValueConst this_val, int argc,
                                        JSValueConst *argv);
JSValue  ns_attr_to_js(JSContext *ctx, JSValueConst owner, const ns_attr *a,
                       gboolean include_base);
ns_node *ns_attr_owner(JSValueConst value);
void     ns_attr_state_release(void *state);
void     ns_attr_detach_matching(ns_js *js, ns_node *owner, const char *namespace_uri,
                                 const char *local_name);
void     ns_attr_detach_owner(ns_js *js, ns_node *owner);
void     ns_attr_detach_all(ns_js *js);

double   ns_window_scroll_prop(JSContext *ctx, const char *prop);
void     ns_js_scroll_viewport(ns_js *js, double x, double y);
void     ns_js_queue_scrollend(ns_js *js, const ns_node *el);
void     ns_js_notify_scroll_to(ns_js *js, const ns_node *target);
JSValue  ns_element_int_attr_getter(JSContext *ctx, JSValueConst this_val, int magic);
JSValue  ns_make_svg_animated_length(JSContext *ctx, const ns_node *n, const char *attr);
JSValue  ns_element_img_natural_width(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_img_natural_height(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_dimension_getter(JSContext *ctx, JSValueConst this_val, int magic);
JSValue  ns_svg_beginElement(JSContext *ctx, JSValueConst this_val, int argc,
                             JSValueConst *argv);
JSValue  ns_svg_setCurrentTime(JSContext *ctx, JSValueConst this_val, int argc,
                               JSValueConst *argv);
JSValue  ns_element_getBoundingClientRect(JSContext *ctx, JSValueConst this_val, int argc,
                                          JSValueConst *argv);
JSValue  ns_element_getClientRects(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_element_getBBox(JSContext *ctx, JSValueConst this_val, int argc,
                            JSValueConst *argv);
JSValue  ns_element_getCTM(JSContext *ctx, JSValueConst this_val, int argc,
                           JSValueConst *argv);
JSValue  ns_element_getScreenCTM(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue  ns_element_getTotalLength(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_element_getPointAtLength(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_element_createSVGPoint(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_element_createSVGRect(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue  ns_element_createSVGMatrix(JSContext *ctx, JSValueConst this_val, int argc,
                                    JSValueConst *argv);
JSValue  ns_element_createSVGTransform(JSContext *ctx, JSValueConst this_val, int argc,
                                       JSValueConst *argv);
JSValue  ns_element_get_ownerSVGElement(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_offsetWidth(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_offsetHeight(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_offsetTop(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_offsetLeft(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_offsetParent(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_clientWidth(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_clientHeight(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_clientTop(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_clientLeft(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_scrollTop(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_scrollLeft(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_scrollWidth(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_scrollHeight(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_set_scrollTop(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_element_set_scrollLeft(JSContext *ctx, JSValueConst this_val, JSValueConst val);
JSValue  ns_element_scroll_to(JSContext *ctx, JSValueConst this_val, int argc,
                              JSValueConst *argv);
JSValue  ns_element_scroll_by(JSContext *ctx, JSValueConst this_val, int argc,
                              JSValueConst *argv);
JSValue  ns_element_scrollIntoView(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_window_scroll_to(JSContext *ctx, JSValueConst this_val, int argc,
                             JSValueConst *argv);
JSValue  ns_window_scroll_by(JSContext *ctx, JSValueConst this_val, int argc,
                             JSValueConst *argv);
JSValue  ns_window_scroll_by_lines(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);
JSValue  ns_window_scroll_by_pages(JSContext *ctx, JSValueConst this_val, int argc,
                                   JSValueConst *argv);

gboolean ns_node_is_shadow_root(const ns_node *n);
ns_node *ns_element_find_shadow_child(const ns_node *host);
JSValue  ns_element_attachShadow(JSContext *ctx, JSValueConst this_val, int argc,
                                 JSValueConst *argv);
JSValue  ns_element_get_shadowRoot(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_get_assignedSlot(JSContext *ctx, JSValueConst this_val);
JSValue  ns_element_assignedNodes(JSContext *ctx, JSValueConst this_val, int argc,
                                  JSValueConst *argv);
JSValue  ns_element_assignedElements(JSContext *ctx, JSValueConst this_val, int argc,
                                     JSValueConst *argv);
JSValue  ns_element_getRootNode(JSContext *ctx, JSValueConst this_val, int argc,
                                JSValueConst *argv);
JSValue  ns_document_element_from_point(JSContext *ctx, JSValueConst this_val, int argc,
                                        JSValueConst *argv);
JSValue  ns_document_elements_from_point(JSContext *ctx, JSValueConst this_val, int argc,
                                         JSValueConst *argv);

#endif
