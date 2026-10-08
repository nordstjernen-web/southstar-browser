/* Southstar — libcurl-backed async fetcher.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "net.h"
#include "net_backend.h"
#include "debuglog.h"

#include <curl/curl.h>

gboolean ns_net_log_fetches_enabled(void);
ns_response *ns_response_copy(const ns_response *src);
void     ns_net_multi_shutdown(void);
int      ns_xferinfo_cb(void *clientp, curl_off_t dltotal, curl_off_t dlnow,
                        curl_off_t ultotal, curl_off_t ulnow);
void     ns_net_begin_abort(void);
void     ns_net_join_rng(void);
void     ns_net_transport_shutdown(void);
CURLSH  *ns_net_share(void);
ns_response *ns_fetch_sync(const char *url, const char *top_url,
                           const char *method, const void *body,
                           gsize body_len, const char *content_type,
                           GPtrArray *extra_headers,
                           GCancellable *cancellable, GError **error);

static GMutex g_fetch_throttle_mutex;
static GCond  g_fetch_idle_cond;
static int    g_fetch_active;
static int    g_preconnect_active;
static GQueue g_fetch_queue = G_QUEUE_INIT;

#define NS_NET_DOMAIN ns_net_error_quark()

static GQuark
ns_net_error_quark(void)
{
    return g_quark_from_static_string("nd-net-error");
}

gboolean
ns_net_idle(void)
{
    g_mutex_lock(&g_fetch_throttle_mutex);
    gboolean idle = g_fetch_active == 0 && g_preconnect_active == 0;
    g_mutex_unlock(&g_fetch_throttle_mutex);
    return idle;
}

static void ns_fetch_task_abandon(GTask *task, const GError *err);

static gboolean
ns_net_drain(int timeout_ms)
{
    ns_net_begin_abort();
    GQueue dropped = G_QUEUE_INIT;
    g_mutex_lock(&g_fetch_throttle_mutex);
    for (GTask *t; (t = g_queue_pop_head(&g_fetch_queue)); )
        g_queue_push_tail(&dropped, t);
    g_mutex_unlock(&g_fetch_throttle_mutex);
    for (GTask *t; (t = g_queue_pop_head(&dropped)); ) {
        GError *err = g_error_new_literal(NS_NET_DOMAIN, 1, "shutting down");
        ns_fetch_task_abandon(t, err);
        g_task_return_error(t, err);
        g_object_unref(t);
    }
    g_mutex_lock(&g_fetch_throttle_mutex);
    gint64 deadline = g_get_monotonic_time() + (gint64)timeout_ms * 1000;
    gboolean drained = TRUE;
    while (g_fetch_active > 0 || g_preconnect_active > 0) {
        if (!g_cond_wait_until(&g_fetch_idle_cond, &g_fetch_throttle_mutex,
                               deadline)) {
            drained = g_fetch_active == 0 && g_preconnect_active == 0;
            break;
        }
    }
    g_mutex_unlock(&g_fetch_throttle_mutex);
    return drained;
}

void
ns_net_shutdown(void)
{
    ns_net_join_rng();
    ns_net_multi_shutdown();
    if (!ns_net_drain(3000))
        return;
    ns_net_backend_shutdown();
    ns_net_transport_shutdown();
}

typedef struct ns_fetch_ctx {
    char *url;
    char *top_url;
    char *method;
    char *content_type;
    guint8 *body;
    gsize body_len;
    GPtrArray *extra_headers;
    char *coalesce_key;
} ns_fetch_ctx;

static void
ns_fetch_ctx_free(gpointer data)
{
    ns_fetch_ctx *ctx = data;
    g_free(ctx->url);
    g_free(ctx->top_url);
    g_free(ctx->method);
    g_free(ctx->content_type);
    g_free(ctx->body);
    if (ctx->extra_headers) g_ptr_array_free(ctx->extra_headers, TRUE);
    g_free(ctx->coalesce_key);
    g_free(ctx);
}

#define NS_PRELOAD_MAX_ENTRIES 64
#define NS_PRELOAD_MAX_BYTES   (16u * 1024u * 1024u)
#define NS_FETCH_JOIN_MAX_WAIT_S (NS_MAX_TIMEOUT_S + 5)

typedef struct {
    gboolean     done;
    ns_response *resp;
    GError      *err;
} ns_sync_waiter;

typedef struct {
    GPtrArray *tasks;
    GPtrArray *syncs;
} ns_coalesce_group;

static GMutex      g_fetch_mutex;
static GCond       g_fetch_cond;
static GHashTable *g_fetch_coalesce;
static GHashTable *g_preload_expected;
static GHashTable *g_preload_store;
static guint       g_preload_bytes;

static void
ns_preload_entry_free(gpointer p)
{
    ns_response *resp = p;
    if (!resp) return;
    guint len = resp->body ? resp->body->len : 0u;
    g_preload_bytes = (g_preload_bytes > len) ? g_preload_bytes - len : 0u;
    ns_response_free(resp);
}

static gboolean
ns_response_forbids_reuse(const ns_response *resp)
{
    if (!resp || !resp->raw_headers) return FALSE;
    g_autofree char *lowered = g_ascii_strdown(resp->raw_headers, -1);
    return strstr(lowered, "no-store") != NULL;
}

void
ns_net_preload_expect(const char *key)
{
    if (!key) return;
    g_mutex_lock(&g_fetch_mutex);
    if (!g_preload_expected)
        g_preload_expected = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                   g_free, NULL);
    if (g_hash_table_size(g_preload_expected) < NS_PRELOAD_MAX_ENTRIES)
        g_hash_table_add(g_preload_expected, g_strdup(key));
    g_mutex_unlock(&g_fetch_mutex);
}

void
ns_net_preload_clear(void)
{
    g_mutex_lock(&g_fetch_mutex);
    if (g_preload_expected) g_hash_table_remove_all(g_preload_expected);
    if (g_preload_store) g_hash_table_remove_all(g_preload_store);
    g_preload_bytes = 0;
    g_mutex_unlock(&g_fetch_mutex);
}

static ns_response *
ns_preload_take_locked(const char *key)
{
    if (!g_preload_store) return NULL;
    char *stored_key = NULL;
    ns_response *resp = NULL;
    if (!g_hash_table_steal_extended(g_preload_store, key,
                                     (gpointer *)&stored_key,
                                     (gpointer *)&resp))
        return NULL;
    g_free(stored_key);
    guint len = (resp && resp->body) ? resp->body->len : 0u;
    g_preload_bytes = (g_preload_bytes > len) ? g_preload_bytes - len : 0u;
    return resp;
}

static void
ns_preload_store_locked(const char *key, const ns_response *resp)
{
    if (!g_preload_expected ||
        !g_hash_table_remove(g_preload_expected, key))
        return;
    if (!resp || resp->status != 200 || resp->error || !resp->body) return;
    if (resp->body->len > NS_PRELOAD_MAX_BYTES) return;
    if (g_preload_bytes + resp->body->len > NS_PRELOAD_MAX_BYTES) return;
    if (ns_response_forbids_reuse(resp)) return;
    if (!g_preload_store)
        g_preload_store = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                g_free, ns_preload_entry_free);
    if (g_hash_table_contains(g_preload_store, key)) return;
    g_preload_bytes += resp->body->len;
    g_hash_table_insert(g_preload_store, g_strdup(key), ns_response_copy(resp));
}

typedef enum {
    NS_FETCH_LEAD,
    NS_FETCH_JOINED,
    NS_FETCH_PRELOADED,
} ns_fetch_claim;

static ns_fetch_claim
ns_fetch_claim_locked(const char *key, ns_response **preloaded,
                      ns_coalesce_group **group)
{
    ns_response *hit = ns_preload_take_locked(key);
    if (hit) {
        *preloaded = hit;
        return NS_FETCH_PRELOADED;
    }
    if (!g_fetch_coalesce)
        g_fetch_coalesce = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                 g_free, NULL);
    ns_coalesce_group *grp = g_hash_table_lookup(g_fetch_coalesce, key);
    if (grp) {
        *group = grp;
        return NS_FETCH_JOINED;
    }
    g_hash_table_insert(g_fetch_coalesce, g_strdup(key),
                        g_new0(ns_coalesce_group, 1));
    return NS_FETCH_LEAD;
}

static ns_response *
ns_fetch_join_async(const char *key, GAsyncReadyCallback callback,
                    gpointer user_data, gboolean *joined)
{
    ns_response *preloaded = NULL;
    ns_coalesce_group *grp = NULL;
    *joined = FALSE;
    g_mutex_lock(&g_fetch_mutex);
    ns_fetch_claim claim = ns_fetch_claim_locked(key, &preloaded, &grp);
    if (claim == NS_FETCH_JOINED) {
        GTask *joiner = g_task_new(NULL, NULL, callback, user_data);
        g_task_set_source_tag(joiner, ns_net_fetch_async);
        if (!grp->tasks) grp->tasks = g_ptr_array_new();
        g_ptr_array_add(grp->tasks, joiner);
        *joined = TRUE;
    }
    g_mutex_unlock(&g_fetch_mutex);
    if (claim == NS_FETCH_PRELOADED) *joined = TRUE;
    return preloaded;
}

static ns_response *
ns_fetch_join_sync(const char *key, gboolean *joined, GError **error)
{
    ns_response *preloaded = NULL;
    ns_coalesce_group *grp = NULL;
    *joined = FALSE;
    g_mutex_lock(&g_fetch_mutex);
    ns_fetch_claim claim = ns_fetch_claim_locked(key, &preloaded, &grp);
    if (claim == NS_FETCH_JOINED) {
        ns_sync_waiter waiter = {0};
        if (!grp->syncs) grp->syncs = g_ptr_array_new();
        g_ptr_array_add(grp->syncs, &waiter);
        gint64 deadline = g_get_monotonic_time() +
            (gint64)NS_FETCH_JOIN_MAX_WAIT_S * G_TIME_SPAN_SECOND;
        while (!waiter.done) {
            if (ns_net_aborting() ||
                !g_cond_wait_until(&g_fetch_cond, &g_fetch_mutex, deadline))
                break;
        }
        if (!waiter.done) {
            g_ptr_array_remove_fast(grp->syncs, &waiter);
            g_mutex_unlock(&g_fetch_mutex);
            g_set_error_literal(error, NS_NET_DOMAIN, 0,
                                "shared fetch abandoned");
            *joined = TRUE;
            return NULL;
        }
        g_mutex_unlock(&g_fetch_mutex);
        if (error) *error = waiter.err;
        else g_clear_error(&waiter.err);
        *joined = TRUE;
        return waiter.resp;
    }
    g_mutex_unlock(&g_fetch_mutex);
    if (claim == NS_FETCH_PRELOADED) *joined = TRUE;
    return preloaded;
}

static void
ns_fetch_coalesce_deliver(const char *key, ns_response *resp, const GError *err)
{
    if (!key) return;
    g_mutex_lock(&g_fetch_mutex);
    if (resp) ns_preload_store_locked(key, resp);
    else if (g_preload_expected) g_hash_table_remove(g_preload_expected, key);
    ns_coalesce_group *grp = g_fetch_coalesce
        ? g_hash_table_lookup(g_fetch_coalesce, key) : NULL;
    if (g_fetch_coalesce) g_hash_table_remove(g_fetch_coalesce, key);
    if (grp && grp->syncs) {
        for (guint i = 0; i < grp->syncs->len; i++) {
            ns_sync_waiter *waiter = g_ptr_array_index(grp->syncs, i);
            waiter->resp = resp ? ns_response_copy(resp) : NULL;
            if (!resp)
                waiter->err = err
                    ? g_error_copy(err)
                    : g_error_new_literal(NS_NET_DOMAIN, 0, "fetch failed");
            waiter->done = TRUE;
        }
        g_cond_broadcast(&g_fetch_cond);
    }
    g_mutex_unlock(&g_fetch_mutex);
    if (!grp) return;
    if (grp->tasks) {
        for (guint i = 0; i < grp->tasks->len; i++) {
            GTask *waiter = g_ptr_array_index(grp->tasks, i);
            if (resp)
                g_task_return_pointer(waiter, ns_response_copy(resp),
                                      (GDestroyNotify)ns_response_free);
            else if (err)
                g_task_return_error(waiter, g_error_copy(err));
            else
                g_task_return_new_error(waiter, NS_NET_DOMAIN, 0, "fetch failed");
            g_object_unref(waiter);
        }
        g_ptr_array_free(grp->tasks, TRUE);
    }
    if (grp->syncs) g_ptr_array_free(grp->syncs, TRUE);
    g_free(grp);
}

static void
ns_fetch_task_abandon(GTask *task, const GError *err)
{
    ns_fetch_ctx *ctx = task ? g_task_get_task_data(task) : NULL;
    if (ctx && ctx->coalesce_key)
        ns_fetch_coalesce_deliver(ctx->coalesce_key, NULL, err);
}

#define NS_MAX_CONCURRENT_FETCHES 32
#define NS_MAX_FETCHES_PER_HOST   6

static GHashTable *g_fetch_host_active;

static void ns_fetch_thread(GTask *task, gpointer source_object,
                            gpointer task_data, GCancellable *cancellable);

static char *
ns_fetch_task_host(GTask *task)
{
    ns_fetch_ctx *ctx = task ? g_task_get_task_data(task) : NULL;
    return (ctx && ctx->url) ? ns_url_host_from(ctx->url) : NULL;
}

static int
ns_fetch_host_count_locked(const char *host)
{
    if (!host || !g_fetch_host_active) return 0;
    return GPOINTER_TO_INT(g_hash_table_lookup(g_fetch_host_active, host));
}

static void
ns_fetch_host_adjust_locked(const char *host, int delta)
{
    if (!host) return;
    if (!g_fetch_host_active)
        g_fetch_host_active = g_hash_table_new_full(g_str_hash, g_str_equal,
                                                    g_free, NULL);
    int n = ns_fetch_host_count_locked(host) + delta;
    if (n <= 0)
        g_hash_table_remove(g_fetch_host_active, host);
    else
        g_hash_table_replace(g_fetch_host_active, g_strdup(host),
                             GINT_TO_POINTER(n));
}

static void
ns_fetch_throttle_dispatch(void)
{
    for (;;) {
        g_mutex_lock(&g_fetch_throttle_mutex);
        if (g_fetch_active >= NS_MAX_CONCURRENT_FETCHES ||
            g_queue_is_empty(&g_fetch_queue)) {
            g_mutex_unlock(&g_fetch_throttle_mutex);
            return;
        }
        GTask *chosen = NULL;
        char *chosen_host = NULL;
        for (GList *l = g_fetch_queue.head; l; l = l->next) {
            GTask *t = l->data;
            char *host = ns_fetch_task_host(t);
            if (!host ||
                ns_fetch_host_count_locked(host) < NS_MAX_FETCHES_PER_HOST) {
                chosen = t;
                chosen_host = host;
                g_queue_delete_link(&g_fetch_queue, l);
                break;
            }
            g_free(host);
        }
        if (!chosen) {
            g_mutex_unlock(&g_fetch_throttle_mutex);
            return;
        }
        g_fetch_active++;
        ns_fetch_host_adjust_locked(chosen_host, 1);
        g_free(chosen_host);
        g_mutex_unlock(&g_fetch_throttle_mutex);
        g_task_run_in_thread(chosen, ns_fetch_thread);
        g_object_unref(chosen);
    }
}

static void
ns_fetch_throttle_submit(GTask *task)
{
    g_mutex_lock(&g_fetch_throttle_mutex);
    g_queue_push_tail(&g_fetch_queue, task);
    g_mutex_unlock(&g_fetch_throttle_mutex);
    ns_fetch_throttle_dispatch();
}

static void
ns_fetch_thread(GTask        *task,
                gpointer      source_object,
                gpointer      task_data,
                GCancellable *cancellable)
{
    (void)source_object;
    ns_fetch_ctx *ctx = task_data;
    GError *err = NULL;
    ns_response *resp = ns_fetch_sync(ctx->url, ctx->top_url, ctx->method,
                                      ctx->body, ctx->body_len, ctx->content_type,
                                      ctx->extra_headers,
                                      cancellable, &err);
    if (ctx->coalesce_key)
        ns_fetch_coalesce_deliver(ctx->coalesce_key, resp, err);
    if (!resp) {
        if (ns_net_log_fetches_enabled())
            ns_debug_log_emit(NS_DLOG_NET, "fetch", "failed %s: %s",
                              ctx->url, err ? err->message : "unknown error");
        g_task_return_error(task, err);
    } else if (ns_net_log_fetches_enabled()) {
        if (resp->error)
            ns_debug_log_emit(NS_DLOG_NET, "fetch", "error %s: %s",
                              ctx->url, resp->error);
        else
            ns_debug_log_emit(NS_DLOG_NET, "fetch", "%ld %s (%u bytes)",
                              resp->status,
                              resp->final_url ? resp->final_url : ctx->url,
                              resp->body ? resp->body->len : 0u);
    }
    if (resp)
        g_task_return_pointer(task, resp, (GDestroyNotify)ns_response_free);
    {
        char *host = (ctx && ctx->url) ? ns_url_host_from(ctx->url) : NULL;
        g_mutex_lock(&g_fetch_throttle_mutex);
        if (g_fetch_active > 0) g_fetch_active--;
        ns_fetch_host_adjust_locked(host, -1);
        g_cond_broadcast(&g_fetch_idle_cond);
        g_mutex_unlock(&g_fetch_throttle_mutex);
        g_free(host);
    }
    ns_fetch_throttle_dispatch();
}

static void
ns_preconnect_thread(GTask *task, gpointer source_object, gpointer task_data,
                     GCancellable *cancellable)
{
    (void)source_object;
    (void)cancellable;
    const char *url = task_data;
    g_mutex_lock(&g_fetch_throttle_mutex);
    g_preconnect_active++;
    g_mutex_unlock(&g_fetch_throttle_mutex);
    CURL *curl = ns_net_aborting() ? NULL : curl_easy_init();
    if (curl) {
        curl_easy_setopt(curl, CURLOPT_URL, url);
        curl_easy_setopt(curl, CURLOPT_CONNECT_ONLY, 1L);
        if (ns_net_share()) curl_easy_setopt(curl, CURLOPT_SHARE, ns_net_share());
        curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, 6L);
        curl_easy_setopt(curl, CURLOPT_TIMEOUT, 10L);
        curl_easy_setopt(curl, CURLOPT_NOSIGNAL, 1L);
        curl_easy_setopt(curl, CURLOPT_NOPROGRESS, 0L);
        curl_easy_setopt(curl, CURLOPT_XFERINFOFUNCTION, ns_xferinfo_cb);
        ns_net_apply_curl_proxy(curl, url);
        ns_net_apply_curl_tls(curl);
        curl_easy_perform(curl);
        curl_easy_cleanup(curl);
    }
    g_mutex_lock(&g_fetch_throttle_mutex);
    if (g_preconnect_active > 0) g_preconnect_active--;
    g_cond_broadcast(&g_fetch_idle_cond);
    g_mutex_unlock(&g_fetch_throttle_mutex);
    g_task_return_boolean(task, TRUE);
}

void
ns_net_preconnect_async(const char *url)
{
    if (!url || !ns_url_is_http_or_https(url)) return;
    GTask *task = g_task_new(NULL, NULL, NULL, NULL);
    g_task_set_task_data(task, g_strdup(url), g_free);
    g_task_run_in_thread(task, ns_preconnect_thread);
    g_object_unref(task);
}

static ns_net_blob_resolver g_blob_resolver = NULL;
static gpointer g_blob_resolver_ud = NULL;

void
ns_net_set_blob_resolver(ns_net_blob_resolver resolver, gpointer user_data)
{
    g_blob_resolver = resolver;
    g_blob_resolver_ud = user_data;
}

static gboolean
ns_net_complete_blob(const char *url, GCancellable *cancellable,
                     GAsyncReadyCallback callback, gpointer user_data)
{
    if (!g_blob_resolver || !g_str_has_prefix(url, "blob:")) return FALSE;
    char *type = NULL;
    GBytes *bytes = g_blob_resolver(url, &type, g_blob_resolver_ud);
    GTask *task = g_task_new(NULL, cancellable, callback, user_data);
    g_task_set_source_tag(task, ns_net_fetch_async);
    ns_response *resp = g_new0(ns_response, 1);
    resp->body = g_byte_array_new();
    resp->final_url = g_strdup(url);
    if (bytes) {
        gsize len = 0;
        const guint8 *data = g_bytes_get_data(bytes, &len);
        if (data && len) g_byte_array_append(resp->body, data, len);
        resp->status = 200;
        resp->content_type = type;
        g_bytes_unref(bytes);
    } else {
        resp->status = 404;
        resp->error = g_strdup("blob URL not found");
        g_free(type);
    }
    g_task_return_pointer(task, resp, (GDestroyNotify)ns_response_free);
    g_object_unref(task);
    return TRUE;
}

void
ns_net_fetch_async(const char        *url,
                   const char        *top_url,
                   GCancellable      *cancellable,
                   GAsyncReadyCallback callback,
                   gpointer            user_data)
{
    ns_net_request_async(url, top_url, "GET", NULL, 0, NULL, NULL,
                         cancellable, callback, user_data);
}

void
ns_net_request_async(const char         *url,
                     const char         *top_url,
                     const char         *method,
                     const void         *body,
                     gsize               body_len,
                     const char         *content_type,
                     const char *const  *extra_headers,
                     GCancellable       *cancellable,
                     GAsyncReadyCallback callback,
                     gpointer            user_data)
{
    g_return_if_fail(url != NULL);

    if (ns_net_complete_blob(url, cancellable, callback, user_data)) return;

    char *key = (!cancellable && !body)
        ? ns_net_request_key(url, top_url, method, extra_headers) : NULL;
    if (key) {
        gboolean joined = FALSE;
        ns_response *preloaded = ns_fetch_join_async(key, callback, user_data,
                                                     &joined);
        if (preloaded) {
            GTask *task = g_task_new(NULL, cancellable, callback, user_data);
            g_task_set_source_tag(task, ns_net_fetch_async);
            g_task_return_pointer(task, preloaded,
                                  (GDestroyNotify)ns_response_free);
            g_object_unref(task);
            g_free(key);
            return;
        }
        if (joined) {
            g_free(key);
            return;
        }
    }

    ns_fetch_ctx *ctx = g_new0(ns_fetch_ctx, 1);
    ctx->url = g_strdup(url);
    ctx->top_url = top_url ? g_strdup(top_url) : NULL;
    ctx->coalesce_key = key;
    if (method && *method) ctx->method = g_strdup(method);
    if (content_type && *content_type) ctx->content_type = g_strdup(content_type);
    if (body && body_len > 0) {
        ctx->body = g_memdup2(body, body_len);
        ctx->body_len = body_len;
    }
    if (extra_headers && extra_headers[0]) {
        ctx->extra_headers = g_ptr_array_new_with_free_func(g_free);
        for (int i = 0; extra_headers[i]; i++)
            g_ptr_array_add(ctx->extra_headers, g_strdup(extra_headers[i]));
    }

    GTask *task = g_task_new(NULL, cancellable, callback, user_data);
    g_task_set_source_tag(task, ns_net_request_async);
    g_task_set_task_data(task, ctx, ns_fetch_ctx_free);
    ns_fetch_throttle_submit(task);
}

ns_response *
ns_net_request_blocking(const char        *url,
                        const char        *top_url,
                        const char        *method,
                        const void        *body,
                        gsize              body_len,
                        const char        *content_type,
                        const char *const *extra_headers,
                        GCancellable      *cancellable,
                        GError           **error)
{
    char *key = (!cancellable && !body)
        ? ns_net_request_key(url, top_url, method, extra_headers) : NULL;
    if (key) {
        gboolean joined = FALSE;
        ns_response *shared = ns_fetch_join_sync(key, &joined, error);
        if (joined) {
            g_free(key);
            return shared;
        }
    }
    GPtrArray *hdrs = NULL;
    if (extra_headers) {
        hdrs = g_ptr_array_new_with_free_func(g_free);
        for (int i = 0; extra_headers[i]; i++)
            g_ptr_array_add(hdrs, g_strdup(extra_headers[i]));
    }
    GError *failure = NULL;
    ns_response *resp = ns_fetch_sync(url, top_url, method,
                                      body, body_len, content_type,
                                      hdrs, cancellable, &failure);
    if (key) {
        ns_fetch_coalesce_deliver(key, resp, failure);
        g_free(key);
    }
    if (hdrs) g_ptr_array_free(hdrs, TRUE);
    if (failure) g_propagate_error(error, failure);
    return resp;
}

ns_response *
ns_net_fetch_blocking(const char *url, GCancellable *cancellable, GError **error)
{
    return ns_net_request_blocking(url, NULL, "GET", NULL, 0, NULL, NULL,
                                   cancellable, error);
}

ns_response *
ns_net_fetch_finish(GAsyncResult *result, GError **error)
{
    g_return_val_if_fail(g_task_is_valid(result, NULL), NULL);
    return g_task_propagate_pointer(G_TASK(result), error);
}
