/* Southstar — JavaScript engine binding (QuickJS).
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "js.h"
#include "js_classid.h"

#include "polyfills.h"
#include "streaming.h"
#include "version.h"

#include <math.h>
#include <string.h>
#include <time.h>

#include <cairo.h>
#include <gio/gio.h>
#include <glib/gstdio.h>
#include "ns_pango.h"
#include "ns_quickjs.h"

#ifdef G_OS_WIN32
#include <windows.h>
#endif
#ifdef __APPLE__
#include <pthread.h>
#endif

#include "anim.h"
#include "config.h"
#include "css.h"
#include "datetime.h"
#include "debuglog.h"
#include "engine.h"
#include "html.h"
#include "idb.h"
#include "image.h"
#include "js_date.h"
#include "js_intl.h"
#include "js_brand.h"
#include "js_realm.h"
#include "layout.h"
#include "net.h"
#include "paint.h"
#include "eventsource.h"
#include "security.h"
#include "svg.h"
#include "video.h"
#include "video_decode.h"
#include "wasm.h"
#include "webgl.h"
#ifdef ND_HAVE_WEBGPU
#include "webgpu.h"
#endif
#include "ws.h"

#include "js_internal.h"
#include "font.h"

static GPrivate g_active_js_key = G_PRIVATE_INIT(NULL);

ns_js *
ns_active_js(void)
{
    return g_private_get(&g_active_js_key);
}

void
ns_set_active_js(ns_js *js)
{
    g_private_set(&g_active_js_key, js);
}

#define NS_SANDBOX_ACTIVE              (1u << 0)
#define NS_SANDBOX_ALLOW_SCRIPTS       (1u << 1)
#define NS_SANDBOX_ALLOW_FORMS         (1u << 2)
#define NS_SANDBOX_ALLOW_SAME_ORIGIN   (1u << 3)
#define NS_SANDBOX_ALLOW_POPUPS        (1u << 4)
#define NS_SANDBOX_ALLOW_MODALS        (1u << 5)
#define NS_SANDBOX_ALLOW_TOP_NAV       (1u << 6)
#define NS_SANDBOX_ALLOW_TOP_NAV_UA    (1u << 7)
#define NS_SANDBOX_ALLOW_DOWNLOADS     (1u << 8)
#define NS_SANDBOX_ALLOW_POPUPS_ESCAPE (1u << 9)
#define NS_SANDBOX_ALLOW_POINTER_LOCK  (1u << 10)
#define NS_SANDBOX_ALLOW_PRESENTATION  (1u << 11)
#define NS_SANDBOX_ALLOW_ORIENTATION   (1u << 12)
#define NS_SANDBOX_ALLOW_STORAGE_ACCESS (1u << 14)
/* Internal flag, never produced by the sandbox-attribute parser: set when the
   frame is cross-origin to its embedder, to deny it the embedding origin's
   localStorage/sessionStorage/cookies (the runtime's storage is keyed to the
   top origin). Must match the (sandbox & 8192) checks in the realm bootstraps. */
#define NS_FRAME_CROSS_ORIGIN          (1u << 13)

uint32_t
ns_js_array_length(JSContext *ctx, JSValueConst arr)
{
    uint32_t len = 0;
    JSValue lv = JS_GetPropertyStr(ctx, arr, "length");
    JS_ToUint32(ctx, &len, lv);
    JS_FreeValue(ctx, lv);
    return len;
}

gboolean
ns_js_get_bool_prop(JSContext *ctx, JSValueConst obj, const char *key,
                    gboolean *was_set)
{
    JSValue v = JS_GetPropertyStr(ctx, obj, key);
    gboolean defined = !JS_IsUndefined(v);
    gboolean truthy = defined && JS_ToBool(ctx, v) > 0;
    JS_FreeValue(ctx, v);
    if (was_set) *was_set = defined;
    return truthy;
}

static gboolean
ns_node_name_is_any_of(const ns_node *n, const char *const *tags)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    for (; *tags; tags++)
        if (g_ascii_strcasecmp(n->name, *tags) == 0) return TRUE;
    return FALSE;
}

static ns_node *
ns_unwrap_element_mut(JSValueConst val)
{
    return (ns_node *)ns_unwrap_element(val);
}

static void
ns_partition_apply(ns_js *js, const char *new_url)
{
    if (!js) return;
    char *origin = ns_url_origin_from(new_url);
    char *new_key;
    if (origin && *origin) {
        new_key = origin;
    } else {
        g_free(origin);
        new_key = g_strdup_printf("opaque://%" G_GUINT64_FORMAT,
                                  ++js->opaque_counter);
    }

    if (js->partition_key && strcmp(js->partition_key, new_key) == 0) {
        g_free(new_key);
        return;
    }

    if (js->partition_key && js->cookie_buckets) {
        g_hash_table_replace(js->cookie_buckets,
                             g_strdup(js->partition_key),
                             g_strdup(js->cookie_value ? js->cookie_value : ""));
    }
    ns_storage_switch_session(js, js->partition_key, new_key);

    g_free(js->cookie_value);
    js->cookie_value = NULL;
    if (js->cookie_buckets) {
        const char *cv = g_hash_table_lookup(js->cookie_buckets, new_key);
        if (cv) js->cookie_value = g_strdup(cv);
    }

    g_free(js->partition_key);
    js->partition_key = new_key;
}

void
ns_storage_load_for(ns_js *js, const char *new_url)
{
    if (!js) return;
    ns_partition_apply(js, new_url);
    ns_storage_load_local(js, new_url);
}

static int
ns_cmp_tag_name(const void *a, const void *b)
{
    return strcmp(*(const char *const *)a, *(const char *const *)b);
}

static gboolean
ns_html_tag_has_plain_interface(const char *lower_name)
{
    static const char *const plain[] = {
        "abbr", "acronym", "address", "article", "aside", "b", "basefont",
        "bdi", "bdo", "big", "center", "cite", "code", "dd", "dfn", "dt",
        "em", "figcaption", "figure", "footer", "header", "hgroup", "i",
        "kbd", "main", "mark", "nav", "nobr", "noembed", "noframes",
        "noscript", "plaintext", "rb", "rp", "rt", "rtc", "ruby", "s",
        "samp", "search", "section", "small", "strike", "strong", "sub",
        "summary", "sup", "tt", "u", "var", "wbr",
    };
    return bsearch(&lower_name, plain, G_N_ELEMENTS(plain), sizeof plain[0],
                   ns_cmp_tag_name) != NULL;
}

/* The prototype of an element of a namespace other than HTML and SVG:
 * MathMLElement's for MathML, Element's for the rest. */
static JSValue
ns_foreign_kind_proto(const ns_js *js, const ns_node *node)
{
    const char *ns = ns_element_get_attr(node, "data-nd-ns-uri");
    gboolean mathml = ns && strcmp(ns, "http://www.w3.org/1998/Math/MathML") == 0;
    return mathml && JS_IsObject(js->proto_mathmlelement)
        ? js->proto_mathmlelement : js->proto_element;
}

static JSValue
ns_html_kind_proto(const ns_js *js, const ns_node *node)
{
    if (!node->name || !js->per_tag_protos) return js->proto_htmlelement;
    gsize n = strlen(node->name);
    gboolean lower_case = TRUE;
    for (gsize i = 0; i < n; i++)
        if (g_ascii_isupper(node->name[i])) lower_case = FALSE;
    if (lower_case) {
        JSValue *slot = g_hash_table_lookup(js->per_tag_protos, node->name);
        if (slot) return *slot;
        if (ns_html_tag_has_plain_interface(node->name))
            return js->proto_htmlelement;
    }
    if (!ns_ce_name_valid(node->name) &&
        JS_IsObject(js->proto_htmlunknownelement))
        return js->proto_htmlunknownelement;
    return js->proto_htmlelement;
}

JSValue
ns_js_kind_proto(const ns_js *js, const ns_node *node)
{
    if (!js || !js->dom_protos_set || !node) return JS_UNDEFINED;
    switch (node->kind) {
    case NS_NODE_ELEMENT:
        if (node->flags & NS_NODE_SVG_NS) {
            if (node->name && g_ascii_strcasecmp(node->name, "a") == 0 &&
                JS_IsObject(js->proto_svgaelement))
                return js->proto_svgaelement;
            return JS_IsObject(js->proto_svgelement)
                ? js->proto_svgelement : js->proto_element;
        }
        if (node->flags & NS_NODE_FOREIGN_NS)
            return ns_foreign_kind_proto(js, node);
        return ns_html_kind_proto(js, node);
    case NS_NODE_TEXT:
        return (node->flags & NS_NODE_CDATA) ? js->proto_cdata : js->proto_text;
    case NS_NODE_COMMENT:
        return (node->flags & NS_NODE_PI) ? js->proto_pi : js->proto_comment;
    case NS_NODE_DOCTYPE:
        return js->proto_doctype;
    case NS_NODE_DOCUMENT:
        return (node->flags & NS_NODE_FRAGMENT) ? js->proto_docfrag
                                                : js->proto_document;
    default:
        return JS_UNDEFINED;
    }
}

gboolean
ns_js_img_src_layout_neutral(const ns_node *n)
{
    if (!n || !n->name || strcmp(n->name, "img") != 0) return FALSE;
    const char *w = ns_element_get_attr(n, "width");
    const char *h = ns_element_get_attr(n, "height");
    return w && *w && h && *h;
}

void
ns_js_set_src_recorded(ns_js *js, ns_node *n, const char *s, gsize slen)
{
    gsize old_len = 0;
    const char *old = ns_element_get_attr_len(n, "src", &old_len);
    gboolean changed = !old || old_len != slen || memcmp(old, s, slen) != 0;
    if (changed && ns_js_img_src_layout_neutral(n))
        ns_js_set_attr_recorded_paint_only(js, n, "src", s);
    else
        ns_js_set_attr_recorded_len(js, n, "src", s, (gssize)slen);
    if (changed && n->name && strcmp(n->name, "img") == 0) {
        n->flags &= ~NS_NODE_IMG_LOAD_FIRED;
        ns_js_start_image_load(js, n, s);
    }
    if (n->name && strcmp(n->name, "iframe") == 0)
        ns_js_schedule_iframe_load_full(js, n, TRUE);
}

gboolean
ns_js_image_natural_size(ns_js *js, const ns_node *n, int *width, int *height)
{
    const ns_image *im = ns_js_image_for_node(js, n);
    if (!im) return FALSE;
    *width = im->natural_width;
    *height = im->natural_height;
    return TRUE;
}

gboolean
ns_node_in_template_content(const ns_node *n)
{
    for (const ns_node *p = n; p; p = p->parent)
        if ((p->flags & NS_NODE_TEMPLATE_CONTENT) ||
            ns_node_is_element_named(p, "template"))
            return TRUE;
    return FALSE;
}

#define NS_SCRIPT_ALREADY_STARTED "data-nd-script-already-started"
#define NS_SCRIPT_EMPTY_SOURCE "data-nd-script-empty-source"

void
ns_bind_event_target_listeners(JSContext *ctx, JSValueConst obj)
{
    ns_bind_fn(ctx, obj, "addEventListener",    ns_target_addEventListener,    2);
    ns_bind_fn(ctx, obj, "removeEventListener", ns_target_removeEventListener, 2);
}

guint
ns_media_native_formats(void)
{
    guint formats = 0;
#ifdef NS_HAVE_LIBAV
    formats |= 1;
#endif
#ifdef NS_AUDIO_NATIVE_VORBIS
    formats |= 2;
#endif
#ifdef NS_AUDIO_NATIVE_OPUS
    formats |= 4;
#endif
    return formats;
}

gboolean
ns_js_url_parses(ns_js *js, const char *url)
{
    if (!url) return FALSE;
    g_autofree char *base = ns_js_doc_base_url(js);
    g_autofree char *resolved = (base && *base)
        ? ns_url_resolve(base, url)
        : ns_url_resolve(NULL, url);
    return resolved != NULL;
}

/* A frame window's listener operations join the page's registry (see
 * et_src), and its tag is its Window.prototype's. */
/* A frame window's prototype chain, copied from the page's, runs through the
 * frame's own Window and EventTarget prototypes. */
static const char ns_frame_window_chain_src[] =
    "(function(G){"
    "  var gp = Object.getPrototypeOf, sp = Object.setPrototypeOf;"
    "  var W = G.Window && G.Window.prototype, ET = G.EventTarget && G.EventTarget.prototype;"
    "  if (!W || !ET) return;"
    "  var named = gp(W);"
    "  if (named && named !== ET && named !== Object.prototype && gp(named) !== ET)"
    "    try { sp(named, ET); } catch (e) {}"
    "  if (gp(G) !== W) try { sp(G, W); } catch (e) {}"
    "})(globalThis)";

static void
ns_js_adopt_frame_window_events(ns_js *js, JSContext *fctx, JSValueConst fg)
{
    JSValue chain = JS_Eval(fctx, ns_frame_window_chain_src,
                            sizeof(ns_frame_window_chain_src) - 1,
                            "<frame-window-chain>",
                            JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(chain)) JS_FreeValue(fctx, JS_GetException(fctx));
    JS_FreeValue(fctx, chain);
    JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    JSValue main_global = JS_GetGlobalObject(main_ctx);
    JSValue adopt = JS_GetPropertyStr(main_ctx, main_global,
                                      "__ndAdoptWindowEventOps");
    if (JS_IsFunction(main_ctx, adopt)) {
        JSValue r = JS_Call(main_ctx, adopt, JS_UNDEFINED, 1, &fg);
        if (JS_IsException(r)) JS_FreeValue(main_ctx, JS_GetException(main_ctx));
        JS_FreeValue(main_ctx, r);
    }
    JS_FreeValue(main_ctx, adopt);
    JS_FreeValue(main_ctx, main_global);
    JSValue sym_ctor = JS_GetPropertyStr(fctx, fg, "Symbol");
    JSValue tag_sym = JS_GetPropertyStr(fctx, sym_ctor, "toStringTag");
    JSAtom tag_atom = JS_ValueToAtom(fctx, tag_sym);
    if (tag_atom != JS_ATOM_NULL) {
        JS_DeleteProperty(fctx, fg, tag_atom, 0);
        JS_FreeAtom(fctx, tag_atom);
    }
    JS_FreeValue(fctx, tag_sym);
    JS_FreeValue(fctx, sym_ctor);
}

gboolean
ns_js_csp_allows_worker(ns_js *js, const char *url)
{
    return !js->csp || ns_csp_allows(js->csp, NS_CSP_WORKER, url, js->current_url);
}

JSValue
ns_worker_compile(JSContext *ctx, const char *src, gsize len, const char *url,
                  gboolean module, gboolean *parse_error)
{
    *parse_error = FALSE;
    if (!module) {
        JSValue fn = JS_Eval(ctx, src, len, url,
                             JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_COMPILE_ONLY);
        if (JS_IsException(fn)) *parse_error = TRUE;
        return fn;
    }
    JSValue fn = ns_js_compile_module_cached(ctx, src, len, url);
    if (JS_IsException(fn)) {
        *parse_error = TRUE;
        return fn;
    }
    if (ns_js_module_set_import_meta(ctx, fn, TRUE) < 0) {
        JS_FreeValue(ctx, fn);
        return JS_EXCEPTION;
    }
    return fn;
}

const char *
ns_js_engine_version(void)
{
    static char *version;
    if (!version)
        version = g_strdup_printf("QuickJS %s", JS_GetVersion());
    return version;
}

void
ns_js_purge_subtree_rafs(ns_js *js, ns_node *root)
{
    if (!js) return;
    for (ns_node *n = root; n; n = ns_node_next_in_subtree(n, root, TRUE)) {
        if (!ns_node_is_element_named(n, "iframe")) continue;
        ns_js_purge_frame_rafs(js, n);
        ns_services_purge_frame_timers(js, n);
        ns_js_scrub_iframe_globals(js, n);
    }
}

/* The resource timing entry of an image, from the times the image cache
 * started and finished fetching it.  An image served from memory without
 * a fetch for this page takes its load time instead. */
void
ns_js_record_image_timing(ns_js *js, const ns_node *node, const char *url,
                          const ns_image *img)
{
    if (!js || !url || !img) return;
    ns_perf_resource_info info = { 0 };
    ns_js_element_perf_info(js, node, &info);
    /* A second image with the same URL comes from the memory cache and,
     * as in other browsers, gets no entry of its own. */
    if (ns_perf_has_resource(js, info.timeline, url, "img")) return;
    info.next_hop_protocol = img->next_hop_protocol;
    info.timing_allow_origin = img->timing_allow_origin;
    info.status = img->http_status;
    info.body_size = img->body_size;
    gint64 now = g_get_monotonic_time();
    gint64 start = img->request_us >= js->time_origin_us ? img->request_us : now;
    gint64 end = img->response_us >= start ? img->response_us : now;
    ns_perf_add_resource_timed(js, &info, url, "img", start, end, NULL);
}

gboolean
ns_js_image_settled(const ns_image *img, gboolean *failed)
{
    *failed = img->failed ? TRUE : FALSE;
    return img->loaded || img->failed;
}

gboolean
ns_js_video_take_load_events(ns_video *video)
{
    if (!video->loaded || video->load_events_fired) return FALSE;
    video->load_events_fired = TRUE;
    return TRUE;
}

void
ns_element_insert_before_single(ns_js *_j, ns_node *parent, ns_node *newc, ns_node *ref)
{
    if (newc == ref) return;
    if (_j && newc->parent) ns_node_iters_pre_remove(_j, newc);
    if (newc->parent) ns_node_remove(newc);
    if (_j) ns_js_unorphan_node(_j, newc);
    newc->parent = parent;
    newc->next_sibling = ref;
    newc->prev_sibling = ref->prev_sibling;
    if (ref->prev_sibling) ref->prev_sibling->next_sibling = newc;
    else parent->first_child = newc;
    ref->prev_sibling = newc;
}

ns_node *
ns_namedmap_owner(JSValueConst this_val)
{
    return ns_live_owner_node(this_val);
}

void
ns_attr_apply_proto(JSContext *ctx, JSValueConst obj)
{
    JSValue gobj = JS_GetGlobalObject(ctx);
    JSValue attr_ctor = JS_GetPropertyStr(ctx, gobj, "Attr");
    JSValue attr_proto = JS_GetPropertyStr(ctx, attr_ctor, "prototype");
    if (JS_IsObject(attr_proto)) JS_SetPrototype(ctx, obj, attr_proto);
    JS_FreeValue(ctx, attr_proto);
    JS_FreeValue(ctx, attr_ctor);
    JS_FreeValue(ctx, gobj);
    ns_bind_fn(ctx, obj, "cloneNode", ns_attr_cloneNode, 0);
    ns_bind_fn(ctx, obj, "isSameNode", ns_element_isSameNode, 1);
}

void
ns_js_set_layout_root(ns_js *js, const struct ns_box *root)
{
    if (!js) return;
    if (js->layout_root != root) ns_box_lookup_forget(js);
    js->layout_root = root;
    ns_js_sync_window_metrics(js);
    if (root) {
        ns_js_promote_deferred_iframes(js);
        ns_intersection_observers_tick(js);
        ns_resize_observers_tick(js);
    }
}

JSValue
ns_live_build_attributes(JSContext *ctx, JSValueConst owner)
{
    JSValue arr = JS_NewArray(ctx);
    const ns_node *root = ns_unwrap_element(owner);
    if (!root) return arr;
    GArray *attrs = g_array_new(FALSE, FALSE, sizeof(JSValue));
    for (const ns_attr *a = root->attrs; a; a = a->next)
        if (!ns_attr_name_is_internal(a->name)) {
            JSValue attr = ns_attr_to_js(ctx, owner, a, FALSE);
            g_array_append_val(attrs, attr);
        }
    for (guint k = 0; k < attrs->len; k++)
        JS_SetPropertyUint32(ctx, arr, k, g_array_index(attrs, JSValue, k));
    g_array_free(attrs, TRUE);
    return arr;
}

gboolean
ns_node_is_disabled_form_control(const ns_node *el)
{
    static const char *const controls[] = {
        "button", "fieldset", "input", "optgroup", "option", "select",
        "textarea", NULL,
    };
    return ns_node_name_is_any_of(el, controls) &&
           ns_element_effectively_disabled(el);
}

void
ns_js_form_reset(ns_js *js, ns_node *form)
{
    if (!js || !js->ctx || !form) return;
    ns_js_reset_form(js->ctx, form);
}

JSContext *
ns_js_image_load_realm(ns_js *js)
{
    JSContext *realm = js->raf_frame_ctx
        ? ns_js_frame_context(js, js->raf_frame_ctx) : NULL;
    return realm ? realm : js->ctx;
}

const char *
ns_js_node_doc_base(ns_js *js, const ns_node *el)
{
    for (const ns_node *p = el ? el->parent : NULL; p; p = p->parent) {
        if (ns_node_is_element_named(p, "iframe") ||
            ns_node_is_element_named(p, "frame") ||
            ns_node_is_element_named(p, "object")) {
            const char *fu = ns_element_get_attr(p, "data-nd-frame-url");
            if (fu && *fu) return fu;
        }
    }
    return js ? js->current_url : NULL;
}

double
ns_arg_d(JSContext *ctx, JSValueConst v)
{
    double d = 0; JS_ToFloat64(ctx, &d, v); return d;
}

GHashTable *
ns_js_platform_globals(ns_js *js)
{
    if (!js->platform_globals) {
        JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx
                                                 : js->ctx;
        JSContext *saved = js->ctx;
        js->ctx = main_ctx;
        js->platform_globals = ns_js_snapshot_globals(js);
        js->ctx = saved;
    }
    return js->platform_globals;
}

JSContext *
ns_js_new_frame_context(ns_js *js)
{
    JSContext *fctx = JS_NewContext(js->rt);
    if (!fctx) return NULL;
    JS_SetContextOpaque(fctx, js);
    JSValue fglobal = JS_GetGlobalObject(fctx);
    ns_hide_shared_array_buffer(fctx, fglobal);
    JS_FreeValue(fctx, fglobal);
    g_ptr_array_add(js->frame_ctxs, fctx);
    return fctx;
}

void
ns_js_frame_realm_finish(ns_js *js, JSContext *fctx, JSValueConst fg,
                         JSValueConst parent_global)
{
    ns_install_pdf_plugins(fctx);
    ns_js_adopt_frame_window_events(js, fctx, fg);
    ns_js_link_interface_ctors(fctx);
    ns_js_lock_global_prototypes(fctx);
    ns_js_brand_node_interfaces(fctx, ns_element_class_id(), ns_attr_class_id());
    JSValue parent_performance =
        JS_GetPropertyStr(fctx, parent_global, "performance");
    gboolean include_memory = JS_IsObject(parent_performance) &&
        ns_obj_has_defined_own_prop(fctx, parent_performance, "memory");
    JS_FreeValue(fctx, parent_performance);
    JS_SetPropertyStr(fctx, fg, "performance",
                      ns_make_performance_object(fctx, js, include_memory));
}

ns_node *
ns_document_root_for(JSContext *ctx, JSValueConst this_val)
{
    ns_node *root = ns_unwrap_element_mut(this_val);
    if (root && root->kind == NS_NODE_DOCUMENT) return root;
    ns_js *js = js_from_ctx(ctx);
    return js ? js->current_doc : NULL;
}

const ns_node *
ns_js_ce_main_doc(const ns_js *js)
{
    return js ? js->ce_main_doc : NULL;
}

JSValue
ns_js_node_wrapper(JSContext *ctx, const ns_node *node)
{
    if (!ctx || !node || !node->js_wrapper) return JS_UNDEFINED;
    return JS_DupValue(ctx, JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper));
}

gboolean
ns_js_node_unpinned(const ns_js *js, const ns_node *node)
{
    return js && js->pinned_wrappers_set &&
           !g_hash_table_contains(js->pinned_wrappers_set, node);
}

gboolean
ns_js_wrapper_pinned(const ns_js *js, const ns_node *node)
{
    return js && node && node->js_wrapper && js->pinned_wrappers_set &&
           g_hash_table_contains(js->pinned_wrappers_set, node);
}

JSValue
ns_js_new_orphan_element(JSContext *ctx, const char *name)
{
    ns_js *js = js_from_ctx(ctx);
    ns_node *node = ns_node_new_element(g_strdup(name));
    if (!node) return JS_UNDEFINED;
    ns_node_arm_js_invalidate(node);
    ns_js_track_orphan(js, node);
    JSValue elem = ns_make_element(ctx, node);
    ns_tag_caller_document(ctx, elem);
    return elem;
}

void
ns_ce_attr_changed(ns_js *js, ns_node *node, const char *attr,
                   const char *old_value, const char *new_value)
{
    if (js && node && attr && js->ctx && !js->halted) {
        if (node == ns_js_focused_node(js) &&
            g_ascii_strcasecmp(attr, "type") == 0)
            ns_js_update_focus_visible(js);
        ns_popover_attr_changed(js, node, attr, old_value, new_value);
    }
    if (js && attr && !old_value && new_value &&
        g_ascii_strcasecmp(attr, "src") == 0 &&
        ns_node_is_element_named(node, "script") &&
        !(node->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS))) {
        ns_js_script_needs_prepare(js, node);
        return;
    }
    if (!js || !node || !attr || !js->ctx || js->halted) return;
    ns_ce_attribute_changed(js, node, attr, old_value, new_value);
}

JSValue
ns_illegal_constructor(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return JS_ThrowTypeError(ctx, "Illegal constructor");
}

static void
ns_js_ensure_per_tag_protos(ns_js *js)
{
    if (!js->per_tag_protos)
        js->per_tag_protos = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, g_free);
}

void
ns_js_register_tag_proto(JSContext *ctx, const char *tag, JSValueConst proto)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js || !tag) return;
    ns_js_ensure_per_tag_protos(js);
    JSValue *old = g_hash_table_lookup(js->per_tag_protos, tag);
    if (old) JS_FreeValue(ctx, *old);
    JSValue *entry = g_new(JSValue, 1);
    *entry = JS_DupValue(ctx, proto);
    g_hash_table_insert(js->per_tag_protos, g_strdup(tag), entry);
}

void
ns_js_adopt_dom_protos(JSContext *ctx, const ns_js_dom_protos *protos)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js) return;
    js->proto_node               = protos->node;
    js->proto_element            = protos->element;
    js->proto_htmlelement        = protos->htmlelement;
    js->proto_htmlunknownelement = protos->htmlunknownelement;
    js->proto_svgelement         = protos->svgelement;
    js->proto_svgaelement        = protos->svgaelement;
    js->proto_mathmlelement      = protos->mathmlelement;
    js->proto_chardata           = protos->chardata;
    js->proto_text               = protos->text;
    js->proto_comment            = protos->comment;
    js->proto_cdata              = protos->cdata;
    js->proto_pi                 = protos->pi;
    js->proto_doctype            = protos->doctype;
    js->proto_docfrag            = protos->docfrag;
    js->proto_document           = protos->document;
    ns_js_ensure_per_tag_protos(js);
    js->dom_protos_set = 1;

    if (js->pinned_wrappers_set) {
        GHashTableIter it;
        gpointer k;
        g_hash_table_iter_init(&it, js->pinned_wrappers_set);
        while (g_hash_table_iter_next(&it, &k, NULL)) {
            ns_node *n = k;
            if (!n->js_wrapper) continue;
            JSValue kind_proto = ns_js_kind_proto(js, n);
            if (JS_IsObject(kind_proto))
                JS_SetPrototype(ctx, JS_MKPTR(JS_TAG_OBJECT, n->js_wrapper),
                                kind_proto);
        }
    }
}

ns_js *
ns_js_alloc(void)
{
    ns_js *js = g_new0(ns_js, 1);
    js->pristine_promise = JS_UNDEFINED;
    js->navigator_brand = JS_UNDEFINED;
    return js;
}

void
ns_js_set_time_origin(ns_js *js, gint64 origin_us, double origin_real_ms)
{
    js->time_origin_us = origin_us;
    js->time_origin_real_ms = origin_real_ms;
}

gboolean
ns_js_create_runtime(ns_js *js, size_t memory_limit, size_t stack_size,
                     JSInterruptHandler *interrupt,
                     JSHostPromiseRejectionTracker *rejection_tracker)
{
    js->rt = JS_NewRuntime();
    if (!js->rt) return FALSE;
    JS_SetInterruptHandler(js->rt, interrupt, js);
    JS_SetMemoryLimit(js->rt, memory_limit);
    JS_SetHostPromiseRejectionTracker(js->rt, rejection_tracker, NULL);
    JS_SetMaxStackSize(js->rt, stack_size);
    return TRUE;
}

gboolean
ns_js_create_context(ns_js *js)
{
    js->ctx = JS_NewContext(js->rt);
    if (!js->ctx) return FALSE;
    js->main_realm_ctx = js->ctx;
    JS_SetContextOpaque(js->ctx, js);
    JS_SetRuntimeOpaque(js->rt, js);
    JS_SetModuleLoaderFunc2(js->rt, ns_js_module_normalize,
                            ns_js_module_loader, NULL, js);
    return TRUE;
}

void
ns_js_init_tables(ns_js *js, gboolean page)
{
    js->pinned_wrappers_set = g_hash_table_new(g_direct_hash, g_direct_equal);
    if (!page) return;
    js->frame_ctxs = g_ptr_array_new();
    js->cookie_buckets = g_hash_table_new_full(
        g_str_hash, g_str_equal, g_free, g_free);
}

void
ns_js_set_partition_key(ns_js *js, const char *partition_key)
{
    js->partition_key = g_strdup(partition_key);
}

void *
ns_js_runtime_state(const ns_js *js)
{
    return js ? js->runtime : NULL;
}

void
ns_js_set_runtime_state(ns_js *js, void *state)
{
    js->runtime = state;
}

void
ns_js_slots_of(ns_js *js, ns_js_slots *out)
{
    out->rt = &js->rt;
    out->ctx = &js->ctx;
    out->main_realm_ctx = &js->main_realm_ctx;
    out->current_url = &js->current_url;
    out->current_doc = &js->current_doc;
    out->raf_frame_ctx = &js->raf_frame_ctx;
    out->ce_main_doc = &js->ce_main_doc;
    out->halted = &js->halted;
    out->pinned_wrappers_set = &js->pinned_wrappers_set;
}

void
ns_js_release_wrapper(ns_js *js, ns_node *node)
{
    JS_FreeValue(js->ctx, JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper));
}

void
ns_js_set_proto_document(ns_js *js, JSValue proto)
{
    JS_FreeValue(js->ctx, js->proto_document);
    js->proto_document = proto;
}

void
ns_js_set_navigator_brand(ns_js *js, JSValue brand)
{
    JS_FreeValue(js->ctx, js->navigator_brand);
    js->navigator_brand = brand;
}

void
ns_js_set_pristine_promise(ns_js *js, JSValue promise)
{
    JS_FreeValue(js->ctx, js->pristine_promise);
    js->pristine_promise = promise;
}

void
ns_tag_caller_document(JSContext *ctx, JSValueConst node_val)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js || !JS_IsObject(node_val)) return;
    JSContext *rctx = JS_GetCallerRealm(ctx);
    JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    JSValue global = JS_GetGlobalObject(rctx);
    JSValue doc = JS_GetPropertyStr(rctx, global, "document");
    if (rctx != main_ctx && JS_IsObject(doc))
        JS_DefinePropertyValueStr(ctx, node_val, "__ndOwnerDoc",
                                  JS_DupValue(ctx, doc), 0);
    else
        ns_tag_owner_document(ctx, doc, node_val);
    JS_FreeValue(rctx, doc);
    JS_FreeValue(rctx, global);
}

GHashTable *
ns_js_pinned_wrappers(const ns_js *js)
{
    return js->pinned_wrappers_set;
}

void
ns_js_detach_wrapper(ns_js *js, ns_node *node, gboolean release)
{
    if (!node || !node->js_wrapper) return;
    JSValue v = JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper);
    JS_SetOpaque(v, NULL);
    node->js_wrapper = NULL;
    node->js_invalidate = NULL;
    if (release) JS_FreeValue(js->ctx, v);
}

void
ns_js_free_frame_contexts(ns_js *js)
{
    if (!js->frame_ctxs) return;
    for (guint i = 0; i < js->frame_ctxs->len; i++)
        JS_FreeContext(g_ptr_array_index(js->frame_ctxs, i));
    g_ptr_array_set_size(js->frame_ctxs, 0);
}

guint
ns_js_frame_context_count(const ns_js *js)
{
    return js->frame_ctxs ? js->frame_ctxs->len : 0;
}

void
ns_js_eval_polyfills(ns_js *js)
{
    ns_js_eval(js, ns_js_polyfills_src,
               sizeof(ns_js_polyfills_src) - 1, "<polyfills>");
    ns_js_eval(js, ns_js_streaming_src,
               sizeof(ns_js_streaming_src) - 1, "<streaming>");
}

void
ns_js_eval_worker_polyfills(ns_js *js)
{
    ns_js_eval(js, ns_js_polyfills_src,
               sizeof(ns_js_polyfills_src) - 1, "<worker-polyfills>");
}

void
ns_js_snapshot_platform_globals(ns_js *js)
{
    if (js->platform_globals) g_hash_table_destroy(js->platform_globals);
    js->platform_globals = ns_js_snapshot_globals(js);
}

void
ns_js_free_page_strings(ns_js *js)
{
    if (js->csp) { ns_csp_free(js->csp); js->csp = NULL; }
    g_clear_pointer(&js->cookie_value, g_free);
    g_clear_pointer(&js->current_url, g_free);
}

void
ns_js_free_pinned_set(ns_js *js)
{
    g_clear_pointer(&js->pinned_wrappers_set, g_hash_table_destroy);
}

void
ns_js_free_realm_values(ns_js *js)
{
    JS_FreeValue(js->ctx, js->pristine_promise);
    js->pristine_promise = JS_UNDEFINED;
    if (!js->dom_protos_set) return;
    JS_FreeValue(js->ctx, js->proto_node);
    JS_FreeValue(js->ctx, js->proto_element);
    JS_FreeValue(js->ctx, js->proto_htmlelement);
    JS_FreeValue(js->ctx, js->proto_svgelement);
    JS_FreeValue(js->ctx, js->proto_svgaelement);
    JS_FreeValue(js->ctx, js->proto_mathmlelement);
    JS_FreeValue(js->ctx, js->proto_chardata);
    JS_FreeValue(js->ctx, js->proto_text);
    JS_FreeValue(js->ctx, js->proto_comment);
    JS_FreeValue(js->ctx, js->proto_cdata);
    JS_FreeValue(js->ctx, js->proto_pi);
    JS_FreeValue(js->ctx, js->proto_doctype);
    JS_FreeValue(js->ctx, js->proto_docfrag);
    JS_FreeValue(js->ctx, js->proto_document);
    JS_FreeValue(js->ctx, js->proto_htmlunknownelement);
    if (js->per_tag_protos) {
        GHashTableIter it;
        gpointer k, v;
        g_hash_table_iter_init(&it, js->per_tag_protos);
        while (g_hash_table_iter_next(&it, &k, &v))
            JS_FreeValue(js->ctx, *(JSValue *)v);
        g_hash_table_destroy(js->per_tag_protos);
        js->per_tag_protos = NULL;
    }
    js->dom_protos_set = 0;
}

void
ns_js_free_platform_globals(ns_js *js)
{
    g_clear_pointer(&js->platform_globals, g_hash_table_destroy);
}

void
ns_js_free_partition(ns_js *js)
{
    g_clear_pointer(&js->cookie_buckets, g_hash_table_destroy);
    g_clear_pointer(&js->partition_key, g_free);
}

void
ns_js_free_lookup_caches(ns_js *js)
{
    if (js->form_data_helper_set) {
        JS_FreeValue(js->ctx, js->form_data_helper);
        js->form_data_helper_set = 0;
    }
}

void
ns_js_free_window_values(ns_js *js)
{
    JS_FreeValue(js->ctx, js->navigator_brand);
    js->navigator_brand = JS_UNDEFINED;
}

void
ns_js_free_engine(ns_js *js)
{
    if (js->frame_ctxs) {
        for (guint i = 0; i < js->frame_ctxs->len; i++)
            JS_FreeContext(g_ptr_array_index(js->frame_ctxs, i));
        g_ptr_array_free(js->frame_ctxs, TRUE);
        js->frame_ctxs = NULL;
    }
    if (js->ctx) JS_FreeContext(js->ctx);
    if (js->rt) JS_FreeRuntime(js->rt);
    g_free(js);
}

void
ns_js_apply_site_quirks(ns_js *js)
{
    static const char src[] =
        "(function(){"
        "try{"
        "var l=location&&location.hostname||'';"
        "if(l!=='freecivweb.com'&&l!=='www.freecivweb.com'&&l!=='fcw.movingborders.es')return;"
        "if(typeof tile_types_setup!=='object'||!tile_types_setup)return;"
        "if(typeof MATCH_NONE!=='number'||typeof MATCH_SAME!=='number'||"
        "typeof MATCH_PAIR!=='number'||typeof MATCH_FULL!=='number'||"
        "typeof CELL_WHOLE!=='number'||typeof CELL_CORNER!=='number')return;"
        "var r=typeof MATCH_RANDOM==='number'?MATCH_RANDOM:4,t=tile_types_setup;"
        "function s(k,m,c){var o=t[k];if(!o)return;if(o.match_style===void 0)o.match_style=m;if(o.sprite_type===void 0)o.sprite_type=c;}"
        "s('l0.lake',MATCH_PAIR,CELL_CORNER);"
        "s('l0.coast',MATCH_FULL,CELL_CORNER);"
        "s('l1.coast',MATCH_PAIR,CELL_CORNER);"
        "s('l0.floor',MATCH_FULL,CELL_CORNER);"
        "s('l1.floor',MATCH_PAIR,CELL_CORNER);"
        "s('l0.arctic',MATCH_NONE,CELL_WHOLE);"
        "s('l0.desert',MATCH_NONE,CELL_WHOLE);"
        "s('l1.desert',r,CELL_WHOLE);"
        "s('l0.forest',MATCH_NONE,CELL_WHOLE);"
        "s('l1.forest',MATCH_SAME,CELL_WHOLE);"
        "s('l0.grassland',MATCH_NONE,CELL_WHOLE);"
        "s('l1.grassland',r,CELL_WHOLE);"
        "s('l0.hills',MATCH_NONE,CELL_WHOLE);"
        "s('l1.hills',MATCH_SAME,CELL_WHOLE);"
        "s('l0.jungle',MATCH_NONE,CELL_WHOLE);"
        "s('l1.jungle',MATCH_SAME,CELL_WHOLE);"
        "s('l0.mountains',MATCH_NONE,CELL_WHOLE);"
        "s('l1.mountains',MATCH_SAME,CELL_WHOLE);"
        "s('l0.plains',MATCH_NONE,CELL_WHOLE);"
        "s('l1.plains',r,CELL_WHOLE);"
        "s('l0.swamp',MATCH_NONE,CELL_WHOLE);"
        "s('l1.swamp',r,CELL_WHOLE);"
        "s('l0.tundra',MATCH_NONE,CELL_WHOLE);"
        "s('l0.inaccessible',MATCH_NONE,CELL_WHOLE);"
        "}catch(e){}"
        "})()";
    if (!js || js->halted || !js->current_url ||
        (!strstr(js->current_url, "freecivweb.com") &&
         !strstr(js->current_url, "fcw.movingborders.es")))
        return;
    JSValue v = JS_Eval(js->ctx, src, strlen(src), "site-quirks",
                        JS_EVAL_TYPE_GLOBAL);
    if (JS_IsException(v))
        JS_FreeValue(js->ctx, JS_GetException(js->ctx));
    JS_FreeValue(js->ctx, v);
}

char *
ns_js_exception_message(JSContext *ctx, JSValueConst ex)
{
    const char *es = JS_ToCString(ctx, ex);
    if (!es) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        return g_strdup("Script error.");
    }
    char *out = g_strdup(es);
    JS_FreeCString(ctx, es);
    return out;
}

void
ns_js_add_csp_header(ns_js *js, const char *header_value)
{
    if (!js || !header_value || !*header_value) return;
    ns_csp *parsed = ns_csp_parse(header_value);
    if (!parsed) return;
    if (js->csp) {
        ns_csp_merge(js->csp, parsed);
        ns_csp_free(parsed);
    } else {
        js->csp = parsed;
    }
}

gboolean
ns_js_csp_form_action_allowed(const ns_js *js, const char *action_url)
{
    if (!js || !js->csp || !action_url) return TRUE;
    return ns_csp_allows(js->csp, NS_CSP_FORM_ACTION, action_url,
                         js->current_url);
}

const char *
ns_js_storage_partition(const ns_js *js)
{
    return js ? js->partition_key : NULL;
}

gboolean
ns_js_inline_handlers_allowed(const ns_js *js)
{
    return ns_csp_inline_event_handler_allowed(js->csp);
}

gint64
ns_js_page_time_origin_us(const ns_js *js)
{
    return js->time_origin_us;
}

double
ns_js_page_time_origin_real_ms(const ns_js *js)
{
    return js->time_origin_real_ms;
}

JSValue
ns_js_navigator_brand(const ns_js *js)
{
    return js->navigator_brand;
}

JSValue
ns_js_pristine_promise(const ns_js *js)
{
    return js->pristine_promise;
}

const ns_box *
ns_js_layout_root(const ns_js *js)
{
    return js->layout_root;
}

const char *
ns_js_cookie_value(const ns_js *js)
{
    return js->cookie_value;
}

void
ns_js_set_cookie_value(ns_js *js, const char *value)
{
    g_free(js->cookie_value);
    js->cookie_value = g_strdup(value);
}

const char *
ns_js_partition_key(const ns_js *js)
{
    return js->partition_key;
}

const ns_js_navigation_timing *
ns_js_page_navigation_timing(const ns_js *js)
{
    return &js->navigation_timing;
}

const ns_csp *
ns_js_page_csp(const ns_js *js)
{
    return js ? js->csp : NULL;
}

gboolean
ns_js_net_csp_allows_connect(const ns_js *js, const char *url, const char *page)
{
    return !js || !js->csp || ns_csp_allows(js->csp, NS_CSP_CONNECT, url, page);
}
