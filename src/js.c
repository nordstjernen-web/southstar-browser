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

gboolean
ns_js_selection_copyable(const ns_js *js)
{
    return js && js->selection_has_range && js->clipboard_write_cb != NULL;
}

gboolean
ns_js_copy_selection(ns_js *js)
{
    if (!ns_js_selection_copyable(js)) return FALSE;
    return js->clipboard_write_cb(js->selection_text ? js->selection_text : "",
                                  js->clipboard_write_user_data);
}

gboolean
ns_js_has_selection_commands(const ns_js *js)
{
    return js && js->selection_cmd_cb != NULL;
}

gboolean
ns_js_selection_command(ns_js *js, const char *cmd)
{
    return js && js->selection_cmd_cb &&
           js->selection_cmd_cb(cmd, js->selection_cmd_user_data);
}

void
ns_js_consume_user_activation(ns_js *js)
{
    if (js) js->user_activation_us = 0;
}

typedef struct ns_hostobj {
    ns_ho_kind kind;
    JSValue    state;
    gpointer   native;
    void     (*free_native)(JSRuntime *rt, gpointer native);
} ns_hostobj;
static ns_hostobj *ns_ho_of(JSValueConst v, ns_ho_kind kind);
typedef struct {
    char *saved;
    gboolean owns_slot;
} ns_frame_url;
static void ns_frame_url_enter(ns_js *js, ns_frame_url *fu, const char *url);
static void ns_frame_url_leave(ns_js *js, ns_frame_url *fu);

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

gint64
ns_js_eval_budget_us(void)
{
    const ns_config *c = ns_config_get();
    int ms = c ? c->js_eval_budget_ms : 60000;
    if (ms <= 0) ms = 60000;
    if (ms > NS_JS_EVAL_BUDGET_MAX_MS) ms = NS_JS_EVAL_BUDGET_MAX_MS;
    return (gint64)ms * 1000LL;
}

#define NS_JS_MONITOR_LIMIT_US (60LL * 1000000LL)

void
ns_js_deadlines(const ns_js *js, gint64 *eval_deadline_us,
                gint64 *monitor_deadline_us)
{
    *eval_deadline_us = js->eval_deadline_us;
    *monitor_deadline_us = js->js_monitor_deadline_us;
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

ns_response *
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

void
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

JSValue
ns_style_decl_new(JSContext *ctx, JSValueConst element)
{
    JSValue obj = JS_NewObjectClass(ctx, ns_style_class_id);
    if (JS_IsException(obj)) return obj;
    ns_style_back *b = g_new0(ns_style_back, 1);
    b->element = JS_DupValue(ctx, element);
    JS_SetOpaque(obj, b);
    return obj;
}

gboolean
ns_is_style_decl(JSValueConst v)
{
    return JS_GetOpaque(v, ns_style_class_id) != NULL;
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

JSValue
ns_token_list_new(JSContext *ctx, JSValueConst element, const char *attr)
{
    JSValue obj = JS_NewObjectClass(ctx, ns_token_list_class_id);
    if (JS_IsException(obj)) return obj;
    ns_token_list_back *b = g_new0(ns_token_list_back, 1);
    b->element = JS_DupValue(ctx, element);
    b->attr = attr;
    JS_SetOpaque(obj, b);
    return obj;
}

gboolean
ns_is_token_list(JSValueConst v)
{
    return JS_GetOpaque(v, ns_token_list_class_id) != NULL;
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

void
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
    if (js) ns_js_unorphan_node(js, n);
    if (js) ns_js_images_forget_node(js, n);
    if (js) ns_focus_forget_node(js, n);
    if (js) ns_js_forget_pending_change_for(js, n);
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


static ns_node *
ns_unwrap_element_mut(JSValueConst val)
{
    return JS_GetOpaque(val, ns_element_class_id);
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


#define NS_SCRIPT_ALREADY_STARTED "data-nd-script-already-started"
#define NS_SCRIPT_EMPTY_SOURCE "data-nd-script-empty-source"

/* Platform objects whose state the page must not see.  What the engine
 * stores on one (its C functions and hidden-source scripts, see
 * JS_IsHostAccess) lives in a state object of its own, as if it were the
 * object's own properties, so the code that builds and drives these
 * objects keeps using plain property access.  The page sees the interface's
 * prototype accessors and nothing else; what the page defines on the object
 * is the object's own. */
static JSClassID ns_hostobj_class_id;

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

int
ns_ho_kind_of(JSValueConst v)
{
    ns_hostobj *d = ns_ho_data(v);
    return d ? (int)d->kind : NS_HO_NONE;
}

static ns_hostobj *
ns_ho_of(JSValueConst v, ns_ho_kind kind)
{
    ns_hostobj *d = ns_ho_data(v);
    return d && d->kind == kind ? d : NULL;
}

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

JSValue
ns_ho_new(JSContext *ctx, int kind, JSValueConst proto)
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

struct ns_raf_scope {
    ns_js     *js;
    JSContext *previous_ctx;
    ns_node   *previous_doc;
    ns_node   *previous_frame;
    ns_node   *frame;
    ns_frame_url fu;
};

ns_raf_scope *
ns_js_raf_scope_enter(ns_js *js, JSContext *ctx, ns_node *frame)
{
    if (frame && ns_node_root(frame) != ns_node_root(js->current_doc))
        return NULL;
    JSContext *current_realm = frame ? ns_js_node_realm_context(js, frame)
                                     : NULL;
    ns_raf_scope *scope = g_new0(ns_raf_scope, 1);
    scope->js = js;
    scope->previous_ctx = js->ctx;
    scope->previous_doc = js->current_doc;
    scope->previous_frame = js->raf_frame_ctx;
    scope->frame = frame;
    js->ctx = current_realm ? current_realm : (ctx ? ctx : js->ctx);
    js->raf_frame_ctx = frame;
    if (frame) {
        ns_node *frame_doc = ns_iframe_document_node(frame);
        if (frame_doc) js->current_doc = frame_doc;
        ns_frame_url_enter(js, &scope->fu,
                           ns_element_get_attr(frame, "data-nd-frame-url"));
    }
    js->eval_deadline_us = g_get_monotonic_time() + ns_js_eval_budget_us();
    return scope;
}

JSContext *
ns_js_raf_scope_context(const ns_raf_scope *scope)
{
    return scope->js->ctx;
}

void
ns_js_raf_scope_leave(ns_raf_scope *scope)
{
    ns_js *js = scope->js;
    ns_drain_microtasks(js);
    if (scope->frame) ns_frame_url_leave(js, &scope->fu);
    js->raf_frame_ctx = scope->previous_frame;
    js->current_doc = scope->previous_doc;
    js->ctx = scope->previous_ctx;
    g_free(scope);
}

void
ns_js_callback_depth_add(ns_js *js, int delta)
{
    js->callback_depth += delta;
}

gboolean
ns_js_mutated(const ns_js *js)
{
    return js->mutated;
}

gboolean
ns_js_observer_tick_pending(const ns_js *js)
{
    return js->observer_tick_source != 0;
}

gboolean
ns_js_message_tasks_pending(const ns_js *js)
{
    return js->message_tasks && !g_queue_is_empty(js->message_tasks);
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
ns_js_forget_frame_context(ns_js *js, const ns_node *frame)
{
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

void
ns_js_set_anim(ns_js *js, struct ns_anim *anim)
{
    if (!js) return;
    js->anim = anim;
    ns_js_raf_set_host_driven(js);
}

void
ns_js_set_image_cache(ns_js *js, struct ns_image_cache *cache)
{
    if (!js) return;
    js->image_cache = (ns_image_cache *)cache;
    if (cache) ns_js_blob_registry_add(js);
    else ns_js_blob_registry_remove(js);
}

static const JSCFunctionListEntry ns_validity_proto_funcs[] = {
    JS_CGETSET_DEF("valid", ns_validity_get_valid, NULL),
};

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
        JSValue el = ns_form_indexed_control(ctx, obj, (uint32_t)idx);
        if (JS_IsException(el)) return -1;
        if (JS_IsUndefined(el)) return 0;
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
    JSValue result = ns_form_named_control(ctx, obj, name);
    JS_FreeCString(ctx, name);
    if (JS_IsException(result)) return -1;
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

ns_image_cache *
ns_js_image_cache(const ns_js *js)
{
    return js->image_cache;
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
    ns_js_brand_node_interfaces(fctx, ns_element_class_id, ns_attr_class_id);
    JSValue parent_performance =
        JS_GetPropertyStr(fctx, parent_global, "performance");
    gboolean include_memory = JS_IsObject(parent_performance) &&
        ns_obj_has_defined_own_prop(fctx, parent_performance, "memory");
    JS_FreeValue(fctx, parent_performance);
    JS_SetPropertyStr(fctx, fg, "performance",
                      ns_make_performance_object(fctx, js, include_memory));
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
    JS_CFUNC_DEF("cancelAsync",             1, ns_services_clear_timer),
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

const ns_node *
ns_js_iframe_document_node(const ns_js *js)
{
    return js->iframe_doc_set ? ns_unwrap_element(js->iframe_doc) : NULL;
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

void
ns_js_install_dom_members(JSContext *ctx, JSValueConst proto, int which)
{
    static const JSCFunctionListEntry track_accessors[] = {
        JS_CGETSET_MAGIC_DEF("default", ns_element_bool_attr_getter,
                             ns_element_bool_attr_setter, 3),
    };
    switch (which) {
    case NS_DOM_MEMBERS_ELEMENT_FUNCTIONS:
        JS_SetPropertyFunctionList(ctx, proto, ns_element_proto_funcs,
                                   G_N_ELEMENTS(ns_element_proto_funcs));
        break;
    case NS_DOM_MEMBERS_ELEMENT_CLASS:
        JS_SetClassProto(ctx, ns_element_class_id, JS_DupValue(ctx, proto));
        ns_bind_fn(ctx, proto, "matches",               ns_element_matches, 1);
        ns_bind_fn(ctx, proto, "webkitMatchesSelector", ns_element_matches, 1);
        ns_bind_fn(ctx, proto, "closest",               ns_element_closest, 1);
        break;
    case NS_DOM_MEMBERS_HTML_ELEMENT:
        ns_proto_define_getset(ctx, proto, "dataset",
                               ns_element_get_dataset, NULL);
        ns_proto_define_getset(ctx, proto, "popover",
                               ns_element_get_popover, ns_element_set_popover);
        break;
    case NS_DOM_MEMBERS_DATASET:
        ns_proto_define_getset(ctx, proto, "dataset",
                               ns_element_get_dataset, NULL);
        break;
    case NS_DOM_MEMBERS_TEXT:
        ns_proto_define_getset(ctx, proto, "assignedSlot",
                               ns_element_get_assignedSlot,
                               ns_element_noop_set);
        break;
    case NS_DOM_MEMBERS_DOCUMENT_FRAGMENT:
        ns_bind_fn(ctx, proto, "querySelector",
                   ns_element_querySelector, 1);
        ns_bind_fn(ctx, proto, "querySelectorAll",
                   ns_element_querySelectorAll, 1);
        break;
    case NS_DOM_MEMBERS_TRACK:
        JS_SetPropertyFunctionList(ctx, proto, track_accessors,
                                   G_N_ELEMENTS(track_accessors));
        break;
    case NS_DOM_MEMBERS_DIALOG:
        ns_proto_define_getset(ctx, proto, "closedBy",
                               ns_dialog_get_closedBy,
                               ns_dialog_set_closedBy);
        break;
    case NS_DOM_MEMBERS_BUTTON:
        ns_proto_define_getset(ctx, proto, "command",
                               ns_button_get_command,
                               ns_button_set_command);
        ns_proto_define_getset(ctx, proto, "commandForElement",
                               ns_button_get_commandForElement,
                               ns_button_set_commandForElement);
        break;
    case NS_DOM_MEMBERS_TEXT_PROPERTY:
        ns_proto_define_getset(ctx, proto, "text",
                               ns_element_get_text, ns_element_set_text);
        break;
    case NS_DOM_MEMBERS_DATA_PROPERTY:
        ns_proto_define_getset(ctx, proto, "data",
                               ns_element_get_data, ns_element_set_data);
        break;
    case NS_DOM_MEMBERS_VALUE_PROPERTY:
        ns_proto_define_getset(ctx, proto, "value",
                               ns_element_get_value_prop,
                               ns_element_set_value_prop);
        break;
    case NS_DOM_MEMBERS_OPTIONS_METHODS:
        JS_DefinePropertyValueStr(ctx, proto, "item",
            JS_NewCFunction(ctx, ns_options_item, "item", 1),
            JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
        JS_DefinePropertyValueStr(ctx, proto, "namedItem",
            JS_NewCFunction(ctx, ns_options_namedItem, "namedItem", 1),
            JS_PROP_CONFIGURABLE | JS_PROP_WRITABLE);
        break;
    default:
        break;
    }
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
            JSValue kind_proto = ns_node_kind_proto(js, n);
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
    js->iframe_doc = JS_UNDEFINED;
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
ns_js_set_callbacks(ns_js *js, ns_js_log_cb log_cb, gpointer log_user_data,
                    ns_js_mutated_cb mut_cb, gpointer mut_user_data,
                    ns_js_navigate_cb nav_cb, gpointer nav_user_data)
{
    js->log_cb = log_cb;
    js->log_user_data = log_user_data;
    js->mut_cb = mut_cb;
    js->mut_user_data = mut_user_data;
    js->nav_cb = nav_cb;
    js->nav_user_data = nav_user_data;
}

void
ns_js_init_tables(ns_js *js, GMainContext *main_context, gboolean page)
{
    js->main_context = main_context;
    js->pinned_wrappers_set = g_hash_table_new(g_direct_hash, g_direct_equal);
    if (!page) return;
    js->frame_ctxs = g_ptr_array_new();
    js->cookie_buckets = g_hash_table_new_full(
        g_str_hash, g_str_equal, g_free, g_free);
}

void
ns_js_set_worker_host(ns_js *js, ns_worker_host *host, const char *partition_key)
{
    js->worker_host = host;
    js->partition_key = g_strdup(partition_key);
}

JSValue
ns_js_register_class(JSContext *ctx, int which)
{
    JSRuntime *rt = JS_GetRuntime(ctx);
    JSValue proto;
    switch (which) {
    case NS_JS_CLASS_ELEMENT:
        ns_new_class_id(&ns_element_class_id);
        JS_NewClass(rt, ns_element_class_id, &ns_element_class);
        proto = JS_NewObject(ctx);
        JS_SetPropertyFunctionList(ctx, proto, ns_element_proto_funcs,
                                   G_N_ELEMENTS(ns_element_proto_funcs));
        ns_define_element_unscopables(ctx, proto);
        JS_SetClassProto(ctx, ns_element_class_id, JS_DupValue(ctx, proto));
        return proto;
    case NS_JS_CLASS_ATTR:
        ns_new_class_id(&ns_attr_class_id);
        JS_NewClass(rt, ns_attr_class_id, &ns_attr_class);
        JS_SetClassProto(ctx, ns_attr_class_id, JS_NewObject(ctx));
        return JS_UNDEFINED;
    case NS_JS_CLASS_STYLE:
        ns_new_class_id(&ns_style_class_id);
        JS_NewClass(rt, ns_style_class_id, &ns_style_class);
        proto = JS_NewObject(ctx);
        ns_cssom_install_style_proto(ctx, proto);
        ns_set_tostring_tag(ctx, proto, "CSSStyleDeclaration");
        JS_SetClassProto(ctx, ns_style_class_id, proto);
        return JS_UNDEFINED;
    case NS_JS_CLASS_TOKEN_LIST:
        ns_new_class_id(&ns_token_list_class_id);
        JS_NewClass(rt, ns_token_list_class_id, &ns_token_list_class);
        proto = JS_NewObject(ctx);
        JS_SetPropertyFunctionList(ctx, proto, ns_tlist_proto_funcs,
                                   G_N_ELEMENTS(ns_tlist_proto_funcs));
        JS_SetClassProto(ctx, ns_token_list_class_id, JS_DupValue(ctx, proto));
        return proto;
    case NS_JS_CLASS_LIVE:
        ns_new_class_id(&ns_live_class_id);
        JS_NewClass(rt, ns_live_class_id, &ns_live_class);
        ns_live_install_protos(ctx);
        JS_SetClassProto(ctx, ns_live_class_id, ns_live_proto(ctx, 0));
        return JS_UNDEFINED;
    case NS_JS_CLASS_DATASET:
        ns_new_class_id(&ns_dataset_class_id);
        JS_NewClass(rt, ns_dataset_class_id, &ns_dataset_class);
        JS_SetClassProto(ctx, ns_dataset_class_id, JS_NewObject(ctx));
        return JS_UNDEFINED;
    case NS_JS_CLASS_STORAGE:
        ns_new_class_id(&ns_storage_class_id);
        JS_NewClass(rt, ns_storage_class_id, &ns_storage_class);
        proto = JS_NewObject(ctx);
        ns_storage_install_proto(ctx, proto);
        JS_SetClassProto(ctx, ns_storage_class_id, JS_DupValue(ctx, proto));
        return proto;
    case NS_JS_CLASS_WINDOW_NAMED:
        ns_new_class_id(&ns_window_named_class_id);
        JS_NewClass(rt, ns_window_named_class_id, &ns_window_named_class);
        return JS_UNDEFINED;
    default:
        return JS_UNDEFINED;
    }
}

JSValue
ns_js_new_window_named(JSContext *ctx)
{
    return JS_NewObjectClass(ctx, ns_window_named_class_id);
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

void
ns_js_set_doc_ready_state(ns_js *js, const ns_node *doc, int state)
{
    if (!js || !doc) return;
    if (!js->doc_ready_states)
        js->doc_ready_states = g_hash_table_new(g_direct_hash, g_direct_equal);
    g_hash_table_insert(js->doc_ready_states, (gpointer)doc,
                        GINT_TO_POINTER(state));
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
    JS_CFUNC_DEF("execCommand", 3, ns_services_exec_command),
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
    JS_CFUNC_DEF("queryCommandSupported", 1, ns_services_query_command_supported),
    JS_CFUNC_DEF("queryCommandEnabled",   1, ns_services_query_command_enabled),
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

static const struct {
    const JSCFunctionListEntry *funcs;
    int count;
} ns_proto_tables[] = {
    [NS_PROTO_TABLE_DOCUMENT_METHODS] = {
        ns_document_proto_methods, G_N_ELEMENTS(ns_document_proto_methods) },
    [NS_PROTO_TABLE_DOCUMENT_ACCESSORS] = {
        ns_document_proto_accessors, G_N_ELEMENTS(ns_document_proto_accessors) },
    [NS_PROTO_TABLE_NAMED_NODE_MAP] = {
        ns_namedmap_proto_funcs, G_N_ELEMENTS(ns_namedmap_proto_funcs) },
    [NS_PROTO_TABLE_SRC] = {
        ns_src_proto_funcs, G_N_ELEMENTS(ns_src_proto_funcs) },
    [NS_PROTO_TABLE_IMAGE] = {
        ns_image_proto_funcs, G_N_ELEMENTS(ns_image_proto_funcs) },
    [NS_PROTO_TABLE_SOURCE] = {
        ns_source_proto_funcs, G_N_ELEMENTS(ns_source_proto_funcs) },
    [NS_PROTO_TABLE_MEDIA] = {
        ns_media_proto_funcs, G_N_ELEMENTS(ns_media_proto_funcs) },
    [NS_PROTO_TABLE_VIDEO] = {
        ns_video_proto_funcs, G_N_ELEMENTS(ns_video_proto_funcs) },
    [NS_PROTO_TABLE_IFRAME] = {
        ns_iframe_proto_funcs, G_N_ELEMENTS(ns_iframe_proto_funcs) },
    [NS_PROTO_TABLE_ANCHOR] = {
        ns_anchor_proto_funcs, G_N_ELEMENTS(ns_anchor_proto_funcs) },
    [NS_PROTO_TABLE_HREF] = {
        ns_href_proto_funcs, G_N_ELEMENTS(ns_href_proto_funcs) },
    [NS_PROTO_TABLE_DOM_IMPLEMENTATION] = {
        ns_dom_implementation_proto_funcs,
        G_N_ELEMENTS(ns_dom_implementation_proto_funcs) },
    [NS_PROTO_TABLE_VALIDITY] = {
        ns_validity_proto_funcs, G_N_ELEMENTS(ns_validity_proto_funcs) },
    [NS_PROTO_TABLE_ELEMENT] = {
        ns_element_proto_funcs, G_N_ELEMENTS(ns_element_proto_funcs) },
};

void
ns_js_install_proto_table(JSContext *ctx, JSValueConst proto, int table)
{
    if (table < 0 || table >= (int)G_N_ELEMENTS(ns_proto_tables)) return;
    JS_SetPropertyFunctionList(ctx, proto, ns_proto_tables[table].funcs,
                               ns_proto_tables[table].count);
}

JSValue
ns_js_host_class_proto(JSContext *ctx, int which)
{
    switch (which) {
    case NS_HOST_PROTO_STYLE:      return JS_GetClassProto(ctx, ns_style_class_id);
    case NS_HOST_PROTO_DATASET:    return JS_GetClassProto(ctx, ns_dataset_class_id);
    case NS_HOST_PROTO_TOKEN_LIST: return JS_GetClassProto(ctx, ns_token_list_class_id);
    default:                       return JS_UNDEFINED;
    }
}

void
ns_js_brand_node_classes(JSContext *ctx)
{
    ns_js_brand_node_interfaces(ctx, ns_element_class_id, ns_attr_class_id);
}

JSValue
ns_js_new_document_object(ns_js *js)
{
    JSValue document = JS_NewObjectClass(js->ctx, ns_element_class_id);
    if (js->current_doc) JS_SetOpaque(document, js->current_doc);
    return document;
}

void
ns_js_set_document(ns_js *js, ns_node *doc, char *origin)
{
    js->current_doc = doc;
    js->ce_main_doc = doc;
    js->autofocus_processed = FALSE;
    js->active_modal = NULL;
    g_free(js->document_origin);
    js->document_origin = origin;
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
ns_js_clear_doc_ready_states(ns_js *js)
{
    if (js->doc_ready_states) g_hash_table_remove_all(js->doc_ready_states);
}

const char *
ns_js_early_inject_src(const ns_js *js)
{
    return js->early_inject_src;
}

void
ns_js_free_doc_ready_states(ns_js *js)
{
    g_clear_pointer(&js->doc_ready_states, g_hash_table_destroy);
}

void
ns_js_free_page_strings(ns_js *js)
{
    if (js->csp) { ns_csp_free(js->csp); js->csp = NULL; }
    g_clear_pointer(&js->early_inject_src, g_free);
    g_clear_pointer(&js->cookie_value, g_free);
    g_clear_pointer(&js->referrer, g_free);
    g_clear_pointer(&js->current_url, g_free);
    g_clear_pointer(&js->document_origin, g_free);
    g_clear_pointer(&js->selection_text, g_free);
}

void
ns_js_stop_observer_tick(ns_js *js)
{
    if (js->observer_tick_source) {
        g_source_remove(js->observer_tick_source);
        js->observer_tick_source = 0;
    }
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
    g_clear_pointer(&js->box_lookup_cache, g_hash_table_destroy);
}

void
ns_js_free_window_values(ns_js *js)
{
    JS_FreeValue(js->ctx, js->navigator_brand);
    js->navigator_brand = JS_UNDEFINED;
    if (js->message_tasks) {
        g_queue_free(js->message_tasks);
        js->message_tasks = NULL;
    }
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

void
ns_js_link_interfaces(JSContext *ctx)
{
    JSValue r = JS_Eval(ctx, ns_interface_links_src,
                        sizeof(ns_interface_links_src) - 1, "<interface-links>",
                        JS_EVAL_TYPE_GLOBAL | JS_EVAL_FLAG_HIDE_SOURCE);
    if (JS_IsException(r)) JS_FreeValue(ctx, JS_GetException(ctx));
    JS_FreeValue(ctx, r);
}

void
ns_js_sweep_orphans(ns_js *js)
{
    if (!js || ns_js_orphan_count(js) == 0) return;
    if (js->eval_depth > 0 || js->callback_depth > 0 || js->dispatch_depth > 0)
        return;
    gint64 now = g_get_monotonic_time();
    if (js->last_orphan_sweep_us != 0 && now - js->last_orphan_sweep_us < 250000)
        return;
    js->last_orphan_sweep_us = now;

    JS_RunGC(js->rt);

    GPtrArray *to_free = ns_js_orphans_collect(js);
    if (!to_free) return;
    for (guint i = 0; i < to_free->len; i++) {
        ns_node *r = g_ptr_array_index(to_free, i);
        ns_js_purge_subtree_rafs(js, r);
        ns_js_purge_subtree_pending_iframes(js, r);
        ns_js_purge_subtree_script_refs(js, r);
        ns_js_purge_subtree_node_refs(js, r);
        ns_node_free(r);
    }
    g_ptr_array_free(to_free, TRUE);
}

void
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

void
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

int
ns_js_iframe_load_depth(const ns_js *js)
{
    return js->iframe_load_depth;
}

gboolean
ns_js_load_delayed(ns_js *js)
{
    return js->load_delay_cb && js->load_delay_cb(js->load_delay_user_data);
}

void
ns_js_set_ready_state(ns_js *js, int state)
{
    js->ready_state = state;
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
ns_js_page_url(const ns_js *js)
{
    return js->current_url;
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
ns_js_request_download(ns_js *js, const char *url, const char *filename)
{
    if (!js->download_cb) return FALSE;
    js->download_cb(url, filename, js->download_user_data);
    return TRUE;
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

void
ns_js_frame_scope_push(ns_js *js, JSContext *realm, ns_node *frame, void *buf)
{
    G_STATIC_ASSERT(sizeof(ns_realm_scope) <= NS_JS_SCOPE_BYTES);
    ns_js_frame_scope_enter(js, realm, frame, buf);
}

typedef struct {
    ns_frame_url url;
    ns_node     *doc;
    ns_node     *script;
    JSValue      iframe_doc;
    int          iframe_doc_set;
    ns_node     *frame;
} ns_frame_load_scope;

void *
ns_js_frame_load_enter(ns_js *js, ns_node *frame, ns_node *doc,
                       const char *url, JSValueConst realm_doc)
{
    ns_frame_load_scope *s = g_new0(ns_frame_load_scope, 1);
    ns_frame_url_enter(js, &s->url, url);
    s->doc = js->current_doc;
    js->current_doc = doc;
    if (doc) ns_doc_id_index_build(doc);
    s->script = js->current_script;
    s->iframe_doc = js->iframe_doc;
    s->iframe_doc_set = js->iframe_doc_set;
    if (JS_IsObject(realm_doc)) {
        js->iframe_doc = realm_doc;
        js->iframe_doc_set = 1;
    }
    s->frame = js->raf_frame_ctx;
    js->raf_frame_ctx = frame;
    js->iframe_load_depth++;
    return s;
}

void
ns_js_frame_load_leave(ns_js *js, void *token)
{
    ns_frame_load_scope *s = token;
    js->iframe_load_depth--;
    js->raf_frame_ctx = s->frame;
    js->iframe_doc = s->iframe_doc;
    js->iframe_doc_set = s->iframe_doc_set;
    js->current_script = s->script;
    js->current_doc = s->doc;
    ns_frame_url_leave(js, &s->url);
    g_free(s);
}

ns_node *
ns_js_frame_eval_begin(ns_js *js, ns_node *script, gint64 deadline_us)
{
    ns_node *previous = js->current_script;
    js->current_script = script;
    js->eval_deadline_us = deadline_us;
    js->eval_depth++;
    return previous;
}

void
ns_js_frame_eval_end(ns_js *js, ns_node *previous_script)
{
    js->eval_depth--;
    js->eval_deadline_us = 0;
    js->current_script = previous_script;
    if (js->eval_depth == 0) {
        ns_js_flush_document_write(js);
        ns_js_schedule_pending_script_drain(js);
    }
}

gboolean
ns_js_has_log(const ns_js *js)
{
    return js->log_cb != NULL;
}

int
ns_js_eval_depth_add(ns_js *js, int delta)
{
    js->eval_depth += delta;
    return js->eval_depth;
}

void
ns_js_ignore_destructive_writes_add(ns_js *js, int delta)
{
    js->ignore_destructive_writes += delta;
}

ns_node *
ns_js_swap_current_script(ns_js *js, ns_node *script)
{
    ns_node *previous = js->current_script;
    js->current_script = script;
    return previous;
}

ns_node *
ns_js_swap_current_document(ns_js *js, ns_node *doc)
{
    ns_node *previous = js->current_doc;
    js->current_doc = doc;
    return previous;
}

JSContext *
ns_js_module_context(const ns_js *js)
{
    return js->module_ctx ? js->module_ctx : js->ctx;
}

JSContext *
ns_js_swap_module_context(ns_js *js, JSContext *ctx)
{
    JSContext *previous = js->module_ctx;
    js->module_ctx = ctx;
    return previous;
}

gboolean
ns_js_iframe_document(const ns_js *js, JSValue *out)
{
    if (!js->iframe_doc_set) return FALSE;
    *out = js->iframe_doc;
    return TRUE;
}

void *
ns_js_frame_url_push(ns_js *js, const char *url)
{
    ns_frame_url *fu = g_new0(ns_frame_url, 1);
    ns_frame_url_enter(js, fu, url);
    return fu;
}

void
ns_js_frame_url_pop(ns_js *js, void *fu)
{
    ns_frame_url_leave(js, fu);
    g_free(fu);
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

gboolean
ns_js_consume_mutated(ns_js *js)
{
    if (!js) return FALSE;
    gboolean m = js->mutated;
    js->mutated = FALSE;
    return m;
}
