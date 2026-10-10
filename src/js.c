/* Southstar — JavaScript engine binding (QuickJS).
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "js.h"
#include "js_classid.h"

JSClassID ns_new_class_id(JSClassID *pclass_id)
{
    static gint next_id = 192;
    gint id = g_atomic_int_get((gint *)pclass_id);
    if (id == 0) {
        gint fresh = g_atomic_int_add(&next_id, 1);
        if (!g_atomic_int_compare_and_exchange((gint *)pclass_id, 0, fresh))
            id = g_atomic_int_get((gint *)pclass_id);
        else
            id = fresh;
    }
    return (JSClassID)id;
}
#include "polyfills.h"
#include "streaming.h"
#include "version.h"

#include <math.h>
#include <string.h>
#include <time.h>

#include <zlib.h>

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
#include "bytecode_cache.h"
#include "camera.h"
#include "mic.h"
#include "config.h"
#include "css.h"
#include "datetime.h"
#include "debuglog.h"
#include "engine.h"
#include "ext.h"
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

#undef JS_CFUNC_DEF
#define JS_CFUNC_DEF(name, length, func1) \
    { name, JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE, \
      JS_DEF_CFUNC, 0, \
      { .func = { length, JS_CFUNC_generic, { .generic = func1 } } } }
#undef JS_CGETSET_DEF
#define JS_CGETSET_DEF(name, fgetter, fsetter) \
    { name, JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE, JS_DEF_CGETSET, 0, \
      { .getset = { .get = { .getter = fgetter }, \
                    .set = { .setter = fsetter } } } }
#undef JS_CGETSET_MAGIC_DEF
#define JS_CGETSET_MAGIC_DEF(name, fgetter, fsetter, magic) \
    { name, JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE, \
      JS_DEF_CGETSET_MAGIC, magic, \
      { .getset = { .get = { .getter_magic = fgetter }, \
                    .set = { .setter_magic = fsetter } } } }

#define NS_JS_BODY_BYTES_MAX (32u * 1024u * 1024u)

static GPrivate g_active_js_key = G_PRIVATE_INIT(NULL);

static ns_js *
ns_active_js(void)
{
    return g_private_get(&g_active_js_key);
}

static void
ns_set_active_js(ns_js *js)
{
    g_private_set(&g_active_js_key, js);
}

#define NS_TRANSIENT_ACTIVATION_US (5 * G_USEC_PER_SEC)

void
ns_js_note_user_activation(ns_js *js)
{
    if (!js) return;
    js->user_activation_us = g_get_monotonic_time();
    js->user_ever_activated = TRUE;
}

gboolean
ns_js_has_transient_activation(ns_js *js)
{
    return js && js->user_activation_us != 0 &&
           g_get_monotonic_time() - js->user_activation_us <=
               NS_TRANSIENT_ACTIVATION_US;
}

gboolean
ns_js_user_activation_state(ns_js *js, gboolean *ever_activated)
{
    *ever_activated = js && js->user_ever_activated;
    return ns_js_has_transient_activation(js);
}

int
ns_js_clipboard_write(ns_js *js, const char *text)
{
    if (!js || !js->clipboard_write_cb) return -1;
    return js->clipboard_write_cb(text, js->clipboard_write_user_data) ? 1 : 0;
}

void
ns_js_consume_user_activation(ns_js *js)
{
    if (js) js->user_activation_us = 0;
}

static char *ns_js_module_normalize(JSContext *ctx, const char *base_name,
                                    const char *name, void *opaque);
static void ns_js_eval(ns_js *js, const char *src, gsize len, const char *origin);
static JSModuleDef *ns_js_module_loader(JSContext *ctx,
                                        const char *module_name, void *opaque,
                                        JSValueConst attributes);
static JSValue ns_js_compile_module_cached(JSContext *ctx, const char *src,
                                           gsize len, const char *module_name);
static int ns_js_module_set_import_meta(JSContext *ctx,
                                        JSValueConst module,
                                        gboolean is_main);
static void ns_attribute_map_release_owner(ns_js *js, ns_node *owner);
static void ns_attribute_maps_release_all(ns_js *js);
static void ns_ce_attr_changed(ns_js *js, ns_node *node, const char *attr,
                               const char *old_value, const char *new_value);
static const ns_node *ns_node_owner_iframe(const ns_node *n);
static JSValue ns_make_performance_object(JSContext *ctx, ns_js *js,
                                          gboolean include_memory);
static void ns_js_schedule_iframe_load_full(ns_js *js, ns_node *iframe,
                                            gboolean force);
static void ns_js_process_pending_iframes(ns_js *js);
static void ns_js_record_attr_change(ns_js *js, ns_node *target,
                                     const char *name, const char *old_value);
static JSValue ns_element_get_list_ref(JSContext *ctx, JSValueConst this_val);
static JSValue ns_proto_of(JSContext *ctx, JSValueConst global,
                           const char *ctor_name);
typedef struct ns_hostobj {
    ns_ho_kind kind;
    JSValue    state;
    gpointer   native;
    void     (*free_native)(JSRuntime *rt, gpointer native);
} ns_hostobj;
static ns_hostobj *ns_ho_of(JSValueConst v, ns_ho_kind kind);
static void ns_js_name_engine_members(JSContext *ctx);
static void ns_js_link_interfaces(JSContext *ctx);
static void ns_hide_shared_array_buffer(JSContext *ctx, JSValueConst global);
static gboolean ns_js_image_loads_pending(const ns_js *js);
static ns_node *ns_iframe_document_node(const ns_node *iframe);
static void ns_js_purge_subtree_rafs(ns_js *js, ns_node *root);
static gboolean ns_dom_hidden_child(const ns_node *c);

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

static gint64
ns_js_eval_budget_us(void)
{
    const ns_config *c = ns_config_get();
    int ms = c ? c->js_eval_budget_ms : 60000;
    if (ms <= 0) ms = 60000;
    if (ms > NS_JS_EVAL_BUDGET_MAX_MS) ms = NS_JS_EVAL_BUDGET_MAX_MS;
    return (gint64)ms * 1000LL;
}

#define NS_JS_MONITOR_LIMIT_US (60LL * 1000000LL)

static int
ns_js_interrupt_cb(JSRuntime *rt, void *opaque)
{
    (void)rt;
    ns_js *js = opaque;
    if (!js) return 0;
    if (js->halted) return 1;
    if (js->worker_host && ns_worker_host_closing(js->worker_host)) {
        js->halted = TRUE;
        return 1;
    }
    gint64 now = g_get_monotonic_time();
    if (js->js_monitor_deadline_us != 0 && now > js->js_monitor_deadline_us) {
        js->halted = TRUE;
        g_warning("[js] monitor: page JavaScript ran longer than %d s — halting it",
                  (int)(NS_JS_MONITOR_LIMIT_US / 1000000LL));
        return 1;
    }
    if (js->eval_deadline_us == 0) return 0;
    return now > js->eval_deadline_us ? 1 : 0;
}

gboolean
ns_js_in_pump(const ns_js *js)
{
    return js && js->in_pump;
}

void
ns_js_credit_pumped_time(ns_js *js, gint64 pump_start_us)
{
    if (!js) return;
    gint64 pumped_us = g_get_monotonic_time() - pump_start_us;
    if (pumped_us <= 0) return;
    if (js->eval_deadline_us != 0)
        js->eval_deadline_us += pumped_us;
    if (js->js_monitor_deadline_us != 0)
        js->js_monitor_deadline_us += pumped_us;
}

typedef struct {
    GMainLoop   *loop;
    ns_response *resp;
    GError      *err;
    gboolean     done;
} ns_js_pumped_fetch;

static void
ns_js_pumped_fetch_done(GObject *src, GAsyncResult *result, gpointer user_data)
{
    (void)src;
    ns_js_pumped_fetch *pf = user_data;
    pf->resp = ns_net_fetch_finish(result, &pf->err);
    pf->done = TRUE;
    if (pf->loop) g_main_loop_quit(pf->loop);
}

static ns_response *
ns_js_fetch_resource(ns_js *js, const char *url, const char *top_url,
                     const char *const *headers, GError **error)
{
    if (!js || js->worker_host)
        return ns_net_request_blocking(url, top_url, "GET", NULL, 0, NULL,
                                       headers, NULL, error);

    ns_js_pumped_fetch pf = {0};
    pf.loop = g_main_loop_new(NULL, FALSE);
    ns_net_request_async(url, top_url, "GET", NULL, 0, NULL, headers, NULL,
                         ns_js_pumped_fetch_done, &pf);
    gboolean saved = js->in_pump;
    js->in_pump = TRUE;
    gint64 pump_start_us = g_get_monotonic_time();
    if (!pf.done)
        g_main_loop_run(pf.loop);
    ns_js_credit_pumped_time(js, pump_start_us);
    js->in_pump = saved;
    g_main_loop_unref(pf.loop);
    if (pf.err) {
        if (error) *error = pf.err;
        else g_error_free(pf.err);
    }
    return pf.resp;
}

/* ns_js_fetch_resource for one of a document's own subresources, recorded
 * as a PerformanceResourceTiming entry with the given initiator type in the
 * timeline info names. */
ns_response *
ns_js_fetch_subresource(ns_js *js, const char *url, const char *top_url,
                        const char *const *headers, GError **error,
                        const char *initiator, const ns_perf_resource_info *info)
{
    gint64 start_us = g_get_monotonic_time();
    ns_response *resp = ns_js_fetch_resource(js, url, top_url, headers, error);
    if (js && !js->worker_host && initiator)
        ns_perf_add_resource_timed(js, info, url, initiator, start_us,
                                   g_get_monotonic_time(), resp);
    return resp;
}

/* The URL of the document whose global is realm: a frame's own URL for a
 * frame realm, the page's otherwise. */
const char *
ns_js_realm_document_url(ns_js *js, JSContext *realm)
{
    if (!js) return NULL;
    if (realm && realm != js->main_realm_ctx) {
        const char *url = ns_js_frame_url(js, ns_js_frame_of_realm(js, realm));
        if (url) return url;
    }
    return js->current_url;
}

typedef struct ns_budget_guard {
    gint64 saved;
} ns_budget_guard;

static void
ns_js_budget_push(ns_js *js, ns_budget_guard *g)
{
    if (!js || !g) return;
    g->saved = js->eval_deadline_us;
    gint64 now = g_get_monotonic_time();
    if (g->saved == 0) {
        js->task_epoch++;
        if (js->task_epoch == 0) js->task_epoch++;
        js->js_monitor_deadline_us = now + NS_JS_MONITOR_LIMIT_US;
    }
    gint64 fresh = now + ns_js_eval_budget_us();
    if (g->saved == 0 || fresh < g->saved)
        js->eval_deadline_us = fresh;
}

static void
ns_js_budget_pop(ns_js *js, ns_budget_guard *g)
{
    if (!js || !g) return;
    js->eval_deadline_us = g->saved;
    if (g->saved == 0)
        js->js_monitor_deadline_us = 0;
}

gint64
ns_js_budget_enter(ns_js *js)
{
    ns_budget_guard g = {0};
    ns_js_budget_push(js, &g);
    return g.saved;
}

void
ns_js_budget_leave(ns_js *js, gint64 saved)
{
    ns_budget_guard g = { saved };
    ns_js_budget_pop(js, &g);
}

typedef struct ns_raf_entry {
    int      id;
    JSContext *ctx;
    JSValue  cb;
    gboolean video_frame;
    ns_node *frame;
    ns_node *media;
} ns_raf_entry;

ns_node *
ns_js_context_frame(ns_js *js, JSContext *ctx)
{
    if (!js || !ctx || ctx == js->main_realm_ctx) return NULL;
    ns_node *frame = ns_js_frame_of_realm(js, ctx);
    return frame ? frame : js->raf_frame_ctx;
}












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

gboolean
ns_js_bytes_view(JSContext *ctx, JSValueConst value, const uint8_t **out_data,
                 size_t *out_len, JSValue *out_holder)
{
    if (out_data) *out_data = NULL;
    if (out_len) *out_len = 0;
    if (out_holder) *out_holder = JS_UNDEFINED;

    if (JS_IsArrayBuffer(value)) {
        size_t total = 0;
        uint8_t *base = JS_GetArrayBuffer(ctx, &total, value);
        if (base) {
            if (out_data) *out_data = base;
            if (out_len) *out_len = total;
            if (out_holder) *out_holder = JS_DupValue(ctx, value);
            return TRUE;
        }
        return FALSE;
    }

    if (!JS_IsObject(value)) return FALSE;

    JSValue buf = JS_GetPropertyStr(ctx, value, "buffer");
    if (JS_IsException(buf)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        return FALSE;
    }
    if (!JS_IsArrayBuffer(buf)) {
        JS_FreeValue(ctx, buf);
        return FALSE;
    }

    JSValue off_v = JS_GetPropertyStr(ctx, value, "byteOffset");
    JSValue len_v = JS_GetPropertyStr(ctx, value, "byteLength");
    if (JS_IsException(off_v) || JS_IsException(len_v)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, off_v);
        JS_FreeValue(ctx, len_v);
        JS_FreeValue(ctx, buf);
        return FALSE;
    }
    gboolean ok = JS_IsNumber(off_v) && JS_IsNumber(len_v);
    uint64_t byte_off = 0, byte_len = 0;
    if (ok && (JS_ToIndex(ctx, &byte_off, off_v) < 0 ||
               JS_ToIndex(ctx, &byte_len, len_v) < 0)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        ok = FALSE;
    }
    JS_FreeValue(ctx, off_v);
    JS_FreeValue(ctx, len_v);
    if (!ok) {
        JS_FreeValue(ctx, buf);
        return FALSE;
    }

    size_t total = 0;
    uint8_t *base = JS_GetArrayBuffer(ctx, &total, buf);
    if (!base || byte_off > total || byte_len > total - byte_off) {
        JS_FreeValue(ctx, buf);
        return FALSE;
    }
    if (out_data) *out_data = base + byte_off;
    if (out_len) *out_len = (size_t)byte_len;
    if (out_holder) *out_holder = buf;
    else JS_FreeValue(ctx, buf);
    return TRUE;
}

static char *
ns_js_copy_bytes(const uint8_t *data, gsize len)
{
    if (len > NS_JS_BODY_BYTES_MAX) return NULL;
    char *out = g_malloc(len + 1);
    if (len > 0 && data) memcpy(out, data, len);
    out[len] = '\0';
    return out;
}

char *
ns_js_body_bytes(JSContext *ctx, JSValueConst value, gsize *out_len)
{
    if (out_len) *out_len = 0;
    if (JS_IsUndefined(value) || JS_IsNull(value)) return NULL;

    if (JS_IsString(value)) {
        size_t slen = 0;
        const char *s = JS_ToCStringLen(ctx, &slen, value);
        if (!s) return NULL;
        char *copy = ns_js_copy_bytes((const uint8_t *)s, (gsize)slen);
        JS_FreeCString(ctx, s);
        if (copy && out_len) *out_len = (gsize)slen;
        return copy;
    }

    const uint8_t *data = NULL;
    size_t len = 0;
    JSValue holder = JS_UNDEFINED;
    if (ns_js_bytes_view(ctx, value, &data, &len, &holder)) {
        char *copy = ns_js_copy_bytes(data, (gsize)len);
        JS_FreeValue(ctx, holder);
        if (copy && out_len) *out_len = (gsize)len;
        return copy;
    }

    if (JS_IsObject(value)) {
        JSValue b = JS_GetPropertyStr(ctx, value, "__ndBlobBytes");
        if (JS_IsException(b)) {
            JS_FreeValue(ctx, JS_GetException(ctx));
            return NULL;
        }
        gboolean has_blob_bytes = !JS_IsUndefined(b) && !JS_IsNull(b);
        JS_FreeValue(ctx, b);
        if (has_blob_bytes) return ns_blob_bytes_as_string(ctx, value, out_len);
    }

    return NULL;
}

gboolean
ns_js_value_is_form_data(JSContext *ctx, JSValueConst v)
{
    (void)ctx;
    return ns_ho_of(v, NS_HO_FORM_DATA) != NULL;
}

gboolean
ns_js_value_is_url_search_params(JSContext *ctx, JSValueConst v)
{
    if (!JS_IsObject(v) || JS_IsFunction(ctx, v)) return FALSE;
    JSValue p = JS_GetPropertyStr(ctx, v, "__ndPairs");
    gboolean is_arr = JS_IsArray(p);
    JS_FreeValue(ctx, p);
    if (!is_arr) return FALSE;
    JSValue a = JS_GetPropertyStr(ctx, v, "append");
    JSValue s = JS_GetPropertyStr(ctx, v, "getAll");
    gboolean ok = JS_IsFunction(ctx, a) && JS_IsFunction(ctx, s);
    JS_FreeValue(ctx, a);
    JS_FreeValue(ctx, s);
    return ok;
}

char *
ns_js_usp_serialize(JSContext *ctx, JSValueConst usp,
                    gsize *out_len, char **out_content_type)
{
    if (out_content_type)
        *out_content_type =
            g_strdup("application/x-www-form-urlencoded;charset=UTF-8");
    JSValue ts_fn = JS_GetPropertyStr(ctx, usp, "toString");
    JSValue str = JS_IsFunction(ctx, ts_fn)
        ? JS_Call(ctx, ts_fn, usp, 0, NULL) : JS_NewString(ctx, "");
    JS_FreeValue(ctx, ts_fn);
    if (JS_IsException(str)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        if (out_len) *out_len = 0;
        return g_strdup("");
    }
    size_t l = 0;
    const char *s = JS_ToCStringLen(ctx, &l, str);
    char *out = s ? g_strndup(s, l) : g_strdup("");
    if (s) JS_FreeCString(ctx, s);
    JS_FreeValue(ctx, str);
    if (out_len) *out_len = out ? strlen(out) : 0;
    return out;
}

void
ns_js_promise_reject(JSContext *ctx, JSValue resolvers[2], const char *message)
{
    JSValue err = JS_NewError(ctx);
    const char *colon = strchr(message, ':');
    gsize name_len = colon ? (gsize)(colon - message) : strlen(message);
    gboolean is_dom_name = name_len > 5 && name_len < 64 &&
        memcmp(message + name_len - 5, "Error", 5) == 0;
    for (gsize i = 0; is_dom_name && i < name_len; i++)
        if (!g_ascii_isalnum((guchar)message[i])) is_dom_name = FALSE;
    if (is_dom_name) {
        char *nm = g_strndup(message, name_len);
        JS_SetPropertyStr(ctx, err, "name", JS_NewString(ctx, nm));
        g_free(nm);
        const char *rest = colon ? colon + 1 : message;
        while (*rest == ' ') rest++;
        JS_SetPropertyStr(ctx, err, "message", JS_NewString(ctx, rest));
    } else {
        JS_SetPropertyStr(ctx, err, "message", JS_NewString(ctx, message));
    }
    JS_Call(ctx, resolvers[1], JS_UNDEFINED, 1, &err);
    JS_FreeValue(ctx, err);
    JS_FreeValue(ctx, resolvers[0]);
    JS_FreeValue(ctx, resolvers[1]);
}

static gboolean
ns_node_name_is_any_of(const ns_node *n, const char *const *tags)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    for (; *tags; tags++)
        if (g_ascii_strcasecmp(n->name, *tags) == 0) return TRUE;
    return FALSE;
}

void
ns_js_source_remove(ns_js *js, guint id)
{
    if (!id) return;
    GMainContext *context = js && js->main_context ? js->main_context
                                                   : g_main_context_default();
    GSource *source = g_main_context_find_source_by_id(context, id);
    if (source) g_source_destroy(source);
}

guint
ns_js_attach_timeout(ns_js *js, guint ms, GSourceFunc func, gpointer data)
{
    GSource *source = g_timeout_source_new(ms);
    g_source_set_callback(source, func, data, NULL);
    guint id = g_source_attach(source, js && js->main_context ? js->main_context : NULL);
    g_source_unref(source);
    return id;
}

guint
ns_js_attach_idle(ns_js *js, GSourceFunc func, gpointer data)
{
    GSource *source = g_idle_source_new();
    g_source_set_callback(source, func, data, NULL);
    guint id = g_source_attach(source, js && js->main_context ? js->main_context : NULL);
    g_source_unref(source);
    return id;
}


typedef struct {
    JSContext *ctx;
    JSJobFunc *func;
    int argc;
    JSValue argv[3];
} ns_message_task;


static void
ns_message_task_free(ns_message_task *task)
{
    for (int i = 0; i < task->argc; i++)
        JS_FreeValue(task->ctx, task->argv[i]);
    g_free(task);
}

static void
ns_js_drop_message_tasks(ns_js *js)
{
    if (js->message_task_source) {
        ns_js_source_remove(js, js->message_task_source);
        js->message_task_source = 0;
    }
    if (!js->message_tasks) return;
    ns_message_task *task;
    while ((task = g_queue_pop_head(js->message_tasks)))
        ns_message_task_free(task);
}

static gboolean
ns_js_run_message_task(gpointer data)
{
    ns_js *js = data;
    if (js->halted) {
        js->message_task_source = 0;
        ns_js_drop_message_tasks(js);
        return G_SOURCE_REMOVE;
    }
    if (js->in_pump || ns_engine_in_blocking_fetch()) {
        js->message_task_source =
            ns_js_attach_timeout(js, 4, ns_js_run_message_task, js);
        return G_SOURCE_REMOVE;
    }
    guint pending = js->message_tasks ? g_queue_get_length(js->message_tasks) : 0;
    for (guint i = 0; i < pending && !js->halted; i++) {
        ns_message_task *task = g_queue_pop_head(js->message_tasks);
        if (!task) break;
        JSValue r = task->func(task->ctx, task->argc, task->argv);
        if (JS_IsException(r))
            JS_FreeValue(task->ctx, JS_GetException(task->ctx));
        JS_FreeValue(task->ctx, r);
        ns_message_task_free(task);
        ns_drain_microtasks(js);
    }
    if (pending) ns_drain_mutations(js);
    if (js->message_tasks && !g_queue_is_empty(js->message_tasks))
        return G_SOURCE_CONTINUE;
    js->message_task_source = 0;
    return G_SOURCE_REMOVE;
}

void
ns_js_queue_message_task(JSContext *ctx, JSJobFunc *func, int argc,
                         JSValueConst *argv)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js || argc > 3) {
        JS_EnqueueJob(ctx, func, argc, argv);
        return;
    }
    ns_message_task *task = g_new0(ns_message_task, 1);
    task->ctx = ctx;
    task->func = func;
    task->argc = argc;
    for (int i = 0; i < argc; i++)
        task->argv[i] = JS_DupValue(ctx, argv[i]);
    if (!js->message_tasks) js->message_tasks = g_queue_new();
    g_queue_push_tail(js->message_tasks, task);
    if (!js->message_task_source)
        js->message_task_source =
            ns_js_attach_timeout(js, 0, ns_js_run_message_task, js);
}

typedef struct {
    JSContext *ctx;
    ns_node   *doc;
    ns_node   *frame;
    char      *url;
    char      *entered_url;
    char     **prev_slot;
    gboolean   active;
    gboolean   is_base;
} ns_realm_scope;

/* The top-level document's URL, even while a frame's code has replaced
 * current_url with the frame's own URL. */
const char *
ns_js_top_url(ns_js *js)
{
    const char *url = js->top_url_slot ? *js->top_url_slot : js->current_url;
    return url ? url : "";
}

void
ns_js_set_top_url(ns_js *js, const char *url)
{
    char **slot = js->top_url_slot ? js->top_url_slot : &js->current_url;
    char *copy = g_strdup(url ? url : "");
    g_free(*slot);
    *slot = copy;
}

/* Makes url the current URL for a frame's script or event.  The outermost
 * entry keeps the top-level URL where ns_js_top_url can find it. */
typedef struct {
    char *saved;
    gboolean owns_slot;
} ns_frame_url;

static void
ns_frame_url_enter(ns_js *js, ns_frame_url *fu, const char *url)
{
    fu->saved = js->current_url;
    fu->owns_slot = js->top_url_slot == NULL;
    if (fu->owns_slot) js->top_url_slot = &fu->saved;
    js->current_url = g_strdup(url ? url : "");
}

static void
ns_frame_url_leave(ns_js *js, ns_frame_url *fu)
{
    if (fu->owns_slot) js->top_url_slot = NULL;
    g_free(js->current_url);
    js->current_url = fu->saved;
}

static ns_node *
ns_js_top_document(ns_node *doc)
{
    while (doc && doc->parent) {
        doc = doc->parent;
        while (doc && doc->kind != NS_NODE_DOCUMENT)
            doc = doc->parent;
    }
    return doc;
}

/* Whether node is in the page: in its document or in one of its frames'.
   The engine's current document says which of them code is running for,
   and parent code can run while a frame's document is current. */
gboolean
ns_js_node_in_page(ns_js *js, const ns_node *node)
{
    ns_node *top = js ? ns_js_top_document(js->current_doc) : NULL;
    return top && node && ns_js_top_document((ns_node *)node) == top;
}

static void
ns_js_realm_scope_save(ns_js *js, ns_realm_scope *scope)
{
    scope->ctx = js->ctx;
    scope->doc = js->current_doc;
    scope->frame = js->raf_frame_ctx;
    scope->url = js->current_url;
    scope->entered_url = NULL;
    scope->prev_slot = js->top_url_slot;
    scope->is_base = FALSE;
    scope->active = TRUE;
}

static void
ns_js_frame_scope_enter(ns_js *js, JSContext *realm, ns_node *frame,
                        ns_realm_scope *scope)
{
    ns_js_realm_scope_save(js, scope);
    if (!js->realm_scope_base && js->ctx == js->main_realm_ctx) {
        js->realm_scope_base = scope;
        scope->is_base = TRUE;
    }
    if (!js->top_url_slot) js->top_url_slot = &scope->url;
    js->ctx = realm;
    ns_node *frame_doc = ns_iframe_document_node(frame);
    if (frame_doc) js->current_doc = frame_doc;
    js->raf_frame_ctx = frame;
    const char *frame_url = ns_element_get_attr(frame, "data-nd-frame-url");
    js->current_url = g_strdup(frame_url ? frame_url : "");
}

const char *
ns_js_realm_url(ns_js *js, JSContext *realm)
{
    if (!js || !realm) return NULL;
    JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    if (realm == main_ctx) return ns_js_top_url(js);
    ns_node *frame = ns_js_frame_of_realm(js, realm);
    const char *url = frame ? ns_element_get_attr(frame, "data-nd-frame-url")
                            : NULL;
    return url && *url ? url : NULL;
}

static void
ns_js_realm_scope_enter(ns_js *js, JSContext *realm, ns_realm_scope *scope)
{
    scope->active = FALSE;
    if (!js || !realm || realm == js->ctx) return;
    if (realm == js->main_realm_ctx) {
        ns_realm_scope *base = js->realm_scope_base;
        ns_js_realm_scope_save(js, scope);
        js->ctx = realm;
        js->raf_frame_ctx = base ? base->frame : NULL;
        js->current_doc = base ? base->doc : ns_js_top_document(js->current_doc);
        js->current_url = g_strdup(ns_js_top_url(js));
        scope->entered_url = g_strdup(js->current_url);
        js->top_url_slot = NULL;
        return;
    }
    ns_node *frame = ns_js_frame_of_realm(js, realm);
    if (!frame) return;
    ns_js_frame_scope_enter(js, realm, frame, scope);
}

static void
ns_js_realm_scope_leave(ns_js *js, ns_realm_scope *scope)
{
    if (!scope->active) return;
    if (scope->entered_url && scope->prev_slot &&
        g_strcmp0(js->current_url, scope->entered_url) != 0) {
        g_free(*scope->prev_slot);
        *scope->prev_slot = g_strdup(js->current_url ? js->current_url : "");
    }
    g_free(scope->entered_url);
    if (scope->is_base) js->realm_scope_base = NULL;
    js->top_url_slot = scope->prev_slot;
    g_free(js->current_url);
    js->current_url = scope->url;
    js->raf_frame_ctx = scope->frame;
    js->current_doc = scope->doc;
    js->ctx = scope->ctx;
}

void
ns_js_microtask_checkpoint(ns_js *js)
{
    if (js && js->ctx && js->callback_depth == 0 && !JS_IsRunningScript(js->ctx))
        ns_drain_microtasks(js);
}

static JSContext *
ns_function_realm(JSContext *ctx, JSValueConst fn)
{
    JSContext *realm = JS_IsFunction(ctx, fn) ? JS_GetFunctionRealm(ctx, fn) : ctx;
    if (!realm) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        realm = ctx;
    }
    return realm;
}

void
ns_drain_microtasks(ns_js *js)
{
    if (!js) return;
    ns_budget_guard g = {0};
    ns_js_budget_push(js, &g);
    JSContext *ctx_out = NULL;
    int r = 0;
    js->callback_depth++;
    for (;;) {
        if (js->eval_deadline_us != 0 &&
            g_get_monotonic_time() > js->eval_deadline_us)
            break;
        ns_realm_scope scope;
        ns_js_realm_scope_enter(js, JS_GetPendingJobRealm(js->rt), &scope);
        r = JS_ExecutePendingJob(js->rt, &ctx_out);
        ns_js_realm_scope_leave(js, &scope);
        if (r <= 0)
            break;
    }
    js->callback_depth--;
    if (r < 0 && js->log_cb) {
        char *msg = NULL;
        if (ctx_out) {
            JSValue ex = JS_GetException(ctx_out);
            const char *raw = JS_ToCString(ctx_out, ex);
            JSValue stack = JS_GetPropertyStr(ctx_out, ex, "stack");
            const char *stack_s = JS_IsUndefined(stack) ? NULL :
                                  JS_ToCString(ctx_out, stack);
            msg = g_strdup_printf("[error] microtask threw: %s%s%s",
                raw ? raw : "(no message)",
                stack_s ? "\n" : "", stack_s ? stack_s : "");
            if (raw) JS_FreeCString(ctx_out, raw);
            if (stack_s) JS_FreeCString(ctx_out, stack_s);
            JS_FreeValue(ctx_out, stack);
            JS_FreeValue(ctx_out, ex);
        } else {
            msg = g_strdup("[error] microtask threw");
        }
        js->log_cb(msg, js->log_user_data);
        g_free(msg);
    }
    ns_js_budget_pop(js, &g);
    ns_storage_drain_deferred_events(js);
    if (js->callback_depth == 0)
        ns_js_report_pending_rejections(js);
}

gboolean
ns_js_due_timers_allowed(const ns_js *js)
{
    if (!js || !js->ctx || js->halted || js->in_pump ||
        js->dispatch_depth > 0 || js->iframe_load_depth > 0 ||
        js->callback_depth > 0 || js->eval_depth > 0)
        return FALSE;
    return !ns_engine_in_blocking_fetch();
}

void
ns_drain_mutations(ns_js *js)
{
    ns_drain_microtasks(js);
    if (js->mutated && js->mut_cb)
        js->mut_cb(js->mut_user_data);
    js->mutated = FALSE;
    ns_storage_schedule_flush(js);
    ns_services_run_due_timers(js);
}

gint64
ns_js_idle_frame_end(const ns_js *js, gint64 now, gint64 end)
{
    if (!js || !js->raf_pending || js->raf_pending->len == 0) return end;
    gint64 frame = (js->raf_last_us > 0 ? js->raf_last_us : now) + 16667;
    if (frame > now && frame < end) end = frame;
    return end;
}

struct ns_timer_scope {
    ns_js         *js;
    JSContext     *ctx;
    JSContext     *previous_ctx;
    ns_realm_scope frame_scope;
    ns_budget_guard budget;
};

int
ns_js_timer_gate(ns_js *js, ns_node *frame, gboolean idle_expired)
{
    if (js->halted) return NS_TIMER_DROP;
    if (js->in_pump) return NS_TIMER_WAIT;
    if (ns_engine_in_blocking_fetch() && !idle_expired) return NS_TIMER_WAIT;
    if (frame && ns_node_root(frame) != ns_node_root(js->current_doc))
        return NS_TIMER_DROP;
    return NS_TIMER_RUN;
}

ns_timer_scope *
ns_js_timer_scope_enter(ns_js *js, JSContext *ctx, ns_node *frame)
{
    ns_timer_scope *scope = g_new0(ns_timer_scope, 1);
    scope->js = js;
    scope->previous_ctx = js->ctx;
    scope->ctx = ctx ? ctx : scope->previous_ctx;
    if (frame) {
        JSContext *current_realm = ns_js_node_realm_context(js, frame);
        if (current_realm) scope->ctx = current_realm;
    }
    scope->frame_scope.active = FALSE;
    if (frame)
        ns_js_frame_scope_enter(js, scope->ctx, frame, &scope->frame_scope);
    else
        js->ctx = scope->ctx;
    ns_js_budget_push(js, &scope->budget);
    js->callback_depth++;
    return scope;
}

JSContext *
ns_js_timer_scope_context(const ns_timer_scope *scope)
{
    return scope->ctx;
}

void
ns_js_timer_scope_leave(ns_timer_scope *scope, gboolean threw,
                        JSValueConst exception)
{
    ns_js *js = scope->js;
    JSContext *ctx = scope->ctx;
    js->callback_depth--;
    ns_js_budget_pop(js, &scope->budget);
    if (threw) {
        const char *msg = JS_ToCString(ctx, exception);
        JSValue stack = JS_GetPropertyStr(ctx, exception, "stack");
        const char *stk = JS_IsUndefined(stack)
            ? NULL : JS_ToCString(ctx, stack);
        if (msg && js->log_cb) {
            char *line = g_strdup_printf("JS error in timer: %s%s%s",
                msg, stk ? "\n" : "", stk ? stk : "");
            js->log_cb(line, js->log_user_data);
            g_free(line);
        }
        if (stk) JS_FreeCString(ctx, stk);
        JS_FreeValue(ctx, stack);
        if (msg) JS_FreeCString(ctx, msg);
        ns_js_report_uncaught(js, exception, js->current_url);
    }
    ns_drain_mutations(js);
    if (scope->frame_scope.active)
        ns_js_realm_scope_leave(js, &scope->frame_scope);
    else
        js->ctx = scope->previous_ctx;
    g_free(scope);
}

gboolean
ns_timer_this_is_detached_window(ns_js *js, JSContext *ctx,
                                 JSValueConst this_val)
{
    if (!js || !JS_IsObject(this_val)) return FALSE;
    JSValue docv = JS_GetPropertyStr(ctx, this_val, "document");
    const ns_node *doc = ns_unwrap_element(docv);
    JS_FreeValue(ctx, docv);
    if (!doc || doc == js->current_doc) return FALSE;
    if (doc->kind != NS_NODE_DOCUMENT || (doc->flags & NS_NODE_FRAGMENT))
        return FALSE;
    /* Attached means in the page's frame tree, whichever of its documents
       the engine is running code for (a parent's listener can run while a
       frame's document is current). */
    ns_node *top = ns_js_top_document(js->current_doc);
    return top && ns_js_top_document((ns_node *)doc) != top;
}

static JSClassID ns_element_class_id;
static JSClassID ns_attr_class_id;
static JSClassID ns_style_class_id;
static JSClassID ns_token_list_class_id;
static JSClassID ns_storage_class_id;
static JSClassID ns_live_class_id;
static JSClassID ns_dataset_class_id;
static JSClassID ns_window_named_class_id;

/* Events.  What the engine sets on an event lives in a state object of its
 * own that page scripts do not see: the attributes are getters on the event
 * interfaces' prototypes, as WebIDL has them, and isTrusted is the one own
 * property, as in other browsers.  The engine itself (its C functions and
 * hidden-source scripts, see JS_IsHostAccess) reads and writes the state
 * as if it were the event's own properties, so the code that builds and
 * dispatches events keeps using plain property access. */
static JSClassID ns_event_class_id;

typedef struct ns_event_data {
    JSValue state;
} ns_event_data;

static ns_event_data *
ns_event_data_of(JSValueConst v)
{
    return ns_event_class_id && JS_VALUE_GET_TAG(v) == JS_TAG_OBJECT
        ? JS_GetOpaque(v, ns_event_class_id) : NULL;
}

static void
ns_event_finalizer(JSRuntime *rt, JSValue val)
{
    ns_event_data *d = JS_GetOpaque(val, ns_event_class_id);
    if (!d) return;
    JS_FreeValueRT(rt, d->state);
    g_free(d);
}

static void
ns_event_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    ns_event_data *d = JS_GetOpaque(val, ns_event_class_id);
    if (d) JS_MarkValue(rt, d->state, mark_func);
}

/* Whether the engine's own definition of prop belongs on the event
 * itself: only isTrusted, which is [LegacyUnforgeable]. */
static gboolean
ns_event_keeps_own(JSContext *ctx, JSValueConst obj, JSAtom prop,
                   JSValueConst val)
{
    (void)obj; (void)val;
    const char *name = JS_AtomToCString(ctx, prop);
    gboolean is_trusted = name && strcmp(name, "isTrusted") == 0;
    if (name) JS_FreeCString(ctx, name);
    return is_trusted;
}

static int
ns_event_get_own_property(JSContext *ctx, JSPropertyDescriptor *desc,
                          JSValueConst obj, JSAtom prop)
{
    ns_event_data *d = ns_event_data_of(obj);
    if (!d) return 0;
    if (JS_IsHostAccess(ctx))
        return JS_GetOwnProperty(ctx, desc, d->state, prop);
    /* A method the engine gave an event whose interfaces lack it (such as
     * a navigation event's intercept()) stays reachable for the page. */
    JSPropertyDescriptor own;
    int has = JS_GetOwnProperty(ctx, &own, d->state, prop);
    if (has <= 0) return has;
    gboolean method = !(own.flags & JS_PROP_GETSET) &&
                      JS_IsFunction(ctx, own.value);
    if (method) {
        JSValue proto = JS_GetPrototype(ctx, obj);
        int inherited = JS_IsObject(proto)
            ? JS_HasProperty(ctx, proto, prop) : 0;
        JS_FreeValue(ctx, proto);
        method = inherited == 0;
    }
    if (method && desc) {
        *desc = own;
        return 1;
    }
    JS_FreeValue(ctx, own.value);
    JS_FreeValue(ctx, own.getter);
    JS_FreeValue(ctx, own.setter);
    return method ? 1 : 0;
}

static int
ns_event_get_own_property_names(JSContext *ctx, JSPropertyEnum **ptab,
                                uint32_t *plen, JSValueConst obj)
{
    ns_event_data *d = ns_event_data_of(obj);
    *ptab = NULL;
    *plen = 0;
    if (!d || !JS_IsHostAccess(ctx)) return 0;
    return JS_GetOwnPropertyNames(ctx, ptab, plen, d->state,
                                  JS_GPN_STRING_MASK | JS_GPN_SYMBOL_MASK);
}

static int
ns_event_delete_property(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    ns_event_data *d = ns_event_data_of(obj);
    if (!d || !JS_IsHostAccess(ctx)) return TRUE;
    return JS_DeleteProperty(ctx, d->state, prop, 0);
}

static int
ns_event_define_own_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                             JSValueConst val, JSValueConst getter,
                             JSValueConst setter, int flags)
{
    ns_event_data *d = ns_event_data_of(obj);
    if (d && JS_IsHostAccess(ctx)) {
        if (!ns_event_keeps_own(ctx, obj, prop, val))
            return JS_DefineProperty(ctx, d->state, prop, val, getter, setter,
                                     flags);
        const char *name = JS_AtomToCString(ctx, prop);
        if (name && strcmp(name, "isTrusted") == 0)
            flags = (flags & ~JS_PROP_CONFIGURABLE) | JS_PROP_HAS_CONFIGURABLE;
        if (name) JS_FreeCString(ctx, name);
    }
    return JS_DefineProperty(ctx, obj, prop, val, getter, setter,
                             flags | JS_PROP_NO_EXOTIC);
}

static int
ns_event_set_state_property(JSContext *ctx, ns_event_data *d, JSValueConst obj,
                            JSAtom prop, JSValueConst value, int flags)
{
    /* An accessor the engine put in the state runs on the event. */
    JSPropertyDescriptor desc;
    int has = JS_GetOwnProperty(ctx, &desc, d->state, prop);
    if (has < 0) return -1;
    if (has) {
        JS_FreeValue(ctx, desc.value);
        if (desc.flags & JS_PROP_GETSET) {
            int ret = TRUE;
            if (JS_IsFunction(ctx, desc.setter)) {
                JSValue r = JS_Call(ctx, desc.setter, obj, 1, &value);
                ret = JS_IsException(r) ? -1 : TRUE;
                JS_FreeValue(ctx, r);
            }
            JS_FreeValue(ctx, desc.getter);
            JS_FreeValue(ctx, desc.setter);
            return ret;
        }
        JS_FreeValue(ctx, desc.getter);
        JS_FreeValue(ctx, desc.setter);
    }
    return JS_SetPropertyReceiver(ctx, d->state, prop,
                                  JS_DupValue(ctx, value), d->state, flags);
}

static int
ns_event_set_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                      JSValueConst value, JSValueConst receiver, int flags)
{
    ns_event_data *d = ns_event_data_of(obj);
    if (d && JS_IsHostAccess(ctx) &&
        JS_VALUE_GET_PTR(receiver) == JS_VALUE_GET_PTR(obj) &&
        !ns_event_keeps_own(ctx, obj, prop, value))
        return ns_event_set_state_property(ctx, d, obj, prop, value, flags);
    /* The page's assignment: an ordinary [[Set]], which finds the
     * interface's getter on the prototype or adds an own property. */
    JSValue proto = JS_GetPrototype(ctx, obj);
    int ret;
    if (JS_IsObject(proto))
        ret = JS_SetPropertyReceiver(ctx, proto, prop, JS_DupValue(ctx, value),
                                     receiver, flags);
    else
        ret = JS_DefineProperty(ctx, receiver, prop, value, JS_UNDEFINED,
                                JS_UNDEFINED, JS_PROP_C_W_E |
                                JS_PROP_HAS_VALUE | JS_PROP_HAS_WRITABLE |
                                JS_PROP_HAS_ENUMERABLE |
                                JS_PROP_HAS_CONFIGURABLE);
    JS_FreeValue(ctx, proto);
    return ret;
}

static JSClassExoticMethods ns_event_exotic = {
    .get_own_property = ns_event_get_own_property,
    .get_own_property_names = ns_event_get_own_property_names,
    .delete_property = ns_event_delete_property,
    .define_own_property = ns_event_define_own_property,
    .set_property = ns_event_set_property,
};

static JSClassDef ns_event_class = {
    .class_name = "Event",
    .finalizer = ns_event_finalizer,
    .gc_mark = ns_event_gc_mark,
    .exotic = &ns_event_exotic,
};

/* A new event object of ctx's realm, with proto as its prototype or
 * Object.prototype, which the event's interface replaces later. */
JSValue
ns_event_new_proto(JSContext *ctx, JSValueConst proto)
{
    ns_new_class_id(&ns_event_class_id);
    JSRuntime *rt = JS_GetRuntime(ctx);
    if (!JS_IsRegisteredClass(rt, ns_event_class_id))
        JS_NewClass(rt, ns_event_class_id, &ns_event_class);
    JSValue object_proto = JS_UNDEFINED;
    if (!JS_IsObject(proto)) {
        JSValue plain = JS_NewObject(ctx);
        object_proto = JS_GetPrototype(ctx, plain);
        JS_FreeValue(ctx, plain);
    }
    JSValue ev = JS_NewObjectProtoClass(ctx,
        JS_IsObject(proto) ? proto : object_proto, ns_event_class_id);
    JS_FreeValue(ctx, object_proto);
    if (JS_IsException(ev)) return ev;
    ns_event_data *d = g_new0(ns_event_data, 1);
    d->state = JS_NewObjectProto(ctx, JS_NULL);
    JS_SetOpaque(ev, d);
    return ev;
}

/* Whether objects with prototype proto are events of ctx's realm. */
static gboolean
ns_proto_is_event(JSContext *ctx, JSValueConst proto)
{
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue ctor = JS_GetPropertyStr(ctx, global, "Event");
    JSValue event_proto = JS_IsObject(ctor)
        ? JS_GetPropertyStr(ctx, ctor, "prototype") : JS_UNDEFINED;
    JS_FreeValue(ctx, ctor);
    JS_FreeValue(ctx, global);
    gboolean is_event = FALSE;
    JSValue p = JS_DupValue(ctx, proto);
    for (int depth = 0; JS_IsObject(p) && depth < 32 && !is_event; depth++) {
        if (JS_VALUE_GET_PTR(p) == JS_VALUE_GET_PTR(event_proto)) {
            is_event = TRUE;
            break;
        }
        JSValue next = JS_GetPrototype(ctx, p);
        JS_FreeValue(ctx, p);
        p = next;
    }
    JS_FreeValue(ctx, p);
    JS_FreeValue(ctx, event_proto);
    return is_event;
}

JSValue
ns_event_new(JSContext *ctx)
{
    return ns_event_new_proto(ctx, JS_UNDEFINED);
}

JSValue
ns_event_state(JSValueConst v)
{
    ns_event_data *d = ns_event_data_of(v);
    return d ? JS_DupValue(NULL, d->state) : JS_UNDEFINED;
}


static ns_node *ns_unwrap_element_mut(JSValueConst val);
static void ns_tag_caller_document(JSContext *ctx, JSValueConst node_val);

typedef struct {
    JSValue element;
} ns_style_back;

static void
ns_style_finalizer(JSRuntime *rt, JSValue val)
{
    ns_style_back *b = JS_GetOpaque(val, ns_style_class_id);
    if (!b) return;
    JS_FreeValueRT(rt, b->element);
    g_free(b);
}

static void
ns_style_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    ns_style_back *b = JS_GetOpaque(val, ns_style_class_id);
    if (b) JS_MarkValue(rt, b->element, mark_func);
}

ns_node *
ns_style_decl_node(JSValueConst this_val)
{
    ns_style_back *b = JS_GetOpaque(this_val, ns_style_class_id);
    return b ? ns_unwrap_element_mut(b->element) : NULL;
}

JSValue
ns_style_decl_proto(JSContext *ctx)
{
    return JS_GetClassProto(ctx, ns_style_class_id);
}

static int
ns_style_get_own_property(JSContext *ctx, JSPropertyDescriptor *desc,
                          JSValueConst obj, JSAtom prop)
{
    ns_node *n = ns_style_decl_node(obj);
    if (!n) return 0;
    JSValue key = JS_AtomToValue(ctx, prop);
    gboolean is_sym = JS_IsSymbol(key);
    JS_FreeValue(ctx, key);
    if (is_sym) return 0;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 0;
    gboolean writable = FALSE;
    char *value = ns_cssom_style_own_value(n, name, &writable);
    JS_FreeCString(ctx, name);
    if (!value) return 0;
    if (desc) {
        desc->flags = JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE |
                      (writable ? JS_PROP_WRITABLE : 0);
        desc->value = JS_NewString(ctx, value);
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    }
    g_free(value);
    return 1;
}

static int
ns_style_set_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                      JSValueConst val, JSValueConst receiver, int flags)
{
    (void)receiver; (void)flags;
    ns_node *n = ns_style_decl_node(obj);
    if (!n) return FALSE;
    ns_js *js = js_from_ctx(ctx);
    if (js && js->pinned_wrappers_set &&
        !g_hash_table_contains(js->pinned_wrappers_set, n))
        return TRUE;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return FALSE;
    ns_cssom_style_set(ctx, n, name, val);
    JS_FreeCString(ctx, name);
    return TRUE;
}

static JSClassExoticMethods ns_style_exotic = {
    .get_own_property = ns_style_get_own_property,
    .set_property     = ns_style_set_property,
};

static JSClassDef ns_style_class = {
    .class_name = "CSSStyleDeclaration",
    .finalizer  = ns_style_finalizer,
    .gc_mark    = ns_style_gc_mark,
    .exotic     = &ns_style_exotic,
};

typedef struct {
    JSValue     element;
    const char *attr;
} ns_token_list_back;

static void
ns_token_list_finalizer(JSRuntime *rt, JSValue val)
{
    ns_token_list_back *b = JS_GetOpaque(val, ns_token_list_class_id);
    if (!b) return;
    JS_FreeValueRT(rt, b->element);
    g_free(b);
}

static void
ns_token_list_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    ns_token_list_back *b = JS_GetOpaque(val, ns_token_list_class_id);
    if (b) JS_MarkValue(rt, b->element, mark_func);
}

static int
ns_tlist_get_own(JSContext *ctx, JSPropertyDescriptor *desc,
                 JSValueConst obj, JSAtom prop)
{
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 0;
    char *token = ns_tlist_named_token(obj, name);
    JS_FreeCString(ctx, name);
    if (!token) return 0;
    if (desc) {
        desc->flags  = JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE;
        desc->value  = JS_NewString(ctx, token);
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    }
    g_free(token);
    return 1;
}

static JSClassExoticMethods ns_token_list_exotic = {
    .get_own_property = ns_tlist_get_own,
};

static JSClassDef ns_token_list_class = {
    .class_name = "DOMTokenList",
    .finalizer  = ns_token_list_finalizer,
    .gc_mark    = ns_token_list_gc_mark,
    .exotic     = &ns_token_list_exotic,
};

ns_node *
ns_token_list_node(JSValueConst this_val, const char **out_attr)
{
    ns_token_list_back *b = JS_GetOpaque(this_val, ns_token_list_class_id);
    if (out_attr) *out_attr = b ? b->attr : "class";
    return b ? ns_unwrap_element_mut(b->element) : NULL;
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

static void
ns_storage_load_for(ns_js *js, const char *new_url)
{
    if (!js) return;
    ns_partition_apply(js, new_url);
    ns_storage_load_local(js, new_url);
}

static void
ns_storage_finalizer(JSRuntime *rt, JSValue val) { (void)rt; (void)val; }

int
ns_storage_area_of(JSValueConst obj)
{
    return (int)(gintptr)JS_GetOpaque(obj, ns_storage_class_id);
}

JSValue
ns_storage_new(JSContext *ctx, int area)
{
    JSValue obj = JS_NewObjectClass(ctx, ns_storage_class_id);
    if (!JS_IsException(obj)) JS_SetOpaque(obj, (void *)(gintptr)area);
    return obj;
}

static int
ns_storage_get_own(JSContext *ctx, JSPropertyDescriptor *desc,
                   JSValueConst obj, JSAtom prop)
{
    if (!ns_storage_area_of(obj)) return 0;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 0;
    char *val = ns_storage_named_value(ctx, obj, name);
    JS_FreeCString(ctx, name);
    if (!val) return 0;
    if (desc) {
        desc->flags  = JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE | JS_PROP_WRITABLE;
        desc->value  = JS_NewString(ctx, val);
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    }
    g_free(val);
    return 1;
}

static int
ns_storage_set_prop(JSContext *ctx, JSValueConst obj, JSAtom prop,
                    JSValueConst val, JSValueConst receiver, int flags)
{
    (void)receiver; (void)flags;
    if (!ns_storage_area_of(obj)) return FALSE;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return FALSE;
    int ret = ns_storage_named_set(ctx, obj, name, val);
    JS_FreeCString(ctx, name);
    return ret;
}

static int
ns_storage_delete(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    if (!ns_storage_area_of(obj)) return FALSE;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return FALSE;
    ns_storage_named_delete(ctx, obj, name);
    JS_FreeCString(ctx, name);
    return TRUE;
}

static int
ns_storage_define_own(JSContext *ctx, JSValueConst this_obj, JSAtom prop,
                      JSValueConst val, JSValueConst getter,
                      JSValueConst setter, int flags)
{
    JSValue key = JS_AtomToValue(ctx, prop);
    gboolean is_symbol = JS_IsSymbol(key);
    JS_FreeValue(ctx, key);
    if (!ns_storage_area_of(this_obj) || is_symbol || JS_IsObject(getter) ||
        JS_IsObject(setter) || JS_IsUndefined(val))
        return JS_DefineProperty(ctx, this_obj, prop, val, getter, setter,
                                 flags | JS_PROP_NO_EXOTIC);
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return -1;
    int ret = ns_storage_named_set(ctx, this_obj, name, val);
    JS_FreeCString(ctx, name);
    return ret;
}

static int
ns_storage_get_own_names(JSContext *ctx, JSPropertyEnum **ptab, uint32_t *plen,
                         JSValueConst obj)
{
    *ptab = NULL;
    *plen = 0;
    if (!ns_storage_area_of(obj)) return 0;
    char **names = ns_storage_names(ctx, obj);
    guint count = names ? g_strv_length(names) : 0;
    if (count == 0) {
        g_strfreev(names);
        return 0;
    }
    JSPropertyEnum *tab = js_malloc(ctx, sizeof(JSPropertyEnum) * count);
    if (!tab) {
        g_strfreev(names);
        return -1;
    }
    for (guint i = 0; i < count; i++) {
        tab[i].atom = JS_NewAtom(ctx, names[i]);
        tab[i].is_enumerable = 1;
    }
    g_strfreev(names);
    *ptab = tab;
    *plen = count;
    return 0;
}

static JSClassExoticMethods ns_storage_exotic = {
    .get_own_property       = ns_storage_get_own,
    .get_own_property_names = ns_storage_get_own_names,
    .set_property           = ns_storage_set_prop,
    .delete_property        = ns_storage_delete,
    .define_own_property    = ns_storage_define_own,
};

static JSClassDef ns_storage_class = {
    .class_name = "Storage",
    .finalizer  = ns_storage_finalizer,
    .exotic     = &ns_storage_exotic,
};

static const JSCFunctionListEntry ns_tlist_proto_funcs[] = {
    JS_CFUNC_DEF("contains", 1, ns_tlist_contains),
    JS_CFUNC_DEF("add",      1, ns_tlist_add),
    JS_CFUNC_DEF("remove",   1, ns_tlist_remove),
    JS_CFUNC_DEF("toggle",   1, ns_tlist_toggle),
    JS_CFUNC_DEF("replace",  2, ns_tlist_replace),
    JS_CFUNC_DEF("item",     1, ns_tlist_item),
    JS_CFUNC_DEF("supports", 1, ns_tlist_supports),
    JS_CFUNC_DEF("toString", 0, ns_tlist_toString),
    JS_CGETSET_DEF("length", ns_tlist_get_length, NULL),
    JS_CGETSET_DEF("value",  ns_tlist_get_value, ns_tlist_set_value),
};

static void
ns_element_finalizer(JSRuntime *rt, JSValue val)
{
    (void)rt;
    ns_node *n = JS_GetOpaque(val, ns_element_class_id);
    if (n) {
        n->js_wrapper = NULL;
        n->js_invalidate = NULL;
    }
}

static int ns_element_named_get_own(JSContext *ctx, JSPropertyDescriptor *desc,
                                    JSValueConst obj, JSAtom prop);
static int ns_element_delete_property(JSContext *ctx, JSValueConst obj, JSAtom prop);
static int ns_element_define_own_property(JSContext *ctx, JSValueConst this_obj, JSAtom prop,
                                          JSValueConst val, JSValueConst getter,
                                          JSValueConst setter, int flags);

static JSClassExoticMethods ns_element_exotic = {
    .get_own_property    = ns_element_named_get_own,
    .delete_property     = ns_element_delete_property,
    .define_own_property = ns_element_define_own_property,
};

static JSClassDef ns_element_class = {
    .class_name = "Element",
    .finalizer  = ns_element_finalizer,
    .exotic     = &ns_element_exotic,
};

static void
ns_js_forget_pending_change(ns_js *js)
{
    js->change_pending = NULL;
    g_clear_pointer(&js->change_baseline, g_free);
}

static void
ns_invalidate_wrapper(ns_node *n)
{
    if (!n) return;
    ns_js *js = ns_active_js();
    ns_mut_scrub_node(js, n);
    ns_attr_detach_owner(js, n);
    ns_attribute_map_release_owner(js, n);
    if (n->js_wrapper) {
        JSValue obj = JS_MKPTR(JS_TAG_OBJECT, n->js_wrapper);
        JS_SetOpaque(obj, NULL);
        void *ptr = n->js_wrapper;
        n->js_wrapper = NULL;
        if (js && js->ctx && js->pinned_wrappers_set &&
            g_hash_table_remove(js->pinned_wrappers_set, n)) {
            JSValue pinned = JS_MKPTR(JS_TAG_OBJECT, ptr);
            JS_FreeValue(js->ctx, pinned);
        }
    }
    if (js && js->orphan_nodes)
        g_hash_table_remove(js->orphan_nodes, n);
    if (js && js->js_image_loads)
        g_hash_table_remove(js->js_image_loads, n);
    if (js) ns_focus_forget_node(js, n);
    if (js && js->change_pending == n) ns_js_forget_pending_change(js);
    if (js) ns_parser_hold_forget(js, n);
    if (js) ns_js_frames_forget_node(js, n);
    if (js) ns_top_layer_forget_node(js, n);
    n->js_invalidate = NULL;

    if (js) ns_dispatch_forget_node(js, n);
}

void
ns_node_arm_js_invalidate(ns_node *n)
{
    if (n && !n->js_invalidate) n->js_invalidate = ns_invalidate_wrapper;
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
ns_foreign_kind_proto(ns_js *js, const ns_node *node)
{
    const char *ns = ns_element_get_attr(node, "data-nd-ns-uri");
    gboolean mathml = ns && strcmp(ns, "http://www.w3.org/1998/Math/MathML") == 0;
    return mathml && JS_IsObject(js->proto_mathmlelement)
        ? js->proto_mathmlelement : js->proto_element;
}

static JSValue
ns_html_kind_proto(ns_js *js, const ns_node *node)
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

static JSValue
ns_node_kind_proto(ns_js *js, const ns_node *node)
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

static const char *const ns_body_window_reflected_handlers[] = {
    "onafterprint", "onbeforeprint", "onbeforeunload", "onblur", "onerror",
    "onfocus", "onhashchange", "onlanguagechange", "onload", "onmessage",
    "onmessageerror", "onoffline", "ononline", "onpagehide", "onpageshow",
    "onpopstate", "onrejectionhandled", "onresize", "onscroll", "onstorage",
    "onunhandledrejection", "onunload",
};

static gboolean
ns_body_has_browsing_context(JSContext *ctx, JSValueConst this_val)
{
    JSValue owner = JS_GetPropertyStr(ctx, this_val, "__ndOwnerDoc");
    gboolean foreign = JS_IsObject(owner);
    JS_FreeValue(ctx, owner);
    return !foreign;
}

static JSValue
ns_body_reflected_get(JSContext *ctx, JSValueConst this_val, int argc,
                      JSValueConst *argv, int magic, JSValueConst *func_data)
{
    (void)argc; (void)argv; (void)magic;
    if (!ns_body_has_browsing_context(ctx, this_val)) return JS_NULL;
    const char *name = JS_ToCString(ctx, func_data[0]);
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue v = name ? JS_GetPropertyStr(ctx, global, name) : JS_NULL;
    JS_FreeValue(ctx, global);
    if (name) JS_FreeCString(ctx, name);
    return v;
}

static JSValue
ns_body_reflected_set(JSContext *ctx, JSValueConst this_val, int argc,
                      JSValueConst *argv, int magic, JSValueConst *func_data)
{
    (void)magic;
    if (!ns_body_has_browsing_context(ctx, this_val)) return JS_UNDEFINED;
    const char *name = JS_ToCString(ctx, func_data[0]);
    JSValue global = JS_GetGlobalObject(ctx);
    if (name) {
        JSValue v = (argc > 0 && JS_IsFunction(ctx, argv[0]))
                    ? JS_DupValue(ctx, argv[0]) : JS_NULL;
        JS_SetPropertyStr(ctx, global, name, v);
    }
    JS_FreeValue(ctx, global);
    if (name) JS_FreeCString(ctx, name);
    return JS_UNDEFINED;
}

static gboolean
ns_name_is_body_reflected_handler(const char *name)
{
    if (!name) return FALSE;
    for (gsize i = 0; i < G_N_ELEMENTS(ns_body_window_reflected_handlers); i++)
        if (g_ascii_strcasecmp(name, ns_body_window_reflected_handlers[i]) == 0)
            return TRUE;
    return FALSE;
}

static JSValue
ns_compile_inline_handler(JSContext *ctx, const char *body)
{
    GString *src = g_string_new("(function(event){\n");
    g_string_append(src, body);
    g_string_append(src, "\n})");
    JSValue fn = JS_Eval(ctx, src->str, src->len, "<inline>",
                         JS_EVAL_TYPE_GLOBAL);
    g_string_free(src, TRUE);
    return fn;
}

static void
ns_body_forward_content_handler(JSContext *ctx, const ns_node *n,
                                const char *name, const char *code)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return;
    if (strcmp(n->name, "body") != 0 && strcmp(n->name, "frameset") != 0) return;
    if (!ns_name_is_body_reflected_handler(name)) return;
    JSValue fn = JS_NULL;
    if (code && *code) {
        JSValue c = ns_compile_inline_handler(ctx, code);
        if (JS_IsException(c)) { JS_FreeValue(ctx, JS_GetException(ctx)); }
        else fn = c;
    }
    JSValue global = JS_GetGlobalObject(ctx);
    JS_SetPropertyStr(ctx, global, name, fn);
    JS_FreeValue(ctx, global);
}

static void
ns_install_body_reflected_handlers(JSContext *ctx, JSValueConst wrapper)
{
    for (gsize i = 0; i < G_N_ELEMENTS(ns_body_window_reflected_handlers); i++) {
        const char *name = ns_body_window_reflected_handlers[i];
        JSValue data = JS_NewString(ctx, name);
        JSValue getter = JS_NewCFunctionData(ctx, ns_body_reflected_get,
                                             0, 0, 1, &data);
        JSValue setter = JS_NewCFunctionData(ctx, ns_body_reflected_set,
                                             1, 0, 1, &data);
        JSAtom atom = JS_NewAtom(ctx, name);
        JS_DefinePropertyGetSet(ctx, wrapper, atom, getter, setter,
                                JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
        JS_FreeAtom(ctx, atom);
        JS_FreeValue(ctx, data);
    }
}

/* The page's own document object, the main realm's document: every realm
 * sees that one object for the page's document node. */
static JSValue
ns_page_document_object(ns_js *js)
{
    JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    JSValue global = JS_GetGlobalObject(main_ctx);
    JSValue doc = JS_GetPropertyStr(main_ctx, global, "document");
    JS_FreeValue(main_ctx, global);
    return doc;
}

JSValue
ns_make_element(JSContext *ctx, const ns_node *cnode)
{
    if (!cnode) return JS_NULL;
    ns_js *js = js_from_ctx(ctx);
    if (js && cnode == js->ce_main_doc && !cnode->parent) {
        JSValue doc = ns_page_document_object(js);
        if (!JS_IsUndefined(doc) && !JS_IsNull(doc))
            return doc;
        JS_FreeValue(ctx, doc);
    }
    ns_node *node = (ns_node *)cnode;
    if (node->js_wrapper) {
        JSValue cached = JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper);
        return JS_DupValue(ctx, cached);
    }
    JSValue obj = JS_NewObjectClass(ctx, ns_element_class_id);
    if (JS_IsException(obj)) return obj;
    JS_SetOpaque(obj, node);
    node->js_wrapper = JS_VALUE_GET_PTR(obj);
    node->js_invalidate = ns_invalidate_wrapper;
    JSValue kind_proto = ns_node_kind_proto(js, node);
    JSContext *node_realm = NULL;
    if (js && ns_realm_cloners_made(js)) {
        node_realm = ns_js_node_realm_context(js, node);
        if (!node_realm) {
            const ns_node *root = node;
            while (root->parent) root = root->parent;
            if (root->kind != NS_NODE_DOCUMENT || (root->flags & NS_NODE_FRAGMENT))
                node_realm = ctx;
        }
    }
    if (node_realm && JS_IsObject(kind_proto))
        kind_proto = ns_realm_proto_for(js, node_realm, kind_proto);
    if (JS_IsObject(kind_proto)) JS_SetPrototype(ctx, obj, kind_proto);
    if (node->kind == NS_NODE_DOCTYPE) {
        const char *pub = "", *sys = "";
        for (const ns_attr *a = node->attrs; a; a = a->next) {
            if (!a->name) continue;
            if (strcmp(a->name, "publicId") == 0) pub = a->value ? a->value : "";
            else if (strcmp(a->name, "systemId") == 0) sys = a->value ? a->value : "";
        }
        JS_DefinePropertyValueStr(ctx, obj, "name",
            JS_NewString(ctx, node->name ? node->name : ""), JS_PROP_ENUMERABLE);
        JS_DefinePropertyValueStr(ctx, obj, "publicId",
            JS_NewString(ctx, pub), JS_PROP_ENUMERABLE);
        JS_DefinePropertyValueStr(ctx, obj, "systemId",
            JS_NewString(ctx, sys), JS_PROP_ENUMERABLE);
    }
    if (node->kind == NS_NODE_ELEMENT && node->name &&
        (strcmp(node->name, "body") == 0 ||
         strcmp(node->name, "frameset") == 0))
        ns_install_body_reflected_handlers(ctx, obj);
    if (js && js->pinned_wrappers_set) {
        JS_DupValue(ctx, obj);
        g_hash_table_add(js->pinned_wrappers_set, node);
    }
    return obj;
}

const ns_node *
ns_unwrap_element(JSValueConst val)
{
    return JS_GetOpaque(val, ns_element_class_id);
}


static int
ns_window_named_fill(JSContext *ctx, JSPropertyDescriptor *desc,
                     JSValueConst window, JSAtom prop)
{
    JSValue key = JS_AtomToValue(ctx, prop);
    JSValue value = ns_window_named_property(ctx, window, key);
    JS_FreeValue(ctx, key);
    if (!JS_IsObject(value)) {
        JS_FreeValue(ctx, value);
        return 0;
    }
    if (!desc) {
        JS_FreeValue(ctx, value);
        return 1;
    }
    desc->flags  = JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE;
    desc->value  = value;
    desc->getter = JS_UNDEFINED;
    desc->setter = JS_UNDEFINED;
    return 1;
}

static int
ns_window_named_get(JSContext *ctx, JSPropertyDescriptor *desc,
                    JSValueConst obj, JSAtom prop)
{
    (void)obj;
    JSValue global = JS_GetGlobalObject(ctx);
    int found = ns_window_named_fill(ctx, desc, global, prop);
    JS_FreeValue(ctx, global);
    return found;
}

static int
ns_window_named_get_receiver(JSContext *ctx, JSPropertyDescriptor *desc,
                             JSValueConst obj, JSAtom prop,
                             JSValueConst receiver)
{
    (void)obj;
    return ns_window_named_fill(ctx, desc, receiver, prop);
}

static JSClassExoticMethods ns_window_named_exotic = {
    .get_own_property          = ns_window_named_get,
    .get_own_property_receiver = ns_window_named_get_receiver,
};

static JSClassDef ns_window_named_class = {
    .class_name = "WindowProperties",
    .exotic     = &ns_window_named_exotic,
};

static void ns_js_start_image_load(ns_js *js, ns_node *el, const char *src);
static void ns_js_flush_ready_images(ns_js *js);
static void ns_js_rescan_subtree_images(ns_js *js, ns_node *root, int depth);

static ns_node *
ns_unwrap_element_mut(JSValueConst val)
{
    return JS_GetOpaque(val, ns_element_class_id);
}

ns_node *
ns_window_document_for(JSContext *ctx, JSValueConst window)
{
    ns_node *target = NULL;
    if (JS_IsObject(window)) {
        JSValue doc = JS_GetPropertyStr(ctx, window, "document");
        target = ns_unwrap_element_mut(doc);
        if (JS_IsException(doc)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, doc);
        if (target && target->kind == NS_NODE_DOCUMENT) return target;
    }
    ns_js *js = js_from_ctx(ctx);
    if (js && js->iframe_doc_set) {
        target = ns_unwrap_element_mut(js->iframe_doc);
        if (target && target->kind == NS_NODE_DOCUMENT) return target;
    }
    return js ? js->current_doc : NULL;
}

enum {
    NS_INSTOF_TAG = 0,
    NS_INSTOF_NODE,
    NS_INSTOF_ELEMENT,
    NS_INSTOF_DOCUMENT,
    NS_INSTOF_FRAGMENT,
    NS_INSTOF_TEXT,
    NS_INSTOF_COMMENT,
    NS_INSTOF_CHARDATA,
    NS_INSTOF_DOCTYPE,
    NS_INSTOF_HTMLCOLLECTION,
    NS_INSTOF_NODELIST,
    NS_INSTOF_HTMLELEMENT,
    NS_INSTOF_SHADOW,
};

typedef struct { const char *ctor; const char *tags; int special; } ns_instof_def;

static const ns_instof_def ns_instof_table[] = {
    { "HTMLAnchorElement",        "a",                  NS_INSTOF_TAG },
    { "HTMLAreaElement",          "area",               NS_INSTOF_TAG },
    { "HTMLAudioElement",         "audio",              NS_INSTOF_TAG },
    { "HTMLBRElement",            "br",                 NS_INSTOF_TAG },
    { "HTMLBaseElement",          "base",               NS_INSTOF_TAG },
    { "HTMLBodyElement",          "body",               NS_INSTOF_TAG },
    { "HTMLButtonElement",        "button",             NS_INSTOF_TAG },
    { "HTMLCanvasElement",        "canvas",             NS_INSTOF_TAG },
    { "HTMLDListElement",         "dl",                 NS_INSTOF_TAG },
    { "HTMLDataElement",          "data",               NS_INSTOF_TAG },
    { "HTMLDataListElement",      "datalist",           NS_INSTOF_TAG },
    { "HTMLDetailsElement",       "details",            NS_INSTOF_TAG },
    { "HTMLDialogElement",        "dialog",             NS_INSTOF_TAG },
    { "HTMLDivElement",           "div",                NS_INSTOF_TAG },
    { "HTMLEmbedElement",         "embed",              NS_INSTOF_TAG },
    { "HTMLFieldSetElement",      "fieldset",           NS_INSTOF_TAG },
    { "HTMLFormElement",          "form",               NS_INSTOF_TAG },
    { "HTMLHRElement",            "hr",                 NS_INSTOF_TAG },
    { "HTMLHeadElement",          "head",               NS_INSTOF_TAG },
    { "HTMLHeadingElement",       "h1 h2 h3 h4 h5 h6",  NS_INSTOF_TAG },
    { "HTMLHtmlElement",          "html",               NS_INSTOF_TAG },
    { "HTMLIFrameElement",        "iframe",             NS_INSTOF_TAG },
    { "HTMLDirectoryElement",     "dir",                NS_INSTOF_TAG },
    { "HTMLFontElement",          "font",               NS_INSTOF_TAG },
    { "HTMLFrameElement",         "frame",              NS_INSTOF_TAG },
    { "HTMLFrameSetElement",      "frameset",           NS_INSTOF_TAG },
    { "HTMLParamElement",         "param",              NS_INSTOF_TAG },
    { "HTMLMarqueeElement",       "marquee",            NS_INSTOF_TAG },
    { "HTMLImageElement",         "img",                NS_INSTOF_TAG },
    { "HTMLInputElement",         "input",              NS_INSTOF_TAG },
    { "HTMLLIElement",            "li",                 NS_INSTOF_TAG },
    { "HTMLLabelElement",         "label",              NS_INSTOF_TAG },
    { "HTMLLegendElement",        "legend",             NS_INSTOF_TAG },
    { "HTMLLinkElement",          "link",               NS_INSTOF_TAG },
    { "HTMLMapElement",           "map",                NS_INSTOF_TAG },
    { "HTMLMenuElement",          "menu",               NS_INSTOF_TAG },
    { "HTMLMetaElement",          "meta",               NS_INSTOF_TAG },
    { "HTMLMeterElement",         "meter",              NS_INSTOF_TAG },
    { "HTMLModElement",           "ins del",            NS_INSTOF_TAG },
    { "HTMLOListElement",         "ol",                 NS_INSTOF_TAG },
    { "HTMLObjectElement",        "object",             NS_INSTOF_TAG },
    { "HTMLOptGroupElement",      "optgroup",           NS_INSTOF_TAG },
    { "HTMLOptionElement",        "option",             NS_INSTOF_TAG },
    { "HTMLOutputElement",        "output",             NS_INSTOF_TAG },
    { "HTMLParagraphElement",     "p",                  NS_INSTOF_TAG },
    { "HTMLPictureElement",       "picture",            NS_INSTOF_TAG },
    { "HTMLPreElement",           "pre listing xmp",    NS_INSTOF_TAG },
    { "HTMLProgressElement",      "progress",           NS_INSTOF_TAG },
    { "HTMLQuoteElement",         "q blockquote",       NS_INSTOF_TAG },
    { "HTMLScriptElement",        "script",             NS_INSTOF_TAG },
    { "HTMLSelectElement",        "select",             NS_INSTOF_TAG },
    { "HTMLSlotElement",          "slot",               NS_INSTOF_TAG },
    { "HTMLSourceElement",        "source",             NS_INSTOF_TAG },
    { "HTMLSpanElement",          "span",               NS_INSTOF_TAG },
    { "HTMLStyleElement",         "style",              NS_INSTOF_TAG },
    { "HTMLTableCaptionElement",  "caption",            NS_INSTOF_TAG },
    { "HTMLTableCellElement",     "td th",              NS_INSTOF_TAG },
    { "HTMLTableColElement",      "col colgroup",       NS_INSTOF_TAG },
    { "HTMLTableElement",         "table",              NS_INSTOF_TAG },
    { "HTMLTableRowElement",      "tr",                 NS_INSTOF_TAG },
    { "HTMLTableSectionElement",  "thead tbody tfoot",  NS_INSTOF_TAG },
    { "HTMLTemplateElement",      "template",           NS_INSTOF_TAG },
    { "HTMLTextAreaElement",      "textarea",           NS_INSTOF_TAG },
    { "HTMLTimeElement",          "time",               NS_INSTOF_TAG },
    { "HTMLTitleElement",         "title",              NS_INSTOF_TAG },
    { "HTMLTrackElement",         "track",              NS_INSTOF_TAG },
    { "HTMLUListElement",         "ul",                 NS_INSTOF_TAG },
    { "HTMLVideoElement",         "video",              NS_INSTOF_TAG },
    { "Node",                     NULL,                 NS_INSTOF_NODE },
    { "Element",                  NULL,                 NS_INSTOF_ELEMENT },
    { "HTMLElement",              NULL,                 NS_INSTOF_ELEMENT },
    { "HTMLDocument",             NULL,                 NS_INSTOF_DOCUMENT },
    { "Document",                 NULL,                 NS_INSTOF_DOCUMENT },
    { "HTMLCollection",           NULL,                 NS_INSTOF_HTMLCOLLECTION },
    { "NodeList",                 NULL,                 NS_INSTOF_NODELIST },
    { "DocumentFragment",         NULL,                 NS_INSTOF_FRAGMENT },
    { "Text",                     NULL,                 NS_INSTOF_TEXT },
    { "CDATASection",             NULL,                 NS_INSTOF_TEXT },
    { "ProcessingInstruction",    NULL,                 NS_INSTOF_CHARDATA },
    { "Comment",                  NULL,                 NS_INSTOF_COMMENT },
    { "CharacterData",            NULL,                 NS_INSTOF_CHARDATA },
    { "DocumentType",             NULL,                 NS_INSTOF_DOCTYPE },
    { "HTMLElement",              NULL,                 NS_INSTOF_HTMLELEMENT },
    { "ShadowRoot",               NULL,                 NS_INSTOF_SHADOW },
};

/* NodeList and HTMLCollection are told apart by the kind of live collection. */
static JSValue
ns_instof_collection(JSContext *ctx, const ns_instof_def *d, JSValueConst v)
{
    if (d->special == NS_INSTOF_HTMLCOLLECTION)
        return JS_NewBool(ctx, ns_live_collection_kind(v) == 1);
    if (ns_live_collection_kind(v) == 0)
        return JS_TRUE;
    if (!JS_IsObject(v))
        return JS_FALSE;
    JSValue m = JS_GetPropertyStr(ctx, v, "__nsNodeList");
    int hit = JS_ToBool(ctx, m);
    JS_FreeValue(ctx, m);
    return JS_NewBool(ctx, hit == 1);
}

/* Whether tag is one of the space-separated names in list. */
static gboolean
ns_instof_list_has(const char *list, const char *tag)
{
    size_t tlen = strlen(tag);
    while (*list) {
        while (*list == ' ') list++;
        const char *tok = list;
        while (*list && *list != ' ') list++;
        if ((size_t)(list - tok) == tlen && strncmp(tok, tag, tlen) == 0)
            return TRUE;
    }
    return FALSE;
}

/* Whether node n is an element with one of the tags of the table entry. */
static gboolean
ns_instof_tag_match(const ns_instof_def *d, const ns_node *n)
{
    if (n->kind != NS_NODE_ELEMENT || !n->name || !d->tags)
        return FALSE;
    if (n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS))
        return FALSE;
    return ns_instof_list_has(d->tags, n->name);
}

static gboolean
ns_instof_is_shadow_host_attr(const ns_node *n)
{
    return n->kind == NS_NODE_ELEMENT &&
           ns_element_get_attr(n, NS_SHADOW_ATTR) != NULL;
}

/* The entries decided by the kind of the node alone. */
static gboolean
ns_instof_leaf_match(int special, const ns_node *n)
{
    switch (special) {
    case NS_INSTOF_TEXT:
        return n->kind == NS_NODE_TEXT;
    case NS_INSTOF_COMMENT:
        return n->kind == NS_NODE_COMMENT;
    case NS_INSTOF_CHARDATA:
        return n->kind == NS_NODE_TEXT || n->kind == NS_NODE_COMMENT;
    case NS_INSTOF_DOCTYPE:
        return n->kind == NS_NODE_DOCTYPE;
    default:
        return FALSE;
    }
}

/* The entries for elements, documents and document fragments. */
static gboolean
ns_instof_container_match(int special, const ns_node *n)
{
    switch (special) {
    case NS_INSTOF_ELEMENT:
        return n->kind == NS_NODE_ELEMENT;
    case NS_INSTOF_DOCUMENT:
        return n->kind == NS_NODE_DOCUMENT && !(n->flags & NS_NODE_FRAGMENT);
    case NS_INSTOF_FRAGMENT:
        return (n->flags & NS_NODE_FRAGMENT) != 0 ||
               ns_instof_is_shadow_host_attr(n);
    case NS_INSTOF_SHADOW:
        return ns_instof_is_shadow_host_attr(n);
    case NS_INSTOF_HTMLELEMENT:
        return n->kind == NS_NODE_ELEMENT &&
               !(n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS));
    default:
        return FALSE;
    }
}

/* Whether node n, the wrapper of an instance, is of the kind of the table
 * entry d. */
static gboolean
ns_instof_node_match(const ns_instof_def *d, const ns_node *n)
{
    /* A shadow root is stored as an element but is a DocumentFragment. */
    if (ns_node_is_shadow_root(n))
        return d->special == NS_INSTOF_NODE ||
               d->special == NS_INSTOF_FRAGMENT ||
               d->special == NS_INSTOF_SHADOW;
    if (d->special == NS_INSTOF_NODE)
        return TRUE;
    if (d->special == NS_INSTOF_TAG)
        return ns_instof_tag_match(d, n);
    return ns_instof_leaf_match(d->special, n) ||
           ns_instof_container_match(d->special, n);
}

/* Whether ctor is the interface object the table entry belongs to. The
 * [Symbol.hasInstance] of an interface object is inherited by the interfaces
 * that extend it, and those are not told apart by node kind: Attr extends
 * Node, HTMLUnknownElement extends HTMLElement, XMLDocument extends Document. */
static gboolean
ns_instof_is_entry_ctor(JSContext *ctx, JSValueConst ctor,
                        const ns_instof_def *d)
{
    if (!JS_IsObject(ctor))
        return FALSE;
    JSValue nv = JS_GetPropertyStr(ctx, ctor, "name");
    const char *name = JS_IsString(nv) ? JS_ToCString(ctx, nv) : NULL;
    if (JS_IsException(nv))
        JS_FreeValue(ctx, JS_GetException(ctx));
    gboolean same = name && strcmp(name, d->ctor) == 0;
    if (name) JS_FreeCString(ctx, name);
    JS_FreeValue(ctx, nv);
    return same;
}

/* OrdinaryHasInstance: whether the prototype of ctor is on the prototype
 * chain of v. */
static JSValue
ns_instof_ordinary(JSContext *ctx, JSValueConst ctor, JSValueConst v)
{
    if (!JS_IsObject(v) || !JS_IsFunction(ctx, ctor))
        return JS_FALSE;
    JSValue proto = JS_GetPropertyStr(ctx, ctor, "prototype");
    if (!JS_IsObject(proto)) {
        if (JS_IsException(proto))
            return proto;
        JS_FreeValue(ctx, proto);
        return JS_ThrowTypeError(ctx,
            "operand 'prototype' property is not an object");
    }
    JSValue cur = JS_DupValue(ctx, v);
    gboolean hit = FALSE;
    for (;;) {
        JSValue next = JS_GetPrototype(ctx, cur);
        JS_FreeValue(ctx, cur);
        if (JS_IsException(next)) {
            JS_FreeValue(ctx, proto);
            return next;
        }
        if (!JS_IsObject(next))
            break;
        hit = JS_VALUE_GET_PTR(next) == JS_VALUE_GET_PTR(proto);
        cur = next;
        if (hit) {
            JS_FreeValue(ctx, cur);
            break;
        }
    }
    JS_FreeValue(ctx, proto);
    return JS_NewBool(ctx, hit);
}

static JSValue
ns_ctor_hasInstance(JSContext *ctx, JSValueConst this_val,
                    int argc, JSValueConst *argv, int magic)
{
    if (argc < 1 || magic < 0 ||
        magic >= (int)G_N_ELEMENTS(ns_instof_table))
        return JS_FALSE;
    const ns_instof_def *d = &ns_instof_table[magic];
    /* A built-in interface that extends the entry's is told by its prototype
     * chain; a class of the page that extends one keeps matching by node
     * kind. */
    if (JS_IsEngineFunction(this_val) &&
        !ns_instof_is_entry_ctor(ctx, this_val, d))
        return ns_instof_ordinary(ctx, this_val, argv[0]);
    if (d->special == NS_INSTOF_HTMLCOLLECTION ||
        d->special == NS_INSTOF_NODELIST)
        return ns_instof_collection(ctx, d, argv[0]);
    const ns_node *n = ns_unwrap_element(argv[0]);
    if (!n)
        return ns_instof_ordinary(ctx, this_val, argv[0]);
    return JS_NewBool(ctx, ns_instof_node_match(d, n));
}

static void
ns_install_hasinstance(JSContext *ctx, JSValueConst global)
{
    JSValue sym = JS_GetPropertyStr(ctx, global, "Symbol");
    if (!JS_IsObject(sym)) { JS_FreeValue(ctx, sym); return; }
    JSValue hi = JS_GetPropertyStr(ctx, sym, "hasInstance");
    JS_FreeValue(ctx, sym);
    JSAtom hi_atom = JS_ValueToAtom(ctx, hi);
    JS_FreeValue(ctx, hi);
    for (int i = 0; i < (int)G_N_ELEMENTS(ns_instof_table); i++) {
        JSValue ctor = JS_GetPropertyStr(ctx, global, ns_instof_table[i].ctor);
        if (JS_IsObject(ctor)) {
            JSValue fn = JS_NewCFunctionMagic(ctx, ns_ctor_hasInstance,
                "[Symbol.hasInstance]", 1, JS_CFUNC_generic_magic, i);
            JS_DefinePropertyValue(ctx, ctor, hi_atom, fn, JS_PROP_CONFIGURABLE);
        }
        JS_FreeValue(ctx, ctor);
    }
    JS_FreeAtom(ctx, hi_atom);
}

static JSValue
ns_element_qualified_upper(JSContext *ctx, const ns_node *n)
{
    const char *pfx = ns_element_get_attr(n, "data-nd-ns-prefix");
    if (!pfx) return JS_UNDEFINED;
    char *q = g_strdup_printf("%s:%s", pfx, n->name);
    char *up = g_ascii_strup(q, -1);
    JSValue v = JS_NewString(ctx, up);
    g_free(q);
    g_free(up);
    return v;
}

static JSValue ns_element_get_ownerDocument(JSContext *ctx, JSValueConst this_val);

static JSValue
ns_element_get_tagName(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || !n->name) return JS_NULL;
    gboolean foreign = (n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)) != 0;
    JSValue owner_doc = ns_element_get_ownerDocument(ctx, this_val);
    gboolean doc_is_xml = FALSE;
    if (JS_IsObject(owner_doc))
        doc_is_xml = ns_doc_wrapper_is_xml(ctx, owner_doc);
    JS_FreeValue(ctx, owner_doc);
    gboolean is_xml = foreign || doc_is_xml;
    if (is_xml) {
        const char *pfx = ns_element_get_attr(n, "data-nd-ns-prefix");
        if (pfx && *pfx) {
            char *q = g_strdup_printf("%s:%s", pfx, n->name);
            JSValue v = JS_NewString(ctx, q);
            g_free(q);
            return v;
        }
        return JS_NewString(ctx, n->name);
    }
    if (n->kind == NS_NODE_ELEMENT) {
        JSValue q = ns_element_qualified_upper(ctx, n);
        if (!JS_IsUndefined(q)) return q;
    }
    char *up = g_ascii_strup(n->name, -1);
    JSValue v = JS_NewString(ctx, up);
    g_free(up);
    return v;
}

static JSValue
ns_element_get_accessKeyLabel(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    const char *key = n ? ns_element_get_attr(n, "accesskey") : NULL;
    if (!key || !g_utf8_validate(key, -1, NULL) || g_utf8_strlen(key, -1) != 1)
        return JS_NewString(ctx, "");
    return JS_NewString(ctx, key);
}

static JSValue
ns_element_get_localName(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || !n->name) return JS_NULL;
    if ((n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)) &&
        ns_element_get_attr(n, "data-nd-ns-uri")) {
        const char *colon = strchr(n->name, ':');
        if (colon) return JS_NewString(ctx, colon + 1);
    }
    return JS_NewString(ctx, n->name);
}

static JSValue
ns_element_get_prefix(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || !n->name) return JS_NULL;
    if (n->kind == NS_NODE_ELEMENT) {
        const char *stored = ns_element_get_attr(n, "data-nd-ns-prefix");
        if (stored) return JS_NewString(ctx, stored);
    }
    if ((n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)) &&
        ns_element_get_attr(n, "data-nd-ns-uri")) {
        const char *colon = strchr(n->name, ':');
        if (colon)
            return JS_NewStringLen(ctx, n->name, (size_t)(colon - n->name));
    }
    return JS_NULL;
}

static gboolean
ns_js_img_src_layout_neutral(const ns_node *n)
{
    if (!n || !n->name || strcmp(n->name, "img") != 0) return FALSE;
    const char *w = ns_element_get_attr(n, "width");
    const char *h = ns_element_get_attr(n, "height");
    return w && *w && h && *h;
}

static void
ns_js_set_attr_recorded_paint_only(ns_js *js, ns_node *n,
                                   const char *name, const char *value)
{
    if (!n || !name) return;
    const char *new_value = value ? value : "";
    const char *old = ns_element_get_attr(n, name);
    if (old && strcmp(old, new_value) == 0) return;
    char *old_copy = old ? g_strdup(old) : NULL;
    ns_element_set_attr(n, name, new_value);
    if (js) {
        ns_js_record_attr_change(js, n, name, old_copy);
        ns_ce_attr_changed(js, n, name, old_copy, new_value);
        if (js->repaint_cb) js->repaint_cb(js->repaint_user_data);
    }
    g_free(old_copy);
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

#define NS_HTML_MAXINT G_GINT64_CONSTANT(2147483647)
#define NS_HTML_MININT G_GINT64_CONSTANT(-2147483648)

enum {
    NS_ENUM_LOADING, NS_ENUM_DECODING, NS_ENUM_METHOD,
    NS_ENUM_CROSSORIGIN, NS_ENUM_REFERRERPOLICY, NS_ENUM_ENTERKEYHINT,
    NS_ENUM_ARIA_ATOMIC, NS_ENUM_ARIA_AUTOCOMPLETE, NS_ENUM_ARIA_BUSY,
    NS_ENUM_ARIA_CHECKED, NS_ENUM_ARIA_CURRENT, NS_ENUM_ARIA_DISABLED,
    NS_ENUM_ARIA_EXPANDED, NS_ENUM_ARIA_HASPOPUP, NS_ENUM_ARIA_HIDDEN,
    NS_ENUM_ARIA_INVALID, NS_ENUM_ARIA_LIVE, NS_ENUM_ARIA_MODAL,
    NS_ENUM_ARIA_MULTILINE, NS_ENUM_ARIA_MULTISELECTABLE,
    NS_ENUM_ARIA_ORIENTATION, NS_ENUM_ARIA_PRESSED, NS_ENUM_ARIA_READONLY,
    NS_ENUM_ARIA_REQUIRED, NS_ENUM_ARIA_SELECTED, NS_ENUM_ARIA_SORT,
};

static JSValue
ns_make_svg_length(JSContext *ctx, const char *str)
{
    double val = 0;
    int unit = 1;
    if (str && *str) {
        char *end = NULL;
        val = g_ascii_strtod(str, &end);
        if (end && *end) {
            while (*end == ' ') end++;
            if      (strcmp(end, "%") == 0)                 unit = 2;
            else if (g_ascii_strcasecmp(end, "em") == 0)    unit = 3;
            else if (g_ascii_strcasecmp(end, "ex") == 0)    unit = 4;
            else if (g_ascii_strcasecmp(end, "px") == 0)    unit = 5;
            else if (g_ascii_strcasecmp(end, "cm") == 0)    unit = 6;
            else if (g_ascii_strcasecmp(end, "mm") == 0)    unit = 7;
            else if (g_ascii_strcasecmp(end, "in") == 0)    unit = 8;
            else if (g_ascii_strcasecmp(end, "pt") == 0)    unit = 9;
            else if (g_ascii_strcasecmp(end, "pc") == 0)    unit = 10;
        }
    }
    JSValue o = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, o, "unitType", JS_NewInt32(ctx, unit));
    JS_SetPropertyStr(ctx, o, "value", JS_NewFloat64(ctx, val));
    JS_SetPropertyStr(ctx, o, "valueInSpecifiedUnits", JS_NewFloat64(ctx, val));
    JS_SetPropertyStr(ctx, o, "valueAsString",
                      JS_NewString(ctx, str && *str ? str : "0"));
    return o;
}

JSValue
ns_make_svg_animated_length(JSContext *ctx, const ns_node *n, const char *attr)
{
    const char *base = ns_element_get_attr(n, attr);
    if (!base) base = "0";
    char anim_attr[64];
    g_snprintf(anim_attr, sizeof anim_attr, "data-nd-anim-%s", attr);
    const char *anim = ns_element_get_attr(n, anim_attr);
    JSValue o = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, o, "baseVal", ns_make_svg_length(ctx, base));
    JS_SetPropertyStr(ctx, o, "animVal",
                      ns_make_svg_length(ctx, anim && *anim ? anim : base));
    return o;
}

static JSValue
ns_element_set_type(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    JSValue r = ns_element_reflect_str_set(ctx, this_val, val, "type");
    ns_node *el = ns_unwrap_element_mut(this_val);
    if (el) ns_input_resanitize_value(el);
    return r;
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

JSValue
ns_element_async_method(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 1 || !JS_IsFunction(ctx, argv[0])) return JS_NewInt32(ctx, 0);
    JSValue bind = JS_GetPropertyStr(ctx, argv[0], "bind");
    JSValue cb = JS_UNDEFINED;
    if (JS_IsFunction(ctx, bind)) {
        JSValueConst args[1] = { this_val };
        cb = JS_Call(ctx, bind, argv[0], 1, args);
    }
    JS_FreeValue(ctx, bind);
    if (JS_IsException(cb)) return cb;
    if (!JS_IsFunction(ctx, cb)) {
        JS_FreeValue(ctx, cb);
        cb = JS_DupValue(ctx, argv[0]);
    }
    JSValue delay = argc > 1 ? JS_DupValue(ctx, argv[1]) : JS_NewInt32(ctx, 0);
    JSValueConst args[2] = { cb, delay };
    JSValue ret = ns_services_set_timeout(ctx, JS_UNDEFINED, 2, args);
    JS_FreeValue(ctx, delay);
    JS_FreeValue(ctx, cb);
    return ret;
}

static JSValue
ns_element_cancelAsync(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    return ns_services_clear_timer(ctx, this_val, argc, argv);
}

static JSValue ns_element_get_select_length(JSContext *ctx, JSValueConst this_val);
static JSValue ns_element_get_form_elements(JSContext *ctx, JSValueConst this_val);
static JSValue ns_form_controls_snapshot(JSContext *ctx, JSValueConst form);

static glong
ns_utf16_length(const char *s)
{
    glong units = 0;
    for (const char *p = s; *p; p = g_utf8_next_char(p))
        units += (g_utf8_get_char(p) > 0xFFFF) ? 2 : 1;
    return units;
}

static JSValue
ns_element_get_text_length(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_NewInt32(ctx, 0);
    if (n->kind == NS_NODE_TEXT || n->kind == NS_NODE_COMMENT)
        return JS_NewInt32(ctx, n->text ? (int)ns_utf16_length(n->text) : 0);
    if (n->name && g_ascii_strcasecmp(n->name, "select") == 0)
        return ns_element_get_select_length(ctx, this_val);
    if (n->name && g_ascii_strcasecmp(n->name, "form") == 0) {
        JSValue arr = ns_form_controls_snapshot(ctx, this_val);
        JSValue len_v = JS_GetPropertyStr(ctx, arr, "length");
        JS_FreeValue(ctx, arr);
        return len_v;
    }
    return JS_UNDEFINED;
}

static JSValue
ns_element_get_nodeValue(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_NULL;
    if (n->kind == NS_NODE_TEXT || n->kind == NS_NODE_COMMENT)
        return JS_NewStringLen(ctx, n->text ? n->text : "", (size_t)n->text_len);
    return JS_NULL;
}

static JSValue
ns_text_replaceWholeText(JSContext *ctx, JSValueConst this_val,
                         int argc, JSValueConst *argv)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n || n->kind != NS_NODE_TEXT) return JS_NULL;
    size_t clen = 0;
    const char *content = argc > 0 && !JS_IsNull(argv[0]) && !JS_IsUndefined(argv[0])
        ? JS_ToCStringLen(ctx, &clen, argv[0]) : NULL;
    gboolean empty = !content || clen == 0;
    ns_node *parent = n->parent;
    ns_node *start = n;
    while (start->prev_sibling && start->prev_sibling->kind == NS_NODE_TEXT)
        start = start->prev_sibling;
    ns_js *js = js_from_ctx(ctx);
    ns_node *result = NULL;
    ns_node *c = start;
    while (c && c->kind == NS_NODE_TEXT) {
        ns_node *next = c->next_sibling;
        if (!empty && c == n) {
            char *old_copy = c->text ? g_memdup2(c->text, c->text_len + 1) : g_strdup("");
            ns_node_replace_text_len_owned(c, g_memdup2(content, clen + 1), (guint32)clen);
            if (js) ns_js_record_character_data(js, c, old_copy);
            g_free(old_copy);
            result = c;
        } else {
            ns_node_remove(c);
            ns_js_orphan_node(js, c);
        }
        c = next;
    }
    if (js) {
        js->mutated = TRUE;
        ns_qcache_invalidate(js);
        if (parent) ns_css_mark_restyle_dirty(parent);
    }
    if (content) JS_FreeCString(ctx, content);
    return result ? ns_make_element(ctx, result) : JS_NULL;
}

JSValue
ns_element_set_nodeValue(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n) return JS_UNDEFINED;
    if (n->kind != NS_NODE_TEXT && n->kind != NS_NODE_COMMENT) return JS_UNDEFINED;
    gboolean is_null = JS_IsNull(val);
    size_t len = 0;
    const char *s = is_null ? "" : JS_ToCStringLen(ctx, &len, val);
    if (s) {
        char *old_copy = n->text ? g_memdup2(n->text, n->text_len + 1) : g_strdup("");
        ns_node_replace_text_len_owned(n, is_null ? g_strdup("") : g_memdup2(s, len + 1), (guint32)len);
        if (!is_null) JS_FreeCString(ctx, s);
        ns_js *_j = js_from_ctx(ctx);
        if (_j) {
            _j->mutated = TRUE;
            ns_js_record_character_data(_j, n, old_copy);
        }
        g_free(old_copy);
    }
    return JS_UNDEFINED;
}

static gboolean
ns_node_is_object_element(const ns_node *n)
{
    return n && n->kind == NS_NODE_ELEMENT && n->name &&
           g_ascii_strcasecmp(n->name, "object") == 0;
}

static JSValue
ns_element_get_data(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (ns_node_is_object_element(n)) {
        const char *v = ns_element_get_attr(n, "data");
        if (!v) return JS_NewString(ctx, "");
        g_autofree char *base = ns_js_doc_base_url(js_from_ctx(ctx));
        if (base && *base) {
            char *resolved = ns_url_resolve(base, v);
            if (resolved) {
                JSValue ret = JS_NewString(ctx, resolved);
                g_free(resolved);
                return ret;
            }
        }
        return JS_NewString(ctx, v);
    }
    return ns_element_get_nodeValue(ctx, this_val);
}

static JSValue
ns_element_set_data(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (ns_node_is_object_element(n)) {
        const char *s = JS_ToCString(ctx, val);
        if (s) {
            ns_js *js = js_from_ctx(ctx);
            const char *old = ns_element_get_attr(n, "data");
            gboolean changed = !old || strcmp(old, s) != 0;
            ns_js_set_attr_recorded(js, n, "data", s);
            if (changed) ns_js_schedule_iframe_load(js, n);
            JS_FreeCString(ctx, s);
        }
        return JS_UNDEFINED;
    }
    if (n && n->kind == NS_NODE_ELEMENT) {
        JS_DefinePropertyValueStr(ctx, this_val, "data",
                                  JS_DupValue(ctx, val), JS_PROP_C_W_E);
        return JS_UNDEFINED;
    }
    return ns_element_set_nodeValue(ctx, this_val, val);
}

static JSValue
ns_element_get_style(JSContext *ctx, JSValueConst this_val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n) return JS_NULL;
    JSValue cached = JS_GetPropertyStr(ctx, this_val, "__nsStyleDecl");
    if (JS_GetOpaque(cached, ns_style_class_id)) return cached;
    JS_FreeValue(ctx, cached);
    JSValue obj = JS_NewObjectClass(ctx, ns_style_class_id);
    if (JS_IsException(obj)) return obj;
    ns_style_back *b = g_new0(ns_style_back, 1);
    b->element = JS_DupValue(ctx, this_val);
    JS_SetOpaque(obj, b);
    JS_DefinePropertyValueStr(ctx, this_val, "__nsStyleDecl",
                              JS_DupValue(ctx, obj), 0);
    return obj;
}

static JSValue
ns_element_put_forwards_attr(JSContext *ctx, JSValueConst this_val,
                             JSValueConst val, const char *attr)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n) return JS_UNDEFINED;
    const char *s = JS_ToCString(ctx, val);
    if (s) {
        ns_js_set_attr_recorded(js_from_ctx(ctx), n, attr, s);
        JS_FreeCString(ctx, s);
    }
    return JS_UNDEFINED;
}

static JSValue
ns_element_set_style(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    return ns_element_put_forwards_attr(ctx, this_val, val, "style");
}

static JSValue
ns_element_set_classList(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    return ns_element_put_forwards_attr(ctx, this_val, val, "class");
}

static JSValue
ns_element_set_relList(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    return ns_element_put_forwards_attr(ctx, this_val, val, "rel");
}

JSValue
ns_make_token_list(JSContext *ctx, JSValueConst element, const char *attr)
{
    if (!ns_unwrap_element_mut(element)) return JS_NULL;
    char key[64];
    g_snprintf(key, sizeof key, "__nsTokenList_%s", attr);
    JSValue cached = JS_GetPropertyStr(ctx, element, key);
    if (JS_GetOpaque(cached, ns_token_list_class_id)) return cached;
    JS_FreeValue(ctx, cached);
    JSValue obj = JS_NewObjectClass(ctx, ns_token_list_class_id);
    if (JS_IsException(obj)) return obj;
    ns_token_list_back *b = g_new0(ns_token_list_back, 1);
    b->element = JS_DupValue(ctx, element);
    b->attr = attr;
    JS_SetOpaque(obj, b);
    JS_DefinePropertyValueStr(ctx, element, key, JS_DupValue(ctx, obj), 0);
    return obj;
}

static JSValue
ns_element_get_classList(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_token_list(ctx, this_val, "class");
}

static JSValue
ns_element_get_relList(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_UNDEFINED;
    gboolean html_ns = !(n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS));
    gboolean ok = (html_ns && (ns_node_is_element_named(n, "a")
                               || ns_node_is_element_named(n, "area")
                               || ns_node_is_element_named(n, "form")
                               || ns_node_is_element_named(n, "link")))
               || ((n->flags & NS_NODE_SVG_NS)
                   && n->name && strcmp(n->name, "a") == 0);
    if (!ok) return JS_UNDEFINED;
    return ns_make_token_list(ctx, this_val, "rel");
}

static JSValue
ns_element_get_sandbox_list(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || (n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS))
        || !ns_node_is_element_named(n, "iframe"))
        return JS_UNDEFINED;
    return ns_make_token_list(ctx, this_val, "sandbox");
}

static JSValue
ns_element_get_sizes_list(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || (n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS)))
        return JS_UNDEFINED;
    if (ns_node_is_element_named(n, "link"))
        return ns_make_token_list(ctx, this_val, "sizes");
    if (ns_node_is_element_named(n, "img")
        || ns_node_is_element_named(n, "source"))
        return ns_element_reflect_str_get(ctx, this_val, "sizes", FALSE);
    return JS_UNDEFINED;
}

static void
ns_element_clear_children(ns_node *n)
{
    ns_node *doc = (ns_node *)ns_node_root(n);
    ns_node *c = n->first_child;
    while (c) {
        ns_node *next = c->next_sibling;
        ns_node_remove(c);
        if (doc && doc != c) {
            ns_doc_id_index_subtree_removed(doc, c);
            ns_doc_class_index_subtree_removed(doc, c);
            ns_doc_tag_index_subtree_removed(doc, c);
        }
        ns_node_free(c);
        c = next;
    }
    n->first_child = NULL;
    n->last_child  = NULL;
}

void
ns_js_orphan_node(ns_js *js, ns_node *n)
{
    if (js && js->orphan_nodes)
        g_hash_table_add(js->orphan_nodes, n);
    else {
        if (js) ns_js_purge_subtree_rafs(js, n);
        ns_node_free(n);
    }
}

void
ns_js_clear_children(ns_js *js, ns_node *n)
{
    if (!js) {
        ns_element_clear_children(n);
        return;
    }
    ns_node *c = n->first_child;
    while (c) {
        ns_node *next = c->next_sibling;
        ns_node_remove(c);
        ns_js_index_child_change(js, n, NULL, c);
        g_hash_table_add(js->orphan_nodes, c);
        c = next;
    }
    n->first_child = NULL;
    n->last_child  = NULL;
}

static void
ns_js_orphan_prune_rec(ns_js *js, ns_node *n, int depth)
{
    if (!n || depth >= 512) return;
    if (js->js_image_loads)
        g_hash_table_remove(js->js_image_loads, n);
    ns_dispatch_forget_node(js, n);
    for (ns_node *c = n->first_child; c; c = c->next_sibling)
        ns_js_orphan_prune_rec(js, c, depth + 1);
}

void
ns_js_orphan_children(ns_js *js, ns_node *n)
{
    if (!js) {
        ns_element_clear_children(n);
        return;
    }
    ns_node *c = n->first_child;
    while (c) {
        ns_node *next = c->next_sibling;
        ns_js_orphan_prune_rec(js, c, 0);
        ns_node_remove(c);
        ns_js_index_child_change(js, n, NULL, c);
        ns_js_orphan_node(js, c);
        c = next;
    }
    n->first_child = NULL;
    n->last_child  = NULL;
}

void
ns_element_replace_all_recorded(ns_js *js, ns_node *n, ns_node *added)
{
    if (!js) {
        ns_element_clear_children(n);
        if (added) ns_node_append_child(n, added);
        return;
    }
    GPtrArray *removed = g_ptr_array_new();
    ns_node *c;
    while ((c = n->first_child) != NULL) {
        ns_ce_disconnect_subtree(js, c);
        ns_node_remove(c);
        g_hash_table_add(js->orphan_nodes, c);
        ns_js_index_child_change(js, n, NULL, c);
        g_ptr_array_add(removed, c);
    }
    GPtrArray *add_arr = NULL;
    if (added) {
        ns_node_append_child(n, added);
        ns_js_index_child_change(js, n, added, NULL);
        add_arr = g_ptr_array_new();
        g_ptr_array_add(add_arr, added);
    }
    ns_css_mark_childlist_dirty(n, added);
    ns_mut_record_emit_child_list_arrays(js, n, add_arr, removed, NULL, NULL);
    if (add_arr) g_ptr_array_free(add_arr, FALSE);
    g_ptr_array_free(removed, FALSE);
}

#define NS_SCRIPT_ALREADY_STARTED "data-nd-script-already-started"
#define NS_SCRIPT_EMPTY_SOURCE "data-nd-script-empty-source"

static void
ns_mark_scripts_already_started_rec(ns_node *root, int depth)
{
    if (!root || depth >= 512) return;
    if (root->kind == NS_NODE_ELEMENT && root->name &&
        strcmp(root->name, "script") == 0)
        ns_element_set_attr(root, NS_SCRIPT_ALREADY_STARTED, "1");
    for (ns_node *c = root->first_child; c; c = c->next_sibling)
        ns_mark_scripts_already_started_rec(c, depth + 1);
}

void
ns_mark_scripts_already_started(ns_node *root)
{
    ns_mark_scripts_already_started_rec(root, 0);
}

static inline const char *
ns_attr_name_normalize(const ns_node *n, const char *raw_name, char **out_lowered)
{
    *out_lowered = NULL;
    if (!n || !raw_name) return raw_name;
    if (n->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS))
        return raw_name;
    gboolean needs_lower = FALSE;
    for (const char *p = raw_name; *p; p++) {
        if ((unsigned char)*p >= 'A' && (unsigned char)*p <= 'Z') {
            needs_lower = TRUE;
            break;
        }
    }
    if (!needs_lower) return raw_name;
    *out_lowered = g_ascii_strdown(raw_name, -1);
    return *out_lowered ? *out_lowered : raw_name;
}

static JSValue
ns_element_getAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || argc < 1) return JS_NULL;
    const char *raw_name = JS_ToCString(ctx, argv[0]);
    if (!raw_name) return JS_NULL;
    char *lowered = NULL;
    const char *name = ns_attr_name_normalize(n, raw_name, &lowered);
    const char *val = NULL;
    gsize val_len = 0;
    for (const ns_attr *a = n->attrs; a; a = a->next) {
        if (!a->name || ns_attr_name_is_internal(a->name)) continue;
        if (strcmp(a->name, name) == 0) {
            val = a->value;
            val_len = a->value_len;
            break;
        }
    }
    JS_FreeCString(ctx, raw_name);
    g_free(lowered);
    return val ? JS_NewStringLen(ctx, val, val_len) : JS_NULL;
}

static JSValue
ns_element_hasAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || argc < 1) return JS_FALSE;
    const char *raw_name = JS_ToCString(ctx, argv[0]);
    if (!raw_name) return JS_FALSE;
    char *lowered = NULL;
    const char *name = ns_attr_name_normalize(n, raw_name, &lowered);
    gboolean found = FALSE;
    for (const ns_attr *a = n->attrs; a; a = a->next) {
        if (!a->name || ns_attr_name_is_internal(a->name)) continue;
        if (strcmp(a->name, name) == 0) {
            found = TRUE;
            break;
        }
    }
    JS_FreeCString(ctx, raw_name);
    g_free(lowered);
    return found ? JS_TRUE : JS_FALSE;
}

JSValue
ns_make_abort_error(JSContext *ctx)
{
    JSValue g = JS_GetGlobalObject(ctx);
    JSValue ctor = JS_GetPropertyStr(ctx, g, "DOMException");
    JS_FreeValue(ctx, g);
    JSValue exc = JS_UNDEFINED;
    if (JS_IsObject(ctor)) {
        JSValue args[2] = {
            JS_NewString(ctx, "The operation was aborted."),
            JS_NewString(ctx, "AbortError"),
        };
        exc = JS_CallConstructor(ctx, ctor, 2, args);
        JS_FreeValue(ctx, args[0]);
        JS_FreeValue(ctx, args[1]);
    }
    JS_FreeValue(ctx, ctor);
    if (JS_IsException(exc) || !JS_IsObject(exc)) {
        if (JS_IsException(exc)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, exc);
        exc = JS_NewError(ctx);
        JS_SetPropertyStr(ctx, exc, "name", JS_NewString(ctx, "AbortError"));
        JS_SetPropertyStr(ctx, exc, "message",
                          JS_NewString(ctx, "The operation was aborted."));
    }
    return exc;
}

static JSValue
ns_event_noop(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_UNDEFINED;
}

/* Platform objects whose state the page must not see.  What the engine
 * stores on one (its C functions and hidden-source scripts, see
 * JS_IsHostAccess) lives in a state object of its own, as if it were the
 * object's own properties, so the code that builds and drives these
 * objects keeps using plain property access.  The page sees the interface's
 * prototype accessors and nothing else; what the page defines on the object
 * is the object's own. */
static JSClassID ns_hostobj_class_id;

#define NS_HO_BIT(kind) (1u << (kind))
#define NS_HO_XHR_EVENT_TARGETS (NS_HO_BIT(NS_HO_XHR) | NS_HO_BIT(NS_HO_XHR_UPLOAD))

static const char *const ns_ho_iface_names[NS_HO_KIND_COUNT] = {
    [NS_HO_ABORT_CONTROLLER] = "AbortController",
    [NS_HO_ABORT_SIGNAL] = "AbortSignal",
    [NS_HO_BROADCAST_CHANNEL] = "BroadcastChannel",
    [NS_HO_FILE_READER] = "FileReader",
    [NS_HO_FORM_DATA] = "FormData",
    [NS_HO_MESSAGE_CHANNEL] = "MessageChannel",
    [NS_HO_MESSAGE_PORT] = "MessagePort",
    [NS_HO_TEXT_ENCODER] = "TextEncoder",
    [NS_HO_TEXT_DECODER] = "TextDecoder",
    [NS_HO_XHR] = "XMLHttpRequest",
    [NS_HO_XHR_UPLOAD] = "XMLHttpRequestUpload",
};

static ns_hostobj *
ns_ho_data(JSValueConst v)
{
    return ns_hostobj_class_id && JS_VALUE_GET_TAG(v) == JS_TAG_OBJECT
        ? JS_GetOpaque(v, ns_hostobj_class_id) : NULL;
}

gboolean
ns_js_is_host_object(JSValueConst v)
{
    return ns_ho_data(v) != NULL;
}

static ns_hostobj *
ns_ho_of(JSValueConst v, ns_ho_kind kind)
{
    ns_hostobj *d = ns_ho_data(v);
    return d && d->kind == kind ? d : NULL;
}

static JSValue
ns_ho_illegal(JSContext *ctx)
{
    return JS_ThrowTypeError(ctx, "Illegal invocation");
}

#define NS_HO_THIS(ctx, this_val, kind) \
    do { if (!ns_ho_of(this_val, kind)) return ns_ho_illegal(ctx); } while (0)

static void
ns_hostobj_finalizer(JSRuntime *rt, JSValue val)
{
    ns_hostobj *d = JS_GetOpaque(val, ns_hostobj_class_id);
    if (!d) return;
    if (d->native && d->free_native) d->free_native(rt, d->native);
    JS_FreeValueRT(rt, d->state);
    g_free(d);
}

static void
ns_hostobj_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    ns_hostobj *d = JS_GetOpaque(val, ns_hostobj_class_id);
    if (d) JS_MarkValue(rt, d->state, mark_func);
}

static int
ns_hostobj_get_own_property(JSContext *ctx, JSPropertyDescriptor *desc,
                            JSValueConst obj, JSAtom prop)
{
    ns_hostobj *d = ns_ho_data(obj);
    if (!d || !JS_IsHostAccess(ctx)) return 0;
    return JS_GetOwnProperty(ctx, desc, d->state, prop);
}

static int
ns_hostobj_get_own_property_names(JSContext *ctx, JSPropertyEnum **ptab,
                                  uint32_t *plen, JSValueConst obj)
{
    ns_hostobj *d = ns_ho_data(obj);
    *ptab = NULL;
    *plen = 0;
    if (!d || !JS_IsHostAccess(ctx)) return 0;
    return JS_GetOwnPropertyNames(ctx, ptab, plen, d->state,
                                  JS_GPN_STRING_MASK | JS_GPN_SYMBOL_MASK);
}

static int
ns_hostobj_delete_property(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    ns_hostobj *d = ns_ho_data(obj);
    if (!d || !JS_IsHostAccess(ctx)) return TRUE;
    return JS_DeleteProperty(ctx, d->state, prop, 0);
}

static int
ns_hostobj_define_own_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                               JSValueConst val, JSValueConst getter,
                               JSValueConst setter, int flags)
{
    ns_hostobj *d = ns_ho_data(obj);
    if (d && JS_IsHostAccess(ctx))
        return JS_DefineProperty(ctx, d->state, prop, val, getter, setter,
                                 flags);
    return JS_DefineProperty(ctx, obj, prop, val, getter, setter,
                             flags | JS_PROP_NO_EXOTIC);
}

static int
ns_hostobj_set_state_property(JSContext *ctx, ns_hostobj *d, JSValueConst obj,
                              JSAtom prop, JSValueConst value, int flags)
{
    JSPropertyDescriptor desc;
    int has = JS_GetOwnProperty(ctx, &desc, d->state, prop);
    if (has < 0) return -1;
    if (has) {
        JS_FreeValue(ctx, desc.value);
        if (desc.flags & JS_PROP_GETSET) {
            int ret = TRUE;
            if (JS_IsFunction(ctx, desc.setter)) {
                JSValue r = JS_Call(ctx, desc.setter, obj, 1, &value);
                ret = JS_IsException(r) ? -1 : TRUE;
                JS_FreeValue(ctx, r);
            }
            JS_FreeValue(ctx, desc.getter);
            JS_FreeValue(ctx, desc.setter);
            return ret;
        }
        JS_FreeValue(ctx, desc.getter);
        JS_FreeValue(ctx, desc.setter);
    }
    return JS_SetPropertyReceiver(ctx, d->state, prop,
                                  JS_DupValue(ctx, value), d->state, flags);
}

static int
ns_hostobj_set_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                        JSValueConst value, JSValueConst receiver, int flags)
{
    ns_hostobj *d = ns_ho_data(obj);
    if (d && JS_IsHostAccess(ctx) &&
        JS_VALUE_GET_PTR(receiver) == JS_VALUE_GET_PTR(obj))
        return ns_hostobj_set_state_property(ctx, d, obj, prop, value, flags);
    JSValue proto = JS_GetPrototype(ctx, obj);
    int ret;
    if (JS_IsObject(proto))
        ret = JS_SetPropertyReceiver(ctx, proto, prop, JS_DupValue(ctx, value),
                                     receiver, flags);
    else
        ret = JS_DefineProperty(ctx, receiver, prop, value, JS_UNDEFINED,
                                JS_UNDEFINED, JS_PROP_C_W_E |
                                JS_PROP_HAS_VALUE | JS_PROP_HAS_WRITABLE |
                                JS_PROP_HAS_ENUMERABLE |
                                JS_PROP_HAS_CONFIGURABLE);
    JS_FreeValue(ctx, proto);
    return ret;
}

static JSClassExoticMethods ns_hostobj_exotic = {
    .get_own_property = ns_hostobj_get_own_property,
    .get_own_property_names = ns_hostobj_get_own_property_names,
    .delete_property = ns_hostobj_delete_property,
    .define_own_property = ns_hostobj_define_own_property,
    .set_property = ns_hostobj_set_property,
};

static JSClassDef ns_hostobj_class = {
    .class_name = "Object",
    .finalizer = ns_hostobj_finalizer,
    .gc_mark = ns_hostobj_gc_mark,
    .exotic = &ns_hostobj_exotic,
};

static JSValue
ns_ho_new(JSContext *ctx, ns_ho_kind kind, JSValueConst proto)
{
    ns_new_class_id(&ns_hostobj_class_id);
    JSRuntime *rt = JS_GetRuntime(ctx);
    if (!JS_IsRegisteredClass(rt, ns_hostobj_class_id))
        JS_NewClass(rt, ns_hostobj_class_id, &ns_hostobj_class);
    JSValue object_proto = JS_UNDEFINED;
    if (!JS_IsObject(proto)) {
        JSValue plain = JS_NewObject(ctx);
        object_proto = JS_GetPrototype(ctx, plain);
        JS_FreeValue(ctx, plain);
    }
    JSValue obj = JS_NewObjectProtoClass(ctx,
        JS_IsObject(proto) ? proto : object_proto, ns_hostobj_class_id);
    JS_FreeValue(ctx, object_proto);
    if (JS_IsException(obj)) return obj;
    ns_hostobj *d = g_new0(ns_hostobj, 1);
    d->kind = kind;
    d->state = JS_NewObjectProto(ctx, JS_NULL);
    JS_SetOpaque(obj, d);
    return obj;
}

JSValue
ns_ho_new_default(JSContext *ctx, ns_ho_kind kind)
{
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue proto = ns_proto_of(ctx, global, ns_ho_iface_names[kind]);
    JS_FreeValue(ctx, global);
    JSValue obj = ns_ho_new(ctx, kind, proto);
    JS_FreeValue(ctx, proto);
    return obj;
}

JSValue
ns_ho_construct(JSContext *ctx, JSValueConst new_target, ns_ho_kind kind)
{
    if (!JS_IsObject(new_target))
        return JS_ThrowTypeError(ctx,
            "Failed to construct '%s': Please use the 'new' operator, this "
            "DOM object constructor cannot be called as a function.",
            ns_ho_iface_names[kind]);
    JSValue proto = JS_GetPropertyStr(ctx, new_target, "prototype");
    if (JS_IsException(proto)) return proto;
    JSValue obj = JS_IsObject(proto) ? ns_ho_new(ctx, kind, proto)
                                     : ns_ho_new_default(ctx, kind);
    JS_FreeValue(ctx, proto);
    return obj;
}

JSValue
ns_form_data_construct(JSContext *ctx, JSValueConst new_target)
{
    return ns_ho_construct(ctx, new_target, NS_HO_FORM_DATA);
}

gboolean
ns_js_value_is_message_port(JSValueConst v)
{
    return ns_ho_of(v, NS_HO_MESSAGE_PORT) != NULL;
}

gboolean
ns_js_value_is_broadcast_channel(JSValueConst v)
{
    return ns_ho_of(v, NS_HO_BROADCAST_CHANNEL) != NULL;
}

JSValue
ns_message_port_state(JSContext *ctx, JSValueConst v)
{
    ns_hostobj *d = ns_ho_of(v, NS_HO_MESSAGE_PORT);
    return d ? JS_DupValue(ctx, d->state) : JS_UNDEFINED;
}

JSValue
ns_message_port_new_object(JSContext *ctx)
{
    return ns_ho_new_default(ctx, NS_HO_MESSAGE_PORT);
}

JSValue
ns_message_channel_construct(JSContext *ctx, JSValueConst new_target)
{
    return ns_ho_construct(ctx, new_target, NS_HO_MESSAGE_CHANNEL);
}

JSValue
ns_broadcast_channel_construct(JSContext *ctx, JSValueConst new_target)
{
    return ns_ho_construct(ctx, new_target, NS_HO_BROADCAST_CHANNEL);
}

typedef enum {
    NS_HA_STRING, NS_HA_BOOL, NS_HA_NUMBER, NS_HA_NULL, NS_HA_UNDEFINED,
    NS_HA_HANDLER,
} ns_ho_attr_type;

typedef struct ns_ho_attr {
    const char     *iface;
    guint           kinds;
    const char     *name;
    ns_ho_attr_type type;
    gboolean        writable;
} ns_ho_attr;

/* The attributes of these interfaces, as accessors on their prototypes;
 * the value is the object's state of that name, or what a fresh object of
 * the interface has. */
static const ns_ho_attr ns_ho_attrs[] = {
    { "AbortController", NS_HO_BIT(NS_HO_ABORT_CONTROLLER), "signal", NS_HA_NULL, FALSE },
    { "AbortSignal", NS_HO_BIT(NS_HO_ABORT_SIGNAL), "aborted", NS_HA_BOOL, FALSE },
    { "AbortSignal", NS_HO_BIT(NS_HO_ABORT_SIGNAL), "onabort", NS_HA_HANDLER, TRUE },
    { "AbortSignal", NS_HO_BIT(NS_HO_ABORT_SIGNAL), "reason", NS_HA_UNDEFINED, FALSE },
    { "BroadcastChannel", NS_HO_BIT(NS_HO_BROADCAST_CHANNEL), "name", NS_HA_STRING, FALSE },
    { "BroadcastChannel", NS_HO_BIT(NS_HO_BROADCAST_CHANNEL), "onmessage", NS_HA_HANDLER, TRUE },
    { "BroadcastChannel", NS_HO_BIT(NS_HO_BROADCAST_CHANNEL), "onmessageerror", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "error", NS_HA_NULL, FALSE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onabort", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onerror", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onload", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onloadend", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onloadstart", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "onprogress", NS_HA_HANDLER, TRUE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "readyState", NS_HA_NUMBER, FALSE },
    { "FileReader", NS_HO_BIT(NS_HO_FILE_READER), "result", NS_HA_NULL, FALSE },
    { "MessageChannel", NS_HO_BIT(NS_HO_MESSAGE_CHANNEL), "port1", NS_HA_NULL, FALSE },
    { "MessageChannel", NS_HO_BIT(NS_HO_MESSAGE_CHANNEL), "port2", NS_HA_NULL, FALSE },
    { "MessagePort", NS_HO_BIT(NS_HO_MESSAGE_PORT), "onmessageerror", NS_HA_HANDLER, TRUE },
    { "TextEncoder", NS_HO_BIT(NS_HO_TEXT_ENCODER), "encoding", NS_HA_STRING, FALSE },
    { "TextDecoder", NS_HO_BIT(NS_HO_TEXT_DECODER), "encoding", NS_HA_STRING, FALSE },
    { "TextDecoder", NS_HO_BIT(NS_HO_TEXT_DECODER), "fatal", NS_HA_BOOL, FALSE },
    { "TextDecoder", NS_HO_BIT(NS_HO_TEXT_DECODER), "ignoreBOM", NS_HA_BOOL, FALSE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onabort", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onerror", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onload", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onloadend", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onloadstart", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "onprogress", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequestEventTarget", NS_HO_XHR_EVENT_TARGETS, "ontimeout", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "onreadystatechange", NS_HA_HANDLER, TRUE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "response", NS_HA_STRING, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "responseURL", NS_HA_STRING, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "responseXML", NS_HA_NULL, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "status", NS_HA_NUMBER, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "statusText", NS_HA_STRING, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "timeout", NS_HA_NUMBER, TRUE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "upload", NS_HA_NULL, FALSE },
    { "XMLHttpRequest", NS_HO_BIT(NS_HO_XHR), "withCredentials", NS_HA_BOOL, TRUE },
};

static const ns_ho_attr *
ns_ho_attr_checked(JSContext *ctx, JSValueConst this_val, int magic,
                   ns_hostobj **out)
{
    if (magic < 0 || magic >= (int)G_N_ELEMENTS(ns_ho_attrs)) return NULL;
    const ns_ho_attr *a = &ns_ho_attrs[magic];
    ns_hostobj *d = ns_ho_data(this_val);
    (void)ctx;
    if (!d || !(a->kinds & NS_HO_BIT(d->kind))) return NULL;
    *out = d;
    return a;
}

static JSValue
ns_ho_attr_get(JSContext *ctx, JSValueConst this_val, int argc,
               JSValueConst *argv, int magic)
{
    (void)argc; (void)argv;
    ns_hostobj *d = NULL;
    const ns_ho_attr *a = ns_ho_attr_checked(ctx, this_val, magic, &d);
    if (!a) return ns_ho_illegal(ctx);
    JSValue v = JS_GetPropertyStr(ctx, d->state, a->name);
    if (!JS_IsUndefined(v)) return v;
    switch (a->type) {
    case NS_HA_STRING:    return JS_NewString(ctx, "");
    case NS_HA_BOOL:      return JS_FALSE;
    case NS_HA_NUMBER:    return JS_NewInt32(ctx, 0);
    case NS_HA_UNDEFINED: return JS_UNDEFINED;
    case NS_HA_NULL:
    case NS_HA_HANDLER:
    default:              return JS_NULL;
    }
}

static JSValue
ns_ho_attr_set(JSContext *ctx, JSValueConst this_val, int argc,
               JSValueConst *argv, int magic)
{
    ns_hostobj *d = NULL;
    const ns_ho_attr *a = ns_ho_attr_checked(ctx, this_val, magic, &d);
    if (!a) return ns_ho_illegal(ctx);
    JSValueConst in = argc > 0 ? argv[0] : JS_UNDEFINED;
    JSValue v;
    switch (a->type) {
    case NS_HA_HANDLER:
        v = JS_IsObject(in) ? JS_DupValue(ctx, in) : JS_NULL;
        break;
    case NS_HA_STRING:
        v = JS_ToString(ctx, in);
        break;
    case NS_HA_BOOL:
        v = JS_NewBool(ctx, JS_ToBool(ctx, in) > 0);
        break;
    case NS_HA_NUMBER:
        v = JS_ToNumber(ctx, in);
        break;
    default:
        v = JS_DupValue(ctx, in);
        break;
    }
    if (JS_IsException(v)) return v;
    JS_SetPropertyStr(ctx, d->state, a->name, v);
    return JS_UNDEFINED;
}

void
ns_ho_install_attrs(JSContext *ctx, JSValueConst global)
{
    const char *iface = NULL;
    JSValue proto = JS_UNDEFINED;
    for (gsize i = 0; i < G_N_ELEMENTS(ns_ho_attrs); i++) {
        const ns_ho_attr *a = &ns_ho_attrs[i];
        if (!iface || strcmp(iface, a->iface) != 0) {
            JS_FreeValue(ctx, proto);
            iface = a->iface;
            proto = ns_proto_of(ctx, global, iface);
        }
        if (!JS_IsObject(proto)) continue;
        char *get_name = g_strconcat("get ", a->name, NULL);
        JSValue getter = JS_NewCFunctionMagic(ctx, ns_ho_attr_get, get_name, 0,
                                              JS_CFUNC_generic_magic, (int)i);
        g_free(get_name);
        JSValue setter = JS_UNDEFINED;
        if (a->writable) {
            char *set_name = g_strconcat("set ", a->name, NULL);
            setter = JS_NewCFunctionMagic(ctx, ns_ho_attr_set, set_name, 1,
                                          JS_CFUNC_generic_magic, (int)i);
            g_free(set_name);
        }
        JSAtom atom = JS_NewAtom(ctx, a->name);
        JS_DefinePropertyGetSet(ctx, proto, atom, getter, setter,
                                JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
        JS_FreeAtom(ctx, atom);
    }
    JS_FreeValue(ctx, proto);
}

void
ns_bind_fn(JSContext *ctx, JSValueConst obj, const char *name,
           JSCFunction *fn, int argc)
{
    JSValue f = JS_NewCFunction(ctx, fn, name, argc);
    if (name[0] == '_' && name[1] == '_') {
        JSAtom atom = JS_NewAtom(ctx, name);
        JS_DefinePropertyValue(ctx, obj, atom, f,
                               JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
        JS_FreeAtom(ctx, atom);
    } else {
        JS_SetPropertyStr(ctx, obj, name, f);
    }
}

void
ns_bind_event_target_listeners(JSContext *ctx, JSValueConst obj)
{
    ns_bind_fn(ctx, obj, "addEventListener",    ns_target_addEventListener,    2);
    ns_bind_fn(ctx, obj, "removeEventListener", ns_target_removeEventListener, 2);
}

static void
ns_bind_fn_if_missing(JSContext *ctx, JSValueConst obj, const char *name,
                      JSCFunction *fn, int argc)
{
    JSAtom atom = JS_NewAtom(ctx, name);
    int has = JS_HasProperty(ctx, obj, atom);
    JS_FreeAtom(ctx, atom);
    if (has <= 0)
        ns_bind_fn(ctx, obj, name, fn, argc);
}

void
ns_bind_fn_if_not_callable(JSContext *ctx, JSValueConst obj, const char *name,
                           JSCFunction *fn, int argc)
{
    JSValue current = JS_GetPropertyStr(ctx, obj, name);
    gboolean callable = JS_IsFunction(ctx, current);
    JS_FreeValue(ctx, current);
    if (!callable)
        ns_bind_fn(ctx, obj, name, fn, argc);
}

static void ns_set_tostring_tag(JSContext *ctx, JSValueConst obj,
                                const char *tag);

JSValue
ns_make_ctor(JSContext *ctx, JSCFunction *fn, const char *name, int argc)
{
    JSValue func = JS_NewCFunction2(ctx, fn, name, argc,
                                    JS_CFUNC_constructor_or_func, 0);
    JSValue proto = JS_NewObject(ctx);
    JS_DefinePropertyValueStr(ctx, proto, "constructor",
                              JS_DupValue(ctx, func),
                              JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    ns_set_tostring_tag(ctx, proto, name);
    JS_DefinePropertyValueStr(ctx, func, "prototype", proto, JS_PROP_WRITABLE);
    return func;
}

void
ns_bind_ctor(JSContext *ctx, JSValueConst obj, const char *name,
             JSCFunction *fn, int argc)
{
    JS_SetPropertyStr(ctx, obj, name, ns_make_ctor(ctx, fn, name, argc));
}

typedef struct ns_fn_def { const char *name; int argc; } ns_fn_def;

static void
ns_bind_ctor_proto_fn(JSContext *ctx, JSValueConst global,
                      const char *ctor_name, const char *fn_name,
                      JSCFunction *fn, int argc)
{
    JSValue ctor = JS_GetPropertyStr(ctx, global, ctor_name);
    JSValue proto = JS_GetPropertyStr(ctx, ctor, "prototype");
    if (JS_IsObject(proto))
        ns_bind_fn_if_missing(ctx, proto, fn_name, fn, argc);
    JS_FreeValue(ctx, proto);
    JS_FreeValue(ctx, ctor);
}

static void
ns_bind_fns(JSContext *ctx, JSValueConst obj, JSCFunction *fn,
            const ns_fn_def *defs, gsize n)
{
    for (gsize i = 0; i < n; i++)
        ns_bind_fn(ctx, obj, defs[i].name, fn, defs[i].argc);
}

void
ns_install_namespace_object(JSContext *ctx, JSValueConst global,
                            const char *name, JSValue obj,
                            const char *tag)
{
    JSValue proto = JS_NewObject(ctx);
    JS_SetPrototype(ctx, obj, proto);
    JS_FreeValue(ctx, proto);
    JSValue sym = JS_GetPropertyStr(ctx, global, "Symbol");
    JSValue tag_sym = JS_GetPropertyStr(ctx, sym, "toStringTag");
    JSAtom tag_atom = JS_ValueToAtom(ctx, tag_sym);
    if (tag_atom != JS_ATOM_NULL) {
        JS_DefinePropertyValue(ctx, obj, tag_atom, JS_NewString(ctx, tag),
            JS_PROP_CONFIGURABLE);
        JS_FreeAtom(ctx, tag_atom);
    }
    JS_FreeValue(ctx, tag_sym);
    JS_FreeValue(ctx, sym);
    JS_DefinePropertyValueStr(ctx, global, name, obj,
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
}

static void
ns_bind_ctors(JSContext *ctx, JSValueConst obj, JSCFunction *fn,
              const ns_fn_def *defs, gsize n)
{
    for (gsize i = 0; i < n; i++)
        ns_bind_ctor(ctx, obj, defs[i].name, fn, defs[i].argc);
}

typedef struct ns_int_constant {
    const char *name;
    int value;
} ns_int_constant;

static void
ns_bind_ctor_int_constants(JSContext *ctx, JSValueConst global,
                           const char *ctor_name,
                           const ns_int_constant *constants, gsize count)
{
    JSValue ctor = JS_GetPropertyStr(ctx, global, ctor_name);
    if (!JS_IsObject(ctor)) {
        JS_FreeValue(ctx, ctor);
        return;
    }
    JSValue proto = JS_GetPropertyStr(ctx, ctor, "prototype");
    for (gsize i = 0; i < count; i++) {
        JS_DefinePropertyValueStr(ctx, ctor, constants[i].name,
            JS_NewInt32(ctx, constants[i].value), JS_PROP_ENUMERABLE);
        if (JS_IsObject(proto))
            JS_DefinePropertyValueStr(ctx, proto, constants[i].name,
                JS_NewInt32(ctx, constants[i].value), JS_PROP_ENUMERABLE);
    }
    JS_FreeValue(ctx, proto);
    JS_FreeValue(ctx, ctor);
}

static JSValue
ns_event_empty_array(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return JS_NewArray(ctx);
}


JSValue
ns_returns_resolved_undefined(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    JSValue resolvers[2];
    JSValue promise = JS_NewPromiseCapability(ctx, resolvers);
    if (JS_IsException(promise)) return promise;
    JSValue undef = JS_UNDEFINED;
    JS_Call(ctx, resolvers[0], JS_UNDEFINED, 1, &undef);
    JS_FreeValue(ctx, resolvers[0]);
    JS_FreeValue(ctx, resolvers[1]);
    return promise;
}

static JSValue
ns_promise_resolve_take(JSContext *ctx, JSValue value)
{
    JSValue resolvers[2];
    JSValue promise = JS_NewPromiseCapability(ctx, resolvers);
    if (JS_IsException(promise)) {
        JS_FreeValue(ctx, value);
        return promise;
    }
    JSValueConst args[1] = { value };
    JS_Call(ctx, resolvers[0], JS_UNDEFINED, 1, args);
    JS_FreeValue(ctx, value);
    JS_FreeValue(ctx, resolvers[0]);
    JS_FreeValue(ctx, resolvers[1]);
    return promise;
}

JSValue
ns_returns_resolved_false(JSContext *ctx, JSValueConst this_val,
                          int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return ns_promise_resolve_take(ctx, JS_FALSE);
}

JSValue
ns_returns_resolved_empty_array(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return ns_promise_resolve_take(ctx, JS_NewArray(ctx));
}

static JSValue
ns_cam_request(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val;
    ns_js *js = js_from_ctx(ctx);
    gboolean want_video = argc > 0 && JS_ToBool(ctx, argv[0]);
    gboolean want_audio = argc > 1 && JS_ToBool(ctx, argv[1]);
    int d = ns_camera_permission(js);
    if (d == 1) {
        if (want_video) ns_camera_acquire();
        if (want_audio) ns_mic_acquire();
        return JS_NewString(ctx, "granted");
    }
    if (d == 0)
        return JS_NewString(ctx, "denied");
    return JS_NewString(ctx, "pending");
}

static JSValue
ns_cam_release(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    ns_camera_release();
    return JS_UNDEFINED;
}

static JSValue
ns_mic_release_js(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    ns_mic_release();
    return JS_UNDEFINED;
}

static JSValue
ns_cam_label(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    ns_camera *cam = ns_camera_active();
    GPtrArray *list = ns_camera_enumerate();
    const char *label = NULL;
    if (cam) {
        for (guint i = 0; i < list->len; i++) {
            ns_camera_info *info = g_ptr_array_index(list, i);
            if (g_strcmp0(info->device, ns_camera_device(cam)) == 0) {
                label = info->label;
                break;
            }
        }
    }
    if (!label && list->len > 0)
        label = ((ns_camera_info *)g_ptr_array_index(list, 0))->label;
    JSValue r = JS_NewString(ctx, label ? label : "Camera");
    for (guint i = 0; i < list->len; i++)
        ns_camera_info_free(g_ptr_array_index(list, i));
    g_ptr_array_free(list, TRUE);
    return r;
}

static JSValue
ns_cam_enumerate(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    GPtrArray *list = ns_camera_enumerate();
    JSValue arr = JS_NewArray(ctx);
    uint32_t n = 0;
    for (guint i = 0; i < list->len; i++) {
        ns_camera_info *info = g_ptr_array_index(list, i);
        JSValue dev = JS_NewObject(ctx);
        JS_SetPropertyStr(ctx, dev, "deviceId", JS_NewString(ctx, info->device));
        JS_SetPropertyStr(ctx, dev, "groupId", JS_NewString(ctx, info->device));
        JS_SetPropertyStr(ctx, dev, "kind", JS_NewString(ctx, "videoinput"));
        JS_SetPropertyStr(ctx, dev, "label", JS_NewString(ctx, info->label));
        JS_SetPropertyUint32(ctx, arr, n++, dev);
        ns_camera_info_free(info);
    }
    g_ptr_array_free(list, TRUE);
    return arr;
}

static JSValue
ns_returns_null(JSContext *ctx, JSValueConst this_val,
                int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_NULL;
}

JSValue
ns_throw_dom_exception(JSContext *ctx, const char *name, int code,
                       const char *message)
{
    JSValue err = JS_NewError(ctx);
    JS_DefinePropertyValueStr(ctx, err, "name", JS_NewString(ctx, name),
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    JS_DefinePropertyValueStr(ctx, err, "message", JS_NewString(ctx, message),
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    JS_DefinePropertyValueStr(ctx, err, "code", JS_NewInt32(ctx, code),
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue dx = JS_GetPropertyStr(ctx, global, "DOMException");
    if (JS_IsObject(dx)) {
        JSValue proto = JS_GetPropertyStr(ctx, dx, "prototype");
        if (JS_IsObject(proto)) JS_SetPrototype(ctx, err, proto);
        JS_FreeValue(ctx, proto);
    }
    JS_FreeValue(ctx, dx);
    JS_FreeValue(ctx, global);
    return JS_Throw(ctx, err);
}

static JSValue
ns_window_find(JSContext *ctx, JSValueConst this_val,
               int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 1) return JS_FALSE;
    const char *needle = JS_ToCString(ctx, argv[0]);
    if (!needle) return JS_FALSE;
    if (!*needle) { JS_FreeCString(ctx, needle); return JS_FALSE; }
    gboolean case_sensitive = argc >= 2 && JS_ToBool(ctx, argv[1]) > 0;
    ns_js *js = js_from_ctx(ctx);
    ns_node *doc = js ? js->current_doc : NULL;
    gboolean found = FALSE;
    if (doc) {
        char *hay = ns_node_collect_text(doc);
        if (hay) {
            if (case_sensitive) {
                found = strstr(hay, needle) != NULL;
            } else {
                char *h = g_utf8_casefold(hay, -1);
                char *n = g_utf8_casefold(needle, -1);
                found = h && n && strstr(h, n) != NULL;
                g_free(h);
                g_free(n);
            }
            g_free(hay);
        }
    }
    JS_FreeCString(ctx, needle);
    return JS_NewBool(ctx, found);
}

JSValue
ns_cache_open(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    JSValue cache = JS_NewObject(ctx);
    ns_bind_fn(ctx, cache, "match",    ns_returns_resolved_undefined, 1);
    ns_bind_fn(ctx, cache, "matchAll", ns_returns_resolved_empty_array, 1);
    ns_bind_fn(ctx, cache, "put",      ns_returns_resolved_undefined, 2);
    ns_bind_fn(ctx, cache, "add",      ns_returns_resolved_undefined, 1);
    ns_bind_fn(ctx, cache, "addAll",   ns_returns_resolved_undefined, 1);
    ns_bind_fn(ctx, cache, "delete",   ns_returns_resolved_false, 1);
    ns_bind_fn(ctx, cache, "keys",     ns_returns_resolved_empty_array, 1);
    return ns_promise_resolve_take(ctx, cache);
}


static gboolean
ns_document_command_run(ns_js *js, const char *cmd)
{
    if (!js || !cmd) return FALSE;
    if (g_ascii_strcasecmp(cmd, "copy") == 0 ||
        g_ascii_strcasecmp(cmd, "cut") == 0) {
        if (!js->selection_has_range || !js->clipboard_write_cb) return FALSE;
        return js->clipboard_write_cb(
            js->selection_text ? js->selection_text : "",
            js->clipboard_write_user_data);
    }
    if (g_ascii_strcasecmp(cmd, "selectall") == 0)
        return js->selection_cmd_cb &&
               js->selection_cmd_cb("selectAll", js->selection_cmd_user_data);
    if (g_ascii_strcasecmp(cmd, "unselect") == 0)
        return js->selection_cmd_cb &&
               js->selection_cmd_cb("unselect", js->selection_cmd_user_data);
    return FALSE;
}

static gboolean
ns_document_command_known(const char *cmd)
{
    return cmd && (g_ascii_strcasecmp(cmd, "copy") == 0 ||
                   g_ascii_strcasecmp(cmd, "cut") == 0 ||
                   g_ascii_strcasecmp(cmd, "selectall") == 0 ||
                   g_ascii_strcasecmp(cmd, "unselect") == 0);
}

static JSValue
ns_document_execCommand(JSContext *ctx, JSValueConst this_val,
                        int argc, JSValueConst *argv)
{
    (void)this_val;
    const char *cmd = argc > 0 ? JS_ToCString(ctx, argv[0]) : NULL;
    gboolean ok = ns_document_command_run(js_from_ctx(ctx), cmd);
    if (cmd) JS_FreeCString(ctx, cmd);
    return ok ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_document_queryCommandSupported(JSContext *ctx, JSValueConst this_val,
                                  int argc, JSValueConst *argv)
{
    (void)this_val;
    const char *cmd = argc > 0 ? JS_ToCString(ctx, argv[0]) : NULL;
    gboolean ok = ns_document_command_known(cmd);
    if (cmd) JS_FreeCString(ctx, cmd);
    return ok ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_document_queryCommandEnabled(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv)
{
    (void)this_val;
    ns_js *js = js_from_ctx(ctx);
    const char *cmd = argc > 0 ? JS_ToCString(ctx, argv[0]) : NULL;
    gboolean ok = FALSE;
    if (ns_document_command_known(cmd) && js) {
        if (g_ascii_strcasecmp(cmd, "copy") == 0 ||
            g_ascii_strcasecmp(cmd, "cut") == 0)
            ok = js->selection_has_range && js->clipboard_write_cb != NULL;
        else
            ok = js->selection_cmd_cb != NULL;
    }
    if (cmd) JS_FreeCString(ctx, cmd);
    return ok ? JS_TRUE : JS_FALSE;
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


static gboolean
ns_window_is_global_of(JSContext *ctx, JSValueConst win)
{
    JSValue global = JS_GetGlobalObject(ctx);
    gboolean same = JS_VALUE_GET_PTR(global) == JS_VALUE_GET_PTR(win);
    JS_FreeValue(ctx, global);
    return same;
}

static ns_node *
ns_window_frame_document(JSContext *ctx, JSValueConst window, ns_node *stale)
{
    JSValue fe = JS_GetPropertyStr(ctx, window, "frameElement");
    ns_node *frame = ns_unwrap_element_mut(fe);
    JS_FreeValue(ctx, fe);
    ns_node *doc = ns_iframe_document_node(frame);
    return doc ? doc : stale;
}

ns_node *
ns_window_current_document_for(JSContext *ctx, JSValueConst window)
{
    ns_js *js = js_from_ctx(ctx);
    JSValue forwarded = ns_window_forward_of(js, window);
    JSValueConst win = JS_IsObject(forwarded) ? forwarded : window;
    ns_node *doc = ns_window_document_for(ctx, win);
    if (doc && !doc->parent && js && (const ns_node *)doc != js->ce_main_doc &&
        !ns_window_is_global_of(ctx, win))
        doc = ns_window_frame_document(ctx, win, doc);
    JS_FreeValue(ctx, forwarded);
    return doc;
}

JSValue
ns_window_structured_clone(JSContext *ctx, JSValueConst this_val,
                           int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 1)
        return JS_ThrowTypeError(ctx, "structuredClone requires at least 1 argument");
    JSValue transfer = JS_UNDEFINED;
    if (argc >= 2 && !JS_IsUndefined(argv[1]) && !JS_IsNull(argv[1])) {
        if (!JS_IsObject(argv[1]))
            return JS_ThrowTypeError(ctx,
                "structuredClone: options is not an object");
        transfer = JS_GetPropertyStr(ctx, argv[1], "transfer");
        if (JS_IsException(transfer)) return transfer;
        if (!JS_IsUndefined(transfer) && !JS_IsArray(transfer)) {
            JS_FreeValue(ctx, transfer);
            return JS_ThrowTypeError(ctx,
                "structuredClone: transfer is not a sequence");
        }
    }
    JSValue old_ports, ports;
    if (ns_port_transfer_prepare(ctx, transfer, JS_UNDEFINED, ctx,
                                 &old_ports, &ports) < 0) {
        JS_FreeValue(ctx, transfer);
        return JS_EXCEPTION;
    }
    JSValue res = ns_structured_clone_transfer(ctx, argv[0], transfer,
                                               old_ports, ports);
    if (!JS_IsException(res)) ns_port_transfer_commit(ctx, old_ports, ports);
    JS_FreeValue(ctx, old_ports);
    JS_FreeValue(ctx, ports);
    return res;
}

JSValue
ns_make_dom_rect(JSContext *ctx, double x, double y, double w, double h)
{
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue ctor = JS_GetPropertyStr(ctx, global, "DOMRect");
    JS_FreeValue(ctx, global);
    if (JS_IsFunction(ctx, ctor)) {
        JSValue args[4] = {
            JS_NewFloat64(ctx, x), JS_NewFloat64(ctx, y),
            JS_NewFloat64(ctx, w), JS_NewFloat64(ctx, h),
        };
        JSValue rect = JS_CallConstructor(ctx, ctor, 4, args);
        for (int i = 0; i < 4; i++) JS_FreeValue(ctx, args[i]);
        JS_FreeValue(ctx, ctor);
        if (!JS_IsException(rect)) return rect;
        JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, rect);
    } else {
        JS_FreeValue(ctx, ctor);
    }
    JSValue r = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, r, "x",      JS_NewFloat64(ctx, x));
    JS_SetPropertyStr(ctx, r, "y",      JS_NewFloat64(ctx, y));
    JS_SetPropertyStr(ctx, r, "top",    JS_NewFloat64(ctx, y));
    JS_SetPropertyStr(ctx, r, "left",   JS_NewFloat64(ctx, x));
    JS_SetPropertyStr(ctx, r, "right",  JS_NewFloat64(ctx, x + w));
    JS_SetPropertyStr(ctx, r, "bottom", JS_NewFloat64(ctx, y + h));
    JS_SetPropertyStr(ctx, r, "width",  JS_NewFloat64(ctx, w));
    JS_SetPropertyStr(ctx, r, "height", JS_NewFloat64(ctx, h));
    return r;
}

static JSValue
ns_event_true(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_TRUE;
}

static JSValue
ns_event_false(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_FALSE;
}


/* hashchange, popstate and similar events are fired at the window: the
 * window is their target, and they neither bubble nor can be cancelled. */
JSValue
ns_make_window_event(JSContext *ctx, const char *type)
{
    JSValue event = ns_make_event(ctx, type, NULL);
    JS_SetPropertyStr(ctx, event, "target", JS_GetGlobalObject(ctx));
    JS_SetPropertyStr(ctx, event, "bubbles", JS_FALSE);
    JS_SetPropertyStr(ctx, event, "cancelable", JS_FALSE);
    return event;
}


gboolean
ns_js_element_is_rendered(JSContext *ctx, JSValueConst v)
{
    const ns_node *n = ns_unwrap_element(v);
    ns_js *js = js_from_ctx(ctx);
    if (!n || !js || !js->current_doc) return FALSE;
    const ns_node *p = n;
    for (; p && p != js->current_doc; p = p->parent) {
        if (ns_node_is_shadow_root(p)) continue;
        if (p->parent && ns_element_find_shadow_child(p->parent) &&
            !ns_node_assigned_slot_node(p))
            return FALSE;
    }
    return p == js->current_doc;
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

static JSValue
ns_window_confirm(JSContext *ctx, JSValueConst this_val,
                  int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_TRUE;
}

static JSValue
ns_window_prompt(JSContext *ctx, JSValueConst this_val,
                 int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc >= 2) return JS_DupValue(ctx, argv[1]);
    return JS_NewString(ctx, "");
}




static JSValue
ns_window_document_fragment_ctor(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    if (!js_from_ctx(ctx)) return JS_NULL;
    ns_node *frag = ns_node_new_document();
    frag->flags |= NS_NODE_FRAGMENT;
    g_hash_table_add(js_from_ctx(ctx)->orphan_nodes, frag);
    return ns_make_element(ctx, frag);
}

static JSValue
ns_window_text_ctor(JSContext *ctx, JSValueConst this_val,
                    int argc, JSValueConst *argv)
{
    (void)this_val;
    ns_js *js = js_from_ctx(ctx);
    if (!js) return JS_NULL;
    char *dup;
    size_t len = 0;
    if (argc >= 1 && !JS_IsUndefined(argv[0])) {
        const char *s = JS_ToCStringLen(ctx, &len, argv[0]);
        if (!s) return JS_EXCEPTION;
        dup = g_memdup2(s, len + 1);
        dup[len] = '\0';
        JS_FreeCString(ctx, s);
    } else {
        dup = g_strdup("");
    }
    ns_node *n = ns_node_new_text_len(dup, (guint32)len);
    g_hash_table_add(js->orphan_nodes, n);
    JSValue wrapper = ns_make_element(ctx, n);
    ns_tag_caller_document(ctx, wrapper);
    return wrapper;
}

static JSValue
ns_window_comment_ctor(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)this_val;
    ns_js *js = js_from_ctx(ctx);
    if (!js) return JS_NULL;
    char *dup;
    size_t len = 0;
    if (argc >= 1 && !JS_IsUndefined(argv[0])) {
        const char *s = JS_ToCStringLen(ctx, &len, argv[0]);
        if (!s) return JS_EXCEPTION;
        dup = g_memdup2(s, len + 1);
        dup[len] = '\0';
        JS_FreeCString(ctx, s);
    } else {
        dup = g_strdup("");
    }
    ns_node *n = ns_node_new_comment_len(dup, (guint32)len);
    g_hash_table_add(js->orphan_nodes, n);
    JSValue wrapper = ns_make_element(ctx, n);
    ns_tag_caller_document(ctx, wrapper);
    return wrapper;
}




























static JSValue
ns_window_image_ctor(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    (void)this_val;
    if (!js_from_ctx(ctx)) return JS_NULL;
    ns_node *el = ns_node_new_element(g_strdup("img"));
    if (argc >= 1) {
        int32_t w = 0;
        if (JS_ToInt32(ctx, &w, argv[0]) == 0 && w > 0) {
            char buf[16];
            g_snprintf(buf, sizeof buf, "%d", w);
            ns_element_set_attr(el, "width", buf);
        }
    }
    if (argc >= 2) {
        int32_t h = 0;
        if (JS_ToInt32(ctx, &h, argv[1]) == 0 && h > 0) {
            char buf[16];
            g_snprintf(buf, sizeof buf, "%d", h);
            ns_element_set_attr(el, "height", buf);
        }
    }
    g_hash_table_add(js_from_ctx(ctx)->orphan_nodes, el);
    return ns_make_element(ctx, el);
}

static JSValue
ns_window_audio_ctor(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    (void)this_val;
    if (!js_from_ctx(ctx)) return JS_NULL;
    ns_node *el = ns_node_new_element(g_strdup("audio"));
    if (argc >= 1) {
        const char *s = JS_ToCString(ctx, argv[0]);
        if (s) { ns_element_set_attr(el, "src", s); JS_FreeCString(ctx, s); }
    }
    g_hash_table_add(js_from_ctx(ctx)->orphan_nodes, el);
    return ns_make_element(ctx, el);
}

static JSValue
ns_window_option_ctor(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv)
{
    (void)this_val;
    if (!js_from_ctx(ctx)) return JS_NULL;
    ns_node *el = ns_node_new_element(g_strdup("option"));
    if (argc >= 1) {
        const char *t = JS_ToCString(ctx, argv[0]);
        if (t) {
            ns_node_append_child(el, ns_node_new_text(g_strdup(t)));
            JS_FreeCString(ctx, t);
        }
    }
    if (argc >= 2) {
        const char *v = JS_ToCString(ctx, argv[1]);
        if (v) { ns_element_set_attr(el, "value", v); JS_FreeCString(ctx, v); }
    }
    if (argc >= 3 && JS_ToBool(ctx, argv[2]))
        ns_element_set_attr(el, "defaultSelected", "");
    if (argc >= 4 && JS_ToBool(ctx, argv[3]))
        ns_element_set_attr(el, "selected", "");
    g_hash_table_add(js_from_ctx(ctx)->orphan_nodes, el);
    return ns_make_element(ctx, el);
}

static JSValue
ns_dom_parser_parseFromString(JSContext *ctx, JSValueConst this_val,
                              int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 1) return JS_NULL;
    const char *src = JS_ToCString(ctx, argv[0]);
    if (!src) return JS_NULL;
    const char *mime = (argc >= 2) ? JS_ToCString(ctx, argv[1]) : NULL;
    gboolean as_xml = mime && (strstr(mime, "xml") != NULL ||
                               strstr(mime, "svg") != NULL);
    ns_node *doc;
    if (as_xml) {
        int error_line = 1, error_column = 1;
        doc = ns_xml_parse_reporting(src, -1, &error_line, &error_column);
        gboolean has_root = FALSE;
        if (doc)
            for (const ns_node *c = doc->first_child; c; c = c->next_sibling)
                if (c->kind == NS_NODE_ELEMENT) { has_root = TRUE; break; }
        if (!has_root) {
            if (doc) ns_node_free(doc);
            char *report = g_strdup_printf(
                "<parsererror xmlns=\"http://www.mozilla.org/newlayout/xml/"
                "parsererror.xml\">error on line %d at column %d: "
                "XML parsing error</parsererror>", error_line, error_column);
            doc = ns_xml_parse(report, -1);
            g_free(report);
        }
        if (!doc)
            doc = ns_html_parse_fragment_with_scripting(NULL, src, -1,
                                                        FALSE);
    } else {
        doc = ns_html_parse_with_scripting(src, -1, FALSE);
    }
    JS_FreeCString(ctx, src);
    if (!doc) { if (mime) JS_FreeCString(ctx, mime); return JS_NULL; }
    ns_mark_scripts_already_started(doc);
    if (js_from_ctx(ctx)) g_hash_table_add(js_from_ctx(ctx)->orphan_nodes, doc);
    JSValue cu = JS_GetPropertyStr(ctx, this_val, "__ns_creator_url");
    const char *doc_url = JS_IsString(cu) ? JS_ToCString(ctx, cu) : NULL;
    JSValue wrapper = ns_make_realm_document(ctx, doc, doc_url, "UTF-8",
                                             mime, as_xml, TRUE);
    if (doc_url) JS_FreeCString(ctx, doc_url);
    JS_FreeValue(ctx, cu);
    if (JS_IsObject(wrapper))
        JS_DefinePropertyValueStr(ctx, wrapper, "defaultView", JS_NULL,
                                  JS_PROP_C_W_E);
    if (mime) JS_FreeCString(ctx, mime);
    return wrapper;
}

static JSValue
ns_window_dom_parser_ctor(JSContext *ctx, JSValueConst this_val,
                          int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue proto = JS_IsObject(this_val)
                        ? JS_GetPropertyStr(ctx, this_val, "prototype") : JS_NULL;
    JSValue obj = JS_IsObject(proto) ? JS_NewObjectProto(ctx, proto)
                                     : JS_NewObject(ctx);
    JS_FreeValue(ctx, proto);
    JSValue g = JS_GetGlobalObject(ctx);
    JSValue docv = JS_GetPropertyStr(ctx, g, "document");
    JSValue urlv = JS_IsObject(docv) ? JS_GetPropertyStr(ctx, docv, "URL")
                                     : JS_UNDEFINED;
    if (JS_IsString(urlv))
        JS_DefinePropertyValueStr(ctx, obj, "__ns_creator_url",
                                  JS_DupValue(ctx, urlv),
                                  JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
    JS_FreeValue(ctx, urlv);
    JS_FreeValue(ctx, docv);
    JS_FreeValue(ctx, g);
    return obj;
}



static JSValue
ns_close_watcher_fire(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue evt = ns_event_new(ctx);
    JS_SetPropertyStr(ctx, evt, "type", JS_NewString(ctx, "close"));
    JS_SetPropertyStr(ctx, evt, "target", JS_DupValue(ctx, this_val));
    JSValueConst args[1] = { evt };
    JSValue disp = JS_GetPropertyStr(ctx, this_val, "dispatchEvent");
    if (JS_IsFunction(ctx, disp)) {
        JSValue r = JS_Call(ctx, disp, this_val, 1, args);
        JS_FreeValue(ctx, r);
    }
    JS_FreeValue(ctx, disp);
    JS_FreeValue(ctx, evt);
    return JS_UNDEFINED;
}

static JSValue
ns_close_watcher_noop(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv)
{
    (void)ctx; (void)this_val; (void)argc; (void)argv;
    return JS_UNDEFINED;
}

static JSValue
ns_window_close_watcher_ctor(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue obj;
    JSValue proto = JS_IsObject(this_val)
                        ? JS_GetPropertyStr(ctx, this_val, "prototype") : JS_NULL;
    if (JS_IsObject(proto)) {
        obj = JS_NewObjectProto(ctx, proto);
    } else {
        obj = JS_NewObject(ctx);
    }
    JS_FreeValue(ctx, proto);
    JS_SetPropertyStr(ctx, obj, "_listeners", JS_NewArray(ctx));
    JS_SetPropertyStr(ctx, obj, "oncancel", JS_NULL);
    JS_SetPropertyStr(ctx, obj, "onclose", JS_NULL);
    ns_bind_fn(ctx, obj, "addEventListener",    ns_target_addEventListener,    2);
    ns_bind_fn(ctx, obj, "removeEventListener", ns_target_removeEventListener, 2);
    ns_bind_fn(ctx, obj, "dispatchEvent",       ns_target_dispatchEvent,       1);
    ns_bind_fn(ctx, obj, "requestClose",        ns_close_watcher_fire,         0);
    ns_bind_fn(ctx, obj, "close",               ns_close_watcher_fire,         0);
    ns_bind_fn(ctx, obj, "destroy",             ns_close_watcher_noop,         0);
    return obj;
}

typedef struct {
    GBytes *bytes;
    char   *type;
} ns_blob_entry;

static void
ns_blob_entry_free(gpointer p)
{
    ns_blob_entry *e = p;
    if (!e) return;
    if (e->bytes) g_bytes_unref(e->bytes);
    g_free(e->type);
    g_free(e);
}

static GPtrArray *g_blob_js_registry = NULL;
static GMutex     g_blob_js_registry_lock;

static GBytes *
ns_js_net_blob_resolver(const char *url, char **out_type, gpointer user_data)
{
    (void)user_data;
    if (out_type) *out_type = NULL;
    if (!url) return NULL;
    GBytes *found = NULL;
    g_mutex_lock(&g_blob_js_registry_lock);
    for (guint i = 0; g_blob_js_registry && i < g_blob_js_registry->len; i++) {
        ns_js *js = g_ptr_array_index(g_blob_js_registry, i);
        if (!js || !js->blob_urls) continue;
        ns_blob_entry *e = g_hash_table_lookup(js->blob_urls, url);
        if (e) {
            if (out_type) *out_type = e->type ? g_strdup(e->type) : NULL;
            found = e->bytes ? g_bytes_ref(e->bytes) : NULL;
            break;
        }
    }
    g_mutex_unlock(&g_blob_js_registry_lock);
    return found;
}

static void
ns_js_blob_registry_add(ns_js *js)
{
    if (!js) return;
    g_mutex_lock(&g_blob_js_registry_lock);
    gboolean install = !g_blob_js_registry;
    if (install)
        g_blob_js_registry = g_ptr_array_new();
    if (!g_ptr_array_find(g_blob_js_registry, js, NULL))
        g_ptr_array_add(g_blob_js_registry, js);
    g_mutex_unlock(&g_blob_js_registry_lock);
    if (install)
        ns_net_set_blob_resolver(ns_js_net_blob_resolver, NULL);
}

static void
ns_js_blob_registry_remove(ns_js *js)
{
    g_mutex_lock(&g_blob_js_registry_lock);
    if (g_blob_js_registry && js)
        g_ptr_array_remove_fast(g_blob_js_registry, js);
    g_mutex_unlock(&g_blob_js_registry_lock);
}

static void
ns_js_blob_urls_put_entry(ns_js *js, const char *url, ns_blob_entry *entry)
{
    g_mutex_lock(&g_blob_js_registry_lock);
    if (!js->blob_urls)
        js->blob_urls = g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, ns_blob_entry_free);
    g_hash_table_replace(js->blob_urls, g_strdup(url), entry);
    g_mutex_unlock(&g_blob_js_registry_lock);
}

void
ns_js_blob_urls_put(ns_js *js, const char *url, const guint8 *bytes, gsize len,
                    const char *type)
{
    if (!js || !url) return;
    ns_blob_entry *e = g_new0(ns_blob_entry, 1);
    e->bytes = g_bytes_new(bytes, len);
    e->type = type && *type ? g_strdup(type) : NULL;
    ns_js_blob_urls_put_entry(js, url, e);
}

void
ns_js_blob_urls_remove(ns_js *js, const char *url)
{
    if (!js || !js->blob_urls || !url) return;
    g_mutex_lock(&g_blob_js_registry_lock);
    g_hash_table_remove(js->blob_urls, url);
    g_mutex_unlock(&g_blob_js_registry_lock);
}

GBytes *
ns_js_blob_url_lookup(ns_js *js, const char *url, char **out_type)
{
    if (out_type) *out_type = NULL;
    if (!js || !js->blob_urls || !url) return NULL;
    ns_blob_entry *e = g_hash_table_lookup(js->blob_urls, url);
    if (!e) return NULL;
    if (out_type) *out_type = e->type;
    return e->bytes;
}

typedef struct ns_filereader_idle {
    ns_js  *js;
    JSValue self;
    guint   source;
    gint64  gen;
} ns_filereader_idle;

static gboolean
ns_filereader_complete(gpointer ud)
{
    ns_filereader_idle *fr = ud;
    ns_js *js = fr->js;
    if (js && js->in_pump) {
        fr->source = ns_js_attach_timeout(js, 4, ns_filereader_complete, fr);
        return G_SOURCE_REMOVE;
    }
    JSContext *ctx = js->ctx;
    JSValue self = fr->self;
    ns_filereader_run(ctx, self, fr->gen);
    JS_FreeValue(ctx, self);
    if (js->filereader_idles)
        g_ptr_array_remove_fast(js->filereader_idles, fr);
    g_free(fr);
    return G_SOURCE_REMOVE;
}

void
ns_js_filereader_schedule(JSContext *ctx, JSValueConst self, gint64 gen)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js) return;
    ns_filereader_idle *fr = g_new0(ns_filereader_idle, 1);
    fr->js = js;
    fr->self = JS_DupValue(ctx, self);
    fr->gen = gen;
    if (!js->filereader_idles)
        js->filereader_idles = g_ptr_array_new();
    g_ptr_array_add(js->filereader_idles, fr);
    fr->source = ns_js_attach_idle(js, ns_filereader_complete, fr);
}

JSValue
ns_window_event_ctor(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    ns_js *js = js_from_ctx(ctx);
    if (js) {
        JSValue elem = ns_ce_html_element_construct(ctx, this_val);
        if (!JS_IsUndefined(elem)) return elem;
    }
    JSValue obj;
    JSValue proto = JS_IsObject(this_val)
                        ? JS_GetPropertyStr(ctx, this_val, "prototype") : JS_NULL;
    if (JS_IsObject(proto) && ns_proto_is_event(ctx, proto))
        obj = ns_event_new_proto(ctx, proto);
    else if (JS_IsObject(proto))
        obj = JS_NewObjectProto(ctx, proto);
    else
        obj = JS_NewObject(ctx);
    JS_FreeValue(ctx, proto);
    if (argc >= 1) {
        JS_SetPropertyStr(ctx, obj, "type", JS_DupValue(ctx, argv[0]));
    }
    {
        JSValue cnamev = JS_IsObject(this_val)
            ? JS_GetPropertyStr(ctx, this_val, "name") : JS_UNDEFINED;
        const char *cname = JS_IsString(cnamev)
            ? JS_ToCString(ctx, cnamev) : NULL;
        if (cname && strcmp(cname, "ErrorEvent") == 0) {
            JS_DefinePropertyValueStr(ctx, obj, "__ndErrorEvent", JS_TRUE, 0);
            gboolean has_init = argc >= 2 && JS_IsObject(argv[1]);
            JSValue mv = has_init
                ? JS_GetPropertyStr(ctx, argv[1], "message") : JS_UNDEFINED;
            JS_SetPropertyStr(ctx, obj, "message", JS_IsUndefined(mv)
                ? JS_NewString(ctx, "") : mv);
            JSValue fv = has_init
                ? JS_GetPropertyStr(ctx, argv[1], "filename") : JS_UNDEFINED;
            JS_SetPropertyStr(ctx, obj, "filename", JS_IsUndefined(fv)
                ? JS_NewString(ctx, "") : fv);
            JSValue lv = has_init
                ? JS_GetPropertyStr(ctx, argv[1], "lineno") : JS_UNDEFINED;
            JS_SetPropertyStr(ctx, obj, "lineno", JS_IsUndefined(lv)
                ? JS_NewInt32(ctx, 0) : lv);
            JSValue cv = has_init
                ? JS_GetPropertyStr(ctx, argv[1], "colno") : JS_UNDEFINED;
            JS_SetPropertyStr(ctx, obj, "colno", JS_IsUndefined(cv)
                ? JS_NewInt32(ctx, 0) : cv);
            JSValue ev2 = has_init
                ? JS_GetPropertyStr(ctx, argv[1], "error") : JS_UNDEFINED;
            JS_SetPropertyStr(ctx, obj, "error", ev2);
        }
        if (cname) JS_FreeCString(ctx, cname);
        JS_FreeValue(ctx, cnamev);
    }
    if (argc >= 2 && JS_IsObject(argv[1])) {
        JSValue b = JS_GetPropertyStr(ctx, argv[1], "bubbles");
        JS_SetPropertyStr(ctx, obj, "bubbles", JS_NewBool(ctx, JS_ToBool(ctx, b)));
        JS_FreeValue(ctx, b);
        JSValue c = JS_GetPropertyStr(ctx, argv[1], "cancelable");
        JS_SetPropertyStr(ctx, obj, "cancelable", JS_NewBool(ctx, JS_ToBool(ctx, c)));
        JS_FreeValue(ctx, c);
        JSValue cp = JS_GetPropertyStr(ctx, argv[1], "composed");
        JS_SetPropertyStr(ctx, obj, "composed", JS_NewBool(ctx, JS_ToBool(ctx, cp)));
        JS_FreeValue(ctx, cp);
        JS_SetPropertyStr(ctx, obj, "detail",
                          JS_GetPropertyStr(ctx, argv[1], "detail"));
    } else {
        JS_SetPropertyStr(ctx, obj, "bubbles", JS_FALSE);
        JS_SetPropertyStr(ctx, obj, "cancelable", JS_FALSE);
        JS_SetPropertyStr(ctx, obj, "composed", JS_FALSE);
    }
    JS_SetPropertyStr(ctx, obj, "defaultPrevented", JS_FALSE);
    ns_bind_fn(ctx, obj, "preventDefault",            ns_event_prevent_default, 0);
    ns_bind_fn(ctx, obj, "stopPropagation",           ns_event_stop_propagation, 0);
    ns_event_define_cancel_bubble(ctx, obj);
    ns_bind_fn(ctx, obj, "stopImmediatePropagation",  ns_event_stop_immediate, 0);
    ns_bind_fn(ctx, obj, "composedPath",              ns_event_composed_path,    0);
    return obj;
}

static void
ns_net_add_private_names(JSContext *ctx)
{
    static const char *const names[] = {
        "__ndBlobBytes", "__ndBlobType", "__ndFileName", "__ndFileMtime",
        "__ndHeaderMap", "__ndSetCookies", "__ndPairs", "__ndOwner",
        "__ndNotify", "__ndReady", "__ndSync", "__ndSetSearchRaw", "__nd",
        "__ndSP", "__ndMediaSourceObjectURL", "__ns_port_bridge",
        "__ns_broadcast_channels",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(names); i++)
        JS_AddEnginePrivateName(ctx, names[i]);
}

static size_t
ns_worker_stack_limit(void)
{
    size_t limit = (size_t)5 * 1024 * 1024;
    size_t stack = 0;
#if defined(__APPLE__)
    stack = pthread_get_stacksize_np(pthread_self());
#elif defined(G_OS_WIN32)
    stack = (size_t)1024 * 1024;
#endif
    if (stack > 0 && stack - stack / 4 < limit)
        limit = stack - stack / 4;
    return limit;
}

/* The names the engine keeps on platform objects for itself, which page
 * scripts must neither see nor clash with (JS_AddEnginePrivateName). */
static void
ns_js_add_engine_private_names(JSContext *ctx)
{
    static const char *const names[] = { "_listeners",
                                         "__ndAdoptWindowEventOps",
                                         "__ndEventTargetMethods",
                                         "__ndIsEngineFunction",
                                         "__ndDispatchPath",
                                         "__ndRealmProto",
                                         "__ndAdoptCss" };
    for (gsize i = 0; i < G_N_ELEMENTS(names); i++)
        JS_AddEnginePrivateName(ctx, names[i]);
    ns_net_add_private_names(ctx);
}


/* An interface object's [[Prototype]] is its parent interface object, as
 * its prototype object's is the parent's prototype object (WebIDL); many
 * were left at Function.prototype. */
static const char ns_interface_ctor_links_src[] =
    "(function(G){"
    "  var FP = Function.prototype, OP = Object.prototype, gp = Object.getPrototypeOf,"
    "      gopd = Object.getOwnPropertyDescriptor;"
    "  Object.getOwnPropertyNames(G).forEach(function(n){"
    "    if (!/^[A-Z]/.test(n)) return;"
    "    var d = gopd(G, n); if (!d || typeof d.value !== 'function') return;"
    "    var F = d.value, P = F.prototype;"
    "    if (!P || typeof P !== 'object' || gp(F) !== FP) return;"
    "    var PP = gp(P); if (!PP || PP === OP) return;"
    "    var c = gopd(PP, 'constructor');"
    "    if (!c || typeof c.value !== 'function' || c.value.prototype !== PP) return;"
    /* DOMException's prototype inherits Error.prototype, but as an interface
     * without a parent its interface object inherits Function.prototype. */
    "    if (c.value === G.Error) return;"
    "    try { Object.setPrototypeOf(F, c.value); } catch (e) {}"
    "  });"
    "})(globalThis)";

void
ns_js_link_interface_ctors(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_interface_ctor_links_src,
                        sizeof(ns_interface_ctor_links_src) - 1,
                        "<interface-ctor-links>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

/* The global object and the prototypes it inherits are immutable prototype
 * exotic objects for page scripts (WebIDL [Global]). */
void
ns_js_lock_global_prototypes(JSContext *ctx)
{
    JSValue o = JS_GetGlobalObject(ctx);
    for (int depth = 0; JS_IsObject(o) && depth < 8; depth++) {
        JS_SetImmutablePrototype(ctx, o);
        JSValue next = JS_GetPrototype(ctx, o);
        JS_FreeValue(ctx, o);
        o = next;
    }
    JS_FreeValue(ctx, o);
}

/* The window's prototype chain as WebIDL has a [Global] interface's:
 * Window.prototype > WindowProperties > EventTarget.prototype; Window's
 * members are the window's own, so Window.prototype keeps only its tag, and
 * EventTarget's methods serve windows through the registry et_src keeps. */
static const char ns_window_global_shape_src[] =
    "(function(G){"
    "  var gp = Object.getPrototypeOf, sp = Object.setPrototypeOf,"
    "      def = Object.defineProperty, gopd = Object.getOwnPropertyDescriptor;"
    "  var ETC = G.EventTarget, WC = G.Window;"
    "  if (typeof ETC !== 'function' || typeof WC !== 'function') return;"
    "  var ET = ETC.prototype, W = WC.prototype, named = gp(W);"
    "  if (named === Object.prototype || named === null) {"
    "    named = Object.create(ET);"
    "    try { sp(W, named); } catch (e) {}"
    "  } else if (named !== ET) {"
    "    try { sp(named, ET); } catch (e) {}"
    "  }"
    "  if (named !== ET)"
    "    try { def(named, Symbol.toStringTag, { value: 'WindowProperties', configurable: true }); } catch (e) {}"
    "  Object.getOwnPropertyNames(W).forEach(function(k){"
    "    if (k !== 'constructor') try { delete W[k]; } catch (e) {} });"
    "  Object.getOwnPropertySymbols(W).forEach(function(k){ try { delete W[k]; } catch (e) {} });"
    "  def(W, Symbol.toStringTag, { value: 'Window', configurable: true });"
    "  try { delete G[Symbol.toStringTag]; } catch (e) {}"
    "  var adopt = G.__ndAdoptWindowEventOps;"
    "  if (typeof adopt === 'function') adopt(G);"
    "  [['addEventListener', 2], ['removeEventListener', 2], ['dispatchEvent', 1]].forEach(function(m){"
    "    var d = gopd(ET, m[0]);"
    "    if (d && typeof d.value === 'function')"
    "      try { def(d.value, 'length', { value: m[1], configurable: true }); } catch (e) {} });"
    "})(globalThis)";

static void
ns_js_shape_window_global(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_window_global_shape_src,
                        sizeof(ns_window_global_shape_src) - 1,
                        "<window-global-shape>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
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

static JSValue
ns_is_engine_function(JSContext *ctx, JSValueConst this_val, int argc,
                      JSValueConst *argv)
{
    (void)this_val;
    return JS_NewBool(ctx, argc > 0 && JS_IsEngineFunction(argv[0]));
}

ns_js *
ns_worker_js_new(const ns_worker_realm *p)
{
    ns_js *js = g_new0(ns_js, 1);
    js->time_origin_us = (g_get_monotonic_time() / 100) * 100;
    js->time_origin_real_ms = floor((double)g_get_real_time() / 100.0) / 10.0;
    js->iframe_doc = JS_UNDEFINED;
    js->pristine_promise = JS_UNDEFINED;
    js->main_context = p->context;
    js->worker_host = p->host;
    js->partition_key = (p->origin && *p->origin)
        ? g_strdup(p->origin) : NULL;
    js->log_cb = ns_worker_log_cb;
    js->log_user_data = p->host;
    js->pinned_wrappers_set = g_hash_table_new(g_direct_hash, g_direct_equal);
    ns_perf_init(js);

    js->rt = JS_NewRuntime();
    if (!js->rt) {
        ns_perf_teardown(js);
        if (js->pinned_wrappers_set) g_hash_table_destroy(js->pinned_wrappers_set);
        g_free(js);
        return NULL;
    }
    const ns_config *c = ns_config_get();
    int mb = c ? c->js_memory_cap_mb : 256;
    if (mb <= 0) mb = 256;
    if (mb > 512) mb = 512;
    JS_SetInterruptHandler(js->rt, ns_js_interrupt_cb, js);
    JS_SetMemoryLimit(js->rt, (size_t)mb * 1024 * 1024);
    JS_SetHostPromiseRejectionTracker(js->rt, ns_worker_promise_rejection_tracker, NULL);
    JS_SetMaxStackSize(js->rt, ns_worker_stack_limit());

    js->ctx = JS_NewContext(js->rt);
    if (js->ctx) ns_js_add_engine_private_names(js->ctx);
    if (!js->ctx) {
        JS_FreeRuntime(js->rt);
        ns_perf_teardown(js);
        if (js->pinned_wrappers_set) g_hash_table_destroy(js->pinned_wrappers_set);
        g_free(js);
        return NULL;
    }
    js->main_realm_ctx = js->ctx;
    JS_SetContextOpaque(js->ctx, js);
    JS_SetRuntimeOpaque(js->rt, js);
    JS_SetModuleLoaderFunc2(js->rt, ns_js_module_normalize,
                            ns_js_module_loader, NULL, js);
    JSContext *ctx = js->ctx;
    JSValue global = JS_GetGlobalObject(ctx);

    ns_worker_install_console(ctx, global);
    ns_bind_fn(ctx, global, "setTimeout",    ns_services_set_timeout, 2);
    ns_bind_fn(ctx, global, "setInterval",   ns_services_set_interval, 2);
    ns_bind_fn(ctx, global, "clearTimeout",  ns_services_clear_timer, 1);
    ns_bind_fn(ctx, global, "clearInterval", ns_services_clear_timer, 1);
    ns_bind_fn(ctx, global, "postMessage",   ns_worker_global_post_message, 1);
    ns_bind_fn(ctx, global, "close",         ns_worker_global_close, 0);
    ns_bind_fn(ctx, global, "importScripts", ns_worker_import_scripts, 1);
    ns_bind_fn(ctx, global, "structuredClone", ns_window_structured_clone, 1);
    ns_bind_fn(ctx, global, "queueMicrotask", ns_services_queue_microtask, 1);
    ns_bind_fn(ctx, global, "btoa", ns_window_btoa, 1);
    ns_bind_fn(ctx, global, "atob", ns_window_atob, 1);
    ns_bind_ctor(ctx, global, "MessageChannel", ns_window_message_channel, 0);
    ns_bind_ctor(ctx, global, "MessagePort", ns_illegal_constructor, 0);

    ns_bind_ctor(ctx, global, "Event", ns_window_event_ctor, 1);
    ns_bind_ctor(ctx, global, "MessageEvent", ns_window_event_ctor, 1);
    ns_bind_ctor(ctx, global, "ErrorEvent", ns_window_event_ctor, 1);
    ns_events_install_worker(ctx, global);
    ns_bind_ctor(ctx, global, "EventTarget", ns_window_event_ctor, 0);
    ns_bind_ctor(ctx, global, "TextEncoder", ns_window_text_encoder_ctor, 0);
    ns_bind_ctor(ctx, global, "TextDecoder", ns_window_text_decoder_ctor, 0);
    ns_net_install_text_codecs(ctx, global);
    ns_bind_ctor(ctx, global, "URLSearchParams", ns_window_usp_ctor, 0);
    ns_usp_install_interface(ctx);
    ns_bind_ctor(ctx, global, "FormData", ns_window_form_data_ctor, 0);
    ns_bind_ctor(ctx, global, "FileReader", ns_window_filereader_ctor, 0);
    JSValue url_ctor = ns_make_ctor(ctx, ns_window_url_ctor, "URL", 1);
    ns_bind_fn(ctx, url_ctor, "canParse", ns_window_url_can_parse, 1);
    ns_bind_fn(ctx, url_ctor, "parse", ns_window_url_parse_static, 1);
    ns_bind_fn(ctx, url_ctor, "createObjectURL", ns_window_url_create_object, 1);
    ns_bind_fn(ctx, url_ctor, "revokeObjectURL", ns_window_url_revoke_object, 1);
    JS_SetPropertyStr(ctx, global, "URL", url_ctor);
    ns_url_install_interface(ctx);

#ifdef NS_HAVE_WASM
    ns_wasm_install(ctx, global);
#endif
    ns_js_intl_install(ctx, global);
    ns_js_temporal_install(ctx, global);
    JS_SetPropertyStr(ctx, global, "crossOriginIsolated", JS_FALSE);
    ns_hide_shared_array_buffer(ctx, global);
    ns_bind_ctor(ctx, global, "XMLHttpRequestEventTarget", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "XMLHttpRequestUpload", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "XMLHttpRequest", ns_window_xhr_ctor, 0);
    ns_xhr_install_interface(ctx, global);
    ns_bind_fn(ctx, global, "fetch",    ns_js_fetch,             1);
    ns_bind_ctor(ctx, global, "Response", ns_window_response_ctor, 0);
    ns_bind_ctor(ctx, global, "Request",  ns_window_request_ctor,  1);
    ns_fetch_install_interfaces(ctx, global);
    JS_SetPropertyStr(ctx, global, "__ndWorkerHeadersOnly", JS_TRUE);
    ns_bind_ctor(ctx, global, "AbortController",
                 ns_window_abort_controller_ctor, 0);
    ns_install_abort_signal_interface(ctx, global);
    ns_net_install_interfaces(ctx, global);
    ns_idb_install(ctx, global);
    ns_js_eval(js, ns_js_polyfills_src,
               sizeof(ns_js_polyfills_src) - 1, "<worker-polyfills>");
    {
        JSValue g = JS_GetGlobalObject(ctx);
        ns_install_event_attribute_getters(ctx, g);
        JS_FreeValue(ctx, g);
    }
    ns_js_name_engine_members(ctx);
    ns_crypto_install_worker(ctx, global);
    ns_webgl_install(ctx, global);
    ns_canvas_register_classes(js->rt);
    ns_canvas_install(ctx, global, FALSE);

    JS_SetPropertyStr(ctx, global, "self", JS_DupValue(ctx, global));
    JS_SetPropertyStr(ctx, global, "globalThis", JS_DupValue(ctx, global));
    ns_bind_ctor(ctx, global, "WorkerGlobalScope", ns_illegal_constructor, 0);
    if (!p->is_service_worker)
        ns_bind_ctor(ctx, global, "DedicatedWorkerGlobalScope",
                     ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "WorkerLocation", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "WorkerNavigator", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "NavigatorUAData", ns_illegal_constructor, 0);
    ns_bind_fn(ctx, global, "reportError", ns_worker_report_error, 1);
    JS_SetPropertyStr(ctx, global, "_listeners", JS_NewArray(ctx));
    ns_bind_event_target_listeners(ctx, global);
    ns_bind_fn(ctx, global, "dispatchEvent",       ns_target_dispatchEvent, 1);
    JS_SetPropertyStr(ctx, global, "onmessage", JS_NULL);
    JS_SetPropertyStr(ctx, global, "onmessageerror", JS_NULL);
    JS_SetPropertyStr(ctx, global, "onerror", JS_NULL);

    JS_SetPropertyStr(ctx, global, "navigator",
                      ns_services_worker_navigator(ctx));

    JSValue performance = JS_NewObject(ctx);
    ns_bind_fn(ctx, performance, "now", ns_worker_performance_now, 0);
    ns_bind_fn(ctx, performance, "getEntries", ns_worker_performance_entries, 0);
    ns_bind_fn(ctx, performance, "getEntriesByType",
               ns_worker_performance_entries, 1);
    ns_bind_fn(ctx, performance, "getEntriesByName",
               ns_worker_performance_entries, 1);
    ns_bind_fn(ctx, performance, "clearMarks", ns_worker_performance_clear, 0);
    ns_bind_fn(ctx, performance, "clearMeasures", ns_worker_performance_clear, 0);
    JS_SetPropertyStr(ctx, performance, "timeOrigin",
                      JS_NewFloat64(ctx, js->time_origin_real_ms));
    JS_SetPropertyStr(ctx, global, "performance", performance);
    JS_SetPropertyStr(ctx, global, "origin",
                      JS_NewString(ctx, p->origin ? p->origin : ""));
    JS_SetPropertyStr(ctx, global, "name",
                      JS_NewString(ctx, p->name ? p->name : ""));
    if (p->origin && strcmp(p->origin, "null") == 0) {
        /* A worker with an opaque origin, such as a data: worker, has no
         * storage of its own: opening a database throws SecurityError. */
        static const char deny_idb[] =
            "(function(){var f=self.indexedDB;if(!f)return;"
            "function deny(){throw new DOMException("
            "'Access to IndexedDB is denied for an opaque origin.',"
            "'SecurityError');}"
            "['open','deleteDatabase','databases'].forEach(function(m){"
            "try{Object.defineProperty(f,m,{value:deny,configurable:true,"
            "writable:true});}catch(e){}});})()";
        JSValue r = JS_Eval(ctx, deny_idb, sizeof deny_idb - 1, "<worker-origin>",
                            JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, r);
    }
    const char *secure_url = p->url && g_str_has_prefix(p->url, "blob:")
        ? p->base_url : p->url;
    JS_SetPropertyStr(ctx, global, "isSecureContext",
                      secure_url && g_str_has_prefix(secure_url, "https:") ? JS_TRUE : JS_FALSE);
    ns_worker_install_location(ctx, global, p->url);

    if (p->is_service_worker)
        ns_sw_install_scope(ctx, global);
    ns_worker_shape_global(ctx, p->is_service_worker);

    JS_FreeValue(ctx, global);
    return js;
}

ns_worker_host *
ns_js_worker_host(const ns_js *js)
{
    return js ? js->worker_host : NULL;
}

void
ns_js_halt(ns_js *js)
{
    if (js) js->halted = TRUE;
}

gboolean
ns_js_csp_allows_worker(ns_js *js, const char *url)
{
    return !js->csp || ns_csp_allows(js->csp, NS_CSP_WORKER, url, js->current_url);
}

int
ns_worker_js_eval(ns_js *js, const char *src, gsize len, const char *url,
                  gboolean module, JSValue *exception)
{
    *exception = JS_UNDEFINED;
    if (!js || !js->ctx) return 2;
    JSContext *ctx = js->ctx;
    gboolean parse_error = FALSE;
    JSValue v;
    if (module) {
        JSValue fn = ns_js_compile_module_cached(ctx, src ? src : "", len,
                                                 url ? url : "<worker>");
        if (JS_IsException(fn)) parse_error = TRUE;
        if (!JS_IsException(fn) && ns_js_module_set_import_meta(ctx, fn, TRUE) < 0) {
            JS_FreeValue(ctx, fn);
            fn = JS_EXCEPTION;
        }
        v = JS_IsException(fn) ? fn : JS_EvalFunction(ctx, fn);
        if (!JS_IsException(v) && JS_PromiseState(ctx, v) == JS_PROMISE_REJECTED) {
            JSValue reason = JS_PromiseResult(ctx, v);
            JS_FreeValue(ctx, v);
            v = JS_Throw(ctx, reason);
        }
    } else {
        v = JS_Eval(ctx, src ? src : "", len, url ? url : "<worker>",
                    JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_COMPILE_ONLY);
        if (JS_IsException(v)) parse_error = TRUE;
        else v = JS_EvalFunction(ctx, v);
    }
    int status = 0;
    if (JS_IsException(v)) {
        *exception = JS_GetException(ctx);
        status = parse_error ? 1 : 2;
    }
    JS_FreeValue(ctx, v);
    return status;
}

static ns_node *
ns_node_scope_document(ns_node *node)
{
    for (ns_node *p = node; p; p = p->parent)
        if (p->kind == NS_NODE_DOCUMENT && !(p->flags & NS_NODE_FRAGMENT))
            return p;
    return NULL;
}

/* The parent of n within its own node tree: a shadow root and a frame's
 * document are roots there, whatever node holds them in the engine. */
static const ns_node *
ns_dom_tree_parent(const ns_node *n)
{
    if (!n || ns_node_is_shadow_root(n) || ns_node_is_embedded_doc(n))
        return NULL;
    return n->parent;
}

/* True when n belongs to a shadow tree rather than its document's tree. */
static gboolean
ns_node_in_shadow_tree(const ns_node *n)
{
    for (const ns_node *p = n; p; p = p->parent) {
        if (ns_node_is_shadow_root(p)) return TRUE;
        if (p->kind == NS_NODE_DOCUMENT) return FALSE;
    }
    return FALSE;
}

void
ns_js_index_child_change(ns_js *js, ns_node *parent,
                         ns_node *added, ns_node *removed)
{
    ns_qcache_invalidate(js);
    ns_node *doc = ns_node_scope_document(parent);
    /* The document's id, class and tag indexes cover its own tree only;
     * nodes inserted into a shadow tree stay out of them. */
    if (js && doc && !ns_node_in_shadow_tree(parent)) {
        if (removed) {
            ns_doc_id_index_subtree_removed   (doc, removed);
            ns_doc_class_index_subtree_removed(doc, removed);
            ns_doc_tag_index_subtree_removed  (doc, removed);
        }
        if (added) {
            ns_doc_id_index_subtree_added   (doc, added);
            ns_doc_class_index_subtree_added(doc, added);
            ns_doc_tag_index_subtree_added  (doc, added);
            const ns_node *form = ns_form_owner(added, NULL);
            JSContext *ctx = js ? (js->ctx ? js->ctx : js->main_realm_ctx) : NULL;
            if (form && form->js_wrapper && ctx) {
                JSValue form_val = JS_MKPTR(JS_TAG_OBJECT, form->js_wrapper);
                JSValue past_map = JS_GetPropertyStr(ctx, form_val, "_ns_past_names");
                if (!JS_IsObject(past_map)) {
                    past_map = JS_NewObject(ctx);
                    JS_DefinePropertyValueStr(ctx, form_val, "_ns_past_names", JS_DupValue(ctx, past_map), 0);
                }
                const char *nm = ns_element_get_attr(added, "name");
                if (nm && *nm)
                    JS_DefinePropertyValueStr(ctx, past_map, nm, ns_make_element(ctx, added), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
                const char *id = ns_element_get_attr(added, "id");
                if (id && *id)
                    JS_DefinePropertyValueStr(ctx, past_map, id, ns_make_element(ctx, added), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
                JS_FreeValue(ctx, past_map);
            }
        }
    }
}

void
ns_js_record_child_change(ns_js *js, ns_node *parent,
                          ns_node *added, ns_node *removed,
                          ns_node *previous_sibling, ns_node *next_sibling)
{
    ns_js_index_child_change(js, parent, added, removed);
    ns_css_mark_childlist_dirty(parent, added);
    ns_mut_record_emit(js, "childList", parent, added, removed,
                       previous_sibling, next_sibling, NULL, NULL, NULL);
}

void
ns_js_record_child_change_arrays(ns_js *js, ns_node *parent,
                                 GPtrArray *added, GPtrArray *removed,
                                 ns_node *previous_sibling,
                                 ns_node *next_sibling)
{
    ns_qcache_invalidate(js);
    ns_node *doc = ns_node_scope_document(parent);
    if (js && doc && !ns_node_in_shadow_tree(parent)) {
        if (removed)
            for (guint i = 0; i < removed->len; i++) {
                ns_node *n = g_ptr_array_index(removed, i);
                ns_doc_id_index_subtree_removed   (doc, n);
                ns_doc_class_index_subtree_removed(doc, n);
                ns_doc_tag_index_subtree_removed  (doc, n);
            }
        if (added)
            for (guint i = 0; i < added->len; i++) {
                ns_node *n = g_ptr_array_index(added, i);
                ns_doc_id_index_subtree_added   (doc, n);
                ns_doc_class_index_subtree_added(doc, n);
                ns_doc_tag_index_subtree_added  (doc, n);
                const ns_node *form = ns_form_owner(n, NULL);
                JSContext *ctx = js ? (js->ctx ? js->ctx : js->main_realm_ctx) : NULL;
                if (form && form->js_wrapper && ctx) {
                    JSValue form_val = JS_MKPTR(JS_TAG_OBJECT, form->js_wrapper);
                    JSValue past_map = JS_GetPropertyStr(ctx, form_val, "_ns_past_names");
                    if (!JS_IsObject(past_map)) {
                        past_map = JS_NewObject(ctx);
                        JS_DefinePropertyValueStr(ctx, form_val, "_ns_past_names", JS_DupValue(ctx, past_map), 0);
                    }
                    const char *nm = ns_element_get_attr(n, "name");
                    if (nm && *nm)
                        JS_DefinePropertyValueStr(ctx, past_map, nm, ns_make_element(ctx, n), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
                    const char *id = ns_element_get_attr(n, "id");
                    if (id && *id)
                        JS_DefinePropertyValueStr(ctx, past_map, id, ns_make_element(ctx, n), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
                    JS_FreeValue(ctx, past_map);
                }
            }
    }
    ns_node *first_added = added && added->len > 0
        ? g_ptr_array_index(added, 0) : NULL;
    ns_css_mark_childlist_dirty(parent, first_added);
    ns_mut_record_emit_child_list_arrays(js, parent, added, removed,
                                         previous_sibling, next_sibling);
}

static gboolean
ns_attr_invalidates_qcache(const char *name)
{
    static const char *const names[] = { "id", "class", "form", "name" };
    for (gsize i = 0; i < G_N_ELEMENTS(names); i++)
        if (g_ascii_strcasecmp(name, names[i]) == 0) return TRUE;
    return FALSE;
}

static void
ns_js_record_attr_change_ns(ns_js *js, ns_node *target,
                            const char *name, const char *namespace_uri,
                            const char *old_value)
{
    if (js && name && ns_attr_invalidates_qcache(name))
        ns_qcache_invalidate(js);
    ns_node *doc = ns_node_scope_document(target);
    if (doc && ns_node_in_shadow_tree(target)) doc = NULL;
    if (js && doc && target && name &&
        g_ascii_strcasecmp(name, "id") == 0) {
        if (old_value && *old_value)
            ns_doc_id_index_unregister(doc, old_value, target);
        const char *new_id = ns_element_get_attr(target, "id");
        if (new_id && *new_id)
            ns_doc_id_index_register(doc, new_id, target);
    }
    if (js && doc && target && name &&
        g_ascii_strcasecmp(name, "class") == 0) {
        if (old_value && *old_value)
            ns_doc_class_index_unregister(doc, old_value, target);
        const char *new_cls = ns_element_get_attr(target, "class");
        if (new_cls && *new_cls)
            ns_doc_class_index_register(doc, new_cls, target);
    }
    if (js && target && name &&
        (g_ascii_strcasecmp(name, "id") == 0 ||
         g_ascii_strcasecmp(name, "name") == 0)) {
        const ns_node *form = ns_form_owner(target, NULL);
        JSContext *ctx = js ? (js->ctx ? js->ctx : js->main_realm_ctx) : NULL;
        if (form && form->js_wrapper && ctx) {
            JSValue form_val = JS_MKPTR(JS_TAG_OBJECT, form->js_wrapper);
            JSValue past_map = JS_GetPropertyStr(ctx, form_val, "_ns_past_names");
            if (!JS_IsObject(past_map)) {
                past_map = JS_NewObject(ctx);
                JS_DefinePropertyValueStr(ctx, form_val, "_ns_past_names", JS_DupValue(ctx, past_map), 0);
            }
            if (old_value && *old_value) {
                JS_DefinePropertyValueStr(ctx, past_map, old_value, ns_make_element(ctx, target), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
            }
            const char *cur = ns_element_get_attr(target, name);
            if (cur && *cur) {
                JS_DefinePropertyValueStr(ctx, past_map, cur, ns_make_element(ctx, target), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
            }
            JS_FreeValue(ctx, past_map);
        }
    }
    ns_css_mark_attr_dirty(target, name, old_value);
    ns_mut_record_emit(js, "attributes", target, NULL, NULL,
                       NULL, NULL, name, namespace_uri, old_value);
}

static void
ns_js_record_attr_change(ns_js *js, ns_node *target,
                         const char *name, const char *old_value)
{
    ns_js_record_attr_change_ns(js, target, name, NULL, old_value);
}

void
ns_js_record_character_data(ns_js *js, ns_node *target, const char *old_value)
{
    ns_css_mark_restyle_dirty(target && target->parent ? target->parent : target);
    ns_mut_record_emit(js, "characterData", target, NULL, NULL,
                       NULL, NULL, NULL, NULL, old_value);
}

void
ns_js_set_attr_recorded_len(ns_js *js, ns_node *n, const char *name,
                            const char *value, gssize len)
{
    if (!n || !name) return;
    gsize vlen = len < 0 ? (value ? strlen(value) : 0) : (gsize)len;
    const char *new_value = value ? value : "";
    gsize old_len = 0;
    const char *old = ns_element_get_attr_len(n, name, &old_len);
    gboolean changed = !(old && old_len == vlen &&
                         memcmp(old, new_value, vlen) == 0);
    char *old_copy = old ? ns_value_dup_len(old, old_len) : NULL;
    ns_element_set_attr_len(n, name, new_value, (gssize)vlen);
    if (js) {
        if (changed) {
            if (ns_css_attr_may_affect_style(n, name))
                js->mutated = TRUE;
        }
        ns_js_record_attr_change(js, n, name, old_copy);
        ns_ce_attr_changed(js, n, name, old_copy, new_value);
    }
    g_free(old_copy);
}

void
ns_js_set_attr_recorded(ns_js *js, ns_node *n, const char *name, const char *value)
{
    ns_js_set_attr_recorded_len(js, n, name, value, -1);
}

void
ns_js_set_attr_ns_recorded(ns_js *js, ns_node *n, const char *namespace_uri,
                           const char *prefix, const char *local_name,
                           const char *name, const char *value)
{
    if (!n || !local_name || !name) return;
    const char *new_value = value ? value : "";
    const ns_attr *old_attr = ns_element_find_attr_ns(n, namespace_uri,
                                                       local_name);
    const char *record_name = old_attr && old_attr->name ? old_attr->name : name;
    const char *old = old_attr ? old_attr->value : NULL;
    gboolean changed = !(old && strcmp(old, new_value) == 0);
    char *old_copy = old ? g_strdup(old) : NULL;
    char *record_copy = g_strdup(record_name);
    ns_element_set_attr_ns(n, namespace_uri, prefix, local_name, name, new_value);
    if (js) {
        if (changed && ns_css_attr_may_affect_style(n, record_copy))
            js->mutated = TRUE;
        ns_js_record_attr_change_ns(js, n, local_name, namespace_uri,
                                    old_copy);
        ns_ce_attr_changed(js, n, record_copy, old_copy, new_value);
    }
    g_free(record_copy);
    g_free(old_copy);
}

void
ns_js_remove_attr_recorded(ns_js *js, ns_node *n, const char *name)
{
    if (!n || !name) return;
    const ns_attr *old_attr = ns_element_find_attr(n, name);
    if (!old_attr) return;
    const char *old = old_attr->value;
    char *old_copy = g_strdup(old);
    ns_attr_detach_matching(js, n, old_attr->namespace_uri,
                            ns_attr_local_name(old_attr));
    ns_element_remove_attr(n, name);
    if (js) {
        if (ns_css_attr_may_affect_style(n, name)) js->mutated = TRUE;
        ns_js_record_attr_change(js, n, name, old_copy);
        ns_ce_attr_changed(js, n, name, old_copy, NULL);
    }
    g_free(old_copy);
}

void
ns_js_remove_attr_ns_recorded(ns_js *js, ns_node *n, const char *namespace_uri,
                              const char *local_name)
{
    if (!n || !local_name) return;
    const ns_attr *old_attr = ns_element_find_attr_ns(n, namespace_uri,
                                                       local_name);
    if (!old_attr) return;
    char *old_copy = g_strdup(old_attr->value ? old_attr->value : "");
    char *record_copy = g_strdup(old_attr->name ? old_attr->name : local_name);
    ns_attr_detach_matching(js, n, namespace_uri, local_name);
    ns_element_remove_attr_ns(n, namespace_uri, local_name);
    if (js) {
        if (ns_css_attr_may_affect_style(n, record_copy)) js->mutated = TRUE;
        ns_js_record_attr_change_ns(js, n, local_name, namespace_uri,
                                    old_copy);
        ns_ce_attr_changed(js, n, record_copy, old_copy, NULL);
    }
    g_free(record_copy);
    g_free(old_copy);
}

JSValue
ns_js_call_observer(ns_js *js, JSContext *ctx, JSValueConst cb,
                    JSValueConst this_val, int argc, JSValueConst *argv,
                    const char *report_type, gboolean fresh_budget)
{
    gint64 saved_deadline = js ? js->eval_deadline_us : 0;
    if (js && fresh_budget)
        js->eval_deadline_us = g_get_monotonic_time() + ns_js_eval_budget_us();
    ns_realm_scope scope;
    ns_js_realm_scope_enter(js, ns_function_realm(ctx, cb), &scope);
    JSValue ret = JS_Call(ctx, cb, this_val, argc, argv);
    if (report_type && JS_IsException(ret)) {
        ns_target_report_exception(js, ctx, report_type);
        ret = JS_UNDEFINED;
    }
    ns_js_realm_scope_leave(js, &scope);
    if (js && fresh_budget) js->eval_deadline_us = saved_deadline;
    return ret;
}

static gboolean
ns_observer_tick_timer(gpointer data)
{
    ns_js *js = data;
    if (!js) return G_SOURCE_REMOVE;
    if (!js->ctx) { js->observer_tick_source = 0; return G_SOURCE_REMOVE; }
    if (js->in_pump || js->dispatch_depth > 0)
        return G_SOURCE_CONTINUE;
    js->observer_tick_source = 0;
    ns_intersection_observers_tick(js);
    ns_resize_observers_tick(js);
    ns_drain_microtasks(js);
    return G_SOURCE_REMOVE;
}

void
ns_observer_schedule_tick(ns_js *js)
{
    if (!js || !js->ctx || js->worker_host || js->observer_tick_source) return;
    js->observer_tick_source = g_timeout_add(4, ns_observer_tick_timer, js);
}

static gboolean ns_js_run_animation_frame_internal(ns_js *js);

static gboolean
ns_raf_tick_timer(gpointer data)
{
    ns_js *js = data;
    if (!js) return G_SOURCE_REMOVE;
    if (!js->ctx || js->halted) {
        js->raf_tick_source = 0;
        return G_SOURCE_REMOVE;
    }
    if (js->in_pump || js->dispatch_depth > 0 || ns_engine_in_blocking_fetch())
        return G_SOURCE_CONTINUE;
    if (!js->raf_pending || js->raf_pending->len == 0) {
        js->raf_tick_source = 0;
        return G_SOURCE_REMOVE;
    }
    if (js->raf_host_driven) {
        js->raf_tick_source = 0;
        return G_SOURCE_REMOVE;
    }
    ns_js_run_animation_frame_internal(js);
    if (js->raf_pending && js->raf_pending->len > 0)
        return G_SOURCE_CONTINUE;
    js->raf_tick_source = 0;
    return G_SOURCE_REMOVE;
}

static void
ns_raf_schedule_tick(ns_js *js)
{
    if (!js || !js->ctx || js->worker_host || js->raf_tick_source ||
        js->raf_host_driven) return;
    js->raf_tick_source = g_timeout_add(16, ns_raf_tick_timer, js);
}

JSValue
ns_window_requestAnimationFrame(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv)
{
    (void)this_val;
    if (!js_from_ctx(ctx) || argc < 1 || !JS_IsFunction(ctx, argv[0]))
        return JS_NewInt32(ctx, 0);
    ns_js *js = js_from_ctx(ctx);
    if (!js->raf_pending)
        js->raf_pending = g_array_new(FALSE, FALSE, sizeof(ns_raf_entry));
    ns_raf_entry e = {
        .id = 0x40000000 + (++js->next_raf_id),
        .ctx = ctx,
        .cb = JS_DupValue(ctx, argv[0]),
        .video_frame = FALSE,
        .frame = ns_js_context_frame(js, ctx)
    };
    g_array_append_val(js->raf_pending, e);
    ns_raf_schedule_tick(js);
    return JS_NewInt32(ctx, e.id);
}

JSValue
ns_window_cancelAnimationFrame(JSContext *ctx, JSValueConst this_val,
                               int argc, JSValueConst *argv)
{
    (void)this_val;
    if (!js_from_ctx(ctx) || argc < 1) return JS_UNDEFINED;
    int32_t id = 0;
    JS_ToInt32(ctx, &id, argv[0]);
    ns_js *js = js_from_ctx(ctx);
    if (!js->raf_pending) return JS_UNDEFINED;
    for (guint i = 0; i < js->raf_pending->len; i++) {
        ns_raf_entry *e = &g_array_index(js->raf_pending, ns_raf_entry, i);
        if (e->id == id) {
            JS_FreeValue(e->ctx ? e->ctx : ctx, e->cb);
            g_array_remove_index(js->raf_pending, i);
            return JS_UNDEFINED;
        }
    }
    return JS_UNDEFINED;
}





/* Only the outermost dispatch drains mutations and microtasks; a dispatch
 * from inside a script or a microtask leaves that to its caller. */
void
ns_js_dispatch_finish(ns_js *js)
{
    if (js->eval_depth == 0 && js->callback_depth == 0 && !js->in_pump) {
        ns_drain_mutations(js);
    } else {
        if (js->mutated && js->mut_cb)
            js->mut_cb(js->mut_user_data);
        js->mutated = FALSE;
        ns_storage_schedule_flush(js);
    }
}

void
ns_js_set_style_table(ns_js *js, GHashTable *styles)
{
    if (!js) return;
    js->style_table = styles;
}

void
ns_js_queue_scrollend(ns_js *js, const ns_node *el)
{
    if (!js || !el) return;
    if (!js->pending_scrollend)
        js->pending_scrollend = g_ptr_array_new();
    for (guint i = 0; i < js->pending_scrollend->len; i++)
        if (js->pending_scrollend->pdata[i] == (gpointer)el) return;
    g_ptr_array_add(js->pending_scrollend, (gpointer)el);
}

static void
ns_js_flush_scrollend(ns_js *js)
{
    if (!js || !js->ctx) return;
    if (!js->pending_scrollend_doc &&
        (!js->pending_scrollend || js->pending_scrollend->len == 0))
        return;
    JSContext *ctx = js->ctx;
    GPtrArray *targets = js->pending_scrollend;
    js->pending_scrollend = NULL;
    gboolean doc_pending = js->pending_scrollend_doc;
    js->pending_scrollend_doc = FALSE;
    if (targets) {
        for (guint i = 0; i < targets->len; i++) {
            const ns_node *el = targets->pdata[i];
            if (!ns_js_node_in_page(js, el)) continue;
            JSValue ev = ns_make_event(ctx, "scrollend", el);
            JS_SetPropertyStr(ctx, ev, "bubbles", JS_FALSE);
            JS_SetPropertyStr(ctx, ev, "cancelable", JS_FALSE);
            ns_js_dispatch_built_event(js, el, "scrollend", ev, NULL);
        }
        g_ptr_array_free(targets, TRUE);
    }
    if (doc_pending) {
        JSValue global = JS_GetGlobalObject(ctx);
        JSValue ev = ns_make_event(ctx, "scrollend", NULL);
        JS_SetPropertyStr(ctx, ev, "cancelable", JS_FALSE);
        JS_SetPropertyStr(ctx, ev, "target", JS_DupValue(ctx, global));
        JS_FreeValue(ctx, global);
        ns_js_dispatch_window_only_event(js, js->current_doc, "scrollend",
                                         ev, NULL);
        if (js->current_doc) {
            JSValue dev = ns_make_event(ctx, "scrollend", js->current_doc);
            JS_SetPropertyStr(ctx, dev, "cancelable", JS_FALSE);
            ns_js_dispatch_built_event(js, js->current_doc, "scrollend", dev,
                                       NULL);
        }
    }
}

gboolean
ns_js_run_animation_frame(ns_js *js)
{
    if (!js) return FALSE;
    js->raf_host_us = g_get_monotonic_time();
    return ns_js_run_animation_frame_internal(js);
}

static gboolean
ns_js_run_animation_frame_internal(ns_js *js)
{
    if (!js || js->halted || js->in_pump) return FALSE;
    ns_js_flush_scrollend(js);
    ns_js_flush_ready_images(js);
    ns_js_flush_autofocus(js);
    ns_drain_microtasks(js);
    ns_js_promote_deferred_iframes(js);
    ns_js_process_pending_iframes(js);
    ns_drain_microtasks(js);
    if (!js->raf_pending || js->raf_pending->len == 0)
        return js->mutated ? TRUE : FALSE;
    gint64 now_us = g_get_monotonic_time();
    js->raf_last_us = now_us;
    GArray *fired = js->raf_pending;
    js->raf_pending = g_array_new(FALSE, FALSE, sizeof(ns_raf_entry));
    ns_budget_guard bg = {0};
    ns_js_budget_push(js, &bg);
    js->callback_depth++;
    for (guint i = 0; i < fired->len; i++) {
        ns_raf_entry *e = &g_array_index(fired, ns_raf_entry, i);
        gboolean frame_connected = !e->frame ||
            ns_node_root(e->frame) == ns_node_root(js->current_doc);
        JSContext *current_realm = e->frame
            ? ns_js_node_realm_context(js, e->frame) : NULL;
        JSContext *callback_ctx = current_realm ? current_realm
            : (e->ctx ? e->ctx : js->ctx);
        if (!frame_connected) {
            JS_FreeValue(callback_ctx, e->cb);
            continue;
        }
        JSContext *previous_ctx = js->ctx;
        ns_node *previous_doc = js->current_doc;
        ns_node *previous_frame = js->raf_frame_ctx;
        ns_frame_url fu;
        js->ctx = callback_ctx;
        js->raf_frame_ctx = e->frame;
        if (e->frame) {
            ns_node *frame_doc = ns_iframe_document_node(e->frame);
            if (frame_doc) js->current_doc = frame_doc;
            ns_frame_url_enter(js, &fu, ns_element_get_attr(e->frame,
                                                             "data-nd-frame-url"));
        }
        js->eval_deadline_us = g_get_monotonic_time() + ns_js_eval_budget_us();
        double ts_ms = ns_perf_relative_ms(now_us,
            ns_js_time_origin_us(js, callback_ctx));
        JSValue arg = JS_NewFloat64(callback_ctx, ts_ms);
        JSValue ret;
        if (e->video_frame) {
            double media_time = 0.0;
            int32_t width = 0;
            int32_t height = 0;
            int32_t presented_frames = 0;
            JSValue media = e->media
                ? ns_make_element(callback_ctx, e->media) : JS_UNDEFINED;
            if (JS_IsObject(media)) {
                JSValue value = JS_GetPropertyStr(callback_ctx, media, "_nd_pos");
                if (JS_IsNumber(value))
                    JS_ToFloat64(callback_ctx, &media_time, value);
                JS_FreeValue(callback_ctx, value);
                value = JS_GetPropertyStr(callback_ctx, media, "videoWidth");
                if (JS_IsNumber(value)) JS_ToInt32(callback_ctx, &width, value);
                JS_FreeValue(callback_ctx, value);
                value = JS_GetPropertyStr(callback_ctx, media, "videoHeight");
                if (JS_IsNumber(value)) JS_ToInt32(callback_ctx, &height, value);
                JS_FreeValue(callback_ctx, value);
                value = JS_GetPropertyStr(callback_ctx, media,
                                          "_nd_presented_frames");
                if (JS_IsNumber(value))
                    JS_ToInt32(callback_ctx, &presented_frames, value);
                JS_FreeValue(callback_ctx, value);
                presented_frames++;
                JS_SetPropertyStr(callback_ctx, media, "_nd_presented_frames",
                                  JS_NewInt32(callback_ctx, presented_frames));
            }
            JSValue meta = JS_NewObject(callback_ctx);
            JS_SetPropertyStr(callback_ctx, meta, "presentationTime",
                              JS_NewFloat64(callback_ctx, ts_ms));
            JS_SetPropertyStr(callback_ctx, meta, "expectedDisplayTime",
                              JS_NewFloat64(callback_ctx, ts_ms));
            JS_SetPropertyStr(callback_ctx, meta, "width",
                              JS_NewInt32(callback_ctx, width));
            JS_SetPropertyStr(callback_ctx, meta, "height",
                              JS_NewInt32(callback_ctx, height));
            JS_SetPropertyStr(callback_ctx, meta, "mediaTime",
                              JS_NewFloat64(callback_ctx, media_time));
            JS_SetPropertyStr(callback_ctx, meta, "presentedFrames",
                              JS_NewInt32(callback_ctx, presented_frames));
            JSValueConst argv[2] = { arg, meta };
            ret = JS_Call(callback_ctx, e->cb, JS_UNDEFINED, 2, argv);
            JS_FreeValue(callback_ctx, meta);
            JS_FreeValue(callback_ctx, media);
        } else {
            JSValueConst argv[1] = { arg };
            ret = JS_Call(callback_ctx, e->cb, JS_UNDEFINED, 1, argv);
        }
        if (JS_IsException(ret)) {
            JSValue ex = JS_GetException(callback_ctx);
            const char *msg = JS_ToCString(callback_ctx, ex);
            if (msg && js->log_cb) {
                char *line = g_strdup_printf(
                    "JS error in requestAnimationFrame: %s", msg);
                js->log_cb(line, js->log_user_data);
                g_free(line);
            }
            if (msg) JS_FreeCString(callback_ctx, msg);
            JS_FreeValue(callback_ctx, ex);
        }
        JS_FreeValue(callback_ctx, ret);
        JS_FreeValue(callback_ctx, arg);
        JS_FreeValue(callback_ctx, e->cb);
        ns_drain_microtasks(js);
        if (e->frame) ns_frame_url_leave(js, &fu);
        js->raf_frame_ctx = previous_frame;
        js->current_doc = previous_doc;
        js->ctx = previous_ctx;
    }
    js->callback_depth--;
    g_array_free(fired, TRUE);
    ns_drain_mutations(js);
    ns_js_budget_pop(js, &bg);
    return js->mutated ? TRUE : FALSE;
}

gboolean
ns_js_has_pending_animation_frame(const ns_js *js)
{
    return js && js->raf_pending && js->raf_pending->len > 0;
}

gboolean
ns_js_has_pending_work(const ns_js *js)
{
    if (!js) return FALSE;
    if (ns_services_timers_pending(js, FALSE)) return TRUE;
    if (js->raf_pending && js->raf_pending->len > 0) return TRUE;
    if (ns_js_net_pending_fetches(js) > 0) return TRUE;
    if (ns_js_net_pending_xhrs(js) > 0) return TRUE;
    if (ns_js_net_pending_sockets(js) > 0) return TRUE;
    if (js->filereader_idles && js->filereader_idles->len > 0) return TRUE;
    if (ns_js_pending_iframe_count(js) > 0) return TRUE;
    if (ns_mutation_drain_pending(js)) return TRUE;
    if (js->observer_tick_source) return TRUE;
    if (ns_js_async_scripts_pending(js)) return TRUE;
    if (ns_ce_has_pending(js))
        return TRUE;
    if (ns_workers_pending(js)) return TRUE;
    return FALSE;
}

gboolean
ns_js_needs_tick(const ns_js *js)
{
    if (!js) return FALSE;
    if (ns_js_has_pending_work(js)) return TRUE;
    if (ns_services_timers_pending(js, TRUE)) return TRUE;
    if (js->message_tasks && !g_queue_is_empty(js->message_tasks))
        return TRUE;
    if (ns_js_image_loads_pending(js))
        return TRUE;
    return FALSE;
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
ns_js_dump_stats(ns_js *js, GString *out)
{
    if (!js || !out)
        return;
    if (js->rt) {
        JSMemoryUsage u;
        memset(&u, 0, sizeof u);
        JS_ComputeMemoryUsage(js->rt, &u);
        g_string_append(out, "JavaScript heap\n");
        g_string_append_printf(out, "  used        %lld bytes\n",
                               (long long)u.memory_used_size);
        g_string_append_printf(out, "  malloc      %lld bytes\n",
                               (long long)u.malloc_size);
        g_string_append_printf(out, "  malloc cap  %lld bytes\n",
                               (long long)u.malloc_limit);
        g_string_append_printf(out, "  objects     %lld\n",
                               (long long)u.obj_count);
        g_string_append_printf(out, "  properties  %lld\n",
                               (long long)u.prop_count);
        g_string_append_printf(out, "  atoms       %lld\n",
                               (long long)u.atom_count);
        g_string_append_printf(out, "  functions   %lld\n",
                               (long long)u.c_func_count);
    }
    g_string_append(out, "\nEvent loop\n");
    g_string_append_printf(out, "  timers          %u\n",
                           ns_services_timer_count(js));
    g_string_append_printf(out, "  anim frames     %u\n",
                           js->raf_pending ? js->raf_pending->len : 0);
    g_string_append_printf(out, "  pending fetch   %u\n",
                           ns_js_net_pending_fetches(js));
    g_string_append_printf(out, "  pending xhr     %u\n",
                           ns_js_net_pending_xhrs(js));
    g_string_append_printf(out, "  websockets      %u\n",
                           ns_js_net_pending_sockets(js));
    g_string_append_printf(out, "  event listeners %u\n",
                           ns_dispatch_listener_count(js));
    g_string_append_printf(out, "  frame contexts  %u\n",
                           js->frame_ctxs ? js->frame_ctxs->len : 0);
}

static void
ns_js_purge_frame_rafs(ns_js *js, const ns_node *frame)
{
    if (!js || !js->raf_pending || !frame) return;
    for (guint i = js->raf_pending->len; i > 0; i--) {
        ns_raf_entry *e = &g_array_index(js->raf_pending, ns_raf_entry, i - 1);
        if (e->frame != frame) continue;
        JS_FreeValue(e->ctx ? e->ctx : js->ctx, e->cb);
        g_array_remove_index(js->raf_pending, i - 1);
    }
    if (js->raf_frame_ctx == frame) js->raf_frame_ctx = NULL;
}

static GHashTable *
ns_js_snapshot_globals(ns_js *js)
{
    GHashTable *names = g_hash_table_new_full(g_str_hash, g_str_equal,
                                              g_free, NULL);
    JSValue global = JS_GetGlobalObject(js->ctx);
    JSPropertyEnum *tab = NULL;
    uint32_t n = 0;
    if (JS_GetOwnPropertyNames(js->ctx, &tab, &n, global,
                               JS_GPN_STRING_MASK) == 0) {
        for (uint32_t i = 0; i < n; i++) {
            const char *s = JS_AtomToCString(js->ctx, tab[i].atom);
            if (s) g_hash_table_add(names, g_strdup(s));
            if (s) JS_FreeCString(js->ctx, s);
            JS_FreeAtom(js->ctx, tab[i].atom);
        }
        js_free(js->ctx, tab);
    }
    JS_FreeValue(js->ctx, global);
    return names;
}

static void
ns_js_record_iframe_globals(ns_js *js, ns_node *iframe, GHashTable *before)
{
    GPtrArray *added = g_ptr_array_new_with_free_func(g_free);
    JSValue global = JS_GetGlobalObject(js->ctx);
    JSPropertyEnum *tab = NULL;
    uint32_t n = 0;
    if (JS_GetOwnPropertyNames(js->ctx, &tab, &n, global,
                               JS_GPN_STRING_MASK) == 0) {
        for (uint32_t i = 0; i < n; i++) {
            const char *s = JS_AtomToCString(js->ctx, tab[i].atom);
            if (s && !g_hash_table_contains(before, s))
                g_ptr_array_add(added, g_strdup(s));
            if (s) JS_FreeCString(js->ctx, s);
            JS_FreeAtom(js->ctx, tab[i].atom);
        }
        js_free(js->ctx, tab);
    }
    JS_FreeValue(js->ctx, global);
    if (added->len == 0) {
        g_ptr_array_free(added, TRUE);
        return;
    }
    if (!js->iframe_globals)
        js->iframe_globals =
            g_hash_table_new_full(g_direct_hash, g_direct_equal, NULL,
                                  (GDestroyNotify)g_ptr_array_unref);
    GPtrArray *prev = g_hash_table_lookup(js->iframe_globals, iframe);
    if (prev) {
        for (guint i = 0; i < added->len; i++)
            g_ptr_array_add(prev, g_strdup(g_ptr_array_index(added, i)));
        g_ptr_array_free(added, TRUE);
    } else {
        g_hash_table_insert(js->iframe_globals, iframe, added);
    }
}

static void
ns_js_scrub_iframe_globals(ns_js *js, const ns_node *iframe)
{
    if (!js || !js->iframe_globals) return;
    GPtrArray *names = g_hash_table_lookup(js->iframe_globals,
                                           (gpointer)iframe);
    if (!names) return;
    JSValue global = JS_GetGlobalObject(js->ctx);
    for (guint i = 0; i < names->len; i++) {
        const char *name = g_ptr_array_index(names, i);
        JSAtom a = JS_NewAtom(js->ctx, name);
        int rc = JS_DeleteProperty(js->ctx, global, a, 0);
        if (rc < 0) JS_FreeValue(js->ctx, JS_GetException(js->ctx));
        if (rc == 0)
            JS_SetProperty(js->ctx, global, a, JS_UNDEFINED);
        JS_FreeAtom(js->ctx, a);
    }
    JS_FreeValue(js->ctx, global);
    g_hash_table_remove(js->iframe_globals, (gpointer)iframe);
}

static void
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

static void
ns_js_anim_event_cb(const ns_node *node, const char *type,
                    const char *name, double elapsed_ms, gpointer user)
{
    ns_js *js = user;
    if (!js || !js->ctx || !node || js->halted) return;
    JSContext *ctx = js->ctx;
    if (g_str_has_prefix(type, "__ns")) {
        JSValue global = JS_GetGlobalObject(ctx);
        JSValue hook = JS_GetPropertyStr(ctx, global, "__ns_anim_script_event");
        if (JS_IsFunction(ctx, hook)) {
            JSValue args[3] = {
                ns_make_element(ctx, node),
                JS_NewInt32(ctx, name ? atoi(name) : -1),
                JS_NewString(ctx, type + 4),
            };
            JSValue r = JS_Call(ctx, hook, JS_UNDEFINED, 3, args);
            if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
            JS_FreeValue(ctx, r);
            for (int i = 0; i < 3; i++) JS_FreeValue(ctx, args[i]);
        }
        JS_FreeValue(ctx, hook);
        JS_FreeValue(ctx, global);
        return;
    }
    JSValue event = ns_make_event(ctx, type, node);
    JS_SetPropertyStr(ctx, event, "bubbles",    JS_TRUE);
    JS_SetPropertyStr(ctx, event, "cancelable", JS_FALSE);
    JS_SetPropertyStr(ctx, event, "elapsedTime",
                      JS_NewFloat64(ctx, elapsed_ms / 1000.0));
    JS_SetPropertyStr(ctx, event, "pseudoElement", JS_NewString(ctx, ""));
    if (type[0] == 'a')
        JS_SetPropertyStr(ctx, event, "animationName",
                          JS_NewString(ctx, name ? name : ""));
    else
        JS_SetPropertyStr(ctx, event, "propertyName",
                          JS_NewString(ctx, name ? name : ""));
    ns_js_dispatch_built_event(js, node, type, event, NULL);

    const char *legacy = NULL;
    if      (strcmp(type, "transitionend") == 0)      legacy = "webkitTransitionEnd";
    else if (strcmp(type, "animationstart") == 0)     legacy = "webkitAnimationStart";
    else if (strcmp(type, "animationend") == 0)       legacy = "webkitAnimationEnd";
    else if (strcmp(type, "animationiteration") == 0) legacy = "webkitAnimationIteration";
    if (legacy) {
        JSValue lev = ns_make_event(ctx, legacy, node);
        JS_SetPropertyStr(ctx, lev, "bubbles",    JS_TRUE);
        JS_SetPropertyStr(ctx, lev, "cancelable", JS_FALSE);
        JS_SetPropertyStr(ctx, lev, "elapsedTime",
                          JS_NewFloat64(ctx, elapsed_ms / 1000.0));
        JS_SetPropertyStr(ctx, lev, "pseudoElement", JS_NewString(ctx, ""));
        if (type[0] == 'a')
            JS_SetPropertyStr(ctx, lev, "animationName",
                              JS_NewString(ctx, name ? name : ""));
        else
            JS_SetPropertyStr(ctx, lev, "propertyName",
                              JS_NewString(ctx, name ? name : ""));
        JS_SetPropertyStr(ctx, lev, "__ns_alias_base", JS_NewString(ctx, type));
        ns_js_dispatch_built_event(js, node, legacy, lev, NULL);
    }
}

void
ns_js_dispatch_anim_events(ns_js *js, ns_anim *anim)
{
    if (!js || !anim || js->halted || js->in_pump) return;
    ns_anim_drain_events(anim, ns_js_anim_event_cb, js);
}

/* The resource timing entry of an image, from the times the image cache
 * started and finished fetching it.  An image served from memory without
 * a fetch for this page takes its load time instead. */
static void
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

static void
ns_js_fire_img_load_once(ns_js *js, ns_node *node, gboolean failed)
{
    if (!js || !node) return;
    if (node->flags & NS_NODE_IMG_LOAD_FIRED) return;
    if (js->halted || js->in_pump) return;
    node->flags |= NS_NODE_IMG_LOAD_FIRED;
    if (ns_node_is_element_named(node, "img")) {
        const ns_image *im = ns_js_image_for_node(js, node);
        if (im) ns_js_record_image_timing(js, node, im->url, im);
    }
    ns_js_dispatch_resource_event(js, node, failed ? "error" : "load");
}

static void
ns_js_walk_collect_media_events(const ns_box *b, GPtrArray *imgs,
                                GArray *img_failed, GPtrArray *videos)
{
    if (!b) return;
    if (b->dom && b->dom->name && b->media) {
        if (strcmp(b->dom->name, "img") == 0 && b->media->image) {
            const ns_image *im = (const ns_image *)b->media->image;
            if ((im->loaded || im->failed) &&
                !(b->dom->flags & NS_NODE_IMG_LOAD_FIRED) &&
                !g_ptr_array_find(imgs, b->dom, NULL)) {
                gboolean failed = im->failed ? TRUE : FALSE;
                g_ptr_array_add(imgs, (gpointer)b->dom);
                g_array_append_val(img_failed, failed);
            }
        } else if (strcmp(b->dom->name, "video") == 0 && b->media->video) {
            ns_video *v = b->media->video;
            if (v->loaded && !v->load_events_fired) {
                v->load_events_fired = TRUE;
                g_ptr_array_add(videos, (gpointer)b->dom);
            }
        }
    }
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        ns_js_walk_collect_media_events(c, imgs, img_failed, videos);
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++)
            ns_js_walk_collect_media_events(
                g_array_index(b->inline_atomics, ns_inline_atomic, i).box,
                imgs, img_failed, videos);
}

void
ns_js_fire_media_load_events(ns_js *js, const struct ns_box *layout)
{
    if (!js || !layout) return;
    if (js->halted || js->in_pump) return;
    GPtrArray *imgs = g_ptr_array_new();
    GArray *img_failed = g_array_new(FALSE, FALSE, sizeof(gboolean));
    GPtrArray *videos = g_ptr_array_new();
    ns_js_walk_collect_media_events(layout, imgs, img_failed, videos);
    for (guint i = 0; i < imgs->len; i++)
        ns_js_fire_img_load_once(js, g_ptr_array_index(imgs, i),
                                 g_array_index(img_failed, gboolean, i));
    for (guint i = 0; i < videos->len; i++) {
        ns_node *v = g_ptr_array_index(videos, i);
        ns_js_dispatch_event(js, v, "loadedmetadata", NULL);
        ns_js_dispatch_event(js, v, "loadeddata", NULL);
        ns_js_dispatch_event(js, v, "canplay", NULL);
    }
    g_ptr_array_free(imgs, TRUE);
    g_array_free(img_failed, TRUE);
    g_ptr_array_free(videos, TRUE);
}

void
ns_element_insert_before_single(ns_js *_j, ns_node *parent, ns_node *newc, ns_node *ref)
{
    if (newc == ref) return;
    if (_j && newc->parent) ns_node_iters_pre_remove(_j, newc);
    if (newc->parent) ns_node_remove(newc);
    if (_j) g_hash_table_remove(_j->orphan_nodes, newc);
    newc->parent = parent;
    newc->next_sibling = ref;
    newc->prev_sibling = ref->prev_sibling;
    if (ref->prev_sibling) ref->prev_sibling->next_sibling = newc;
    else parent->first_child = newc;
    ref->prev_sibling = newc;
}

void
ns_insert_sibling_before(ns_node *ref, ns_node *newc)
{
    if (!ref || !ref->parent || !newc) return;
    if (ref == newc) return;
    if (newc->parent) ns_node_remove(newc);
    ns_node *parent = ref->parent;
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

static void
ns_attribute_map_release_owner(ns_js *js, ns_node *owner)
{
    if (!js || !js->ctx || !js->attribute_maps || !owner) return;
    gpointer wrapper = g_hash_table_lookup(js->attribute_maps, owner);
    if (!wrapper) return;
    g_hash_table_remove(js->attribute_maps, owner);
    JS_FreeValue(js->ctx, JS_MKPTR(JS_TAG_OBJECT, wrapper));
}

static void
ns_attribute_maps_release_all(ns_js *js)
{
    if (!js || !js->ctx || !js->attribute_maps) return;
    GList *wrappers = g_hash_table_get_values(js->attribute_maps);
    g_hash_table_remove_all(js->attribute_maps);
    for (GList *item = wrappers; item; item = item->next)
        JS_FreeValue(js->ctx, JS_MKPTR(JS_TAG_OBJECT, item->data));
    g_list_free(wrappers);
}

static void
ns_attr_finalizer(JSRuntime *rt, JSValue value)
{
    (void)rt;
    ns_attr_state_release(JS_GetOpaque(value, ns_attr_class_id));
}

static JSClassDef ns_attr_class = {
    .class_name = "Attr",
    .finalizer = ns_attr_finalizer,
};

static JSValue ns_element_isSameNode(JSContext *ctx, JSValueConst this_val,
                                     int argc, JSValueConst *argv);

void *
ns_attr_opaque(JSValueConst value)
{
    return JS_GetOpaque(value, ns_attr_class_id);
}

JSValue
ns_attr_new_object(JSContext *ctx, void *state)
{
    JSValue obj = JS_NewObjectClass(ctx, ns_attr_class_id);
    if (!JS_IsException(obj)) JS_SetOpaque(obj, state);
    return obj;
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

static JSValue
ns_element_get_attributes(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind != NS_NODE_ELEMENT) return JS_NewObject(ctx);
    ns_js *js = js_from_ctx(ctx);
    gpointer cached = js && js->attribute_maps
        ? g_hash_table_lookup(js->attribute_maps, n) : NULL;
    if (cached)
        return JS_DupValue(ctx, JS_MKPTR(JS_TAG_OBJECT, cached));
    JSValue arr = ns_make_live(ctx, this_val, NS_LIVE_ATTRIBUTES, NULL);
    if (JS_IsException(arr)) return arr;
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue ctor = JS_GetPropertyStr(ctx, global, "NamedNodeMap");
    JSValue proto = JS_GetPropertyStr(ctx, ctor, "prototype");
    if (JS_IsObject(proto)) JS_SetPrototype(ctx, arr, proto);
    JS_FreeValue(ctx, proto);
    JS_FreeValue(ctx, ctor);
    JS_FreeValue(ctx, global);
    if (js && js->attribute_maps) {
        JS_DupValue(ctx, arr);
        g_hash_table_insert(js->attribute_maps, (gpointer)n,
                            JS_VALUE_GET_PTR(arr));
    }
    return arr;
}

static JSValue
ns_element_getAttributeNames(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    const ns_node *n = ns_unwrap_element(this_val);
    JSValue arr = JS_NewArray(ctx);
    if (!n || n->kind != NS_NODE_ELEMENT) return arr;
    uint32_t i = 0;
    for (const ns_attr *a = n->attrs; a; a = a->next)
        if (a->name && !ns_attr_name_is_internal(a->name))
            JS_SetPropertyUint32(ctx, arr, i++, JS_NewString(ctx, a->name));
    return arr;
}

static JSValue
ns_element_hasAttributes(JSContext *ctx, JSValueConst this_val,
                         int argc, JSValueConst *argv)
{
    (void)ctx; (void)argc; (void)argv;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind != NS_NODE_ELEMENT) return JS_FALSE;
    for (const ns_attr *a = n->attrs; a; a = a->next)
        if (a->name && !ns_attr_name_is_internal(a->name)) return JS_TRUE;
    return JS_FALSE;
}

static JSValue ns_element_setAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);
static JSValue ns_element_removeAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv);

static JSValue
ns_anim_finish_job(JSContext *ctx, int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_UNDEFINED;
    JSValueConst anim = argv[0];
    JSValue ev = ns_event_new(ctx);
    JS_SetPropertyStr(ctx, ev, "type",          JS_NewString(ctx, "finish"));
    JS_SetPropertyStr(ctx, ev, "target",        JS_DupValue(ctx, anim));
    JS_SetPropertyStr(ctx, ev, "currentTarget", JS_DupValue(ctx, anim));

    JSValue onf = JS_GetPropertyStr(ctx, anim, "onfinish");
    if (JS_IsFunction(ctx, onf)) {
        JSValueConst a[1] = { ev };
        JSValue r = JS_Call(ctx, onf, anim, 1, a);
        if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, r);
    }
    JS_FreeValue(ctx, onf);

    JSValue listeners = JS_GetPropertyStr(ctx, anim, "_listeners");
    if (JS_IsArray(listeners)) {
        uint32_t len = ns_js_array_length(ctx, listeners);
        for (uint32_t i = 0; i < len; i++) {
            JSValue e = JS_GetPropertyUint32(ctx, listeners, i);
            JSValue tv = JS_GetPropertyStr(ctx, e, "type");
            const char *ts = JS_ToCString(ctx, tv);
            if (ts && strcmp(ts, "finish") == 0) {
                JSValue cb = JS_GetPropertyStr(ctx, e, "cb");
                if (JS_IsFunction(ctx, cb)) {
                    JSValueConst a[1] = { ev };
                    JSValue r = JS_Call(ctx, cb, anim, 1, a);
                    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
                    JS_FreeValue(ctx, r);
                }
                JS_FreeValue(ctx, cb);
            }
            if (ts) JS_FreeCString(ctx, ts);
            JS_FreeValue(ctx, tv);
            JS_FreeValue(ctx, e);
        }
    }
    JS_FreeValue(ctx, listeners);
    JS_FreeValue(ctx, ev);
    return JS_UNDEFINED;
}

static JSValue
ns_element_animate(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    static const ns_fn_def anim_methods[] = {
        { "play", 0 }, { "pause", 0 }, { "cancel", 0 },
        { "finish", 0 }, { "reverse", 0 },
        { "commitStyles", 0 }, { "persist", 0 },
        { "updatePlaybackRate", 1 },
    };
    JSValue anim = JS_NewObject(ctx);
    ns_bind_fns(ctx, anim, ns_event_noop, anim_methods, G_N_ELEMENTS(anim_methods));
    ns_bind_fn(ctx, anim, "addEventListener",    ns_port_add_event_listener,    2);
    ns_bind_fn(ctx, anim, "removeEventListener", ns_port_remove_event_listener, 2);
    JS_SetPropertyStr(ctx, anim, "_listeners",   JS_NewArray(ctx));
    JS_SetPropertyStr(ctx, anim, "onfinish",     JS_NULL);
    JS_SetPropertyStr(ctx, anim, "oncancel",     JS_NULL);
    JS_SetPropertyStr(ctx, anim, "playState",    JS_NewString(ctx, "finished"));
    JS_SetPropertyStr(ctx, anim, "playbackRate", JS_NewInt32(ctx, 1));
    JS_SetPropertyStr(ctx, anim, "currentTime",  JS_NewFloat64(ctx, 0));
    JS_SetPropertyStr(ctx, anim, "startTime",    JS_NULL);
    JS_SetPropertyStr(ctx, anim, "pending",      JS_FALSE);
    JS_SetPropertyStr(ctx, anim, "id",           JS_NewString(ctx, ""));
    JSValue resolvers[2];
    JSValue finished = JS_NewPromiseCapability(ctx, resolvers);
    if (JS_IsException(finished)) { JS_FreeValue(ctx, anim); return finished; }
    JSValueConst self_arg[1] = { anim };
    JSValue r = JS_Call(ctx, resolvers[0], JS_UNDEFINED, 1, self_arg);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
    JS_FreeValue(ctx, resolvers[0]);
    JS_FreeValue(ctx, resolvers[1]);
    JS_SetPropertyStr(ctx, anim, "finished", finished);
    JS_SetPropertyStr(ctx, anim, "ready",    JS_DupValue(ctx, finished));

    JSValueConst job_arg[1] = { anim };
    JS_EnqueueJob(ctx, ns_anim_finish_job, 1, job_arg);
    return anim;
}

static JSValue
ns_element_toggleAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (argc < 1)
        return JS_ThrowTypeError(ctx,
            "1 argument required, but only 0 present");
    if (!n) return JS_FALSE;
    size_t raw_len = 0;
    const char *raw_name = JS_ToCStringLen(ctx, &raw_len, argv[0]);
    if (!raw_name) return JS_FALSE;
    if (strlen(raw_name) != raw_len || !ns_valid_attr_name(raw_name)) {
        JS_FreeCString(ctx, raw_name);
        return ns_throw_dom_exception(ctx, "InvalidCharacterError", 5,
                                      "toggleAttribute: invalid attribute name");
    }
    char *lowered = NULL;
    const char *name = ns_attr_name_normalize(n, raw_name, &lowered);
    if (ns_attr_name_is_internal(name)) {
        JS_FreeCString(ctx, raw_name);
        g_free(lowered);
        return JS_FALSE;
    }
    gboolean had = ns_element_get_attr(n, name) != NULL;
    gboolean want;
    if (argc >= 2 && !JS_IsUndefined(argv[1]))
        want = JS_ToBool(ctx, argv[1]) ? TRUE : FALSE;
    else
        want = !had;
    ns_js *_j = js_from_ctx(ctx);
    if (want && !had)      ns_js_set_attr_recorded(_j, n, name, "");
    else if (!want && had) ns_js_remove_attr_recorded(_j, n, name);
    JS_FreeCString(ctx, raw_name);
    g_free(lowered);
    return want ? JS_TRUE : JS_FALSE;
}

static const ns_attr *
ns_element_attr_by_namespace(const ns_node *n, const char *namespace_uri,
                             const char *local)
{
    const ns_attr *a = ns_element_find_attr_ns(n, namespace_uri, local);
    return a && !ns_attr_name_is_internal(a->name) ? a : NULL;
}

static const ns_attr *
ns_page_attr_by_namespace(const ns_node *n, const char *namespace_uri,
                          const char *local_name)
{
    const ns_attr *a = ns_element_attr_by_namespace(n, namespace_uri,
                                                    local_name);
    return a && a->name && ns_attr_name_is_internal(a->name) ? NULL : a;
}

static JSValue
ns_element_getAttributeNS(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 2) return JS_NULL;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind != NS_NODE_ELEMENT) return JS_NULL;
    gboolean ns_null = JS_IsNull(argv[0]) || JS_IsUndefined(argv[0]);
    const char *ns_raw = ns_null ? NULL : JS_ToCString(ctx, argv[0]);
    const char *ns_uri = ns_raw && *ns_raw ? ns_raw : NULL;
    const char *local = JS_ToCString(ctx, argv[1]);
    if (!local) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        return JS_NULL;
    }
    const ns_attr *a = ns_page_attr_by_namespace(n, ns_uri, local);
    if (ns_raw) JS_FreeCString(ctx, ns_raw);
    JS_FreeCString(ctx, local);
    return a ? JS_NewString(ctx, a->value ? a->value : "") : JS_NULL;
}

static JSValue
ns_element_hasAttributeNS(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 2) return JS_FALSE;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind != NS_NODE_ELEMENT) return JS_FALSE;
    gboolean ns_null = JS_IsNull(argv[0]) || JS_IsUndefined(argv[0]);
    const char *ns_raw = ns_null ? NULL : JS_ToCString(ctx, argv[0]);
    const char *ns_uri = ns_raw && *ns_raw ? ns_raw : NULL;
    const char *local = JS_ToCString(ctx, argv[1]);
    if (!local) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        return JS_FALSE;
    }
    const ns_attr *a = ns_page_attr_by_namespace(n, ns_uri, local);
    if (ns_raw) JS_FreeCString(ctx, ns_raw);
    JS_FreeCString(ctx, local);
    return a ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_setAttributeNS(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (argc < 3)
        return JS_ThrowTypeError(ctx,
            "3 arguments required, but only %d present", argc);
    gboolean ns_null = JS_IsNull(argv[0]) || JS_IsUndefined(argv[0]);
    const char *ns_raw = ns_null ? NULL : JS_ToCString(ctx, argv[0]);
    const char *ns_uri = ns_raw && *ns_raw ? ns_raw : NULL;
    size_t name_len = 0;
    const char *name = JS_ToCStringLen(ctx, &name_len, argv[1]);
    if (!name) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        return JS_EXCEPTION;
    }
    if (strlen(name) != name_len) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        JS_FreeCString(ctx, name);
        return ns_throw_dom_exception(ctx, "InvalidCharacterError", 5,
            "invalid qualified name");
    }
    JSValue verr = ns_validate_attr_ns(ctx, ns_uri, name);
    if (JS_IsException(verr)) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        JS_FreeCString(ctx, name);
        return verr;
    }
    if (!n || n->kind != NS_NODE_ELEMENT) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        JS_FreeCString(ctx, name);
        return JS_UNDEFINED;
    }
    const char *val  = JS_ToCString(ctx, argv[2]);
    const char *colon = name ? strchr(name, ':') : NULL;
    g_autofree char *prefix = colon ? g_strndup(name, (gsize)(colon - name)) : NULL;
    const char *local = colon ? colon + 1 : name;
    if (name && val && local && !ns_attr_name_is_internal(name))
        ns_js_set_attr_ns_recorded(js_from_ctx(ctx), n, ns_uri, prefix,
                                   local, name, val);
    if (ns_raw) JS_FreeCString(ctx, ns_raw);
    if (name) JS_FreeCString(ctx, name);
    if (val)  JS_FreeCString(ctx, val);
    return JS_UNDEFINED;
}

static JSValue
ns_element_removeAttributeNS(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    if (argc < 2) return JS_UNDEFINED;
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n || n->kind != NS_NODE_ELEMENT) return JS_UNDEFINED;
    gboolean ns_null = JS_IsNull(argv[0]) || JS_IsUndefined(argv[0]);
    const char *ns_raw = ns_null ? NULL : JS_ToCString(ctx, argv[0]);
    const char *ns_uri = ns_raw && *ns_raw ? ns_raw : NULL;
    const char *local = JS_ToCString(ctx, argv[1]);
    if (!local) {
        if (ns_raw) JS_FreeCString(ctx, ns_raw);
        return JS_UNDEFINED;
    }
    if (ns_page_attr_by_namespace(n, ns_uri, local))
        ns_js_remove_attr_ns_recorded(js_from_ctx(ctx), n, ns_uri, local);
    if (ns_raw) JS_FreeCString(ctx, ns_raw);
    JS_FreeCString(ctx, local);
    return JS_UNDEFINED;
}

static JSValue
ns_element_setAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (argc < 2) return JS_UNDEFINED;
    size_t raw_len = 0;
    const char *raw_name = JS_ToCStringLen(ctx, &raw_len, argv[0]);
    if (!raw_name || strlen(raw_name) != raw_len || !ns_valid_attr_name(raw_name)) {
        if (raw_name) JS_FreeCString(ctx, raw_name);
        return ns_throw_dom_exception(ctx, "InvalidCharacterError", 5,
            "setAttribute: invalid attribute name");
    }
    if (!n || n->kind != NS_NODE_ELEMENT) {
        JS_FreeCString(ctx, raw_name);
        return JS_UNDEFINED;
    }
    char *lowered = NULL;
    const char *name = ns_attr_name_normalize(n, raw_name, &lowered);
    size_t val_len = 0;
    const char *val  = JS_ToCStringLen(ctx, &val_len, argv[1]);
    if (name && val && !ns_attr_name_is_internal(name)) {
        gsize old_len = 0;
        const char *old = ns_element_get_attr_len(n, name, &old_len);
        gboolean changed = !old || old_len != val_len ||
                           memcmp(old, val, val_len) != 0;
        char *old_copy = old ? ns_value_dup_len(old, old_len) : NULL;
        ns_js *_j = js_from_ctx(ctx);
        gboolean img_src_paint_only =
            changed && g_ascii_strcasecmp(name, "src") == 0 &&
            ns_js_img_src_layout_neutral(n);
        if (changed) {
            ns_element_set_attr_len(n, name, val, (gssize)val_len);
        }
        ns_body_forward_content_handler(ctx, n, name, val);
        if (changed && _j) {
            if (!img_src_paint_only && ns_css_attr_may_affect_style(n, name))
                _j->mutated = TRUE;
            if (img_src_paint_only && _j->repaint_cb)
                _j->repaint_cb(_j->repaint_user_data);
        }
        if (_j) ns_js_record_attr_change(_j, n, name, old_copy);
        if (_j) ns_ce_attr_changed(_j, n, name, old_copy, val);
        if (changed && g_ascii_strcasecmp(name, "type") == 0 &&
            ns_node_is_element_named(n, "input"))
            ns_input_resanitize_value(n);
        if (g_ascii_strcasecmp(name, "open") == 0 && !old_copy &&
            ns_node_is_element_named(n, "details"))
            ns_js_details_toggle_open(_j, n, TRUE);
        g_free(old_copy);
        if (changed && g_ascii_strcasecmp(name, "src") == 0 &&
            n->name && strcmp(n->name, "img") == 0)
            ns_js_start_image_load(js_from_ctx(ctx), n, val);
        if (n->name && strcmp(n->name, "iframe") == 0 &&
            (g_ascii_strcasecmp(name, "src") == 0 ||
             g_ascii_strcasecmp(name, "srcdoc") == 0))
            ns_js_schedule_iframe_load_full(js_from_ctx(ctx), n, TRUE);
        else if (changed && n->name && strcmp(n->name, "object") == 0 &&
                 g_ascii_strcasecmp(name, "data") == 0)
            ns_js_schedule_iframe_load(js_from_ctx(ctx), n);
    }
    if (raw_name) JS_FreeCString(ctx, raw_name);
    g_free(lowered);
    if (val)  JS_FreeCString(ctx, val);
    return JS_UNDEFINED;
}

static JSValue
ns_element_removeAttribute(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n || argc < 1 || n->kind != NS_NODE_ELEMENT) return JS_UNDEFINED;
    const char *raw_name = JS_ToCString(ctx, argv[0]);
    if (!raw_name) return JS_UNDEFINED;
    char *lowered = NULL;
    const char *name = ns_attr_name_normalize(n, raw_name, &lowered);
    if (ns_attr_name_is_internal(name)) {
        JS_FreeCString(ctx, raw_name);
        g_free(lowered);
        return JS_UNDEFINED;
    }
    if (ns_element_find_attr(n, name)) {
        ns_js *_j = js_from_ctx(ctx);
        ns_js_remove_attr_recorded(_j, n, name);
        if (g_ascii_strcasecmp(name, "open") == 0 &&
            ns_node_is_element_named(n, "details"))
            ns_js_details_toggle_open(_j, n, FALSE);
    }
    ns_body_forward_content_handler(ctx, n, name, NULL);
    JS_FreeCString(ctx, raw_name);
    g_free(lowered);
    return JS_UNDEFINED;
}

static const ns_node *
next_element_sibling(const ns_node *n)
{
    for (const ns_node *s = n ? n->next_sibling : NULL; s; s = s->next_sibling)
        if (s->kind == NS_NODE_ELEMENT && !ns_node_is_shadow_root(s)) return s;
    return NULL;
}

static const ns_node *
prev_element_sibling(const ns_node *n)
{
    for (const ns_node *s = n ? n->prev_sibling : NULL; s; s = s->prev_sibling)
        if (s->kind == NS_NODE_ELEMENT && !ns_node_is_shadow_root(s)) return s;
    return NULL;
}

static const ns_node *
first_element_child(const ns_node *n)
{
    for (const ns_node *c = n ? n->first_child : NULL; c; c = c->next_sibling)
        if (c->kind == NS_NODE_ELEMENT && !ns_node_is_shadow_root(c)) return c;
    return NULL;
}

static JSValue
ns_element_get_parentElement(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind == NS_NODE_DOCUMENT || ns_node_is_shadow_root(n))
        return JS_NULL;
    if (!n->parent || n->parent->kind != NS_NODE_ELEMENT ||
        ns_node_is_shadow_root(n->parent))
        return JS_NULL;
    return ns_make_element(ctx, n->parent);
}

static JSValue
ns_element_get_parentNode(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || n->kind == NS_NODE_DOCUMENT || !n->parent ||
        ns_node_is_shadow_root(n))
        return JS_NULL;
    return ns_make_element(ctx, n->parent);
}

static JSValue
ns_element_get_firstElementChild(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (ns_node_is_element_named(n, "template")) return JS_NULL;
    return ns_make_element(ctx, first_element_child(n));
}

static JSValue
ns_element_get_nextElementSibling(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_element(ctx, next_element_sibling(ns_unwrap_element(this_val)));
}

static JSValue
ns_element_get_previousElementSibling(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_element(ctx, prev_element_sibling(ns_unwrap_element(this_val)));
}

static JSValue
ns_element_get_children(JSContext *ctx, JSValueConst this_val)
{
    if (!ns_unwrap_element(this_val)) return JS_NewArray(ctx);
    return ns_make_live(ctx, this_val, NS_LIVE_CHILDREN, NULL);
}

static gboolean
ns_dom_hidden_child(const ns_node *c)
{
    return ns_node_is_embedded_doc(c) || ns_node_is_shadow_root(c);
}

static ns_node *
ns_dom_first_child(const ns_node *n)
{
    ns_node *c = n ? n->first_child : NULL;
    while (ns_dom_hidden_child(c)) c = c->next_sibling;
    return c;
}

static ns_node *
ns_dom_last_child(const ns_node *n)
{
    ns_node *c = n ? n->last_child : NULL;
    while (ns_dom_hidden_child(c)) c = c->prev_sibling;
    return c;
}

static ns_node *
ns_dom_next_sibling(const ns_node *n)
{
    ns_node *c = n ? n->next_sibling : NULL;
    while (ns_dom_hidden_child(c)) c = c->next_sibling;
    return c;
}

static ns_node *
ns_dom_prev_sibling(const ns_node *n)
{
    ns_node *c = n ? n->prev_sibling : NULL;
    while (ns_dom_hidden_child(c)) c = c->prev_sibling;
    return c;
}

static JSValue
ns_element_get_firstChild(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (ns_node_is_element_named(n, "template")) return JS_NULL;
    return ns_make_element(ctx, ns_dom_first_child(n));
}

static JSValue
ns_element_get_lastChild(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (ns_node_is_element_named(n, "template")) return JS_NULL;
    return ns_make_element(ctx, ns_dom_last_child(n));
}

static JSValue
ns_element_get_lastElementChild(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || ns_node_is_element_named(n, "template")) return JS_NULL;
    for (const ns_node *c = n->last_child; c; c = c->prev_sibling)
        if (c->kind == NS_NODE_ELEMENT && !ns_node_is_shadow_root(c))
            return ns_make_element(ctx, c);
    return JS_NULL;
}

static JSValue
ns_element_get_nextSibling(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    return ns_make_element(ctx, ns_dom_next_sibling(n));
}

static JSValue
ns_element_get_previousSibling(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    return ns_make_element(ctx, ns_dom_prev_sibling(n));
}

static JSValue
ns_element_get_childNodes(JSContext *ctx, JSValueConst this_val)
{
    if (!ns_unwrap_element(this_val)) return JS_NewArray(ctx);
    JSValue cached = JS_GetPropertyStr(ctx, this_val, "__ndChildNodes");
    if (JS_IsObject(cached)) return cached;
    JS_FreeValue(ctx, cached);
    JSValue list = ns_make_live(ctx, this_val, NS_LIVE_CHILDNODES, NULL);
    JS_DefinePropertyValueStr(ctx, this_val, "__ndChildNodes",
                              JS_DupValue(ctx, list), 0);
    return list;
}

static JSValue
ns_element_get_childElementCount(JSContext *ctx, JSValueConst this_val)
{
    (void)ctx;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || ns_node_is_element_named(n, "template")) return JS_NewInt32(ctx, 0);
    int count = 0;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling)
        if (c->kind == NS_NODE_ELEMENT && !ns_node_is_shadow_root(c)) count++;
    return JS_NewInt32(ctx, count);
}

static JSValue
ns_element_contains(JSContext *ctx, JSValueConst this_val,
                    int argc, JSValueConst *argv)
{
    (void)ctx;
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el || argc < 1) return JS_FALSE;
    const ns_node *other = ns_unwrap_element(argv[0]);
    if (!other) return JS_FALSE;
    for (const ns_node *cur = other; cur; cur = ns_dom_tree_parent(cur))
        if (cur == el) return JS_TRUE;
    return JS_FALSE;
}

static JSValue
ns_element_hasChildNodes(JSContext *ctx, JSValueConst this_val,
                         int argc, JSValueConst *argv)
{
    (void)ctx; (void)argc; (void)argv;
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el || ns_node_is_element_named(el, "template")) return JS_FALSE;
    for (const ns_node *c = el->first_child; c; c = c->next_sibling)
        if (!ns_dom_hidden_child(c)) return JS_TRUE;
    return JS_FALSE;
}

static gboolean
ns_attr_lookup_equal_value(const ns_attr *a, const ns_attr *want,
                           const char **out)
{
    for (; a; a = a->next) {
        if (g_strcmp0(a->namespace_uri, want->namespace_uri) == 0 &&
            strcmp(ns_attr_local_name(a), ns_attr_local_name(want)) == 0) {
            if (out) *out = a->value;
            return TRUE;
        }
    }
    return FALSE;
}

static guint
ns_attr_count(const ns_attr *a)
{
    guint n = 0;
    for (; a; a = a->next) {
        if (!a->name || ns_attr_name_is_internal(a->name)) continue;
        n++;
    }
    return n;
}

static gboolean
ns_node_attrs_equal(const ns_node *a, const ns_node *b)
{
    if (ns_attr_count(a->attrs) != ns_attr_count(b->attrs)) return FALSE;
    for (const ns_attr *p = a->attrs; p; p = p->next) {
        if (!p->name || ns_attr_name_is_internal(p->name)) continue;
        const char *bv = NULL;
        if (!ns_attr_lookup_equal_value(b->attrs, p, &bv)) return FALSE;
        const char *av = p->value ? p->value : "";
        bv = bv ? bv : "";
        if (strcmp(av, bv) != 0) return FALSE;
    }
    return TRUE;
}

static const char *
ns_node_namespace_uri(const ns_node *n)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return NULL;
    const char *stored = ns_element_get_attr(n, "data-nd-ns-uri");
    if (stored) return stored;
    if (n->flags & NS_NODE_SVG_NS)
        return "http://www.w3.org/2000/svg";
    if ((n->flags & NS_NODE_FOREIGN_NS) || (n->flags & NS_NODE_XML_DOC))
        return NULL;
    return "http://www.w3.org/1999/xhtml";
}

static const char *
ns_node_prefix(const ns_node *n, char *buf, size_t buf_sz)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return NULL;
    const char *stored = ns_element_get_attr(n, "data-nd-ns-prefix");
    if (stored) return stored;
    if (n->name) {
        const char *colon = strchr(n->name, ':');
        if (colon) {
            size_t len = (size_t)(colon - n->name);
            if (len < buf_sz) {
                memcpy(buf, n->name, len);
                buf[len] = '\0';
                return buf;
            }
        }
    }
    return NULL;
}

static const char *
ns_node_local_name(const ns_node *n)
{
    if (!n || n->kind != NS_NODE_ELEMENT) return NULL;
    if (n->name) {
        const char *colon = strchr(n->name, ':');
        if (colon) return colon + 1;
        return n->name;
    }
    return NULL;
}

static gboolean
ns_node_equal(const ns_node *a, const ns_node *b, int depth)
{
    if (a == b) return TRUE;
    if (!a || !b || depth >= 512) return FALSE;
    if (a->kind != b->kind) return FALSE;
    if (a->kind == NS_NODE_ELEMENT) {
        if (g_strcmp0(ns_node_namespace_uri(a), ns_node_namespace_uri(b)) != 0)
            return FALSE;
        char buf_a[64], buf_b[64];
        const char *pfx_a = ns_node_prefix(a, buf_a, sizeof(buf_a));
        const char *pfx_b = ns_node_prefix(b, buf_b, sizeof(buf_b));
        if (g_strcmp0(pfx_a, pfx_b) != 0)
            return FALSE;
        if (g_strcmp0(ns_node_local_name(a), ns_node_local_name(b)) != 0)
            return FALSE;
    } else if (a->kind == NS_NODE_COMMENT) {
        if ((a->flags & NS_NODE_PI) != (b->flags & NS_NODE_PI))
            return FALSE;
        if ((a->flags & NS_NODE_PI) && g_strcmp0(a->name, b->name) != 0)
            return FALSE;
    } else if (a->kind == NS_NODE_DOCTYPE) {
        if (g_strcmp0(a->name, b->name) != 0) return FALSE;
    }
    if ((a->text == NULL) != (b->text == NULL)) return FALSE;
    if (a->text && b->text && strcmp(a->text, b->text) != 0) return FALSE;
    if (!ns_node_attrs_equal(a, b)) return FALSE;
    const ns_node *ca = a->first_child, *cb = b->first_child;
    while (ca && cb) {
        if (!ns_node_equal(ca, cb, depth + 1)) return FALSE;
        ca = ca->next_sibling;
        cb = cb->next_sibling;
    }
    return ca == NULL && cb == NULL;
}

static JSValue
ns_element_isEqualNode(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)ctx;
    const ns_node *a = ns_unwrap_element(this_val);
    if (!a || argc < 1) return JS_FALSE;
    if (JS_IsNull(argv[0]) || JS_IsUndefined(argv[0])) return JS_FALSE;
    const ns_node *b = ns_unwrap_element(argv[0]);
    if (!b) return JS_FALSE;
    return ns_node_equal(a, b, 0) ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_isSameNode(JSContext *ctx, JSValueConst this_val,
                      int argc, JSValueConst *argv)
{
    (void)ctx;
    if (argc < 1 || !JS_IsObject(argv[0])) return JS_FALSE;
    return (JS_VALUE_GET_PTR(this_val) == JS_VALUE_GET_PTR(argv[0])) ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_compareDocumentPosition(JSContext *ctx, JSValueConst this_val,
                                   int argc, JSValueConst *argv)
{
    const ns_node *a = ns_unwrap_element(this_val);
    if (!a || argc < 1) return JS_NewInt32(ctx, 0);
    const ns_node *b = ns_unwrap_element(argv[0]);
    if (!b) return JS_NewInt32(ctx, 1);
    if (a == b) return JS_NewInt32(ctx, 0);
    for (const ns_node *p = ns_dom_tree_parent(a); p; p = ns_dom_tree_parent(p))
        if (p == b) return JS_NewInt32(ctx, 0x02 | 0x08);
    for (const ns_node *p = ns_dom_tree_parent(b); p; p = ns_dom_tree_parent(p))
        if (p == a) return JS_NewInt32(ctx, 0x04 | 0x10);
    const ns_node *anc_a = a, *anc_b = b;
    GPtrArray *pa = g_ptr_array_new();
    GPtrArray *pb = g_ptr_array_new();
    for (; anc_a; anc_a = ns_dom_tree_parent(anc_a))
        g_ptr_array_add(pa, (gpointer)anc_a);
    for (; anc_b; anc_b = ns_dom_tree_parent(anc_b))
        g_ptr_array_add(pb, (gpointer)anc_b);
    const ns_node *common = NULL;
    guint ia = pa->len, ib = pb->len;
    while (ia > 0 && ib > 0 && pa->pdata[ia - 1] == pb->pdata[ib - 1]) {
        common = pa->pdata[ia - 1];
        ia--; ib--;
    }
    int32_t result = 0x01 | 0x20 | (a < b ? 0x04 : 0x02);
    if (common && ia > 0 && ib > 0) {
        const ns_node *child_a = pa->pdata[ia - 1];
        const ns_node *child_b = pb->pdata[ib - 1];
        for (const ns_node *c = common->first_child; c; c = c->next_sibling) {
            if (c == child_a) { result = 0x04; break; }
            if (c == child_b) { result = 0x02; break; }
        }
    }
    g_ptr_array_free(pa, TRUE);
    g_ptr_array_free(pb, TRUE);
    return JS_NewInt32(ctx, result);
}

static JSValue
ns_element_get_nodeType(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_NewInt32(ctx, 0);
    if ((el->flags & NS_NODE_FRAGMENT) || ns_node_is_shadow_root(el))
        return JS_NewInt32(ctx, 11);
    switch (el->kind) {
        case NS_NODE_ELEMENT: return JS_NewInt32(ctx, 1);
        case NS_NODE_TEXT:
            return JS_NewInt32(ctx, (el->flags & NS_NODE_CDATA) ? 4 : 3);
        case NS_NODE_COMMENT:
            return JS_NewInt32(ctx, (el->flags & NS_NODE_PI) ? 7 : 8);
        case NS_NODE_DOCUMENT:return JS_NewInt32(ctx, 9);
        case NS_NODE_DOCTYPE: return JS_NewInt32(ctx, 10);
    }
    return JS_NewInt32(ctx, 0);
}

static JSValue
ns_element_get_nodeName(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_NewString(ctx, "#text");
    if ((el->flags & NS_NODE_FRAGMENT) || ns_node_is_shadow_root(el))
        return JS_NewString(ctx, "#document-fragment");
    switch (el->kind) {
        case NS_NODE_TEXT:
            return JS_NewString(ctx, (el->flags & NS_NODE_CDATA)
                                ? "#cdata-section" : "#text");
        case NS_NODE_COMMENT:
            if ((el->flags & NS_NODE_PI) && el->name)
                return JS_NewString(ctx, el->name);
            return JS_NewString(ctx, "#comment");
        case NS_NODE_DOCUMENT: return JS_NewString(ctx, "#document");
        case NS_NODE_DOCTYPE:
            return JS_NewString(ctx, el->name ? el->name : "html");
        case NS_NODE_ELEMENT:
            break;
    }
    if (!el->name) return JS_NewString(ctx, "");
    if (el->flags & NS_NODE_KEEP_CASE)
        return JS_NewString(ctx, el->name);
    {
        JSValue q = ns_element_qualified_upper(ctx, el);
        if (!JS_IsUndefined(q)) return q;
    }
    if (el->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS))
        return JS_NewString(ctx, el->name);
    for (const char *p = el->name; *p; p++)
        if (*p >= 'A' && *p <= 'Z')
            return JS_NewString(ctx, el->name);
    char *up = g_ascii_strup(el->name, -1);
    JSValue v = JS_NewString(ctx, up);
    g_free(up);
    return v;
}

static void
ns_box_lookup_cache_build_walk(GHashTable *t, const ns_box *b)
{
    if (!b) return;
    if (b->dom)
        g_hash_table_insert(t, (gpointer)b->dom, (gpointer)b);
    for (const ns_box *c = b->first_child; c; c = c->next_sibling)
        ns_box_lookup_cache_build_walk(t, c);
    if (b->inline_atomics)
        for (guint i = 0; i < b->inline_atomics->len; i++)
            ns_box_lookup_cache_build_walk(t,
                g_array_index(b->inline_atomics, ns_inline_atomic, i).box);
}

const ns_box *
ns_box_find_by_dom(const ns_box *root, const ns_node *target)
{
    if (!root || !target) return NULL;
    if (root->dom == target) return root;
    ns_js *js = ns_active_js();
    if (js && js->layout_root == root) {
        if (js->box_lookup_cache_root == root)
            return g_hash_table_lookup(js->box_lookup_cache, target);
        if (js->box_lookup_pending_root != root) {
            js->box_lookup_pending_root = root;
            js->box_lookup_pending_count = 0;
        }
        if (++js->box_lookup_pending_count >= 8) {
            if (js->box_lookup_cache)
                g_hash_table_remove_all(js->box_lookup_cache);
            else
                js->box_lookup_cache = g_hash_table_new(g_direct_hash,
                                                        g_direct_equal);
            ns_box_lookup_cache_build_walk(js->box_lookup_cache, root);
            js->box_lookup_cache_root = root;
            return g_hash_table_lookup(js->box_lookup_cache, target);
        }
    }
    for (const ns_box *c = root->first_child; c; c = c->next_sibling) {
        const ns_box *m = ns_box_find_by_dom(c, target);
        if (m) return m;
    }
    if (root->inline_atomics)
        for (guint i = 0; i < root->inline_atomics->len; i++) {
            const ns_box *m = ns_box_find_by_dom(
                g_array_index(root->inline_atomics, ns_inline_atomic, i).box,
                target);
            if (m) return m;
        }
    return NULL;
}

const ns_box *
ns_js_box_for_node(ns_js *js, const ns_node *n)
{
    return js && js->layout_root ? ns_box_find_by_dom(js->layout_root, n) : NULL;
}

void
ns_js_set_layout_root(ns_js *js, const struct ns_box *root)
{
    if (!js) return;
    if (js->layout_root != root) {
        if (js->box_lookup_cache) {
            g_hash_table_remove_all(js->box_lookup_cache);
            js->box_lookup_cache_root = NULL;
        }
        js->box_lookup_pending_root = NULL;
        js->box_lookup_pending_count = 0;
    }
    js->layout_root = root;
    ns_js_sync_window_metrics(js);
    if (root) {
        ns_js_promote_deferred_iframes(js);
        ns_intersection_observers_tick(js);
        ns_resize_observers_tick(js);
    }
}

void
ns_js_set_anim(ns_js *js, struct ns_anim *anim)
{
    if (!js) return;
    js->anim = anim;
    js->raf_host_driven = TRUE;
}

void
ns_js_set_image_cache(ns_js *js, struct ns_image_cache *cache)
{
    if (!js) return;
    js->image_cache = (ns_image_cache *)cache;
    if (cache) ns_js_blob_registry_add(js);
    else ns_js_blob_registry_remove(js);
}

static JSValue
ns_element_get_zero_int(JSContext *ctx, JSValueConst this_val)
{
    (void)this_val;
    return JS_NewInt32(ctx, 0);
}

static JSValue
ns_element_get_isContentEditable(JSContext *ctx, JSValueConst this_val)
{
    (void)ctx;
    const ns_node *n = ns_unwrap_element(this_val);
    for (const ns_node *p = n; p; p = p->parent) {
        const char *v = ns_element_get_attr(p, "contenteditable");
        if (!v) continue;
        if (g_ascii_strcasecmp(v, "false") == 0)
            return JS_FALSE;
        if (*v == '\0' || g_ascii_strcasecmp(v, "true") == 0 ||
            g_ascii_strcasecmp(v, "plaintext-only") == 0)
            return JS_TRUE;
    }
    return JS_FALSE;
}

static const ns_image *
ns_image_for_element(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || !n->name || strcmp(n->name, "img") != 0) return NULL;
    return ns_js_image_for_node(js_from_ctx(ctx), n);
}

static JSValue
ns_element_img_complete(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || !n->name || strcmp(n->name, "img") != 0) return JS_TRUE;
    const char *src = ns_element_get_attr(n, "src");
    if (!src || !*src) return JS_TRUE;
    const ns_image *im = ns_image_for_element(ctx, this_val);
    if (!im) return JS_FALSE;
    return (im->loaded || im->failed) ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_img_natural_dimension(JSContext *ctx, JSValueConst this_val, gboolean width)
{
    const ns_image *im = ns_image_for_element(ctx, this_val);
    int natural = im ? (width ? im->natural_width : im->natural_height) : 0;
    if (natural <= 0) return JS_NewInt32(ctx, 0);
    double density = ns_img_chosen_density(ns_unwrap_element(this_val));
    return JS_NewInt32(ctx, (int32_t)(natural / density));
}

JSValue
ns_element_img_natural_width(JSContext *ctx, JSValueConst this_val)
{
    return ns_img_natural_dimension(ctx, this_val, TRUE);
}

static JSValue
ns_element_img_current_src(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_NewString(ctx, "");
    char *chosen = ns_img_chosen_url(n);
    if (!chosen || !*chosen) { g_free(chosen); return JS_NewString(ctx, ""); }
    ns_js *js = js_from_ctx(ctx);
    const char *base = js ? ns_js_node_doc_base(js, n) : NULL;
    char *abs_url = base ? ns_url_resolve(base, chosen) : NULL;
    JSValue r = JS_NewString(ctx, abs_url ? abs_url : chosen);
    g_free(abs_url);
    g_free(chosen);
    return r;
}

JSValue
ns_element_img_natural_height(JSContext *ctx, JSValueConst this_val)
{
    return ns_img_natural_dimension(ctx, this_val, FALSE);
}

static JSValue
ns_element_get_empty_array_prop(JSContext *ctx, JSValueConst this_val)
{
    (void)this_val;
    return JS_NewArray(ctx);
}

static const JSCFunctionListEntry ns_validity_proto_funcs[] = {
    JS_CGETSET_DEF("valid", ns_validity_get_valid, NULL),
};

static void
ns_collect_labels_for(JSContext *ctx, const ns_node *scan, const ns_node *n,
                      JSValue arr, uint32_t *idx, int depth)
{
    if (!scan || depth >= 512) return;
    if (ns_node_is_element_named(scan, "label") &&
        ns_label_associated_control(scan) == n)
        JS_SetPropertyUint32(ctx, arr, (*idx)++, ns_make_element(ctx, scan));
    for (const ns_node *c = scan->first_child; c; c = c->next_sibling)
        ns_collect_labels_for(ctx, c, n, arr, idx, depth + 1);
}

static JSValue
ns_element_get_labels(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!ns_js_node_is_labelable(n)) return JS_NULL;
    return ns_make_live(ctx, this_val, NS_LIVE_LABELS, NULL);
}

static JSValue
ns_element_get_form_enctype(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    const char *v = n ? ns_element_get_attr(n, "enctype") : NULL;
    return JS_NewString(ctx, ns_enum_normalize("enctype", v));
}

static JSValue
ns_element_set_form_enctype(JSContext *ctx, JSValueConst this_val,
                            JSValueConst val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n) return JS_UNDEFINED;
    size_t len = 0;
    const char *s = JS_ToCStringLen(ctx, &len, val);
    if (s) {
        ns_js_set_attr_recorded_len(js_from_ctx(ctx), n, "enctype", s,
                                    (gssize)len);
        JS_FreeCString(ctx, s);
    }
    return JS_UNDEFINED;
}

static JSValue
ns_element_get_isConnected(JSContext *ctx, JSValueConst this_val)
{
    (void)ctx;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_FALSE;
    const ns_node *root = n;
    while (root->parent) root = root->parent;
    return root->kind == NS_NODE_DOCUMENT && !(root->flags & NS_NODE_FRAGMENT)
        ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_get_baseURI(JSContext *ctx, JSValueConst this_val)
{
    (void)this_val;
    ns_js *js = js_from_ctx(ctx);
    if (!js) return JS_NewString(ctx, "about:blank");
    g_autofree char *base = ns_js_doc_base_url(js);
    return JS_NewString(ctx, base && *base ? base : "about:blank");
}

static JSValue
ns_element_get_ownerDocument(JSContext *ctx, JSValueConst this_val)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js) return JS_NULL;
    const ns_node *el = ns_unwrap_element(this_val);
    if (el && el->kind == NS_NODE_DOCUMENT && !(el->flags & NS_NODE_FRAGMENT))
        return JS_NULL;
    ns_node *attr_owner = ns_attr_owner(this_val);
    if (attr_owner) {
        JSValue owner_elem = ns_make_element(ctx, attr_owner);
        JSValue doc = ns_element_get_ownerDocument(ctx, owner_elem);
        JS_FreeValue(ctx, owner_elem);
        return doc;
    }
    if (el) {
        const ns_node *doc_anc = NULL;
        for (const ns_node *p = el->parent; p; p = p->parent)
            if (p->kind == NS_NODE_DOCUMENT && !(p->flags & NS_NODE_FRAGMENT)) {
                doc_anc = p;
                break;
            }
        if (doc_anc) {
            if (doc_anc != js->current_doc) {
                const ns_node *host = doc_anc->parent;
                if (host && host->js_wrapper &&
                    (ns_node_is_element_named(host, "iframe") ||
                     ns_node_is_element_named(host, "object"))) {
                    JSValue hw = JS_MKPTR(JS_TAG_OBJECT, host->js_wrapper);
                    JSValue realm = JS_GetPropertyStr(ctx, hw, "__ndRealmDoc");
                    if (JS_IsObject(realm)) return realm;
                    JS_FreeValue(ctx, realm);
                }
                return ns_make_element(ctx, doc_anc);
            }
            return ns_make_element(ctx, js->current_doc);
        }
    }
    JSValue own = JS_GetPropertyStr(ctx, this_val, "__ndOwnerDoc");
    if (JS_IsObject(own)) return own;
    JS_FreeValue(ctx, own);
    JSValue noown = JS_GetPropertyStr(ctx, this_val, "__ndNoOwnerDoc");
    gboolean no_owner = JS_ToBool(ctx, noown) > 0;
    JS_FreeValue(ctx, noown);
    if (no_owner) return JS_NULL;
    if (!js->current_doc) return JS_NULL;
    return ns_make_element(ctx, js->current_doc);
}

static JSValue
ns_element_get_namespaceURI(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (n && n->kind == NS_NODE_ELEMENT) {
        const char *stored = ns_element_get_attr(n, "data-nd-ns-uri");
        if (stored) return JS_NewString(ctx, stored);
    }
    if (n && (n->flags & NS_NODE_SVG_NS))
        return JS_NewString(ctx, "http://www.w3.org/2000/svg");
    if (n && (n->flags & NS_NODE_FOREIGN_NS))
        return JS_NULL;
    return JS_NewString(ctx, "http://www.w3.org/1999/xhtml");
}

static JSValue
ns_element_get_null(JSContext *ctx, JSValueConst this_val)
{
    (void)ctx; (void)this_val;
    return JS_NULL;
}

static JSValue
ns_input_get_files(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *n = ns_unwrap_element(this_val);
    if (!ns_node_is_element_named(n, "input")) return JS_NULL;
    const char *type = ns_element_get_attr(n, "type");
    if (!type || g_ascii_strcasecmp(type, "file") != 0) return JS_NULL;
    g_autofree char *path = g_strdup(ns_element_get_attr(n, "data-nd-file-path"));

    JSValue cached_path = JS_GetPropertyStr(ctx, this_val, "__nd_files_path");
    const char *cp = JS_IsString(cached_path) ? JS_ToCString(ctx, cached_path) : NULL;
    gboolean same = (!path && !cp) ||
                    (path && cp && strcmp(path, cp) == 0);
    if (cp) JS_FreeCString(ctx, cp);
    JS_FreeValue(ctx, cached_path);
    if (same) {
        JSValue cached = JS_GetPropertyStr(ctx, this_val, "__nd_files_cache");
        if (JS_IsArray(cached)) return cached;
        JS_FreeValue(ctx, cached);
    }

    JSValue arr = JS_NewArray(ctx);
    if (path && *path) {
        char *contents = NULL;
        gsize len = 0;
        GError *err = NULL;
        if (g_file_get_contents(path, &contents, &len, &err)) {
            const char *base = strrchr(path, '/');
#ifdef G_OS_WIN32
            const char *base_w = strrchr(path, '\\');
            if (!base || (base_w && base_w > base)) base = base_w;
#endif
            const char *bname = base ? base + 1 : path;

            g_autofree char *mime = g_content_type_guess(path,
                (const guchar *)contents, len < 4096 ? len : 4096, NULL);
            g_autofree char *mime_type = mime ? g_content_type_get_mime_type(mime) : NULL;

            JSValue global = JS_GetGlobalObject(ctx);
            JSValue file_ctor = JS_GetPropertyStr(ctx, global, "File");
            JS_FreeValue(ctx, global);
            if (JS_IsConstructor(ctx, file_ctor)) {
                JSValue ab = JS_NewArrayBufferCopy(ctx,
                    (const uint8_t *)contents, len);
                JSValue parts = JS_NewArray(ctx);
                JS_SetPropertyUint32(ctx, parts, 0, ab);
                JSValue opts = JS_NewObject(ctx);
                JS_SetPropertyStr(ctx, opts, "type",
                    JS_NewString(ctx, mime_type && *mime_type
                                       ? mime_type : "application/octet-stream"));
                JSValue name_str = JS_NewString(ctx, bname);
                JSValueConst args[3] = { parts, name_str, opts };
                JSValue file = JS_CallConstructor(ctx, file_ctor, 3, args);
                if (!JS_IsException(file))
                    JS_SetPropertyUint32(ctx, arr, 0, file);
                else
                    JS_FreeValue(ctx, JS_GetException(ctx));
                JS_FreeValue(ctx, parts);
                JS_FreeValue(ctx, name_str);
                JS_FreeValue(ctx, opts);
            }
            JS_FreeValue(ctx, file_ctor);
            g_free(contents);
        } else if (err) {
            g_error_free(err);
        }
    }
    JS_SetPropertyStr(ctx, arr, "item",
        JS_NewCFunction(ctx, ns_namedmap_item, "item", 1));

    JS_DefinePropertyValueStr(ctx, this_val, "__nd_files_path",
        JS_NewString(ctx, path ? path : ""),
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    JS_DefinePropertyValueStr(ctx, this_val, "__nd_files_cache",
        JS_DupValue(ctx, arr),
        JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    return arr;
}

static JSValue
ns_element_noop_set(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    (void)ctx; (void)this_val; (void)val;
    return JS_UNDEFINED;
}

static JSValue
ns_element_get_hidden(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_FALSE;
    const char *hidden = ns_element_get_attr(el, "hidden");
    if (!hidden) return JS_FALSE;
    if (g_ascii_strcasecmp(hidden, "until-found") == 0)
        return JS_NewString(ctx, "until-found");
    return JS_TRUE;
}

static JSValue
ns_element_set_hidden(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    ns_node *el = ns_unwrap_element_mut(this_val);
    if (!el) return JS_UNDEFINED;
    ns_js *_j = js_from_ctx(ctx);
    if (JS_IsString(val)) {
        size_t len = 0;
        const char *s = JS_ToCStringLen(ctx, &len, val);
        if (s && len == 11 && g_ascii_strncasecmp(s, "until-found", 11) == 0)
            ns_js_set_attr_recorded(_j, el, "hidden", "until-found");
        else if (!s || len == 0)
            ns_js_remove_attr_recorded(_j, el, "hidden");
        else
            ns_js_set_attr_recorded(_j, el, "hidden", "");
        if (s) JS_FreeCString(ctx, s);
    } else if (JS_ToBool(ctx, val)) {
        ns_js_set_attr_recorded(_j, el, "hidden", "");
    } else {
        ns_js_remove_attr_recorded(_j, el, "hidden");
    }
    return JS_UNDEFINED;
}

static JSValue
ns_element_get_disabled(JSContext *ctx, JSValueConst this_val)
{
    (void)ctx;
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_FALSE;
    if (ns_node_is_element_named(el, "style"))
        return (el->flags & NS_NODE_SHEET_DISABLED) ? JS_TRUE : JS_FALSE;
    return ns_element_get_attr(el, "disabled") ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_set_disabled(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    ns_node *el = ns_unwrap_element_mut(this_val);
    if (!el) return JS_UNDEFINED;
    ns_js *_j = js_from_ctx(ctx);
    gboolean disabled = JS_ToBool(ctx, val);
    if (ns_node_is_element_named(el, "style")) {
        guint32 flags = disabled ? el->flags | NS_NODE_SHEET_DISABLED
                                 : el->flags & ~NS_NODE_SHEET_DISABLED;
        if (flags != el->flags && _j) _j->mutated = TRUE;
        el->flags = flags;
        return JS_UNDEFINED;
    }
    if (disabled) ns_js_set_attr_recorded(_j, el, "disabled", "");
    else          ns_js_remove_attr_recorded(_j, el, "disabled");
    return JS_UNDEFINED;
}

static int
ns_live_get_own_hook(JSContext *ctx, JSPropertyDescriptor *desc,
                     JSValueConst obj, JSAtom prop)
{
    void *b = JS_GetOpaque(obj, ns_live_class_id);
    if (!b) return 0;
    uint32_t idx = 0;
    gboolean is_index = JS_AtomIsArrayIndex(ctx, &idx, prop);
    const char *name = NULL;
    if (!is_index) {
        if (!ns_live_named_access(b)) return 0;
        name = JS_AtomToCString(ctx, prop);
        if (!name) return 0;
        gboolean proto_has = strcmp(name, "length") == 0;
        if (!proto_has) {
            JSValue proto = JS_GetPrototype(ctx, obj);
            proto_has = JS_IsObject(proto) && JS_HasProperty(ctx, proto, prop) > 0;
            JS_FreeValue(ctx, proto);
        }
        if (proto_has) {
            JS_FreeCString(ctx, name);
            return 0;
        }
    }
    JSValue value = JS_UNDEFINED;
    int found = ns_live_get_own(ctx, b, is_index, idx, name, &value);
    if (name) JS_FreeCString(ctx, name);
    if (!found) return 0;
    if (desc) {
        desc->flags  = is_index ? (JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE)
                                : JS_PROP_CONFIGURABLE;
        desc->value  = value;
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    } else {
        JS_FreeValue(ctx, value);
    }
    return 1;
}

static int
ns_live_get_own_names_hook(JSContext *ctx, JSPropertyEnum **ptab,
                           uint32_t *plen, JSValueConst obj)
{
    *ptab = NULL;
    *plen = 0;
    void *b = JS_GetOpaque(obj, ns_live_class_id);
    if (!b) return 0;
    GPtrArray *named = g_ptr_array_new_with_free_func(g_free);
    uint32_t len = ns_live_own_names(ctx, b, obj, named);
    uint32_t total = len + named->len;
    if (total == 0) {
        g_ptr_array_free(named, TRUE);
        return 0;
    }
    JSPropertyEnum *tab = js_malloc(ctx, sizeof(JSPropertyEnum) * total);
    if (!tab) {
        g_ptr_array_free(named, TRUE);
        return -1;
    }
    for (uint32_t i = 0; i < len; i++) {
        tab[i].atom = JS_NewAtomUInt32(ctx, i);
        tab[i].is_enumerable = 1;
    }
    for (guint i = 0; i < named->len; i++) {
        tab[len + i].atom = JS_NewAtom(ctx, g_ptr_array_index(named, i));
        tab[len + i].is_enumerable = 0;
    }
    g_ptr_array_free(named, TRUE);
    *ptab = tab;
    *plen = total;
    return 0;
}

static int
ns_live_delete_hook(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    void *b = JS_GetOpaque(obj, ns_live_class_id);
    if (!b) return 1;
    uint32_t idx = 0;
    if (JS_AtomIsArrayIndex(ctx, &idx, prop))
        return ns_live_delete(ctx, b, TRUE, idx, NULL);
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 1;
    int ret = ns_live_delete(ctx, b, FALSE, 0, name);
    JS_FreeCString(ctx, name);
    return ret;
}

static int
ns_live_define_hook(JSContext *ctx, JSValueConst this_obj, JSAtom prop,
                    JSValueConst val, JSValueConst getter,
                    JSValueConst setter, int flags)
{
    void *b = JS_GetOpaque(this_obj, ns_live_class_id);
    if (b) {
        uint32_t idx = 0;
        if (JS_AtomIsArrayIndex(ctx, &idx, prop)) return 0;
        const char *name = JS_AtomToCString(ctx, prop);
        if (name) {
            int reject = ns_live_define_rejects(ctx, b, name);
            JS_FreeCString(ctx, name);
            if (reject) {
                if (flags & JS_PROP_THROW) {
                    JS_ThrowTypeError(ctx, "Cannot define property on this object");
                    return -1;
                }
                return FALSE;
            }
        }
    }
    return JS_DefineProperty(ctx, this_obj, prop, val, getter, setter,
                             flags | JS_PROP_NO_EXOTIC);
}

static void
ns_live_finalizer(JSRuntime *rt, JSValue val)
{
    (void)rt;
    ns_live_back_free(JS_GetOpaque(val, ns_live_class_id));
}

static void
ns_live_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    void *b = JS_GetOpaque(val, ns_live_class_id);
    if (!b) return;
    JS_MarkValue(rt, ns_live_back_owner(b), mark_func);
    JS_MarkValue(rt, ns_live_back_cache(b), mark_func);
}

static JSClassExoticMethods ns_live_exotic = {
    .get_own_property       = ns_live_get_own_hook,
    .get_own_property_names = ns_live_get_own_names_hook,
    .delete_property        = ns_live_delete_hook,
    .define_own_property    = ns_live_define_hook,
};

static JSClassDef ns_live_class = {
    .class_name = "HTMLCollection",
    .finalizer  = ns_live_finalizer,
    .gc_mark    = ns_live_gc_mark,
    .exotic     = &ns_live_exotic,
};

JSValue
ns_live_object_new(JSContext *ctx, void *back)
{
    JSValue obj = JS_NewObjectClass(ctx, ns_live_class_id);
    if (!JS_IsException(obj)) JS_SetOpaque(obj, back);
    return obj;
}

void *
ns_live_back_of(JSValueConst obj)
{
    return JS_GetOpaque(obj, ns_live_class_id);
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

JSValue
ns_live_build_labels(JSContext *ctx, const ns_node *n)
{
    JSValue arr = JS_NewArray(ctx);
    uint32_t i = 0;
    ns_collect_labels_for(ctx, ns_node_root(n), n, arr, &i, 0);
    return arr;
}

static JSValue
ns_element_get_option_index(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *opt = ns_unwrap_element(this_val);
    if (!opt) return JS_NewInt32(ctx, -1);
    const ns_node *sel = NULL;
    for (const ns_node *p = opt->parent; p; p = p->parent) {
        if (ns_node_is_element_named(p, "select")) { sel = p; break; }
    }
    if (!sel) return JS_NewInt32(ctx, -1);
    int idx = 0;
    for (const ns_node *c = sel->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
        if (strcmp(c->name, "option") == 0) {
            if (c == opt) return JS_NewInt32(ctx, idx);
            idx++;
        } else if (strcmp(c->name, "optgroup") == 0) {
            for (const ns_node *cc = c->first_child; cc; cc = cc->next_sibling) {
                if (ns_node_is_element_named(cc, "option")) {
                    if (cc == opt) return JS_NewInt32(ctx, idx);
                    idx++;
                }
            }
        }
    }
    return JS_NewInt32(ctx, -1);
}

static JSValue
ns_element_get_select_length(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *sel = ns_unwrap_element(this_val);
    if (!sel || !sel->name || strcmp(sel->name, "select") != 0)
        return JS_NewInt32(ctx, 0);
    int count = 0;
    for (const ns_node *c = sel->first_child; c; c = c->next_sibling) {
        if (c->kind != NS_NODE_ELEMENT || !c->name) continue;
        if (strcmp(c->name, "option") == 0) count++;
        else if (strcmp(c->name, "optgroup") == 0) {
            for (const ns_node *cc = c->first_child; cc; cc = cc->next_sibling)
                if (ns_node_is_element_named(cc, "option")) count++;
        }
    }
    return JS_NewInt32(ctx, count);
}

enum {
    NS_ANCHOR_HREF = 0,
    NS_ANCHOR_PROTOCOL,
    NS_ANCHOR_HOST,
    NS_ANCHOR_HOSTNAME,
    NS_ANCHOR_PORT,
    NS_ANCHOR_PATHNAME,
    NS_ANCHOR_SEARCH,
    NS_ANCHOR_HASH,
    NS_ANCHOR_ORIGIN,
    NS_ANCHOR_USERNAME,
    NS_ANCHOR_PASSWORD,
};

static char *
ns_js_document_base_url(ns_js *js, const ns_node *doc, const char *current_url)
{
    if (doc) {
        GPtrArray *bases = ns_doc_tag_index_lookup(doc, "base");
        for (guint i = 0; bases && i < bases->len; i++) {
            const ns_node *b = g_ptr_array_index(bases, i);
            const char *bh = ns_element_get_attr(b, "href");
            if (!bh || !*bh) continue;
            if (current_url && *current_url) {
                char *r = ns_url_resolve(current_url, bh);
                if (r) {
                    if (js->csp &&
                        !ns_csp_allows(js->csp, NS_CSP_BASE_URI, r,
                                       current_url)) {
                        g_free(r);
                        continue;
                    }
                    return r;
                }
            }
            if (js->csp &&
                !ns_csp_allows(js->csp, NS_CSP_BASE_URI, bh, current_url))
                continue;
            return g_strdup(bh);
        }
    }
    return current_url && *current_url ? g_strdup(current_url) : NULL;
}

char *
ns_js_doc_base_url(ns_js *js)
{
    if (!js) return NULL;
    return ns_js_document_base_url(js, js->current_doc, js->current_url);
}

static JSValue
ns_element_list_set(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    ns_node *el = ns_unwrap_element_mut(this_val);
    if (ns_node_is_custom_element(el))
        JS_DefinePropertyValueStr(ctx, this_val, "list",
                                  JS_DupValue(ctx, val), JS_PROP_C_W_E);
    return JS_UNDEFINED;
}

static JSValue
ns_element_get_list_ref(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_NULL;
    if (ns_node_is_custom_element(el)) return JS_UNDEFINED;
    const char *id = ns_element_get_attr(el, "list");
    if (!id || !*id) return JS_NULL;
    ns_js *_j = js_from_ctx(ctx);
    if (!_j || !_j->current_doc) return JS_NULL;
    ns_node *found = ns_node_find_by_id(_j->current_doc, id);
    if (!found || !found->name ||
        g_ascii_strcasecmp(found->name, "datalist") != 0)
        return JS_NULL;
    return ns_make_element(ctx, found);
}

static JSValue
ns_element_template_content(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *t = ns_unwrap_element(this_val);
    if (!t || !t->name) return JS_NewString(ctx, "");
    if (g_ascii_strcasecmp(t->name, "template") == 0) {
        JSValue cached = JS_GetPropertyStr(ctx, this_val, "__nd_template_content");
        if (JS_IsException(cached)) return cached;
        if (!JS_IsUndefined(cached)) return cached;
        JS_FreeValue(ctx, cached);
        ns_node *frag = ns_template_content_get(ns_unwrap_element_mut(this_val));
        if (!frag) return JS_NULL;
        JSValue wrapped = ns_make_element(ctx, frag);
        JS_DefinePropertyValueStr(ctx, this_val, "__nd_template_content",
                                  JS_DupValue(ctx, wrapped),
                                  JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
        return wrapped;
    }
    return ns_element_reflect_str_get(ctx, this_val, "content", FALSE);
}

static JSValue
ns_element_set_content(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    if (ns_node_is_element_named(ns_unwrap_element(this_val), "template"))
        return JS_UNDEFINED;
    return ns_element_reflect_str_set(ctx, this_val, val, "content");
}

static gboolean
ns_element_reflects_rows(const ns_node *n)
{
    return ns_node_is_element_named(n, "textarea") ||
           ns_node_is_element_named(n, "frameset");
}

static JSValue
ns_element_set_rows(JSContext *ctx, JSValueConst this_val, JSValueConst val)
{
    if (!ns_element_reflects_rows(ns_unwrap_element(this_val)))
        return JS_UNDEFINED;
    return ns_element_int_attr_setter(ctx, this_val, val,
                                      ns_int_attr_index("rows"));
}

static JSValue
ns_element_table_rows(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *tbl = ns_unwrap_element(this_val);
    if (ns_element_reflects_rows(tbl))
        return ns_element_int_attr_getter(ctx, this_val,
                                          ns_int_attr_index("rows"));
    JSValue arr = JS_NewArray(ctx);
    if (!tbl) return arr;
    uint32_t idx = 0;
    for (const ns_node *c = tbl->first_child; c; c = c->next_sibling)
        if (ns_node_is_element_named(c, "thead"))
            for (const ns_node *r = c->first_child; r; r = r->next_sibling)
                if (ns_node_is_element_named(r, "tr"))
                    JS_SetPropertyUint32(ctx, arr, idx++,
                                         ns_make_element(ctx, r));
    for (const ns_node *c = tbl->first_child; c; c = c->next_sibling) {
        if (ns_node_is_element_named(c, "tr"))
            JS_SetPropertyUint32(ctx, arr, idx++, ns_make_element(ctx, c));
        else if (ns_node_is_element_named(c, "tbody"))
            for (const ns_node *r = c->first_child; r; r = r->next_sibling)
                if (ns_node_is_element_named(r, "tr"))
                    JS_SetPropertyUint32(ctx, arr, idx++,
                                         ns_make_element(ctx, r));
    }
    for (const ns_node *c = tbl->first_child; c; c = c->next_sibling)
        if (ns_node_is_element_named(c, "tfoot"))
            for (const ns_node *r = c->first_child; r; r = r->next_sibling)
                if (ns_node_is_element_named(r, "tr"))
                    JS_SetPropertyUint32(ctx, arr, idx++,
                                         ns_make_element(ctx, r));
    return arr;
}

static JSValue
ns_element_table_section(JSContext *ctx, JSValueConst this_val, const char *tag)
{
    const ns_node *tbl = ns_unwrap_element(this_val);
    if (!tbl) return JS_NULL;
    for (const ns_node *c = tbl->first_child; c; c = c->next_sibling)
        if (ns_node_is_element_named(c, tag))
            return ns_make_element(ctx, c);
    return JS_NULL;
}

static JSValue
ns_element_table_caption(JSContext *ctx, JSValueConst this_val)
{ return ns_element_table_section(ctx, this_val, "caption"); }

static JSValue
ns_element_table_thead(JSContext *ctx, JSValueConst this_val)
{ return ns_element_table_section(ctx, this_val, "thead"); }

static JSValue
ns_element_table_tfoot(JSContext *ctx, JSValueConst this_val)
{ return ns_element_table_section(ctx, this_val, "tfoot"); }

static JSValue
ns_element_table_tbodies(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *tbl = ns_unwrap_element(this_val);
    JSValue arr = JS_NewArray(ctx);
    if (!tbl) return arr;
    uint32_t i = 0;
    for (const ns_node *c = tbl->first_child; c; c = c->next_sibling)
        if (ns_node_is_element_named(c, "tbody"))
            JS_SetPropertyUint32(ctx, arr, i++, ns_make_element(ctx, c));
    return arr;
}

static JSValue
ns_element_tr_cells(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *tr = ns_unwrap_element(this_val);
    JSValue arr = JS_NewArray(ctx);
    if (!tr) return arr;
    uint32_t i = 0;
    for (const ns_node *c = tr->first_child; c; c = c->next_sibling)
        if (ns_node_is_element_named(c, "td") ||
            ns_node_is_element_named(c, "th"))
            JS_SetPropertyUint32(ctx, arr, i++, ns_make_element(ctx, c));
    return arr;
}

static JSValue
ns_element_get_form_elements(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_live(ctx, this_val, NS_LIVE_FORM_ELEMENTS, NULL);
}

static JSValue
ns_form_controls_snapshot(JSContext *ctx, JSValueConst form)
{
    const ns_node *node = ns_unwrap_element(form);
    JSValue cached = ns_qcache_get(ctx, node, 'f', "");
    if (!JS_IsUndefined(cached)) return cached;
    JSValue live = ns_element_get_form_elements(ctx, form);
    JSValue snap = ns_live_snapshot(ctx, live);
    JS_FreeValue(ctx, live);
    ns_qcache_put(ctx, node, 'f', "", snap);
    return snap;
}

static JSValue
ns_element_get_form(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el) return JS_NULL;
    if (ns_node_is_element_named(el, "label"))
        el = ns_label_associated_control(el);
    if (!el) return JS_NULL;
    ns_js *_j = js_from_ctx(ctx);
    const ns_node *form = ns_form_owner(el, _j ? _j->current_doc : NULL);
    return form ? ns_make_element(ctx, form) : JS_NULL;
}

JSValue
ns_array_item(JSContext *ctx, JSValueConst this_val,
              int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_NULL;
    int32_t idx = 0;
    JS_ToInt32(ctx, &idx, argv[0]);
    if (idx < 0) return JS_NULL;
    JSValue v = JS_GetPropertyUint32(ctx, this_val, (uint32_t)idx);
    if (JS_IsUndefined(v)) { JS_FreeValue(ctx, v); return JS_NULL; }
    return v;
}

JSValue
ns_array_namedItem(JSContext *ctx, JSValueConst this_val,
                   int argc, JSValueConst *argv)
{
    if (argc < 1 || !JS_IsString(argv[0])) return JS_NULL;
    const char *name = JS_ToCString(ctx, argv[0]);
    if (!name) return JS_NULL;
    if (!*name) { JS_FreeCString(ctx, name); return JS_NULL; }
    JSValue result = JS_NULL;
    JSValue lenv = JS_GetPropertyStr(ctx, this_val, "length");
    uint32_t n = 0; JS_ToUint32(ctx, &n, lenv); JS_FreeValue(ctx, lenv);
    for (uint32_t i = 0; i < n; i++) {
        JSValue item = JS_GetPropertyUint32(ctx, this_val, i);
        const ns_node *el = ns_unwrap_element(item);
        if (el) {
            const char *id = ns_element_get_attr(el, "id");
            const char *nm = ns_element_get_attr(el, "name");
            if ((id && strcmp(id, name) == 0) ||
                (nm && strcmp(nm, name) == 0)) {
                result = item;
                break;
            }
        }
        JS_FreeValue(ctx, item);
    }
    JS_FreeCString(ctx, name);
    return result;
}

static gboolean
ns_node_is_radio_input(const ns_node *el)
{
    if (!ns_node_is_element_named(el, "input")) return FALSE;
    const char *type = ns_element_get_attr(el, "type");
    return type && g_ascii_strcasecmp(type, "radio") == 0;
}

static JSValue
ns_radio_node_list_get_value(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue snap = ns_live_snapshot(ctx, this_val);
    uint32_t n = ns_js_array_length(ctx, snap);
    for (uint32_t i = 0; i < n; i++) {
        JSValue item = JS_GetPropertyUint32(ctx, snap, i);
        const ns_node *el = ns_unwrap_element(item);
        if (ns_node_is_radio_input(el) && ns_input_is_checked(el)) {
            const char *value = ns_element_get_attr(el, "value");
            JSValue result = JS_NewString(ctx, value ? value : "on");
            JS_FreeValue(ctx, item);
            JS_FreeValue(ctx, snap);
            return result;
        }
        JS_FreeValue(ctx, item);
    }
    JS_FreeValue(ctx, snap);
    return JS_NewString(ctx, "");
}

static JSValue
ns_radio_node_list_set_value(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_UNDEFINED;
    const char *wanted = JS_ToCString(ctx, argv[0]);
    if (!wanted) return JS_UNDEFINED;
    JSValue snap = ns_live_snapshot(ctx, this_val);
    uint32_t n = ns_js_array_length(ctx, snap);
    ns_node *chosen = NULL;
    for (uint32_t i = 0; i < n; i++) {
        JSValue item = JS_GetPropertyUint32(ctx, snap, i);
        ns_node *el = (ns_node *)ns_unwrap_element(item);
        if (ns_node_is_radio_input(el)) {
            const char *value = ns_element_get_attr(el, "value");
            if (strcmp(value ? value : "on", wanted) == 0 && !chosen)
                chosen = el;
        }
        JS_FreeValue(ctx, item);
    }
    if (chosen) {
        ns_js *js = js_from_ctx(ctx);
        for (uint32_t i = 0; i < n; i++) {
            JSValue item = JS_GetPropertyUint32(ctx, snap, i);
            ns_node *el = (ns_node *)ns_unwrap_element(item);
            if (ns_node_is_radio_input(el) && el != chosen)
                ns_js_set_checkedness(js, el, FALSE);
            JS_FreeValue(ctx, item);
        }
        ns_js_set_checkedness(js, chosen, TRUE);
    }
    JS_FreeValue(ctx, snap);
    JS_FreeCString(ctx, wanted);
    return JS_UNDEFINED;
}

JSValue
ns_form_elements_named_lookup(JSContext *ctx, JSValueConst this_val,
                              const char *name)
{
    if (!name) return JS_NULL;
    JSValue first = JS_NULL;
    uint32_t count = 0;
    JSValue snap = ns_live_snapshot(ctx, this_val);
    uint32_t n = ns_js_array_length(ctx, snap);
    for (uint32_t i = 0; i < n; i++) {
        JSValue item = JS_GetPropertyUint32(ctx, snap, i);
        const ns_node *el = ns_unwrap_element(item);
        gboolean match = FALSE;
        if (el) {
            const char *id = ns_element_get_attr(el, "id");
            const char *nm = ns_element_get_attr(el, "name");
            match = (id && strcmp(id, name) == 0) ||
                    (nm && strcmp(nm, name) == 0);
        }
        if (match) {
            count++;
            if (count == 1) {
                first = item;
            } else {
                JS_FreeValue(ctx, item);
            }
        } else {
            JS_FreeValue(ctx, item);
        }
    }
    JS_FreeValue(ctx, snap);
    if (count > 1) {
        ns_node *first_node = (ns_node *)ns_unwrap_element(first);
        const ns_node *form = ns_form_owner(first_node, NULL);
        JSValue form_val = form ? ns_make_element(ctx, form) : JS_UNDEFINED;
        void *b = ns_live_back_of(this_val);
        if (!JS_IsObject(form_val) && b && JS_IsObject(ns_live_back_owner(b)))
            form_val = JS_DupValue(ctx, ns_live_back_owner(b));
        if (JS_IsObject(form_val)) {
            JSValue rnl = JS_UNDEFINED;
            JSValue rnl_map = JS_GetPropertyStr(ctx, form_val, "_ns_rnl");
            if (JS_IsObject(rnl_map)) {
                rnl = JS_GetPropertyStr(ctx, rnl_map, name);
            } else {
                rnl_map = JS_NewObject(ctx);
                JS_DefinePropertyValueStr(ctx, form_val, "_ns_rnl", JS_DupValue(ctx, rnl_map), 0);
            }
            if (JS_IsUndefined(rnl)) {
                rnl = ns_make_live(ctx, form_val, NS_LIVE_RADIO_NODE_LIST, name);
                JS_DefinePropertyValueStr(ctx, rnl_map, name, JS_DupValue(ctx, rnl), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
            }
            JS_FreeValue(ctx, rnl_map);
            JS_FreeValue(ctx, form_val);
            return rnl;
        }
        return ns_make_live(ctx, JS_UNDEFINED, NS_LIVE_RADIO_NODE_LIST, name);
    }
    return first;
}

static gboolean
ns_live_is_array_index(const char *name)
{
    if (!name || !*name) return FALSE;
    for (const char *p = name; *p; p++) {
        if (*p < '0' || *p > '9') return FALSE;
    }
    if (name[0] == '0' && name[1] != '\0') return FALSE;
    char *end = NULL;
    long long idx = strtoll(name, &end, 10);
    return end && *end == '\0' && idx >= 0 && idx <= 4294967294LL;
}

static int
ns_element_named_get_own(JSContext *ctx, JSPropertyDescriptor *desc,
                         JSValueConst obj, JSAtom prop)
{
    const ns_node *n = ns_unwrap_element(obj);
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name ||
        strcmp(n->name, "form") != 0)
        return 0;
    JSValue keyv = JS_AtomToValue(ctx, prop);
    gboolean is_str = JS_IsString(keyv);
    JS_FreeValue(ctx, keyv);
    if (!is_str) return 0;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 0;
    if (!*name || strncmp(name, "_ns_", 4) == 0 || strncmp(name, "__", 2) == 0) {
        JS_FreeCString(ctx, name);
        return 0;
    }
    if (ns_live_is_array_index(name)) {
        char *end = NULL;
        unsigned long idx = strtoul(name, &end, 10);
        JS_FreeCString(ctx, name);
        JSValue elements = ns_form_controls_snapshot(ctx, obj);
        uint32_t len = ns_js_array_length(ctx, elements);
        if ((unsigned long)idx >= len) { JS_FreeValue(ctx, elements); return 0; }
        JSValue el = JS_GetPropertyUint32(ctx, elements, (uint32_t)idx);
        JS_FreeValue(ctx, elements);
        if (desc) {
            desc->flags  = JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE;
            desc->value  = el;
            desc->getter = JS_UNDEFINED;
            desc->setter = JS_UNDEFINED;
        } else {
            JS_FreeValue(ctx, el);
        }
        return 1;
    }
    JSValue elements = ns_form_controls_snapshot(ctx, obj);
    JSValue result = ns_form_elements_named_lookup(ctx, elements, name);
    JS_FreeValue(ctx, elements);
    if (!JS_IsNull(result) && !JS_IsUndefined(result)) {
        JSValue past_map = JS_GetPropertyStr(ctx, obj, "_ns_past_names");
        if (!JS_IsObject(past_map)) {
            past_map = JS_NewObject(ctx);
            JS_DefinePropertyValueStr(ctx, obj, "_ns_past_names", JS_DupValue(ctx, past_map), 0);
        }
        const ns_node *res_node = ns_unwrap_element(result);
        if (res_node) {
            JS_DefinePropertyValueStr(ctx, past_map, name, JS_DupValue(ctx, result), JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
        }
        JS_FreeValue(ctx, past_map);
    } else {
        JS_FreeValue(ctx, result);
        result = JS_NULL;
        JSValue past_map = JS_GetPropertyStr(ctx, obj, "_ns_past_names");
        if (JS_IsObject(past_map)) {
            JSValue candidate = JS_GetPropertyStr(ctx, past_map, name);
            if (JS_IsObject(candidate)) {
                const ns_node *cand_node = ns_unwrap_element(candidate);
                if (cand_node && ns_form_owner(cand_node, NULL) == n) {
                    result = candidate;
                } else {
                    JS_FreeValue(ctx, candidate);
                    JS_SetPropertyStr(ctx, past_map, name, JS_UNDEFINED);
                }
            } else {
                JS_FreeValue(ctx, candidate);
            }
        }
        JS_FreeValue(ctx, past_map);
    }
    JS_FreeCString(ctx, name);
    if (JS_IsNull(result) || JS_IsUndefined(result)) {
        JS_FreeValue(ctx, result);
        return 0;
    }
    if (desc) {
        desc->flags  = JS_PROP_CONFIGURABLE;
        desc->value  = result;
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    } else {
        JS_FreeValue(ctx, result);
    }
    return 1;
}

static int
ns_element_delete_property(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    const ns_node *n = ns_unwrap_element(obj);
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name || strcmp(n->name, "form") != 0)
        return 1;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 1;
    if (!*name || strncmp(name, "_ns_", 4) == 0 || strncmp(name, "__", 2) == 0) {
        JS_FreeCString(ctx, name);
        return 1;
    }
    JS_FreeCString(ctx, name);
    JSPropertyDescriptor desc;
    int has = ns_element_named_get_own(ctx, &desc, obj, prop);
    if (has > 0) {
        JS_FreeValue(ctx, desc.value);
        return 0;
    }
    return 1;
}

static int
ns_element_define_own_property(JSContext *ctx, JSValueConst this_obj, JSAtom prop,
                               JSValueConst val, JSValueConst getter,
                               JSValueConst setter, int flags)
{
    const ns_node *n = ns_unwrap_element(this_obj);
    if (n && n->kind == NS_NODE_ELEMENT && n->name && strcmp(n->name, "form") == 0) {
        const char *name = JS_AtomToCString(ctx, prop);
        if (name) {
            if (*name && strncmp(name, "_ns_", 4) != 0 && strncmp(name, "__", 2) != 0) {
                JSPropertyDescriptor desc;
                int has = ns_element_named_get_own(ctx, &desc, this_obj, prop);
                if (has > 0) {
                    JS_FreeValue(ctx, desc.value);
                    JS_FreeCString(ctx, name);
                    JS_ThrowTypeError(ctx, "Cannot define property on form");
                    return -1;
                }
            }
            JS_FreeCString(ctx, name);
        }
    }
    return JS_DefineProperty(ctx, this_obj, prop, val, getter, setter,
                             flags | JS_PROP_NO_EXOTIC);
}

typedef struct {
    JSValue element;
} ns_dataset_back;

static void
ns_dataset_finalizer(JSRuntime *rt, JSValue val)
{
    ns_dataset_back *b = JS_GetOpaque(val, ns_dataset_class_id);
    if (!b) return;
    JS_FreeValueRT(rt, b->element);
    g_free(b);
}

static void
ns_dataset_gc_mark(JSRuntime *rt, JSValueConst val, JS_MarkFunc *mark_func)
{
    ns_dataset_back *b = JS_GetOpaque(val, ns_dataset_class_id);
    if (b) JS_MarkValue(rt, b->element, mark_func);
}

ns_node *
ns_dataset_node(JSValueConst obj)
{
    ns_dataset_back *b = JS_GetOpaque(obj, ns_dataset_class_id);
    return b ? ns_unwrap_element_mut(b->element) : NULL;
}

static int
ns_dataset_get_own(JSContext *ctx, JSPropertyDescriptor *desc,
                   JSValueConst obj, JSAtom prop)
{
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return 0;
    char *value = ns_dataset_named_value(obj, name);
    JS_FreeCString(ctx, name);
    if (!value) return 0;
    if (desc) {
        desc->flags  = JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE |
                       JS_PROP_WRITABLE;
        desc->value  = JS_NewString(ctx, value);
        desc->getter = JS_UNDEFINED;
        desc->setter = JS_UNDEFINED;
    }
    g_free(value);
    return 1;
}

static int
ns_dataset_get_own_names(JSContext *ctx, JSPropertyEnum **ptab,
                         uint32_t *plen, JSValueConst obj)
{
    *ptab = NULL;
    *plen = 0;
    char **names = ns_dataset_names(obj);
    guint count = names ? g_strv_length(names) : 0;
    if (count == 0) {
        g_strfreev(names);
        return 0;
    }
    JSPropertyEnum *tab = js_malloc(ctx, sizeof(JSPropertyEnum) * count);
    if (!tab) {
        g_strfreev(names);
        return -1;
    }
    for (guint i = 0; i < count; i++) {
        tab[i].atom = JS_NewAtom(ctx, names[i]);
        tab[i].is_enumerable = 1;
    }
    g_strfreev(names);
    *ptab = tab;
    *plen = count;
    return 0;
}

static int
ns_dataset_set_property(JSContext *ctx, JSValueConst obj, JSAtom prop,
                        JSValueConst val, JSValueConst receiver, int flags)
{
    (void)receiver; (void)flags;
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return FALSE;
    int ret = ns_dataset_named_set(ctx, obj, name, val);
    JS_FreeCString(ctx, name);
    return ret;
}

static int
ns_dataset_delete_property(JSContext *ctx, JSValueConst obj, JSAtom prop)
{
    const char *name = JS_AtomToCString(ctx, prop);
    if (!name) return TRUE;
    ns_dataset_named_delete(ctx, obj, name);
    JS_FreeCString(ctx, name);
    return TRUE;
}

static JSClassExoticMethods ns_dataset_exotic = {
    .get_own_property       = ns_dataset_get_own,
    .get_own_property_names = ns_dataset_get_own_names,
    .set_property           = ns_dataset_set_property,
    .delete_property        = ns_dataset_delete_property,
};

static JSClassDef ns_dataset_class = {
    .class_name = "DOMStringMap",
    .finalizer  = ns_dataset_finalizer,
    .gc_mark    = ns_dataset_gc_mark,
    .exotic     = &ns_dataset_exotic,
};

static JSValue
ns_element_get_dataset(JSContext *ctx, JSValueConst this_val)
{
    const ns_node *el = ns_unwrap_element(this_val);
    if (!ns_element_has_dataset(el)) return JS_UNDEFINED;
    JSValue ds = JS_NewObjectClass(ctx, ns_dataset_class_id);
    if (JS_IsException(ds)) return ds;
    ns_dataset_back *b = g_new0(ns_dataset_back, 1);
    b->element = JS_DupValue(ctx, this_val);
    JS_SetOpaque(ds, b);
    return ds;
}

void
ns_js_fire_window_focus_event(ns_js *js, ns_node *doc, const char *type)
{
    if (!doc || js->halted || js->in_pump) return;
    JSContext *realm = doc->parent ? ns_js_node_realm_context(js, doc)
                     : js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    if (!realm) return;
    ns_realm_scope scope;
    scope.active = FALSE;
    if (doc->parent) {
        if (realm != js->ctx)
            ns_js_frame_scope_enter(js, realm, doc->parent, &scope);
    } else {
        ns_js_realm_scope_enter(js, realm, &scope);
    }
    JSValue ev = ns_make_window_event(js->ctx, type);
    JS_SetPropertyStr(js->ctx, ev, "target",
                      ns_js_event_window_for_document(js, doc));
    JS_SetPropertyStr(js->ctx, ev, "relatedTarget", JS_NULL);
    ns_js_dispatch_window_only_event(js, doc, type, ev, NULL);
    ns_js_realm_scope_leave(js, &scope);
}

void
ns_js_note_user_edit(ns_js *js, const ns_node *el, const char *value_before)
{
    if (!js || !el || js->change_pending == el) return;
    if (!ns_node_is_element_named(el, "input") &&
        !ns_node_is_element_named(el, "textarea"))
        return;
    ns_js_forget_pending_change(js);
    ns_node_arm_js_invalidate((ns_node *)el);
    js->change_pending = el;
    js->change_baseline = g_strdup(value_before ? value_before : "");
}

void
ns_js_commit_change(ns_js *js, const ns_node *el)
{
    if (!js || !el || js->change_pending != el) return;
    gboolean changed =
        g_strcmp0(ns_node_editable_value(el), js->change_baseline) != 0;
    ns_js_forget_pending_change(js);
    if (changed) ns_js_dispatch_event(js, el, "change", NULL);
}

static JSValue
ns_element_checkVisibility(JSContext *ctx, JSValueConst this_val,
                           int argc, JSValueConst *argv)
{
    const ns_node *el = ns_unwrap_element(this_val);
    ns_js *js = js_from_ctx(ctx);
    if (!el || !js) return JS_FALSE;
    ns_js_flush_layout(js);
    gboolean visible = js->layout_root &&
                       ns_box_find_by_dom(js->layout_root, el) != NULL;
    if (visible && argc >= 1 && JS_IsObject(argv[0])) {
        if (ns_js_get_bool_prop(ctx, argv[0], "checkVisibilityCSS", NULL) ||
            ns_js_get_bool_prop(ctx, argv[0], "visibilityProperty", NULL)) {
            char *v = ns_js_computed_text(ctx, el, "visibility");
            if (v && (strcmp(v, "hidden") == 0 || strcmp(v, "collapse") == 0))
                visible = FALSE;
            g_free(v);
        }
        if (visible &&
            (ns_js_get_bool_prop(ctx, argv[0], "checkOpacity", NULL) ||
             ns_js_get_bool_prop(ctx, argv[0], "opacityProperty", NULL))) {
            char *o = ns_js_computed_text(ctx, el, "opacity");
            if (o && g_ascii_strtod(o, NULL) == 0.0) visible = FALSE;
            g_free(o);
        }
    }
    return visible ? JS_TRUE : JS_FALSE;
}

static int
ns_pointer_capture_index(JSContext *ctx, JSValueConst set, int32_t id)
{
    if (!JS_IsArray(set)) return -1;
    uint32_t len = ns_js_array_length(ctx, set);
    for (uint32_t i = 0; i < len; i++) {
        JSValue v = JS_GetPropertyUint32(ctx, set, i);
        int32_t e = 0;
        JS_ToInt32(ctx, &e, v);
        JS_FreeValue(ctx, v);
        if (e == id) return (int)i;
    }
    return -1;
}

static JSValue
ns_element_setPointerCapture(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_UNDEFINED;
    int32_t id = 0;
    JS_ToInt32(ctx, &id, argv[0]);
    JSValue set = JS_GetPropertyStr(ctx, this_val, "_pointerCaptures");
    if (!JS_IsArray(set)) {
        JS_FreeValue(ctx, set);
        set = JS_NewArray(ctx);
        JS_SetPropertyStr(ctx, this_val, "_pointerCaptures",
                          JS_DupValue(ctx, set));
    }
    if (ns_pointer_capture_index(ctx, set, id) < 0)
        JS_SetPropertyUint32(ctx, set, ns_js_array_length(ctx, set),
                             JS_NewInt32(ctx, id));
    JS_FreeValue(ctx, set);
    return JS_UNDEFINED;
}

static JSValue
ns_element_hasPointerCapture(JSContext *ctx, JSValueConst this_val,
                             int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_FALSE;
    int32_t id = 0;
    JS_ToInt32(ctx, &id, argv[0]);
    JSValue set = JS_GetPropertyStr(ctx, this_val, "_pointerCaptures");
    gboolean found = ns_pointer_capture_index(ctx, set, id) >= 0;
    JS_FreeValue(ctx, set);
    return found ? JS_TRUE : JS_FALSE;
}

static JSValue
ns_element_releasePointerCapture(JSContext *ctx, JSValueConst this_val,
                                 int argc, JSValueConst *argv)
{
    if (argc < 1) return JS_UNDEFINED;
    int32_t id = 0;
    JS_ToInt32(ctx, &id, argv[0]);
    JSValue set = JS_GetPropertyStr(ctx, this_val, "_pointerCaptures");
    int idx = ns_pointer_capture_index(ctx, set, id);
    if (idx >= 0) {
        JSValue splice = JS_GetPropertyStr(ctx, set, "splice");
        JSValueConst sargs[2] = { JS_NewInt32(ctx, idx), JS_NewInt32(ctx, 1) };
        JSValue r = JS_Call(ctx, splice, set, 2, sargs);
        JS_FreeValue(ctx, r);
        JS_FreeValue(ctx, sargs[0]);
        JS_FreeValue(ctx, sargs[1]);
        JS_FreeValue(ctx, splice);
    }
    JS_FreeValue(ctx, set);
    return JS_UNDEFINED;
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

static const ns_node *
ns_js_form_owner_for(const ns_node *el, ns_js *js)
{
    const ns_node *doc = js && js->current_doc ? js->current_doc
                                                : ns_node_root(el);
    return ns_form_owner(el, doc);
}

static const ns_node *
ns_node_owner_iframe(const ns_node *n)
{
    for (const ns_node *p = n ? n->parent : NULL; p; p = p->parent)
        if (ns_node_is_element_named(p, "iframe")) return p;
    return NULL;
}

static gboolean
ns_iframe_follow_href(JSContext *ctx, const ns_node *el, const char *href)
{
    const ns_node *iframe = ns_node_owner_iframe(el);
    if (!iframe || !iframe->js_wrapper || !href || !*href) return FALSE;
    JSValue iw = JS_MKPTR(JS_TAG_OBJECT, iframe->js_wrapper);
    JSValue win = JS_GetPropertyStr(ctx, iw, "__ndRealmWindow");
    gboolean followed = FALSE;
    if (JS_IsObject(win)) {
        JSValue loc = JS_GetPropertyStr(ctx, win, "location");
        if (JS_IsObject(loc)) {
            JS_SetPropertyStr(ctx, loc, "href", JS_NewString(ctx, href));
            followed = TRUE;
        }
        JS_FreeValue(ctx, loc);
    }
    JS_FreeValue(ctx, win);
    return followed;
}

static gboolean
ns_node_is_interactive_content(const ns_node *el)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !el->name) return FALSE;
    if (ns_node_is_element_named(el, "a") ||
        ns_node_is_element_named(el, "area"))
        return ns_element_get_attr(el, "href") != NULL;
    if (ns_node_is_element_named(el, "input")) {
        const char *t = ns_element_get_attr(el, "type");
        return !t || g_ascii_strcasecmp(t, "hidden") != 0;
    }
    if (ns_node_is_element_named(el, "audio") ||
        ns_node_is_element_named(el, "video"))
        return ns_element_get_attr(el, "controls") != NULL;
    if (ns_node_is_element_named(el, "img") ||
        ns_node_is_element_named(el, "object"))
        return ns_element_get_attr(el, "usemap") != NULL;
    return ns_node_is_element_named(el, "button") ||
           ns_node_is_element_named(el, "details") ||
           ns_node_is_element_named(el, "embed") ||
           ns_node_is_element_named(el, "iframe") ||
           ns_node_is_element_named(el, "label") ||
           ns_node_is_element_named(el, "select") ||
           ns_node_is_element_named(el, "textarea");
}

gboolean
ns_node_has_activation_behavior(const ns_node *cur)
{
    if (!cur || cur->kind != NS_NODE_ELEMENT || !cur->name) return FALSE;
    if (ns_node_is_element_named(cur, "a") ||
        ns_node_is_element_named(cur, "area"))
        return ns_element_get_attr(cur, "href") != NULL;
    if (ns_node_is_element_named(cur, "input")) {
        const char *t = ns_element_get_attr(cur, "type");
        return !t || g_ascii_strcasecmp(t, "hidden") != 0;
    }
    if (ns_node_is_element_named(cur, "button")) return TRUE;
    if (ns_node_is_element_named(cur, "label")) return TRUE;
    return ns_node_is_element_named(cur, "summary") &&
           ns_summary_toggle_target(cur) != NULL;
}

const ns_node *
ns_click_activation_target(const ns_node *el)
{
    for (const ns_node *cur = el; cur; cur = cur->parent)
        if (ns_node_has_activation_behavior(cur)) return cur;
    return NULL;
}

static void
ns_js_activate_label(ns_js *js, const ns_node *label, const ns_node *target)
{
    for (const ns_node *cur = target; cur && cur != label; cur = cur->parent)
        if (ns_node_is_interactive_content(cur)) return;
    const ns_node *control = NULL;
    const char *forv = ns_element_get_attr(label, "for");
    if (forv && *forv && js->current_doc) {
        const ns_node *t = ns_node_find_by_id(js->current_doc, forv);
        if (ns_js_node_is_labelable(t)) control = t;
    }
    if (!control) control = ns_js_first_labelable_descendant(label, 0);
    if (!control || control == target) return;
    if (ns_node_is_disabled_form_control(control)) return;
    ns_js_activate_element(js, control);
}

gboolean
ns_js_anchor_fragment_navigate(ns_js *js, const char *abs_url)
{
    if (!abs_url || !js->current_url || !*js->current_url) return FALSE;
    const char *h = strchr(abs_url, '#');
    if (!h) return FALSE;
    size_t base_len = (size_t)(h - abs_url);
    const char *cur = js->current_url;
    const char *ch = strchr(cur, '#');
    size_t cur_len = ch ? (size_t)(ch - cur) : strlen(cur);
    if (cur_len != base_len || strncmp(cur, abs_url, base_len) != 0)
        return FALSE;
    if (strcmp(cur, abs_url) == 0) {
        if (js->fragment_nav_cb)
            js->fragment_nav_cb(abs_url, js->fragment_nav_user_data);
        return TRUE;
    }
    char *old_url = g_strdup(cur);
    g_free(js->current_url);
    js->current_url = g_strdup(abs_url);
    if (js->soft_nav_cb)
        js->soft_nav_cb(js->current_url, FALSE, js->soft_nav_user_data);
    if (js->fragment_nav_cb)
        js->fragment_nav_cb(js->current_url, js->fragment_nav_user_data);
    char *new_url = g_strdup(js->current_url);
    ns_js_dispatch_hashchange(js, old_url, new_url);
    g_free(new_url);
    g_free(old_url);
    return TRUE;
}

JSValue
ns_element_activation_behavior(JSContext *ctx, const ns_node *act,
                               const ns_node *target)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js) return JS_UNDEFINED;
    if (ns_node_is_element_named(act, "summary")) {
        ns_js_activate_summary(js, act);
        return JS_UNDEFINED;
    }
    if (ns_node_is_element_named(act, "label")) {
        ns_js_activate_label(js, act, target);
        return JS_UNDEFINED;
    }
    if (ns_node_is_element_named(act, "a") ||
        ns_node_is_element_named(act, "area")) {
        g_autofree char *href = g_strdup(ns_element_get_attr(act, "href"));
        if (href && g_str_has_prefix(href, "javascript:")) {
            char *code = g_uri_unescape_string(href + strlen("javascript:"),
                                               NULL);
            char *r = ns_js_eval_source(js, code ? code
                                        : href + strlen("javascript:"),
                                        "javascript-url");
            g_free(r);
            g_free(code);
            return JS_UNDEFINED;
        }
        if (ns_iframe_follow_href(ctx, act, href))
            return JS_UNDEFINED;
        if (href && *href && ns_element_get_attr(act, "download") && js->download_cb) {
            g_autofree char *abs_url = ns_element_anchor_resolved_href(act, js);
            const char *dl = ns_element_get_attr(act, "download");
            js->download_cb(abs_url ? abs_url : href, dl, js->download_user_data);
            return JS_UNDEFINED;
        }
        if (href && *href) {
            g_autofree char *abs_url = ns_element_anchor_resolved_href(act, js);
            if (abs_url && ns_js_anchor_fragment_navigate(js, abs_url))
                return JS_UNDEFINED;
            if (js->nav_cb)
                js->nav_cb(href, FALSE, js->nav_user_data);
        }
        return JS_UNDEFINED;
    }
    if (ns_node_is_disabled_form_control(act))
        return JS_UNDEFINED;
    if (ns_node_is_element_named(act, "button")) {
        ns_button_activation(js, (ns_node *)act, target);
        return JS_UNDEFINED;
    }
    if (ns_node_is_submit_trigger(act)) {
        const ns_node *form = ns_js_form_owner_for(act, js);
        if (form) {
            JSValue r = ns_js_request_submit_form(ctx, form, act);
            if (JS_IsException(r)) return r;
            JS_FreeValue(ctx, r);
        }
    } else if (ns_node_is_reset_trigger(act)) {
        ns_node *form = (ns_node *)ns_js_form_owner_for(act, js);
        if (form) {
            JSValue r = ns_js_reset_form(ctx, form);
            if (JS_IsException(r)) return r;
            JS_FreeValue(ctx, r);
        }
    }
    ns_popover_target_activation(js, (ns_node *)act, target);
    return JS_UNDEFINED;
}

void
ns_js_request_repaint(ns_js *js)
{
    if (js && js->repaint_cb) js->repaint_cb(js->repaint_user_data);
}

void
ns_js_notify_scroll_to(ns_js *js, const ns_node *target)
{
    if (js && js->scroll_to_cb) js->scroll_to_cb(target, js->scroll_to_user_data);
}

typedef struct ns_js_image_load {
    ns_js     *js;
    JSContext *realm;
    ns_node   *el;
    ns_image  *img;
    char      *requested_url;
    double     start_ms;
    guint      ready_idle;
} ns_js_image_load;

static gboolean
ns_js_image_loads_pending(const ns_js *js)
{
    if (!js || !js->js_image_loads) return FALSE;
    GHashTableIter it;
    gpointer key, value;
    g_hash_table_iter_init(&it, js->js_image_loads);
    while (g_hash_table_iter_next(&it, &key, &value)) {
        const ns_js_image_load *r = value;
        if (r && r->el && !(r->el->flags & NS_NODE_IMG_LOAD_FIRED))
            return TRUE;
    }
    return FALSE;
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

JSContext *
ns_js_realm_for_node(ns_js *js, const ns_node *node)
{
    return ns_js_node_realm_context(js, node);
}

/* The frame element whose content document holds node, or NULL for the
 * page's own document (fallback content inside an <object> included). */
static ns_node *
ns_js_node_content_frame(const ns_node *node)
{
    const ns_node *p = node;
    while (p && !(p->kind == NS_NODE_DOCUMENT && !(p->flags & NS_NODE_FRAGMENT)))
        p = p->parent;
    ns_node *frame = p ? p->parent : NULL;
    return frame && (ns_node_is_element_named(frame, "iframe") ||
                     ns_node_is_element_named(frame, "frame") ||
                     ns_node_is_element_named(frame, "object"))
        ? frame : NULL;
}

static gboolean
ns_js_attr_has_token(const ns_node *el, const char *attr, const char *token)
{
    const char *v = ns_element_get_attr(el, attr);
    if (!v) return FALSE;
    gchar **parts = g_strsplit_set(v, " \t\n\f\r", -1);
    gboolean found = FALSE;
    for (int i = 0; parts[i] && !found; i++)
        found = g_ascii_strcasecmp(parts[i], token) == 0;
    g_strfreev(parts);
    return found;
}

/* Whether a script or stylesheet link blocks rendering, as
 * PerformanceResourceTiming.renderBlockingStatus reports it: the element
 * is in its document's <head> and has blocking="render", or is a
 * parser-inserted classic script without async or defer, or a stylesheet
 * link the parser created for media that applies to the screen. */
static gboolean
ns_js_element_render_blocking(const ns_node *el)
{
    if (!el || el->kind != NS_NODE_ELEMENT) return FALSE;
    gboolean in_head = FALSE;
    for (const ns_node *p = el->parent; p && p->kind == NS_NODE_ELEMENT;
         p = p->parent)
        if (ns_node_is_element_named(p, "head")) { in_head = TRUE; break; }
    if (!in_head) return FALSE;
    if (ns_js_attr_has_token(el, "blocking", "render")) return TRUE;
    if (el->flags & NS_NODE_NOT_PARSER_INSERTED) return FALSE;
    if (ns_node_is_element_named(el, "script"))
        return !ns_script_type_is_module(el) &&
               !ns_element_get_attr(el, "async") &&
               !ns_element_get_attr(el, "defer");
    if (ns_node_is_element_named(el, "link")) {
        const char *media = ns_element_get_attr(el, "media");
        return ns_js_attr_has_token(el, "rel", "stylesheet") &&
               !ns_js_attr_has_token(el, "rel", "alternate") &&
               (!media || !*media || g_ascii_strcasecmp(media, "all") == 0 ||
                g_ascii_strcasecmp(media, "screen") == 0);
    }
    return FALSE;
}

/* The resource timing details that come from the element a resource is
 * loaded for: the timeline of its document (the frame's realm, the frame
 * element for a frame without one yet, NULL for the page's own), that
 * document's URL, its render-blocking status and its CORS mode. */
void
ns_js_element_perf_info(ns_js *js, const ns_node *el,
                        ns_perf_resource_info *info)
{
    ns_node *frame = ns_js_node_content_frame(el);
    JSContext *fctx = frame && js ? ns_js_frame_context(js, frame) : NULL;
    info->timeline = fctx ? (gconstpointer)fctx : (gconstpointer)frame;
    const char *url = frame && js ? ns_js_frame_url(js, frame) : NULL;
    info->document_url = url ? url : js ? js->current_url : NULL;
    info->render_blocking = ns_js_element_render_blocking(el);
    info->cors_mode = el && ns_element_get_attr(el, "crossorigin") != NULL;
}

char *
ns_js_node_document_base_url(ns_js *js, const ns_node *node)
{
    const ns_node *doc = NULL;
    for (const ns_node *p = node; p; p = p->parent) {
        if (p->kind == NS_NODE_DOCUMENT) {
            doc = p;
            break;
        }
    }
    const char *fallback = ns_js_node_doc_base(js, node);
    return ns_js_document_base_url(js, doc, fallback);
}

static void
ns_js_rescan_subtree_images(ns_js *js, ns_node *root, int depth)
{
    if (!js || !root || depth >= 512) return;
    if (root->kind == NS_NODE_ELEMENT && root->name &&
        strcmp(root->name, "img") == 0) {
        const char *src = ns_element_get_attr(root, "src");
        ns_js_image_load *r = (src && *src && js->js_image_loads)
            ? g_hash_table_lookup(js->js_image_loads, root) : NULL;
        if (r && r->requested_url) {
            const char *base = ns_js_node_doc_base(js, root);
            char *abs = base ? ns_url_resolve(base, src) : NULL;
            if (abs && strcmp(abs, r->requested_url) != 0) {
                root->flags &= ~NS_NODE_IMG_LOAD_FIRED;
                ns_js_start_image_load(js, root, src);
            }
            g_free(abs);
        }
    }
    for (ns_node *c = root->first_child; c; c = c->next_sibling)
        ns_js_rescan_subtree_images(js, c, depth + 1);
}

static void
ns_js_flush_ready_images(ns_js *js)
{
    if (!js || js->halted || js->in_pump || !js->js_image_loads) return;
    GPtrArray *ready = NULL;
    GHashTableIter it;
    gpointer key, val;
    g_hash_table_iter_init(&it, js->js_image_loads);
    while (g_hash_table_iter_next(&it, &key, &val)) {
        ns_js_image_load *r = val;
        ns_node *el = key;
        if (!r || !r->img) continue;
        if (el->flags & NS_NODE_IMG_LOAD_FIRED) continue;
        if (!r->img->loaded && !r->img->failed) continue;
        if (!ready) ready = g_ptr_array_new();
        g_ptr_array_add(ready, el);
    }
    if (!ready) return;
    for (guint i = 0; i < ready->len; i++) {
        ns_node *el = ready->pdata[i];
        ns_js_image_load *r = g_hash_table_lookup(js->js_image_loads, el);
        if (!r || !r->img) continue;
        ns_realm_scope scope;
        ns_js_realm_scope_enter(js, r->realm, &scope);
        ns_js_fire_img_load_once(js, el, r->img->failed);
        ns_js_realm_scope_leave(js, &scope);
    }
    g_ptr_array_free(ready, TRUE);
}

static void
ns_js_image_load_free(gpointer data)
{
    ns_js_image_load *r = data;
    if (!r) return;
    if (r->ready_idle) g_source_remove(r->ready_idle);
    if (r->js && r->js->image_cache)
        ns_image_cache_cancel_cb(r->js->image_cache, r);
    g_free(r->requested_url);
    g_free(r);
}

static gboolean
ns_js_image_ready_idle(gpointer data)
{
    ns_js_image_load *r = data;
    ns_js *js = r->js;
    if (js && !js->halted && (js->in_pump || ns_engine_in_blocking_fetch())) {
        r->ready_idle = g_timeout_add(4, ns_js_image_ready_idle, r);
        return G_SOURCE_REMOVE;
    }
    r->ready_idle = 0;
    if (!js || js->halted) return G_SOURCE_REMOVE;
    if (g_hash_table_lookup(js->js_image_loads, r->el) != r)
        return G_SOURCE_REMOVE;
    const char *cur_src = ns_element_get_attr(r->el, "src");
    const char *cur_base = ns_js_node_doc_base(js, r->el);
    char *abs_url = NULL;
    if (cur_src && *cur_src && cur_base)
        abs_url = ns_url_resolve(cur_base, cur_src);
    else if (cur_src)
        abs_url = g_strdup(cur_src);
    if (!abs_url || !r->requested_url || strcmp(abs_url, r->requested_url) != 0) {
        g_free(abs_url);
        if (cur_src && *cur_src)
            ns_js_start_image_load(js, r->el, cur_src);
        return G_SOURCE_REMOVE;
    }
    g_free(abs_url);
    ns_image *img = r->img;
    if (img && img->failed && js->log_cb && img->http_status != 204) {
        char *line = g_strdup_printf("[image] error: %s — %s (HTTP %ld)",
            img->url ? img->url : "(no url)",
            img->error ? img->error : "(no error msg)",
            img->http_status);
        js->log_cb(line, js->log_user_data);
        g_free(line);
    }
    if (img && (img->loaded || img->failed)) {
        ns_realm_scope scope;
        ns_js_realm_scope_enter(js, r->realm, &scope);
        ns_js_fire_img_load_once(js, r->el, img->failed);
        ns_js_realm_scope_leave(js, &scope);
        if (img->loaded) js->mutated = TRUE;
    }
    if (js->repaint_cb) js->repaint_cb(js->repaint_user_data);
    return G_SOURCE_REMOVE;
}

static void
ns_js_on_image_ready(ns_image *img, gpointer user_data)
{
    ns_js_image_load *r = user_data;
    if (!r || !r->js || !r->el) return;
    ns_js *js = r->js;
    if (js->halted) return;
    if (g_hash_table_lookup(js->js_image_loads, r->el) != r) return;
    r->img = img;
    if (!r->ready_idle)
        r->ready_idle = g_idle_add(ns_js_image_ready_idle, r);
}

static void
ns_js_start_image_load(ns_js *js, ns_node *el, const char *src)
{
    if (!js || !js->image_cache || !el) return;
    if (!el->name || strcmp(el->name, "img") != 0) return;
    if (!js->js_image_loads)
        js->js_image_loads = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                                   NULL, ns_js_image_load_free);
    if (!src || !*src) {
        g_hash_table_remove(js->js_image_loads, el);
        return;
    }
    const char *base = ns_js_node_doc_base(js, el);
    char *abs_url = base ? ns_url_resolve(base, src) : g_strdup(src);
    if (!abs_url) {
        g_hash_table_remove(js->js_image_loads, el);
        return;
    }
    ns_js_image_load *r = g_new0(ns_js_image_load, 1);
    r->realm = js->raf_frame_ctx
        ? ns_js_frame_context(js, js->raf_frame_ctx) : NULL;
    if (!r->realm) r->realm = js->ctx;
    r->js = js;
    r->el = el;
    r->requested_url = abs_url;
    r->start_ms = ns_perf_now_ms(js);
    g_hash_table_insert(js->js_image_loads, el, r);
    r->img = ns_image_cache_get(js->image_cache, abs_url,
                                base ? base : abs_url,
                                ns_js_on_image_ready, r);
    if (r->img && (r->img->loaded || r->img->failed) && !r->ready_idle)
        r->ready_idle = g_idle_add(ns_js_image_ready_idle, r);
}

const ns_image *
ns_js_image_for_node(ns_js *js, const ns_node *el)
{
    if (!js || !el) return NULL;
    if (js->js_image_loads) {
        ns_js_image_load *r = g_hash_table_lookup(js->js_image_loads, el);
        if (r && r->img) return r->img;
    }
    if (js->image_cache && el->name && strcmp(el->name, "img") == 0) {
        const char *src = ns_element_get_attr(el, "src");
        if (src && *src) {
            const char *base = ns_js_node_doc_base(js, el);
            char *abs_url = base ? ns_url_resolve(base, src) : g_strdup(src);
            if (abs_url) {
                ns_image *im = ns_image_cache_peek(js->image_cache, abs_url);
                g_free(abs_url);
                if (im) return im;
            }
        }
    }
    if (js->layout_root) {
        const ns_box *b = ns_box_find_by_dom(js->layout_root, el);
        if (b && b->media && b->media->image)
            return (const ns_image *)b->media->image;
    }
    return NULL;
}

static gboolean
ns_js_urls_share_http_authority(const char *a, const char *b)
{
    if (!ns_url_is_http_or_https(a)) return FALSE;
    const char *authority = strstr(a, "://") + 3;
    size_t authority_len = strcspn(authority, "/?#\\");
    if (authority_len == 0) return FALSE;
    size_t n = (size_t)(authority - a) + authority_len;
    return strncmp(a, b, n) == 0 &&
           (b[n] == '\0' || b[n] == '/' || b[n] == '?' || b[n] == '#');
}

static gboolean
ns_js_urls_same_origin(const char *a, const char *b)
{
    if (!a || !b) return FALSE;
    if (ns_js_urls_share_http_authority(a, b)) return TRUE;
    if (ns_url_is_http_or_https(a) || ns_url_is_http_or_https(b))
        return ns_url_same_origin(a, b);
    const char *ca = strchr(a, ':'), *cb = strchr(b, ':');
    return ca && cb && ca - a == cb - b &&
           g_ascii_strncasecmp(a, b, (gsize)(ca - a)) == 0;
}

static const char *
ns_js_caller_document_url(ns_js *js, JSContext *ctx)
{
    ns_node *frame = ns_js_frame_of_realm(js, JS_GetCallerRealm(ctx));
    if (frame) {
        const char *fu = ns_element_get_attr(frame, "data-nd-frame-url");
        if (fu && *fu) return fu;
    }
    return js->document_origin ? js->document_origin : js->current_url;
}

gboolean
ns_js_resource_origin_clean(ns_js *js, JSContext *ctx, const char *url,
                            const char *cors_allow_origin)
{
    if (!js || !url) return FALSE;
    if (g_str_has_prefix(url, "data:") || g_str_has_prefix(url, "blob:"))
        return TRUE;
    const char *doc_url = ns_js_caller_document_url(js, ctx);
    if (ns_js_urls_same_origin(url, doc_url)) return TRUE;
    return cors_allow_origin && ns_cors_allows(doc_url, url, cors_allow_origin);
}














double
ns_arg_d(JSContext *ctx, JSValueConst v)
{
    double d = 0; JS_ToFloat64(ctx, &d, v); return d;
}























































































static JSValue
ns_element_toDataURL(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    const ns_node *el = ns_unwrap_element(this_val);
    if (!el || !js_from_ctx(ctx)) return JS_NewString(ctx, "data:,");
    ns_canvas_state *st = ns_canvas_state_for(js_from_ctx(ctx), el);
    if (!st || !st->surf) return JS_NewString(ctx, "data:,");
    if (!st->origin_clean)
        return ns_throw_dom_exception(ctx, "SecurityError", 18,
            "Tainted canvases may not be exported.");
    GByteArray *buf = g_byte_array_new();
    cairo_status_t s = cairo_surface_write_to_png_stream(st->surf,
        ns_canvas_png_write, buf);
    if (s != CAIRO_STATUS_SUCCESS) {
        g_byte_array_free(buf, TRUE);
        return JS_NewString(ctx, "data:,");
    }
    gchar *b64 = g_base64_encode(buf->data, buf->len);
    g_byte_array_free(buf, TRUE);
    char *url = g_strconcat("data:image/png;base64,", b64, NULL);
    JSValue ret = JS_NewString(ctx, url);
    g_free(url);
    g_free(b64);
    return ret;
}

static JSValue
ns_element_toBlob(JSContext *ctx, JSValueConst this_val,
                  int argc, JSValueConst *argv)
{
    if (argc < 1 || !JS_IsFunction(ctx, argv[0])) return JS_UNDEFINED;
    const ns_node *el = ns_unwrap_element(this_val);
    ns_canvas_state *st = el && js_from_ctx(ctx)
        ? ns_canvas_state_for(js_from_ctx(ctx), el) : NULL;
    if (st && !st->origin_clean)
        return ns_throw_dom_exception(ctx, "SecurityError", 18,
            "Tainted canvases may not be exported.");
    JSValue cb = JS_DupValue(ctx, argv[0]);
    JSValue blob = JS_NULL;
    if (st && st->surf) {
        GByteArray *buf = g_byte_array_new();
        cairo_status_t s = cairo_surface_write_to_png_stream(st->surf,
            ns_canvas_png_write, buf);
        if (s == CAIRO_STATUS_SUCCESS) {
            JSValue ab = JS_NewArrayBufferCopy(ctx, buf->data, buf->len);
            JSValue global = JS_GetGlobalObject(ctx);
            JSValue u8c = JS_GetPropertyStr(ctx, global, "Uint8Array");
            JS_FreeValue(ctx, global);
            JSValueConst u8args[1] = { ab };
            JSValue u8a = JS_CallConstructor(ctx, u8c, 1, u8args);
            JS_FreeValue(ctx, u8c);
            JS_FreeValue(ctx, ab);
            blob = JS_NewObject(ctx);
            JS_SetPropertyStr(ctx, blob, "__ndBlobBytes", u8a);
            JS_SetPropertyStr(ctx, blob, "size", JS_NewInt64(ctx, buf->len));
            JS_SetPropertyStr(ctx, blob, "type",
                              JS_NewString(ctx, "image/png"));
        }
        g_byte_array_free(buf, TRUE);
    }
    JSValueConst cb_args[1] = { blob };
    JSValue r = JS_Call(ctx, cb, JS_UNDEFINED, 1, cb_args);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
    JS_FreeValue(ctx, blob);
    JS_FreeValue(ctx, cb);
    return JS_UNDEFINED;
}


static JSValue
ns_element_show_picker(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)argc;
    (void)argv;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n || (!ns_node_is_element_named(n, "input") &&
               !ns_node_is_element_named(n, "select")))
        return JS_ThrowTypeError(ctx, "Illegal invocation");
    gboolean readonly_applies =
        ns_node_is_element_named(n, "input") &&
        ns_element_get_attr(n, "readonly") != NULL &&
        ns_input_type_supports_readonly(ns_element_get_attr(n, "type"));
    if (ns_element_effectively_disabled(n) || readonly_applies)
        return ns_throw_dom_exception(ctx, "InvalidStateError", 11,
            "showPicker() cannot be used on immutable controls");
    ns_js *js = js_from_ctx(ctx);
    if (!ns_js_has_transient_activation(js))
        return ns_throw_dom_exception(ctx, "NotAllowedError", 0,
            "showPicker() requires a user gesture");
    ns_js_consume_user_activation(js);
    return JS_UNDEFINED;
}

static ns_node *
ns_first_descendant_named_rec(ns_node *root, const char *name, int depth)
{
    if (!root || depth >= 512) return NULL;
    for (ns_node *c = root->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            g_ascii_strcasecmp(c->name, name) == 0)
            return c;
        ns_node *d = ns_first_descendant_named_rec(c, name, depth + 1);
        if (d) return d;
    }
    return NULL;
}

static ns_node *
ns_first_descendant_named(ns_node *root, const char *name)
{
    return ns_first_descendant_named_rec(root, name, 0);
}

static void
ns_collect_descendants_named_rec(ns_node *root, const char *name,
                                 GPtrArray *out, int depth)
{
    if (!root || depth >= 512) return;
    for (ns_node *c = root->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            g_ascii_strcasecmp(c->name, name) == 0)
            g_ptr_array_add(out, c);
        ns_collect_descendants_named_rec(c, name, out, depth + 1);
    }
}

static void
ns_collect_descendants_named(ns_node *root, const char *name, GPtrArray *out)
{
    ns_collect_descendants_named_rec(root, name, out, 0);
}

static JSValue
ns_table_create_section(JSContext *ctx, JSValueConst this_val, const char *name,
                        gboolean before_body)
{
    ns_node *tbl = ns_unwrap_element_mut(this_val);
    if (!tbl) return JS_NULL;
    ns_node *existing = ns_first_descendant_named(tbl, name);
    if (existing) return ns_make_element(ctx, existing);
    ns_node *sec = ns_node_new_element(g_strdup(name));
    ns_js *_j = js_from_ctx(ctx);
    if (before_body) {
        ns_node *body = ns_first_descendant_named(tbl, "tbody");
        if (body && body->parent == tbl)
            ns_element_insert_before_single(_j, tbl, sec, body);
        else
            ns_node_append_child(tbl, sec);
    } else {
        ns_node_append_child(tbl, sec);
    }
    if (_j) { _j->mutated = TRUE; ns_qcache_invalidate(_j); }
    return ns_make_element(ctx, sec);
}

static JSValue
ns_table_createTHead(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{ (void)argc; (void)argv; return ns_table_create_section(ctx, this_val, "thead", TRUE); }

static JSValue
ns_table_createTBody(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{ (void)argc; (void)argv; return ns_table_create_section(ctx, this_val, "tbody", FALSE); }

static JSValue
ns_table_createTFoot(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{ (void)argc; (void)argv; return ns_table_create_section(ctx, this_val, "tfoot", FALSE); }

static JSValue
ns_table_createCaption(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    ns_node *tbl = ns_unwrap_element_mut(this_val);
    if (!tbl) return JS_NULL;
    ns_node *existing = ns_first_descendant_named(tbl, "caption");
    if (existing) return ns_make_element(ctx, existing);
    ns_node *cap = ns_node_new_element(g_strdup("caption"));
    ns_js *_j = js_from_ctx(ctx);
    if (tbl->first_child)
        ns_element_insert_before_single(_j, tbl, cap, tbl->first_child);
    else
        ns_node_append_child(tbl, cap);
    if (_j) { _j->mutated = TRUE; ns_qcache_invalidate(_j); }
    return ns_make_element(ctx, cap);
}

static void
ns_table_delete_section(JSContext *ctx, JSValueConst this_val, const char *name)
{
    ns_node *tbl = ns_unwrap_element_mut(this_val);
    if (!tbl) return;
    ns_node *existing = ns_first_descendant_named(tbl, name);
    if (!existing) return;
    ns_js *_j = js_from_ctx(ctx);
    ns_node_remove(existing);
    ns_js_orphan_node(_j, existing);
    if (_j) { _j->mutated = TRUE; ns_qcache_invalidate(_j); }
}

static JSValue
ns_table_deleteTHead(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{ (void)argc; (void)argv; ns_table_delete_section(ctx, this_val, "thead"); return JS_UNDEFINED; }

static JSValue
ns_table_deleteTFoot(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{ (void)argc; (void)argv; ns_table_delete_section(ctx, this_val, "tfoot"); return JS_UNDEFINED; }

static JSValue
ns_table_deleteCaption(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{ (void)argc; (void)argv; ns_table_delete_section(ctx, this_val, "caption"); return JS_UNDEFINED; }

static JSValue
ns_table_insertRow(JSContext *ctx, JSValueConst this_val,
                   int argc, JSValueConst *argv)
{
    int32_t idx = -1;
    if (argc >= 1 && JS_ToInt32(ctx, &idx, argv[0])) return JS_EXCEPTION;
    ns_node *tbl = ns_unwrap_element_mut(this_val);
    if (!tbl) return JS_NULL;
    GPtrArray *rows = g_ptr_array_new();
    ns_collect_descendants_named(tbl, "tr", rows);
    if (idx > (int32_t)rows->len) idx = (int32_t)rows->len;
    ns_node *new_tr = ns_node_new_element(g_strdup("tr"));
    ns_js *_j = js_from_ctx(ctx);
    if (idx < 0 || idx == (int32_t)rows->len) {
        ns_node *parent;
        if (rows->len > 0) {
            parent = ((ns_node *)g_ptr_array_index(rows, rows->len - 1))->parent;
        } else if (ns_node_is_element_named(tbl, "table")) {
            parent = NULL;
            for (ns_node *c = tbl->first_child; c; c = c->next_sibling)
                if (ns_node_is_element_named(c, "tbody")) parent = c;
            if (!parent) {
                parent = ns_node_new_element(g_strdup("tbody"));
                ns_node_append_child(tbl, parent);
            }
        } else {
            parent = tbl;
        }
        if (!parent) parent = tbl;
        ns_node_append_child(parent, new_tr);
    } else {
        ns_node *ref = g_ptr_array_index(rows, idx);
        if (ref && ref->parent)
            ns_element_insert_before_single(_j, ref->parent, new_tr, ref);
        else
            ns_node_append_child(tbl, new_tr);
    }
    g_ptr_array_free(rows, TRUE);
    if (_j) {
        ns_js_record_child_change(_j, new_tr->parent, new_tr, NULL,
                                  new_tr->prev_sibling, new_tr->next_sibling);
        _j->mutated = TRUE;
    }
    return ns_make_element(ctx, new_tr);
}

static JSValue
ns_table_deleteRow(JSContext *ctx, JSValueConst this_val,
                   int argc, JSValueConst *argv)
{
    ns_node *tbl = ns_unwrap_element_mut(this_val);
    if (!tbl || argc < 1) return JS_UNDEFINED;
    int32_t idx = -1;
    JS_ToInt32(ctx, &idx, argv[0]);
    GPtrArray *rows = g_ptr_array_new();
    ns_collect_descendants_named(tbl, "tr", rows);
    if (idx < 0) idx = (int32_t)rows->len - 1;
    if (idx >= 0 && (uint32_t)idx < rows->len) {
        ns_node *r = g_ptr_array_index(rows, idx);
        ns_js *_j = js_from_ctx(ctx);
        ns_node *parent = r->parent;
        ns_node *prev = r->prev_sibling, *next = r->next_sibling;
        ns_node_remove(r);
        ns_js_orphan_node(_j, r);
        if (_j) {
            ns_js_record_child_change(_j, parent, NULL, r, prev, next);
            _j->mutated = TRUE;
        }
    }
    g_ptr_array_free(rows, TRUE);
    return JS_UNDEFINED;
}

static JSValue
ns_tr_insertCell(JSContext *ctx, JSValueConst this_val,
                 int argc, JSValueConst *argv)
{
    int32_t idx = -1;
    if (argc >= 1 && JS_ToInt32(ctx, &idx, argv[0])) return JS_EXCEPTION;
    ns_node *tr = ns_unwrap_element_mut(this_val);
    if (!tr) return JS_NULL;
    GPtrArray *cells = g_ptr_array_new();
    for (ns_node *c = tr->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            (g_ascii_strcasecmp(c->name, "td") == 0 ||
             g_ascii_strcasecmp(c->name, "th") == 0))
            g_ptr_array_add(cells, c);
    }
    if (idx > (int32_t)cells->len) idx = (int32_t)cells->len;
    ns_node *cell = ns_node_new_element(g_strdup("td"));
    ns_js *_j = js_from_ctx(ctx);
    if (idx < 0 || idx == (int32_t)cells->len) {
        ns_node_append_child(tr, cell);
    } else {
        ns_node *ref = g_ptr_array_index(cells, idx);
        ns_element_insert_before_single(_j, tr, cell, ref);
    }
    g_ptr_array_free(cells, TRUE);
    if (_j) {
        ns_js_record_child_change(_j, cell->parent, cell, NULL,
                                  cell->prev_sibling, cell->next_sibling);
        _j->mutated = TRUE;
    }
    return ns_make_element(ctx, cell);
}

static JSValue
ns_tr_deleteCell(JSContext *ctx, JSValueConst this_val,
                 int argc, JSValueConst *argv)
{
    ns_node *tr = ns_unwrap_element_mut(this_val);
    if (!tr || argc < 1) return JS_UNDEFINED;
    int32_t idx = -1;
    JS_ToInt32(ctx, &idx, argv[0]);
    GPtrArray *cells = g_ptr_array_new();
    for (ns_node *c = tr->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_ELEMENT && c->name &&
            (g_ascii_strcasecmp(c->name, "td") == 0 ||
             g_ascii_strcasecmp(c->name, "th") == 0))
            g_ptr_array_add(cells, c);
    }
    if (idx < 0) idx = (int32_t)cells->len - 1;
    if (idx >= 0 && (uint32_t)idx < cells->len) {
        ns_node *r = g_ptr_array_index(cells, idx);
        ns_js *_j = js_from_ctx(ctx);
        ns_node *parent = r->parent;
        ns_node *prev = r->prev_sibling, *next = r->next_sibling;
        ns_node_remove(r);
        ns_js_orphan_node(_j, r);
        if (_j) {
            ns_js_record_child_change(_j, parent, NULL, r, prev, next);
            _j->mutated = TRUE;
        }
    }
    g_ptr_array_free(cells, TRUE);
    return JS_UNDEFINED;
}

static void
ns_obj_adopt_global_proto(JSContext *ctx, JSValueConst obj, const char *iface)
{
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue ctor = JS_GetPropertyStr(ctx, global, iface);
    if (JS_IsObject(ctor)) {
        JSValue proto = JS_GetPropertyStr(ctx, ctor, "prototype");
        if (JS_IsObject(proto)) JS_SetPrototype(ctx, obj, proto);
        JS_FreeValue(ctx, proto);
    }
    JS_FreeValue(ctx, ctor);
    JS_FreeValue(ctx, global);
}

static JSValue
ns_media_request_video_frame_callback(JSContext *ctx, JSValueConst this_val,
                                       int argc, JSValueConst *argv)
{
    if (!js_from_ctx(ctx) || argc < 1 || !JS_IsFunction(ctx, argv[0]))
        return JS_NewInt32(ctx, 0);
    ns_js *js = js_from_ctx(ctx);
    if (!js->raf_pending)
        js->raf_pending = g_array_new(FALSE, FALSE, sizeof(ns_raf_entry));
    ns_raf_entry e = {
        .id = 0x40000000 + (++js->next_raf_id),
        .ctx = ctx,
        .cb = JS_DupValue(ctx, argv[0]),
        .video_frame = TRUE,
        .frame = ns_js_context_frame(js, ctx),
        .media = ns_unwrap_element_mut(this_val)
    };
    g_array_append_val(js->raf_pending, e);
    ns_raf_schedule_tick(js);
    return JS_NewInt32(ctx, e.id);
}

void
ns_js_window_action(ns_js *js, const char *action)
{
    if (js && js->window_action_cb)
        js->window_action_cb(action, js->window_action_user_data);
}

void
ns_js_scroll_viewport(ns_js *js, double x, double y)
{
    if (!js) return;
    if (!isfinite(x) || x < 0) x = 0;
    if (!isfinite(y) || y < 0) y = 0;
    if (js->viewport_scroll_cb)
        js->viewport_scroll_cb(&x, &y, js->viewport_scroll_user_data);
    ns_js_note_viewport_scroll(js, x, y);
}

static ns_node *
ns_iframe_document_node(const ns_node *iframe)
{
    if (!iframe) return NULL;
    for (ns_node *c = iframe->first_child; c; c = c->next_sibling)
        if (c->kind == NS_NODE_DOCUMENT) return c;
    return NULL;
}

static ns_node *
ns_iframe_content_root(const ns_node *iframe)
{
    if (!iframe) return NULL;
    for (ns_node *c = iframe->first_child; c; c = c->next_sibling) {
        if (c->kind == NS_NODE_DOCUMENT) {
            for (ns_node *e = c->first_child; e; e = e->next_sibling)
                if (e->kind == NS_NODE_ELEMENT) return e;
            return NULL;
        }
        if (c->kind == NS_NODE_ELEMENT) return c;
    }
    return NULL;
}

static ns_node *
ns_iframe_ensure_content_root(ns_node *iframe)
{
    ns_node *root = ns_iframe_content_root(iframe);
    if (root) return root;
    if (!iframe) return NULL;
    root = ns_node_new_element(g_strdup("html"));
    ns_node_append_child(root, ns_node_new_element(g_strdup("head")));
    ns_node_append_child(root, ns_node_new_element(g_strdup("body")));
    ns_node *doc = ns_node_new_document();
    ns_node_append_child(doc, root);
    ns_node_append_child(iframe, doc);
    return root;
}

JSValue
ns_iframe_platform_names(JSContext *fctx, ns_js *js)
{
    if (!js->platform_globals) {
        /* A frame made while the document is still being installed, before
         * any of the page's scripts ran: every name the window has is the
         * platform's. */
        JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx
                                                 : js->ctx;
        JSContext *saved = js->ctx;
        js->ctx = main_ctx;
        js->platform_globals = ns_js_snapshot_globals(js);
        js->ctx = saved;
    }
    JSValue names = JS_NewObjectProto(fctx, JS_NULL);
    GHashTableIter it;
    gpointer k;
    g_hash_table_iter_init(&it, js->platform_globals);
    while (g_hash_table_iter_next(&it, &k, NULL))
        JS_SetPropertyStr(fctx, names, (const char *)k, JS_TRUE);
    return names;
}

static void ns_install_pdf_plugins(JSContext *ctx);

static gboolean
ns_obj_has_defined_own_prop(JSContext *ctx, JSValueConst obj, const char *name)
{
    JSAtom atom = JS_NewAtom(ctx, name);
    JSPropertyDescriptor desc;
    int has = JS_GetOwnProperty(ctx, &desc, obj, atom);
    JS_FreeAtom(ctx, atom);
    if (has < 0) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        return FALSE;
    }
    if (has == 0) return FALSE;
    gboolean defined = (desc.flags & JS_PROP_GETSET) ||
                       !JS_IsUndefined(desc.value);
    JS_FreeValue(ctx, desc.value);
    JS_FreeValue(ctx, desc.getter);
    JS_FreeValue(ctx, desc.setter);
    return defined;
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

JSValue
ns_js_frame_window_events(JSContext *fctx)
{
    JSValue window_events = JS_NewObjectProto(fctx, JS_NULL);
    JS_SetPropertyStr(fctx, window_events, "add",
        JS_NewCFunction(fctx, ns_window_addEventListener,
                        "addEventListener", 2));
    JS_SetPropertyStr(fctx, window_events, "remove",
        JS_NewCFunction(fctx, ns_window_removeEventListener,
                        "removeEventListener", 2));
    JS_SetPropertyStr(fctx, window_events, "dispatch",
        JS_NewCFunction(fctx, ns_window_dispatchEvent,
                        "dispatchEvent", 1));
    return window_events;
}

void
ns_js_frame_realm_finish(ns_js *js, JSContext *fctx, JSValueConst fg,
                         JSValueConst parent_global)
{
    ns_install_pdf_plugins(fctx);
    ns_js_adopt_frame_window_events(js, fctx, fg);
    ns_js_link_interface_ctors(fctx);
    ns_js_lock_global_prototypes(fctx);
    ns_js_brand_node_interfaces(fctx, ns_element_class_id, ns_attr_class_id);
    JSValue parent_performance =
        JS_GetPropertyStr(fctx, parent_global, "performance");
    gboolean include_memory = JS_IsObject(parent_performance) &&
        ns_obj_has_defined_own_prop(fctx, parent_performance, "memory");
    JS_FreeValue(fctx, parent_performance);
    JS_SetPropertyStr(fctx, fg, "performance",
                      ns_make_performance_object(fctx, js, include_memory));
}

/* The URL a frame's document shows when it differs from the URL it was
 * loaded under: "about:blank" for a frame without a source and
 * "about:srcdoc" for a srcdoc frame, whose base URL and origin come from
 * the document that holds the frame (data-nd-frame-url). */
static void ns_js_set_doc_ready_state(ns_js *js, const ns_node *doc, int state);

static const char *
ns_iframe_doc_url(const ns_node *iframe)
{
    const char *u = iframe
        ? ns_element_get_attr(iframe, "data-nd-frame-doc-url") : NULL;
    return u && *u ? u : NULL;
}

static void
ns_frame_document_show_url(JSContext *ctx, JSValueConst doc, const char *url)
{
    if (!JS_IsObject(doc) || !url || !*url) return;
    JS_DefinePropertyValueStr(ctx, doc, "URL", JS_NewString(ctx, url),
                              JS_PROP_C_W_E);
    JS_DefinePropertyValueStr(ctx, doc, "documentURI", JS_NewString(ctx, url),
                              JS_PROP_C_W_E);
}

static JSValue
ns_iframe_build_content_document(JSContext *ctx, ns_node *iframe)
{
    if (!ns_iframe_ensure_content_root(iframe)) return JS_NULL;
    ns_node *doc = ns_iframe_document_node(iframe);
    if (!doc) return JS_NULL;
    if (doc->js_wrapper) return ns_make_element(ctx, doc);
    const char *cs = iframe
        ? ns_element_get_attr(iframe, "data-nd-frame-charset") : NULL;
    const char *url = iframe
        ? ns_element_get_attr(iframe, "data-nd-frame-url") : NULL;
    const char *shown = ns_iframe_doc_url(iframe);
    if (!url || !*url) {
        /* The frame's initial about:blank document: its base URL and origin
         * are its creator's, and it is complete from the start. */
        url = ns_js_node_doc_base(js_from_ctx(ctx), iframe);
        shown = "about:blank";
        ns_js_set_doc_ready_state(js_from_ctx(ctx), doc, 2);
    }
    gboolean is_xml = (doc->flags & NS_NODE_XML_DOC) != 0;
    const char *mime = is_xml ? "application/xml" : "text/html";
    g_autofree char *url_copy = g_strdup(url);
    g_autofree char *cs_copy = g_strdup(cs);
    g_autofree char *shown_copy = g_strdup(shown);
    JSValue cd = ns_make_realm_document(ctx, doc, url_copy, cs_copy, mime,
                                        is_xml, FALSE);
    ns_frame_document_show_url(ctx, cd, shown_copy);
    if (JS_IsObject(cd))
        JS_SetPropertyStr(ctx, cd, "defaultView", JS_GetGlobalObject(ctx));
    return cd;
}

gboolean
ns_iframe_is_cross_origin(ns_js *js, const ns_node *iframe)
{
    unsigned sandbox = ns_iframe_effective_sandbox(iframe);
    if ((sandbox & NS_SANDBOX_ACTIVE) &&
        !(sandbox & NS_SANDBOX_ALLOW_SAME_ORIGIN))
        return TRUE;
    const char *frame_url = ns_element_get_attr(iframe, "data-nd-frame-url");
    if (!frame_url || !*frame_url) return FALSE;
    const char *embedder = ns_js_node_doc_base(js, iframe);
    if (js && embedder == js->current_url && js->document_origin)
        embedder = js->document_origin;
    return !embedder || !ns_url_same_origin(frame_url, embedder);
}

JSValue
ns_iframe_content_document(JSContext *ctx, JSValueConst this_val, ns_node *n)
{
    JSValue realm = JS_GetPropertyStr(ctx, this_val, "__ndRealmDoc");
    if (JS_IsObject(realm)) return realm;
    JS_FreeValue(ctx, realm);
    ns_node *doc = ns_iframe_document_node(n);
    if (doc && doc->js_wrapper) return ns_make_element(ctx, doc);
    if (!ns_node_is_element_named(n, "iframe")) return JS_NULL;
    return ns_iframe_build_content_document(ctx, n);
}

/* A frame element has a content navigable only while it is connected to a
 * document that has a browsing context: the page's document or, through
 * frames, a document nested in it. A frame created by createElement() and
 * not inserted yet, or one in a document from createHTMLDocument(), has
 * none, so its contentWindow and contentDocument are null. */
static gboolean
ns_frame_owner_has_browsing_context(ns_js *js, const ns_node *frame)
{
    if (!js || !frame) return FALSE;
    /* The page's document, whichever document is current while a frame's
     * script runs; a frame's document hangs below its frame element. */
    const ns_node *page = js->ce_main_doc ? js->ce_main_doc : js->current_doc;
    const ns_node *root = frame;
    while (root->parent) root = root->parent;
    return root == page;
}

static JSValue
ns_element_get_contentDocument(JSContext *ctx, JSValueConst this_val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n || (!ns_node_is_element_named(n, "iframe") &&
               !ns_node_is_element_named(n, "object") &&
               !ns_node_is_element_named(n, "frame") &&
               !ns_node_is_element_named(n, "embed")))
        return JS_ThrowTypeError(ctx, "Illegal invocation");
    if (!ns_node_is_element_named(n, "iframe") &&
        !ns_node_is_element_named(n, "object")) return JS_NULL;
    if (!ns_frame_owner_has_browsing_context(js_from_ctx(ctx), n))
        return JS_NULL;
    if (ns_iframe_is_cross_origin(js_from_ctx(ctx), n)) return JS_NULL;
    return ns_iframe_content_document(ctx, this_val, n);
}

static gboolean
ns_node_is_frame_owner(const ns_node *n)
{
    return n && (ns_node_is_element_named(n, "iframe") ||
                 ns_node_is_element_named(n, "object") ||
                 ns_node_is_element_named(n, "frame") ||
                 ns_node_is_element_named(n, "embed"));
}

static JSValue
ns_element_get_contentWindow(JSContext *ctx, JSValueConst this_val)
{
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!ns_node_is_frame_owner(n))
        return JS_ThrowTypeError(ctx, "Illegal invocation");
    if (!ns_node_is_element_named(n, "iframe")) return JS_NULL;
    if (!ns_frame_owner_has_browsing_context(js_from_ctx(ctx), n))
        return JS_NULL;
    if (!ns_iframe_ensure_content_root(n)) return JS_NULL;
    JSValue win = ns_iframe_realm_window(ctx, this_val, n);
    if (!JS_IsObject(win) || !ns_iframe_is_cross_origin(js_from_ctx(ctx), n))
        return win;
    return ns_iframe_cross_origin_window(ctx, win);
}

JSValue
ns_window_child_frame_window(JSContext *ctx, ns_node *doc, uint32_t index,
                             const char *name, gboolean raw)
{
    const ns_node *frame = ns_window_child_frame(doc, index, name);
    if (!frame) return JS_UNDEFINED;
    JSValue el = ns_make_element(ctx, frame);
    JSValue win = JS_UNDEFINED;
    if (!raw)
        win = ns_element_get_contentWindow(ctx, el);
    else if (ns_node_is_element_named(frame, "iframe") &&
             ns_iframe_ensure_content_root((ns_node *)frame))
        win = ns_iframe_realm_window(ctx, el, (ns_node *)frame);
    JS_FreeValue(ctx, el);
    return win;
}

static JSValue
ns_element_getSVGDocument(JSContext *ctx, JSValueConst this_val,
                          int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    ns_node *n = ns_unwrap_element_mut(this_val);
    if (!n || (!ns_node_is_element_named(n, "iframe") &&
               !ns_node_is_element_named(n, "object") &&
               !ns_node_is_element_named(n, "embed")))
        return JS_NULL;
    JSValue doc = ns_element_get_contentDocument(ctx, this_val);
    if (!JS_IsObject(doc)) return doc;
    JSValue root = JS_GetPropertyStr(ctx, doc, "documentElement");
    JSValue name = JS_IsObject(root) ? JS_GetPropertyStr(ctx, root, "localName")
                                     : JS_NULL;
    const char *nm = JS_ToCString(ctx, name);
    gboolean is_svg = nm && g_ascii_strcasecmp(nm, "svg") == 0;
    if (nm) JS_FreeCString(ctx, nm);
    JS_FreeValue(ctx, name);
    JS_FreeValue(ctx, root);
    if (is_svg) return doc;
    JS_FreeValue(ctx, doc);
    return JS_NULL;
}

static JSValue
ns_element_getNumberOfChars(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    const ns_node *n = ns_unwrap_element(this_val);
    if (!n) return JS_NewInt32(ctx, 0);
    char *t = ns_node_collect_text(n);
    glong len = t ? g_utf8_strlen(t, -1) : 0;
    g_free(t);
    return JS_NewInt32(ctx, (int)len);
}

static const JSCFunctionListEntry ns_element_proto_funcs[] = {
    JS_CFUNC_DEF("getNumberOfChars",         0, ns_element_getNumberOfChars),
    JS_CFUNC_DEF("getSVGDocument",           0, ns_element_getSVGDocument),
    JS_CGETSET_DEF("contentDocument",        ns_element_get_contentDocument,        ns_element_noop_set),
    JS_CGETSET_DEF("contentWindow",          ns_element_get_contentWindow,          ns_element_noop_set),
    JS_CGETSET_DEF("tagName",                ns_element_get_tagName,                ns_element_noop_set),
    JS_CGETSET_DEF("accessKeyLabel",         ns_element_get_accessKeyLabel,         ns_element_noop_set),
    JS_CGETSET_DEF("localName",              ns_element_get_localName,              ns_element_noop_set),
    JS_CGETSET_DEF("prefix",                 ns_element_get_prefix,                 ns_element_noop_set),
    JS_CGETSET_DEF("textContent",            ns_element_get_textContent,            ns_element_set_textContent),
    JS_CGETSET_DEF("innerText",              ns_element_get_innerText,              ns_element_set_innerText),
    JS_CGETSET_DEF("outerText",              ns_element_get_innerText,              ns_element_set_outerText),
    JS_CGETSET_DEF("id",                     ns_element_get_id,                     ns_element_set_id),
    JS_CGETSET_DEF("className",              ns_element_get_className,              ns_element_set_className),
    JS_CGETSET_DEF("innerHTML",              ns_element_get_innerHTML,              ns_element_set_innerHTML),
    JS_CGETSET_DEF("outerHTML",              ns_element_get_outerHTML,              ns_element_set_outerHTML),
    JS_CGETSET_DEF("style",                  ns_element_get_style,                  ns_element_set_style),
    JS_CGETSET_DEF("classList",              ns_element_get_classList,              ns_element_set_classList),
    JS_CGETSET_DEF("relList",                ns_element_get_relList,                ns_element_set_relList),
    JS_CGETSET_DEF("itemScope",              ns_element_get_itemScope,              ns_element_set_itemScope),
    JS_CGETSET_DEF("itemId",                 ns_element_get_itemId,                 ns_element_set_itemId),
    JS_CGETSET_DEF("itemType",               ns_element_get_itemType,               ns_element_noop_set),
    JS_CGETSET_DEF("itemProp",               ns_element_get_itemProp,               ns_element_noop_set),
    JS_CGETSET_DEF("itemRef",                ns_element_get_itemRef,                ns_element_noop_set),
    JS_CGETSET_DEF("itemValue",              ns_element_get_itemValue,              ns_element_set_itemValue),
    JS_CGETSET_DEF("properties",             ns_element_get_properties,             ns_element_noop_set),
    JS_CGETSET_DEF("parentElement",          ns_element_get_parentElement,          ns_element_noop_set),
    JS_CGETSET_DEF("parentNode",             ns_element_get_parentNode,             ns_element_noop_set),
    JS_CGETSET_DEF("firstElementChild",      ns_element_get_firstElementChild,      ns_element_noop_set),
    JS_CGETSET_DEF("lastElementChild",       ns_element_get_lastElementChild,       ns_element_noop_set),
    JS_CGETSET_DEF("nextElementSibling",     ns_element_get_nextElementSibling,     ns_element_noop_set),
    JS_CGETSET_DEF("previousElementSibling", ns_element_get_previousElementSibling, ns_element_noop_set),
    JS_CGETSET_DEF("firstChild",             ns_element_get_firstChild,             ns_element_noop_set),
    JS_CGETSET_DEF("lastChild",              ns_element_get_lastChild,              ns_element_noop_set),
    JS_CGETSET_DEF("nextSibling",            ns_element_get_nextSibling,            ns_element_noop_set),
    JS_CGETSET_DEF("previousSibling",        ns_element_get_previousSibling,        ns_element_noop_set),
    JS_CGETSET_DEF("childNodes",             ns_element_get_childNodes,             ns_element_noop_set),
    JS_CGETSET_DEF("childElementCount",      ns_element_get_childElementCount,      ns_element_noop_set),
    JS_CGETSET_DEF("children",               ns_element_get_children,               ns_element_noop_set),
    JS_CFUNC_DEF("getAttribute",            1, ns_element_getAttribute),
    JS_CFUNC_DEF("hasAttribute",            1, ns_element_hasAttribute),
    JS_CFUNC_DEF("setAttribute",            2, ns_element_setAttribute),
    JS_CFUNC_DEF("removeAttribute",         1, ns_element_removeAttribute),
    JS_CFUNC_DEF("showPopover",             0, ns_element_showPopover),
    JS_CFUNC_DEF("hidePopover",             0, ns_element_hidePopover),
    JS_CFUNC_DEF("togglePopover",           0, ns_element_togglePopover),
    JS_CFUNC_DEF("toggleAttribute",         1, ns_element_toggleAttribute),
    JS_CFUNC_DEF("getAttributeNS",          2, ns_element_getAttributeNS),
    JS_CFUNC_DEF("hasAttributeNS",          2, ns_element_hasAttributeNS),
    JS_CFUNC_DEF("setAttributeNS",          3, ns_element_setAttributeNS),
    JS_CFUNC_DEF("removeAttributeNS",       2, ns_element_removeAttributeNS),
    JS_CFUNC_DEF("requestFullscreen",       0, ns_element_request_fullscreen),
    JS_CFUNC_DEF("webkitRequestFullscreen", 0, ns_element_request_fullscreen),
    JS_CFUNC_DEF("webkitRequestFullScreen", 0, ns_element_request_fullscreen),
    JS_CFUNC_DEF("mozRequestFullScreen",    0, ns_element_request_fullscreen),
    JS_CFUNC_DEF("msRequestFullscreen",     0, ns_element_request_fullscreen),
    JS_CFUNC_DEF("getAnimations",           0, ns_event_empty_array),
    JS_CFUNC_DEF("animate",                 2, ns_element_animate),
    JS_CFUNC_DEF("getRootNode",             1, ns_element_getRootNode),
    JS_CFUNC_DEF("isEqualNode",             1, ns_element_isEqualNode),
    JS_CFUNC_DEF("isSameNode",              1, ns_element_isSameNode),
    JS_CFUNC_DEF("compareDocumentPosition", 1, ns_element_compareDocumentPosition),
    JS_CFUNC_DEF("lookupPrefix",            1, ns_element_lookupPrefix),
    JS_CFUNC_DEF("lookupNamespaceURI",      1, ns_element_lookupNamespaceURI),
    JS_CFUNC_DEF("isDefaultNamespace",      1, ns_element_isDefaultNamespace),
    JS_CFUNC_DEF("getClientRects",          0, ns_element_getClientRects),
    JS_CFUNC_DEF("scrollBy",                2, ns_element_scroll_by),
    JS_CFUNC_DEF("scrollTo",                2, ns_element_scroll_to),
    JS_CFUNC_DEF("scroll",                  2, ns_element_scroll_to),
    JS_CFUNC_DEF("scrollIntoViewIfNeeded",  1, ns_element_scrollIntoView),
    JS_CFUNC_DEF("requestPointerLock",      0, ns_element_requestPointerLock),
    JS_CFUNC_DEF("releasePointerLock",      0, ns_event_noop),
    JS_CFUNC_DEF("releaseCapture",          0, ns_event_noop),
    JS_CFUNC_DEF("setCapture",              0, ns_event_noop),
    JS_CFUNC_DEF("associateForm",           1, ns_event_noop),
    JS_CFUNC_DEF("setMessage",              2, ns_event_noop),
    JS_CFUNC_DEF("syncInputValidity",       1, ns_event_noop),
    JS_CFUNC_DEF("_connect",                0, ns_event_noop),
    JS_CFUNC_DEF("_disconnect",             0, ns_event_noop),
    JS_CGETSET_DEF("length",            ns_element_get_text_length, ns_element_noop_set),
    JS_CFUNC_DEF("substringData", 2, ns_element_substring_data),
    JS_CFUNC_DEF("appendData",    1, ns_element_append_data),
    JS_CFUNC_DEF("deleteData",    2, ns_element_delete_data),
    JS_CFUNC_DEF("insertData",    2, ns_element_insert_data),
    JS_CFUNC_DEF("replaceData",   3, ns_element_replace_data),
    JS_CFUNC_DEF("splitText",     1, ns_element_split_text),
    JS_CFUNC_DEF("select",              0, ns_input_select),
    JS_CFUNC_DEF("setSelectionRange",   3, ns_input_setSelectionRange),
    JS_CFUNC_DEF("setRangeText",        1, ns_input_setRangeText),
    JS_CFUNC_DEF("stepUp",              0, ns_input_stepUp),
    JS_CFUNC_DEF("stepDown",            0, ns_input_stepDown),
    JS_CFUNC_DEF("showPicker",          0, ns_element_show_picker),
    JS_CFUNC_DEF("play",                0, ns_media_play),
    JS_CFUNC_DEF("pause",               0, ns_media_pause),
    JS_CFUNC_DEF("load",                0, ns_media_load),
    JS_CFUNC_DEF("canPlayType",         1, ns_media_canPlayType),
    JS_CFUNC_DEF("fastSeek",            1, ns_media_fast_seek),
    JS_CFUNC_DEF("addTextTrack",        3, ns_media_addTextTrack),
    JS_CFUNC_DEF("setMediaKeys",        1, ns_media_set_media_keys),
    JS_CFUNC_DEF("getVideoPlaybackQuality", 0, ns_media_get_video_playback_quality),
    JS_CFUNC_DEF("requestVideoFrameCallback", 1, ns_media_request_video_frame_callback),
    JS_CFUNC_DEF("cancelVideoFrameCallback",  1, ns_window_cancelAnimationFrame),
    JS_CGETSET_DEF("validity",          ns_element_get_validity,          ns_element_noop_set),
    JS_CGETSET_DEF("validationMessage", ns_element_get_validation_message, ns_element_noop_set),
    JS_CGETSET_DEF("willValidate",      ns_element_get_will_validate,     ns_element_noop_set),
    JS_CGETSET_DEF("labels",            ns_element_get_labels,            ns_element_noop_set),
    JS_CGETSET_DEF("files",             ns_input_get_files,               ns_element_noop_set),
    JS_CGETSET_DEF("indeterminate",     ns_element_get_indeterminate,     ns_element_set_indeterminate),
    JS_CGETSET_DEF("selectionStart",    ns_element_get_selection_start,   ns_element_set_selection_start),
    JS_CGETSET_DEF("selectionEnd",      ns_element_get_selection_end,     ns_element_set_selection_end),
    JS_CGETSET_DEF("selectionDirection", ns_element_get_selection_dir,    ns_element_set_selection_dir),
    JS_CGETSET_DEF("textLength",        ns_text_control_get_text_length,  ns_element_noop_set),
    JS_CGETSET_DEF("defaultValue",      ns_element_get_default_value,     ns_element_set_default_value),
    JS_CGETSET_DEF("defaultChecked",    ns_element_get_default_checked,   ns_element_set_default_checked),
    JS_CGETSET_DEF("defaultSelected",   ns_element_get_default_selected,  ns_element_set_default_selected),
    JS_CGETSET_DEF("currentTime",       ns_media_get_current_time,        ns_media_set_current_time),
    JS_CGETSET_DEF("duration",          ns_media_get_duration,            ns_element_noop_set),
    JS_CGETSET_DEF("paused",            ns_media_get_paused,              ns_element_noop_set),
    JS_CGETSET_DEF("ended",             ns_media_get_ended,               ns_element_noop_set),
    JS_CGETSET_DEF("seeking",           ns_element_get_zero_int,          ns_element_noop_set),
    JS_CGETSET_DEF("volume",            ns_media_get_volume,              ns_media_set_volume),
    JS_CGETSET_DEF("playbackRate",      ns_media_get_playbackRate,        ns_media_set_playbackRate),
    JS_CGETSET_DEF("defaultPlaybackRate", ns_media_get_defaultPlaybackRate, ns_media_set_defaultPlaybackRate),
    JS_CGETSET_DEF("error",             ns_media_get_error,               ns_element_noop_set),
    JS_CGETSET_DEF("muted",             ns_media_get_muted,               ns_media_set_muted),
    JS_CGETSET_DEF("readyState",        ns_media_get_readyState,          ns_element_noop_set),
    JS_CGETSET_DEF("networkState",      ns_media_get_networkState,        ns_element_noop_set),
    JS_CGETSET_DEF("seekable",          ns_media_get_seekable_ranges,     ns_element_noop_set),
    JS_CGETSET_DEF("buffered",          ns_media_get_buffered_ranges,     ns_element_noop_set),
    JS_CGETSET_DEF("played",            ns_media_get_played_ranges,       ns_element_noop_set),
    JS_CGETSET_DEF("textTracks",        ns_media_get_textTracks,          ns_element_noop_set),
    JS_CGETSET_DEF("videoTracks",       ns_element_get_empty_array_prop,  ns_element_noop_set),
    JS_CGETSET_DEF("audioTracks",       ns_element_get_empty_array_prop,  ns_element_noop_set),
    JS_CGETSET_DEF("valueAsNumber",     ns_element_get_value_as_number,   ns_element_set_value_as_number),
    JS_CGETSET_DEF("valueAsDate",       ns_element_get_value_as_date,     ns_element_set_value_as_date),
    JS_CGETSET_DEF("position",          ns_element_get_progress_position, ns_element_noop_set),
    JS_CGETSET_DEF("encoding",          ns_element_get_form_enctype,      ns_element_set_form_enctype),
    JS_CGETSET_DEF("isContentEditable", ns_element_get_isContentEditable,  ns_element_noop_set),
    JS_CGETSET_DEF("translate",         ns_element_get_translate,         ns_element_set_translate),
    JS_CGETSET_DEF("offsetParent",      ns_element_get_offsetParent,      ns_element_noop_set),
    JS_CGETSET_DEF("videoWidth",        ns_element_get_zero_int,          ns_element_noop_set),
    JS_CGETSET_DEF("videoHeight",       ns_element_get_zero_int,          ns_element_noop_set),
    JS_CGETSET_DEF("srcObject",         ns_media_get_srcObject,           ns_media_set_srcObject),
    JS_CGETSET_DEF("clientInformation", ns_element_get_null,              ns_element_noop_set),
    JS_CFUNC_DEF("decode",            0, ns_returns_resolved_undefined),
    JS_CFUNC_DEF("toBlob",            1, ns_element_toBlob),
    JS_CFUNC_DEF("attachInternals",   0, ns_element_attachInternals),
    JS_CGETSET_DEF("_internals",      ns_element_get_internals, ns_element_noop_set),
    JS_CGETSET_DEF("index",           ns_element_get_option_index,   ns_element_noop_set),
    JS_CGETSET_DEF("rows",            ns_element_table_rows,         ns_element_set_rows),
    JS_CGETSET_DEF("caption",         ns_element_table_caption,      ns_element_noop_set),
    JS_CGETSET_DEF("tHead",           ns_element_table_thead,        ns_element_noop_set),
    JS_CGETSET_DEF("tFoot",           ns_element_table_tfoot,        ns_element_noop_set),
    JS_CGETSET_DEF("tBodies",         ns_element_table_tbodies,      ns_element_noop_set),
    JS_CGETSET_DEF("cells",           ns_element_tr_cells,           ns_element_noop_set),
    JS_CGETSET_DEF("rowIndex",        ns_element_get_zero_int,       ns_element_noop_set),
    JS_CGETSET_DEF("sectionRowIndex", ns_element_get_zero_int,       ns_element_noop_set),
    JS_CGETSET_DEF("cellIndex",       ns_element_get_zero_int,       ns_element_noop_set),
    JS_CGETSET_MAGIC_DEF("colSpan",   ns_element_int_attr_getter,    ns_element_int_attr_setter, 6),
    JS_CGETSET_MAGIC_DEF("rowSpan",   ns_element_int_attr_getter,    ns_element_int_attr_setter, 7),
    JS_CGETSET_DEF("returnValue",     ns_dialog_get_returnValue,     ns_dialog_set_returnValue),
    JS_CFUNC_DEF("createCaption",  0, ns_table_createCaption),
    JS_CFUNC_DEF("createTHead",    0, ns_table_createTHead),
    JS_CFUNC_DEF("createTFoot",    0, ns_table_createTFoot),
    JS_CFUNC_DEF("createTBody",    0, ns_table_createTBody),
    JS_CFUNC_DEF("deleteCaption",  0, ns_table_deleteCaption),
    JS_CFUNC_DEF("deleteTHead",    0, ns_table_deleteTHead),
    JS_CFUNC_DEF("deleteTFoot",    0, ns_table_deleteTFoot),
    JS_CFUNC_DEF("insertRow",      1, ns_table_insertRow),
    JS_CFUNC_DEF("deleteRow",      1, ns_table_deleteRow),
    JS_CFUNC_DEF("insertCell",     1, ns_tr_insertCell),
    JS_CFUNC_DEF("deleteCell",     1, ns_tr_deleteCell),
    JS_CFUNC_DEF("add",            2, ns_select_add),
    JS_CFUNC_DEF("appendChild",             1, ns_element_appendChild),
    JS_CFUNC_DEF("removeChild",             1, ns_element_removeChild),
    JS_CFUNC_DEF("insertBefore",            2, ns_element_insertBefore),
    JS_CFUNC_DEF("moveBefore",              2, ns_element_moveBefore),
    JS_CFUNC_DEF("replaceChild",            2, ns_element_replaceChild),
    JS_CFUNC_DEF("insertAdjacentHTML",      2, ns_element_insertAdjacentHTML),
    JS_CFUNC_DEF("getHTML",                 0, ns_element_getHTML),
    JS_CFUNC_DEF("setHTMLUnsafe",           1, ns_element_setHTMLUnsafe),
    JS_CFUNC_DEF("insertAdjacentElement",   2, ns_element_insertAdjacentElement),
    JS_CFUNC_DEF("insertAdjacentText",      2, ns_element_insertAdjacentText),
    JS_CFUNC_DEF("replaceChildren",         0, ns_element_replaceChildren),
    JS_CFUNC_DEF("getAttributeNames",       0, ns_element_getAttributeNames),
    JS_CFUNC_DEF("getAttributeNode",        1, ns_element_getAttributeNode),
    JS_CFUNC_DEF("getAttributeNodeNS",      2, ns_element_getAttributeNodeNS),
    JS_CFUNC_DEF("removeAttributeNode",     1, ns_element_removeAttributeNode),
    JS_CFUNC_DEF("setAttributeNode",        1, ns_element_setAttributeNode),
    JS_CFUNC_DEF("setAttributeNodeNS",      1, ns_element_setAttributeNode),
    JS_CFUNC_DEF("hasAttributes",           0, ns_element_hasAttributes),
    JS_CFUNC_DEF("remove",                  0, ns_element_remove_self),
    JS_CFUNC_DEF("cloneNode",               1, ns_element_cloneNode),
    JS_CFUNC_DEF("normalize",               0, ns_element_normalize),
    JS_CFUNC_DEF("append",                  0, ns_element_append),
    JS_CFUNC_DEF("prepend",                 0, ns_element_prepend),
    JS_CFUNC_DEF("before",                  0, ns_element_before),
    JS_CFUNC_DEF("after",                   0, ns_element_after),
    JS_CFUNC_DEF("replaceWith",             0, ns_element_replaceWith),
    JS_CFUNC_DEF("addEventListener",        2, ns_element_addEventListener),
    JS_CFUNC_DEF("removeEventListener",     2, ns_element_removeEventListener),
    JS_CFUNC_DEF("getElementsByTagName",    1, ns_element_getElementsByTagName),
    JS_CFUNC_DEF("getElementsByTagNameNS",  2, ns_element_getElementsByTagNameNS),
    JS_CFUNC_DEF("getElementById",          1, ns_element_getElementById),
    JS_CFUNC_DEF("getElementsByClassName",  1, ns_element_getElementsByClassName),
    JS_CFUNC_DEF("querySelector",           1, ns_element_querySelector),
    JS_CFUNC_DEF("querySelectorAll",        1, ns_element_querySelectorAll),
    JS_CFUNC_DEF("matches",                 1, ns_element_matches),
    JS_CFUNC_DEF("webkitMatchesSelector",   1, ns_element_matches),
    JS_CFUNC_DEF("closest",                 1, ns_element_closest),
    JS_CFUNC_DEF("contains",                1, ns_element_contains),
    JS_CFUNC_DEF("hasChildNodes",           0, ns_element_hasChildNodes),
    JS_CFUNC_DEF("getBoundingClientRect",   0, ns_element_getBoundingClientRect),
    JS_CFUNC_DEF("getBBox",                 0, ns_element_getBBox),
    JS_CFUNC_DEF("getCTM",                  0, ns_element_getCTM),
    JS_CFUNC_DEF("getScreenCTM",            0, ns_element_getScreenCTM),
    JS_CFUNC_DEF("getTotalLength",          0, ns_element_getTotalLength),
    JS_CFUNC_DEF("getPointAtLength",        1, ns_element_getPointAtLength),
    JS_CFUNC_DEF("createSVGPoint",          0, ns_element_createSVGPoint),
    JS_CFUNC_DEF("createSVGRect",           0, ns_element_createSVGRect),
    JS_CFUNC_DEF("createSVGMatrix",         0, ns_element_createSVGMatrix),
    JS_CFUNC_DEF("createSVGTransform",      0, ns_element_createSVGTransform),
    JS_CGETSET_DEF("ownerSVGElement",       ns_element_get_ownerSVGElement, ns_element_noop_set),
    JS_CFUNC_DEF("focus",                   0, ns_element_focus),
    JS_CFUNC_DEF("blur",                    0, ns_element_blur),
    JS_CFUNC_DEF("click",                   0, ns_element_click),
    JS_CFUNC_DEF("submit",                  0, ns_element_form_submit),
    JS_CFUNC_DEF("requestSubmit",           0, ns_element_form_requestSubmit),
    JS_CFUNC_DEF("reset",                   0, ns_element_form_reset),
    JS_CFUNC_DEF("checkValidity",           0, ns_element_check_validity),
    JS_CFUNC_DEF("reportValidity",          0, ns_element_check_validity),
    JS_CFUNC_DEF("setCustomValidity",       1, ns_element_setCustomValidity),
    JS_CFUNC_DEF("cancelAsync",             1, ns_element_cancelAsync),
    JS_CFUNC_DEF("announce",                1, ns_event_noop),
    JS_CFUNC_DEF("scrollIntoView",          0, ns_element_scrollIntoView),
    JS_CFUNC_DEF("checkVisibility",         0, ns_element_checkVisibility),
    JS_CFUNC_DEF("setPointerCapture",       1, ns_element_setPointerCapture),
    JS_CFUNC_DEF("releasePointerCapture",   1, ns_element_releasePointerCapture),
    JS_CFUNC_DEF("hasPointerCapture",       1, ns_element_hasPointerCapture),
    JS_CFUNC_DEF("show",                    0, ns_element_show),
    JS_CFUNC_DEF("showModal",               0, ns_element_showModal),
    JS_CFUNC_DEF("close",                   0, ns_element_close),
    JS_CFUNC_DEF("requestClose",            0, ns_element_requestClose),
    JS_CFUNC_DEF("dispatchEvent",           1, ns_element_dispatchEvent),
    JS_CFUNC_DEF("assignedNodes",           0, ns_element_assignedNodes),
    JS_CFUNC_DEF("assignedElements",        0, ns_element_assignedElements),
    JS_CFUNC_DEF("getContext",              1, ns_element_getContext),
    JS_CFUNC_DEF("toDataURL",               0, ns_element_toDataURL),
    JS_CGETSET_DEF("nodeType",      ns_element_get_nodeType, ns_element_noop_set),
    JS_CGETSET_DEF("nodeValue",     ns_element_get_nodeValue, ns_element_set_nodeValue),
    JS_CGETSET_DEF("wholeText",     ns_element_get_wholeText, ns_element_noop_set),
    JS_CFUNC_DEF("replaceWholeText", 1, ns_text_replaceWholeText),
    JS_CGETSET_DEF("nodeName",      ns_element_get_nodeName, ns_element_noop_set),
    JS_CGETSET_DEF("dataset",       ns_element_get_dataset,  ns_element_noop_set),
    JS_CGETSET_DEF("offsetTop",     ns_element_get_offsetTop,    ns_element_noop_set),
    JS_CGETSET_DEF("offsetLeft",    ns_element_get_offsetLeft,   ns_element_noop_set),
    JS_CGETSET_DEF("offsetWidth",   ns_element_get_offsetWidth,  ns_element_noop_set),
    JS_CGETSET_DEF("offsetHeight",  ns_element_get_offsetHeight, ns_element_noop_set),
    JS_CGETSET_DEF("clientTop",     ns_element_get_clientTop,   ns_element_noop_set),
    JS_CGETSET_DEF("clientLeft",    ns_element_get_clientLeft,  ns_element_noop_set),
    JS_CGETSET_DEF("clientWidth",   ns_element_get_clientWidth,  ns_element_noop_set),
    JS_CGETSET_DEF("clientHeight",  ns_element_get_clientHeight, ns_element_noop_set),
    JS_CGETSET_DEF("scrollTop",     ns_element_get_scrollTop,    ns_element_set_scrollTop),
    JS_CGETSET_DEF("scrollLeft",    ns_element_get_scrollLeft,   ns_element_set_scrollLeft),
    JS_CGETSET_DEF("scrollWidth",   ns_element_get_scrollWidth,  ns_element_noop_set),
    JS_CGETSET_DEF("scrollHeight",  ns_element_get_scrollHeight, ns_element_noop_set),
    JS_CGETSET_DEF("attributes",    ns_element_get_attributes, ns_element_noop_set),
    JS_CGETSET_DEF("naturalWidth",  ns_element_img_natural_width, ns_element_noop_set),
    JS_CGETSET_DEF("naturalHeight", ns_element_img_natural_height, ns_element_noop_set),
    JS_CGETSET_DEF("complete",      ns_element_img_complete, ns_element_noop_set),
    JS_CGETSET_DEF("currentSrc",    ns_element_img_current_src, ns_element_noop_set),
    JS_CGETSET_DEF("content",       ns_element_template_content, ns_element_set_content),
    JS_CGETSET_DEF("hidden",        ns_element_get_hidden,     ns_element_set_hidden),
    JS_CGETSET_MAGIC_DEF("title",       ns_element_attr_getter, ns_element_attr_setter, 0),
    JS_CGETSET_MAGIC_DEF("name",        ns_element_attr_getter, ns_element_attr_setter, 1),
    JS_CGETSET_MAGIC_DEF("alt",         ns_element_attr_getter, ns_element_attr_setter, 2),
    JS_CGETSET_MAGIC_DEF("src",         ns_element_attr_getter, ns_element_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("href",        ns_element_anchor_part_get, ns_element_anchor_href_set, NS_ANCHOR_HREF),
    JS_CGETSET_MAGIC_DEF("protocol",    ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_PROTOCOL),
    JS_CGETSET_MAGIC_DEF("host",        ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_HOST),
    JS_CGETSET_MAGIC_DEF("hostname",    ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_HOSTNAME),
    JS_CGETSET_MAGIC_DEF("port",        ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_PORT),
    JS_CGETSET_MAGIC_DEF("pathname",    ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_PATHNAME),
    JS_CGETSET_MAGIC_DEF("search",      ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_SEARCH),
    JS_CGETSET_MAGIC_DEF("hash",        ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_HASH),
    JS_CGETSET_MAGIC_DEF("origin",      ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_ORIGIN),
    JS_CGETSET_MAGIC_DEF("username",    ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_USERNAME),
    JS_CGETSET_MAGIC_DEF("password",    ns_element_anchor_part_get, ns_element_url_part_set, NS_ANCHOR_PASSWORD),
    JS_CGETSET_DEF("type",              ns_element_get_type,    ns_element_set_type),
    JS_CGETSET_MAGIC_DEF("placeholder", ns_element_attr_getter, ns_element_attr_setter, 6),
    JS_CGETSET_MAGIC_DEF("lang",        ns_element_attr_getter, ns_element_attr_setter, 7),
    JS_CGETSET_DEF("dir",               ns_element_get_dir,     ns_element_set_dir),
    JS_CGETSET_MAGIC_DEF("action",      ns_element_attr_getter, ns_element_attr_setter,  9),
    JS_CGETSET_MAGIC_DEF("method",      ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_METHOD),
    JS_CGETSET_MAGIC_DEF("enctype",     ns_element_attr_getter, ns_element_attr_setter, 11),
    JS_CGETSET_MAGIC_DEF("target",      ns_element_attr_getter, ns_element_attr_setter, 12),
    JS_CGETSET_MAGIC_DEF("rel",         ns_element_attr_getter, ns_element_attr_setter, 13),
    JS_CGETSET_MAGIC_DEF("accept",      ns_element_attr_getter, ns_element_attr_setter, 14),
    JS_CGETSET_MAGIC_DEF("acceptCharset", ns_element_attr_getter, ns_element_attr_setter, 15),
    JS_CGETSET_DEF("autocomplete", ns_element_get_autocomplete, ns_element_set_autocomplete),
    JS_CGETSET_DEF("list",        ns_element_get_list_ref, ns_element_list_set),
    JS_CGETSET_MAGIC_DEF("min",         ns_element_range_number_getter, ns_element_range_number_setter, NS_RANGE_MIN),
    JS_CGETSET_MAGIC_DEF("max",         ns_element_range_number_getter, ns_element_range_number_setter, NS_RANGE_MAX),
    JS_CGETSET_MAGIC_DEF("low",         ns_element_range_number_getter, ns_element_range_number_setter, NS_RANGE_LOW),
    JS_CGETSET_MAGIC_DEF("high",        ns_element_range_number_getter, ns_element_range_number_setter, NS_RANGE_HIGH),
    JS_CGETSET_MAGIC_DEF("optimum",     ns_element_range_number_getter, ns_element_range_number_setter, NS_RANGE_OPTIMUM),
    JS_CGETSET_MAGIC_DEF("step",        ns_element_attr_getter, ns_element_attr_setter, 20),
    JS_CGETSET_MAGIC_DEF("pattern",     ns_element_attr_getter, ns_element_attr_setter, 21),
    JS_CGETSET_DEF("spellcheck",    ns_element_get_spellcheck, ns_element_set_spellcheck),
    JS_CGETSET_MAGIC_DEF("crossOrigin",    ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_CROSSORIGIN),
    JS_CGETSET_MAGIC_DEF("referrerPolicy", ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_REFERRERPOLICY),
    JS_CGETSET_MAGIC_DEF("decoding",       ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_DECODING),
    JS_CGETSET_MAGIC_DEF("loading",        ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_LOADING),
    JS_CGETSET_MAGIC_DEF("fetchPriority",  ns_element_attr_getter, ns_element_attr_setter, 27),
    JS_CGETSET_DEF("sizes",                ns_element_get_sizes_list, ns_element_attr_setter_sizes),
    JS_CGETSET_DEF("sandbox",              ns_element_get_sandbox_list, ns_element_attr_setter_sandbox),
    JS_CGETSET_MAGIC_DEF("srcset",         ns_element_attr_getter, ns_element_attr_setter, 29),
    JS_CGETSET_MAGIC_DEF("useMap",         ns_element_attr_getter, ns_element_attr_setter, 30),
    JS_CGETSET_MAGIC_DEF("inputMode",      ns_element_attr_getter, ns_element_attr_setter, 31),
    JS_CGETSET_MAGIC_DEF("size",           ns_element_int_attr_getter, ns_element_int_attr_setter, 2),
    JS_CGETSET_MAGIC_DEF("cols",           ns_element_int_attr_getter, ns_element_int_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("allowFullscreen", ns_element_bool_attr_getter, ns_element_bool_attr_setter, 0),
    JS_CGETSET_MAGIC_DEF("declare",        ns_element_bool_attr_getter, ns_element_bool_attr_setter, 1),
    JS_CGETSET_MAGIC_DEF("defaultMuted",   ns_element_bool_attr_getter, ns_element_bool_attr_setter, 2),
    JS_CGETSET_MAGIC_DEF("default",        ns_element_bool_attr_getter, ns_element_bool_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("noHref",         ns_element_bool_attr_getter, ns_element_bool_attr_setter, 4),
    JS_CGETSET_MAGIC_DEF("noShade",        ns_element_bool_attr_getter, ns_element_bool_attr_setter, 5),
    JS_CGETSET_MAGIC_DEF("compact",        ns_element_bool_attr_getter, ns_element_bool_attr_setter, 6),
    JS_CGETSET_MAGIC_DEF("noWrap",         ns_element_bool_attr_getter, ns_element_bool_attr_setter, 7),
    JS_CGETSET_MAGIC_DEF("trueSpeed",      ns_element_bool_attr_getter, ns_element_bool_attr_setter, 8),
    JS_CGETSET_MAGIC_DEF("noResize",       ns_element_bool_attr_getter, ns_element_bool_attr_setter, 9),
    JS_CGETSET_MAGIC_DEF("face",           ns_element_attr_getter, ns_element_attr_setter, 130),
    JS_CGETSET_MAGIC_DEF("text",           ns_element_attr_getter, ns_element_attr_setter, 131),
    JS_CGETSET_MAGIC_DEF("hspace",         ns_element_int_attr_getter, ns_element_int_attr_setter, 11),
    JS_CGETSET_MAGIC_DEF("vspace",         ns_element_int_attr_getter, ns_element_int_attr_setter, 12),
    JS_CGETSET_MAGIC_DEF("scrollAmount",   ns_element_int_attr_getter, ns_element_int_attr_setter, 13),
    JS_CGETSET_MAGIC_DEF("scrollDelay",    ns_element_int_attr_getter, ns_element_int_attr_setter, 14),
    JS_CGETSET_MAGIC_DEF("maxLength",      ns_element_int_attr_getter, ns_element_int_attr_setter, 0),
    JS_CGETSET_MAGIC_DEF("minLength",      ns_element_int_attr_getter, ns_element_int_attr_setter, 1),
    JS_CGETSET_MAGIC_DEF("span",           ns_element_int_attr_getter, ns_element_int_attr_setter, 5),
    JS_CGETSET_MAGIC_DEF("width",          ns_element_dimension_getter, ns_element_dimension_setter, 8),
    JS_CGETSET_MAGIC_DEF("height",         ns_element_dimension_getter, ns_element_dimension_setter, 9),
    JS_CFUNC_DEF("beginElement",   0, ns_svg_beginElement),
    JS_CFUNC_DEF("setCurrentTime", 1, ns_svg_setCurrentTime),
    JS_CGETSET_MAGIC_DEF("start",          ns_element_int_attr_getter, ns_element_int_attr_setter, 10),
    JS_CGETSET_MAGIC_DEF("coords",         ns_element_attr_getter, ns_element_attr_setter, 37),
    JS_CGETSET_MAGIC_DEF("shape",          ns_element_attr_getter, ns_element_attr_setter, 38),
    JS_CGETSET_MAGIC_DEF("formAction",     ns_element_attr_getter, ns_element_attr_setter, 39),
    JS_CGETSET_MAGIC_DEF("formMethod",     ns_element_attr_getter, ns_element_attr_setter, 40),
    JS_CGETSET_MAGIC_DEF("formEnctype",    ns_element_attr_getter, ns_element_attr_setter, 41),
    JS_CGETSET_MAGIC_DEF("formTarget",     ns_element_attr_getter, ns_element_attr_setter, 42),
    JS_CGETSET_MAGIC_DEF("integrity",      ns_element_attr_getter, ns_element_attr_setter, 43),
    JS_CGETSET_MAGIC_DEF("kind",           ns_element_attr_getter, ns_element_attr_setter, 44),
    JS_CGETSET_MAGIC_DEF("hreflang",       ns_element_attr_getter, ns_element_attr_setter, 46),
    JS_CGETSET_MAGIC_DEF("charset",        ns_element_attr_getter, ns_element_attr_setter, 47),
    JS_CGETSET_MAGIC_DEF("ping",           ns_element_attr_getter, ns_element_attr_setter, 90),
    JS_CGETSET_MAGIC_DEF("rev",            ns_element_attr_getter, ns_element_attr_setter, 91),
    JS_CGETSET_MAGIC_DEF("as",             ns_element_attr_getter, ns_element_attr_setter, 92),
    JS_CGETSET_MAGIC_DEF("align",          ns_element_attr_getter, ns_element_attr_setter, 93),
    JS_CGETSET_MAGIC_DEF("vAlign",         ns_element_attr_getter, ns_element_attr_setter, 94),
    JS_CGETSET_MAGIC_DEF("ch",             ns_element_attr_getter, ns_element_attr_setter, 95),
    JS_CGETSET_MAGIC_DEF("chOff",          ns_element_attr_getter, ns_element_attr_setter, 96),
    JS_CGETSET_MAGIC_DEF("bgColor",        ns_element_attr_getter, ns_element_attr_setter, 97),
    JS_CGETSET_MAGIC_DEF("background",     ns_element_attr_getter, ns_element_attr_setter, 98),
    JS_CGETSET_MAGIC_DEF("link",           ns_element_attr_getter, ns_element_attr_setter, 99),
    JS_CGETSET_MAGIC_DEF("vLink",          ns_element_attr_getter, ns_element_attr_setter, 100),
    JS_CGETSET_MAGIC_DEF("aLink",          ns_element_attr_getter, ns_element_attr_setter, 101),
    JS_CGETSET_MAGIC_DEF("color",          ns_element_attr_getter, ns_element_attr_setter, 102),
    JS_CGETSET_MAGIC_DEF("clear",          ns_element_attr_getter, ns_element_attr_setter, 103),
    JS_CGETSET_MAGIC_DEF("summary",        ns_element_attr_getter, ns_element_attr_setter, 104),
    JS_CGETSET_MAGIC_DEF("frame",          ns_element_attr_getter, ns_element_attr_setter, 105),
    JS_CGETSET_MAGIC_DEF("rules",          ns_element_attr_getter, ns_element_attr_setter, 106),
    JS_CGETSET_MAGIC_DEF("border",         ns_element_attr_getter, ns_element_attr_setter, 107),
    JS_CGETSET_MAGIC_DEF("cellPadding",    ns_element_attr_getter, ns_element_attr_setter, 108),
    JS_CGETSET_MAGIC_DEF("cellSpacing",    ns_element_attr_getter, ns_element_attr_setter, 109),
    JS_CGETSET_MAGIC_DEF("axis",           ns_element_attr_getter, ns_element_attr_setter, 110),
    JS_CGETSET_MAGIC_DEF("abbr",           ns_element_attr_getter, ns_element_attr_setter, 111),
    JS_CGETSET_MAGIC_DEF("headers",        ns_element_attr_getter, ns_element_attr_setter, 112),
    JS_CGETSET_MAGIC_DEF("scheme",         ns_element_attr_getter, ns_element_attr_setter, 113),
    JS_CGETSET_MAGIC_DEF("standby",        ns_element_attr_getter, ns_element_attr_setter, 114),
    JS_CGETSET_MAGIC_DEF("codeType",       ns_element_attr_getter, ns_element_attr_setter, 115),
    JS_CGETSET_MAGIC_DEF("codeBase",       ns_element_attr_getter, ns_element_attr_setter, 116),
    JS_CGETSET_MAGIC_DEF("code",           ns_element_attr_getter, ns_element_attr_setter, 117),
    JS_CGETSET_MAGIC_DEF("archive",        ns_element_attr_getter, ns_element_attr_setter, 118),
    JS_CGETSET_MAGIC_DEF("scrolling",      ns_element_attr_getter, ns_element_attr_setter, 119),
    JS_CGETSET_MAGIC_DEF("frameBorder",    ns_element_attr_getter, ns_element_attr_setter, 120),
    JS_CGETSET_MAGIC_DEF("marginWidth",    ns_element_attr_getter, ns_element_attr_setter, 121),
    JS_CGETSET_MAGIC_DEF("marginHeight",   ns_element_attr_getter, ns_element_attr_setter, 122),
    JS_CGETSET_MAGIC_DEF("longDesc",       ns_element_attr_getter, ns_element_attr_setter, 123),
    JS_CGETSET_MAGIC_DEF("lowsrc",         ns_element_attr_getter, ns_element_attr_setter, 124),
    JS_CGETSET_MAGIC_DEF("version",        ns_element_attr_getter, ns_element_attr_setter, 125),
    JS_CGETSET_MAGIC_DEF("event",          ns_element_attr_getter, ns_element_attr_setter, 126),
    JS_CGETSET_MAGIC_DEF("valueType",      ns_element_attr_getter, ns_element_attr_setter, 127),
    JS_CGETSET_MAGIC_DEF("srclang",        ns_element_attr_getter, ns_element_attr_setter, 128),
    JS_CGETSET_MAGIC_DEF("dirName",        ns_element_attr_getter, ns_element_attr_setter, 129),
    JS_CGETSET_MAGIC_DEF("httpEquiv",      ns_element_attr_getter, ns_element_attr_setter, 49),
    JS_CGETSET_DEF("contentEditable", ns_element_get_contentEditable, ns_element_set_contentEditable),
    JS_CGETSET_MAGIC_DEF("slot",           ns_element_attr_getter, ns_element_attr_setter, 51),
    JS_CGETSET_MAGIC_DEF("role", ns_element_aria_string_getter, ns_element_aria_string_setter, 0),
    JS_CGETSET_MAGIC_DEF("ariaLabel", ns_element_aria_string_getter, ns_element_aria_string_setter, 1),
    JS_CGETSET_MAGIC_DEF("ariaBrailleLabel", ns_element_aria_string_getter, ns_element_aria_string_setter, 2),
    JS_CGETSET_MAGIC_DEF("ariaBrailleRoleDescription", ns_element_aria_string_getter, ns_element_aria_string_setter, 3),
    JS_CGETSET_MAGIC_DEF("ariaColCount", ns_element_aria_string_getter, ns_element_aria_string_setter, 4),
    JS_CGETSET_MAGIC_DEF("ariaColIndex", ns_element_aria_string_getter, ns_element_aria_string_setter, 5),
    JS_CGETSET_MAGIC_DEF("ariaColIndexText", ns_element_aria_string_getter, ns_element_aria_string_setter, 6),
    JS_CGETSET_MAGIC_DEF("ariaColSpan", ns_element_aria_string_getter, ns_element_aria_string_setter, 7),
    JS_CGETSET_MAGIC_DEF("ariaDescription", ns_element_aria_string_getter, ns_element_aria_string_setter, 8),
    JS_CGETSET_MAGIC_DEF("ariaKeyShortcuts", ns_element_aria_string_getter, ns_element_aria_string_setter, 9),
    JS_CGETSET_MAGIC_DEF("ariaLevel", ns_element_aria_string_getter, ns_element_aria_string_setter, 10),
    JS_CGETSET_MAGIC_DEF("ariaPlaceholder", ns_element_aria_string_getter, ns_element_aria_string_setter, 11),
    JS_CGETSET_MAGIC_DEF("ariaPosInSet", ns_element_aria_string_getter, ns_element_aria_string_setter, 12),
    JS_CGETSET_MAGIC_DEF("ariaRelevant", ns_element_aria_string_getter, ns_element_aria_string_setter, 13),
    JS_CGETSET_MAGIC_DEF("ariaRoleDescription", ns_element_aria_string_getter, ns_element_aria_string_setter, 14),
    JS_CGETSET_MAGIC_DEF("ariaRowCount", ns_element_aria_string_getter, ns_element_aria_string_setter, 15),
    JS_CGETSET_MAGIC_DEF("ariaRowIndex", ns_element_aria_string_getter, ns_element_aria_string_setter, 16),
    JS_CGETSET_MAGIC_DEF("ariaRowIndexText", ns_element_aria_string_getter, ns_element_aria_string_setter, 17),
    JS_CGETSET_MAGIC_DEF("ariaRowSpan", ns_element_aria_string_getter, ns_element_aria_string_setter, 18),
    JS_CGETSET_MAGIC_DEF("ariaSetSize", ns_element_aria_string_getter, ns_element_aria_string_setter, 19),
    JS_CGETSET_MAGIC_DEF("ariaValueMax", ns_element_aria_string_getter, ns_element_aria_string_setter, 20),
    JS_CGETSET_MAGIC_DEF("ariaValueMin", ns_element_aria_string_getter, ns_element_aria_string_setter, 21),
    JS_CGETSET_MAGIC_DEF("ariaValueNow", ns_element_aria_string_getter, ns_element_aria_string_setter, 22),
    JS_CGETSET_MAGIC_DEF("ariaValueText", ns_element_aria_string_getter, ns_element_aria_string_setter, 23),
    JS_CGETSET_MAGIC_DEF("ariaHidden",     ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_HIDDEN),
    JS_CGETSET_MAGIC_DEF("ariaDisabled",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_DISABLED),
    JS_CGETSET_MAGIC_DEF("ariaPressed",    ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_PRESSED),
    JS_CGETSET_MAGIC_DEF("ariaExpanded",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_EXPANDED),
    JS_CGETSET_MAGIC_DEF("ariaControls",   ns_element_attr_getter, ns_element_attr_setter, 59),
    JS_CGETSET_MAGIC_DEF("ariaDescribedBy", ns_element_attr_getter, ns_element_attr_setter, 60),
    JS_CGETSET_MAGIC_DEF("ariaLabelledBy", ns_element_attr_getter, ns_element_attr_setter, 61),
    JS_CGETSET_MAGIC_DEF("ariaLive",       ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_LIVE),
    JS_CGETSET_MAGIC_DEF("ariaBusy",       ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_BUSY),
    JS_CGETSET_MAGIC_DEF("ariaChecked",    ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_CHECKED),
    JS_CGETSET_MAGIC_DEF("ariaCurrent",    ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_CURRENT),
    JS_CGETSET_MAGIC_DEF("ariaSelected",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_SELECTED),
    JS_CGETSET_MAGIC_DEF("ariaAtomic",     ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_ATOMIC),
    JS_CGETSET_MAGIC_DEF("ariaAutoComplete", ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_AUTOCOMPLETE),
    JS_CGETSET_MAGIC_DEF("ariaHasPopup",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_HASPOPUP),
    JS_CGETSET_MAGIC_DEF("ariaInvalid",    ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_INVALID),
    JS_CGETSET_MAGIC_DEF("ariaModal",      ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_MODAL),
    JS_CGETSET_MAGIC_DEF("ariaMultiLine",  ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_MULTILINE),
    JS_CGETSET_MAGIC_DEF("ariaMultiSelectable", ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_MULTISELECTABLE),
    JS_CGETSET_MAGIC_DEF("ariaOrientation", ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_ORIENTATION),
    JS_CGETSET_MAGIC_DEF("ariaReadOnly",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_READONLY),
    JS_CGETSET_MAGIC_DEF("ariaRequired",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_REQUIRED),
    JS_CGETSET_MAGIC_DEF("ariaSort",       ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ARIA_SORT),
    JS_CGETSET_MAGIC_DEF("nonce",          ns_element_attr_getter, ns_element_attr_setter, 72),
    JS_CGETSET_MAGIC_DEF("accessKey",      ns_element_attr_getter, ns_element_attr_setter, 73),
    JS_CGETSET_MAGIC_DEF("dateTime",       ns_element_attr_getter, ns_element_attr_setter, 74),
    JS_CGETSET_MAGIC_DEF("srcdoc",         ns_element_attr_getter, ns_element_attr_setter, 75),
    JS_CGETSET_MAGIC_DEF("popoverTargetAction", ns_element_attr_getter, ns_element_attr_setter, 77),
    JS_CGETSET_DEF("autocapitalize", ns_element_get_autocapitalize, ns_element_set_autocapitalize),
    JS_CGETSET_MAGIC_DEF("enterKeyHint",   ns_element_enum_getter, ns_element_enum_setter, NS_ENUM_ENTERKEYHINT),
    JS_CGETSET_MAGIC_DEF("charSet",        ns_element_attr_getter, ns_element_attr_setter, 47),
    JS_CGETSET_MAGIC_DEF("poster",         ns_element_attr_getter, ns_element_attr_setter, 83),
    JS_CGETSET_MAGIC_DEF("preload",        ns_element_attr_getter, ns_element_attr_setter, 84),
    JS_CGETSET_MAGIC_DEF("wrap",           ns_element_attr_getter, ns_element_attr_setter, 85),
    JS_CGETSET_MAGIC_DEF("scope",          ns_element_attr_getter, ns_element_attr_setter, 86),
    JS_CGETSET_MAGIC_DEF("cite",           ns_element_attr_getter, ns_element_attr_setter, 87),
    JS_CGETSET_MAGIC_DEF("media",          ns_element_attr_getter, ns_element_attr_setter, 88),
    JS_CGETSET_MAGIC_DEF("download",       ns_element_attr_getter, ns_element_attr_setter, 89),
    JS_CGETSET_MAGIC_DEF("open",        ns_element_boolattr_getter, ns_element_boolattr_setter,  0),
    JS_CGETSET_DEF("selected",          ns_element_get_selected,    ns_element_set_selected),
    JS_CGETSET_MAGIC_DEF("multiple",    ns_element_boolattr_getter, ns_element_boolattr_setter,  2),
    JS_CGETSET_MAGIC_DEF("readOnly",    ns_element_boolattr_getter, ns_element_boolattr_setter,  3),
    JS_CGETSET_MAGIC_DEF("autofocus",   ns_element_boolattr_getter, ns_element_boolattr_setter,  4),
    JS_CGETSET_MAGIC_DEF("controls",    ns_element_boolattr_getter, ns_element_boolattr_setter,  5),
    JS_CGETSET_MAGIC_DEF("loop",        ns_element_boolattr_getter, ns_element_boolattr_setter,  6),
    JS_CGETSET_MAGIC_DEF("autoplay",    ns_element_boolattr_getter, ns_element_boolattr_setter,  8),
    JS_CGETSET_MAGIC_DEF("defer",       ns_element_boolattr_getter, ns_element_boolattr_setter,  9),
    JS_CGETSET_MAGIC_DEF("async",       ns_element_boolattr_getter, ns_element_boolattr_setter, 10),
    JS_CGETSET_MAGIC_DEF("noValidate",  ns_element_boolattr_getter, ns_element_boolattr_setter, 11),
    JS_CGETSET_MAGIC_DEF("isMap",       ns_element_boolattr_getter, ns_element_boolattr_setter, 12),
    JS_CGETSET_DEF("draggable",   ns_element_get_draggable, ns_element_set_draggable),
    JS_CGETSET_MAGIC_DEF("reversed",    ns_element_boolattr_getter, ns_element_boolattr_setter, 14),
    JS_CGETSET_MAGIC_DEF("playsInline", ns_element_boolattr_getter, ns_element_boolattr_setter, 15),
    JS_CGETSET_MAGIC_DEF("inert",       ns_element_boolattr_getter, ns_element_boolattr_setter, 17),
    JS_CGETSET_MAGIC_DEF("noModule",    ns_element_boolattr_getter, ns_element_boolattr_setter, 18),
    JS_CGETSET_MAGIC_DEF("formNoValidate", ns_element_boolattr_getter, ns_element_boolattr_setter, 19),
    JS_CGETSET_MAGIC_DEF("required",    ns_element_boolattr_getter, ns_element_boolattr_setter, 20),
    JS_CGETSET_DEF("htmlFor",                ns_element_get_htmlFor, ns_element_set_htmlFor),
    JS_CGETSET_DEF("control",                ns_element_get_label_control, ns_element_noop_set),
    JS_CGETSET_DEF("popoverTargetElement",   ns_element_get_popoverTargetElement, ns_element_set_popoverTargetElement),
    JS_CGETSET_DEF("tabIndex",               ns_element_get_tabIndex, ns_element_set_tabIndex),
    JS_CGETSET_DEF("isConnected",            ns_element_get_isConnected,    ns_element_noop_set),
    JS_CGETSET_DEF("baseURI",                ns_element_get_baseURI,        ns_element_noop_set),
    JS_CGETSET_DEF("ownerDocument",          ns_element_get_ownerDocument,  ns_element_noop_set),
    JS_CGETSET_DEF("namespaceURI",           ns_element_get_namespaceURI,   ns_element_noop_set),
    JS_CGETSET_DEF("shadowRoot",             ns_element_get_shadowRoot,     ns_element_noop_set),
    JS_CGETSET_DEF("assignedSlot",           ns_element_get_assignedSlot,   ns_element_noop_set),
    JS_CFUNC_DEF("attachShadow",             1, ns_element_attachShadow),
    JS_CGETSET_DEF("disabled",      ns_element_get_disabled,   ns_element_set_disabled),
    JS_CGETSET_DEF("checked",       ns_element_get_checked,    ns_element_set_checked),
    JS_CGETSET_DEF("label",         ns_element_get_label_prop, ns_element_set_label_prop),
    JS_CGETSET_DEF("selectedIndex", ns_element_get_selectedIndex, ns_element_set_selectedIndex),
    JS_CGETSET_DEF("options",       ns_element_get_options,       ns_element_noop_set),
    JS_CGETSET_DEF("selectedOptions", ns_element_get_selectedOptions, ns_element_noop_set),
    JS_CGETSET_DEF("elements",      ns_element_get_form_elements, ns_element_noop_set),
    JS_CGETSET_DEF("form",          ns_element_get_form,          ns_element_noop_set),
};

static const JSCFunctionListEntry ns_src_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("src", ns_element_attr_getter,
                         ns_element_attr_setter, 3),
};

static const JSCFunctionListEntry ns_image_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("src", ns_element_attr_getter,
                         ns_element_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("srcset", ns_element_attr_getter,
                         ns_element_attr_setter, 29),
    JS_CGETSET_DEF("currentSrc", ns_element_img_current_src,
                   ns_element_noop_set),
    JS_CGETSET_DEF("naturalWidth", ns_element_img_natural_width,
                   ns_element_noop_set),
    JS_CGETSET_DEF("naturalHeight", ns_element_img_natural_height,
                   ns_element_noop_set),
    JS_CGETSET_DEF("complete", ns_element_img_complete,
                   ns_element_noop_set),
};

static const JSCFunctionListEntry ns_source_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("src", ns_element_attr_getter,
                         ns_element_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("srcset", ns_element_attr_getter,
                         ns_element_attr_setter, 29),
};

static const JSCFunctionListEntry ns_media_proto_funcs[] = {
    JS_CFUNC_DEF("play", 0, ns_media_play),
    JS_CFUNC_DEF("pause", 0, ns_media_pause),
    JS_CFUNC_DEF("load", 0, ns_media_load),
    JS_CFUNC_DEF("canPlayType", 1, ns_media_canPlayType),
    JS_CFUNC_DEF("fastSeek", 1, ns_media_fast_seek),
    JS_CGETSET_MAGIC_DEF("src", ns_element_attr_getter,
                         ns_element_attr_setter, 3),
    JS_CGETSET_DEF("currentSrc", ns_element_img_current_src,
                   ns_element_noop_set),
    JS_CGETSET_DEF("currentTime", ns_media_get_current_time,
                   ns_media_set_current_time),
    JS_CGETSET_DEF("duration", ns_media_get_duration, ns_element_noop_set),
    JS_CGETSET_DEF("paused", ns_media_get_paused, ns_element_noop_set),
    JS_CGETSET_DEF("ended", ns_media_get_ended, ns_element_noop_set),
    JS_CGETSET_DEF("seeking", ns_media_get_seeking, ns_element_noop_set),
    JS_CGETSET_DEF("volume", ns_media_get_volume, ns_media_set_volume),
    JS_CGETSET_DEF("muted", ns_media_get_muted, ns_media_set_muted),
    JS_CGETSET_DEF("playbackRate", ns_media_get_playbackRate,
                   ns_media_set_playbackRate),
    JS_CGETSET_DEF("defaultPlaybackRate", ns_media_get_defaultPlaybackRate,
                   ns_media_set_defaultPlaybackRate),
    JS_CGETSET_DEF("readyState", ns_media_get_readyState, ns_element_noop_set),
    JS_CGETSET_DEF("networkState", ns_media_get_networkState,
                   ns_element_noop_set),
    JS_CGETSET_DEF("buffered", ns_media_get_buffered_ranges,
                   ns_element_noop_set),
    JS_CGETSET_DEF("played", ns_media_get_played_ranges,
                   ns_element_noop_set),
    JS_CGETSET_DEF("seekable", ns_media_get_seekable_ranges,
                   ns_element_noop_set),
};

static const JSCFunctionListEntry ns_video_proto_funcs[] = {
    JS_CFUNC_DEF("getVideoPlaybackQuality", 0,
                 ns_media_get_video_playback_quality),
    JS_CFUNC_DEF("requestVideoFrameCallback", 1,
                 ns_media_request_video_frame_callback),
    JS_CFUNC_DEF("cancelVideoFrameCallback", 1,
                 ns_window_cancelAnimationFrame),
    JS_CGETSET_DEF("videoWidth", ns_element_get_zero_int,
                   ns_element_noop_set),
    JS_CGETSET_DEF("videoHeight", ns_element_get_zero_int,
                   ns_element_noop_set),
};

static const JSCFunctionListEntry ns_iframe_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("src", ns_element_attr_getter,
                         ns_element_attr_setter, 3),
    JS_CGETSET_MAGIC_DEF("srcdoc", ns_element_attr_getter,
                         ns_element_attr_setter, 75),
};

static const JSCFunctionListEntry ns_anchor_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("href", ns_element_anchor_part_get,
                         ns_element_anchor_href_set, NS_ANCHOR_HREF),
    JS_CGETSET_MAGIC_DEF("ping", ns_element_attr_getter,
                         ns_element_attr_setter, 90),
    JS_CGETSET_MAGIC_DEF("download", ns_element_attr_getter,
                         ns_element_attr_setter, 89),
};

static const JSCFunctionListEntry ns_href_proto_funcs[] = {
    JS_CGETSET_MAGIC_DEF("href", ns_element_anchor_part_get,
                         ns_element_anchor_href_set, NS_ANCHOR_HREF),
};

ns_node *
ns_document_root_for(JSContext *ctx, JSValueConst this_val)
{
    ns_node *root = ns_unwrap_element_mut(this_val);
    if (root && root->kind == NS_NODE_DOCUMENT) return root;
    ns_js *js = js_from_ctx(ctx);
    return js ? js->current_doc : NULL;
}

static JSValue
ns_document_get_forms(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_live(ctx, this_val, NS_LIVE_DOC_TAG, "form");
}

static JSValue
ns_document_get_images(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_live(ctx, this_val, NS_LIVE_DOC_TAG, "img");
}

static JSValue
ns_document_get_all(JSContext *ctx, JSValueConst this_val)
{
    JSValue all = ns_make_live(ctx, this_val, NS_LIVE_DOC_TAG, "*");
    JS_SetIsHTMLDDA(ctx, all);
    return all;
}

static gboolean
ns_point_in_hit_bounds(ns_js *js, double x, double y)
{
    double w = ns_css_viewport_w();
    double h = ns_css_viewport_h();
    if (js->layout_root) {
        if (js->layout_root->content_width  > w) w = js->layout_root->content_width;
        if (js->layout_root->content_height > h) h = js->layout_root->content_height;
    }
    return x <= w && y <= h;
}

static gboolean
ns_hit_point_usable(double x, double y)
{
    return isfinite(x) && isfinite(y) && x >= 0 && y >= 0;
}

static ns_js *
ns_hit_layout_js(JSContext *ctx)
{
    ns_js *js = js_from_ctx(ctx);
    if (!js || !js->current_doc) return NULL;
    ns_js_flush_layout(js);
    return js->layout_root ? js : NULL;
}

static const ns_node *
ns_hit_frame_of(const ns_node *doc)
{
    return doc && doc->kind == NS_NODE_DOCUMENT &&
        ns_node_is_element_named(doc->parent, "iframe") ? doc->parent : NULL;
}

static gboolean
ns_hit_frame_point(ns_js *js, const ns_node *frame, double *x, double *y)
{
    const ns_box *fb = ns_box_find_by_dom(js->layout_root, frame);
    if (!fb) return FALSE;
    double fx, fy, fw, fh;
    ns_box_visual_border_box(fb, &fx, &fy, &fw, &fh);
    double cw = fw - fb->border.left - fb->border.right -
                fb->padding.left - fb->padding.right;
    double ch = fh - fb->border.top - fb->border.bottom -
                fb->padding.top - fb->padding.bottom;
    if (*x >= cw || *y >= ch) return FALSE;
    *x += fx + fb->border.left + fb->padding.left;
    *y += fy + fb->border.top + fb->padding.top;
    return TRUE;
}

static const ns_node *
ns_hit_layout_point(JSContext *ctx, ns_js *js, const ns_node *doc, double *x, double *y)
{
    const ns_node *frame = ns_hit_frame_of(doc);
    if (frame) return ns_hit_frame_point(js, frame, x, y) ? doc : NULL;
    if (!ns_point_in_hit_bounds(js, *x, *y)) return NULL;
    *x += ns_window_scroll_prop(ctx, "scrollX");
    *y += ns_window_scroll_prop(ctx, "scrollY");
    return js->current_doc;
}

static const ns_node *
ns_hit_owner_node(const ns_box *hit, const ns_node *doc)
{
    /* Content of a nested document is not part of this document's hit
     * test: a point over a frame hits the frame element. */
    const ns_node *node = hit->dom;
    const ns_node *p = hit->dom;
    for (; p && p != doc; p = p->parent)
        if (p->kind == NS_NODE_DOCUMENT && p->parent)
            node = p->parent;
    return p ? node : NULL;
}

/* The node a point hits in the document this_val is, or NULL: the point is
 * in that document's viewport coordinates, and for a frame's document it is
 * translated into the frame's content box first. local_x and local_y get the
 * point inside the hit box. */
static const ns_node *
ns_document_hit_node(JSContext *ctx, JSValueConst this_val, double x, double y,
                     const ns_node **doc_out, const ns_box **box_out,
                     double *local_x, double *local_y)
{
    *doc_out = NULL;
    *box_out = NULL;
    if (!ns_hit_point_usable(x, y)) return NULL;
    ns_js *js = ns_hit_layout_js(ctx);
    if (!js) return NULL;
    const ns_node *doc = ns_hit_layout_point(ctx, js, ns_unwrap_element(this_val), &x, &y);
    if (!doc) return NULL;
    const ns_box *hit = ns_box_hit_test_local(js->layout_root, x, y,
                                              local_x, local_y);
    if (!hit || !hit->dom) return NULL;
    const ns_node *node = ns_hit_owner_node(hit, doc);
    if (!node) return NULL;
    *doc_out = doc;
    *box_out = node == hit->dom ? hit : NULL;
    return node;
}

JSValue
ns_document_element_from_point(JSContext *ctx, JSValueConst this_val,
                               int argc, JSValueConst *argv)
{
    if (argc < 2) return JS_NULL;
    double x = 0, y = 0;
    JS_ToFloat64(ctx, &x, argv[0]);
    JS_ToFloat64(ctx, &y, argv[1]);
    const ns_node *doc = NULL;
    const ns_box *box = NULL;
    double local_x = 0, local_y = 0;
    const ns_node *node = ns_document_hit_node(ctx, this_val, x, y, &doc, &box,
                                               &local_x, &local_y);
    if (!node) return JS_NULL;
    const ns_node *area = box ? ns_box_image_map_area(box, local_x, local_y)
                              : NULL;
    return ns_make_element(ctx, area ? area : node);
}

JSValue
ns_document_elements_from_point(JSContext *ctx, JSValueConst this_val,
                                int argc, JSValueConst *argv)
{
    JSValue arr = JS_NewArray(ctx);
    if (argc < 2) return arr;
    double x = 0, y = 0;
    JS_ToFloat64(ctx, &x, argv[0]);
    JS_ToFloat64(ctx, &y, argv[1]);
    const ns_node *doc = NULL;
    const ns_box *box = NULL;
    double local_x = 0, local_y = 0;
    const ns_node *node = ns_document_hit_node(ctx, this_val, x, y, &doc, &box,
                                               &local_x, &local_y);
    uint32_t i = 0;
    for (const ns_node *n = node; n && n != doc; n = n->parent)
        if (n->kind == NS_NODE_ELEMENT)
            JS_SetPropertyUint32(ctx, arr, i++, ns_make_element(ctx, n));
    return arr;
}

static void
ns_set_tostring_tag(JSContext *ctx, JSValueConst obj, const char *tag)
{
    JSValue g = JS_GetGlobalObject(ctx);
    JSValue sym = JS_GetPropertyStr(ctx, g, "Symbol");
    JSValue tag_sym = JS_GetPropertyStr(ctx, sym, "toStringTag");
    JSAtom tag_atom = JS_ValueToAtom(ctx, tag_sym);
    if (tag_atom != JS_ATOM_NULL) {
        JS_DefinePropertyValue(ctx, obj, tag_atom, JS_NewString(ctx, tag),
            JS_PROP_CONFIGURABLE);
        JS_FreeAtom(ctx, tag_atom);
    }
    JS_FreeValue(ctx, tag_sym);
    JS_FreeValue(ctx, sym);
    JS_FreeValue(ctx, g);
}

static const JSCFunctionListEntry ns_namedmap_proto_funcs[] = {
    JS_CFUNC_DEF("getNamedItem",      1, ns_namedmap_getNamedItem),
    JS_CFUNC_DEF("getNamedItemNS",    2, ns_namedmap_getNamedItemNS),
    JS_CFUNC_DEF("setNamedItem",      1, ns_namedmap_setNamedItem),
    JS_CFUNC_DEF("setNamedItemNS",    1, ns_namedmap_setNamedItem),
    JS_CFUNC_DEF("removeNamedItem",   1, ns_namedmap_removeNamedItem),
    JS_CFUNC_DEF("removeNamedItemNS", 2, ns_namedmap_removeNamedItemNS),
    JS_CFUNC_DEF("item",              1, ns_namedmap_item),
    JS_CGETSET_DEF("length", ns_namedmap_get_length, ns_element_noop_set),
};

static const JSCFunctionListEntry ns_dom_implementation_proto_funcs[] = {
    JS_CFUNC_DEF("hasFeature",         2, ns_event_true),
    JS_CFUNC_DEF("createHTMLDocument", 1, ns_impl_create_html_document),
    JS_CFUNC_DEF("createDocument",     3, ns_impl_create_document),
    JS_CFUNC_DEF("createDocumentType", 3, ns_impl_create_document_type),
};

static JSValue
ns_document_get_links(JSContext *ctx, JSValueConst this_val)
{
    return ns_make_live(ctx, this_val, NS_LIVE_LINKS, NULL);
}

static JSValue
ns_js_engine_name_js(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return JS_NewString(ctx, "quickjs");
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
    if (js && js->orphan_nodes) g_hash_table_add(js->orphan_nodes, node);
    JSValue elem = ns_make_element(ctx, node);
    ns_tag_caller_document(ctx, elem);
    return elem;
}

static void
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

void
ns_js_note_viewport_scroll(ns_js *js, double x, double y)
{
    if (!js || !js->ctx || js->halted) return;
    JSContext *ctx = js->ctx;
    if (ns_window_scroll_prop(ctx, "scrollX") != x ||
        ns_window_scroll_prop(ctx, "scrollY") != y)
        js->pending_scrollend_doc = TRUE;
    ns_box_set_hit_viewport(x, y);
    JSValue global = JS_GetGlobalObject(ctx);
    JS_SetPropertyStr(ctx, global, "scrollX", JS_NewFloat64(ctx, x));
    JS_SetPropertyStr(ctx, global, "scrollY", JS_NewFloat64(ctx, y));
    JS_SetPropertyStr(ctx, global, "pageXOffset", JS_NewFloat64(ctx, x));
    JS_SetPropertyStr(ctx, global, "pageYOffset", JS_NewFloat64(ctx, y));
    if (js->in_scroll_dispatch) {
        JS_FreeValue(ctx, global);
        return;
    }
    js->in_scroll_dispatch = TRUE;
    /* Viewport scrolling fires one scroll event at the document, which
     * bubbles to the window. */
    if (js->current_doc) {
        ns_js_dispatch_event(js, js->current_doc, "scroll", NULL);
    } else {
        JSValue ev = ns_make_window_event(ctx, "scroll");
        ns_js_dispatch_window_only_event(js, NULL, "scroll", ev, NULL);
    }
    JS_FreeValue(ctx, global);
    js->in_scroll_dispatch = FALSE;
    ns_observer_schedule_tick(js);
}

void
ns_js_reeval_media_queries(ns_js *js)
{
    ns_services_reeval_media_queries(js);
}

void
ns_js_dispatch_resize(ns_js *js)
{
    if (!js || !js->ctx) return;
    JSContext *ctx = js->ctx;
    JSValue ev = ns_make_window_event(ctx, "resize");
    ns_js_dispatch_window_only_event(js, js->current_doc, "resize", ev, NULL);
    ns_services_reeval_media_queries(js);
}

void
ns_js_fire_page_transition(ns_js *js, const char *type, gboolean persisted)
{
    if (!js || !js->ctx || !type) return;
    JSContext *ctx = js->ctx;
    /* Fired at the window, but with the document as the event's target. */
    JSValue ev = ns_make_event(ctx, type, js->current_doc);
    JS_SetPropertyStr(ctx, ev, "persisted", persisted ? JS_TRUE : JS_FALSE);
    ns_js_dispatch_window_only_event(js, js->current_doc, type, ev, NULL);
}

JSValue
ns_illegal_constructor(JSContext *ctx, JSValueConst this_val,
                       int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    return JS_ThrowTypeError(ctx, "Illegal constructor");
}

static gboolean
ns_value_nodetype_in(JSContext *ctx, JSValueConst v, int32_t a, int32_t b)
{
    JSValue nt = JS_GetPropertyStr(ctx, v, "nodeType");
    int32_t t = 0;
    JS_ToInt32(ctx, &t, nt);
    JS_FreeValue(ctx, nt);
    return t == a || t == b;
}

static JSValue
ns_static_range_ctor(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 1 || !JS_IsObject(argv[0]))
        return JS_ThrowTypeError(ctx,
            "Failed to construct 'StaticRange': 1 argument required");
    JSValue sc = JS_GetPropertyStr(ctx, argv[0], "startContainer");
    JSValue ec = JS_GetPropertyStr(ctx, argv[0], "endContainer");
    JSValue sov = JS_GetPropertyStr(ctx, argv[0], "startOffset");
    JSValue eov = JS_GetPropertyStr(ctx, argv[0], "endOffset");
    if (!JS_IsObject(sc) || !JS_IsObject(ec) ||
        JS_IsUndefined(sov) || JS_IsUndefined(eov)) {
        JS_FreeValue(ctx, sc); JS_FreeValue(ctx, ec);
        JS_FreeValue(ctx, sov); JS_FreeValue(ctx, eov);
        return JS_ThrowTypeError(ctx,
            "Failed to construct 'StaticRange': a required member is undefined");
    }
    if (ns_value_nodetype_in(ctx, sc, 10, 2) ||
        ns_value_nodetype_in(ctx, ec, 10, 2)) {
        JS_FreeValue(ctx, sc); JS_FreeValue(ctx, ec);
        JS_FreeValue(ctx, sov); JS_FreeValue(ctx, eov);
        return ns_throw_dom_exception(ctx, "InvalidNodeTypeError", 24,
            "StaticRange boundary must not be a DocumentType or Attr node");
    }
    int32_t so = 0, eo = 0;
    JS_ToInt32(ctx, &so, sov); JS_FreeValue(ctx, sov);
    JS_ToInt32(ctx, &eo, eov); JS_FreeValue(ctx, eov);
    gboolean collapsed = (so == eo) &&
        JS_VALUE_GET_PTR(sc) == JS_VALUE_GET_PTR(ec) && JS_IsObject(sc);
    JSValue r = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, r, "startContainer", sc);
    JS_SetPropertyStr(ctx, r, "startOffset",    JS_NewInt32(ctx, so));
    JS_SetPropertyStr(ctx, r, "endContainer",   ec);
    JS_SetPropertyStr(ctx, r, "endOffset",      JS_NewInt32(ctx, eo));
    JS_SetPropertyStr(ctx, r, "collapsed",      JS_NewBool(ctx, collapsed));
    return r;
}

static gboolean
ns_global_has(JSContext *ctx, JSValueConst global, const char *name)
{
    JSAtom atom = JS_NewAtom(ctx, name);
    int has = JS_HasProperty(ctx, global, atom);
    JS_FreeAtom(ctx, atom);
    return has > 0;
}

static void
ns_set_if_missing(JSContext *ctx, JSValueConst global,
                  const char *name, JSValue value)
{
    if (ns_global_has(ctx, global, name))
        JS_FreeValue(ctx, value);
    else
        JS_SetPropertyStr(ctx, global, name, value);
}

/* No document or worker here is cross-origin isolated (crossOriginIsolated
 * is always false), and the HTML standard removes SharedArrayBuffer from
 * the global object of every realm that is not. Shared memory a
 * WebAssembly.Memory creates still works. */
static void
ns_hide_shared_array_buffer(JSContext *ctx, JSValueConst global)
{
    JSAtom atom = JS_NewAtom(ctx, "SharedArrayBuffer");
    if (JS_DeleteProperty(ctx, global, atom, 0) < 0)
        JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeAtom(ctx, atom);
}

static void
ns_install_window_compat(JSContext *ctx, JSValueConst global)
{
    {
        static const struct { const char *n; int v; } svglen_consts[] = {
            { "SVG_LENGTHTYPE_UNKNOWN",    0 }, { "SVG_LENGTHTYPE_NUMBER",     1 },
            { "SVG_LENGTHTYPE_PERCENTAGE", 2 }, { "SVG_LENGTHTYPE_EMS",        3 },
            { "SVG_LENGTHTYPE_EXS",        4 }, { "SVG_LENGTHTYPE_PX",         5 },
            { "SVG_LENGTHTYPE_CM",         6 }, { "SVG_LENGTHTYPE_MM",         7 },
            { "SVG_LENGTHTYPE_IN",         8 }, { "SVG_LENGTHTYPE_PT",         9 },
            { "SVG_LENGTHTYPE_PC",        10 },
        };
        JSValue svglen = JS_GetPropertyStr(ctx, global, "SVGLength");
        if (JS_IsObject(svglen)) {
            JSValue proto = JS_GetPropertyStr(ctx, svglen, "prototype");
            for (gsize i = 0; i < G_N_ELEMENTS(svglen_consts); i++) {
                JS_SetPropertyStr(ctx, svglen, svglen_consts[i].n,
                                  JS_NewInt32(ctx, svglen_consts[i].v));
                if (JS_IsObject(proto))
                    JS_SetPropertyStr(ctx, proto, svglen_consts[i].n,
                                      JS_NewInt32(ctx, svglen_consts[i].v));
            }
            JS_FreeValue(ctx, proto);
        }
        JS_FreeValue(ctx, svglen);
    }

    {
        static const struct { const char *name; int value; } constants[] = {
            { "ANY_TYPE", 0 }, { "NUMBER_TYPE", 1 }, { "STRING_TYPE", 2 },
            { "BOOLEAN_TYPE", 3 }, { "UNORDERED_NODE_ITERATOR_TYPE", 4 },
            { "ORDERED_NODE_ITERATOR_TYPE", 5 },
            { "UNORDERED_NODE_SNAPSHOT_TYPE", 6 },
            { "ORDERED_NODE_SNAPSHOT_TYPE", 7 },
            { "ANY_UNORDERED_NODE_TYPE", 8 },
            { "FIRST_ORDERED_NODE_TYPE", 9 },
        };
        JSValue ctor = JS_GetPropertyStr(ctx, global, "XPathResult");
        JSValue proto = JS_IsObject(ctor)
            ? JS_GetPropertyStr(ctx, ctor, "prototype") : JS_UNDEFINED;
        if (JS_IsObject(proto)) {
            ns_bind_fn(ctx, proto, "iterateNext", ns_returns_null, 0);
            ns_bind_fn(ctx, proto, "snapshotItem", ns_returns_null, 1);
            for (gsize i = 0; i < G_N_ELEMENTS(constants); i++) {
                JS_DefinePropertyValueStr(ctx, ctor, constants[i].name,
                    JS_NewInt32(ctx, constants[i].value), 0);
                JS_DefinePropertyValueStr(ctx, proto, constants[i].name,
                    JS_NewInt32(ctx, constants[i].value), 0);
            }
        }
        JS_FreeValue(ctx, proto);
        JS_FreeValue(ctx, ctor);
    }

    ns_install_event_handler_props(ctx, global);

    static const char *const bars[] = {
        "locationbar", "menubar", "personalbar",
        "scrollbars", "statusbar", "toolbar",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(bars); i++) {
        JSValue bar = JS_NewObject(ctx);
        JS_SetPropertyStr(ctx, bar, "visible", JS_TRUE);
        ns_set_if_missing(ctx, global, bars[i], bar);
    }

    ns_set_if_missing(ctx, global, "crossOriginIsolated", JS_FALSE);
    ns_hide_shared_array_buffer(ctx, global);
    ns_set_if_missing(ctx, global, "frameElement", JS_NULL);
    ns_js_intl_install(ctx, global);
    ns_js_temporal_install(ctx, global);
    ns_js_realm_install(ctx, global);

    ns_bind_fn_if_missing(ctx, global, "captureEvents", ns_event_noop, 0);
    ns_bind_fn_if_missing(ctx, global, "releaseEvents", ns_event_noop, 0);

    JSValue external = JS_NewObject(ctx);
    ns_set_if_missing(ctx, global, "external", external);

    JSValue navigator = JS_GetPropertyStr(ctx, global, "navigator");
    ns_set_if_missing(ctx, global, "clientInformation",
                      JS_DupValue(ctx, navigator));
    JS_FreeValue(ctx, navigator);

    JSValue url_ctor = JS_GetPropertyStr(ctx, global, "URL");
    JS_DefinePropertyValueStr(ctx, global, "webkitURL", JS_DupValue(ctx, url_ctor),
                              JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    JS_FreeValue(ctx, url_ctor);

}

typedef struct {
    z_stream  zs;
    gboolean  decompress;
    gboolean  inited;
    gboolean  ended;
} ns_zlib_codec;

static JSClassID ns_zlib_class_id;

#define NS_ZLIB_MAX_OUTPUT ((gsize)256u * 1024u * 1024u)

static void
ns_zlib_finalizer(JSRuntime *rt, JSValue val)
{
    (void)rt;
    ns_zlib_codec *c = JS_GetOpaque(val, ns_zlib_class_id);
    if (!c) return;
    if (c->inited) {
        if (c->decompress) inflateEnd(&c->zs);
        else               deflateEnd(&c->zs);
    }
    g_free(c);
}

static JSClassDef ns_zlib_class = {
    "NSZlibCodec",
    .finalizer = ns_zlib_finalizer,
};

static int
ns_zlib_window_bits(const char *format)
{
    if (!format) return 0;
    if (strcmp(format, "gzip") == 0)        return 15 + 16;
    if (strcmp(format, "deflate") == 0)     return 15;
    if (strcmp(format, "deflate-raw") == 0) return -15;
    return 0;
}

static JSValue
ns_zlib_create(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 2)
        return JS_ThrowTypeError(ctx, "zlib codec: format and mode required");
    const char *format = JS_ToCString(ctx, argv[0]);
    if (!format) return JS_EXCEPTION;
    int window_bits = ns_zlib_window_bits(format);
    JS_FreeCString(ctx, format);
    if (window_bits == 0)
        return JS_ThrowTypeError(ctx, "Unsupported compression format");

    gboolean decompress = JS_ToBool(ctx, argv[1]) > 0;
    ns_zlib_codec *c = g_new0(ns_zlib_codec, 1);
    c->decompress = decompress;
    int rc = decompress
        ? inflateInit2(&c->zs, window_bits)
        : deflateInit2(&c->zs, Z_DEFAULT_COMPRESSION, Z_DEFLATED,
                       window_bits, 8, Z_DEFAULT_STRATEGY);
    if (rc != Z_OK) {
        g_free(c);
        return JS_ThrowInternalError(ctx, "zlib initialization failed");
    }
    c->inited = TRUE;

    JSValue obj = JS_NewObjectClass(ctx, ns_zlib_class_id);
    if (JS_IsException(obj)) {
        if (decompress) inflateEnd(&c->zs);
        else            deflateEnd(&c->zs);
        g_free(c);
        return obj;
    }
    JS_SetOpaque(obj, c);
    return obj;
}

static JSValue
ns_zlib_run(JSContext *ctx, ns_zlib_codec *c, const uint8_t *in,
            size_t in_len, gboolean finish)
{
    if (!c || !c->inited)
        return JS_ThrowTypeError(ctx, "zlib codec: invalid state");
    if (c->ended)
        return JS_NewArrayBufferCopy(ctx, (const uint8_t *)"", 0);

    GByteArray *out = g_byte_array_new();
    uint8_t buf[16384];
    c->zs.next_in  = (Bytef *)in;
    c->zs.avail_in = (uInt)in_len;
    int flush = finish ? Z_FINISH : Z_NO_FLUSH;
    int rc;
    do {
        c->zs.next_out  = buf;
        c->zs.avail_out = sizeof(buf);
        rc = c->decompress ? inflate(&c->zs, flush) : deflate(&c->zs, flush);
        if (rc == Z_STREAM_ERROR || rc == Z_DATA_ERROR ||
            rc == Z_NEED_DICT || rc == Z_MEM_ERROR) {
            g_byte_array_free(out, TRUE);
            return JS_ThrowTypeError(ctx, "zlib %s error",
                                     c->decompress ? "decompression"
                                                   : "compression");
        }
        size_t produced = sizeof(buf) - c->zs.avail_out;
        if (produced > NS_ZLIB_MAX_OUTPUT - out->len) {
            g_byte_array_free(out, TRUE);
            return JS_ThrowRangeError(ctx, "zlib %s output too large",
                                      c->decompress ? "decompression"
                                                    : "compression");
        }
        if (produced) g_byte_array_append(out, buf, (guint)produced);
        if (rc == Z_STREAM_END) { c->ended = TRUE; break; }
        if (rc == Z_BUF_ERROR) break;
    } while (c->zs.avail_out == 0);

    JSValue ab = JS_NewArrayBufferCopy(ctx, out->data, out->len);
    g_byte_array_free(out, TRUE);
    return ab;
}

static ns_zlib_codec *
ns_zlib_unwrap(JSContext *ctx, JSValueConst v)
{
    (void)ctx;
    return ns_zlib_class_id ? JS_GetOpaque(v, ns_zlib_class_id) : NULL;
}

static JSValue
ns_zlib_push(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 2)
        return JS_ThrowTypeError(ctx, "zlib push: codec and data required");
    ns_zlib_codec *c = ns_zlib_unwrap(ctx, argv[0]);
    if (!c) return JS_ThrowTypeError(ctx, "zlib push: invalid codec");

    size_t off = 0, len = 0;
    JSValue buf = JS_GetArrayBufferViewBuffer(ctx, argv[1], &off, &len);
    if (JS_IsException(buf)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        size_t total = 0;
        uint8_t *base = JS_GetArrayBuffer(ctx, &total, argv[1]);
        if (!base) return JS_ThrowTypeError(ctx, "zlib push: ArrayBufferView expected");
        return ns_zlib_run(ctx, c, base, total, FALSE);
    }
    size_t total = 0;
    uint8_t *base = JS_GetArrayBuffer(ctx, &total, buf);
    JS_FreeValue(ctx, buf);
    if (!base || off + len > total)
        return JS_ThrowInternalError(ctx, "zlib push: backing buffer unavailable");
    return ns_zlib_run(ctx, c, base + off, len, FALSE);
}

static JSValue
ns_zlib_finish(JSContext *ctx, JSValueConst this_val, int argc, JSValueConst *argv)
{
    (void)this_val;
    if (argc < 1)
        return JS_ThrowTypeError(ctx, "zlib finish: codec required");
    ns_zlib_codec *c = ns_zlib_unwrap(ctx, argv[0]);
    if (!c) return JS_ThrowTypeError(ctx, "zlib finish: invalid codec");
    return ns_zlib_run(ctx, c, (const uint8_t *)"", 0, TRUE);
}

static void
ns_define_element_unscopables(JSContext *ctx, JSValue proto)
{
    JSValue gobj = JS_GetGlobalObject(ctx);
    JSValue sym = JS_GetPropertyStr(ctx, gobj, "Symbol");
    JSValue us_sym = JS_GetPropertyStr(ctx, sym, "unscopables");
    JSAtom us_atom = JS_ValueToAtom(ctx, us_sym);
    if (us_atom != JS_ATOM_NULL) {
        static const char *const names[] = {
            "before", "after", "replaceWith", "remove",
            "prepend", "append", "replaceChildren",
        };
        JSValue us = JS_NewObjectProto(ctx, JS_NULL);
        for (gsize i = 0; i < G_N_ELEMENTS(names); i++)
            JS_DefinePropertyValueStr(ctx, us, names[i], JS_TRUE,
                                      JS_PROP_C_W_E);
        JS_DefinePropertyValue(ctx, proto, us_atom, us, JS_PROP_CONFIGURABLE);
        JS_FreeAtom(ctx, us_atom);
    }
    JS_FreeValue(ctx, us_sym);
    JS_FreeValue(ctx, sym);
    JS_FreeValue(ctx, gobj);
}

JSValue
ns_own_data_props_toJSON(JSContext *ctx, JSValueConst this_val,
                         int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue out = JS_NewObject(ctx);
    if (JS_IsException(out)) return out;
    JSPropertyEnum *tab = NULL;
    uint32_t n = 0;
    if (JS_GetOwnPropertyNames(ctx, &tab, &n, this_val,
                               JS_GPN_STRING_MASK | JS_GPN_ENUM_ONLY) < 0)
        return out;
    for (uint32_t i = 0; i < n; i++) {
        JSValue pv = JS_GetProperty(ctx, this_val, tab[i].atom);
        if (JS_IsException(pv) || JS_IsFunction(ctx, pv)) {
            JS_FreeValue(ctx, pv);
            continue;
        }
        JS_DefinePropertyValue(ctx, out, tab[i].atom, pv, JS_PROP_C_W_E);
    }
    JS_FreePropertyEnum(ctx, tab, n);
    return out;
}

static JSValue
ns_window_performance_toJSON(JSContext *ctx, JSValueConst this_val,
                            int argc, JSValueConst *argv)
{
    (void)argc; (void)argv;
    JSValue out = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, out, "timeOrigin",
                      JS_GetPropertyStr(ctx, this_val, "timeOrigin"));
    JSValue timing = JS_GetPropertyStr(ctx, this_val, "timing");
    if (JS_IsObject(timing)) JS_SetPropertyStr(ctx, out, "timing", timing);
    else JS_FreeValue(ctx, timing);
    JSValue nav = JS_GetPropertyStr(ctx, this_val, "navigation");
    if (JS_IsObject(nav)) JS_SetPropertyStr(ctx, out, "navigation", nav);
    else JS_FreeValue(ctx, nav);
    return out;
}

static void
ns_performance_extend_event_target(JSContext *ctx, JSValueConst global,
                                   JSValueConst performance)
{
    JSValue et = JS_GetPropertyStr(ctx, global, "EventTarget");
    if (JS_IsObject(et)) {
        JSValue etp = JS_GetPropertyStr(ctx, et, "prototype");
        if (JS_IsObject(etp)) JS_SetPrototype(ctx, performance, etp);
        JS_FreeValue(ctx, etp);
    }
    JS_FreeValue(ctx, et);
}

static JSValue
ns_proto_of(JSContext *ctx, JSValueConst global, const char *ctor_name)
{
    JSValue ctor = JS_GetPropertyStr(ctx, global, ctor_name);
    JSValue proto = JS_IsObject(ctor)
        ? JS_GetPropertyStr(ctx, ctor, "prototype") : JS_UNDEFINED;
    JS_FreeValue(ctx, ctor);
    return proto;
}

static void
ns_install_performance_prototype(JSContext *ctx, JSValueConst global)
{
    JSValue proto = ns_proto_of(ctx, global, "Performance");
    if (!JS_IsObject(proto)) {
        JS_FreeValue(ctx, proto);
        return;
    }
    ns_performance_extend_event_target(ctx, global, proto);
    ns_bind_fn(ctx, proto, "now", ns_window_performance_now, 0);
    ns_bind_fn(ctx, proto, "mark", ns_window_performance_mark, 1);
    ns_bind_fn(ctx, proto, "measure", ns_window_performance_measure, 3);
    ns_bind_fn(ctx, proto, "clearMarks", ns_window_performance_clearMarks, 1);
    ns_bind_fn(ctx, proto, "clearMeasures", ns_window_performance_clearMeasures, 1);
    ns_bind_fn(ctx, proto, "getEntries", ns_window_performance_getEntries, 0);
    ns_bind_fn(ctx, proto, "getEntriesByName",
               ns_window_performance_getEntriesByName, 2);
    ns_bind_fn(ctx, proto, "getEntriesByType",
               ns_window_performance_getEntriesByType, 1);
    ns_bind_fn(ctx, proto, "clearResourceTimings",
               ns_window_performance_clearResourceTimings, 0);
    ns_bind_fn(ctx, proto, "setResourceTimingBufferSize", ns_event_noop, 1);
    ns_bind_fn(ctx, proto, "toJSON", ns_window_performance_toJSON, 0);
    /* Each window's performance object answers with its own document's
     * time origin and timing objects. */
    JSAtom atom = JS_NewAtom(ctx, "timeOrigin");
    JS_DefinePropertyGetSet(ctx, proto, atom,
        JS_NewCFunction2(ctx, ns_window_performance_time_origin_get,
                         "get timeOrigin", 0, JS_CFUNC_generic, 0),
        JS_UNDEFINED, JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
    JS_FreeAtom(ctx, atom);
    static const char *const objects[] = { "timing", "navigation",
                                           "eventCounts" };
    for (int i = 0; i < 3; i++) {
        char *getter_name = g_strconcat("get ", objects[i], NULL);
        atom = JS_NewAtom(ctx, objects[i]);
        JS_DefinePropertyGetSet(ctx, proto, atom,
            JS_NewCFunctionMagic(ctx, ns_window_performance_object_get,
                                 getter_name, 0, JS_CFUNC_generic_magic, i),
            JS_UNDEFINED, JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
        JS_FreeAtom(ctx, atom);
        g_free(getter_name);
    }
    ns_set_tostring_tag(ctx, proto, "Performance");
    JS_FreeValue(ctx, proto);
}

static JSValue
ns_make_performance_object(JSContext *ctx, ns_js *js,
                           gboolean include_memory)
{
    JSValue global = JS_GetGlobalObject(ctx);
    ns_install_performance_prototype(ctx, global);
    JSValue performance = ns_perf_new_performance_object(ctx);
    ns_obj_adopt_global_proto(ctx, performance, "Performance");
    double origin_real_ms = ns_js_time_origin_real_ms(js, ctx);

    JSValue perf_timing = JS_NewObject(ctx);
    const struct { const char *k; double relative_ms; gboolean present; }
        timing_fields[] = {
            {"navigationStart",0,TRUE},
            {"unloadEventStart",0,FALSE},{"unloadEventEnd",0,FALSE},
            {"redirectStart",0,FALSE},{"redirectEnd",0,FALSE},
            {"fetchStart",0,TRUE},{"domainLookupStart",0,TRUE},
            {"domainLookupEnd",js ? js->navigation_timing.domain_lookup_end_ms : 0,TRUE},
            {"connectStart",js ? js->navigation_timing.connect_start_ms : 0,TRUE},
            {"connectEnd",js ? js->navigation_timing.connect_end_ms : 0,TRUE},
            {"secureConnectionStart",
             js ? js->navigation_timing.secure_connection_start_ms : 0,
             js && js->navigation_timing.secure_connection_start_ms > 0},
            {"requestStart",js ? js->navigation_timing.request_start_ms : 0,TRUE},
            {"responseStart",js ? js->navigation_timing.response_start_ms : 0,TRUE},
            {"responseEnd",js ? js->navigation_timing.response_end_ms : 0,TRUE},
            {"domLoading",0,FALSE},{"domInteractive",0,FALSE},
            {"domContentLoadedEventStart",0,FALSE},
            {"domContentLoadedEventEnd",0,FALSE},{"domComplete",0,FALSE},
            {"loadEventStart",0,FALSE},{"loadEventEnd",0,FALSE},
        };
    for (gsize i = 0; i < G_N_ELEMENTS(timing_fields); i++) {
        gint64 value = timing_fields[i].present && js
            ? (gint64)floor(origin_real_ms + timing_fields[i].relative_ms)
            : 0;
        JS_SetPropertyStr(ctx, perf_timing, timing_fields[i].k,
                          JS_NewInt64(ctx, value));
    }
    ns_bind_fn(ctx, perf_timing, "toJSON", ns_own_data_props_toJSON, 0);
    ns_obj_adopt_global_proto(ctx, perf_timing, "PerformanceTiming");

    JSValue perf_nav = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, perf_nav, "type", JS_NewInt32(ctx, 0));
    JS_SetPropertyStr(ctx, perf_nav, "redirectCount", JS_NewInt32(ctx, 0));
    ns_bind_fn(ctx, perf_nav, "toJSON", ns_own_data_props_toJSON, 0);
    ns_obj_adopt_global_proto(ctx, perf_nav, "PerformanceNavigation");
    ns_perf_set_performance_objects(ctx, performance, perf_timing, perf_nav,
                                    JS_NewObject(ctx));

    if (include_memory) {
        JSAtom atom = JS_NewAtom(ctx, "memory");
        JSValue getter = JS_NewCFunction2(ctx,
            ns_window_performance_memory_get,
            "get memory", 0, JS_CFUNC_generic, 0);
        JS_DefinePropertyGetSet(ctx, performance, atom, getter, JS_UNDEFINED,
                                JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
        JS_FreeAtom(ctx, atom);
    }
    JS_FreeValue(ctx, global);
    return performance;
}

static void
ns_chain_proto(JSContext *ctx, JSValueConst global, const char *child_ctor,
               JSValueConst parent_proto)
{
    if (!JS_IsObject(parent_proto)) return;
    JSValue proto = ns_proto_of(ctx, global, child_ctor);
    if (JS_IsObject(proto)) JS_SetPrototype(ctx, proto, parent_proto);
    JS_FreeValue(ctx, proto);
}

static void
ns_install_tostringtag(JSContext *ctx, JSValueConst global)
{
    JSValue sym = JS_GetPropertyStr(ctx, global, "Symbol");
    if (!JS_IsObject(sym)) { JS_FreeValue(ctx, sym); return; }
    JSValue tst = JS_GetPropertyStr(ctx, sym, "toStringTag");
    JS_FreeValue(ctx, sym);
    JSAtom tag_atom = JS_ValueToAtom(ctx, tst);
    JS_FreeValue(ctx, tst);
    if (tag_atom == JS_ATOM_NULL) return;

    static const char *const base_ifaces[] = {
        "HTMLElement", "HTMLUnknownElement", "Element", "Node",
        "CharacterData", "Text",
        "Comment", "CDATASection", "ProcessingInstruction", "Document",
        "HTMLDocument", "XMLDocument", "DocumentFragment", "ShadowRoot",
        "DocumentType", "Attr", "SVGElement", "SVGAElement", "SVGSVGElement",
        "MathMLElement",
        "Event", "UIEvent", "MouseEvent", "KeyboardEvent", "FocusEvent",
        "InputEvent", "CompositionEvent", "TextEvent", "TouchEvent",
        "PointerEvent", "WheelEvent", "DragEvent", "CustomEvent",
        "ProgressEvent", "ErrorEvent", "MessageEvent", "CloseEvent",
        "PopStateEvent", "HashChangeEvent", "StorageEvent", "AnimationEvent",
        "TransitionEvent", "ClipboardEvent", "SubmitEvent", "BeforeUnloadEvent",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(base_ifaces); i++) {
        JSValue proto = ns_proto_of(ctx, global, base_ifaces[i]);
        if (JS_IsObject(proto))
            JS_DefinePropertyValue(ctx, proto, tag_atom,
                JS_NewString(ctx, base_ifaces[i]), JS_PROP_CONFIGURABLE);
        JS_FreeValue(ctx, proto);
    }
    for (gsize i = 0; i < G_N_ELEMENTS(ns_instof_table); i++) {
        const ns_instof_def *d = &ns_instof_table[i];
        if (d->special != NS_INSTOF_TAG) continue;
        JSValue proto = ns_proto_of(ctx, global, d->ctor);
        if (JS_IsObject(proto))
            JS_DefinePropertyValue(ctx, proto, tag_atom,
                JS_NewString(ctx, d->ctor), JS_PROP_CONFIGURABLE);
        JS_FreeValue(ctx, proto);
    }
    JS_FreeAtom(ctx, tag_atom);
}

/* The members each non-element node interface defines (WebIDL, as Chrome
 * ships them). The element member table used to be installed on Node,
 * Document, HTMLDocument and DocumentFragment as well, so a document, a
 * text node or a fragment had click(), style, tagName and the rest;
 * ns_install_node_shapes keeps only these names on those prototypes. A
 * name a character-data node, a doctype or a shadow root should have but
 * only found through Node.prototype (or DocumentFragment.prototype) is
 * moved down first. The __shady_ shims stay. */
static const char ns_node_shapes_src[] =
    "(function(G){"
    "  var W = {"
        "  Node: 'ATTRIBUTE_NODE CDATA_SECTION_NODE COMMENT_NODE "
        "DOCUMENT_FRAGMENT_NODE DOCUMENT_NODE "
        "DOCUMENT_POSITION_CONTAINED_BY DOCUMENT_POSITION_CONTAINS "
        "DOCUMENT_POSITION_DISCONNECTED DOCUMENT_POSITION_FOLLOWING "
        "DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC "
        "DOCUMENT_POSITION_PRECEDING DOCUMENT_TYPE_NODE ELEMENT_NODE "
        "ENTITY_NODE ENTITY_REFERENCE_NODE NOTATION_NODE "
        "PROCESSING_INSTRUCTION_NODE TEXT_NODE appendChild baseURI "
        "childNodes cloneNode compareDocumentPosition contains firstChild "
        "getRootNode hasChildNodes insertBefore isConnected "
        "isDefaultNamespace isEqualNode isSameNode lastChild "
        "lookupNamespaceURI lookupPrefix nextSibling nodeName nodeType "
        "nodeValue normalize ownerDocument parentElement parentNode "
        "previousSibling removeChild replaceChild textContent',"
        "  CharacterData: 'after appendData before data deleteData insertData length "
        "nextElementSibling previousElementSibling remove replaceData "
        "replaceWith substringData',"
        "  Text: 'assignedSlot splitText wholeText',"
        "  Comment: '',"
        "  ProcessingInstruction: 'getAttribute getAttributeNames hasAttribute hasAttributes "
        "removeAttribute setAttribute sheet target toggleAttribute',"
        "  CDATASection: '',"
        "  DocumentType: 'after before name publicId remove replaceWith systemId',"
        "  Document: 'URL activeElement activeViewTransition adoptNode "
        "adoptedStyleSheets alinkColor all anchors append applets "
        "ariaNotify bgColor body browsingTopics captureEvents "
        "caretPositionFromPoint caretRangeFromPoint characterSet charset "
        "childElementCount children clear close compatMode contentType "
        "cookie createAttribute createAttributeNS createCDATASection "
        "createComment createDocumentFragment createElement createElementNS "
        "createEvent createExpression createNSResolver createNodeIterator "
        "createProcessingInstruction createRange createTextNode "
        "createTreeWalker currentScript customElementRegistry defaultView "
        "designMode dir doctype documentElement documentURI domain "
        "elementFromPoint elementsFromPoint embeds evaluate execCommand "
        "exitFullscreen exitPictureInPicture exitPointerLock featurePolicy "
        "fgColor firstElementChild fonts forms fragmentDirective fullscreen "
        "fullscreenElement fullscreenEnabled getAnimations getElementById "
        "getElementsByClassName getElementsByName getElementsByTagName "
        "getElementsByTagNameNS getSelection hasFocus hasPrivateToken "
        "hasRedemptionRecord hasStorageAccess hasUnpartitionedCookieAccess "
        "head hidden images implementation importNode inputEncoding "
        "lastElementChild lastModified linkColor links moveBefore onabort "
        "onanimationcancel onanimationend onanimationiteration "
        "onanimationstart onauxclick onbeforecopy onbeforecut onbeforeinput "
        "onbeforematch onbeforepaste onbeforetoggle onbeforexrselect onblur "
        "oncancel oncanplay oncanplaythrough onchange onclick onclose "
        "oncommand oncontentvisibilityautostatechange oncontextlost "
        "oncontextmenu oncontextrestored oncopy oncuechange oncut "
        "ondblclick ondrag ondragend ondragenter ondragleave ondragover "
        "ondragstart ondrop ondurationchange onemptied onended onerror "
        "onfocus onformdata onfreeze onfullscreenchange onfullscreenerror "
        "ongotpointercapture oninput oninvalid onkeydown onkeypress onkeyup "
        "onload onloadeddata onloadedmetadata onloadstart "
        "onlostpointercapture onmousedown onmouseenter onmouseleave "
        "onmousemove onmouseout onmouseover onmouseup onmousewheel onpaste "
        "onpause onplay onplaying onpointercancel onpointerdown "
        "onpointerenter onpointerleave onpointerlockchange "
        "onpointerlockerror onpointermove onpointerout onpointerover "
        "onpointerrawupdate onpointerup onprerenderingchange onprogress "
        "onratechange onreadystatechange onreset onresize onresume onscroll "
        "onscrollend onscrollsnapchange onscrollsnapchanging onsearch "
        "onsecuritypolicyviolation onseeked onseeking onselect "
        "onselectionchange onselectstart onslotchange onstalled onsubmit "
        "onsuspend ontimeupdate ontoggle ontransitioncancel ontransitionend "
        "ontransitionrun ontransitionstart onvisibilitychange "
        "onvolumechange onwaiting onwebkitanimationend "
        "onwebkitanimationiteration onwebkitanimationstart "
        "onwebkitfullscreenchange onwebkitfullscreenerror "
        "onwebkittransitionend onwheel open pictureInPictureElement "
        "pictureInPictureEnabled plugins pointerLockElement prepend "
        "prerendering queryCommandEnabled queryCommandIndeterm "
        "queryCommandState queryCommandSupported queryCommandValue "
        "querySelector querySelectorAll readyState referrer releaseEvents "
        "replaceChildren requestStorageAccess rootElement scripts "
        "scrollingElement startViewTransition styleSheets timeline title "
        "visibilityState vlinkColor wasDiscarded webkitCancelFullScreen "
        "webkitCurrentFullScreenElement webkitExitFullscreen "
        "webkitFullscreenElement webkitFullscreenEnabled webkitHidden "
        "webkitIsFullScreen webkitVisibilityState write writeln xmlEncoding "
        "xmlStandalone xmlVersion',"
        "  HTMLDocument: '',"
        "  XMLDocument: '',"
        "  DocumentFragment: 'append childElementCount children firstElementChild getElementById "
        "lastElementChild moveBefore prepend querySelector querySelectorAll "
        "replaceChildren',"
        "  ShadowRoot: 'activeElement adoptedStyleSheets clonable customElementRegistry "
        "delegatesFocus elementFromPoint elementsFromPoint "
        "fullscreenElement getAnimations getHTML getSelection host "
        "innerHTML mode onslotchange pictureInPictureElement "
        "pointerLockElement referenceTarget serializable setHTML "
        "setHTMLUnsafe slotAssignment styleSheets',"
    "  };"
    "  var up = { HTMLDocument: 'Document', XMLDocument: 'Document' };"
    "  Object.keys(up).forEach(function(n){"
    "    var C = G[n], U = G[up[n]]; if (!C || !U || !C.prototype || !U.prototype) return;"
    "    var P = C.prototype, Q = U.prototype, want = new Set(W[up[n]].split(' '));"
    "    Object.getOwnPropertyNames(P).forEach(function(k){"
    "      if (!want.has(k) || Object.prototype.hasOwnProperty.call(Q, k)) return;"
    "      try { Object.defineProperty(Q, k, Object.getOwnPropertyDescriptor(P, k)); } catch (e) {}"
    "    });"
    "  });"
    "  var from = { CharacterData: ['Node'], Text: ['Node'], Comment: ['Node'],"
    "    ProcessingInstruction: ['Node'], CDATASection: ['Node'],"
    "    DocumentType: ['Node'], ShadowRoot: ['DocumentFragment', 'Node'] };"
    "  var own = Object.prototype.hasOwnProperty, plan = [];"
    "  Object.keys(W).forEach(function(n){"
    "    var C = G[n]; if (typeof C !== 'function' || !C.prototype) return;"
    "    var P = C.prototype, want = new Set(W[n] ? W[n].split(' ') : []), add = [];"
    "    want.forEach(function(k){"
    "      if (own.call(P, k)) return;"
    "      (from[n] || []).some(function(s){"
    "        var S = G[s] && G[s].prototype, d = S && Object.getOwnPropertyDescriptor(S, k);"
    "        if (d) add.push([k, d]);"
    "        return !!d;"
    "      });"
    "    });"
    "    var del = Object.getOwnPropertyNames(P).filter(function(k){"
    "      return k !== 'constructor' && !want.has(k) && k.slice(0, 8) !== '__shady_';"
    "    });"
    "    plan.push([P, add, del]);"
    "  });"
    "  plan.forEach(function(p){ p[1].forEach(function(a){"
    "    try { Object.defineProperty(p[0], a[0], a[1]); } catch (e) {} }); });"
    "  plan.forEach(function(p){ p[2].forEach(function(k){"
    "    try { delete p[0][k]; } catch (e) {} }); });"
    "})(globalThis)";

static void
ns_install_node_shapes(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_node_shapes_src, sizeof(ns_node_shapes_src) - 1,
                        "<node-shapes>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

/* The same for elements: the element member table sits on Element.prototype
 * only, but it holds what HTMLElement and each HTML element interface define
 * as well (value, href, checked, click(), style...). Each HTML element
 * interface and HTMLElement take their members from it, SVGElement takes
 * the members of the SVG element interfaces this engine does not have yet
 * (getBBox() and the rest; an <svg> or <circle> is an SVGElement here), and
 * Element and HTMLElement then keep only their own. */
static const char ns_element_shapes_src[] =
    "(function(G){"
    "  var W = {"
        "  Element: 'activeViewTransition after animate append ariaActionsElements "
        "ariaActiveDescendantElement ariaAtomic ariaAutoComplete "
        "ariaBrailleLabel ariaBrailleRoleDescription ariaBusy ariaChecked "
        "ariaColCount ariaColIndex ariaColIndexText ariaColSpan "
        "ariaControlsElements ariaCurrent ariaDescribedByElements "
        "ariaDescription ariaDetailsElements ariaDisabled "
        "ariaErrorMessageElements ariaExpanded ariaFlowToElements "
        "ariaHasPopup ariaHidden ariaInvalid ariaKeyShortcuts ariaLabel "
        "ariaLabelledByElements ariaLevel ariaLive ariaModal ariaMultiLine "
        "ariaMultiSelectable ariaNotify ariaOrientation ariaPlaceholder "
        "ariaPosInSet ariaPressed ariaReadOnly ariaRelevant ariaRequired "
        "ariaRoleDescription ariaRowCount ariaRowIndex ariaRowIndexText "
        "ariaRowSpan ariaSelected ariaSetSize ariaSort ariaValueMax "
        "ariaValueMin ariaValueNow ariaValueText assignedSlot attachShadow "
        "attributes before checkVisibility childElementCount children "
        "classList className clientHeight clientLeft clientTop clientWidth "
        "closest computedStyleMap currentCSSZoom customElementRegistry "
        "elementTiming firstElementChild getAnimations getAttribute "
        "getAttributeNS getAttributeNames getAttributeNode "
        "getAttributeNodeNS getBoundingClientRect getClientRects "
        "getElementsByClassName getElementsByTagName getElementsByTagNameNS "
        "getHTML hasAttribute hasAttributeNS hasAttributes "
        "hasPointerCapture id innerHTML insertAdjacentElement "
        "insertAdjacentHTML insertAdjacentText lastElementChild localName "
        "matches moveBefore namespaceURI nextElementSibling onbeforecopy "
        "onbeforecut onbeforepaste onfullscreenchange onfullscreenerror "
        "onsearch onwebkitfullscreenchange onwebkitfullscreenerror "
        "outerHTML part prefix prepend previousElementSibling pseudo "
        "querySelector querySelectorAll releasePointerCapture remove "
        "removeAttribute removeAttributeNS removeAttributeNode "
        "replaceChildren replaceWith requestFullscreen requestPointerLock "
        "role scroll scrollBy scrollHeight scrollIntoView "
        "scrollIntoViewIfNeeded scrollLeft scrollTo scrollTop scrollWidth "
        "setAttribute setAttributeNS setAttributeNode setAttributeNodeNS "
        "setHTML setHTMLUnsafe setPointerCapture shadowRoot slot "
        "startViewTransition tagName toggleAttribute webkitMatchesSelector "
        "webkitRequestFullScreen webkitRequestFullscreen',"
        "  HTMLElement: 'accessKey attachInternals attributeStyleMap autocapitalize "
        "autocorrect autofocus blur click contentEditable dataset dir "
        "draggable editContext enterKeyHint focus focusGroup "
        "focusGroupStart hidden hidePopover inert innerText inputMode "
        "isContentEditable lang nonce offsetHeight offsetLeft offsetParent "
        "offsetTop offsetWidth onabort onanimationcancel onanimationend "
        "onanimationiteration onanimationstart onauxclick onbeforeinput "
        "onbeforematch onbeforetoggle onbeforexrselect onblur oncancel "
        "oncanplay oncanplaythrough onchange onclick onclose oncommand "
        "oncontentvisibilityautostatechange oncontextlost oncontextmenu "
        "oncontextrestored oncopy oncuechange oncut ondblclick ondrag "
        "ondragend ondragenter ondragleave ondragover ondragstart ondrop "
        "ondurationchange onemptied onended onerror onfocus onformdata "
        "ongotpointercapture oninput oninvalid onkeydown onkeypress onkeyup "
        "onload onloadeddata onloadedmetadata onloadstart "
        "onlostpointercapture onmousedown onmouseenter onmouseleave "
        "onmousemove onmouseout onmouseover onmouseup onmousewheel onpaste "
        "onpause onplay onplaying onpointercancel onpointerdown "
        "onpointerenter onpointerleave onpointermove onpointerout "
        "onpointerover onpointerrawupdate onpointerup onprogress "
        "onratechange onreset onresize onscroll onscrollend "
        "onscrollsnapchange onscrollsnapchanging onsecuritypolicyviolation "
        "onseeked onseeking onselect onselectionchange onselectstart "
        "onslotchange onstalled onsubmit onsuspend ontimeupdate ontoggle "
        "ontransitioncancel ontransitionend ontransitionrun "
        "ontransitionstart onvolumechange onwaiting onwebkitanimationend "
        "onwebkitanimationiteration onwebkitanimationstart "
        "onwebkittransitionend onwheel outerText popover showPopover "
        "spellcheck style tabIndex title togglePopover translate "
        "virtualKeyboardPolicy writingSuggestions',"
        "  MathMLElement: 'attributeStyleMap autofocus blur dataset focus focusGroup "
        "focusGroupStart nonce onabort onanimationcancel onanimationend "
        "onanimationiteration onanimationstart onauxclick onbeforeinput "
        "onbeforematch onbeforetoggle onbeforexrselect onblur oncancel "
        "oncanplay oncanplaythrough onchange onclick onclose oncommand "
        "oncontentvisibilityautostatechange oncontextlost oncontextmenu "
        "oncontextrestored oncopy oncuechange oncut ondblclick ondrag "
        "ondragend ondragenter ondragleave ondragover ondragstart ondrop "
        "ondurationchange onemptied onended onerror onfocus onformdata "
        "ongotpointercapture oninput oninvalid onkeydown onkeypress onkeyup "
        "onload onloadeddata onloadedmetadata onloadstart "
        "onlostpointercapture onmousedown onmouseenter onmouseleave "
        "onmousemove onmouseout onmouseover onmouseup onmousewheel onpaste "
        "onpause onplay onplaying onpointercancel onpointerdown "
        "onpointerenter onpointerleave onpointermove onpointerout "
        "onpointerover onpointerrawupdate onpointerup onprogress "
        "onratechange onreset onresize onscroll onscrollend "
        "onscrollsnapchange onscrollsnapchanging onsecuritypolicyviolation "
        "onseeked onseeking onselect onselectionchange onselectstart "
        "onslotchange onstalled onsubmit onsuspend ontimeupdate ontoggle "
        "ontransitioncancel ontransitionend ontransitionrun "
        "ontransitionstart onvolumechange onwaiting onwebkitanimationend "
        "onwebkitanimationiteration onwebkitanimationstart "
        "onwebkittransitionend onwheel style tabIndex',"
        "  SVGElement: 'LENGTHADJUST_SPACING LENGTHADJUST_SPACINGANDGLYPHS "
        "LENGTHADJUST_UNKNOWN SVG_CHANNEL_A SVG_CHANNEL_B SVG_CHANNEL_G "
        "SVG_CHANNEL_R SVG_CHANNEL_UNKNOWN SVG_EDGEMODE_DUPLICATE "
        "SVG_EDGEMODE_NONE SVG_EDGEMODE_UNKNOWN SVG_EDGEMODE_WRAP "
        "SVG_FEBLEND_MODE_COLOR SVG_FEBLEND_MODE_COLOR_BURN "
        "SVG_FEBLEND_MODE_COLOR_DODGE SVG_FEBLEND_MODE_DARKEN "
        "SVG_FEBLEND_MODE_DIFFERENCE SVG_FEBLEND_MODE_EXCLUSION "
        "SVG_FEBLEND_MODE_HARD_LIGHT SVG_FEBLEND_MODE_HUE "
        "SVG_FEBLEND_MODE_LIGHTEN SVG_FEBLEND_MODE_LUMINOSITY "
        "SVG_FEBLEND_MODE_MULTIPLY SVG_FEBLEND_MODE_NORMAL "
        "SVG_FEBLEND_MODE_OVERLAY SVG_FEBLEND_MODE_SATURATION "
        "SVG_FEBLEND_MODE_SCREEN SVG_FEBLEND_MODE_SOFT_LIGHT "
        "SVG_FEBLEND_MODE_UNKNOWN SVG_FECOLORMATRIX_TYPE_HUEROTATE "
        "SVG_FECOLORMATRIX_TYPE_LUMINANCETOALPHA "
        "SVG_FECOLORMATRIX_TYPE_MATRIX SVG_FECOLORMATRIX_TYPE_SATURATE "
        "SVG_FECOLORMATRIX_TYPE_UNKNOWN "
        "SVG_FECOMPONENTTRANSFER_TYPE_DISCRETE "
        "SVG_FECOMPONENTTRANSFER_TYPE_GAMMA "
        "SVG_FECOMPONENTTRANSFER_TYPE_IDENTITY "
        "SVG_FECOMPONENTTRANSFER_TYPE_LINEAR "
        "SVG_FECOMPONENTTRANSFER_TYPE_TABLE "
        "SVG_FECOMPONENTTRANSFER_TYPE_UNKNOWN "
        "SVG_FECOMPOSITE_OPERATOR_ARITHMETIC SVG_FECOMPOSITE_OPERATOR_ATOP "
        "SVG_FECOMPOSITE_OPERATOR_IN SVG_FECOMPOSITE_OPERATOR_OUT "
        "SVG_FECOMPOSITE_OPERATOR_OVER SVG_FECOMPOSITE_OPERATOR_UNKNOWN "
        "SVG_FECOMPOSITE_OPERATOR_XOR SVG_MARKERUNITS_STROKEWIDTH "
        "SVG_MARKERUNITS_UNKNOWN SVG_MARKERUNITS_USERSPACEONUSE "
        "SVG_MARKER_ORIENT_ANGLE SVG_MARKER_ORIENT_AUTO "
        "SVG_MARKER_ORIENT_UNKNOWN SVG_MORPHOLOGY_OPERATOR_DILATE "
        "SVG_MORPHOLOGY_OPERATOR_ERODE SVG_MORPHOLOGY_OPERATOR_UNKNOWN "
        "SVG_SPREADMETHOD_PAD SVG_SPREADMETHOD_REFLECT "
        "SVG_SPREADMETHOD_REPEAT SVG_SPREADMETHOD_UNKNOWN "
        "SVG_STITCHTYPE_NOSTITCH SVG_STITCHTYPE_STITCH "
        "SVG_STITCHTYPE_UNKNOWN SVG_TURBULENCE_TYPE_FRACTALNOISE "
        "SVG_TURBULENCE_TYPE_TURBULENCE SVG_TURBULENCE_TYPE_UNKNOWN "
        "SVG_ZOOMANDPAN_DISABLE SVG_ZOOMANDPAN_MAGNIFY "
        "SVG_ZOOMANDPAN_UNKNOWN TEXTPATH_METHODTYPE_ALIGN "
        "TEXTPATH_METHODTYPE_STRETCH TEXTPATH_METHODTYPE_UNKNOWN "
        "TEXTPATH_SIDETYPE_LEFT TEXTPATH_SIDETYPE_RIGHT "
        "TEXTPATH_SIDETYPE_UNKNOWN TEXTPATH_SPACINGTYPE_AUTO "
        "TEXTPATH_SPACINGTYPE_EXACT TEXTPATH_SPACINGTYPE_UNKNOWN amplitude "
        "animatedPoints animationsPaused async attributeStyleMap autofocus "
        "azimuth baseFrequencyX baseFrequencyY beginElement beginElementAt "
        "bias blur checkEnclosure checkIntersection className clipPathUnits "
        "createSVGAngle createSVGLength createSVGMatrix createSVGNumber "
        "createSVGPoint createSVGRect createSVGTransform "
        "createSVGTransformFromMatrix crossOrigin currentScale "
        "currentTranslate cx cy dataset decode decoding deselectAll "
        "diffuseConstant disabled divisor download dx dy edgeMode elevation "
        "endElement endElementAt exponent farthestViewportElement "
        "filterUnits focus focusGroup focusGroupStart forceRedraw fr fx fy "
        "getBBox getCTM getCharNumAtPosition getComputedTextLength "
        "getCurrentTime getElementById getEnclosureList "
        "getEndPositionOfChar getExtentOfChar getIntersectionList "
        "getNumberOfChars getPointAtLength getRotationOfChar getScreenCTM "
        "getSimpleDuration getStartPositionOfChar getStartTime "
        "getSubStringLength getTotalLength gradientTransform gradientUnits "
        "height href hreflang in1 in2 intercept interestForElement "
        "isPointInFill isPointInStroke k1 k2 k3 k4 kernelMatrix "
        "kernelUnitLengthX kernelUnitLengthY lengthAdjust limitingConeAngle "
        "markerHeight markerUnits markerWidth maskContentUnits maskUnits "
        "media method mode nearestViewportElement nonce numOctaves offset "
        "onabort onanimationcancel onanimationend onanimationiteration "
        "onanimationstart onauxclick onbeforeinput onbeforematch "
        "onbeforetoggle onbeforexrselect onbegin onblur oncancel oncanplay "
        "oncanplaythrough onchange onclick onclose oncommand "
        "oncontentvisibilityautostatechange oncontextlost oncontextmenu "
        "oncontextrestored oncopy oncuechange oncut ondblclick ondrag "
        "ondragend ondragenter ondragleave ondragover ondragstart ondrop "
        "ondurationchange onemptied onend onended onerror onfocus "
        "onformdata ongotpointercapture oninput oninvalid onkeydown "
        "onkeypress onkeyup onload onloadeddata onloadedmetadata "
        "onloadstart onlostpointercapture onmousedown onmouseenter "
        "onmouseleave onmousemove onmouseout onmouseover onmouseup "
        "onmousewheel onpaste onpause onplay onplaying onpointercancel "
        "onpointerdown onpointerenter onpointerleave onpointermove "
        "onpointerout onpointerover onpointerrawupdate onpointerup "
        "onprogress onratechange onrepeat onreset onresize onscroll "
        "onscrollend onscrollsnapchange onscrollsnapchanging "
        "onsecuritypolicyviolation onseeked onseeking onselect "
        "onselectionchange onselectstart onslotchange onstalled onsubmit "
        "onsuspend ontimeupdate ontoggle ontransitioncancel ontransitionend "
        "ontransitionrun ontransitionstart onvolumechange onwaiting "
        "onwebkitanimationend onwebkitanimationiteration "
        "onwebkitanimationstart onwebkittransitionend onwheel operator "
        "orderX orderY orientAngle orientType ownerSVGElement pathLength "
        "patternContentUnits patternTransform patternUnits pauseAnimations "
        "ping points pointsAtX pointsAtY pointsAtZ preserveAlpha "
        "preserveAspectRatio primitiveUnits r radiusX radiusY refX refY "
        "referrerPolicy rel relList requiredExtensions result rotate rx ry "
        "scale seed selectSubString setCurrentTime setOrientToAngle "
        "setOrientToAuto setStdDeviation sheet slope spacing "
        "specularConstant specularExponent spreadMethod startOffset "
        "stdDeviationX stdDeviationY stitchTiles style surfaceScale "
        "suspendRedraw systemLanguage tabIndex tableValues target "
        "targetElement targetX targetY textLength title transform type "
        "unpauseAnimations unsuspendRedraw unsuspendRedrawAll values "
        "viewBox viewportElement width x x1 x2 xChannelSelector y y1 y2 "
        "yChannelSelector z zoomAndPan',"
        "  HTMLAnchorElement: 'attributionSrc charset coords download hash host hostname href "
        "hrefTranslate hreflang interestForElement name origin password "
        "pathname ping port protocol referrerPolicy rel relList rev search "
        "shape target text toString type username',"
        "  HTMLAreaElement: 'alt attributionSrc coords download hash host hostname href "
        "interestForElement noHref origin password pathname ping port "
        "protocol referrerPolicy rel relList search shape target toString "
        "username',"
        "  HTMLBRElement: 'clear',"
        "  HTMLBaseElement: 'href target',"
        "  HTMLBodyElement: 'aLink background bgColor link onafterprint onbeforeprint "
        "onbeforeunload onblur onerror onfocus ongamepadconnected "
        "ongamepaddisconnected onhashchange onlanguagechange onload "
        "onmessage onmessageerror onoffline ononline onpagehide onpageshow "
        "onpopstate onrejectionhandled onresize onscroll onstorage "
        "onunhandledrejection onunload text vLink',"
        "  HTMLButtonElement: 'checkValidity command commandForElement disabled form formAction "
        "formEnctype formMethod formNoValidate formTarget "
        "interestForElement labels name popoverTargetAction "
        "popoverTargetElement reportValidity setCustomValidity type "
        "validationMessage validity value willValidate',"
        "  HTMLCanvasElement: 'captureStream getContext height toBlob toDataURL "
        "transferControlToOffscreen width',"
        "  HTMLDListElement: 'compact',"
        "  HTMLDataElement: 'value',"
        "  HTMLDataListElement: 'options',"
        "  HTMLDetailsElement: 'name open',"
        "  HTMLDialogElement: 'close closedBy open requestClose returnValue show showModal',"
        "  HTMLDirectoryElement: 'compact',"
        "  HTMLDivElement: 'align',"
        "  HTMLEmbedElement: 'align getSVGDocument height name src type width',"
        "  HTMLFieldSetElement: 'checkValidity disabled elements form name reportValidity "
        "setCustomValidity type validationMessage validity willValidate',"
        "  HTMLFontElement: 'color face size',"
        "  HTMLFormElement: 'acceptCharset action autocomplete checkValidity elements encoding "
        "enctype length method name noValidate rel relList reportValidity "
        "requestSubmit reset submit target',"
        "  HTMLFrameElement: 'contentDocument contentWindow frameBorder longDesc marginHeight "
        "marginWidth name noResize scrolling src',"
        "  HTMLFrameSetElement: 'cols onafterprint onbeforeprint onbeforeunload onblur onerror "
        "onfocus ongamepadconnected ongamepaddisconnected onhashchange "
        "onlanguagechange onload onmessage onmessageerror onoffline "
        "ononline onpagehide onpageshow onpopstate onrejectionhandled "
        "onresize onscroll onstorage onunhandledrejection onunload rows',"
        "  HTMLHRElement: 'align color noShade size width',"
        "  HTMLHeadingElement: 'align',"
        "  HTMLHtmlElement: 'version',"
        "  HTMLIFrameElement: 'adAuctionHeaders align allow allowFullscreen allowPaymentRequest "
        "browsingTopics contentDocument contentWindow credentialless csp "
        "featurePolicy frameBorder getSVGDocument height loading longDesc "
        "marginHeight marginWidth name privateToken referrerPolicy sandbox "
        "scrolling src srcdoc width',"
        "  HTMLImageElement: 'align alt attributionSrc border browsingTopics complete "
        "crossOrigin currentSrc decode decoding fetchPriority height hspace "
        "isMap loading longDesc lowsrc name naturalHeight naturalWidth "
        "referrerPolicy sizes src srcset useMap vspace width x y',"
        "  HTMLInputElement: 'accept align alt autocomplete checkValidity checked "
        "createValueRange defaultChecked defaultValue dirName disabled "
        "files form formAction formEnctype formMethod formNoValidate "
        "formTarget height incremental indeterminate labels list max "
        "maxLength min minLength multiple name pattern placeholder "
        "popoverTargetAction popoverTargetElement readOnly reportValidity "
        "required select selectionDirection selectionEnd selectionStart "
        "setCustomValidity setRangeText setSelectionRange showPicker size "
        "src step stepDown stepUp type useMap validationMessage validity "
        "value valueAsDate valueAsNumber webkitEntries webkitdirectory "
        "width willValidate',"
        "  HTMLLIElement: 'type value',"
        "  HTMLLabelElement: 'control form htmlFor',"
        "  HTMLLegendElement: 'align form',"
        "  HTMLLinkElement: 'as blocking charset crossOrigin disabled fetchPriority href "
        "hreflang imageSizes imageSrcset integrity media referrerPolicy rel "
        "relList rev sheet sizes target type',"
        "  HTMLMapElement: 'areas name',"
        "  HTMLMarqueeElement: 'behavior bgColor direction height hspace loop scrollAmount "
        "scrollDelay start stop trueSpeed vspace width',"
        "  HTMLMediaElement: 'HAVE_CURRENT_DATA HAVE_ENOUGH_DATA HAVE_FUTURE_DATA HAVE_METADATA "
        "HAVE_NOTHING NETWORK_EMPTY NETWORK_IDLE NETWORK_LOADING "
        "NETWORK_NO_SOURCE addTextTrack autoplay buffered canPlayType "
        "captureStream controls controlsList crossOrigin currentSrc "
        "currentTime defaultMuted defaultPlaybackRate disableRemotePlayback "
        "duration ended error load loading loop mediaKeys muted "
        "networkState onencrypted onwaitingforkey pause paused play "
        "playbackRate played preload preservesPitch readyState remote "
        "seekable seeking setMediaKeys setSinkId sinkId src srcObject "
        "textTracks volume webkitAudioDecodedByteCount "
        "webkitVideoDecodedByteCount',"
        "  HTMLMenuElement: 'compact',"
        "  HTMLMetaElement: 'content httpEquiv media name scheme',"
        "  HTMLMeterElement: 'high labels low max min optimum value',"
        "  HTMLModElement: 'cite dateTime',"
        "  HTMLOListElement: 'compact reversed start type',"
        "  HTMLObjectElement: 'align archive border checkValidity code codeBase codeType "
        "contentDocument contentWindow data declare form getSVGDocument "
        "height hspace name reportValidity setCustomValidity standby type "
        "useMap validationMessage validity vspace width willValidate',"
        "  HTMLOptGroupElement: 'disabled label',"
        "  HTMLOptionElement: 'defaultSelected disabled form index label selected text value',"
        "  HTMLOutputElement: 'checkValidity defaultValue form htmlFor labels name reportValidity "
        "setCustomValidity type validationMessage validity value "
        "willValidate',"
        "  HTMLParagraphElement: 'align',"
        "  HTMLParamElement: 'name type value valueType',"
        "  HTMLPreElement: 'width',"
        "  HTMLProgressElement: 'labels max position value',"
        "  HTMLQuoteElement: 'cite',"
        "  HTMLScriptElement: 'async attributionSrc blocking charset crossOrigin defer event "
        "fetchPriority htmlFor innerText integrity noModule referrerPolicy "
        "src text textContent type',"
        "  HTMLSelectElement: 'add autocomplete checkValidity disabled form item labels length "
        "multiple name namedItem options remove reportValidity required "
        "selectedIndex selectedOptions setCustomValidity showPicker size "
        "type validationMessage validity value willValidate',"
        "  HTMLSlotElement: 'assign assignedElements assignedNodes name',"
        "  HTMLSourceElement: 'height media sizes src srcset type width',"
        "  HTMLStyleElement: 'blocking disabled media sheet type',"
        "  HTMLTableCaptionElement: 'align',"
        "  HTMLTableCellElement: 'abbr align axis bgColor cellIndex ch chOff colSpan headers height "
        "noWrap rowSpan scope vAlign width',"
        "  HTMLTableColElement: 'align ch chOff span vAlign width',"
        "  HTMLTableElement: 'align bgColor border caption cellPadding cellSpacing createCaption "
        "createTBody createTFoot createTHead deleteCaption deleteRow "
        "deleteTFoot deleteTHead frame insertRow rows rules summary tBodies "
        "tFoot tHead width',"
        "  HTMLTableRowElement: 'align bgColor cells ch chOff deleteCell insertCell rowIndex "
        "sectionRowIndex vAlign',"
        "  HTMLTableSectionElement: 'align ch chOff deleteRow insertRow rows vAlign',"
        "  HTMLTemplateElement: 'content htmlFor shadowRootClonable shadowRootCustomElementRegistry "
        "shadowRootDelegatesFocus shadowRootMode shadowRootReferenceTarget "
        "shadowRootSerializable shadowRootSlotAssignment',"
        "  HTMLTextAreaElement: 'autocomplete checkValidity cols createValueRange defaultValue "
        "dirName disabled form labels maxLength minLength name placeholder "
        "readOnly reportValidity required rows select selectionDirection "
        "selectionEnd selectionStart setCustomValidity setRangeText "
        "setSelectionRange textLength type validationMessage validity value "
        "willValidate wrap',"
        "  HTMLTimeElement: 'dateTime',"
        "  HTMLTitleElement: 'text',"
        "  HTMLTrackElement: 'ERROR LOADED LOADING NONE default kind label readyState src "
        "srclang track',"
        "  HTMLUListElement: 'compact type',"
        "  HTMLVideoElement: 'cancelVideoFrameCallback disablePictureInPicture "
        "getVideoPlaybackQuality height onenterpictureinpicture "
        "onleavepictureinpicture playsInline poster requestPictureInPicture "
        "requestVideoFrameCallback videoHeight videoWidth "
        "webkitDecodedFrameCount webkitDroppedFrameCount width',"
        "  SVGSVGElement: 'SVG_ZOOMANDPAN_DISABLE SVG_ZOOMANDPAN_MAGNIFY "
        "SVG_ZOOMANDPAN_UNKNOWN animationsPaused checkEnclosure "
        "checkIntersection createSVGAngle createSVGLength createSVGMatrix "
        "createSVGNumber createSVGPoint createSVGRect createSVGTransform "
        "createSVGTransformFromMatrix currentScale currentTranslate "
        "deselectAll forceRedraw getCurrentTime getElementById "
        "getEnclosureList getIntersectionList height pauseAnimations "
        "preserveAspectRatio setCurrentTime suspendRedraw unpauseAnimations "
        "unsuspendRedraw unsuspendRedrawAll viewBox width x y zoomAndPan',"
        "  SVGAElement: 'download href hreflang interestForElement ping referrerPolicy rel "
        "relList target type',"
    "  };"
    "  var E = G.Element && G.Element.prototype, H = G.HTMLElement && G.HTMLElement.prototype;"
    "  if (!E) return;"
    "  var own = Object.prototype.hasOwnProperty;"
    "  function down(n, srcs){"
    "    var C = G[n]; if (typeof C !== 'function' || !C.prototype || !W[n]) return;"
    "    var P = C.prototype;"
    "    W[n].split(' ').forEach(function(k){"
    "      if (!k || own.call(P, k)) return;"
    "      srcs.some(function(S){"
    "        var d = S && S !== P && Object.getOwnPropertyDescriptor(S, k);"
    "        if (d) { try { Object.defineProperty(P, k, d); } catch (e) {} }"
    "        return !!d;"
    "      });"
    "    });"
    "  }"
    "  Object.keys(W).forEach(function(n){"
    "    if (n !== 'HTMLElement' && /^HTML.+Element$/.test(n)) down(n, [E, H]);"
    "  });"
    "  down('HTMLElement', [E]);"
    "  down('SVGElement', [E, H]);"
    "  down('MathMLElement', [E, H]);"
    "  down('SVGSVGElement', [E]);"
    "  down('SVGAElement', [E]);"
    "  ['Element', 'HTMLElement'].forEach(function(n){"
    "    var C = G[n]; if (typeof C !== 'function' || !C.prototype) return;"
    "    var P = C.prototype, want = new Set(W[n].split(' '));"
    "    Object.getOwnPropertyNames(P).forEach(function(k){"
    "      if (k === 'constructor' || want.has(k) || k.slice(0, 8) === '__shady_') return;"
    "      try { delete P[k]; } catch (e) {}"
    "    });"
    "  });"
    "})(globalThis)";

static void
ns_install_element_shapes(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_element_shapes_src,
                        sizeof(ns_element_shapes_src) - 1, "<element-shapes>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

static void
ns_install_web_api_shapes(JSContext *ctx, JSValueConst global)
{
    static const char *const source =
        "(function(){"
        " function normalize(name,obj,tag,evented){"
        "  var C=globalThis[name];"
        "  if(typeof C!=='function'||!C.prototype||!obj)return;"
        "  var P=C.prototype;"
        "  if(evented&&typeof EventTarget==='function'&&EventTarget.prototype)"
        "   try{Object.setPrototypeOf(P,EventTarget.prototype);}catch(e){}"
        "  Object.getOwnPropertyNames(obj).forEach(function(k){"
        "   if(k[0]==='_'||k==='constructor')return;"
        "   var d=Object.getOwnPropertyDescriptor(obj,k);if(!d||!d.configurable)return;"
        "   if(Object.prototype.hasOwnProperty.call(P,k)){try{delete obj[k];}catch(e){}return;}"
        "   if(d.get||d.set||typeof d.value==='function'){"
        "    try{Object.defineProperty(P,k,d);delete obj[k];}catch(e){}return;"
        "   }"
        "   (function(value,writable,enumerable){"
        "    var holder={get value(){return value;},set value(v){value=v;}};"
        "    var hd=Object.getOwnPropertyDescriptor(holder,'value');"
        "    try{Object.defineProperty(P,k,{configurable:true,enumerable:enumerable,"
        "     get:hd.get,set:writable?hd.set:undefined});delete obj[k];}catch(e){}"
        "   })(d.value,d.writable,d.enumerable);"
        "  });"
        "  try{Object.setPrototypeOf(obj,P);}catch(e){}"
        "  try{delete obj[Symbol.toStringTag];}catch(e){}"
        "  try{Object.defineProperty(P,Symbol.toStringTag,{value:tag||name,configurable:true});}catch(e){}"
        " }"
        " normalize('History',globalThis.history,'History',false);"
        " normalize('Performance',globalThis.performance,'Performance',true);"
        " normalize('Screen',globalThis.screen,'Screen',true);"
        " normalize('Crypto',globalThis.crypto,'Crypto',false);"
        " normalize('SubtleCrypto',globalThis.crypto&&globalThis.crypto.subtle,'SubtleCrypto',false);"
        " normalize('Permissions',globalThis.navigator&&navigator.permissions,'Permissions',false);"
        " normalize('NetworkInformation',globalThis.navigator&&navigator.connection,'NetworkInformation',true);"
        " normalize('NavigatorUAData',globalThis.navigator&&navigator.userAgentData,'NavigatorUAData',false);"
        " normalize('PluginArray',globalThis.navigator&&navigator.plugins,'PluginArray',false);"
        " normalize('MimeTypeArray',globalThis.navigator&&navigator.mimeTypes,'MimeTypeArray',false);"
        " normalize('MediaDevices',globalThis.navigator&&navigator.mediaDevices,'MediaDevices',true);"
        " normalize('MediaCapabilities',globalThis.navigator&&navigator.mediaCapabilities,'MediaCapabilities',false);"
        " normalize('UserActivation',globalThis.navigator&&navigator.userActivation,'UserActivation',false);"
        " normalize('StorageManager',globalThis.navigator&&navigator.storage,'StorageManager',false);"
        " normalize('WakeLock',globalThis.navigator&&navigator.wakeLock,'WakeLock',false);"
        " var PS=globalThis.PermissionStatus&&PermissionStatus.prototype;"
        " if(PS&&typeof EventTarget==='function'&&EventTarget.prototype)"
        "  try{Object.setPrototypeOf(PS,EventTarget.prototype);"
        "   Object.defineProperty(PS,Symbol.toStringTag,{value:'PermissionStatus',configurable:true});}catch(e){}"
        "})()";
    JSValue result = JS_Eval(ctx, source, strlen(source),
                             "<web-api-shapes>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(result)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, result);
    (void)global;
}

/* HTML's PDF viewer plugins: when the user agent views PDFs itself
 * (pdfViewerEnabled), navigator.plugins holds the five PDF viewer plugin
 * objects and navigator.mimeTypes the two PDF MIME types, as the standard
 * prescribes for every browser. */
static const char ns_pdf_plugins_src[] =
    "(function(){"
    " if (typeof PluginArray !== 'function' || typeof Plugin !== 'function' ||"
    "     typeof MimeType !== 'function' || typeof MimeTypeArray !== 'function') return;"
    " var nav = navigator; if (!nav || nav.pdfViewerEnabled !== true) return;"
    " var pa = nav.plugins, ma = nav.mimeTypes;"
    " if (!pa || !ma || typeof pa !== 'object' || typeof ma !== 'object') return;"
    " var dp = Object.defineProperty, gopd = Object.getOwnPropertyDescriptor, st = new WeakMap();"
    " function state(o){ var s = st.get(o); if (!s) throw new TypeError('Illegal invocation'); return s; }"
    " function getter(P, name, f){"
    "  var h = { get [name](){ return f(state(this)); } };"
    "  dp(P, name, { get: gopd(h, name).get, enumerable: true, configurable: true }); }"
    " function need(n, iface, op){"
    "  if (n < 1) throw new TypeError(\"Failed to execute '\" + op + \"' on '\" + iface +"
    "    \"': 1 argument required, but only 0 present.\"); }"
    " function list(P, iface, refresh){"
    "  var m = { item(index){ var s = state(this); need(arguments.length, iface, 'item');"
    "      var i = index >>> 0; return i < s.items.length ? s.items[i] : null; },"
    "    namedItem(name){ var s = state(this); need(arguments.length, iface, 'namedItem');"
    "      name = String(name); for (var i = 0; i < s.items.length; i++)"
    "        if (s.key(s.items[i]) === name) return s.items[i]; return null; } };"
    "  dp(P, 'item', { value: m.item, writable: true, enumerable: true, configurable: true });"
    "  dp(P, 'namedItem', { value: m.namedItem, writable: true, enumerable: true, configurable: true });"
    "  if (refresh) dp(P, 'refresh', { value: { refresh(){ state(this); } }.refresh,"
    "    writable: true, enumerable: true, configurable: true });"
    "  getter(P, 'length', function(s){ return s.items.length; });"
    "  dp(P, Symbol.iterator, { value: Array.prototype.values, writable: true, configurable: true }); }"
    " function fill(o, items, key){"
    "  Object.getOwnPropertyNames(o).forEach(function(k){ try { delete o[k]; } catch (e) {} });"
    "  st.set(o, { items: items, key: key });"
    "  items.forEach(function(it, i){ dp(o, i, { value: it, writable: false, enumerable: true, configurable: true }); });"
    "  items.forEach(function(it){ var k = key(it); if (!(k in o))"
    "    dp(o, k, { value: it, writable: false, enumerable: false, configurable: true }); }); }"
    " var names = ['PDF Viewer', 'Chrome PDF Viewer', 'Chromium PDF Viewer',"
    "              'Microsoft Edge PDF Viewer', 'WebKit built-in PDF'];"
    " var types = ['application/pdf', 'text/pdf'];"
    " function mimesFor(plugin){ return types.map(function(t){"
    "   var m = Object.create(MimeType.prototype); st.set(m, { type: t, plugin: plugin }); return m; }); }"
    " var plugins = names.map(function(n){ return Object.create(Plugin.prototype); });"
    " var mimes = mimesFor(plugins[0]);"
    " plugins.forEach(function(p, i){ fill(p, mimesFor(p), function(m){ return st.get(m).type; });"
    "   st.get(p).name = names[i]; });"
    " fill(pa, plugins, function(p){ return st.get(p).name; });"
    " fill(ma, mimes, function(m){ return st.get(m).type; });"
    " list(PluginArray.prototype, 'PluginArray', true);"
    " list(MimeTypeArray.prototype, 'MimeTypeArray', false);"
    " list(Plugin.prototype, 'Plugin', false);"
    " getter(Plugin.prototype, 'name', function(s){ return s.name; });"
    " getter(Plugin.prototype, 'description', function(){ return 'Portable Document Format'; });"
    " getter(Plugin.prototype, 'filename', function(){ return 'internal-pdf-viewer'; });"
    " getter(MimeType.prototype, 'type', function(s){ return s.type; });"
    " getter(MimeType.prototype, 'description', function(){ return 'Portable Document Format'; });"
    " getter(MimeType.prototype, 'suffixes', function(){ return 'pdf'; });"
    " getter(MimeType.prototype, 'enabledPlugin', function(s){ return s.plugin; });"
    "})()";

static void
ns_install_pdf_plugins(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_pdf_plugins_src, sizeof(ns_pdf_plugins_src) - 1,
                        "<pdf-plugins>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

static void
ns_install_navigator_shape(JSContext *ctx)
{
    /* The getters accept the window's navigator and the navigators in the
     * engine's set (see <navigator-iface>), each frame realm's own. */
    static const char *const source =
        "(function(others){"
        " if(typeof Navigator!=='function'||typeof navigator!=='object'||!navigator)return;"
        " var nav=navigator,P=Navigator.prototype;"
        " if(!(others instanceof WeakSet))others=new WeakSet();"
        /* An attribute whose value is an object ([SameObject] in WebIDL) is
         * each navigator's own: a frame's navigator gets objects of its own
         * realm (ns_realm_install_singletons hands them over). */
        " var slots=new WeakMap(),mine=Object.create(null);slots.set(nav,mine);"
        " function objectGetter(name){"
        "  var holder={get [name](){var m=slots.get(this);if(!m)throw new TypeError('Illegal invocation');return m[name];}};"
        "  return Object.getOwnPropertyDescriptor(holder,name).get;"
        " }"
        " Object.getOwnPropertyNames(nav).forEach(function(name){"
        "  if(name[0]==='_')return;"
        "  var d=Object.getOwnPropertyDescriptor(nav,name);"
        "  if(!d||!d.configurable)return;"
        "  if(Object.prototype.hasOwnProperty.call(P,name)){try{delete nav[name];}catch(e){}return;}"
        "  if(typeof d.value==='function'){"
        "   try{Object.defineProperty(P,name,{value:d.value,writable:true,enumerable:true,configurable:true});delete nav[name];}catch(e){}"
        "   return;"
        "  }"
        "  if(d.value&&typeof d.value==='object'){"
        "   mine[name]=d.value;"
        "   try{Object.defineProperty(P,name,{get:objectGetter(name),enumerable:true,configurable:true});delete nav[name];}catch(e){}"
        "   return;"
        "  }"
        "  (function(value){"
        "   var holder={get value(){if(this!==nav&&!others.has(this))throw new TypeError('Illegal invocation');return value;}};"
        "   var get=Object.getOwnPropertyDescriptor(holder,'value').get;"
        "   try{Object.defineProperty(P,name,{get:get,enumerable:true,configurable:true});delete nav[name];}catch(e){}"
        "  })(d.value);"
        " });"
        " Object.getOwnPropertyNames(P).forEach(function(name){"
        "  if(name==='constructor')return;"
        "  var d=Object.getOwnPropertyDescriptor(P,name);"
        "  if(!d||!d.configurable||!('value' in d)||!d.value||typeof d.value!=='object')return;"
        "  mine[name]=d.value;"
        "  try{Object.defineProperty(P,name,{get:objectGetter(name),enumerable:true,configurable:true});}catch(e){}"
        " });"
        " Object.defineProperty(others,'navigatorObjects',{value:function(){return mine;}});"
        " Object.defineProperty(others,'adoptNavigatorObjects',{value:function(n,values){slots.set(n,values);}});"
        " try{Object.setPrototypeOf(nav,P);}catch(e){}"
        " try{Object.defineProperty(P,Symbol.toStringTag,{value:'Navigator',configurable:true});}catch(e){}"
        "})";
    JSValue fn = JS_Eval(ctx, source, strlen(source),
                         "<navigator-shape>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(fn)) {
        JS_FreeValue(ctx, JS_GetException(ctx));
        return;
    }
    ns_js *js = js_from_ctx(ctx);
    JSValueConst args[1] = { js ? js->navigator_brand : JS_UNDEFINED };
    JSValue result = JS_Call(ctx, fn, JS_UNDEFINED, 1, args);
    if (JS_IsException(result)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, result);
    JS_FreeValue(ctx, fn);
}

static void
ns_set_ctor_proto(JSContext *ctx, JSValueConst global, const char *ctor_name,
                  JSValueConst proto)
{
    JSValue ctor = JS_GetPropertyStr(ctx, global, ctor_name);
    if (JS_IsObject(ctor) && JS_IsObject(proto)) {
        JS_DefinePropertyValueStr(ctx, ctor, "prototype",
            JS_DupValue(ctx, proto), JS_PROP_WRITABLE);
        JS_DefinePropertyValueStr(ctx, (JSValue)proto, "constructor",
            JS_DupValue(ctx, ctor), JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
    }
    JS_FreeValue(ctx, ctor);
}

static void
ns_proto_delete_names(JSContext *ctx, JSValueConst proto,
                      const char *const *names, gsize n)
{
    if (!JS_IsObject(proto)) return;
    for (gsize i = 0; i < n; i++) {
        JSAtom a = JS_NewAtom(ctx, names[i]);
        JS_DeleteProperty(ctx, proto, a, 0);
        JS_FreeAtom(ctx, a);
    }
}

static const char *const ns_element_only_methods[] = {
    "matches", "closest", "webkitMatchesSelector",
};

static const char *const ns_parent_query_methods[] = {
    "querySelector", "querySelectorAll",
};

static void
ns_proto_define_getset(JSContext *ctx, JSValueConst proto, const char *name,
                       JSValue (*getter)(JSContext *, JSValueConst),
                       JSValue (*setter)(JSContext *, JSValueConst,
                                         JSValueConst))
{
    JSAtom atom = JS_NewAtom(ctx, name);
    JSValue g = JS_NewCFunction2(ctx, (JSCFunction *)(void *)getter, name, 0,
                                 JS_CFUNC_getter, 0);
    JSValue s = setter
        ? JS_NewCFunction2(ctx, (JSCFunction *)(void *)setter, name, 1,
                           JS_CFUNC_setter, 0)
        : JS_UNDEFINED;
    JS_DefinePropertyGetSet(ctx, proto, atom, g, s,
                            JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
    JS_FreeAtom(ctx, atom);
}

/* MathMLElement extends Element and has the dataset of HTMLOrSVGElement; the
 * rest of its members are moved down from Element by the element shapes. */
static JSValue
ns_install_mathml_proto(JSContext *ctx, JSValueConst global,
                        JSValueConst elem_proto)
{
    ns_chain_proto(ctx, global, "MathMLElement", elem_proto);
    JSValue proto = ns_proto_of(ctx, global, "MathMLElement");
    if (JS_IsObject(proto))
        ns_proto_define_getset(ctx, proto, "dataset",
                               ns_element_get_dataset, NULL);
    return proto;
}

static void
ns_install_dom_hierarchy(ns_js *js, JSContext *ctx, JSValueConst global)
{
    JSValue node_proto = ns_proto_of(ctx, global, "Node");
    if (!JS_IsObject(node_proto)) { JS_FreeValue(ctx, node_proto); return; }

    JSValue elem_proto = JS_NewObject(ctx);
    JS_SetPrototype(ctx, elem_proto, node_proto);
    JS_SetPropertyFunctionList(ctx, elem_proto, ns_element_proto_funcs,
                               G_N_ELEMENTS(ns_element_proto_funcs));
    ns_define_element_unscopables(ctx, elem_proto);
    JS_SetClassProto(ctx, ns_element_class_id, JS_DupValue(ctx, elem_proto));
    ns_bind_fn(ctx, elem_proto, "matches",               ns_element_matches, 1);
    ns_bind_fn(ctx, elem_proto, "webkitMatchesSelector", ns_element_matches, 1);
    ns_bind_fn(ctx, elem_proto, "closest",               ns_element_closest, 1);

    static const char *const element_spec_methods[] = {
        "setAttribute", "getAttribute", "hasAttribute", "removeAttribute",
        "toggleAttribute", "getAttributeNS", "setAttributeNS",
        "removeAttributeNS", "hasAttributeNS", "getAttributeNames",
        "attachShadow", "querySelector", "querySelectorAll",
        "getElementsByTagName", "getElementsByClassName",
        "insertAdjacentElement", "insertAdjacentHTML", "insertAdjacentText",
        "getBoundingClientRect", "getClientRects", "scrollIntoView",
        "append", "prepend", "before", "after", "remove", "replaceWith",
        "replaceChildren",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(element_spec_methods); i++) {
        JSValue fn = JS_GetPropertyStr(ctx, node_proto,
                                       element_spec_methods[i]);
        if (JS_IsFunction(ctx, fn))
            JS_DefinePropertyValueStr(ctx, elem_proto,
                                      element_spec_methods[i], fn,
                                      JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
        else
            JS_FreeValue(ctx, fn);
    }

    static const char *const element_spec_accessors[] = {
        "children", "firstElementChild", "lastElementChild",
        "childElementCount", "nextElementSibling", "previousElementSibling",
        "innerHTML", "outerHTML", "id", "className", "classList",
        "attributes", "tagName", "localName", "namespaceURI",
        "scrollTop", "scrollLeft", "scrollWidth", "scrollHeight",
        "clientTop", "clientLeft", "clientWidth", "clientHeight",
        "shadowRoot", "assignedSlot",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(element_spec_accessors); i++) {
        JSAtom atom = JS_NewAtom(ctx, element_spec_accessors[i]);
        JSPropertyDescriptor d;
        int r = JS_GetOwnProperty(ctx, &d, node_proto, atom);
        if (r > 0) {
            if (d.flags & JS_PROP_GETSET)
                JS_DefinePropertyGetSet(ctx, elem_proto, atom,
                                        d.getter, d.setter,
                                        JS_PROP_CONFIGURABLE);
            else
                JS_DefinePropertyValue(ctx, elem_proto, atom, d.value,
                                       JS_PROP_WRITABLE |
                                       JS_PROP_CONFIGURABLE);
        }
        JS_FreeAtom(ctx, atom);
    }

    ns_proto_delete_names(ctx, node_proto, ns_element_only_methods,
                          G_N_ELEMENTS(ns_element_only_methods));
    ns_proto_delete_names(ctx, node_proto, ns_parent_query_methods,
                          G_N_ELEMENTS(ns_parent_query_methods));
    static const char *const doc_like[] = {
        "Document", "HTMLDocument", "XMLDocument", "DocumentFragment",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(doc_like); i++) {
        JSValue p = ns_proto_of(ctx, global, doc_like[i]);
        ns_proto_delete_names(ctx, p, ns_element_only_methods,
                              G_N_ELEMENTS(ns_element_only_methods));
        JS_FreeValue(ctx, p);
    }

    {
        JSValue move_before = JS_GetPropertyStr(ctx, node_proto, "moveBefore");
        if (JS_IsFunction(ctx, move_before)) {
            for (gsize i = 0; i < G_N_ELEMENTS(doc_like); i++) {
                JSValue p = ns_proto_of(ctx, global, doc_like[i]);
                if (JS_IsObject(p))
                    JS_DefinePropertyValueStr(ctx, p, "moveBefore",
                                              JS_DupValue(ctx, move_before),
                                              JS_PROP_C_W_E);
                JS_FreeValue(ctx, p);
            }
        }
        JS_FreeValue(ctx, move_before);
        static const char *const parentnode_only_methods[] = { "moveBefore" };
        ns_proto_delete_names(ctx, node_proto, parentnode_only_methods,
                              G_N_ELEMENTS(parentnode_only_methods));
    }

    ns_set_ctor_proto(ctx, global, "Node", node_proto);
    ns_set_ctor_proto(ctx, global, "Element", elem_proto);

    JSValue htmlelem_proto = JS_NewObject(ctx);
    JS_SetPrototype(ctx, htmlelem_proto, elem_proto);
    ns_set_ctor_proto(ctx, global, "HTMLElement", htmlelem_proto);
    ns_proto_define_getset(ctx, htmlelem_proto, "dataset",
                           ns_element_get_dataset, NULL);
    ns_proto_define_getset(ctx, htmlelem_proto, "popover",
                           ns_element_get_popover, ns_element_set_popover);
    ns_chain_proto(ctx, global, "SVGElement", elem_proto);
    ns_chain_proto(ctx, global, "SVGSVGElement", elem_proto);
    JSValue svg_proto = ns_proto_of(ctx, global, "SVGElement");
    ns_chain_proto(ctx, global, "SVGAElement", svg_proto);
    JSValue svga_proto = ns_proto_of(ctx, global, "SVGAElement");
    if (JS_IsObject(svg_proto))
        ns_proto_define_getset(ctx, svg_proto, "dataset",
                               ns_element_get_dataset, NULL);
    JSValue mathml_proto = ns_install_mathml_proto(ctx, global, elem_proto);

    JSValue chardata_proto = ns_proto_of(ctx, global, "CharacterData");
    if (JS_IsObject(chardata_proto)) {
        JS_SetPrototype(ctx, chardata_proto, node_proto);
        ns_chain_proto(ctx, global, "Text", chardata_proto);
        ns_chain_proto(ctx, global, "Comment", chardata_proto);
        ns_chain_proto(ctx, global, "ProcessingInstruction", chardata_proto);
    }
    JSValue text_proto    = ns_proto_of(ctx, global, "Text");
    JSValue comment_proto = ns_proto_of(ctx, global, "Comment");
    JSValue pi_proto      = ns_proto_of(ctx, global, "ProcessingInstruction");
    JSValue cdata_proto   = ns_proto_of(ctx, global, "CDATASection");
    JSValue doctype_proto = ns_proto_of(ctx, global, "DocumentType");
    JSValue docfrag_proto = ns_proto_of(ctx, global, "DocumentFragment");
    if (JS_IsObject(text_proto)) ns_chain_proto(ctx, global, "CDATASection", text_proto);
    if (JS_IsObject(text_proto))
        ns_proto_define_getset(ctx, text_proto, "assignedSlot",
                               ns_element_get_assignedSlot,
                               ns_element_noop_set);
    if (JS_IsObject(doctype_proto)) JS_SetPrototype(ctx, doctype_proto, node_proto);
    if (JS_IsObject(docfrag_proto)) {
        JS_SetPrototype(ctx, docfrag_proto, node_proto);
        ns_bind_fn(ctx, docfrag_proto, "querySelector",
                   ns_element_querySelector, 1);
        ns_bind_fn(ctx, docfrag_proto, "querySelectorAll",
                   ns_element_querySelectorAll, 1);
    }
    JSValue shadow_proto = ns_proto_of(ctx, global, "ShadowRoot");
    if (JS_IsObject(shadow_proto) && JS_IsObject(docfrag_proto))
        JS_SetPrototype(ctx, shadow_proto, docfrag_proto);
    JS_FreeValue(ctx, shadow_proto);

    static const char *const misplaced_element_members[] = {
        "attachShadow", "shadowRoot", "assignedSlot", "default",
    };
    ns_proto_delete_names(ctx, node_proto, misplaced_element_members,
                          G_N_ELEMENTS(misplaced_element_members));
    for (gsize i = 0; i < G_N_ELEMENTS(doc_like); i++) {
        JSValue p = ns_proto_of(ctx, global, doc_like[i]);
        ns_proto_delete_names(ctx, p, misplaced_element_members,
                              G_N_ELEMENTS(misplaced_element_members));
        JS_FreeValue(ctx, p);
    }

    JSValue doc_proto = ns_proto_of(ctx, global, "Document");
    if (JS_IsObject(doc_proto)) JS_SetPrototype(ctx, doc_proto, node_proto);
    JSValue htmldoc_proto = ns_proto_of(ctx, global, "HTMLDocument");
    if (JS_IsObject(htmldoc_proto) && JS_IsObject(doc_proto))
        JS_SetPrototype(ctx, htmldoc_proto, doc_proto);
    js->proto_document = JS_IsObject(htmldoc_proto) ? htmldoc_proto
                                                    : JS_DupValue(ctx, doc_proto);
    if (JS_IsObject(htmldoc_proto)) JS_FreeValue(ctx, doc_proto);

    js->proto_node        = node_proto;
    js->proto_element     = elem_proto;
    js->proto_htmlelement = htmlelem_proto;
    js->proto_svgelement  = svg_proto;
    js->proto_svgaelement = svga_proto;
    js->proto_mathmlelement = mathml_proto;
    js->proto_chardata    = chardata_proto;
    js->proto_text        = text_proto;
    js->proto_comment     = comment_proto;
    js->proto_pi          = pi_proto;
    js->proto_cdata       = cdata_proto;
    js->proto_doctype     = doctype_proto;
    js->proto_docfrag     = docfrag_proto;
    JSValue unknownelem_proto = ns_proto_of(ctx, global, "HTMLUnknownElement");
    if (JS_IsObject(unknownelem_proto)) JS_SetPrototype(ctx, unknownelem_proto, htmlelem_proto);
    js->proto_htmlunknownelement = unknownelem_proto;
    JSValue attr_proto = ns_proto_of(ctx, global, "Attr");
    if (JS_IsObject(attr_proto)) JS_SetPrototype(ctx, attr_proto, node_proto);
    JS_FreeValue(ctx, attr_proto);

    {
        static const JSCFunctionListEntry track_accessors[] = {
            JS_CGETSET_MAGIC_DEF("default", ns_element_bool_attr_getter,
                                 ns_element_bool_attr_setter, 3),
        };
        JSValue track_proto = ns_proto_of(ctx, global, "HTMLTrackElement");
        if (JS_IsObject(track_proto))
            JS_SetPropertyFunctionList(ctx, track_proto, track_accessors,
                                       G_N_ELEMENTS(track_accessors));
        JS_FreeValue(ctx, track_proto);
    }
    {
        JSValue dialog_proto = ns_proto_of(ctx, global, "HTMLDialogElement");
        if (JS_IsObject(dialog_proto))
            ns_proto_define_getset(ctx, dialog_proto, "closedBy",
                                   ns_dialog_get_closedBy,
                                   ns_dialog_set_closedBy);
        JS_FreeValue(ctx, dialog_proto);
    }
    {
        JSValue button_proto = ns_proto_of(ctx, global, "HTMLButtonElement");
        if (JS_IsObject(button_proto)) {
            ns_proto_define_getset(ctx, button_proto, "command",
                                   ns_button_get_command,
                                   ns_button_set_command);
            ns_proto_define_getset(ctx, button_proto, "commandForElement",
                                   ns_button_get_commandForElement,
                                   ns_button_set_commandForElement);
        }
        JS_FreeValue(ctx, button_proto);
    }

    js->per_tag_protos = g_hash_table_new_full(g_str_hash, g_str_equal,
                                               g_free, g_free);
    static const struct {
        const char *tag;
        gboolean has_text, has_data, has_value;
    } tag_props[] = {
        { "option",   TRUE,  FALSE, TRUE  },
        { "a",        TRUE,  FALSE, FALSE },
        { "script",   TRUE,  FALSE, FALSE },
        { "title",    TRUE,  FALSE, FALSE },
        { "object",   FALSE, TRUE,  TRUE  },
        { "input",    FALSE, FALSE, TRUE  },
        { "select",   FALSE, FALSE, TRUE  },
        { "textarea", FALSE, FALSE, TRUE  },
        { "button",   FALSE, FALSE, TRUE  },
        { "output",   FALSE, FALSE, TRUE  },
        { "progress", FALSE, FALSE, TRUE  },
        { "meter",    FALSE, FALSE, TRUE  },
        { "li",       FALSE, FALSE, TRUE  },
        { "param",    FALSE, FALSE, TRUE  },
        { "data",     FALSE, FALSE, TRUE  },
    };
    for (gsize i = 0; i < G_N_ELEMENTS(ns_instof_table); i++) {
        const ns_instof_def *d = &ns_instof_table[i];
        if (d->special != NS_INSTOF_TAG || !d->tags) continue;
        JSValue ctor = JS_GetPropertyStr(ctx, global, d->ctor);
        JSValue proto = JS_IsObject(ctor)
            ? JS_GetPropertyStr(ctx, ctor, "prototype") : JS_UNDEFINED;
        JS_FreeValue(ctx, ctor);
        if (JS_IsObject(proto)) {
            JS_SetPrototype(ctx, proto, htmlelem_proto);
            char **tags = g_strsplit(d->tags, " ", -1);
            for (gsize t = 0; tags[t]; t++) {
                if (!*tags[t]) continue;
                JSValue *old = g_hash_table_lookup(js->per_tag_protos, tags[t]);
                if (old) JS_FreeValue(ctx, *old);
                JSValue *entry = g_new(JSValue, 1);
                *entry = JS_DupValue(ctx, proto);
                g_hash_table_insert(js->per_tag_protos,
                                    g_strdup(tags[t]), entry);
            }
            g_strfreev(tags);
        }
        JS_FreeValue(ctx, proto);
    }

    JSValue media_proto = ns_proto_of(ctx, global, "HTMLMediaElement");
    if (JS_IsObject(media_proto)) {
        JS_SetPrototype(ctx, media_proto, htmlelem_proto);
        ns_chain_proto(ctx, global, "HTMLAudioElement", media_proto);
        ns_chain_proto(ctx, global, "HTMLVideoElement", media_proto);
    }
    JS_FreeValue(ctx, media_proto);

    for (gsize i = 0; i < G_N_ELEMENTS(tag_props); i++) {
        JSValue *slot = g_hash_table_lookup(js->per_tag_protos,
                                            tag_props[i].tag);
        gboolean is_new = (slot == NULL);
        JSValue tp;
        if (slot) {
            tp = *slot;
        } else {
            tp = JS_NewObject(ctx);
            JS_SetPrototype(ctx, tp, htmlelem_proto);
        }
        if (tag_props[i].has_text)
            ns_proto_define_getset(ctx, tp, "text",
                                   ns_element_get_text, ns_element_set_text);
        if (tag_props[i].has_data)
            ns_proto_define_getset(ctx, tp, "data",
                                   ns_element_get_data, ns_element_set_data);
        if (tag_props[i].has_value)
            ns_proto_define_getset(ctx, tp, "value",
                                   ns_element_get_value_prop,
                                   ns_element_set_value_prop);
        if (strcmp(tag_props[i].tag, "select") == 0) {
            JS_DefinePropertyValueStr(ctx, tp, "item",
                JS_NewCFunction(ctx, ns_options_item, "item", 1),
                JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
            JS_DefinePropertyValueStr(ctx, tp, "namedItem",
                JS_NewCFunction(ctx, ns_options_namedItem, "namedItem", 1),
                JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
        }
        if (is_new) {
            JSValue *entry = g_new(JSValue, 1);
            *entry = tp;
            g_hash_table_insert(js->per_tag_protos,
                                g_strdup(tag_props[i].tag), entry);
        }
    }
    if (JS_IsObject(chardata_proto))
        ns_proto_define_getset(ctx, chardata_proto, "data",
                               ns_element_get_data, ns_element_set_data);

    js->dom_protos_set    = 1;

    if (js->pinned_wrappers_set) {
        GHashTableIter it;
        gpointer k;
        g_hash_table_iter_init(&it, js->pinned_wrappers_set);
        while (g_hash_table_iter_next(&it, &k, NULL)) {
            ns_node *n = k;
            if (!n->js_wrapper) continue;
            JSValue kind_proto = ns_node_kind_proto(js, n);
            if (JS_IsObject(kind_proto))
                JS_SetPrototype(ctx, JS_MKPTR(JS_TAG_OBJECT, n->js_wrapper),
                                kind_proto);
        }
    }
}

static JSValue
ns_chrome_load_times(JSContext *ctx, JSValueConst this_val,
                     int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    ns_js *js = js_from_ctx(ctx);
    double origin = js ? js->time_origin_real_ms / 1000.0 : 0.0;
    double now = ns_perf_now_ms(js) / 1000.0;
    JSValue o = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, o, "requestTime",            JS_NewFloat64(ctx, origin));
    JS_SetPropertyStr(ctx, o, "startLoadTime",          JS_NewFloat64(ctx, origin));
    JS_SetPropertyStr(ctx, o, "commitLoadTime",         JS_NewFloat64(ctx, origin + 0.04));
    JS_SetPropertyStr(ctx, o, "finishDocumentLoadTime", JS_NewFloat64(ctx, origin + 0.09));
    JS_SetPropertyStr(ctx, o, "finishLoadTime",         JS_NewFloat64(ctx, origin + now));
    JS_SetPropertyStr(ctx, o, "firstPaintTime",         JS_NewFloat64(ctx, origin + 0.1));
    JS_SetPropertyStr(ctx, o, "firstPaintAfterLoadTime", JS_NewFloat64(ctx, 0.0));
    JS_SetPropertyStr(ctx, o, "navigationType",         JS_NewString(ctx, "Other"));
    JS_SetPropertyStr(ctx, o, "wasFetchedViaSpdy",      JS_TRUE);
    JS_SetPropertyStr(ctx, o, "wasNpnNegotiated",       JS_TRUE);
    JS_SetPropertyStr(ctx, o, "npnNegotiatedProtocol", JS_NewString(ctx, "h2"));
    JS_SetPropertyStr(ctx, o, "wasAlternateProtocolAvailable", JS_FALSE);
    JS_SetPropertyStr(ctx, o, "connectionInfo",         JS_NewString(ctx, "h2"));
    return o;
}

static JSValue
ns_chrome_csi(JSContext *ctx, JSValueConst this_val,
              int argc, JSValueConst *argv)
{
    (void)this_val; (void)argc; (void)argv;
    ns_js *js = js_from_ctx(ctx);
    double now = ns_perf_now_ms(js);
    JSValue o = JS_NewObject(ctx);
    JS_SetPropertyStr(ctx, o, "startE",
                      JS_NewFloat64(ctx, js ? js->time_origin_real_ms : 0.0));
    JS_SetPropertyStr(ctx, o, "onloadT",
                      JS_NewFloat64(ctx, (js ? js->time_origin_real_ms : 0.0) + now));
    JS_SetPropertyStr(ctx, o, "pageT", JS_NewFloat64(ctx, now));
    JS_SetPropertyStr(ctx, o, "tran", JS_NewInt32(ctx, 15));
    return o;
}

static void
ns_install_window_chrome(JSContext *ctx, JSValueConst global)
{
    JSValue chrome = JS_NewObject(ctx);
    ns_bind_fn(ctx, chrome, "loadTimes", ns_chrome_load_times, 0);
    ns_bind_fn(ctx, chrome, "csi",       ns_chrome_csi,        0);

    JS_SetPropertyStr(ctx, global, "chrome", chrome);
}

static double
ns_js_coarsen_performance_ms(double value)
{
    if (value <= 0) return 0;
    return floor(value * 10.0) / 10.0;
}

static void
ns_js_coarsen_navigation_timing(ns_js *js)
{
    js->time_origin_us = (js->time_origin_us / 100) * 100;
    js->time_origin_real_ms = ns_js_coarsen_performance_ms(
        js->time_origin_real_ms);
    js->navigation_timing.origin_us = js->time_origin_us;
    js->navigation_timing.origin_real_ms = js->time_origin_real_ms;
    js->navigation_timing.domain_lookup_start_ms =
        ns_js_coarsen_performance_ms(
            js->navigation_timing.domain_lookup_start_ms);
    js->navigation_timing.domain_lookup_end_ms =
        ns_js_coarsen_performance_ms(
            js->navigation_timing.domain_lookup_end_ms);
    js->navigation_timing.connect_start_ms = ns_js_coarsen_performance_ms(
        js->navigation_timing.connect_start_ms);
    js->navigation_timing.connect_end_ms = ns_js_coarsen_performance_ms(
        js->navigation_timing.connect_end_ms);
    js->navigation_timing.secure_connection_start_ms =
        ns_js_coarsen_performance_ms(
            js->navigation_timing.secure_connection_start_ms);
    js->navigation_timing.request_start_ms = ns_js_coarsen_performance_ms(
        js->navigation_timing.request_start_ms);
    js->navigation_timing.response_start_ms = ns_js_coarsen_performance_ms(
        js->navigation_timing.response_start_ms);
    js->navigation_timing.response_end_ms = ns_js_coarsen_performance_ms(
        js->navigation_timing.response_end_ms);
}

static void
ns_js_set_navigation_milestone(ns_js *js, double *field,
                               const char *legacy_key)
{
    if (!js || !field || !legacy_key) return;
    *field = ns_perf_now_ms(js);
    JSValue global = JS_GetGlobalObject(js->ctx);
    JSValue performance = JS_GetPropertyStr(js->ctx, global, "performance");
    JSValue timing = JS_GetPropertyStr(js->ctx, performance, "timing");
    if (JS_IsObject(timing)) {
        gint64 absolute_ms = (gint64)floor(js->time_origin_real_ms + *field);
        JS_SetPropertyStr(js->ctx, timing, legacy_key,
                          JS_NewInt64(js->ctx, absolute_ms));
    }
    JS_FreeValue(js->ctx, timing);
    JS_FreeValue(js->ctx, performance);
    JS_FreeValue(js->ctx, global);
}

static void
ns_install_idle_deadline(JSContext *ctx, JSValueConst global)
{
    ns_bind_ctor(ctx, global, "IdleDeadline", ns_illegal_constructor, 0);
    JSValue ctor = JS_GetPropertyStr(ctx, global, "IdleDeadline");
    JSValue proto = JS_IsObject(ctor)
        ? JS_GetPropertyStr(ctx, ctor, "prototype") : JS_UNDEFINED;
    if (JS_IsObject(proto))
        ns_services_install_idle_deadline(ctx, proto);
    JS_FreeValue(ctx, proto);
    JS_FreeValue(ctx, ctor);
}

ns_js *
ns_js_new(ns_js_log_cb log_cb, gpointer log_user_data,
          ns_js_mutated_cb mut_cb, gpointer mut_user_data,
          ns_js_navigate_cb nav_cb, gpointer nav_user_data,
          const ns_js_navigation_timing *navigation_timing)
{
    ns_js *js = g_new0(ns_js, 1);
    if (navigation_timing && navigation_timing->origin_us > 0 &&
        navigation_timing->origin_real_ms > 0) {
        js->navigation_timing = *navigation_timing;
        js->time_origin_us = navigation_timing->origin_us;
        js->time_origin_real_ms = navigation_timing->origin_real_ms;
    } else {
        js->time_origin_us = g_get_monotonic_time();
        js->time_origin_real_ms = (double)g_get_real_time() / 1000.0;
        js->navigation_timing.origin_us = js->time_origin_us;
        js->navigation_timing.origin_real_ms = js->time_origin_real_ms;
    }
    ns_js_coarsen_navigation_timing(js);
    ns_perf_init(js);
    js->rt = JS_NewRuntime();
    if (js->rt) {
        const ns_config *c = ns_config_get();
        int mb = c ? c->js_memory_cap_mb : 2048;
        if (mb <= 0) mb = 2048;
        JS_SetInterruptHandler(js->rt, ns_js_interrupt_cb, js);
        JS_SetMemoryLimit(js->rt, (size_t)mb * 1024 * 1024);
        JS_SetHostPromiseRejectionTracker(
            js->rt, ns_js_promise_rejection_tracker, NULL);
        JS_SetMaxStackSize(js->rt, (size_t)5 * 1024 * 1024);
    }
    if (!js->rt) { g_free(js); return NULL; }
    js->ctx = JS_NewContext(js->rt);
    if (js->ctx) ns_js_add_engine_private_names(js->ctx);
    if (!js->ctx) { JS_FreeRuntime(js->rt); g_free(js); return NULL; }
    JS_AddEnginePrivateName(js->ctx, "__ndRealmDoc");
    JS_AddEnginePrivateName(js->ctx, "__ndRealmWindow");
    js->main_realm_ctx = js->ctx;
    JS_SetContextOpaque(js->ctx, js);
    JS_SetRuntimeOpaque(js->rt, js);
    JS_SetModuleLoaderFunc2(js->rt, ns_js_module_normalize,
                            ns_js_module_loader, NULL, js);
    JSContext *ctx = js->ctx;
    js->log_cb = log_cb;
    js->log_user_data = log_user_data;
    js->mut_cb = mut_cb;
    js->mut_user_data = mut_user_data;
    js->nav_cb = nav_cb;
    js->nav_user_data = nav_user_data;
    js->scroll_to_cb = NULL;
    js->scroll_to_user_data = NULL;
    js->fragment_nav_cb = NULL;
    js->fragment_nav_user_data = NULL;
    js->form_submit_cb = NULL;
    js->form_submit_user_data = NULL;
    js->soft_nav_cb = NULL;
    js->soft_nav_user_data = NULL;
    js->iframe_doc = JS_UNDEFINED;
    js->main_context = g_main_context_default();
    js->frame_ctxs = g_ptr_array_new();
    js->navigator_brand = JS_UNDEFINED;
    js->orphan_nodes = g_hash_table_new(g_direct_hash, g_direct_equal);
    js->pinned_wrappers_set = g_hash_table_new(g_direct_hash, g_direct_equal);
    js->attribute_maps = g_hash_table_new(g_direct_hash, g_direct_equal);
    js->cookie_buckets = g_hash_table_new_full(
        g_str_hash, g_str_equal, g_free, g_free);
    ns_storage_init(js);
    ns_media_init(js);

    ns_new_class_id(&ns_element_class_id);
    JS_NewClass(js->rt, ns_element_class_id, &ns_element_class);
    JSValue element_proto = JS_NewObject(ctx);
    JS_SetPropertyFunctionList(ctx, element_proto, ns_element_proto_funcs,
                               G_N_ELEMENTS(ns_element_proto_funcs));
    ns_define_element_unscopables(ctx, element_proto);
    JS_SetClassProto(ctx, ns_element_class_id, JS_DupValue(ctx, element_proto));

    ns_new_class_id(&ns_attr_class_id);
    JS_NewClass(js->rt, ns_attr_class_id, &ns_attr_class);
    JS_SetClassProto(ctx, ns_attr_class_id, JS_NewObject(ctx));

    ns_new_class_id(&ns_style_class_id);
    JS_NewClass(js->rt, ns_style_class_id, &ns_style_class);
    JSValue style_proto = JS_NewObject(ctx);
    ns_cssom_install_style_proto(ctx, style_proto);
    ns_set_tostring_tag(ctx, style_proto, "CSSStyleDeclaration");
    JS_SetClassProto(ctx, ns_style_class_id, style_proto);

    ns_new_class_id(&ns_token_list_class_id);
    JS_NewClass(js->rt, ns_token_list_class_id, &ns_token_list_class);
    JSValue tlist_proto = JS_NewObject(ctx);
    JS_SetPropertyFunctionList(ctx, tlist_proto, ns_tlist_proto_funcs,
                               G_N_ELEMENTS(ns_tlist_proto_funcs));
    {
        static const char *iter_src =
            "(function(p){"
            " var a = Array.prototype;"
            " if (a) Object.setPrototypeOf(p, a);"
            " Object.defineProperty(p, Symbol.toStringTag,"
            "   { value: 'DOMTokenList', configurable: true });"
            "})";
        JSValue f = JS_Eval(ctx, iter_src, strlen(iter_src),
                            "<tlist-iter>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        if (!JS_IsException(f)) {
            JSValueConst args[1] = { tlist_proto };
            JSValue r = JS_Call(ctx, f, JS_UNDEFINED, 1, args);
            JS_FreeValue(ctx, r);
        } else {
            JS_FreeValue(ctx, JS_GetException(ctx));
        }
        JS_FreeValue(ctx, f);
    }
    JS_SetClassProto(ctx, ns_token_list_class_id, tlist_proto);

    ns_new_class_id(&ns_live_class_id);
    JS_NewClass(js->rt, ns_live_class_id, &ns_live_class);
    ns_live_install_protos(ctx);
    {
        JSValue rnl_proto = ns_live_proto(ctx, 2);
        JSAtom value_atom = JS_NewAtom(ctx, "value");
        JS_DefinePropertyGetSet(ctx, rnl_proto, value_atom,
            JS_NewCFunction2(ctx, ns_radio_node_list_get_value, "get value", 0,
                             JS_CFUNC_generic, 0),
            JS_NewCFunction2(ctx, ns_radio_node_list_set_value, "set value", 1,
                             JS_CFUNC_generic, 0),
            JS_PROP_CONFIGURABLE | JS_PROP_ENUMERABLE);
        JS_FreeAtom(ctx, value_atom);
        JS_FreeValue(ctx, rnl_proto);
    }
    JS_SetClassProto(ctx, ns_live_class_id, ns_live_proto(ctx, 0));

    ns_new_class_id(&ns_dataset_class_id);
    JS_NewClass(js->rt, ns_dataset_class_id, &ns_dataset_class);
    JSValue dataset_proto = JS_NewObject(ctx);
    JS_SetClassProto(ctx, ns_dataset_class_id, dataset_proto);

    ns_new_class_id(&ns_storage_class_id);
    JS_NewClass(js->rt, ns_storage_class_id, &ns_storage_class);
    JSValue storage_proto = JS_NewObject(ctx);
    ns_storage_install_proto(ctx, storage_proto);
    JS_SetClassProto(ctx, ns_storage_class_id, storage_proto);

    JSValue global = JS_GetGlobalObject(ctx);
    JSValue storage_ctor = JS_NewCFunction2(ctx, ns_illegal_constructor,
                                            "Storage", 0,
                                            JS_CFUNC_constructor, 0);
    JS_SetConstructor(ctx, storage_ctor, storage_proto);
    JS_SetPropertyStr(ctx, global, "Storage", storage_ctor);
    ns_new_class_id(&ns_window_named_class_id);
    JS_NewClass(js->rt, ns_window_named_class_id, &ns_window_named_class);
    ns_services_install_console(ctx, global);

    ns_bind_fn(ctx, global, "alert",         ns_services_alert,       1);
    ns_bind_fn(ctx, global, "__jsEngine",    ns_js_engine_name_js,    0);
    ns_bind_fn(ctx, global, "setTimeout",    ns_services_set_timeout,   2);
    ns_bind_fn(ctx, global, "setInterval",   ns_services_set_interval,  2);
    ns_bind_fn(ctx, global, "clearTimeout",  ns_services_clear_timer,        1);
    ns_bind_fn(ctx, global, "clearInterval", ns_services_clear_timer,        1);
    ns_bind_fn(ctx, global, "fetch",         ns_js_fetch,             1);
    ns_window_bind_post_message(ctx, global);

    ns_bind_ctor(ctx, global, "Navigator", ns_illegal_constructor, 0);
    JSValue navigator = ns_services_window_navigator(ctx);

    ns_bind_fn(ctx, global, "__nd_camera_request",   ns_cam_request,   2);
    ns_bind_fn(ctx, global, "__nd_camera_release",   ns_cam_release,   0);
    ns_bind_fn(ctx, global, "__nd_mic_release",      ns_mic_release_js, 0);
    ns_bind_fn(ctx, global, "__nd_camera_label",     ns_cam_label,     0);
    ns_bind_fn(ctx, global, "__nd_camera_enumerate", ns_cam_enumerate, 0);

    ns_sw_install_container(ctx, navigator);

#ifdef ND_HAVE_WEBGPU
    ns_webgpu_install(ctx, js, navigator);
#endif

    JS_SetPropertyStr(ctx, global, "navigator", navigator);

    gboolean nav_chrome_compat = ns_services_chrome_compat();
    if (nav_chrome_compat)
        ns_install_window_chrome(ctx, global);

    ns_bind_ctor(ctx, global, "Performance", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "PerformanceTiming", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "PerformanceNavigation", ns_illegal_constructor, 0);
    {
        static const ns_int_constant constants[] = {
            { "TYPE_NAVIGATE", 0 },
            { "TYPE_RELOAD", 1 },
            { "TYPE_BACK_FORWARD", 2 },
            { "TYPE_RESERVED", 255 },
        };
        ns_bind_ctor_int_constants(ctx, global, "PerformanceNavigation",
                                   constants, G_N_ELEMENTS(constants));
    }
    JS_SetPropertyStr(ctx, global, "performance",
                      ns_make_performance_object(ctx, js,
                                                 nav_chrome_compat));

    ns_bind_ctor(ctx, global, "MutationObserver",     ns_mutation_observer_ctor,     1);
    ns_bind_ctor(ctx, global, "IntersectionObserver", ns_intersection_observer_ctor, 1);
    ns_bind_ctor(ctx, global, "ResizeObserver",       ns_resize_observer_ctor,       1);
    ns_bind_ctor(ctx, global, "PerformanceObserver",  ns_perf_observer_ctor,         1);
    ns_bind_ctor(ctx, global, "PerformanceObserverEntryList",
                 ns_illegal_constructor, 0);
    ns_perf_install_entry_list(ctx, global);
    ns_bind_ctor_proto_fn(ctx, global, "MutationObserver",
                          "observe", ns_mutation_observer_observe, 2);
    ns_bind_ctor_proto_fn(ctx, global, "MutationObserver",
                          "disconnect", ns_mutation_observer_disconnect, 0);
    ns_bind_ctor_proto_fn(ctx, global, "MutationObserver",
                          "takeRecords", ns_mutation_observer_takeRecords, 0);
    ns_bind_ctor_proto_fn(ctx, global, "IntersectionObserver",
                          "observe", ns_intersection_observer_observe, 1);
    ns_bind_ctor_proto_fn(ctx, global, "IntersectionObserver",
                          "unobserve", ns_intersection_observer_unobserve, 1);
    ns_bind_ctor_proto_fn(ctx, global, "IntersectionObserver",
                          "disconnect", ns_intersection_observer_disconnect, 0);
    ns_bind_ctor_proto_fn(ctx, global, "IntersectionObserver",
                          "takeRecords", ns_intersection_observer_takeRecords, 0);
    ns_bind_ctor_proto_fn(ctx, global, "ResizeObserver",
                          "observe", ns_resize_observer_observe, 2);
    ns_bind_ctor_proto_fn(ctx, global, "ResizeObserver",
                          "unobserve", ns_resize_observer_unobserve, 1);
    ns_bind_ctor_proto_fn(ctx, global, "ResizeObserver",
                          "disconnect", ns_resize_observer_disconnect, 0);
    ns_bind_ctor_proto_fn(ctx, global, "PerformanceObserver",
                          "observe", ns_perf_observer_observe, 1);
    ns_bind_ctor_proto_fn(ctx, global, "PerformanceObserver",
                          "disconnect", ns_perf_observer_disconnect, 0);
    ns_bind_ctor_proto_fn(ctx, global, "PerformanceObserver",
                          "takeRecords", ns_perf_observer_takeRecords, 0);
    JSValue perf_observer = JS_GetPropertyStr(ctx, global, "PerformanceObserver");
    JS_SetPropertyStr(ctx, perf_observer, "supportedEntryTypes",
                      ns_perf_supported_entry_types(ctx));
    JS_FreeValue(ctx, perf_observer);

    ns_bind_fn(ctx, global, "addEventListener",    ns_window_addEventListener,    2);
    ns_bind_fn(ctx, global, "removeEventListener", ns_window_removeEventListener, 2);
    ns_bind_fn(ctx, global, "dispatchEvent",       ns_window_dispatchEvent,         1);
    JS_SetPropertyStr(ctx, global, "scrollY", JS_NewInt32(ctx, 0));
    JS_SetPropertyStr(ctx, global, "scrollX", JS_NewInt32(ctx, 0));
    JS_SetPropertyStr(ctx, global, "pageYOffset", JS_NewInt32(ctx, 0));
    JS_SetPropertyStr(ctx, global, "pageXOffset", JS_NewInt32(ctx, 0));
    ns_js_sync_window_metrics(js);

    ns_window_install_actions(ctx, global);
    ns_bind_fn(ctx, global, "find",  ns_window_find,  7);
    ns_bind_fn(ctx, global, "scrollTo", ns_window_scroll_to, 2);
    ns_bind_fn(ctx, global, "scrollBy", ns_window_scroll_by, 2);
    ns_bind_fn(ctx, global, "scroll",   ns_window_scroll_to, 2);
    ns_bind_fn(ctx, global, "scrollByLines", ns_window_scroll_by_lines, 1);
    ns_bind_fn(ctx, global, "scrollByPages", ns_window_scroll_by_pages, 1);
    ns_bind_fn(ctx, global, "open",                  ns_window_open_method,            3);
    ns_bind_fn(ctx, global, "confirm",               ns_window_confirm,                1);
    ns_bind_fn(ctx, global, "prompt",                ns_window_prompt,                 2);
    ns_bind_fn(ctx, global, "matchMedia",            ns_services_match_media,          1);
    ns_cssom_install_window(ctx, global);
    ns_bind_fn(ctx, global, "requestAnimationFrame", ns_window_requestAnimationFrame,  1);
    ns_bind_fn(ctx, global, "cancelAnimationFrame",  ns_window_cancelAnimationFrame,   1);
    ns_js_input_install_wpt(ctx, global);
    ns_bind_fn(ctx, global, "__ndUpdateBlobURL",     ns_window_url_update_object,      2);
    ns_bind_fn(ctx, global, "__ndMseAppend",         ns_window_mse_append,             3);
    ns_bind_fn(ctx, global, "__ndMseEos",            ns_window_mse_eos,                1);
    ns_bind_fn(ctx, global, "__ndMseBuffered",       ns_window_mse_buffered,           2);
    ns_bind_fn(ctx, global, "__ndMseBufferedStart",  ns_window_mse_buffered_start,     2);
    ns_bind_fn(ctx, global, "__ndMseRemove",         ns_window_mse_remove,             4);
    ns_bind_fn(ctx, global, "__ndMseBytes",          ns_window_mse_bytes,      2);
    ns_bind_fn(ctx, global, "__ndMseTypeSupported",
               ns_media_source_is_type_supported, 1);

    ns_events_install_window_base(ctx, global);

    ns_bind_ctor(ctx, global, "History", ns_illegal_constructor, 0);
    ns_window_install_history(ctx, global);

    ns_crypto_install_window(ctx, global);

    ns_bind_fn(ctx, global, "btoa", ns_window_btoa, 1);
    ns_bind_fn(ctx, global, "atob", ns_window_atob, 1);
    JSValue url_ctor = ns_make_ctor(ctx, ns_window_url_ctor, "URL", 1);
    ns_bind_fn(ctx, url_ctor, "canParse",        ns_window_url_can_parse, 1);
    ns_bind_fn(ctx, url_ctor, "parse",           ns_window_url_parse_static, 1);
    ns_bind_fn(ctx, url_ctor, "createObjectURL", ns_window_url_create_object, 1);
    ns_bind_fn(ctx, url_ctor, "revokeObjectURL", ns_window_url_revoke_object, 1);
    JS_SetPropertyStr(ctx, global, "URL", url_ctor);
    JSValue custom_elements = JS_NewObject(ctx);
    ns_bind_fn(ctx, custom_elements, "define",      ns_ce_define,      3);
    ns_bind_fn(ctx, custom_elements, "get",         ns_ce_get,         1);
    ns_bind_fn(ctx, custom_elements, "upgrade",     ns_ce_upgrade,     1);
    ns_bind_fn(ctx, custom_elements, "whenDefined", ns_ce_when_defined, 1);
    ns_bind_fn(ctx, custom_elements, "getName",     ns_ce_get_name,    1);
    JS_SetPropertyStr(ctx, global, "customElements", custom_elements);

    ns_canvas_register_classes(js->rt);
    ns_bind_ctor(ctx, global, "Image",           ns_window_image_ctor,           2);
    ns_bind_ctor(ctx, global, "MediaError",       ns_illegal_constructor,          0);
    {
        static const ns_int_constant constants[] = {
            { "MEDIA_ERR_ABORTED", 1 },
            { "MEDIA_ERR_NETWORK", 2 },
            { "MEDIA_ERR_DECODE", 3 },
            { "MEDIA_ERR_SRC_NOT_SUPPORTED", 4 },
        };
        ns_bind_ctor_int_constants(ctx, global, "MediaError",
                                   constants, G_N_ELEMENTS(constants));
    }
    ns_bind_ctor(ctx, global, "StaticRange",  ns_static_range_ctor,   1);
    ns_bind_ctor(ctx, global, "VTTCue",       ns_vtt_cue_ctor,        3);
    ns_services_install_clipboard_item(ctx, global);
    ns_bind_ctor(ctx, global, "CustomStateSet", ns_custom_state_set_ctor, 0);
    ns_webgl_install(ctx, global);
    ns_bind_ctor(ctx, global, "Audio",           ns_window_audio_ctor,           1);
    ns_media_install_audio(ctx, global);
    ns_bind_ctor(ctx, global, "DocumentFragment", ns_window_document_fragment_ctor, 0);
    ns_bind_ctor(ctx, global, "Text",            ns_window_text_ctor,            1);
    ns_bind_ctor(ctx, global, "Comment",         ns_window_comment_ctor,         1);
    ns_bind_ctor(ctx, global, "Option",          ns_window_option_ctor,          4);
    ns_bind_ctor(ctx, global, "URLSearchParams", ns_window_usp_ctor, 0);
    ns_usp_install_interface(ctx);
    ns_url_install_interface(ctx);
    ns_bind_ctor(ctx, global, "XMLHttpRequestEventTarget", ns_illegal_constructor, 0);
    ns_bind_ctor(ctx, global, "XMLHttpRequestUpload", ns_illegal_constructor,    0);
    ns_bind_ctor(ctx, global, "XMLHttpRequest",  ns_window_xhr_ctor,             0);
    ns_xhr_install_interface(ctx, global);
    ns_bind_ctor(ctx, global, "DOMParser",       ns_window_dom_parser_ctor,      0);
    ns_bind_ctor_proto_fn(ctx, global, "DOMParser", "parseFromString",
                          ns_dom_parser_parseFromString, 2);
    ns_bind_ctor(ctx, global, "FormData",        ns_window_form_data_ctor,       0);
    ns_bind_ctor(ctx, global, "AbortController", ns_window_abort_controller_ctor, 0);
    ns_bind_ctor(ctx, global, "CloseWatcher",    ns_window_close_watcher_ctor,    0);

    ns_install_abort_signal_interface(ctx, global);

    JSValue caches_obj = JS_NewObject(ctx);
    ns_bind_fn(ctx, caches_obj, "open",   ns_cache_open, 1);
    ns_bind_fn(ctx, caches_obj, "has",    ns_returns_resolved_false, 1);
    ns_bind_fn(ctx, caches_obj, "delete", ns_returns_resolved_false, 1);
    ns_bind_fn(ctx, caches_obj, "keys",   ns_returns_resolved_empty_array, 0);
    ns_bind_fn(ctx, caches_obj, "match",  ns_returns_resolved_undefined, 2);
    {
        JSValue prev = JS_GetPropertyStr(ctx, global, "caches");
        JS_FreeValue(ctx, prev);
        JS_SetPropertyStr(ctx, global, "caches", caches_obj);
    }
    ns_bind_ctor(ctx, global, "TextEncoder", ns_window_text_encoder_ctor, 0);
    ns_bind_ctor(ctx, global, "TextDecoder", ns_window_text_decoder_ctor, 0);
    ns_net_install_text_codecs(ctx, global);
    ns_bind_ctor(ctx, global, "Response",    ns_window_response_ctor,     0);
    ns_bind_ctor(ctx, global, "Request",     ns_window_request_ctor,      1);
    ns_fetch_install_interfaces(ctx, global);
    ns_bind_ctor(ctx, global, "FileReader",  ns_window_filereader_ctor,   0);

    ns_events_install_window(ctx, global);

    static const ns_fn_def event_base_ctors[] = {
        { "EventTarget", 0 }, { "Node", 0 }, { "Element", 0 },
        { "HTMLElement", 0 }, { "SVGElement", 0 }, { "SVGAElement", 0 },
        { "SVGSVGElement", 0 }, { "MathMLElement", 0 },
        { "HTMLDocument", 0 },
        { "Window", 0 },
    };
    ns_bind_ctors(ctx, global, ns_window_event_ctor,
                  event_base_ctors, G_N_ELEMENTS(event_base_ctors));
    ns_canvas_install(ctx, global, TRUE);
    ns_bind_ctor(ctx, global, "Document", ns_document_ctor, 0);

    {
        JSValue hp = ns_proto_of(ctx, global, "HTMLDocument");
        JSValue dp = ns_proto_of(ctx, global, "Document");
        if (JS_IsObject(hp) && JS_IsObject(dp)) {
            JS_SetPrototype(ctx, hp, dp);
            JS_FreeValue(ctx, js->proto_document);
            js->proto_document = JS_DupValue(ctx, hp);
        }
        JS_FreeValue(ctx, hp);
        JS_FreeValue(ctx, dp);
    }

    {
        static const struct { const char *name; int value; } node_constants[] = {
            { "ELEMENT_NODE",                1 },
            { "ATTRIBUTE_NODE",              2 },
            { "TEXT_NODE",                   3 },
            { "CDATA_SECTION_NODE",          4 },
            { "ENTITY_REFERENCE_NODE",       5 },
            { "ENTITY_NODE",                 6 },
            { "PROCESSING_INSTRUCTION_NODE", 7 },
            { "COMMENT_NODE",                8 },
            { "DOCUMENT_NODE",               9 },
            { "DOCUMENT_TYPE_NODE",         10 },
            { "DOCUMENT_FRAGMENT_NODE",     11 },
            { "NOTATION_NODE",              12 },
            { "DOCUMENT_POSITION_DISCONNECTED",            0x01 },
            { "DOCUMENT_POSITION_PRECEDING",               0x02 },
            { "DOCUMENT_POSITION_FOLLOWING",               0x04 },
            { "DOCUMENT_POSITION_CONTAINS",                0x08 },
            { "DOCUMENT_POSITION_CONTAINED_BY",            0x10 },
            { "DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC", 0x20 },
        };
        static const char *const node_carriers[] = { "Node", "Element", "HTMLElement", "Document", "HTMLDocument", "DocumentFragment" };
        static const char *const element_proto_carriers[] = { "Node", "Element", "HTMLElement" };
        for (gsize i = 0; i < G_N_ELEMENTS(node_constants); i++)
            JS_DefinePropertyValueStr(ctx, element_proto,
                node_constants[i].name,
                JS_NewInt32(ctx, node_constants[i].value),
                JS_PROP_ENUMERABLE);
        for (gsize c = 0; c < G_N_ELEMENTS(node_carriers); c++) {
            JSValue carrier = JS_GetPropertyStr(ctx, global, node_carriers[c]);
            if (!JS_IsObject(carrier)) { JS_FreeValue(ctx, carrier); continue; }
            JSValue proto = JS_GetPropertyStr(ctx, carrier, "prototype");
            /* The constants belong to Node.prototype, which the document
             * and fragment prototypes inherit from. */
            gboolean doc_like_carrier = c >= 3;
            for (gsize i = 0; i < G_N_ELEMENTS(node_constants); i++) {
                JS_DefinePropertyValueStr(ctx, carrier, node_constants[i].name,
                    JS_NewInt32(ctx, node_constants[i].value),
                    JS_PROP_ENUMERABLE);
                if (JS_IsObject(proto) && !doc_like_carrier)
                    JS_DefinePropertyValueStr(ctx, proto, node_constants[i].name,
                        JS_NewInt32(ctx, node_constants[i].value),
                        JS_PROP_ENUMERABLE);
            }
            if (JS_IsObject(proto)) {
                JS_SetPropertyFunctionList(ctx, proto, ns_element_proto_funcs,
                                           G_N_ELEMENTS(ns_element_proto_funcs));
                if (JS_VALUE_GET_PTR(proto) != JS_VALUE_GET_PTR(element_proto))
                    JS_SetPrototype(ctx, proto, element_proto);
            }
            JS_FreeValue(ctx, proto);
            JS_FreeValue(ctx, carrier);
        }
        for (gsize c = 0; c < G_N_ELEMENTS(element_proto_carriers); c++) {
            JSValue carrier = JS_GetPropertyStr(ctx, global, element_proto_carriers[c]);
            if (!JS_IsObject(carrier)) { JS_FreeValue(ctx, carrier); continue; }
            JS_DefinePropertyValueStr(ctx, carrier, "prototype",
                                      JS_DupValue(ctx, element_proto),
                                      JS_PROP_WRITABLE);
            JS_DefinePropertyValueStr(ctx, element_proto, "constructor",
                                      JS_DupValue(ctx, carrier),
                                      JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
            JS_FreeValue(ctx, carrier);
        }
        {
            JSValue et = JS_GetPropertyStr(ctx, global, "EventTarget");
            if (JS_IsObject(et)) {
                JSValue et_proto = JS_GetPropertyStr(ctx, et, "prototype");
                if (JS_IsObject(et_proto))
                    JS_SetPrototype(ctx, element_proto, et_proto);
                JS_FreeValue(ctx, et_proto);
            }
            JS_FreeValue(ctx, et);
        }
        {
            JSValue perf = JS_GetPropertyStr(ctx, global, "performance");
            if (JS_IsObject(perf))
                ns_performance_extend_event_target(ctx, global, perf);
            JS_FreeValue(ctx, perf);
        }
        JS_FreeValue(ctx, element_proto);
        JSValue ev_carrier = JS_GetPropertyStr(ctx, global, "Event");
        if (JS_IsObject(ev_carrier)) {
            static const struct { const char *name; int value; } evph[] = {
                { "NONE",            0 },
                { "CAPTURING_PHASE", 1 },
                { "AT_TARGET",       2 },
                { "BUBBLING_PHASE",  3 },
            };
            JSValue proto = JS_GetPropertyStr(ctx, ev_carrier, "prototype");
            for (gsize i = 0; i < G_N_ELEMENTS(evph); i++) {
                JS_DefinePropertyValueStr(ctx, ev_carrier, evph[i].name,
                    JS_NewInt32(ctx, evph[i].value), 0);
                if (JS_IsObject(proto))
                    JS_DefinePropertyValueStr(ctx, proto, evph[i].name,
                        JS_NewInt32(ctx, evph[i].value), 0);
            }
            JS_FreeValue(ctx, proto);
        }
        JS_FreeValue(ctx, ev_carrier);
    }

    ns_bind_fn(ctx, global, "__ndIsEngineFunction", ns_is_engine_function, 1);
    ns_js_install_event_target(ctx, global);

    {
        JSValue g2 = JS_GetGlobalObject(ctx);
        JSValue win_proto = JS_GetPrototype(ctx, g2);
        if (JS_IsObject(win_proto)) {
            JSValue base = JS_GetPrototype(ctx, win_proto);
            JSValue wnamed = JS_NewObjectClass(ctx, ns_window_named_class_id);
            if (!JS_IsException(wnamed)) {
                JS_SetPrototype(ctx, wnamed, base);
                JS_SetPrototype(ctx, win_proto, wnamed);
                JS_FreeValue(ctx, wnamed);
            }
            JS_FreeValue(ctx, base);
        }
        JS_FreeValue(ctx, win_proto);
        JS_FreeValue(ctx, g2);
    }

    {
        static const char *post_message_src =
            "(function(){"
            "  var w = globalThis;"
            "  var nextTaskId = 1;"
            "  var canceled = Object.create(null);"
            "  function later(cb){ return setTimeout(cb, 0); }"
            "  if (!w.scheduler) w.scheduler = {};"
            "  if (typeof w.scheduler.postTask !== 'function')"
            "    w.scheduler.postTask = function(cb){"
            "      var id = nextTaskId++;"
            "      return new Promise(function(resolve, reject){"
            "        later(function(){"
            "          if (canceled[id]) return;"
            "          try { resolve(typeof cb === 'function' ? cb() : undefined); }"
            "          catch (e) { reject(e); }"
            "        });"
            "      });"
            "    };"
            "  if (typeof w.scheduler.yield !== 'function')"
            "    w.scheduler.yield = function(){ return new Promise(function(resolve){ later(resolve); }); };"
            "})();";
        JSValue pm_ret = JS_Eval(ctx, post_message_src, strlen(post_message_src),
                                 "<post-message>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        JS_FreeValue(ctx, pm_ret);
    }

    ns_window_install_browsing_context(ctx, global);

    static const char *const window_event_handlers[] = {
        "onload", "onunload", "onbeforeunload", "onhashchange", "onpopstate",
        "onpagehide", "onpageshow", "onmessage", "onmessageerror",
        "onoffline", "ononline", "onresize", "onscroll", "onstorage",
        "onerror", "onrejectionhandled", "onunhandledrejection",
        "onlanguagechange", "onafterprint", "onbeforeprint",
    };
    for (gsize i = 0; i < G_N_ELEMENTS(window_event_handlers); i++)
        JS_SetPropertyStr(ctx, global, window_event_handlers[i], JS_NULL);

    ns_bind_fn(ctx, global, "getSelection",        ns_window_get_selection, 0);
    ns_bind_fn(ctx, global, "requestIdleCallback", ns_services_request_idle_callback, 2);
    ns_bind_fn(ctx, global, "cancelIdleCallback",  ns_services_clear_timer,                1);
    ns_install_idle_deadline(ctx, global);

    ns_window_install_state(js, ctx, global);

    ns_services_install_screen(ctx, global);

    ns_bind_fn(ctx, global, "structuredClone",  ns_window_structured_clone,  1);
    ns_bind_fn(ctx, global, "reportError",      ns_window_report_error,      1);
    ns_bind_fn(ctx, global, "queueMicrotask",   ns_services_queue_microtask,   1);
    ns_bind_ctor(ctx, global, "MessageChannel",   ns_window_message_channel,   0);
    ns_bind_ctor(ctx, global, "MessagePort",      ns_illegal_constructor,      0);
    ns_bind_ctor(ctx, global, "BroadcastChannel", ns_window_broadcast_channel, 1);
    ns_services_install_rtc(ctx, global);
    ns_bind_ctor(ctx, global, "Notification",   ns_services_notification_ctor, 2);
    ns_worker_install_constructor(ctx, global);
    ns_new_class_id(&ns_zlib_class_id);
    JS_NewClass(js->rt, ns_zlib_class_id, &ns_zlib_class);
    ns_js_net_install_sockets(ctx, global);


    ns_cssom_install_css(ctx, global);

    ns_crypto_install_window_subtle(ctx, global);

#ifdef NS_HAVE_WASM
    ns_wasm_install(ctx, global);
#endif

    ns_storage_install_window(ctx, global);

    ns_install_window_compat(ctx, global);

    {
        static const char *nav_iface_src =
            "(function(){"
            " if (typeof Navigator !== 'function' || typeof navigator !== 'object'"
            "     || !navigator) return;"
            " var nav = navigator, Np = Navigator.prototype, others = new WeakSet();"
            " var names = ['userAgent','appName','appCodeName','appVersion',"
            "   'platform','language','onLine','doNotTrack','globalPrivacyControl',"
            "   'cookieEnabled','hardwareConcurrency','vendor','product',"
            "   'productSub','maxTouchPoints','deviceMemory','pdfViewerEnabled',"
            "   'webdriver','vendorSub','languages'];"
            " names.forEach(function(n){"
            "   if (!(n in nav)) return;"
            "   var val = nav[n];"
            "   try { delete nav[n]; } catch(e) {}"
            "   var holder = { get [n](){"
            "     if (this !== nav && !others.has(this))"
            "       throw new TypeError('Illegal invocation');"
            "     return val; } };"
            "   Object.defineProperty(Np, n, { configurable:true, enumerable:true,"
            "     get: Object.getOwnPropertyDescriptor(holder,n).get });"
            " });"
            " try { Object.setPrototypeOf(nav, Np); } catch(e) {}"
            " try { Object.defineProperty(Np, Symbol.toStringTag,"
            "   { value:'Navigator', configurable:true }); } catch(e) {}"
            " return others;"
            "})();";
        /* The set of navigators the getters accept besides the window's:
         * the engine adds each frame realm's own (see
         * ns_realm_install_singletons); pages never see it. */
        JSValue nr = JS_Eval(ctx, nav_iface_src, strlen(nav_iface_src),
                             "<navigator-iface>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        if (JS_IsException(nr)) {
            JS_FreeValue(ctx, JS_GetException(ctx));
        } else if (JS_IsObject(nr) && !js->worker_host) {
            JS_FreeValue(ctx, js->navigator_brand);
            js->navigator_brand = nr;
            nr = JS_UNDEFINED;
        }
        JS_FreeValue(ctx, nr);
    }

    {
        static const char *hide_src =
            "(function(){"
            " Object.getOwnPropertyNames(globalThis).forEach(function(k){"
            "   if (/^__(nd|ns|js|ND)/.test(k)) {"
            "     var d = Object.getOwnPropertyDescriptor(globalThis, k);"
            "     if (d && d.enumerable && d.configurable) {"
            "       try { Object.defineProperty(globalThis, k,"
            "         { enumerable: false }); } catch(e) {} } } });"
            "})();";
        JSValue hr = JS_Eval(ctx, hide_src, strlen(hide_src),
                             "<hide-internals>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        JS_FreeValue(ctx, hr);
    }

    js->pristine_promise = JS_GetPropertyStr(ctx, global, "Promise");
    JS_FreeValue(ctx, global);
    ns_set_active_js(js);
    return js;
}

static void
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

static void
ns_js_set_doc_ready_state(ns_js *js, const ns_node *doc, int state)
{
    if (!js || !doc) return;
    if (!js->doc_ready_states)
        js->doc_ready_states = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_hash_table_insert(js->doc_ready_states, (gpointer)doc,
                        GINT_TO_POINTER(state));
}

static JSValue
ns_document_window_global(JSContext *ctx, ns_js *js)
{
    return js && js->ctx ? JS_GetGlobalObject(js->ctx) : JS_GetGlobalObject(ctx);
}

static gboolean
ns_node_is_frame_element(const ns_node *n)
{
    return n && n->kind == NS_NODE_ELEMENT &&
           (ns_node_is_element_named(n, "iframe") ||
            ns_node_is_element_named(n, "frame") ||
            ns_node_is_element_named(n, "object"));
}

static JSValue
ns_document_get_defaultView(JSContext *ctx, JSValueConst this_val)
{
    /* The window of the document's browsing context: the page's window, or
     * a frame's window for the frame's document, whichever realm asks. A
     * document without a browsing context (createHTMLDocument(), DOMParser,
     * an XHR response) has none. */
    ns_js *js = js_from_ctx(ctx);
    ns_node *doc = ns_unwrap_element_mut(this_val);
    if (!js || !doc || doc->kind != NS_NODE_DOCUMENT || doc == js->current_doc)
        return ns_document_window_global(ctx, js);
    ns_node *owner = doc->parent;
    if (ns_node_is_frame_element(owner)) {
        JSValue el = ns_make_element(ctx, owner);
        JSValue win = ns_iframe_realm_window(ctx, el, owner);
        JS_FreeValue(ctx, el);
        return win;
    }
    return JS_NULL;
}

static const JSCFunctionListEntry ns_document_funcs[] = {
    JS_CGETSET_MAGIC_DEF("fgColor",    ns_document_get_color, ns_document_set_color, 0),
    JS_CGETSET_MAGIC_DEF("bgColor",    ns_document_get_color, ns_document_set_color, 1),
    JS_CGETSET_MAGIC_DEF("linkColor",  ns_document_get_color, ns_document_set_color, 2),
    JS_CGETSET_MAGIC_DEF("vlinkColor", ns_document_get_color, ns_document_set_color, 3),
    JS_CGETSET_MAGIC_DEF("alinkColor", ns_document_get_color, ns_document_set_color, 4),
    JS_CFUNC_DEF("getElementById",          1, ns_document_getElementById),
    JS_CFUNC_DEF("createElement",            1, ns_document_createElement),
    JS_CFUNC_DEF("createElementNS",          2, ns_document_createElementNS),
    JS_CFUNC_DEF("createTextNode",           1, ns_document_createTextNode),
    JS_CFUNC_DEF("createComment",            1, ns_document_createComment),
    JS_CFUNC_DEF("createCDATASection",       1, ns_document_createCDATASection),
    JS_CFUNC_DEF("createProcessingInstruction", 2,
                 ns_document_createProcessingInstruction),
    JS_CFUNC_DEF("createAttribute",          1, ns_document_createAttribute),
    JS_CFUNC_DEF("createAttributeNS",        2, ns_document_createAttributeNS),
    JS_CFUNC_DEF("createEvent",              1, ns_document_createEvent),
    JS_CFUNC_DEF("createDocumentFragment",   0, ns_document_createDocumentFragment),
    JS_CFUNC_DEF("getElementsByTagName",    1, ns_document_getElementsByTagName),
    JS_CFUNC_DEF("getElementsByTagNameNS",  2, ns_document_getElementsByTagNameNS),
    JS_CFUNC_DEF("getElementsByClassName",  1, ns_document_getElementsByClassName),
    JS_CFUNC_DEF("querySelector",           1, ns_document_querySelector),
    JS_CFUNC_DEF("querySelectorAll",        1, ns_document_querySelectorAll),
    JS_CGETSET_DEF("documentElement", ns_document_get_documentElement, ns_element_noop_set),
    JS_CGETSET_DEF("body",            ns_document_get_body,            ns_document_set_body),
    JS_CGETSET_DEF("head",            ns_document_get_head,            ns_element_noop_set),
    JS_CGETSET_DEF("activeElement",   ns_document_get_activeElement,   ns_element_noop_set),
    JS_CGETSET_DEF("forms",           ns_document_get_forms,           ns_element_noop_set),
    JS_CGETSET_DEF("images",          ns_document_get_images,          ns_element_noop_set),
    JS_CGETSET_DEF("links",           ns_document_get_links,           ns_element_noop_set),
    JS_CGETSET_DEF("scripts",         ns_document_get_scripts,         ns_element_noop_set),
    JS_CGETSET_DEF("embeds",          ns_document_get_embeds,          ns_element_noop_set),
    JS_CGETSET_DEF("plugins",         ns_document_get_plugins,         ns_element_noop_set),
    JS_CGETSET_DEF("designMode",      ns_document_get_designMode,      ns_element_noop_set),
    JS_CGETSET_DEF("lastModified",    ns_document_get_lastModified,    ns_element_noop_set),
    JS_CGETSET_DEF("all",             ns_document_get_all,             ns_element_noop_set),
    JS_CGETSET_DEF("anchors",         ns_document_get_anchors,         ns_element_noop_set),
    JS_CGETSET_DEF("applets",         ns_document_get_applets,         ns_element_noop_set),
    JS_CGETSET_DEF("fonts",           ns_document_get_fonts,           ns_element_noop_set),
    JS_CGETSET_DEF("implementation",  ns_document_implementation,      NULL),
    JS_CFUNC_DEF("write",      1, ns_document_write),
    JS_CFUNC_DEF("writeln",    1, ns_document_writeln),
    JS_CFUNC_DEF("open",       0, ns_document_open),
    JS_CFUNC_DEF("close",      0, ns_document_close),
    JS_CFUNC_DEF("execCommand", 3, ns_document_execCommand),
    JS_CFUNC_DEF("hasFocus",          0, ns_document_has_focus),
    JS_CFUNC_DEF("elementFromPoint",  2, ns_document_element_from_point),
    JS_CFUNC_DEF("elementsFromPoint", 2, ns_document_elements_from_point),
    JS_CFUNC_DEF("createRange",       0, ns_document_create_range),
    JS_CFUNC_DEF("createTreeWalker",  3, ns_document_create_tree_walker),
    JS_CFUNC_DEF("createNodeIterator",3, ns_document_create_node_iterator),
    JS_CFUNC_DEF("getSelection",      0, ns_window_get_selection),
    JS_CFUNC_DEF("adoptNode",         1, ns_document_adopt_node),
    JS_CFUNC_DEF("importNode",        2, ns_document_import_node),
    JS_CFUNC_DEF("exitFullscreen", 0, ns_document_exit_fullscreen),
    JS_CFUNC_DEF("webkitExitFullscreen", 0, ns_document_exit_fullscreen),
    JS_CFUNC_DEF("webkitCancelFullScreen", 0, ns_document_exit_fullscreen),
    JS_CFUNC_DEF("mozCancelFullScreen", 0, ns_document_exit_fullscreen),
    JS_CFUNC_DEF("msExitFullscreen", 0, ns_document_exit_fullscreen),
    JS_CFUNC_DEF("exitPointerLock", 0, ns_document_exitPointerLock),
    JS_CFUNC_DEF("queryCommandSupported", 1, ns_document_queryCommandSupported),
    JS_CFUNC_DEF("queryCommandEnabled",   1, ns_document_queryCommandEnabled),
    JS_CFUNC_DEF("queryCommandState",     1, ns_event_false),
    JS_CFUNC_DEF("queryCommandValue",     1, ns_event_noop),
    JS_CGETSET_DEF("currentScript",      ns_document_get_currentScript, ns_element_noop_set),
    JS_CGETSET_DEF("rootElement",        ns_document_get_documentElement, ns_element_noop_set),
    JS_CGETSET_DEF("fullscreenElement",  ns_document_get_fullscreen_element, ns_element_noop_set),
    JS_CGETSET_DEF("webkitFullscreenElement", ns_document_get_fullscreen_element, ns_element_noop_set),
    JS_CGETSET_DEF("mozFullScreenElement", ns_document_get_fullscreen_element, ns_element_noop_set),
    JS_CGETSET_DEF("msFullscreenElement", ns_document_get_fullscreen_element, ns_element_noop_set),
    JS_CGETSET_DEF("webkitIsFullScreen", ns_document_get_is_fullscreen, ns_element_noop_set),
    JS_CGETSET_DEF("mozFullScreen", ns_document_get_is_fullscreen, ns_element_noop_set),
    JS_CGETSET_DEF("pointerLockElement", ns_document_get_pointerLockElement, ns_element_noop_set),
    JS_CGETSET_DEF("fullscreenEnabled",  ns_document_get_fullscreen_enabled, ns_element_noop_set),
    JS_CGETSET_DEF("webkitFullscreenEnabled", ns_document_get_fullscreen_enabled, ns_element_noop_set),
    JS_CGETSET_DEF("scrollingElement",   ns_document_get_scrollingElement, ns_element_noop_set),
    JS_CFUNC_DEF("addEventListener",    2, ns_document_addEventListener),
    JS_CFUNC_DEF("removeEventListener", 2, ns_document_removeEventListener),
    JS_CFUNC_DEF("dispatchEvent",       1, ns_document_dispatchEvent),
    JS_CFUNC_DEF("getElementsByName",   1, ns_document_getElementsByName),
    JS_CFUNC_DEF("getItems",            1, ns_document_getItems),
    JS_CGETSET_DEF("title",           ns_document_get_title,  ns_document_set_title),
    JS_CGETSET_DEF("cookie",          ns_document_get_cookie, ns_document_set_cookie),
    JS_CGETSET_DEF("referrer",        ns_document_get_referrer,        ns_element_noop_set),
    JS_CGETSET_DEF("readyState",      ns_document_get_readyState,      ns_element_noop_set),
    JS_CGETSET_DEF("hidden",          ns_document_get_hidden,          ns_element_noop_set),
    JS_CGETSET_DEF("visibilityState", ns_document_get_visibilityState, ns_element_noop_set),
    JS_CGETSET_DEF("compatMode",      ns_document_get_compatMode,      ns_element_noop_set),
    JS_CGETSET_DEF("dir",             ns_document_get_dir,             ns_document_set_dir),
};

void
ns_document_install_funcs(JSContext *ctx, JSValueConst doc)
{
    JS_SetPropertyFunctionList(ctx, doc, ns_document_funcs,
                               G_N_ELEMENTS(ns_document_funcs));
    ns_install_event_handler_props(ctx, doc);
}

static const JSCFunctionListEntry ns_document_proto_accessors[] = {
    JS_CGETSET_DEF("documentElement", ns_document_get_documentElement, ns_element_noop_set),
    JS_CGETSET_DEF("body",            ns_document_get_body,            ns_document_set_body),
    JS_CGETSET_DEF("head",            ns_document_get_head,            ns_element_noop_set),
    JS_CGETSET_DEF("currentScript",   ns_document_get_currentScript,   ns_element_noop_set),
    JS_CGETSET_DEF("defaultView",     ns_document_get_defaultView,     ns_element_noop_set),
    JS_CGETSET_DEF("implementation",  ns_document_implementation,      NULL),
    JS_CGETSET_DEF("cookie",          ns_document_get_cookie,          ns_document_set_cookie),
    JS_CGETSET_DEF("readyState",      ns_document_get_readyState,      ns_element_noop_set),
    JS_CGETSET_DEF("xmlVersion",      ns_document_get_xmlVersion,      ns_element_noop_set),
};

static const JSCFunctionListEntry ns_document_proto_methods[] = {
    JS_CFUNC_DEF("adoptNode",              1, ns_document_adopt_node),
    JS_CFUNC_DEF("createDocumentFragment", 0, ns_document_createDocumentFragment),
    JS_CFUNC_DEF("createElement",          1, ns_document_createElement),
    JS_CFUNC_DEF("createElementNS",        2, ns_document_createElementNS),
    JS_CFUNC_DEF("createTreeWalker",       3, ns_document_create_tree_walker),
};

static void
ns_js_reset_runtime_state(ns_js *js)
{
    if (!js) return;
    ns_top_layer_clear(js);
    ns_focus_reset(js);
    ns_js_forget_pending_change(js);
    ns_storage_free_deferred_events(js);

    if (js->pending_scrollend) {
        g_ptr_array_free(js->pending_scrollend, TRUE);
        js->pending_scrollend = NULL;
    }
    js->pending_scrollend_doc = FALSE;

    ns_services_reset(js);

    if (js->raf_pending) {
        for (guint i = 0; i < js->raf_pending->len; i++) {
            ns_raf_entry *e = &g_array_index(js->raf_pending, ns_raf_entry, i);
            JS_FreeValue(e->ctx ? e->ctx : js->ctx, e->cb);
        }
        g_array_set_size(js->raf_pending, 0);
    }

    ns_dispatch_reset(js);

    ns_js_net_reset(js);

    ns_ce_reset(js);
    ns_css_clear_defined_elements();
    ns_css_clear_registered_properties();

    ns_js_cancel_async_scripts(js);

    ns_attr_detach_all(js);
    ns_attribute_maps_release_all(js);
    if (js->pinned_wrappers_set) {
        GList *pinned = g_hash_table_get_keys(js->pinned_wrappers_set);
        g_hash_table_remove_all(js->pinned_wrappers_set);
        for (GList *l = pinned; l; l = l->next) {
            ns_node *node = l->data;
            if (node && node->js_wrapper) {
                JSValue v = JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper);
                JS_SetOpaque(v, NULL);
                node->js_wrapper = NULL;
                node->js_invalidate = NULL;
                JS_FreeValue(js->ctx, v);
            }
        }
        g_list_free(pinned);
    }

    ns_observers_reset(js);
    ns_perf_reset_observers(js);

    if (js->orphan_nodes) {
        GList *list = g_hash_table_get_keys(js->orphan_nodes);
        g_hash_table_remove_all(js->orphan_nodes);
        for (GList *l = list; l; l = l->next)
            ns_node_free(l->data);
        g_list_free(list);
    }

    ns_js_frames_reset(js);
    ns_window_links_clear(js, FALSE);
    ns_js_drop_message_tasks(js);
    ns_js_clear_frame_clocks(js, FALSE);
    ns_js_drop_pending_rejections(js);
    if (js->frame_ctxs) {
        for (guint i = 0; i < js->frame_ctxs->len; i++)
            JS_FreeContext(g_ptr_array_index(js->frame_ctxs, i));
        g_ptr_array_set_size(js->frame_ctxs, 0);
    }

    ns_drain_microtasks(js);
    JS_RunGC(js->rt);
}


static void
ns_js_install_document(ns_js *js, ns_node *doc, const char *base_url)
{
    ns_js_reset_runtime_state(js);

    js->current_doc = doc;
    js->ce_main_doc = doc;
    js->autofocus_processed = FALSE;
    js->active_modal = NULL;
    ns_dom_set_active_modal(NULL);
    g_free(js->current_url);
    js->current_url = g_strdup(base_url ? base_url : "");
    g_free(js->document_origin);
    js->document_origin = base_url && *base_url
        ? ns_url_origin_from(base_url) : NULL;

    if (doc) {
        ns_doc_id_index_build(doc);
        ns_doc_class_index_build(doc);
        ns_doc_tag_index_build(doc);
    }

    ns_debug_log_emit(NS_DLOG_JS, "install", "document %s",
                      js->current_url ? js->current_url : "(none)");

    ns_storage_load_for(js, js->current_url);
    ns_js_seed_cookies_from_jar(js);

    JSContext *ctx = js->ctx;
    JSValue global = JS_GetGlobalObject(ctx);

    JSValue document = JS_NewObjectClass(ctx, ns_element_class_id);
    if (js->current_doc) JS_SetOpaque(document, js->current_doc);
    JS_SetPropertyStr(ctx, document, "URL",         JS_NewString(ctx, js->current_url));
    JS_SetPropertyStr(ctx, document, "documentURI", JS_NewString(ctx, js->current_url));
    JS_SetPropertyStr(ctx, document, "baseURI",     JS_NewString(ctx, js->current_url));
    JS_SetPropertyStr(ctx, document, "characterSet", JS_NewString(ctx, "UTF-8"));
    JS_DefinePropertyValueStr(ctx, document, "charset",
                              JS_NewString(ctx, "UTF-8"), JS_PROP_C_W_E);
    JS_SetPropertyStr(ctx, document, "inputEncoding", JS_NewString(ctx, "UTF-8"));
    JS_SetPropertyStr(ctx, document, "contentType",  JS_NewString(ctx, "text/html"));
    {
        char *dom_host = ns_url_host_from(js->current_url);
        JS_SetPropertyStr(ctx, document, "domain",
                          JS_NewString(ctx, dom_host ? dom_host : ""));
        g_free(dom_host);
    }
    JS_SetPropertyStr(ctx, document, "defaultView",  JS_DupValue(ctx, global));
    JS_SetPropertyStr(ctx, document, "ownerDocument", JS_NULL);
    JS_SetPropertyStr(ctx, document, "nodeName",     JS_NewString(ctx, "#document"));
    JS_SetPropertyStr(ctx, document, "nodeType",     JS_NewInt32(ctx, 9));
    ns_document_define_doctype_getter(ctx, document);
    JS_SetPropertyStr(ctx, document, "xmlVersion",   JS_NewString(ctx, "1.0"));
    JS_SetPropertyStr(ctx, document, "xmlEncoding",  JS_NULL);
    JS_SetPropertyStr(ctx, document, "xmlStandalone", JS_FALSE);
    ns_document_install_funcs(ctx, document);
    {
        JSValue doc_ctor = JS_GetPropertyStr(ctx, global, "HTMLDocument");
        if (!JS_IsObject(doc_ctor)) {
            JS_FreeValue(ctx, doc_ctor);
            doc_ctor = JS_GetPropertyStr(ctx, global, "Document");
        }
        if (JS_IsObject(doc_ctor)) {
            JSValue doc_proto = JS_GetPropertyStr(ctx, doc_ctor, "prototype");
            if (JS_IsObject(doc_proto))
                JS_SetPrototype(ctx, document, doc_proto);
            JS_FreeValue(ctx, doc_proto);
        }
        JS_FreeValue(ctx, doc_ctor);
    }
    {
        static const char *const doc_proto_ctors[] = {
            "Document", "HTMLDocument", NULL
        };
        for (int i = 0; doc_proto_ctors[i]; i++) {
            JSValue proto = ns_proto_of(ctx, global, doc_proto_ctors[i]);
            if (JS_IsObject(proto)) {
                JS_SetPropertyFunctionList(ctx, proto, ns_document_proto_methods,
                                           G_N_ELEMENTS(ns_document_proto_methods));
                JS_SetPropertyFunctionList(ctx, proto, ns_document_proto_accessors,
                                           G_N_ELEMENTS(ns_document_proto_accessors));
                JS_DefinePropertyValueStr(ctx, proto, "open",
                    JS_NewCFunction(ctx, ns_document_open, "open", 0),
                    JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
                JS_DefinePropertyValueStr(ctx, proto, "close",
                    JS_NewCFunction(ctx, ns_document_close, "close", 0),
                    JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
                JS_DefinePropertyValueStr(ctx, proto, "write",
                    JS_NewCFunction(ctx, ns_document_write, "write", 1),
                    JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
                JS_DefinePropertyValueStr(ctx, proto, "writeln",
                    JS_NewCFunction(ctx, ns_document_writeln, "writeln", 1),
                    JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
            }
            JS_FreeValue(ctx, proto);
        }
    }
    ns_document_expose_legacy_named(js->ctx, js->current_doc, document);
    JS_SetPropertyStr(ctx, global, "document", document);

    JSValue location = ns_window_make_location(ctx);
    JS_SetPropertyStr(ctx, global, "location", location);
    JS_SetPropertyStr(ctx, document, "location", JS_DupValue(ctx, location));
    {
        char *origin = ns_url_origin_from(js->current_url);
        JS_SetPropertyStr(ctx, global, "origin",
                          JS_NewString(ctx, origin ? origin : ""));
        JS_SetPropertyStr(ctx, global, "isSecureContext",
                          js->current_url &&
                          g_str_has_prefix(js->current_url, "https:")
                              ? JS_TRUE : JS_FALSE);
        g_free(origin);
    }
    {
        static const char *loc_fwd =
            "(function(){"
            " var loc = location;"
            " var d = { configurable: true, enumerable: true,"
            "   get: function(){ return loc; },"
            "   set: function(v){ loc.href = v; } };"
            " Object.defineProperty(globalThis, 'location', d);"
            " try { Object.defineProperty(document, 'location', d); } catch(e){}"
            "})();";
        JSValue r = JS_Eval(ctx, loc_fwd, strlen(loc_fwd),
                            "<location-forward>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, r);
    }

    static const ns_fn_def shim_ctors[] = {
        { "XMLDocument", 0 }, { "XSLTProcessor", 0 },
        { "Range", 0 }, { "NodeFilter", 0 },
        { "DOMTokenList", 0 }, { "NodeList", 0 }, { "HTMLCollection", 0 },
        { "CSSStyleSheet", 0 }, { "CSSStyleDeclaration", 0 },
        { "CSSRule", 0 }, { "CSSStyleRule", 0 },
        { "MediaList", 0 }, { "MediaQueryList", 0 },
        { "Selection", 0 }, { "Animation", 0 },
        { "FileList", 0 },
        { "HTMLInputElement", 0 }, { "HTMLAnchorElement", 0 },
        { "HTMLImageElement", 0 }, { "HTMLFormElement", 0 },
        { "HTMLSelectElement", 0 }, { "HTMLOptionElement", 0 },
        { "HTMLButtonElement", 0 }, { "HTMLDivElement", 0 },
        { "HTMLSpanElement", 0 }, { "HTMLTableElement", 0 },
        { "HTMLTableRowElement", 0 }, { "HTMLTableCellElement", 0 },
        { "HTMLLabelElement", 0 }, { "HTMLTextAreaElement", 0 },
        { "HTMLVideoElement", 0 }, { "HTMLAudioElement", 0 },
        { "HTMLMediaElement", 0 }, { "HTMLDialogElement", 0 },
        { "HTMLDetailsElement", 0 }, { "HTMLScriptElement", 0 },
        { "HTMLLinkElement", 0 }, { "HTMLMetaElement", 0 },
        { "HTMLStyleElement", 0 }, { "HTMLBodyElement", 0 },
        { "HTMLHtmlElement", 0 }, { "HTMLHeadElement", 0 },
        { "HTMLIFrameElement", 0 }, { "HTMLCanvasElement", 0 },
        { "HTMLAreaElement", 0 }, { "HTMLBaseElement", 0 },
        { "HTMLBRElement", 0 }, { "HTMLDataElement", 0 },
        { "HTMLDataListElement", 0 }, { "HTMLDListElement", 0 },
        { "HTMLEmbedElement", 0 }, { "HTMLFieldSetElement", 0 },
        { "HTMLHeadingElement", 0 }, { "HTMLHRElement", 0 },
        { "HTMLLegendElement", 0 }, { "HTMLLIElement", 0 },
        { "HTMLMapElement", 0 }, { "HTMLMenuElement", 0 },
        { "HTMLMeterElement", 0 }, { "HTMLModElement", 0 },
        { "HTMLObjectElement", 0 }, { "HTMLOListElement", 0 },
        { "HTMLOptGroupElement", 0 }, { "HTMLOutputElement", 0 },
        { "HTMLParagraphElement", 0 }, { "HTMLPictureElement", 0 },
        { "HTMLPreElement", 0 }, { "HTMLProgressElement", 0 },
        { "HTMLQuoteElement", 0 }, { "HTMLSlotElement", 0 },
        { "HTMLSourceElement", 0 }, { "HTMLTableCaptionElement", 0 },
        { "HTMLTableColElement", 0 }, { "HTMLTableSectionElement", 0 },
        { "HTMLTemplateElement", 0 }, { "HTMLTimeElement", 0 },
        { "HTMLTitleElement", 0 }, { "HTMLTrackElement", 0 },
        { "HTMLUListElement", 0 }, { "HTMLUnknownElement", 0 },
        { "HTMLFontElement", 0 }, { "HTMLMarqueeElement", 0 },
        { "HTMLFrameElement", 0 }, { "HTMLFrameSetElement", 0 },
        { "HTMLParamElement", 0 }, { "HTMLDirectoryElement", 0 },
        { "CharacterData", 0 },
        { "CDATASection", 0 },
        { "ProcessingInstruction", 0 }, { "Attr", 0 },
        { "DocumentType", 0 },
        { "HTMLOptionsCollection", 0 }, { "HTMLAllCollection", 0 },
        { "RadioNodeList", 0 },
        { "ValidityState", 0 },
        { "DOMStringList", 0 }, { "DOMStringMap", 0 },
        { "NamedNodeMap", 0 }, { "TreeWalker", 0 }, { "NodeIterator", 0 },
        { "MutationRecord", 0 }, { "IntersectionObserverEntry", 0 },
        { "ResizeObserverEntry", 0 },
        { "PerformanceEntry", 0 }, { "PerformanceMark", 0 },
        { "PerformanceMeasure", 0 }, { "PerformanceResourceTiming", 0 },
        { "PerformanceNavigationTiming", 0 },
        { "FontFaceSet", 0 },
        { "ReadableStream", 1 }, { "WritableStream", 1 },
        { "TransformStream", 1 },
        { "ByteLengthQueuingStrategy", 1 }, { "CountQueuingStrategy", 1 },
        { "Geolocation", 0 }, { "Permissions", 0 },
        { "Crypto", 0 }, { "SubtleCrypto", 0 }, { "CryptoKey", 0 },
    };
    ns_bind_ctors(ctx, global, ns_window_event_ctor, shim_ctors, G_N_ELEMENTS(shim_ctors));
    {
        JSValue proto = ns_proto_of(ctx, global, "NamedNodeMap");
        if (JS_IsObject(proto)) {
            JS_SetPropertyFunctionList(ctx, proto, ns_namedmap_proto_funcs,
                                       G_N_ELEMENTS(ns_namedmap_proto_funcs));
            JSAtom len_atom = JS_NewAtom(ctx, "length");
            JS_DefinePropertyGetSet(ctx, proto, len_atom,
                JS_NewCFunction2(ctx, ns_live_length_get, "get length", 0,
                                 JS_CFUNC_generic, 0),
                JS_UNDEFINED, JS_PROP_CONFIGURABLE);
            JS_FreeAtom(ctx, len_atom);
        }
        JS_FreeValue(ctx, proto);
    }
    {
        static const char *source =
            "(function(p){"
            " var A=Array.prototype;"
            " function d(k,v){Object.defineProperty(p,k,{value:v,writable:true,configurable:true});}"
            " d('entries',A.entries);d('keys',A.keys);d('values',A.values);d('forEach',A.forEach);"
            " Object.defineProperty(p,Symbol.iterator,{value:A.values,writable:true,configurable:true});"
            "})(NamedNodeMap.prototype)";
        JSValue result = JS_Eval(ctx, source, strlen(source),
                                 "<namednodemap>", JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
        if (JS_IsException(result)) JS_FreeValue(ctx, JS_GetException(ctx));
        JS_FreeValue(ctx, result);
    }
    ns_bind_ctor(ctx, global, "ShadowRoot", ns_illegal_constructor, 0);
    {
        static const ns_int_constant constants[] = {
            { "NETWORK_EMPTY", 0 },
            { "NETWORK_IDLE", 1 },
            { "NETWORK_LOADING", 2 },
            { "NETWORK_NO_SOURCE", 3 },
            { "HAVE_NOTHING", 0 },
            { "HAVE_METADATA", 1 },
            { "HAVE_CURRENT_DATA", 2 },
            { "HAVE_FUTURE_DATA", 3 },
            { "HAVE_ENOUGH_DATA", 4 },
        };
        ns_bind_ctor_int_constants(ctx, global, "HTMLMediaElement",
                                   constants, G_N_ELEMENTS(constants));
    }
    static const ns_fn_def navigator_ctors[] = {
        { "NavigatorUAData", 0 }, { "PluginArray", 0 },
        { "MimeTypeArray", 0 }, { "Plugin", 0 }, { "MimeType", 0 },
        { "MediaDevices", 0 }, { "MediaCapabilities", 0 },
        { "NetworkInformation", 0 }, { "UserActivation", 0 },
        { "StorageManager", 0 }, { "WakeLock", 0 },
    };
    ns_bind_ctors(ctx, global, ns_illegal_constructor,
                  navigator_ctors, G_N_ELEMENTS(navigator_ctors));
    ns_bind_ctor(ctx, global, "DOMImplementation", ns_illegal_constructor, 0);
    ns_perf_install_entry_list(ctx, global);
    {
        static const struct {
            const char *name;
            const JSCFunctionListEntry *funcs;
            int count;
        } interfaces[] = {
            { "HTMLScriptElement", ns_src_proto_funcs,
              G_N_ELEMENTS(ns_src_proto_funcs) },
            { "HTMLImageElement", ns_image_proto_funcs,
              G_N_ELEMENTS(ns_image_proto_funcs) },
            { "HTMLSourceElement", ns_source_proto_funcs,
              G_N_ELEMENTS(ns_source_proto_funcs) },
            { "HTMLTrackElement", ns_src_proto_funcs,
              G_N_ELEMENTS(ns_src_proto_funcs) },
            { "HTMLMediaElement", ns_media_proto_funcs,
              G_N_ELEMENTS(ns_media_proto_funcs) },
            { "HTMLVideoElement", ns_video_proto_funcs,
              G_N_ELEMENTS(ns_video_proto_funcs) },
            { "HTMLIFrameElement", ns_iframe_proto_funcs,
              G_N_ELEMENTS(ns_iframe_proto_funcs) },
            { "HTMLEmbedElement", ns_src_proto_funcs,
              G_N_ELEMENTS(ns_src_proto_funcs) },
            { "HTMLFrameElement", ns_src_proto_funcs,
              G_N_ELEMENTS(ns_src_proto_funcs) },
            { "HTMLAnchorElement", ns_anchor_proto_funcs,
              G_N_ELEMENTS(ns_anchor_proto_funcs) },
            { "HTMLAreaElement", ns_href_proto_funcs,
              G_N_ELEMENTS(ns_href_proto_funcs) },
            { "HTMLLinkElement", ns_href_proto_funcs,
              G_N_ELEMENTS(ns_href_proto_funcs) },
        };
        for (gsize i = 0; i < G_N_ELEMENTS(interfaces); i++) {
            JSValue proto = ns_proto_of(ctx, global, interfaces[i].name);
            if (JS_IsObject(proto))
                JS_SetPropertyFunctionList(ctx, proto, interfaces[i].funcs,
                                           interfaces[i].count);
            JS_FreeValue(ctx, proto);
        }
    }
    {
        JSValue implementation_proto =
            ns_proto_of(ctx, global, "DOMImplementation");
        if (JS_IsObject(implementation_proto))
            JS_SetPropertyFunctionList(ctx, implementation_proto,
                                       ns_dom_implementation_proto_funcs,
                                       G_N_ELEMENTS(ns_dom_implementation_proto_funcs));
        JS_FreeValue(ctx, implementation_proto);
    }
    {
        JSValue tree_walker_proto = ns_proto_of(ctx, global, "TreeWalker");
        ns_tree_walker_install_proto(ctx, tree_walker_proto);
        JS_FreeValue(ctx, tree_walker_proto);
    }
    ns_live_wire_constructors(ctx, global);
    {
        JSValue validity_proto = ns_proto_of(ctx, global, "ValidityState");
        if (JS_IsObject(validity_proto)) {
            JS_SetPropertyFunctionList(ctx, validity_proto,
                                       ns_validity_proto_funcs,
                                       G_N_ELEMENTS(ns_validity_proto_funcs));
            ns_set_tostring_tag(ctx, validity_proto, "ValidityState");
        }
        JS_FreeValue(ctx, validity_proto);
    }
    {
        JSValue doc_ctor = JS_GetPropertyStr(ctx, global, "Document");
        JSValue doc_proto = JS_GetPropertyStr(ctx, doc_ctor, "prototype");
        JSValue xml_ctor = JS_GetPropertyStr(ctx, global, "XMLDocument");
        JSValue xml_proto = JS_GetPropertyStr(ctx, xml_ctor, "prototype");
        if (JS_IsObject(doc_proto) && JS_IsObject(xml_proto))
            JS_SetPrototype(ctx, xml_proto, doc_proto);
        JS_FreeValue(ctx, doc_ctor);
        JS_FreeValue(ctx, doc_proto);
        JS_FreeValue(ctx, xml_ctor);
        JS_FreeValue(ctx, xml_proto);
    }
    ns_install_dom_hierarchy(js, ctx, global);
    ns_install_web_api_shapes(ctx, global);
    ns_bind_ctor(ctx, global, "FontFace", ns_window_fontface_ctor, 3);
    {
        JSValue ctor = JS_GetPropertyStr(ctx, global, "CSSStyleDeclaration");
        JSValue proto = JS_GetClassProto(ctx, ns_style_class_id);
        if (JS_IsObject(ctor) && JS_IsObject(proto)) {
            JS_DefinePropertyValueStr(ctx, proto, "constructor",
                                      JS_DupValue(ctx, ctor),
                                      JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
            JS_DefinePropertyValueStr(ctx, ctor, "prototype", proto,
                                      JS_PROP_WRITABLE);
        } else {
            JS_FreeValue(ctx, proto);
        }
        JS_FreeValue(ctx, ctor);
    }
    {
        JSValue ctor = JS_GetPropertyStr(ctx, global, "DOMStringMap");
        JSValue proto = JS_GetClassProto(ctx, ns_dataset_class_id);
        if (JS_IsObject(ctor) && JS_IsObject(proto)) {
            JS_DefinePropertyValueStr(ctx, proto, "constructor",
                                      JS_DupValue(ctx, ctor),
                                      JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
            JS_DefinePropertyValueStr(ctx, ctor, "prototype", proto,
                                      JS_PROP_WRITABLE);
        } else {
            JS_FreeValue(ctx, proto);
        }
        JS_FreeValue(ctx, ctor);
    }
    {
        JSValue ctor = JS_GetPropertyStr(ctx, global, "DOMTokenList");
        JSValue proto = JS_GetClassProto(ctx, ns_token_list_class_id);
        if (JS_IsObject(ctor) && JS_IsObject(proto)) {
            JS_DefinePropertyValueStr(ctx, proto, "constructor",
                                      JS_DupValue(ctx, ctor),
                                      JS_PROP_WRITABLE | JS_PROP_CONFIGURABLE);
            JS_DefinePropertyValueStr(ctx, ctor, "prototype", proto,
                                      JS_PROP_WRITABLE);
        } else {
            JS_FreeValue(ctx, proto);
        }
        JS_FreeValue(ctx, ctor);
    }
    ns_bind_ctor(ctx, global, "XMLSerializer", ns_xml_serializer_ctor, 0);

    ns_bind_fn(ctx, global, "__ns_zlib_create", ns_zlib_create, 2);
    ns_bind_fn(ctx, global, "__ns_zlib_push",   ns_zlib_push,   2);
    ns_bind_fn(ctx, global, "__ns_zlib_finish", ns_zlib_finish, 1);
    ns_bind_fn(ctx, global, "__ndNativeRange",  ns_native_range, 0);

    ns_install_hasinstance(ctx, global);
    ns_install_tostringtag(ctx, global);
    {
        static const char *const on_targets[] = {
            "HTMLElement", "SVGElement", "MathMLElement", "Document", "Window",
        };
        for (gsize i = 0; i < G_N_ELEMENTS(on_targets); i++) {
            JSValue p = ns_proto_of(ctx, global, on_targets[i]);
            ns_install_event_handler_accessors(ctx, p);
            JS_FreeValue(ctx, p);
        }
        ns_install_event_handler_accessors(ctx, global);
    }
    ns_idb_install(ctx, global);
    ns_net_install_interfaces(ctx, global);

    JS_FreeValue(ctx, global);

    ns_js_eval(js, ns_js_polyfills_src,
               sizeof(ns_js_polyfills_src) - 1, "<polyfills>");
    ns_js_eval(js, ns_js_streaming_src,
               sizeof(ns_js_streaming_src) - 1, "<streaming>");
    ns_drain_microtasks(js);
    {
        JSValue g = JS_GetGlobalObject(ctx);
        ns_install_event_attribute_getters(ctx, g);
        JS_FreeValue(ctx, g);
    }
    ns_install_navigator_shape(ctx);
    ns_install_pdf_plugins(ctx);
    ns_js_link_interfaces(ctx);
    ns_js_name_engine_members(ctx);
    ns_js_shape_window_global(ctx);
    ns_js_link_interface_ctors(ctx);
    ns_js_lock_global_prototypes(ctx);
    {
        JSValue g = JS_GetGlobalObject(ctx);
        JSValue doc_val = JS_GetPropertyStr(ctx, g, "document");
        ns_document_lift_methods_to_proto(ctx, doc_val);
        JS_FreeValue(ctx, doc_val);
        JS_FreeValue(ctx, g);
    }
    ns_install_node_shapes(ctx);
    ns_install_element_shapes(ctx);
    ns_js_brand_node_interfaces(ctx, ns_element_class_id, ns_attr_class_id);
}

void
ns_js_free(ns_js *js)
{
    if (!js) return;
    if (js->doc_ready_states) {
        g_hash_table_destroy(js->doc_ready_states);
        js->doc_ready_states = NULL;
    }
    ns_js_frames_clear_initial_blank(js);
    ns_top_layer_clear(js);
    ns_js_fonts_teardown(js);
    if (js->lifecycle_source) {
        ns_js_source_remove(js, js->lifecycle_source);
        js->lifecycle_source = 0;
    }
    if (js->lifecycle_tasks) {
        g_array_free(js->lifecycle_tasks, TRUE);
        js->lifecycle_tasks = NULL;
    }
    g_clear_pointer(&js->lifecycle_origin, g_free);
    ns_js_blob_registry_remove(js);
    ns_storage_flush(js);
    if (js->csp) { ns_csp_free(js->csp); js->csp = NULL; }
    g_free(js->early_inject_src);
    g_free(js->cookie_value);
    g_free(js->referrer);
    g_free(js->current_url);
    g_free(js->change_baseline);
    g_free(js->document_origin);
    g_free(js->selection_text);
    ns_document_write_teardown(js);
    ns_workers_teardown(js);
    ns_window_history_teardown(js);
    ns_perf_teardown(js);
    ns_traversal_teardown(js);
    ns_services_teardown(js);
    if (js->blob_urls) g_hash_table_destroy(js->blob_urls);
    if (js->raf_pending) {
        for (guint i = 0; i < js->raf_pending->len; i++) {
            ns_raf_entry *e = &g_array_index(js->raf_pending, ns_raf_entry, i);
            JS_FreeValue(e->ctx ? e->ctx : js->ctx, e->cb);
        }
        g_array_free(js->raf_pending, TRUE);
    }
    ns_canvas_states_teardown(js);
    if (js->js_image_loads) {
        g_hash_table_destroy(js->js_image_loads);
        js->js_image_loads = NULL;
    }
    ns_dispatch_teardown_listeners(js);
    ns_js_net_teardown(js);
    if (js->observer_tick_source) {
        g_source_remove(js->observer_tick_source);
        js->observer_tick_source = 0;
    }
    if (js->raf_tick_source) {
        g_source_remove(js->raf_tick_source);
        js->raf_tick_source = 0;
    }
    ns_js_loader_teardown(js);
    if (js->filereader_idles) {
        for (guint i = 0; i < js->filereader_idles->len; i++) {
            ns_filereader_idle *fr = g_ptr_array_index(js->filereader_idles, i);
            if (!fr) continue;
            ns_js_source_remove(js, fr->source);
            JS_FreeValue(js->ctx, fr->self);
            g_free(fr);
        }
        g_ptr_array_free(js->filereader_idles, TRUE);
        js->filereader_idles = NULL;
    }
    ns_attr_detach_all(js);
    ns_attribute_maps_release_all(js);
    if (js->attribute_maps) {
        g_hash_table_destroy(js->attribute_maps);
        js->attribute_maps = NULL;
    }
    if (js->pinned_wrappers_set) {
        GList *pinned = g_hash_table_get_keys(js->pinned_wrappers_set);
        g_hash_table_remove_all(js->pinned_wrappers_set);
        for (GList *l = pinned; l; l = l->next) {
            ns_node *node = l->data;
            if (node && node->js_wrapper) {
                JSValue v = JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper);
                JS_SetOpaque(v, NULL);
                node->js_wrapper = NULL;
                node->js_invalidate = NULL;
                JS_FreeValue(js->ctx, v);
            }
        }
        g_list_free(pinned);
        g_hash_table_destroy(js->pinned_wrappers_set);
        js->pinned_wrappers_set = NULL;
    }
    if (js->orphan_nodes) {
        GList *list = g_hash_table_get_keys(js->orphan_nodes);
        GPtrArray *roots = g_ptr_array_new();
        for (GList *l = list; l; l = l->next) {
            ns_node *node = l->data;
            if (!node) continue;
            if (node->js_wrapper) {
                JSValue v = JS_MKPTR(JS_TAG_OBJECT, node->js_wrapper);
                JS_SetOpaque(v, NULL);
                node->js_wrapper = NULL;
                node->js_invalidate = NULL;
            }
            gboolean owned = FALSE;
            for (ns_node *a = node->parent; a; a = a->parent) {
                if (a == js->current_doc ||
                    g_hash_table_contains(js->orphan_nodes, a)) {
                    owned = TRUE;
                    break;
                }
            }
            if (!owned)
                g_ptr_array_add(roots, node);
        }
        g_list_free(list);
        g_hash_table_destroy(js->orphan_nodes);
        js->orphan_nodes = NULL;
        for (guint i = 0; i < roots->len; i++)
            ns_node_free(g_ptr_array_index(roots, i));
        g_ptr_array_free(roots, TRUE);
    }
    if (js->iframe_globals) {
        g_hash_table_destroy(js->iframe_globals);
        js->iframe_globals = NULL;
    }
    ns_storage_free_deferred_events(js);
    ns_focus_teardown(js);
    ns_js_input_teardown(js);
    JS_FreeValue(js->ctx, js->pristine_promise);
    if (js->dom_protos_set) {
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
    ns_observers_teardown(js);
    ns_ce_teardown(js);
    g_clear_pointer(&js->platform_globals, g_hash_table_destroy);
    ns_storage_teardown(js);
    ns_media_teardown(js);
    if (js->cookie_buckets)
        g_hash_table_destroy(js->cookie_buckets);
    g_free(js->partition_key);
    ns_collections_teardown(js);
    ns_cssom_teardown(js);
    ns_url_teardown(js);
    if (js->form_data_helper_set) {
        JS_FreeValue(js->ctx, js->form_data_helper);
        js->form_data_helper_set = 0;
    }
    if (js->box_lookup_cache) {
        g_hash_table_destroy(js->box_lookup_cache);
        js->box_lookup_cache = NULL;
    }
    if (ns_active_js() == js) ns_set_active_js(NULL);
    ns_css_set_fullscreen_node(NULL);
    ns_js_frames_teardown(js);
    ns_window_links_clear(js, TRUE);
    ns_js_drop_message_tasks(js);
    ns_js_clear_frame_clocks(js, TRUE);
    JS_FreeValue(js->ctx, js->navigator_brand);
    js->navigator_brand = JS_UNDEFINED;
    if (js->message_tasks) {
        g_queue_free(js->message_tasks);
        js->message_tasks = NULL;
    }
    ns_js_drop_pending_rejections(js);
    ns_dispatch_teardown(js);
    if (js->frame_ctxs) {
        for (guint i = 0; i < js->frame_ctxs->len; i++)
            JS_FreeContext(g_ptr_array_index(js->frame_ctxs, i));
        g_ptr_array_free(js->frame_ctxs, TRUE);
        js->frame_ctxs = NULL;
    }
    JS_FreeContext(js->ctx);
    JS_FreeRuntime(js->rt);
    g_free(js);
}

static gboolean
ns_js_profile_enabled(void)
{
    static gint cached = -1;
    if (G_UNLIKELY(cached < 0))
        cached = g_getenv("NS_PROFILE") ? 1 : 0;
    return cached == 1;
}

static void
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

static gboolean
ns_js_eval_bytecode_cached(ns_js *js, const char *src, gsize len,
                           const char *origin, JSValue *out_value,
                           gboolean *out_compiled)
{
    if (out_compiled) *out_compiled = FALSE;
    if (len < 1024) return FALSE;
    gsize bc_len = 0;
    guint8 *bc = ns_bytecode_cache_get(src, len, &bc_len);
    if (!bc) return FALSE;
    JSValue fn = JS_ReadObject(js->ctx, bc, bc_len, JS_READ_OBJ_BYTECODE);
    g_free(bc);
    if (JS_IsException(fn)) {
        JS_FreeValue(js->ctx, JS_GetException(js->ctx));
        return FALSE;
    }
    *out_value = JS_EvalFunction(js->ctx, fn);
    if (out_compiled) *out_compiled = TRUE;
    (void)origin;
    return TRUE;
}

static void
ns_js_bytecode_cache_store(ns_js *js, JSValue fn_obj, const char *src, gsize len)
{
    if (len < 1024) return;
    size_t bc_size = 0;
    uint8_t *bc = JS_WriteObject(js->ctx, &bc_size, fn_obj,
                                 JS_WRITE_OBJ_BYTECODE);
    if (!bc || bc_size == 0) {
        if (bc) js_free(js->ctx, bc);
        return;
    }
    ns_bytecode_cache_put(src, len, bc, bc_size);
    js_free(js->ctx, bc);
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

static const char ns_member_names_src[] =
    "(function(G){"
    "  var toStr = Function.prototype.toString, done = new Set(), seen = new Set();"
    "  function engine(f){ try { return / \\{ \\[native code\\] \\}$/.test(toStr.call(f)); } catch (e) { return false; } }"
    "  function name(f, n){"
    "    if (typeof f !== 'function' || done.has(f) || f.name === n || !engine(f)) return;"
    "    done.add(f);"
    "    try { Object.defineProperty(f, 'name', { value: n, configurable: true }); } catch (e) {}"
    "  }"
    "  function alias(o, f){"
    "    return typeof f === 'function' && typeof f.name === 'string' && f.name !== '' && o[f.name] === f;"
    "  }"
    "  function fix(o){"
    "    if (!o || (typeof o !== 'object' && typeof o !== 'function') || seen.has(o)) return;"
    "    seen.add(o);"
    "    var keys; try { keys = Object.getOwnPropertyNames(o); } catch (e) { return; }"
    "    keys.forEach(function(k){"
    "      if (/^[A-Z]/.test(k) || k === 'constructor' || k === 'prototype' || k.slice(0, 2) === '__') return;"
    "      var d; try { d = Object.getOwnPropertyDescriptor(o, k); } catch (e) { return; }"
    "      if (!d) return;"
    "      if (d.get) name(d.get, 'get ' + k);"
    "      if (d.set) name(d.set, 'set ' + k);"
    "      if ('value' in d && !alias(o, d.value)) name(d.value, k);"
    "    });"
    "  }"
    "  var gkeys = Object.getOwnPropertyNames(G);"
    "  gkeys.forEach(function(k){"
    "    var v; try { v = G[k]; } catch (e) { return; }"
    "    if (/^[A-Z]/.test(k) && typeof v === 'function') {"
    "      if (v.name !== k && G[v.name] !== v) name(v, k);"
    "      fix(v); fix(v.prototype);"
    "    }"
    "  });"
    "  fix(G); fix(Object.getPrototypeOf(G));"
    "  gkeys.forEach(function(k){"
    "    if (/^[A-Z]/.test(k)) return;"
    "    var v; try { v = G[k]; } catch (e) { return; }"
    "    if (v && typeof v === 'object') fix(v);"
    "  });"
    "})(globalThis)";

static const char ns_interface_links_src[] =
    "(function(G){"
    "  var OP = Object.prototype;"
    "  function tag(proto, n){"
    "    if (!proto || proto === OP || Object.prototype.hasOwnProperty.call(proto, Symbol.toStringTag)) return;"
    "    try { Object.defineProperty(proto, Symbol.toStringTag, { value: n, configurable: true }); } catch (e) {}"
    "  }"
    "  function iface(n){"
    "    var C = G[n];"
    "    if (typeof C !== 'function') {"
    "      C = ({ [n]: function(){ throw new TypeError('Illegal constructor'); } })[n];"
    "      try { Object.defineProperty(G, n, { value: C, writable: true, configurable: true, enumerable: false }); } catch (e) { return null; }"
    "    }"
    "    if (C.prototype && typeof C.prototype === 'object') tag(C.prototype, n);"
    "    return C;"
    "  }"
    "  Object.getOwnPropertyNames(G).forEach(function(n){"
    "    if (!/^[A-Z][A-Za-z0-9]*$/.test(n)) return;"
    "    var C; try { C = G[n]; } catch (e) { return; }"
    "    if (typeof C === 'function' && C.prototype && typeof C.prototype === 'object' &&"
    "        C.prototype.constructor === C && !/Error$/.test(n)) tag(C.prototype, n);"
    "  });"
    "  function link(get, n){"
    "    var o; try { o = get(); } catch (e) { return; }"
    "    if (!o || typeof o !== 'object') return;"
    "    var C = iface(n); if (!C || !C.prototype) return;"
    "    var p = Object.getPrototypeOf(o);"
    "    if (p === OP || p === null) { try { Object.setPrototypeOf(o, C.prototype); } catch (e) {} }"
    "    else tag(p, n);"
    "  }"
    "  var N = G.navigator || {};"
    "  link(function(){ return G.location; }, 'Location');"
    "  link(function(){ return G.screen; }, 'Screen');"
    "  ['locationbar','menubar','personalbar','scrollbars','statusbar','toolbar'].forEach(function(k){ link(function(){ return G[k]; }, 'BarProp'); });"
    "  link(function(){ return G.visualViewport; }, 'VisualViewport');"
    "  link(function(){ return G.customElements; }, 'CustomElementRegistry');"
    "  link(function(){ return G.external; }, 'External');"
    "  link(function(){ return G.caches; }, 'CacheStorage');"
    "  link(function(){ return G.scheduler; }, 'Scheduler');"
    "  link(function(){ return G.localStorage; }, 'Storage');"
    "  link(function(){ return G.sessionStorage; }, 'Storage');"
    "  link(function(){ return G.indexedDB; }, 'IDBFactory');"
    "  link(function(){ return G.navigation; }, 'Navigation');"
    "  link(function(){ return G.document && G.document.fonts; }, 'FontFaceSet');"
    "  link(function(){ return G.performance && G.performance.memory; }, 'MemoryInfo');"
    "  link(function(){ return N.clipboard; }, 'Clipboard');"
    "  link(function(){ return N.locks; }, 'LockManager');"
    "  link(function(){ return N.serviceWorker; }, 'ServiceWorkerContainer');"
    "  link(function(){ return N.geolocation; }, 'Geolocation');"
    "  link(function(){ return N.permissions; }, 'Permissions');"
    "  link(function(){ return N.connection; }, 'NetworkInformation');"
    "  link(function(){ return N.storage; }, 'StorageManager');"
    "  link(function(){ return N.userAgentData; }, 'NavigatorUAData');"
    "  link(function(){ return N.mediaDevices; }, 'MediaDevices');"
    "  function adopt(r, n){"
    "    if (!r || typeof r !== 'object') return r;"
    "    var p = Object.getPrototypeOf(r);"
    "    if (p === OP || p === null) { var C = iface(n);"
    "      if (C && C.prototype) { try { Object.setPrototypeOf(r, C.prototype); } catch (e) {} } }"
    "    return r;"
    "  }"
    "  function owner(o, k){ while (o) { if (Object.prototype.hasOwnProperty.call(o, k)) return o; o = Object.getPrototypeOf(o); } return null; }"
    "  function wrap(o, k, after){"
    "    var d = o && Object.getOwnPropertyDescriptor(o, k);"
    "    if (!d || typeof d.value !== 'function') return;"
    "    var orig = d.value;"
    "    var w = ({ [k]: function(){ return after(orig.apply(this, arguments), arguments); } })[k];"
    "    try { Object.defineProperty(w, 'length', { value: orig.length, configurable: true }); } catch (e) {}"
    "    try { Object.defineProperty(o, k, { value: w, writable: d.writable, enumerable: d.enumerable, configurable: d.configurable }); } catch (e) {}"
    "  }"
    "  function wrapGetter(o, k, n){"
    "    var d = o && Object.getOwnPropertyDescriptor(o, k);"
    "    if (!d || typeof d.get !== 'function') { if (o) adopt(o[k], n); return; }"
    "    var orig = d.get;"
    "    var g = ({ ['get ' + k]: function(){ return adopt(orig.call(this), n); } })['get ' + k];"
    "    try { Object.defineProperty(o, k, { get: g, set: d.set, enumerable: d.enumerable, configurable: d.configurable }); } catch (e) {}"
    "  }"
    "  var seenCtx = new WeakSet();"
    "  function instrument2d(c){"
    "    if (!c || seenCtx.has(c)) return c; seenCtx.add(c);"
    "    wrap(owner(c, 'measureText'), 'measureText', function(r){ return adopt(r, 'TextMetrics'); });"
    "    wrap(owner(c, 'getImageData'), 'getImageData', function(r){ return adopt(r, 'ImageData'); });"
    "    wrap(owner(c, 'createImageData'), 'createImageData', function(r){ return adopt(r, 'ImageData'); });"
    "    return c;"
    "  }"
    "  var ctxIface = { '2d': 'CanvasRenderingContext2D', 'webgl': 'WebGLRenderingContext',"
    "    'experimental-webgl': 'WebGLRenderingContext', 'webgl2': 'WebGL2RenderingContext',"
    "    'bitmaprenderer': 'ImageBitmapRenderingContext' };"
    "  try {"
    "    var cv = G.document && G.document.createElement('canvas');"
    "    var cvOwner = cv && owner(cv, 'getContext');"
    "    if (cvOwner && cvOwner !== cv) wrap(cvOwner, 'getContext', function(r, a){"
    "      var n = ctxIface[String(a[0]).toLowerCase()];"
    "      if (n) adopt(r, n);"
    "      return n === 'CanvasRenderingContext2D' ? instrument2d(r) : r;"
    "    });"
    "  } catch (e) {}"
    "  if (G.OffscreenCanvas && G.OffscreenCanvas.prototype) wrap(owner(G.OffscreenCanvas.prototype, 'getContext'), 'getContext', function(r, a){"
    "    if (String(a[0]).toLowerCase() === '2d') { adopt(r, 'OffscreenCanvasRenderingContext2D'); return instrument2d(r); }"
    "    return r;"
    "  });"
    "  wrap(owner(G, 'matchMedia'), 'matchMedia', function(r){ return adopt(r, 'MediaQueryList'); });"
    "  if (G.document) wrapGetter(owner(G.document, 'fonts'), 'fonts', 'FontFaceSet');"
    "  if (G.performance) wrapGetter(owner(G.performance, 'memory'), 'memory', 'MemoryInfo');"
    "  wrapGetter(owner(N, 'scheduling'), 'scheduling', 'Scheduling');"
    "})(globalThis)";

static void
ns_js_link_interfaces(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_interface_links_src,
                        sizeof(ns_interface_links_src) - 1, "<interface-links>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

static void
ns_js_name_engine_members(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_member_names_src, sizeof(ns_member_names_src) - 1,
                        "<member-names>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

static int
ns_js_origin_is_engine_code(const char *origin)
{
    return origin && origin[0] == '<' &&
           strcmp(origin, "<inline>") != 0 && strcmp(origin, "<timer>") != 0;
}

static void
ns_js_eval(ns_js *js, const char *src, gsize len, const char *origin)
{
    int source_flags = ns_js_origin_is_engine_code(origin)
        ? JS_EVAL_FLAG_HIDE_SOURCE : 0;
    ns_budget_guard bg = {0};
    ns_js_budget_push(js, &bg);
    if (js->iframe_doc_set) {
        GString *w = g_string_new("(function(document){\n");
        g_string_append_len(w, src ? src : "", (gssize)len);
        g_string_append(w, "\n})");
        JSValue fn = JS_Eval(js->ctx, w->str, w->len, origin ? origin : "inline",
                             JS_EVAL_TYPE_GLOBAL | source_flags);
        g_string_free(w, TRUE);
        if (!JS_IsException(fn) && JS_IsFunction(js->ctx, fn)) {
            JSValue global = JS_GetGlobalObject(js->ctx);
            JSValueConst args[1] = { js->iframe_doc };
            js->eval_depth++;
            JSValue v = JS_Call(js->ctx, fn, global, 1, args);
            js->eval_depth--;
            if (js->eval_depth == 0) {
                ns_js_flush_document_write(js);
                ns_js_schedule_pending_script_drain(js);
            }
            if (JS_IsException(v)) {
                JSValue ex = JS_GetException(js->ctx);
                const char *msg = JS_ToCString(js->ctx, ex);
                if (msg && js->log_cb) {
                    JSValue stk = JS_GetPropertyStr(js->ctx, ex, "stack");
                    const char *stack = JS_ToCString(js->ctx, stk);
                    char *line = g_strdup_printf("JS error in %s: %s%s%s",
                                                 origin ? origin : "inline", msg,
                                                 stack ? "\n" : "", stack ? stack : "");
                    js->log_cb(line, js->log_user_data);
                    g_free(line);
                    if (stack) JS_FreeCString(js->ctx, stack);
                    JS_FreeValue(js->ctx, stk);
                }
                if (msg) JS_FreeCString(js->ctx, msg);
                JS_FreeValue(js->ctx, ex);
            }
            JS_FreeValue(js->ctx, v);
            JS_FreeValue(js->ctx, global);
            JS_FreeValue(js->ctx, fn);
            ns_js_budget_pop(js, &bg);
            return;
        }
        if (JS_IsException(fn)) JS_FreeValue(js->ctx, JS_GetException(js->ctx));
        JS_FreeValue(js->ctx, fn);
        /* fall through: run unwrapped so a script that can't be wrapped still runs */
    }
    char *copy = g_strndup(src ? src : "", len);
    gboolean profile = ns_js_profile_enabled();
    gint64 t0 = profile ? g_get_monotonic_time() : 0;
    js->eval_depth++;

    JSValue v = JS_UNDEFINED;
    gboolean cache_hit = FALSE;
    /* An inline script's positions count from where its text starts in the
     * document; an external script's from the start of its own file. */
    const ns_node *inline_script = js->current_script &&
        !ns_element_get_attr(js->current_script, "src") ? js->current_script
                                                         : NULL;
    int src_line = (inline_script && inline_script->src_line > 0)
                       ? inline_script->src_line : 1;
    int src_col = (inline_script && inline_script->src_col > 0)
                      ? inline_script->src_col : 1;
    if (!(ns_js_eval_bytecode_cached(js, copy, len, origin, &v, &cache_hit) &&
          cache_hit)) {
        JSValue geval = JS_GetGlobalObject(js->ctx);
        JSEvalOptions opts = {
            .version = JS_EVAL_OPTIONS_VERSION,
            .eval_flags = JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_COMPILE_ONLY |
                          source_flags,
            .filename = origin,
            .line_num = src_line,
            .col_num = src_col,
        };
        JSValue fn = JS_EvalThis2(js->ctx, geval, copy, len, &opts);
        JS_FreeValue(js->ctx, geval);
        if (!JS_IsException(fn)) {
            ns_js_bytecode_cache_store(js, fn, copy, len);
            v = JS_EvalFunction(js->ctx, fn);
        } else {
            v = fn;
        }
    }

    g_free(copy);
    js->eval_depth--;
    if (js->eval_depth == 0) {
        ns_js_flush_document_write(js);
        ns_js_schedule_pending_script_drain(js);
    }
    if (profile)
        g_printerr("[profile] js eval     %6.1fms  %zub  %s%s\n",
                   (g_get_monotonic_time() - t0) / 1000.0, (size_t)len,
                   cache_hit ? "[bc] " : "",
                   origin ? origin : "<inline>");
    if (JS_IsException(v)) {
        JSValue ex = JS_GetException(js->ctx);
        const char *msg = JS_ToCString(js->ctx, ex);
        if (msg && js->log_cb) {
            JSValue stk = JS_GetPropertyStr(js->ctx, ex, "stack");
            const char *stack = JS_ToCString(js->ctx, stk);
            char *line = g_strdup_printf("JS error in %s: %s%s%s",
                                         origin ? origin : "inline",
                                         msg,
                                         stack && *stack ? "\n" : "",
                                         stack ? stack : "");
            js->log_cb(line, js->log_user_data);
            g_free(line);
            if (stack) JS_FreeCString(js->ctx, stack);
            JS_FreeValue(js->ctx, stk);
        }
        if (msg) JS_FreeCString(js->ctx, msg);
        ns_js_report_uncaught(js, ex, origin);
        JS_FreeValue(js->ctx, ex);
    }
    ns_js_apply_site_quirks(js);
    JS_FreeValue(js->ctx, v);
    ns_drain_microtasks(js);
    ns_js_budget_pop(js, &bg);
}

#define NS_MAX_SCRIPT_BYTES (32u * 1024u * 1024u)

static char *
ns_js_module_normalize(JSContext *ctx, const char *base_name,
                       const char *name, void *opaque)
{
    if (!name) return NULL;
    char *resolved = ns_js_module_resolve(opaque, base_name, name);
    char *out = js_strdup(ctx, resolved);
    g_free(resolved);
    return out;
}

char *
ns_js_decode_data_url(const char *url, gsize *out_len)
{
    GByteArray *body = g_byte_array_new();
    if (!ns_data_url_decode(url, body, NS_MAX_SCRIPT_BYTES, NULL, NULL)) {
        g_byte_array_free(body, TRUE);
        return NULL;
    }
    gsize len = body->len;
    guint8 nul = 0;
    g_byte_array_append(body, &nul, 1);
    if (out_len) *out_len = len;
    return (char *)g_byte_array_free(body, FALSE);
}

static void
ns_js_log_module_compile_error(ns_js *js, JSContext *ctx, const char *module_name)
{
    if (!js || !js->log_cb) return;
    JSValue exc = JS_GetException(ctx);
    const char *msg = JS_ToCString(ctx, exc);
    char *line = g_strdup_printf("module compile failed %s: %s",
                                 module_name, msg ? msg : "(no message)");
    js->log_cb(line, js->log_user_data);
    g_free(line);
    if (msg) JS_FreeCString(ctx, msg);
    JS_Throw(ctx, exc);
}

static gboolean
ns_js_attrs_type_is(JSContext *ctx, JSValueConst attributes, const char *want)
{
    if (!JS_IsObject(attributes)) return FALSE;
    JSValue t = JS_GetPropertyStr(ctx, attributes, "type");
    gboolean match = FALSE;
    if (JS_IsString(t)) {
        const char *s = JS_ToCString(ctx, t);
        match = s && !strcmp(s, want);
        if (s) JS_FreeCString(ctx, s);
    }
    JS_FreeValue(ctx, t);
    return match;
}

static int
ns_js_json_module_init(JSContext *ctx, JSModuleDef *m)
{
    return JS_SetModuleExport(ctx, m, "default",
                              JS_GetModulePrivateValue(ctx, m));
}

static JSModuleDef *
ns_js_make_json_module(JSContext *ctx, const char *module_name,
                       const char *body, gsize body_len)
{
    char *json_text = g_strndup(body, body_len);
    JSValue json = JS_ParseJSON(ctx, json_text, body_len, module_name);
    g_free(json_text);
    if (JS_IsException(json)) return NULL;
    JSModuleDef *m = JS_NewCModule(ctx, module_name, ns_js_json_module_init);
    if (!m) {
        JS_FreeValue(ctx, json);
        return NULL;
    }
    JS_AddModuleExport(ctx, m, "default");
    JS_SetModulePrivateValue(ctx, m, json);
    return m;
}

static int
ns_js_module_set_import_meta(JSContext *ctx, JSValueConst module,
                             gboolean is_main)
{
    if (JS_VALUE_GET_TAG(module) != JS_TAG_MODULE) return -1;
    JSModuleDef *def = JS_VALUE_GET_PTR(module);
    JSAtom name_atom = JS_GetModuleName(ctx, def);
    const char *name = JS_AtomToCString(ctx, name_atom);
    JS_FreeAtom(ctx, name_atom);
    if (!name) return -1;
    JSValue meta = JS_GetImportMeta(ctx, def);
    if (JS_IsException(meta)) {
        JS_FreeCString(ctx, name);
        return -1;
    }
    JS_DefinePropertyValueStr(ctx, meta, "url", JS_NewString(ctx, name),
                              JS_PROP_C_W_E);
    JS_DefinePropertyValueStr(ctx, meta, "main", JS_NewBool(ctx, is_main),
                              JS_PROP_C_W_E);
    JS_FreeValue(ctx, meta);
    JS_FreeCString(ctx, name);
    return 0;
}

static JSValue
ns_js_compile_module_cached(JSContext *ctx, const char *src, gsize len,
                            const char *module_name)
{
    gsize  name_len = module_name ? strlen(module_name) : 0;
    gsize  key_len  = name_len + 1 + len;
    char  *key      = g_malloc(key_len);
    memcpy(key, module_name ? module_name : "", name_len);
    key[name_len] = '\0';
    memcpy(key + name_len + 1, src ? src : "", len);

    if (len >= 1024) {
        gsize bc_len = 0;
        guint8 *bc = ns_bytecode_cache_get(key, key_len, &bc_len);
        if (bc) {
            JSValue m = JS_ReadObject(ctx, bc, bc_len, JS_READ_OBJ_BYTECODE);
            g_free(bc);
            if (!JS_IsException(m) && JS_VALUE_GET_TAG(m) == JS_TAG_MODULE) {
                if (ns_js_module_set_import_meta(ctx, m, FALSE) < 0) {
                    JS_FreeValue(ctx, m);
                    g_free(key);
                    return JS_EXCEPTION;
                }
                g_free(key);
                return m;
            }
            if (JS_IsException(m)) JS_FreeValue(ctx, JS_GetException(ctx));
            else JS_FreeValue(ctx, m);
        }
    }

    JSValue func_val = JS_Eval(ctx, src, len, module_name,
                               JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY);
    if (!JS_IsException(func_val) && len >= 1024 &&
        JS_VALUE_GET_TAG(func_val) == JS_TAG_MODULE) {
        size_t bc_size = 0;
        uint8_t *bc = JS_WriteObject(ctx, &bc_size, func_val,
                                     JS_WRITE_OBJ_BYTECODE);
        if (bc) {
            if (bc_size > 0) ns_bytecode_cache_put(key, key_len, bc, bc_size);
            js_free(ctx, bc);
        }
    }
    if (!JS_IsException(func_val) &&
        ns_js_module_set_import_meta(ctx, func_val, FALSE) < 0) {
        JS_FreeValue(ctx, func_val);
        func_val = JS_EXCEPTION;
    }
    g_free(key);
    return func_val;
}

static JSModuleDef *
ns_js_module_loader(JSContext *ctx, const char *module_name, void *opaque,
                    JSValueConst attributes)
{
    ns_js *js = opaque;
    gboolean json_module = ns_js_attrs_type_is(ctx, attributes, "json");
    if (!module_name) return NULL;
    gsize body_len = 0;
    char *body = ns_js_module_fetch(js, ctx, module_name, json_module, &body_len);
    if (!body) return NULL;
    JSModuleDef *m = NULL;
    if (json_module) {
        m = ns_js_make_json_module(ctx, module_name, body, body_len);
    } else {
        JSValue func_val =
            ns_js_compile_module_cached(ctx, body, body_len, module_name);
        if (!JS_IsException(func_val)) {
            m = JS_VALUE_GET_PTR(func_val);
            JS_FreeValue(ctx, func_val);
        }
    }
    g_free(body);
    if (!m) ns_js_log_module_compile_error(js, ctx, module_name);
    return m;
}

static void
ns_js_eval_module(ns_js *js, const char *src, gsize len, const char *origin)
{
    ns_budget_guard bg = {0};
    ns_js_budget_push(js, &bg);
    JSContext *ctx = js->module_ctx ? js->module_ctx : js->ctx;
    char *copy = g_strndup(src ? src : "", len);
    gboolean profile = ns_js_profile_enabled();
    gint64 t0 = profile ? g_get_monotonic_time() : 0;
    js->eval_depth++;
    JSValue fn = ns_js_compile_module_cached(ctx, copy, len,
                                             origin ? origin : "module");
    if (!JS_IsException(fn) &&
        ns_js_module_set_import_meta(ctx, fn, TRUE) < 0) {
        JS_FreeValue(ctx, fn);
        fn = JS_EXCEPTION;
    }
    js->ignore_destructive_writes++;
    JSValue v = JS_IsException(fn) ? fn : JS_EvalFunction(ctx, fn);
    js->ignore_destructive_writes--;
    g_free(copy);
    js->eval_depth--;
    if (js->eval_depth == 0) {
        ns_js_flush_document_write(js);
        ns_js_schedule_pending_script_drain(js);
    }
    if (profile)
        g_printerr("[profile] js module   %6.1fms  %zub  %s\n",
                   (g_get_monotonic_time() - t0) / 1000.0, (size_t)len,
                   origin ? origin : "module");
    if (JS_IsException(v)) {
        JSValue ex = JS_GetException(ctx);
        const char *msg = JS_ToCString(ctx, ex);
        if (msg && js->log_cb) {
            char *line = g_strdup_printf("JS module error in %s: %s",
                                         origin ? origin : "module", msg);
            js->log_cb(line, js->log_user_data);
            g_free(line);
        }
        if (msg) JS_FreeCString(ctx, msg);
        JS_FreeValue(ctx, ex);
    } else {
        JSPromiseStateEnum st = JS_PromiseState(ctx, v);
        if (st == JS_PROMISE_REJECTED) {
            JSValue reason = JS_PromiseResult(ctx, v);
            const char *msg = JS_ToCString(ctx, reason);
            if (msg && js->log_cb) {
                char *line = g_strdup_printf(
                    "JS module rejected in %s: %s",
                    origin ? origin : "module", msg);
                js->log_cb(line, js->log_user_data);
                g_free(line);
            }
            if (msg) JS_FreeCString(ctx, msg);
            JS_FreeValue(ctx, reason);
        }
    }
    JS_FreeValue(ctx, v);
    ns_drain_microtasks(js);
    ns_js_budget_pop(js, &bg);
}

void
ns_js_eval_script_source(ns_js *js, ns_node *script, const char *source,
                         gsize length, const char *origin,
                         gboolean is_module)
{
    JSContext *realm = ns_js_node_realm_context(js, script);
    if (!realm || realm == js->ctx) {
        if (is_module) {
            ns_js_eval_module(js, source, length, origin);
        } else {
            ns_node *previous_script = js->current_script;
            js->current_script = script;
            ns_js_eval(js, source, length, origin);
            js->current_script = previous_script;
        }
        return;
    }

    ns_node *document = NULL;
    for (ns_node *p = script; p; p = p->parent) {
        if (p->kind == NS_NODE_DOCUMENT) {
            document = p;
            break;
        }
    }
    ns_node *previous_doc = js->current_doc;
    ns_node *previous_script = js->current_script;
    ns_frame_url fu;
    js->current_doc = document ? document : previous_doc;
    js->current_script = script;
    ns_frame_url_enter(js, &fu, origin);

    if (is_module) {
        JSContext *previous_module_ctx = js->module_ctx;
        js->module_ctx = realm;
        ns_js_eval_module(js, source, length, origin);
        js->module_ctx = previous_module_ctx;
    } else {
        ns_budget_guard budget = {0};
        ns_js_budget_push(js, &budget);
        g_autofree char *copy = g_strndup(source ? source : "", length);
        js->eval_depth++;
        JSValue value = JS_Eval(realm, copy, length,
                                origin ? origin : "inline",
                                JS_EVAL_TYPE_GLOBAL);
        js->eval_depth--;
        if (js->eval_depth == 0) {
            ns_js_flush_document_write(js);
            ns_js_schedule_pending_script_drain(js);
        }
        if (JS_IsException(value)) {
            JSValue exception = JS_GetException(realm);
            const char *message = JS_ToCString(realm, exception);
            if (message && js->log_cb) {
                JSValue stack_value = JS_GetPropertyStr(realm, exception,
                                                        "stack");
                const char *stack = JS_ToCString(realm, stack_value);
                char *line = g_strdup_printf("JS error in %s: %s%s%s",
                    origin ? origin : "inline", message,
                    stack && *stack ? "\n" : "", stack ? stack : "");
                js->log_cb(line, js->log_user_data);
                g_free(line);
                if (stack) JS_FreeCString(realm, stack);
                JS_FreeValue(realm, stack_value);
            }
            if (message) JS_FreeCString(realm, message);
            JS_FreeValue(realm, exception);
        }
        JS_FreeValue(realm, value);
        ns_drain_microtasks(js);
        ns_js_budget_pop(js, &budget);
    }

    ns_frame_url_leave(js, &fu);
    js->current_script = previous_script;
    js->current_doc = previous_doc;
}

/* The document.referrer of the document loading into iframe: the URL of
 * the document holding the iframe, cut down by the referrer policy. */
static char *
ns_js_frame_referrer(ns_js *js, ns_node *iframe, const char *frame_url)
{
    const ns_node *holder = iframe->parent;
    while (holder && holder->kind != NS_NODE_DOCUMENT) holder = holder->parent;
    const char *holder_url = holder && holder->parent
        ? ns_js_frame_url(js, holder->parent)
        : ns_js_top_url(js);
    const ns_config *cfg = ns_config_get();
    ns_referer_policy policy = cfg ? cfg->referer_policy
                                   : NS_REFERER_STRICT_ORIGIN_WHEN_CROSS;
    const char *attr = ns_element_get_attr(iframe, "referrerpolicy");
    if (attr && g_ascii_strcasecmp(attr, "no-referrer") == 0)
        policy = NS_REFERER_NO_REFERRER;
    else if (attr && g_ascii_strcasecmp(attr, "same-origin") == 0)
        policy = NS_REFERER_SAME_ORIGIN;
    else if (attr && g_ascii_strcasecmp(attr, "unsafe-url") == 0)
        policy = NS_REFERER_UNSAFE_URL;
    const char *srcdoc = ns_element_get_attr(iframe, "srcdoc");
    if (srcdoc && *srcdoc && policy != NS_REFERER_UNSAFE_URL) {
        /* about:srcdoc is never same-origin with its holder's URL. */
        char *origin = policy == NS_REFERER_STRICT_ORIGIN_WHEN_CROSS &&
                       ns_url_is_http_or_https(holder_url)
            ? ns_url_origin_from(holder_url) : NULL;
        char *out = origin ? g_strdup_printf("%s/", origin) : g_strdup("");
        g_free(origin);
        return out;
    }
    char *referrer = ns_net_referer_for(frame_url, holder_url, policy);
    return referrer ? referrer : g_strdup("");
}

static void
ns_js_mark_iframe_source(ns_js *js, ns_node *iframe, const char *origin,
                         const char *abs_url)
{
    const char *srcdoc = ns_element_get_attr(iframe, "srcdoc");
    const char *frame_url = srcdoc && *srcdoc ? origin
                          : abs_url && *abs_url ? abs_url : origin;
    if (js) {
        char *referrer = ns_js_frame_referrer(js, iframe, frame_url);
        ns_js_frame_set_source(js, iframe, frame_url ? frame_url : "",
                               referrer);
        g_free(referrer);
    }
    if (srcdoc && *srcdoc) {
        ns_element_set_attr(iframe, "data-nd-frame-srcdoc", srcdoc);
        ns_element_set_attr(iframe, "data-nd-frame-url", origin);
        ns_element_set_attr(iframe, "data-nd-frame-doc-url", "about:srcdoc");
        return;
    }
    ns_element_set_attr(iframe, "data-nd-frame-srcdoc", "");
    ns_element_set_attr(iframe, "data-nd-frame-url",
                        abs_url && *abs_url ? abs_url : origin);
    ns_element_set_attr(iframe, "data-nd-frame-doc-url",
                        abs_url && *abs_url ? "" : "about:blank");
    if (abs_url && *abs_url) ns_css_mark_visited(abs_url);
}

static const char *
ns_frame_src_attr(const ns_node *n)
{
    if (ns_node_is_element_named(n, "iframe")) return "src";
    if (ns_node_is_element_named(n, "object")) return "data";
    return NULL;
}

static gboolean
ns_js_iframe_source_loaded(ns_js *js, ns_node *iframe)
{
    if (!js || !iframe) return FALSE;
    if (!ns_element_get_attr(iframe, "data-nd-frame-loaded")) return FALSE;
    const char *srcdoc = ns_element_get_attr(iframe, "srcdoc");
    const char *loaded_srcdoc = ns_element_get_attr(iframe, "data-nd-frame-srcdoc");
    if (srcdoc && *srcdoc)
        return loaded_srcdoc && strcmp(loaded_srcdoc, srcdoc) == 0;

    const char *attr = ns_frame_src_attr(iframe);
    const char *src = attr ? ns_element_get_attr(iframe, attr) : NULL;
    if (!src || !*src || g_str_has_prefix(src, "about:"))
        return ns_node_is_element_named(iframe, "iframe");
    const char *origin = (js->current_url && *js->current_url)
                       ? js->current_url : "inline";
    char *abs_url = ns_url_resolve(origin, src);
    if (!abs_url) return FALSE;
    const char *loaded_url = ns_element_get_attr(iframe, "data-nd-frame-url");
    gboolean same = loaded_url && strcmp(loaded_url, abs_url) == 0;
    g_free(abs_url);
    return same;
}

static void
ns_js_schedule_iframe_load_full(ns_js *js, ns_node *iframe, gboolean force)
{
    if (!js || !iframe || js->halted) return;
    if (!ns_frame_src_attr(iframe)) return;
    if (!force && ns_js_iframe_source_loaded(js, iframe)) return;
    const char *loading = ns_element_get_attr(iframe, "loading");
    gboolean lazy = loading && g_ascii_strcasecmp(loading, "lazy") == 0;
    if (!force && lazy && js->iframe_load_depth == 0) {
        const ns_box *box = js->layout_root
            ? ns_box_find_by_dom(js->layout_root, iframe) : NULL;
        double limit = js->layout_root
            ? js->layout_root->scroll_y + ns_css_viewport_h() + 1500.0 : 0;
        if (!box || box->y > limit) {
            ns_js_deferred_iframe_add(js, iframe);
            return;
        }
    }
    if (force) {
        ns_element_remove_attr(iframe, "data-nd-frame-loaded");
        ns_css_mark_attr_dirty(iframe, "data-nd-frame-loaded", "1");
    }
    ns_js_deferred_iframe_remove(js, iframe);
    if (!ns_js_pending_iframe_add(js, iframe)) return;
    js->mutated = TRUE;
}

void
ns_js_schedule_iframe_load(ns_js *js, ns_node *iframe)
{
    ns_js_schedule_iframe_load_full(js, iframe, FALSE);
}

gboolean
ns_js_iframe_beyond_load_range(ns_js *js, ns_node *iframe)
{
    if (!js->layout_root) return TRUE;
    const ns_box *box = ns_box_find_by_dom(js->layout_root, iframe);
    double limit = js->layout_root->scroll_y + ns_css_viewport_h() + 1500.0;
    return !box || box->y > limit;
}

static gboolean
ns_subtree_has_wrapper(ns_node *root)
{
    GPtrArray *stack = g_ptr_array_new();
    g_ptr_array_add(stack, root);
    gboolean found = FALSE;
    while (stack->len > 0) {
        ns_node *n = g_ptr_array_index(stack, stack->len - 1);
        g_ptr_array_set_size(stack, stack->len - 1);
        if (n->js_wrapper) { found = TRUE; break; }
        if (n->tpl_content) g_ptr_array_add(stack, n->tpl_content);
        for (ns_node *c = n->first_child; c; c = c->next_sibling)
            g_ptr_array_add(stack, c);
    }
    g_ptr_array_free(stack, TRUE);
    return found;
}

static gboolean
ns_js_node_in_tree(const ns_node *n, const ns_node *root)
{
    for (const ns_node *p = n; p; p = p->parent)
        if (p == root) return TRUE;
    return FALSE;
}

static void
ns_js_purge_subtree_script_refs(ns_js *js, ns_node *root)
{
    if (!js || !root) return;
    ns_js_forget_script_roots_in(js, root);
    if (js->lifecycle_tasks) {
        guint i = 0;
        while (i < js->lifecycle_tasks->len) {
            ns_script_task *task =
                &g_array_index(js->lifecycle_tasks, ns_script_task, i);
            if (ns_js_node_in_tree(task->node, root))
                g_array_remove_index(js->lifecycle_tasks, i);
            else
                i++;
        }
    }
}

static void
ns_js_purge_subtree_node_refs(ns_js *js, ns_node *root)
{
    if (!js || !root) return;
    if (js->raf_pending) {
        for (guint i = js->raf_pending->len; i > 0; i--) {
            ns_raf_entry *e =
                &g_array_index(js->raf_pending, ns_raf_entry, i - 1);
            if (!e->media || !ns_js_node_in_tree(e->media, root)) continue;
            JS_FreeValue(e->ctx ? e->ctx : js->ctx, e->cb);
            g_array_remove_index(js->raf_pending, i - 1);
        }
    }
    ns_focus_forget_subtree(js, root);
}

static void
ns_js_sweep_orphans(ns_js *js)
{
    if (!js || !js->orphan_nodes || g_hash_table_size(js->orphan_nodes) == 0) return;
    if (js->eval_depth > 0 || js->callback_depth > 0 || js->dispatch_depth > 0)
        return;
    gint64 now = g_get_monotonic_time();
    if (js->last_orphan_sweep_us != 0 && now - js->last_orphan_sweep_us < 250000)
        return;
    js->last_orphan_sweep_us = now;

    JS_RunGC(js->rt);

    GPtrArray *to_free = g_ptr_array_new();
    GHashTableIter it; gpointer key;
    g_hash_table_iter_init(&it, js->orphan_nodes);
    while (g_hash_table_iter_next(&it, &key, NULL)) {
        ns_node *r = key;
        if (r->parent != NULL) continue;
        if (ns_subtree_has_wrapper(r)) continue;
        g_ptr_array_add(to_free, r);
    }
    for (guint i = 0; i < to_free->len; i++) {
        ns_node *r = g_ptr_array_index(to_free, i);
        g_hash_table_remove(js->orphan_nodes, r);
        ns_js_purge_subtree_rafs(js, r);
        ns_js_purge_subtree_pending_iframes(js, r);
        ns_js_purge_subtree_script_refs(js, r);
        ns_js_purge_subtree_node_refs(js, r);
        ns_node_free(r);
    }
    g_ptr_array_free(to_free, TRUE);
}

static void
ns_js_unpin_subtree(ns_js *js, ns_node *root)
{
    if (!js || !js->pinned_wrappers_set || !root) return;
    GPtrArray *stack = g_ptr_array_new();
    g_ptr_array_add(stack, root);
    while (stack->len > 0) {
        ns_node *n = g_ptr_array_index(stack, stack->len - 1);
        g_ptr_array_set_size(stack, stack->len - 1);
        for (ns_node *c = n->first_child; c; c = c->next_sibling)
            g_ptr_array_add(stack, c);
        void *wrap = n->js_wrapper;
        if (wrap && g_hash_table_remove(js->pinned_wrappers_set, n)) {
            JS_FreeValue(js->ctx, JS_MKPTR(JS_TAG_OBJECT, wrap));
        }
    }
    g_ptr_array_free(stack, TRUE);
}

static void
ns_js_iframe_clear_content(ns_js *js, ns_node *iframe)
{
    ns_js_purge_frame_rafs(js, iframe);
    ns_services_purge_frame_timers(js, iframe);
    ns_js_scrub_iframe_globals(js, iframe);
    ns_node *c = iframe->first_child;
    while (c) {
        ns_node *next = c->next_sibling;
        if (c->kind == NS_NODE_DOCUMENT) {
            ns_node_remove(c);
            ns_js_unpin_subtree(js, c);
            if (js->orphan_nodes) g_hash_table_add(js->orphan_nodes, c);
        }
        c = next;
    }
}

typedef struct ns_iframe_classic_source {
    char *text;
    gsize len;
    char *url;
    ns_node *script;
} ns_iframe_classic_source;

static void
ns_iframe_classic_source_free(gpointer data)
{
    ns_iframe_classic_source *source = data;
    if (!source) return;
    g_free(source->text);
    g_free(source->url);
    g_free(source);
}

static void
ns_iframe_classic_source_add(GPtrArray *sources, const char *text, gsize len,
                             const char *url, ns_node *script)
{
    if (!sources || !text || len == 0) return;
    ns_iframe_classic_source *source = g_new0(ns_iframe_classic_source, 1);
    source->text = g_strndup(text, len);
    source->len = len;
    source->url = g_strdup(url && *url ? url : "inline");
    source->script = script;
    g_ptr_array_add(sources, source);
}

static gboolean
ns_js_tree_has_inline_handlers(const ns_node *n, int depth)
{
    if (!n || depth >= NS_DOM_MAX_DEPTH) return FALSE;
    if (n->kind == NS_NODE_ELEMENT)
        for (const ns_attr *a = n->attrs; a; a = a->next)
            if (a->name && a->value && *a->value &&
                g_ascii_strncasecmp(a->name, "on", 2) == 0)
                return TRUE;
    for (const ns_node *c = n->first_child; c; c = c->next_sibling)
        if (ns_js_tree_has_inline_handlers(c, depth + 1)) return TRUE;
    return FALSE;
}

static void
ns_js_run_iframe_scripts(ns_js *js, ns_node *content_root,
                         const char *origin, JSValue iframe_doc,
                         JSValueConst iframe_scope, unsigned sandbox,
                         ns_node *iframe)
{
    GArray *tasks = g_array_new(FALSE, FALSE, sizeof(ns_script_task));
    ns_js_collect_script_tasks(content_root, tasks);
    GString *concat = g_string_new(NULL);
    GPtrArray *classic_sources =
        g_ptr_array_new_with_free_func(ns_iframe_classic_source_free);
    GPtrArray *modules = g_ptr_array_new();
    ns_iframe_exposed_names *exposed_names = ns_iframe_exposed_names_new();
    const guint expose_scan_cap = 131072;
    JSValue proto_snapshot = ns_js_iframe_proto_snapshot(js->ctx);
    ns_js_iframe_clear_global_zone(js->ctx);
    ns_js_iframe_restore_globals(js->ctx);

    for (guint i = 0; i < tasks->len; i++) {
        ns_node *n = g_array_index(tasks, ns_script_task, i).node;
        if (!n || ns_element_get_attr(n, NS_SCRIPT_ALREADY_STARTED)) continue;
        if (!ns_script_type_supported(n) || ns_script_skipped_by_nomodule(n)) {
            ns_element_set_attr(n, NS_SCRIPT_ALREADY_STARTED, "1");
            continue;
        }
        if (ns_script_type_is_module(n)) { g_ptr_array_add(modules, n); continue; }
        ns_element_set_attr(n, NS_SCRIPT_ALREADY_STARTED, "1");
        const char *src = ns_element_get_attr(n, "src");
        if (src && *src) {
            char *abs_url = ns_url_resolve(origin, src);
            if (abs_url) {
                GError *err = NULL;
                ns_perf_resource_info info = { 0 };
                ns_js_element_perf_info(js, n, &info);
                ns_response *r = ns_js_fetch_subresource(js, abs_url, origin,
                                                         NULL, &err, "script",
                                                         &info);
                gboolean nosniff_blocked = r &&
                    ns_net_header_is_nosniff(r->x_content_type_options) &&
                    !ns_content_type_is_javascript(r->content_type);
                if (!nosniff_blocked && r && r->body && r->body->len > 0 &&
                    !r->error && (r->status == 200 || r->status == 0)) {
                    if (r->body->len <= expose_scan_cap)
                        ns_iframe_exposed_names_scan(exposed_names,
                            (const char *)r->body->data, r->body->len);
                    ns_iframe_classic_source_add(classic_sources,
                        (const char *)r->body->data, r->body->len, abs_url, n);
                    g_string_append_len(concat, (const char *)r->body->data,
                                        (gssize)r->body->len);
                    g_string_append(concat, "\n;\n");
                }
                if (r) ns_response_free(r);
                g_clear_error(&err);
                g_free(abs_url);
            }
        } else {
            GString *inline_source = g_string_new(NULL);
            for (const ns_node *c = n->first_child; c; c = c->next_sibling)
                if (c->kind == NS_NODE_TEXT && c->text) {
                    gsize inline_len = strlen(c->text);
                    ns_iframe_exposed_names_scan(exposed_names, c->text,
                                                 inline_len);
                    g_string_append_len(inline_source, c->text,
                                        (gssize)inline_len);
                    g_string_append(concat, c->text);
                }
            ns_iframe_classic_source_add(classic_sources,
                inline_source->str, inline_source->len, origin, n);
            g_string_free(inline_source, TRUE);
            g_string_append(concat, "\n;\n");
        }
    }

    JSContext *fctx = NULL;
    JSValue fwin = JS_NULL, floc = JS_NULL, fhist = JS_NULL;
    gboolean has_inline_handlers =
        ns_js_tree_has_inline_handlers(content_root, 0);
    JSContext *initial_blank = iframe
        ? ns_js_frame_take_initial_blank(js, iframe) : NULL;
    gboolean reuse_blank = initial_blank && !(sandbox & NS_FRAME_CROSS_ORIGIN) &&
        ns_js_frame_context(js, iframe) == initial_blank;
    if (reuse_blank || classic_sources->len > 0 || modules->len > 0 ||
        has_inline_handlers)
        fctx = ns_iframe_make_realm_context(js, iframe, iframe_doc, origin,
                                            ns_iframe_doc_url(iframe), sandbox,
                                            reuse_blank ? initial_blank : NULL,
                                            &fwin, &floc, &fhist);

    if ((classic_sources->len > 0 || has_inline_handlers || reuse_blank) &&
        fctx && JS_IsObject(fwin)) {
        JSValue outward_window = JS_NULL;
        if ((sandbox & NS_FRAME_CROSS_ORIGIN) && iframe) {
            outward_window = ns_iframe_lookup_realm_window(js, iframe);
            if (!JS_IsObject(outward_window) && iframe->js_wrapper) {
                JS_FreeValue(js->ctx, outward_window);
                JSValue iw = JS_MKPTR(JS_TAG_OBJECT, iframe->js_wrapper);
                outward_window = JS_GetPropertyStr(js->ctx, iw, "__ndRealmWindow");
            }
        }
        gboolean preserve_outward = JS_IsObject(outward_window) &&
            JS_VALUE_GET_PTR(outward_window) != JS_VALUE_GET_PTR(fwin);
        if (preserve_outward)
            ns_window_link_outward(js, outward_window, fwin);
        if (JS_IsObject(iframe_doc))
            JS_SetPropertyStr(js->ctx, iframe_doc, "defaultView",
                              JS_DupValue(js->ctx, fwin));
        ns_iframe_store_realm_window(js, iframe,
            preserve_outward ? outward_window : fwin);
        if (iframe && iframe->js_wrapper) {
            JSValue iw = JS_MKPTR(JS_TAG_OBJECT, iframe->js_wrapper);
            JS_SetPropertyStr(js->ctx, iw, "__ndRealmWindow",
                              JS_DupValue(js->ctx,
                                  preserve_outward ? outward_window : fwin));
        }
        JS_FreeValue(js->ctx, outward_window);
        gint64 iframe_deadline_us =
            g_get_monotonic_time() + ns_js_eval_budget_us();
        /* The frame's scripts run in its realm scope, as its timers and
         * event handlers do, so work they start (a fetch, an XHR) belongs
         * to the frame's document. */
        ns_realm_scope frame_scope;
        gboolean in_frame_scope = iframe != NULL;
        if (in_frame_scope)
            ns_js_frame_scope_enter(js, fctx, iframe, &frame_scope);
        for (guint i = 0; i < classic_sources->len; i++) {
            if (g_get_monotonic_time() >= iframe_deadline_us) break;
            ns_iframe_classic_source *source =
                g_ptr_array_index(classic_sources, i);
            ns_node *previous_script = js->current_script;
            js->current_script = source->script;
            js->eval_deadline_us = iframe_deadline_us;
            js->eval_depth++;
            JSValue v = JS_Eval(fctx, source->text, source->len, source->url,
                                JS_EVAL_TYPE_GLOBAL);
            js->eval_depth--;
            js->eval_deadline_us = 0;
            js->current_script = previous_script;
            if (js->eval_depth == 0) {
                ns_js_flush_document_write(js);
                ns_js_schedule_pending_script_drain(js);
            }
            if (JS_IsException(v)) {
                JSValue ex = JS_GetException(fctx);
                const char *m = JS_ToCString(fctx, ex);
                if (m && js->log_cb) {
                    JSValue stk = JS_GetPropertyStr(fctx, ex, "stack");
                    const char *s = JS_ToCString(fctx, stk);
                    char *line = g_strdup_printf("JS error in %s: %s%s%s",
                                                 source->url, m,
                                                 s ? "\n" : "", s ? s : "");
                    js->log_cb(line, js->log_user_data);
                    g_free(line);
                    if (s) JS_FreeCString(fctx, s);
                    JS_FreeValue(fctx, stk);
                }
                if (m) JS_FreeCString(fctx, m);
                JS_FreeValue(fctx, ex);
            }
            JS_FreeValue(fctx, v);
        }
        if (in_frame_scope) ns_js_realm_scope_leave(js, &frame_scope);
    } else if (concat->len > 0) {
        GString *w = g_string_new(
            "(function(window,self,globalThis,top,parent,document,location,history){\nwith(window){\n");
        g_string_append_len(w, concat->str, (gssize)concat->len);
        g_string_append(w, "\n}\n");
        char *exposing = ns_iframe_exposed_names_script(exposed_names);
        if (exposing) g_string_append(w, exposing);
        g_free(exposing);
        g_string_append(w, "\n})");
        JSValue fn = JS_Eval(js->ctx, w->str, w->len, origin ? origin : "inline",
                             JS_EVAL_TYPE_GLOBAL);
        g_string_free(w, TRUE);
        if (!JS_IsException(fn) && JS_IsFunction(js->ctx, fn)) {
            JSValue g = JS_GetGlobalObject(js->ctx);
            JSValue scope = JS_IsObject(iframe_scope)
                ? JS_DupValue(js->ctx, iframe_scope)
                : ns_iframe_make_scope(js->ctx, iframe_doc, origin,
                                       ns_iframe_doc_url(iframe), sandbox);
            JSValue swin = JS_NULL, sloc = JS_NULL, shist = JS_NULL;
            if (JS_IsObject(scope)) {
                swin  = JS_GetPropertyStr(js->ctx, scope, "window");
                sloc  = JS_GetPropertyStr(js->ctx, scope, "location");
                shist = JS_GetPropertyStr(js->ctx, scope, "history");
            }
            if (!JS_IsObject(swin))  { JS_FreeValue(js->ctx, swin);  swin  = JS_DupValue(js->ctx, g); }
            if (!JS_IsObject(sloc))  { JS_FreeValue(js->ctx, sloc);  sloc  = JS_GetPropertyStr(js->ctx, g, "location"); }
            if (!JS_IsObject(shist)) { JS_FreeValue(js->ctx, shist); shist = JS_GetPropertyStr(js->ctx, g, "history"); }
            JSValueConst args[8] = { swin, swin, g, g, swin,
                                     iframe_doc, sloc, shist };
            js->eval_deadline_us = g_get_monotonic_time() + ns_js_eval_budget_us();
            js->eval_depth++;
            JSValue v = JS_Call(js->ctx, fn, swin, 8, args);
            js->eval_depth--;
            js->eval_deadline_us = 0;
            if (js->eval_depth == 0) {
                ns_js_flush_document_write(js);
                ns_js_schedule_pending_script_drain(js);
            }
            if (JS_IsException(v)) {
                JSValue ex = JS_GetException(js->ctx);
                const char *m = JS_ToCString(js->ctx, ex);
                if (m && js->log_cb) {
                    JSValue stk = JS_GetPropertyStr(js->ctx, ex, "stack");
                    const char *s = JS_ToCString(js->ctx, stk);
                    char *line = g_strdup_printf("JS error in %s: %s%s%s",
                                                 origin ? origin : "inline", m,
                                                 s ? "\n" : "", s ? s : "");
                    js->log_cb(line, js->log_user_data);
                    g_free(line);
                    if (s) JS_FreeCString(js->ctx, s);
                    JS_FreeValue(js->ctx, stk);
                }
                if (m) JS_FreeCString(js->ctx, m);
                JS_FreeValue(js->ctx, ex);
            }
            JS_FreeValue(js->ctx, v);
            JS_FreeValue(js->ctx, swin);
            JS_FreeValue(js->ctx, sloc);
            JS_FreeValue(js->ctx, shist);
            JS_FreeValue(js->ctx, scope);
            JS_FreeValue(js->ctx, g);
        } else if (JS_IsException(fn)) {
            JS_FreeValue(js->ctx, JS_GetException(js->ctx));
        }
        JS_FreeValue(js->ctx, fn);
    }

    if (modules->len > 0 && JS_IsObject(fwin)) {
        JSValue mscope = JS_NewObject(js->ctx);
        JS_SetPropertyStr(js->ctx, mscope, "window", JS_DupValue(js->ctx, fwin));
        JS_SetPropertyStr(js->ctx, mscope, "location", JS_DupValue(js->ctx, floc));
        JS_SetPropertyStr(js->ctx, mscope, "history", JS_DupValue(js->ctx, fhist));
        ns_js_run_iframe_modules(js, (ns_node **)modules->pdata, modules->len,
                                 origin, iframe_doc, mscope, sandbox);
        JS_FreeValue(js->ctx, mscope);
    } else {
        ns_js_run_iframe_modules(js, (ns_node **)modules->pdata, modules->len,
                                 origin, iframe_doc, iframe_scope, sandbox);
    }
    ns_js_iframe_proto_cleanup(js->ctx, proto_snapshot);
    ns_js_iframe_restore_globals(js->ctx);
    JS_FreeValue(js->ctx, proto_snapshot);

    if (fctx) {
        JS_FreeValue(fctx, fwin);
        JS_FreeValue(fctx, floc);
        JS_FreeValue(fctx, fhist);
    }
    g_string_free(concat, TRUE);
    g_ptr_array_free(classic_sources, TRUE);
    g_ptr_array_free(modules, TRUE);
    ns_iframe_exposed_names_free(exposed_names);
    g_array_free(tasks, TRUE);
}

static gboolean
ns_iframe_framing_blocked(const char *embedder_url, const char *framed_url,
                          ns_response *resp)
{
    if (!resp) return FALSE;
    if (resp->xframe_options && *resp->xframe_options) {
        g_autofree char *v = g_strdup(resp->xframe_options);
        g_strstrip(v);
        if (g_ascii_strcasecmp(v, "deny") == 0)
            return TRUE;
        if (g_ascii_strcasecmp(v, "sameorigin") == 0 &&
            !ns_url_same_origin(framed_url, embedder_url))
            return TRUE;
    }
    if (resp->csp_header && *resp->csp_header) {
        ns_csp *csp = ns_csp_parse(resp->csp_header);
        gboolean allowed = ns_csp_allows(csp, NS_CSP_FRAME_ANCESTORS,
                                         embedder_url, framed_url);
        ns_csp_free(csp);
        if (!allowed) return TRUE;
    }
    return FALSE;
}

/* Whether a frame's response is one a browser downloads instead of
 * showing: one sent as an attachment, or of a type it does not display. */
static gboolean
ns_frame_response_is_download(const ns_response *resp)
{
    if (!resp || resp->error) return FALSE;
    if (resp->content_disposition) {
        const char *cd = resp->content_disposition;
        while (g_ascii_isspace(*cd)) cd++;
        if (g_ascii_strncasecmp(cd, "attachment", 10) == 0) return TRUE;
    }
    const char *type = resp->content_type;
    if (!type || !*type) return FALSE;
    char *mime = g_ascii_strdown(type, strcspn(type, ";"));
    g_strstrip(mime);
    gboolean shown = !*mime || g_str_has_prefix(mime, "text/") ||
        g_str_has_prefix(mime, "image/") || g_str_has_prefix(mime, "video/") ||
        g_str_has_prefix(mime, "audio/") || g_str_has_suffix(mime, "+xml") ||
        strcmp(mime, "application/xml") == 0 ||
        strcmp(mime, "application/xhtml+xml") == 0 ||
        strcmp(mime, "application/json") == 0 ||
        strcmp(mime, "application/pdf") == 0;
    g_free(mime);
    return !shown;
}

static void
ns_js_load_iframe_now(ns_js *js, ns_node *iframe)
{
    if (!js || !iframe || js->halted || !js->current_doc) return;
    if (!ns_js_node_in_page(js, iframe)) return;

    if (ns_element_get_attr(iframe, "data-nd-doc-written")) {
        const char *sa = ns_frame_src_attr(iframe);
        const char *sv = sa ? ns_element_get_attr(iframe, sa) : NULL;
        const char *sd = ns_element_get_attr(iframe, "srcdoc");
        if ((!sv || !*sv) && (!sd || !*sd)) {
            ns_element_set_attr(iframe, "data-nd-frame-loaded", "1");
            ns_css_mark_attr_dirty(iframe, "data-nd-frame-loaded", NULL);
            ns_js_dispatch_resource_event(js, iframe, "load");
            return;
        }
    }

    unsigned sandbox = ns_iframe_effective_sandbox(iframe);
    gboolean scripts_ok = !(sandbox & NS_SANDBOX_ACTIVE) ||
                          (sandbox & NS_SANDBOX_ALLOW_SCRIPTS);

    const char *origin = (js->current_url && *js->current_url)
                       ? js->current_url : "inline";
    const char *src_attr = ns_frame_src_attr(iframe);
    const char *src    = src_attr ? ns_element_get_attr(iframe, src_attr) : NULL;
    const char *srcdoc = ns_element_get_attr(iframe, "srcdoc");

    char *abs_url = NULL;
    char *decoded = NULL;
    ns_response *resp = NULL;

    ns_js_start_frame_clock(js, iframe);
    if (srcdoc && *srcdoc) {
        decoded = g_strdup(srcdoc);
        abs_url = g_strdup(origin);
    } else if (src && *src && !g_str_has_prefix(src, "about:")) {
        abs_url = ns_url_resolve(origin, src);
        gboolean is_object = ns_node_is_element_named(iframe, "object");
        ns_csp_kind frame_kind = is_object ? NS_CSP_OBJECT : NS_CSP_FRAME;
        if (abs_url && js->csp &&
            !ns_csp_allows(js->csp, frame_kind, abs_url, origin)) {
            if (js->log_cb) {
                char *line = g_strdup_printf(
                    "Blocked %s %s by Content-Security-Policy %s",
                    is_object ? "object" : "iframe", abs_url,
                    is_object ? "object-src" : "frame-src");
                js->log_cb(line, js->log_user_data);
                g_free(line);
            }
            g_free(abs_url);
            abs_url = NULL;
        }
        if (abs_url) {
            int frame_depth = 0, same_url_ancestors = 0;
            for (const ns_node *p = iframe->parent; p; p = p->parent) {
                if (p->kind != NS_NODE_ELEMENT) continue;
                if (!ns_node_is_element_named(p, "iframe") &&
                    !ns_node_is_element_named(p, "frame") &&
                    !ns_node_is_element_named(p, "object"))
                    continue;
                frame_depth++;
                const char *au = ns_element_get_attr(p, "data-nd-frame-url");
                if (au && strcmp(au, abs_url) == 0) same_url_ancestors++;
            }
            if (frame_depth >= 20 || same_url_ancestors >= 2) {
                if (js->log_cb) {
                    char *line = g_strdup_printf(
                        "Blocked recursive/over-nested iframe %s "
                        "(depth=%d same-url=%d)",
                        abs_url, frame_depth, same_url_ancestors);
                    js->log_cb(line, js->log_user_data);
                    g_free(line);
                }
                g_free(abs_url);
                abs_url = NULL;
            }
        }
        if (abs_url) {
            GError *err = NULL;
            gint64 frame_fetch_us = g_get_monotonic_time();
            static const char *const iframe_dest[] = {
                "X-ND-Fetch-Dest: iframe", NULL };
            static const char *const frame_dest[] = {
                "X-ND-Fetch-Dest: frame", NULL };
            static const char *const object_dest[] = {
                "X-ND-Fetch-Dest: object", NULL };
            const char *const *dest_headers =
                is_object ? object_dest
                : (iframe->name && g_ascii_strcasecmp(iframe->name, "frame") == 0
                   ? frame_dest : iframe_dest);
            resp = ns_js_fetch_resource(js, abs_url, origin, dest_headers, &err);
            /* A response the frame would hand to the download manager
             * gets no resource timing entry, as in other browsers. */
            if (!ns_frame_response_is_download(resp)) {
                ns_perf_resource_info info = { 0 };
                ns_js_element_perf_info(js, iframe, &info);
                ns_perf_add_resource_timed(js, &info, abs_url,
                                           is_object ? "object" : "iframe",
                                           frame_fetch_us,
                                           g_get_monotonic_time(), resp);
            }
            if (resp && resp->final_url && *resp->final_url &&
                strcmp(resp->final_url, abs_url) != 0) {
                g_free(abs_url);
                abs_url = g_strdup(resp->final_url);
                if (js->csp &&
                    !ns_csp_allows(js->csp, frame_kind, abs_url, origin)) {
                    if (js->log_cb) {
                        char *line = g_strdup_printf(
                            "Blocked %s redirected to %s by "
                            "Content-Security-Policy %s",
                            is_object ? "object" : "iframe", abs_url,
                            is_object ? "object-src" : "frame-src");
                        js->log_cb(line, js->log_user_data);
                        g_free(line);
                    }
                    ns_response_free(resp);
                    resp = NULL;
                }
            }
            if (resp && ns_iframe_framing_blocked(origin, abs_url, resp)) {
                if (js->log_cb) {
                    char *line = g_strdup_printf(
                        "Blocked framing of %s (X-Frame-Options / "
                        "frame-ancestors)", abs_url);
                    js->log_cb(line, js->log_user_data);
                    g_free(line);
                }
            } else if (resp && resp->body &&
                (resp->status == 200 || resp->status == 0) && !resp->error) {
                if (resp->body->len > 0 && resp->content_type &&
                    g_ascii_strncasecmp(resp->content_type, "image/", 6) == 0 &&
                    strstr(resp->content_type, "xml") == NULL) {
                    decoded = ns_html_image_document(
                        resp->final_url ? resp->final_url : abs_url);
                    ns_element_set_attr(iframe, "data-nd-frame-charset", "UTF-8");
                } else if (resp->body->len > 0) {
                    decoded = ns_html_decode_body_full(
                        (const char *)resp->body->data, resp->body->len,
                        resp->content_type, NULL);
                    char *declared = ns_html_declared_charset(
                        (const char *)resp->body->data, resp->body->len,
                        resp->content_type);
                    ns_element_set_attr(iframe, "data-nd-frame-charset",
                                        declared ? declared : "UTF-8");
                    g_free(declared);
                } else if (resp->content_type &&
                           strstr(resp->content_type, "xml") != NULL) {
                    decoded = g_strdup("");
                    ns_element_set_attr(iframe, "data-nd-frame-charset", "UTF-8");
                }
            }
            else if (js->log_cb) {
                char *line = g_strdup_printf("iframe %s: %s", abs_url,
                    err ? err->message :
                    (resp && resp->error ? resp->error : "load failed"));
                js->log_cb(line, js->log_user_data);
                g_free(line);
            }
            g_clear_error(&err);
        }
    }

    if (ns_node_is_element_named(iframe, "object")) {
        gboolean doc_type = resp && resp->content_type &&
            (strstr(resp->content_type, "html") != NULL ||
             strstr(resp->content_type, "xml") != NULL);
        if (!doc_type) {
            if (resp) ns_response_free(resp);
            g_free(decoded);
            g_free(abs_url);
            return;
        }
    }

    ns_js_iframe_clear_content(js, iframe);
    ns_js_sweep_orphans(js);

    gboolean is_plain_xml = resp && resp->content_type
        && strstr(resp->content_type, "xml") != NULL
        && strstr(resp->content_type, "html") == NULL;

    gboolean is_xhtml = resp && resp->content_type
        && strstr(resp->content_type, "xhtml") != NULL;
    gboolean is_xml_content = is_xhtml || is_plain_xml;
    gboolean xhtml_suppress_scripts = FALSE;
    gboolean malformed_xml = FALSE;
    gboolean xml_empty = FALSE;
    if (is_xml_content && decoded && resp && resp->body) {
        char *xml_charset = ns_html_declared_charset(
            (const char *)resp->body->data, resp->body->len, resp->content_type);
        gboolean utf8_declared = !xml_charset ||
            g_ascii_strcasecmp(xml_charset, "UTF-8") == 0 ||
            g_ascii_strcasecmp(xml_charset, "UTF8") == 0;
        g_free(xml_charset);
        if (utf8_declared &&
            !g_utf8_validate((const char *)resp->body->data,
                             (gssize)resp->body->len, NULL)) {
            malformed_xml = TRUE;
            xhtml_suppress_scripts = TRUE;
        }
    }
    if (is_xml_content && decoded && !malformed_xml) {
        const char *q = decoded;
        while (*q && g_ascii_isspace((unsigned char)*q)) q++;
        if (!*q) {
            xml_empty = TRUE;
            xhtml_suppress_scripts = TRUE;
        } else {
            char *root_ns = NULL;
            if (!ns_xml_well_formed(decoded, -1, &root_ns)) {
                malformed_xml = TRUE;
                xhtml_suppress_scripts = TRUE;
            } else if (!root_ns ||
                       strcmp(root_ns, "http://www.w3.org/1999/xhtml") != 0) {
                xhtml_suppress_scripts = TRUE;
            }
            g_free(root_ns);
        }
    }

    ns_node *content_root = NULL;
    ns_node *content_doc = NULL;
    gboolean doc_is_xml = FALSE;
    if (xml_empty) {
        content_doc = ns_node_new_document();
        content_root = content_doc;
        doc_is_xml = TRUE;
        ns_node_append_child(iframe, content_doc);
    } else if (is_xml_content && !malformed_xml && decoded) {
        content_doc = ns_xml_parse(decoded, (gssize)strlen(decoded));
        if (content_doc) {
            for (ns_node *c = content_doc->first_child; c; c = c->next_sibling)
                if (c->kind == NS_NODE_ELEMENT) { content_root = c; break; }
            if (!content_root) {
                ns_node_free(content_doc);
                content_doc = NULL;
            } else {
                doc_is_xml = TRUE;
                ns_node_own_strings_deep(content_doc);
                ns_node_append_child(iframe, content_doc);
            }
        }
    }
    gboolean is_plaintext = resp && resp->content_type &&
        g_ascii_strncasecmp(resp->content_type, "text/plain", 10) == 0;
    if (!content_doc && decoded && is_plaintext) {
        content_doc = ns_node_new_document();
        ns_node *html = ns_node_new_element(g_strdup("html"));
        ns_node *head = ns_node_new_element(g_strdup("head"));
        ns_node *body = ns_node_new_element(g_strdup("body"));
        ns_node *pre  = ns_node_new_element(g_strdup("pre"));
        ns_node_append_child(pre, ns_node_new_text(g_strdup(decoded)));
        ns_node_append_child(body, pre);
        ns_node_append_child(html, head);
        ns_node_append_child(html, body);
        ns_node_append_child(content_doc, html);
        content_root = html;
        ns_node_own_strings_deep(content_doc);
        ns_node_append_child(iframe, content_doc);
    }
    if (!content_doc && decoded) {
        if (malformed_xml) {
            g_free(decoded);
            decoded = g_strdup(
                "<html><head><title>Parse Error</title></head><body>"
                "<p>This XML document is not well-formed.</p></body></html>");
        }
        ns_node *cdoc = ns_html_parse(decoded, (gssize)strlen(decoded));
        if (cdoc) {
            ns_node *html = ns_node_find_first_element(cdoc, "html");
            if (html) {
                content_doc = cdoc;
                content_root = html;
                ns_node_own_strings_deep(content_doc);
                ns_node_append_child(iframe, content_doc);
            } else {
                ns_node_free(cdoc);
            }
        }
    }

    gboolean blank_frame = FALSE;
    if (!content_doc && !decoded &&
        ns_node_is_element_named(iframe, "iframe")) {
        content_root = ns_iframe_ensure_content_root(iframe);
        if (content_root) content_doc = content_root->parent;
        /* A frame without a source keeps about:blank as its URL; the
         * creator's URL stands in for its base URL and origin. */
        if (!abs_url) {
            abs_url = g_strdup(origin);
            blank_frame = TRUE;
        }
    }

    if (content_root && content_doc) {
        ns_element_set_attr(iframe, "data-nd-frame-loaded", "1");
        ns_css_mark_attr_dirty(iframe, "data-nd-frame-loaded", NULL);

        const char *iorigin = abs_url && *abs_url ? abs_url : origin;
        ns_js_mark_iframe_source(js, iframe, origin,
                                 blank_frame ? NULL : abs_url);
        if ((iorigin && js->current_url &&
             !ns_url_same_origin(iorigin, js->current_url)) ||
            ((sandbox & NS_SANDBOX_ACTIVE) &&
             !(sandbox & NS_SANDBOX_ALLOW_SAME_ORIGIN)))
            sandbox |= NS_FRAME_CROSS_ORIGIN;
        const char *cs = ns_element_get_attr(iframe, "data-nd-frame-charset");
        if (!cs || !*cs) cs = "UTF-8";
        JSValue realm_doc = ns_make_realm_document(
            js->ctx, content_doc, iorigin, cs,
            resp ? resp->content_type : NULL, doc_is_xml, FALSE);
        ns_frame_document_show_url(js->ctx, realm_doc,
                                   ns_iframe_doc_url(iframe));
        if ((sandbox & NS_SANDBOX_ACTIVE) &&
            !(sandbox & NS_SANDBOX_ALLOW_SAME_ORIGIN))
            ns_realmdoc_deny_cookie(js->ctx, realm_doc);
        JSValue realm_scope = JS_NULL;
        if (JS_IsObject(realm_doc))
            realm_scope = ns_iframe_make_scope(js->ctx, realm_doc, iorigin,
                                               ns_iframe_doc_url(iframe),
                                               sandbox);
        if (JS_IsObject(realm_doc) && JS_IsObject(realm_scope)) {
            JSValue win = JS_GetPropertyStr(js->ctx, realm_scope, "window");
            if (JS_IsObject(win))
                JS_SetPropertyStr(js->ctx, realm_doc, "defaultView",
                                  JS_DupValue(js->ctx, win));
            JSValue rloc = JS_GetPropertyStr(js->ctx, realm_scope, "location");
            if (JS_IsObject(rloc))
                JS_SetPropertyStr(js->ctx, realm_doc, "location", rloc);
            else
                JS_FreeValue(js->ctx, rloc);
            JS_FreeValue(js->ctx, win);
        }
        if (JS_IsObject(realm_doc)) {
            JSValue iw = ns_make_element(js->ctx, iframe);
            JS_SetPropertyStr(js->ctx, iw, "__ndRealmDoc",
                              JS_DupValue(js->ctx, realm_doc));
            if (JS_IsObject(realm_scope)) {
                JSValue win = JS_GetPropertyStr(js->ctx, realm_scope, "window");
                if (JS_IsObject(win))
                    JS_SetPropertyStr(js->ctx, iw, "__ndRealmWindow",
                                      JS_DupValue(js->ctx, win));
                JS_FreeValue(js->ctx, win);
            }
            JS_FreeValue(js->ctx, iw);
        }

        ns_frame_url fu;
        ns_frame_url_enter(js, &fu, iorigin);
        ns_node *prev_doc = js->current_doc;
        js->current_doc = content_doc;
        if (content_doc) ns_doc_id_index_build(content_doc);
        ns_node *prev_script = js->current_script;
        JSValue prev_idoc = js->iframe_doc;
        int prev_idoc_set = js->iframe_doc_set;
        if (JS_IsObject(realm_doc)) {
            js->iframe_doc = realm_doc;
            js->iframe_doc_set = 1;
        }
        ns_node *prev_frame_ctx = js->raf_frame_ctx;
        js->raf_frame_ctx = iframe;
        GHashTable *globals_before = ns_js_snapshot_globals(js);
        js->iframe_load_depth++;
        ns_js_set_doc_ready_state(js, content_doc, 0);
        if (xhtml_suppress_scripts)
            ns_js_mark_scripts_already_started(content_root);
        if (!scripts_ok) {
            ns_js_mark_scripts_already_started(content_root);
            if (js->log_cb) {
                char *line = g_strdup_printf(
                    "Blocked scripts in sandboxed iframe %s "
                    "(missing allow-scripts)", iorigin);
                js->log_cb(line, js->log_user_data);
                g_free(line);
            }
        } else if (JS_IsObject(realm_doc)) {
            ns_js_run_iframe_scripts(js, content_root, iorigin, realm_doc,
                                     realm_scope, sandbox, iframe);
        } else {
            GArray *tasks = g_array_new(FALSE, FALSE, sizeof(ns_script_task));
            ns_js_collect_script_tasks(content_root, tasks);
            ns_js_run_parser_blocking_scripts(js, tasks, iorigin);
            ns_js_run_script_schedule(js, tasks, NS_SCRIPT_DEFERRED, iorigin);
            ns_js_run_script_schedule(js, tasks, NS_SCRIPT_ASYNC, iorigin);
            g_array_free(tasks, TRUE);
        }
        if (content_doc) {
            ns_js_set_doc_ready_state(js, content_doc, 1);
            ns_js_dispatch_event(js, content_doc, "readystatechange", NULL);
            ns_js_dispatch_event(js, content_doc, "DOMContentLoaded", NULL);
            ns_js_set_doc_ready_state(js, content_doc, 2);
            ns_js_dispatch_event(js, content_doc, "readystatechange", NULL);
            ns_js_dispatch_event(js, content_doc, "load", NULL);
            ns_js_fire_page_transition(js, "pageshow", FALSE);
        }
        js->iframe_load_depth--;
        ns_js_record_iframe_globals(js, iframe, globals_before);
        g_hash_table_destroy(globals_before);
        js->raf_frame_ctx = prev_frame_ctx;
        js->iframe_doc = prev_idoc;
        js->iframe_doc_set = prev_idoc_set;
        js->current_script = prev_script;
        js->current_doc = prev_doc;
        ns_frame_url_leave(js, &fu);
        JS_FreeValue(js->ctx, realm_scope);
        JS_FreeValue(js->ctx, realm_doc);
    }

    js->mutated = TRUE;
    if (content_root)
        ns_js_schedule_static_iframes(js, content_root);
    ns_js_schedule_pending_script_drain(js);
    ns_js_dispatch_resource_event(js, iframe, "load");

    if (resp) ns_response_free(resp);
    g_free(decoded);
    g_free(abs_url);
}

static void
ns_js_process_pending_iframes(ns_js *js)
{
    if (!js || js->halted || ns_js_pending_iframe_count(js) == 0) return;
    if (js->iframe_load_depth > 0 || js->eval_depth > 0) return;
    GHashTable *loaded = g_hash_table_new(g_direct_hash, g_direct_equal);
    while (!js->halted && ns_js_pending_iframe_count(js) > 0) {
        ns_node *iframe = ns_js_pending_iframe_first(js);
        if (!g_hash_table_add(loaded, iframe)) break;
        ns_js_pending_iframe_remove_first(js);
        ns_js_load_iframe_now(js, iframe);
    }
    g_hash_table_destroy(loaded);
}

static void
ns_js_schedule_static_iframes_rec(ns_js *js, ns_node *n, int depth)
{
    if (!n || depth >= 512) return;
    const char *frame_attr = ns_frame_src_attr(n);
    if (frame_attr) {
        if (!ns_element_get_attr(n, "data-nd-frame-loaded")) {
            const char *src    = ns_element_get_attr(n, frame_attr);
            const char *srcdoc = ns_element_get_attr(n, "srcdoc");
            if ((src && *src) || (srcdoc && *srcdoc) ||
                ns_node_is_element_named(n, "iframe"))
                ns_js_schedule_iframe_load(js, n);
        }
        return;
    }
    for (ns_node *c = n->first_child; c; c = c->next_sibling)
        ns_js_schedule_static_iframes_rec(js, c, depth + 1);
}

void
ns_js_schedule_static_iframes(ns_js *js, ns_node *n)
{
    ns_js_schedule_static_iframes_rec(js, n, 0);
}

static void
ns_js_lifecycle_clear(ns_js *js)
{
    if (!js) return;
    if (js->lifecycle_source) {
        ns_js_source_remove(js, js->lifecycle_source);
        js->lifecycle_source = 0;
    }
    if (js->lifecycle_tasks) {
        g_array_free(js->lifecycle_tasks, TRUE);
        js->lifecycle_tasks = NULL;
    }
    g_clear_pointer(&js->lifecycle_origin, g_free);
    js->lifecycle_doc = NULL;
    js->lifecycle_phase = 0;
    js->lifecycle_start_us = 0;
}

static gboolean ns_js_lifecycle_tick(gpointer data);

static void
ns_js_lifecycle_schedule(ns_js *js)
{
    if (!js || js->lifecycle_source) return;
    js->lifecycle_source =
        ns_js_attach_timeout(js, 0, ns_js_lifecycle_tick, js);
}

static gboolean
ns_js_lifecycle_has_blockers(ns_js *js)
{
    if (!js) return FALSE;
    return js->eval_depth > 0 || js->iframe_load_depth > 0 || js->in_pump ||
        ns_js_image_loads_pending(js) ||
        ns_js_pending_iframe_count(js) > 0 ||
        ns_js_has_pending_script_roots(js) ||
        (js->load_delay_cb && js->load_delay_cb(js->load_delay_user_data));
}

static void
ns_js_run_content_script(ns_js *js, const char *src)
{
    static const char *const natives[] = {
        "__nd_ext_manifest", "__nd_ext_base", "__nd_ext_sread",
        "__nd_ext_swrite", "__nd_ext_platform", "__nd_ext_uilang",
    };
    char *result = ns_js_eval_source(js, src, "content-script");
    g_free(result);
    if (!js || !js->ctx) return;
    JSValue global = JS_GetGlobalObject(js->ctx);
    for (gsize i = 0; i < G_N_ELEMENTS(natives); i++) {
        JSAtom atom = JS_NewAtom(js->ctx, natives[i]);
        if (JS_DeleteProperty(js->ctx, global, atom, 0) < 0)
            JS_FreeValue(js->ctx, JS_GetException(js->ctx));
        JS_FreeAtom(js->ctx, atom);
    }
    JS_FreeValue(js->ctx, global);
}

static gboolean
ns_js_lifecycle_tick(gpointer data)
{
    ns_js *js = data;
    if (!js) return G_SOURCE_REMOVE;
    js->lifecycle_source = 0;
    if (js->halted || !js->lifecycle_doc || !js->lifecycle_tasks) {
        ns_js_lifecycle_clear(js);
        return G_SOURCE_REMOVE;
    }
    if (js->eval_depth > 0 || js->callback_depth > 0 || js->in_pump) {
        js->lifecycle_source =
            ns_js_attach_timeout(js, 4, ns_js_lifecycle_tick, js);
        return G_SOURCE_REMOVE;
    }
    ns_node *doc = js->lifecycle_doc;
    const char *origin = js->lifecycle_origin && *js->lifecycle_origin
        ? js->lifecycle_origin : "inline";
    if (js->lifecycle_phase == 0) {
        ns_js_set_navigation_milestone(js,
            &js->navigation_timing.dom_interactive_ms, "domInteractive");
        js->ready_state = 1;
        ns_js_dispatch_event(js, doc, "readystatechange", NULL);
        js->lifecycle_phase = 1;
        ns_js_lifecycle_schedule(js);
        return G_SOURCE_REMOVE;
    }
    if (js->lifecycle_phase == 1) {
        if (ns_js_run_next_script_schedule(js, js->lifecycle_tasks,
                                           NS_SCRIPT_DEFERRED, origin)) {
            ns_js_lifecycle_schedule(js);
            return G_SOURCE_REMOVE;
        }
        ns_js_set_navigation_milestone(js,
            &js->navigation_timing.dom_content_loaded_event_start_ms,
            "domContentLoadedEventStart");
        ns_js_dispatch_event(js, doc, "DOMContentLoaded", NULL);
        ns_js_set_navigation_milestone(js,
            &js->navigation_timing.dom_content_loaded_event_end_ms,
            "domContentLoadedEventEnd");
        ns_ce_upgrade_subtree_all(js, doc);
        js->lifecycle_phase = 2;
        ns_js_lifecycle_schedule(js);
        return G_SOURCE_REMOVE;
    }
    if (js->lifecycle_phase == 2) {
        if (ns_js_run_next_script_schedule(js, js->lifecycle_tasks,
                                           NS_SCRIPT_ASYNC, origin)) {
            ns_js_lifecycle_schedule(js);
            return G_SOURCE_REMOVE;
        }
        ns_js_drain_load_event_scripts(js);
        ns_js_process_pending_iframes(js);
        ns_drain_microtasks(js);
        ns_js_schedule_pending_script_drain(js);
        js->lifecycle_phase = 3;
        ns_js_lifecycle_schedule(js);
        return G_SOURCE_REMOVE;
    }
    ns_js_drain_load_event_scripts(js);
    ns_js_process_pending_iframes(js);
    ns_drain_microtasks(js);
    if (ns_js_lifecycle_has_blockers(js)) {
        ns_js_lifecycle_schedule(js);
        return G_SOURCE_REMOVE;
    }
    ns_js_set_navigation_milestone(js,
        &js->navigation_timing.dom_complete_ms, "domComplete");
    js->ready_state = 2;
    if (js->rt) JS_RunGC(js->rt);
    ns_js_flush_autofocus(js);
    ns_js_dispatch_event(js, doc, "readystatechange", NULL);
    ns_js_set_navigation_milestone(js,
        &js->navigation_timing.load_event_start_ms, "loadEventStart");
    ns_js_dispatch_event(js, doc, "load", NULL);
    ns_js_set_navigation_milestone(js,
        &js->navigation_timing.load_event_end_ms, "loadEventEnd");
    ns_js_fire_page_transition(js, "pageshow", FALSE);
    ns_ce_upgrade_subtree_all(js, doc);
    JSValue global = JS_GetGlobalObject(js->ctx);
    g_autofree char *content_script =
        ns_ext_content_scripts_for_url(js->ctx, global,
                                       js->lifecycle_origin, FALSE);
    JS_FreeValue(js->ctx, global);
    if (content_script)
        ns_js_run_content_script(js, content_script);
    if (ns_js_profile_enabled())
        g_printerr("[profile] js lifecycle total=%.1fms\n",
                   (g_get_monotonic_time() - js->lifecycle_start_us) / 1000.0);
    ns_js_lifecycle_clear(js);
    return G_SOURCE_REMOVE;
}

/* Starts tracking the document's own <img> elements the way script-made
 * images are tracked, so each gets its load or error event, and its
 * resource timing entry, when the image cache finishes with it, and the
 * window's load event waits for them.  Images whose source depends on
 * layout (srcset, <picture>) or that load lazily are left to layout. */
static void
ns_js_track_document_images(ns_js *js, ns_node *n, int depth)
{
    if (!n || depth >= 512 || (depth > 0 && ns_dom_hidden_child(n))) return;
    if (ns_node_is_element_named(n, "img")) {
        const char *src = ns_element_get_attr(n, "src");
        const char *loading = ns_element_get_attr(n, "loading");
        if (src && *src && !ns_element_get_attr(n, "srcset") &&
            !(loading && g_ascii_strcasecmp(loading, "lazy") == 0) &&
            !ns_node_is_element_named(n->parent, "picture") &&
            !(js->js_image_loads && g_hash_table_contains(js->js_image_loads, n)) &&
            !(n->flags & NS_NODE_IMG_LOAD_FIRED))
            ns_js_start_image_load(js, n, src);
        return;
    }
    if (ns_node_is_element_named(n, "template")) return;
    for (ns_node *c = n->first_child; c; c = c->next_sibling)
        ns_js_track_document_images(js, c, depth + 1);
}

void
ns_js_run_scripts_in_doc(ns_js *js, ns_node *doc, const char *base_url_borrowed)
{
    if (!js || !doc) return;
    g_autofree char *base_url = g_strdup(base_url_borrowed);
    gboolean profile = ns_js_profile_enabled();
    if (profile)
        g_printerr("[profile] js run_scripts_in_doc start halted=%d in_pump=%d url=%s\n",
                   js->halted, js->in_pump, base_url ? base_url : "(null)");
    if (js->halted || js->in_pump) return;
    ns_js_lifecycle_clear(js);
    js->ready_state = 0;
    if (js->doc_ready_states) g_hash_table_remove_all(js->doc_ready_states);
    ns_js_frames_clear_initial_blank(js);
    ns_js_set_navigation_milestone(js,
        &js->navigation_timing.dom_loading_ms, "domLoading");
    gint64 t0 = g_get_monotonic_time();
    ns_js_install_document(js, doc, base_url);
    if (js->platform_globals) g_hash_table_destroy(js->platform_globals);
    js->platform_globals = ns_js_snapshot_globals(js);
    {
        const char *early = g_getenv("NS_EARLY_JS_FILE");
        char *early_src = NULL;
        if (early && *early &&
            g_file_get_contents(early, &early_src, NULL, NULL)) {
            char *r = ns_js_eval_source(js, early_src, "early-inject");
            g_free(r);
            g_free(early_src);
        }
    }
    if (js->early_inject_src && *js->early_inject_src) {
        char *r = ns_js_eval_source(js, js->early_inject_src, "early-inject");
        g_free(r);
    }
    {
        JSValue global = JS_GetGlobalObject(js->ctx);
        g_autofree char *cs =
            ns_ext_content_scripts_for_url(js->ctx, global,
                                           base_url && *base_url ? base_url : NULL,
                                           TRUE);
        JS_FreeValue(js->ctx, global);
        if (cs) ns_js_run_content_script(js, cs);
    }
    ns_js_schedule_static_iframes(js, doc);
    ns_js_track_document_images(js, doc, 0);
    {
        /* Stylesheets the engine fetched before this document's realm
         * existed go into its resource timing now. */
        GPtrArray *sheets = ns_engine_take_resource_timings(base_url);
        for (guint i = 0; i < sheets->len; i++) {
            const ns_engine_resource_timing *t = g_ptr_array_index(sheets, i);
            /* A frame's sheets belong to the frame's own timeline. */
            if (t->in_frame) continue;
            ns_perf_resource_info info = { 0 };
            ns_js_element_perf_info(js, doc, &info);
            info.render_blocking = t->render_blocking;
            ns_perf_add_resource_timed(js, &info, t->url, t->initiator,
                                       t->start_us, t->end_us, t->resp);
        }
        g_ptr_array_free(sheets, TRUE);
    }
    const char *origin = base_url && *base_url ? base_url : "inline";
    GArray *tasks = g_array_new(FALSE, FALSE, sizeof(ns_script_task));
    ns_js_register_import_maps(js, doc);
    ns_js_collect_script_tasks(doc, tasks);
    ns_js_run_parser_blocking_scripts(js, tasks, origin);
    js->lifecycle_tasks = tasks;
    js->lifecycle_doc = doc;
    js->lifecycle_origin = g_strdup(origin);
    js->lifecycle_phase = 0;
    js->lifecycle_start_us = t0;
    ns_js_lifecycle_schedule(js);
}

void
ns_js_set_form_submit_cb(ns_js *js, ns_js_form_submit_cb cb, gpointer user_data)
{
    if (!js) return;
    js->form_submit_cb = cb;
    js->form_submit_user_data = user_data;
}

void
ns_js_set_download_cb(ns_js *js, ns_js_download_cb cb, gpointer user_data)
{
    if (!js) return;
    js->download_cb = cb;
    js->download_user_data = user_data;
}

void
ns_js_set_clipboard_write_cb(ns_js *js, ns_js_clipboard_write_cb cb,
                             gpointer user_data)
{
    if (!js) return;
    js->clipboard_write_cb = cb;
    js->clipboard_write_user_data = user_data;
}

void
ns_js_set_selection_cmd_cb(ns_js *js, ns_js_selection_cmd_cb cb,
                           gpointer user_data)
{
    if (!js) return;
    js->selection_cmd_cb = cb;
    js->selection_cmd_user_data = user_data;
}

void
ns_js_set_selection(ns_js *js, const char *text, gboolean has_range,
                    double x, double y, double w, double h)
{
    if (!js) return;
    g_free(js->selection_text);
    js->selection_text = g_strdup(text ? text : "");
    js->selection_has_range = has_range;
    js->selection_x = x;
    js->selection_y = y;
    js->selection_w = w;
    js->selection_h = h;
}

gboolean
ns_js_selection_state(const ns_js *js, const char **text, double rect[4])
{
    *text = js->selection_text;
    rect[0] = js->selection_x;
    rect[1] = js->selection_y;
    rect[2] = js->selection_w;
    rect[3] = js->selection_h;
    return js->selection_has_range;
}

void
ns_js_track_orphan(ns_js *js, ns_node *node)
{
    if (js && js->orphan_nodes) g_hash_table_add(js->orphan_nodes, node);
}

void
ns_js_set_scroll_to_cb(ns_js *js, ns_js_scroll_to_cb cb, gpointer user_data)
{
    if (!js) return;
    js->scroll_to_cb = cb;
    js->scroll_to_user_data = user_data;
}

void
ns_js_set_fragment_nav_cb(ns_js *js, ns_js_fragment_nav_cb cb,
                          gpointer user_data)
{
    if (!js) return;
    js->fragment_nav_cb = cb;
    js->fragment_nav_user_data = user_data;
}

void
ns_js_set_soft_nav_cb(ns_js *js, ns_js_soft_nav_cb cb, gpointer user_data)
{
    if (!js) return;
    js->soft_nav_cb = cb;
    js->soft_nav_user_data = user_data;
}

void
ns_js_set_window_action_cb(ns_js *js, ns_js_window_action_cb cb,
                           gpointer user_data)
{
    if (!js) return;
    js->window_action_cb = cb;
    js->window_action_user_data = user_data;
}

void
ns_js_set_early_inject_src(ns_js *js, const char *src)
{
    if (!js) return;
    g_free(js->early_inject_src);
    js->early_inject_src = g_strdup(src);
}

gboolean
ns_js_wpt_hooks_enabled(const ns_js *js)
{
    return js && js->early_inject_src;
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

void
ns_js_set_viewport_scroll_cb(ns_js *js, ns_js_viewport_scroll_cb cb,
                             gpointer user_data)
{
    if (!js) return;
    js->viewport_scroll_cb = cb;
    js->viewport_scroll_user_data = user_data;
}

void
ns_js_set_layout_flush_cb(ns_js *js, ns_js_layout_flush_cb cb, gpointer user_data)
{
    if (!js) return;
    js->layout_flush_cb = cb;
    js->layout_flush_user_data = user_data;
}

void
ns_js_set_load_delay_cb(ns_js *js, gboolean (*cb)(gpointer), gpointer user_data)
{
    if (!js) return;
    js->load_delay_cb = cb;
    js->load_delay_user_data = user_data;
}

void
ns_js_flush_layout(ns_js *js)
{
    if (!js || !js->layout_flush_cb || js->in_layout_flush) return;
    js->in_layout_flush = TRUE;
    js->layout_flush_cb(js->layout_flush_user_data);
    js->in_layout_flush = FALSE;
}

void
ns_js_flush_style(ns_js *js)
{
    ns_js_flush_layout(js);
}


const char *
ns_js_current_url(const ns_js *js)
{
    return js && js->current_url ? js->current_url : "";
}

void
ns_js_set_current_url(ns_js *js, const char *url)
{
    char *copy = g_strdup(url);
    g_free(js->current_url);
    js->current_url = copy;
}

void
ns_js_soft_navigate(ns_js *js, const char *url, gboolean replace)
{
    if (js->soft_nav_cb)
        js->soft_nav_cb(url, replace, js->soft_nav_user_data);
}

gboolean
ns_js_navigate(ns_js *js, const char *url, gboolean reload)
{
    if (!js->nav_cb) return FALSE;
    js->nav_cb(url, reload, js->nav_user_data);
    return TRUE;
}

JSContext *
ns_js_main_realm_context(const ns_js *js)
{
    return js->main_realm_ctx;
}

const char *
ns_js_document_origin(const ns_js *js)
{
    return js->document_origin;
}

void *
ns_js_realm_url_enter(ns_js *js, JSContext *realm)
{
    const char *url = ns_js_realm_url(js, realm);
    if (!js || !url || !*url) return NULL;
    g_autofree char *copy = g_strdup(url);
    ns_frame_url *fu = g_new0(ns_frame_url, 1);
    ns_frame_url_enter(js, fu, copy);
    return fu;
}

void
ns_js_realm_url_leave(ns_js *js, void *token)
{
    ns_frame_url *fu = token;
    if (!fu) return;
    ns_frame_url_leave(js, fu);
    g_free(fu);
}

void
ns_js_dispatch_main_window_event(ns_js *js, const char *type, JSValue event)
{
    JSContext *main_ctx = js->main_realm_ctx ? js->main_realm_ctx : js->ctx;
    ns_node *main_doc = js->current_doc;
    while (main_doc && main_doc->parent) {
        main_doc = main_doc->parent;
        while (main_doc && main_doc->kind != NS_NODE_DOCUMENT)
            main_doc = main_doc->parent;
    }
    JSContext *saved_ctx = js->ctx;
    ns_node *saved_doc = js->current_doc;
    js->ctx = main_ctx;
    js->current_doc = main_doc;
    ns_js_dispatch_window_only_event(js, main_doc, type, event, NULL);
    js->ctx = saved_ctx;
    js->current_doc = saved_doc;
}

gboolean
ns_js_in_frame_load(const ns_js *js)
{
    return js->iframe_load_depth > 0;
}

gboolean
ns_js_can_navigate(const ns_js *js)
{
    return js->nav_cb != NULL;
}

void
ns_js_fragment_navigated(ns_js *js, const char *url)
{
    if (js->fragment_nav_cb)
        js->fragment_nav_cb(url, js->fragment_nav_user_data);
}

gboolean
ns_js_window_events_blocked(const ns_js *js)
{
    return !js->current_doc || js->halted || js->in_pump;
}

void
ns_js_dispatch_document_window_event(ns_js *js, const char *type, JSValue event)
{
    ns_js_dispatch_window_only_event(js, js->current_doc, type, event, NULL);
}

const char *
ns_js_storage_partition(const ns_js *js)
{
    return js ? js->partition_key : NULL;
}

gboolean
ns_js_halted(const ns_js *js)
{
    return js && js->halted;
}

void
ns_js_in_main_realm(ns_js *js, void (*fn)(void *data), void *data)
{
    ns_realm_scope scope;
    ns_js_realm_scope_enter(js, js->main_realm_ctx, &scope);
    fn(data);
    ns_js_realm_scope_leave(js, &scope);
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

JSContext *
ns_js_main_realm(const ns_js *js)
{
    return js->main_realm_ctx;
}

JSContext *
ns_js_main_context(const ns_js *js)
{
    return js->ctx;
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

JSClassID
ns_js_storage_class_id(void)
{
    return ns_storage_class_id;
}

JSClassID
ns_js_window_named_class_id(void)
{
    return ns_window_named_class_id;
}

GMainContext *
ns_js_glib_context(const ns_js *js)
{
    return js ? js->main_context : NULL;
}

const ns_box *
ns_js_layout_root(const ns_js *js)
{
    return js->layout_root;
}

gboolean
ns_js_is_worker(const ns_js *js)
{
    return js->worker_host != NULL;
}

void
ns_js_mark_mutated(ns_js *js)
{
    if (js) js->mutated = TRUE;
}

gboolean
ns_js_autofocus_processed(const ns_js *js)
{
    return js && js->autofocus_processed;
}

void
ns_js_set_autofocus_processed(ns_js *js)
{
    if (js) js->autofocus_processed = TRUE;
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

const char *
ns_js_referrer(const ns_js *js)
{
    return js->referrer;
}

int
ns_js_ready_state(const ns_js *js)
{
    return js ? js->ready_state : 0;
}

int
ns_js_ignore_destructive_writes(const ns_js *js)
{
    return js ? js->ignore_destructive_writes : 0;
}

const ns_node *
ns_js_active_modal(const ns_js *js)
{
    return js ? js->active_modal : NULL;
}

gboolean
ns_js_has_window_action(const ns_js *js)
{
    return js && js->window_action_cb;
}

void
ns_js_set_active_modal(ns_js *js, const ns_node *modal)
{
    if (!js) return;
    js->active_modal = modal;
    ns_dom_set_active_modal(modal);
}

int
ns_js_doc_ready_state(const ns_js *js, const ns_node *doc)
{
    gpointer state = NULL;
    if (js->doc_ready_states &&
        g_hash_table_lookup_extended(js->doc_ready_states, doc, NULL, &state))
        return GPOINTER_TO_INT(state);
    return -1;
}

const ns_node *
ns_js_current_script(const ns_js *js)
{
    return js->current_script;
}

void
ns_js_unorphan_node(ns_js *js, ns_node *n)
{
    g_hash_table_remove(js->orphan_nodes, n);
}

GHashTable *
ns_js_style_table(const ns_js *js)
{
    return js ? js->style_table : NULL;
}

struct ns_anim *
ns_js_anim(const ns_js *js)
{
    return js ? js->anim : NULL;
}

const ns_js_navigation_timing *
ns_js_page_navigation_timing(const ns_js *js)
{
    return &js->navigation_timing;
}

gboolean
ns_js_log_enabled(const ns_js *js)
{
    return js->log_cb != NULL;
}

int
ns_js_eval_depth(const ns_js *js)
{
    return js ? js->eval_depth : 0;
}

int
ns_js_callback_depth(const ns_js *js)
{
    return js ? js->callback_depth : 0;
}

int
ns_js_dispatch_depth(const ns_js *js)
{
    return js ? js->dispatch_depth : 0;
}

const ns_csp *
ns_js_page_csp(const ns_js *js)
{
    return js ? js->csp : NULL;
}

void
ns_js_rescan_pending_images(ns_js *js, ns_node *root)
{
    if (js && js->js_image_loads && g_hash_table_size(js->js_image_loads) > 0)
        ns_js_rescan_subtree_images(js, root, 0);
}

void
ns_js_log_line(ns_js *js, const char *line)
{
    if (js->log_cb) js->log_cb(line, js->log_user_data);
}

const char *
ns_js_net_page_url(const ns_js *js)
{
    return js ? js->current_url : NULL;
}

gboolean
ns_js_net_csp_allows_connect(const ns_js *js, const char *url, const char *page)
{
    return !js || !js->csp || ns_csp_allows(js->csp, NS_CSP_CONNECT, url, page);
}

gpointer
ns_js_net_enter_handler_realm(JSContext *ctx, JSValueConst obj, const char *type)
{
    ns_realm_scope *scope = g_new0(ns_realm_scope, 1);
    ns_js_realm_scope_enter(js_from_ctx(ctx),
                            ns_target_handler_realm(ctx, obj, type, "fn"), scope);
    return scope;
}

void
ns_js_net_leave_handler_realm(JSContext *ctx, gpointer scope)
{
    ns_js_realm_scope_leave(js_from_ctx(ctx), scope);
    g_free(scope);
}

void
ns_js_realm_scope_push(ns_js *js, JSContext *realm, void *buf)
{
    G_STATIC_ASSERT(sizeof(ns_realm_scope) <= NS_JS_SCOPE_BYTES);
    ns_js_realm_scope_enter(js, realm, buf);
}

void
ns_js_realm_scope_pop(ns_js *js, void *buf)
{
    ns_js_realm_scope_leave(js, buf);
}

typedef struct {
    JSContext   *ctx;
    ns_node     *doc;
    ns_node     *frame;
    gboolean     entered;
    ns_frame_url url;
} ns_dispatch_scope;

void
ns_js_dispatch_scope_enter(ns_js *js, JSContext *ctx, ns_node *doc,
                           ns_node *frame, void *buf)
{
    G_STATIC_ASSERT(sizeof(ns_dispatch_scope) <= NS_JS_SCOPE_BYTES);
    ns_dispatch_scope *s = buf;
    s->ctx = js->ctx;
    s->doc = js->current_doc;
    s->frame = js->raf_frame_ctx;
    s->entered = frame != NULL;
    js->ctx = ctx;
    if (frame) {
        js->current_doc = doc;
        js->raf_frame_ctx = frame;
        ns_frame_url_enter(js, &s->url,
                           ns_element_get_attr(frame, "data-nd-frame-url"));
    }
}

void
ns_js_dispatch_scope_leave(ns_js *js, void *buf)
{
    ns_dispatch_scope *s = buf;
    if (s->entered) ns_frame_url_leave(js, &s->url);
    js->raf_frame_ctx = s->frame;
    js->current_doc = s->doc;
    js->ctx = s->ctx;
}

void
ns_js_set_realm(ns_js *js, JSContext *ctx, ns_node *doc)
{
    js->ctx = ctx;
    js->current_doc = doc;
}

void
ns_js_dispatch_depth_add(ns_js *js, int delta)
{
    js->dispatch_depth += delta;
}

JSValue
ns_js_net_host_state(JSContext *ctx, JSValueConst v, int kind)
{
    ns_hostobj *d = ns_ho_data(v);
    return d && (int)d->kind == kind ? JS_DupValue(ctx, d->state) : JS_UNDEFINED;
}

gboolean
ns_js_net_pump_iteration(ns_js *js)
{
    if (js && js->halted) return FALSE;
    g_main_context_iteration(js && js->main_context ? js->main_context : NULL,
                             TRUE);
    return TRUE;
}

gboolean
ns_js_net_host_is(JSValueConst v, int kind)
{
    ns_hostobj *d = ns_ho_data(v);
    return d && (int)d->kind == kind;
}

JSContext *
ns_js_pattern_context(void)
{
    ns_js *js = ns_active_js();
    return js ? (js->main_realm_ctx ? js->main_realm_ctx : js->ctx) : NULL;
}

const ns_node *
ns_js_current_document(const ns_js *js)
{
    return js->current_doc;
}

gboolean
ns_js_events_suspended(const ns_js *js)
{
    return js->halted || js->in_pump;
}

void
ns_js_submit_form(ns_js *js, const ns_node *form, const ns_node *submitter)
{
    if (js->form_submit_cb)
        js->form_submit_cb(form, submitter, js->form_submit_user_data);
}

void
ns_js_dispatch_hashchange(ns_js *js, const char *old_url, const char *new_url)
{
    if (!js || !js->ctx) return;
    if (js->halted || js->in_pump || js->in_hashchange) return;
    js->in_hashchange = TRUE;
    /* The top-level window's hash changed, possibly through a frame's
     * parent.location, so the event goes to the top-level window. */
    ns_realm_scope scope;
    ns_js_realm_scope_enter(js, js->main_realm_ctx, &scope);
    JSContext *ctx = js->ctx;
    JSValue global = JS_GetGlobalObject(ctx);
    JSValue ev = ns_make_window_event(ctx, "hashchange");
    JS_SetPropertyStr(ctx, ev, "oldURL",
                      JS_NewString(ctx, old_url ? old_url : ""));
    JS_SetPropertyStr(ctx, ev, "newURL",
                      JS_NewString(ctx, new_url ? new_url : ""));
    if (js->current_doc) {
        ns_js_dispatch_window_only_event(js, js->current_doc, "hashchange",
                                         JS_DupValue(ctx, ev), NULL);
    } else {
        JSValue handler = JS_GetPropertyStr(ctx, global, "onhashchange");
        if (JS_IsFunction(ctx, handler)) {
            JSValueConst args[1] = { ev };
            JSValue r = JS_Call(ctx, handler, global, 1, args);
            if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
            JS_FreeValue(ctx, r);
        }
        JS_FreeValue(ctx, handler);
    }
    JS_FreeValue(ctx, ev);
    JS_FreeValue(ctx, global);
    ns_js_realm_scope_leave(js, &scope);
    js->in_hashchange = FALSE;
}

gboolean
ns_js_consume_mutated(ns_js *js)
{
    if (!js) return FALSE;
    gboolean m = js->mutated;
    js->mutated = FALSE;
    return m;
}

char *
ns_js_eval_source(ns_js *js, const char *src, const char *origin)
{
    if (!js || !src) return NULL;
    if (js->halted || js->in_pump) return NULL;
    ns_budget_guard bg = {0};
    ns_js_budget_push(js, &bg);
    JSValue v = JS_Eval(js->ctx, src, strlen(src), origin ? origin : "console", JS_EVAL_TYPE_GLOBAL);
    char *out = NULL;
    if (JS_IsException(v)) {
        JSValue ex = JS_GetException(js->ctx);
        const char *msg = JS_ToCString(js->ctx, ex);
        out = g_strdup_printf("error: %s", msg ? msg : "(no message)");
        if (msg) JS_FreeCString(js->ctx, msg);
        JS_FreeValue(js->ctx, ex);
    } else {
        const char *s = JS_ToCString(js->ctx, v);
        out = g_strdup(s ? s : "undefined");
        if (s) JS_FreeCString(js->ctx, s);
    }
    JS_FreeValue(js->ctx, v);
    ns_drain_microtasks(js);
    ns_js_budget_pop(js, &bg);
    return out;
}
