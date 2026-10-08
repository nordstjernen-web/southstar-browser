/* Southstar — libcurl-backed async fetcher.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "net.h"
#include "net_backend.h"
#include "cache.h"
#include "config.h"
#include "csp.h"
#include "debuglog.h"
#include "ext.h"
#include "html.h"
#include "image.h"
#include "security.h"

#include <curl/curl.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

char    *ns_url_to_ascii(const char *url);
char    *ns_url_origin_from_any(const char *url);
char    *ns_url_site_from(const char *url);
gboolean ns_url_is_same_site(const char *a, const char *b);
const char *ns_net_cookie_dir(void);
const char *ns_net_hsts_curl_path(void);
const char *ns_net_altsvc_path(void);
char    *ns_net_cookie_path_for_partition(const char *top_origin);
char    *ns_net_cookie_js_path_for_partition(const char *top_origin);
void     ns_net_state_shutdown(void);
gboolean ns_net_log_fetches_enabled(void);
const char *ns_net_pick_configured_proxy(const char *url);
const char *ns_net_configured_no_proxy(void);
void     ns_net_perf_record(gint64 start_us, gint64 end_us, guint64 bytes);
void     ns_net_conn_stat_record(const char *url, long http_version,
                                 long new_connections);
void     ns_net_log_record(const char *method, const char *url, long status,
                           const char *content_type, guint64 body_len,
                           double duration_ms, const char *req_headers,
                           const char *resp_headers, const char *error);
size_t   ns_write_cb(char *data, size_t size, size_t nmemb, void *userdata);
size_t   ns_header_cb(char *buffer, size_t size, size_t nitems,
                      void *userdata);
ns_response *ns_response_copy(const ns_response *src);
const char *ns_net_http_version_name(long version);
gboolean ns_net_host_recently_dead(const char *origin);
void     ns_net_host_mark_dead(const char *origin);
void     ns_net_host_mark_alive(const char *origin);
gboolean ns_net_acquire_origin_slot(const char *origin,
                                    GCancellable *cancellable);
void     ns_net_release_origin_slot(const char *origin);
CURLcode ns_net_multi_perform(CURL *easy, GCancellable *cancellable);
void     ns_net_multi_shutdown(void);
long     ns_net_http_version(void);
int      ns_xferinfo_cb(void *clientp, curl_off_t dltotal, curl_off_t dlnow,
                        curl_off_t ultotal, curl_off_t ulnow);
void     ns_net_begin_abort(void);
void     ns_net_join_rng(void);
void     ns_net_transport_shutdown(void);
CURLSH  *ns_net_share(void);
const char *ns_net_accept_encoding(void);
char    *ns_net_slist_serialize(struct curl_slist *list);

static GMutex g_fetch_throttle_mutex;
static GCond  g_fetch_idle_cond;
static int    g_fetch_active;
static int    g_preconnect_active;
static GQueue g_fetch_queue = G_QUEUE_INIT;
static gboolean
ns_url_is_ftp(const char *url)
{
    return url && g_str_has_prefix(url, "ftp://");
}

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

#define NS_NET_RESPONSE_RECHECK_BYTES (16ULL * 1024ULL * 1024ULL)
guint64 ns_net_response_budget(void);
gboolean ns_net_synthesize_data_response(const char *url, ns_response *resp);
gboolean ns_net_synthesize_file_response(const char *url, const char *top_url,
                                         ns_response *resp);
gboolean ns_net_synthesize_view_source_response(const char *url,
                                                const char *top_url,
                                                GCancellable *cancellable,
                                                ns_response *resp);
void     ns_net_finish_ftp_response(ns_response *resp);
gboolean ns_net_synthesize_about_response(const char *url, const char *top_url,
                                          const char *method,
                                          const void *req_body,
                                          gsize req_body_len,
                                          ns_response *resp);

static gboolean
is_simple_get(const char *method)
{
    return !method || !*method || g_ascii_strcasecmp(method, "GET") == 0;
}

static ns_response *
response_from_cache_entry(ns_cache_entry *e)
{
    ns_response *resp = g_new0(ns_response, 1);
    resp->status       = e->status;
    resp->final_url    = g_strdup(e->final_url);
    resp->content_type = g_strdup(e->content_type);
    resp->cors_allow_origin = g_strdup(e->cors_allow_origin);
    resp->body         = e->body;
    e->body = NULL;
    return resp;
}

static gboolean ns_fetch_is_navigation(const char *top_url,
                                        GPtrArray *extra_headers);

static gboolean
ns_fetch_has_user_activation(GPtrArray *extra_headers)
{
    if (!extra_headers) return FALSE;
    for (guint i = 0; i < extra_headers->len; i++) {
        const char *header = g_ptr_array_index(extra_headers, i);
        if (header && g_ascii_strncasecmp(
                header, "X-ND-User-Activated:", 20) == 0)
            return TRUE;
    }
    return FALSE;
}

void
ns_hop_out_clear(ns_hop_out *out)
{
    if (!out) return;
    g_free(out->effective_url);
    g_free(out->remote_ip);
    g_free(out->tls_warning);
    g_free(out->error_message);
    out->effective_url = NULL;
    out->remote_ip = NULL;
    out->tls_warning = NULL;
    out->error_message = NULL;
}

gboolean
ns_hop_transport_curl(const ns_hop_req *req, ns_write_ctx *wctx,
                      ns_header_ctx *hctx, ns_hop_out *out,
                      GCancellable *cancellable)
{
    CURL *curl = curl_easy_init();
    if (!curl) {
        out->error_message = g_strdup("curl_easy_init failed");
        return FALSE;
    }
    if (getenv("NS_NET_TRACE"))
        curl_easy_setopt(curl, CURLOPT_VERBOSE, 1L);
    if (ns_net_share()) curl_easy_setopt(curl, CURLOPT_SHARE, ns_net_share());

    char errbuf[CURL_ERROR_SIZE];
    errbuf[0] = '\0';

    curl_easy_setopt(curl, CURLOPT_URL, req->url);
    if (req->proxy && *req->proxy)
        curl_easy_setopt(curl, CURLOPT_PROXY, req->proxy);
    if (req->no_proxy && *req->no_proxy)
        curl_easy_setopt(curl, CURLOPT_NOPROXY, req->no_proxy);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION,
                     req->follow_redirects ? 1L : 0L);
    curl_easy_setopt(curl, CURLOPT_UNRESTRICTED_AUTH, 0L);
    curl_easy_setopt(curl, CURLOPT_MAXREDIRS, req->max_redirs);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, req->timeout_s);
    curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, req->connect_timeout_s);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, req->user_agent);
    curl_easy_setopt(curl, CURLOPT_ACCEPT_ENCODING,
                     req->accept_encoding ? req->accept_encoding : "");
    switch (req->referer_policy) {
    case NS_REFERER_NO_REFERRER:
        curl_easy_setopt(curl, CURLOPT_AUTOREFERER, 0L);
        curl_easy_setopt(curl, CURLOPT_REFERER, "");
        break;
    case NS_REFERER_UNSAFE_URL:
        curl_easy_setopt(curl, CURLOPT_AUTOREFERER, 1L);
        break;
    default:
        curl_easy_setopt(curl, CURLOPT_AUTOREFERER, 0L);
        break;
    }
    if (req->referer && *req->referer)
        curl_easy_setopt(curl, CURLOPT_REFERER, req->referer);

    gboolean method_is_post = req->method &&
                              g_ascii_strcasecmp(req->method, "POST") == 0;
    gboolean method_is_get  = !req->method || !*req->method ||
                              g_ascii_strcasecmp(req->method, "GET") == 0;
    gboolean has_body = req->body && req->body_len > 0;
    if (method_is_post)
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
    if (has_body) {
        curl_easy_setopt(curl, CURLOPT_POSTFIELDS, req->body);
        curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, (long)req->body_len);
    } else if (method_is_post) {
        /* A POST without data: libcurl would otherwise read the body from
         * stdin and send it chunked, which a server that does not take
         * chunked requests reads as the start of the next request on the
         * connection. */
        curl_easy_setopt(curl, CURLOPT_POSTFIELDS, "");
        curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, 0L);
    }
    if (!method_is_post && !method_is_get && req->method &&
        !strpbrk(req->method, "\r\n"))
        curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, req->method);

    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, req->headers);
    curl_easy_setopt(curl, CURLOPT_NOSIGNAL, 1L);
    curl_easy_setopt(curl, CURLOPT_ERRORBUFFER, errbuf);
    ns_net_apply_curl_tls(curl);

    if (req->cookie_jar_path) {
        curl_easy_setopt(curl, CURLOPT_COOKIEFILE, req->cookie_jar_path);
        curl_easy_setopt(curl, CURLOPT_COOKIEJAR,  req->cookie_jar_path);
        if (req->cookie_js_path)
            curl_easy_setopt(curl, CURLOPT_COOKIEFILE, req->cookie_js_path);
    }

    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, ns_write_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, wctx);
    curl_easy_setopt(curl, CURLOPT_MAXFILESIZE_LARGE, (curl_off_t)wctx->budget);
    curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, ns_header_cb);
    curl_easy_setopt(curl, CURLOPT_HEADERDATA, hctx);

    curl_easy_setopt(curl, CURLOPT_PROTOCOLS_STR, "http,https,ftp");
    curl_easy_setopt(curl, CURLOPT_REDIR_PROTOCOLS_STR,
                     req->initial_https ? "https" :
                     (req->request_ftp ? "ftp" : "http,https"));

    const char *hsts_curl = ns_net_hsts_curl_path();
    if (hsts_curl) {
        curl_easy_setopt(curl, CURLOPT_HSTS_CTRL, (long)CURLHSTS_ENABLE);
        curl_easy_setopt(curl, CURLOPT_HSTS, hsts_curl);
    }
    curl_easy_setopt(curl, CURLOPT_HTTP_VERSION, req->http_version_pref);
    const char *altsvc = ns_net_altsvc_path();
    if (altsvc)
        curl_easy_setopt(curl, CURLOPT_ALTSVC, altsvc);

    curl_easy_setopt(curl, CURLOPT_NOPROGRESS, 0L);
    curl_easy_setopt(curl, CURLOPT_XFERINFOFUNCTION, ns_xferinfo_cb);
    curl_easy_setopt(curl, CURLOPT_XFERINFODATA, cancellable);

    CURLcode rc = ns_net_multi_perform(curl, cancellable);

    if (rc == CURLE_RECV_ERROR && hctx->location && *hctx->location &&
        g_strstr_len(errbuf, -1, "unexpected eof")) {
        long redirect_status = 0;
        curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &redirect_status);
        if (redirect_status == 301 || redirect_status == 302 ||
            redirect_status == 303 || redirect_status == 307 ||
            redirect_status == 308)
            rc = CURLE_OK;
    }

    if ((rc == CURLE_PEER_FAILED_VERIFICATION ||
         rc == CURLE_SSL_CACERT_BADFILE) &&
        g_str_has_prefix(req->url, "https://")) {
        const ns_config *cfg = ns_config_get();
        gboolean opt_in = cfg && cfg->tls_allow_insecure_override;
        char *fb_host = ns_url_host_from(req->url);
        gboolean hsts_pinned = ns_net_hsts_should_upgrade(fb_host);
        if (opt_in && !hsts_pinned) {
            char *warn = g_strdup_printf(
                "Insecure: TLS certificate not trusted (%s)",
                errbuf[0] ? errbuf : curl_easy_strerror(rc));
            g_byte_array_set_size(wctx->body, 0);
            wctx->total = 0;
            wctx->next_recheck = NS_NET_RESPONSE_RECHECK_BYTES;
            wctx->exceeded = FALSE;
            errbuf[0] = '\0';
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 0L);
            curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 0L);
            curl_easy_setopt(curl, CURLOPT_COOKIEFILE, "");
            curl_easy_setopt(curl, CURLOPT_COOKIEJAR,  NULL);
            curl_easy_setopt(curl, CURLOPT_COOKIELIST, "ALL");
            rc = ns_net_multi_perform(curl, cancellable);
            if (rc == CURLE_OK)
                out->tls_warning = warn;
            else
                g_free(warn);
        }
        g_free(fb_host);
    }

    if (rc == CURLE_ABORTED_BY_CALLBACK && cancellable &&
        g_cancellable_is_cancelled(cancellable)) {
        out->cancelled = TRUE;
        curl_easy_cleanup(curl);
        return FALSE;
    }

    long status = 0;
    char *eff_url = NULL;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &status);
    curl_easy_getinfo(curl, CURLINFO_EFFECTIVE_URL, &eff_url);
    out->status = status;
    out->effective_url = g_strdup(eff_url ? eff_url : req->url);
    {
        curl_off_t lookup_us = 0, connect_us = 0, tls_us = 0;
        curl_off_t pre_us = 0, start_us = 0, total_us = 0;
        curl_easy_getinfo(curl, CURLINFO_NAMELOOKUP_TIME_T, &lookup_us);
        curl_easy_getinfo(curl, CURLINFO_CONNECT_TIME_T, &connect_us);
        curl_easy_getinfo(curl, CURLINFO_APPCONNECT_TIME_T, &tls_us);
        curl_easy_getinfo(curl, CURLINFO_PRETRANSFER_TIME_T, &pre_us);
        curl_easy_getinfo(curl, CURLINFO_STARTTRANSFER_TIME_T, &start_us);
        curl_easy_getinfo(curl, CURLINFO_TOTAL_TIME_T, &total_us);
        out->t_namelookup_ms   = (double)lookup_us / 1000.0;
        out->t_connect_ms      = (double)connect_us / 1000.0;
        out->t_appconnect_ms   = (double)tls_us / 1000.0;
        out->t_pretransfer_ms  = (double)pre_us / 1000.0;
        out->t_starttransfer_ms = (double)start_us / 1000.0;
        out->t_total_ms        = (double)total_us / 1000.0;
    }
    {
        char *ip = NULL;
        curl_easy_getinfo(curl, CURLINFO_PRIMARY_IP, &ip);
        if (ip && *ip) out->remote_ip = g_strdup(ip);
    }
    {
        long hv = 0, nc = 0;
        curl_easy_getinfo(curl, CURLINFO_HTTP_VERSION, &hv);
        curl_easy_getinfo(curl, CURLINFO_NUM_CONNECTS, &nc);
        out->http_version = hv;
        out->num_connects = nc;
    }
    out->tls_verify_failed = (rc == CURLE_PEER_FAILED_VERIFICATION ||
                              rc == CURLE_SSL_CACERT_BADFILE ||
                              rc == CURLE_SSL_ISSUER_ERROR);
    out->connect_failed = (rc == CURLE_COULDNT_CONNECT ||
                           rc == CURLE_OPERATION_TIMEDOUT ||
                           rc == CURLE_COULDNT_RESOLVE_HOST);
    if (rc == CURLE_FILESIZE_EXCEEDED)
        wctx->exceeded = TRUE;
    out->ok = (rc == CURLE_OK);
    if (rc != CURLE_OK && !wctx->exceeded)
        out->error_message =
            g_strdup(errbuf[0] ? errbuf : curl_easy_strerror(rc));

    curl_easy_cleanup(curl);
    return TRUE;
}

#ifndef NS_HTTP_BACKEND_NGHTTP2
gboolean
ns_hop_transport(const ns_hop_req *req, ns_write_ctx *wctx,
                 ns_header_ctx *hctx, ns_hop_out *out,
                 GCancellable *cancellable)
{
    return ns_hop_transport_curl(req, wctx, hctx, out, cancellable);
}

void
ns_net_backend_shutdown(void)
{
}
#endif

static const char *const ns_script_accept_headers[] = {
    "Accept: text/javascript, application/javascript, application/ecmascript, application/x-javascript, */*;q=0.8",
    "X-ND-Fetch-Dest: script",
    NULL
};

static const char *const ns_style_accept_headers[] = {
    "Accept: text/css,*/*;q=0.1",
    "X-ND-Fetch-Dest: style",
    NULL
};

static const char *const ns_image_accept_headers[] = {
    "Accept: image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
    "X-ND-Fetch-Dest: image",
    NULL
};

static const char *const ns_font_accept_headers[] = {
    "Accept: font/woff2,font/woff,application/font-woff,application/octet-stream;q=0.8,*/*;q=0.5",
    "X-ND-Fetch-Dest: font",
    NULL
};

const char *const *
ns_net_accept_headers_for(ns_fetch_destination dest)
{
    switch (dest) {
    case NS_FETCH_DEST_SCRIPT: return ns_script_accept_headers;
    case NS_FETCH_DEST_STYLE: return ns_style_accept_headers;
    case NS_FETCH_DEST_IMAGE: return ns_image_accept_headers;
    case NS_FETCH_DEST_FONT: return ns_font_accept_headers;
    default: return NULL;
    }
}

static const char *
ns_net_fetch_destination(GPtrArray *extra_headers)
{
    static const char prefix[] = "X-ND-Fetch-Dest:";
    for (guint i = 0; extra_headers && i < extra_headers->len; i++) {
        const char *h = g_ptr_array_index(extra_headers, i);
        if (!h || g_ascii_strncasecmp(h, prefix, sizeof prefix - 1) != 0)
            continue;
        h += sizeof prefix - 1;
        while (*h == ' ' || *h == '\t') h++;
        static const char *const known[] = {
            "script", "style", "image", "font", "iframe", "frame", "object",
            "embed", "worker", "sharedworker", "serviceworker",
        };
        for (gsize k = 0; k < G_N_ELEMENTS(known); k++)
            if (g_ascii_strcasecmp(h, known[k]) == 0) return known[k];
    }
    return "empty";
}

/* A request for a nested browsing context's document is a navigation too:
 * Fetch gives it mode "navigate" and the document Accept header. */
static gboolean
ns_net_dest_is_nested_navigation(const char *dest)
{
    return strcmp(dest, "iframe") == 0 || strcmp(dest, "frame") == 0 ||
           strcmp(dest, "object") == 0 || strcmp(dest, "embed") == 0;
}

/* Worker scripts are fetched with mode "same-origin". */
static gboolean
ns_net_dest_is_worker(const char *dest)
{
    return strcmp(dest, "worker") == 0 || strcmp(dest, "sharedworker") == 0 ||
           strcmp(dest, "serviceworker") == 0;
}

static char *
ns_net_partition_key(const char *url, const char *top_url)
{
    const ns_config *cfg = ns_config_get();
    const char *ua = (cfg && cfg->user_agent && *cfg->user_agent)
        ? cfg->user_agent
        : ns_user_agent_for_mode(cfg ? cfg->compat_mode : NULL);
    const char *effective_top_url = top_url ? top_url : url;
    g_autofree char *top_origin = ns_url_origin_from(effective_top_url);
    g_autofree char *top_site   = ns_url_site_from(effective_top_url);
    const char *partition = (top_site && *top_site) ? top_site
                          : (top_origin ? top_origin : "");
    return g_strdup_printf("top=%s\x1f" "ua=%s\x1f" "al=%s", partition, ua,
                           ns_net_effective_accept_language());
}

static const char *
ns_net_request_origin(const char *url, const char *top_url,
                      const char *top_origin, const char *method)
{
    if (!top_origin || !*top_origin) return "";
    if (strpbrk(top_origin, "\r\n") || strlen(top_origin) >= 4096) return "";
    if (top_url && !ns_url_same_origin(top_url, url)) return top_origin;
    if (method && *method &&
        g_ascii_strcasecmp(method, "GET") != 0 &&
        g_ascii_strcasecmp(method, "HEAD") != 0)
        return top_origin;
    return "";
}

static int
ns_header_line_cmp(gconstpointer a, gconstpointer b)
{
    return g_strcmp0(*(const char *const *)a, *(const char *const *)b);
}

char *
ns_net_request_key(const char *url, const char *top_url, const char *method,
                   const char *const *extra_headers)
{
    if (!url || !ns_url_is_http_or_https(url)) return NULL;
    if (method && *method && g_ascii_strcasecmp(method, "GET") != 0) return NULL;
    g_autofree char *partition = ns_net_partition_key(url, top_url);
    GString *key = g_string_new("GET\x1f");
    g_string_append(key, url);
    g_string_append_c(key, '\x1f');
    g_string_append(key, partition);
    GPtrArray *sorted = g_ptr_array_new();
    for (int i = 0; extra_headers && extra_headers[i]; i++)
        g_ptr_array_add(sorted, (gpointer)extra_headers[i]);
    g_ptr_array_sort(sorted, ns_header_line_cmp);
    for (guint i = 0; i < sorted->len; i++) {
        g_string_append_c(key, '\x1f');
        g_string_append(key, g_ptr_array_index(sorted, i));
    }
    g_ptr_array_free(sorted, TRUE);
    return g_string_free(key, FALSE);
}

static ns_response *
ns_fetch_sync_hop(const char *url, const char *top_url, const char *method,
                  const void *body, gsize body_len, const char *content_type,
                  GPtrArray *extra_headers,
                  GCancellable *cancellable, GError **error,
                  gboolean follow_redirects, char **location_out)
{
    if (location_out) *location_out = NULL;
    gboolean is_navigation = ns_fetch_is_navigation(top_url, extra_headers);
    gboolean user_activated = ns_fetch_has_user_activation(extra_headers);
    ns_response *resp = g_new0(ns_response, 1);
    resp->body = g_byte_array_new();

    if (!is_navigation) {
        char *dead_host = ns_url_origin_from_any(url);
        gboolean dead = dead_host && *dead_host &&
                        ns_net_host_recently_dead(dead_host);
        g_free(dead_host);
        if (dead) {
            resp->error =
                g_strdup("host unreachable (recent connection failure)");
            resp->final_url = g_strdup(url);
            return resp;
        }
    }

    if (ns_net_synthesize_about_response(url, top_url, method, body, body_len,
                                         resp))
        return resp;
    if (ns_net_synthesize_view_source_response(url, top_url, cancellable, resp))
        return resp;
    if (ns_net_synthesize_data_response(url, resp))
        return resp;
    if (ns_net_synthesize_file_response(url, top_url, resp))
        return resp;

    char *hsts_upgraded = NULL;
    char *idn_ascii = ns_url_to_ascii(url);
    if (idn_ascii && strcmp(idn_ascii, url) != 0) {
        hsts_upgraded = idn_ascii;
        url = hsts_upgraded;
    } else {
        g_free(idn_ascii);
    }

    char *https_url = ns_net_hsts_upgrade(url);
    if (https_url) {
        g_free(hsts_upgraded);
        hsts_upgraded = https_url;
        url = hsts_upgraded;
    }

    gboolean request_http = ns_url_is_http_or_https(url);
    gboolean request_ftp = ns_url_is_ftp(url);
    if (request_ftp && top_url && *top_url && !is_navigation &&
        !ns_url_is_ftp(top_url)) {
        resp->final_url = g_strdup(url);
        resp->status = 0;
        resp->error = g_strdup("FTP access is not allowed from this page");
        g_free(hsts_upgraded);
        return resp;
    }
    const ns_config *cfg = ns_config_get();
    const char *configured_ua =
        (cfg && cfg->user_agent && *cfg->user_agent) ? cfg->user_agent
            : ns_user_agent_for_mode(cfg ? cfg->compat_mode : NULL);
    const char *effective_ua = configured_ua;
    const char *accept_language = ns_net_effective_accept_language();
    const char *effective_top_url = top_url ? top_url : url;
    char *top_origin = ns_url_origin_from(effective_top_url);
    char *top_site   = ns_url_site_from(effective_top_url);
    const char *partition_key = (top_site && *top_site) ? top_site
                              : (top_origin ? top_origin : "");
    char *cache_partition = ns_net_partition_key(url, top_url);
    const char *request_accept = "*/*";
    for (guint i = 0; extra_headers && i < extra_headers->len; i++) {
        const char *h = g_ptr_array_index(extra_headers, i);
        if (g_ascii_strncasecmp(h, "Accept:", 7) != 0) continue;
        for (h += 7; *h == ' ' || *h == '\t'; h++) {}
        request_accept = h;
        break;
    }
    g_autofree char *vary_accept =
        g_strdup_printf("Accept: %s", request_accept);
    g_autofree char *vary_language =
        g_strdup_printf("Accept-Language: %s", accept_language);
    g_autofree char *vary_agent =
        g_strdup_printf("User-Agent: %s", effective_ua);
    const char *request_origin =
        ns_net_request_origin(url, top_url, top_origin, method);
    g_autofree char *vary_origin =
        g_strdup_printf("Origin: %s", request_origin);
    const char *const cache_request_headers[] = {
        vary_accept, vary_language, vary_agent, vary_origin, NULL
    };
    ns_cookie_policy cookie_policy = cfg ? cfg->cookie_policy : NS_COOKIE_FIRST_PARTY;
    gboolean cookies_allowed = request_http &&
        (cookie_policy != NS_COOKIE_NEVER);
    if (cookies_allowed && cookie_policy == NS_COOKIE_FIRST_PARTY &&
        top_url && !ns_url_is_same_site(url, effective_top_url))
        cookies_allowed = FALSE;
    if (!*partition_key)
        cookies_allowed = FALSE;
    char *cookie_partition_path = cookies_allowed
        ? ns_net_cookie_path_for_partition(partition_key) : NULL;

    ns_cache_entry *cached = NULL;
    if (request_http && is_simple_get(method)) {
        cached = ns_cache_get(url, cache_partition, cache_request_headers);
        if (cached && ns_cache_is_fresh(cached)) {
            gboolean cache_has_cors =
                cached->cors_allow_origin ||
                ns_url_same_origin(effective_top_url, cached->final_url);
            if (!cache_has_cors) {
                ns_cache_entry_free(cached);
                cached = NULL;
            } else {
                ns_response_free(resp);
                ns_response *from_cache = response_from_cache_entry(cached);
                ns_cache_entry_free(cached);
                g_free(cache_partition);
                g_free(cookie_partition_path);
                g_free(top_origin);
                g_free(top_site);
                g_free(hsts_upgraded);
                return from_cache;
            }
        }
    }

    char *referer = ns_net_referer_for(url, top_url,
        cfg ? cfg->referer_policy : NS_REFERER_STRICT_ORIGIN_WHEN_CROSS);
    char *origin_slot = ns_url_origin_from(url);
    gboolean origin_held = FALSE;
    if (origin_slot) {
        origin_held = ns_net_acquire_origin_slot(origin_slot, cancellable);
        if (!origin_held) {
            g_set_error_literal(error, G_IO_ERROR, G_IO_ERROR_CANCELLED,
                                "fetch cancelled");
            g_free(origin_slot);
            g_free(referer);
            g_free(cache_partition);
            g_free(cookie_partition_path);
            g_free(top_origin);
            g_free(top_site);
            g_free(hsts_upgraded);
            ns_cache_entry_free(cached);
            ns_response_free(resp);
            return NULL;
        }
    }

    if (getenv("NS_NET_LOG"))
        fprintf(stderr, "NS_NET %s %s\n", method ? method : "GET", url);

    long max_redirs = cfg ? (long)cfg->max_redirects : (long)NS_MAX_REDIRECTS;
    if (max_redirs < 0)                       max_redirs = 0;
    if (max_redirs > (long)NS_MAX_REDIRECTS)  max_redirs = (long)NS_MAX_REDIRECTS;

    long fetch_timeout = (long)NS_DEFAULT_TIMEOUT_S;
    if (extra_headers) {
        for (guint i = 0; i < extra_headers->len; i++) {
            const char *h = g_ptr_array_index(extra_headers, i);
            if (h && g_str_has_prefix(h, "X-ND-Timeout-Seconds:")) {
                fetch_timeout = (long)g_ascii_strtoll(
                    h + strlen("X-ND-Timeout-Seconds:"), NULL, 10);
                break;
            }
        }
    }
    if (fetch_timeout < 1) fetch_timeout = 1;
    if (fetch_timeout > (long)NS_MAX_TIMEOUT_S) fetch_timeout = (long)NS_MAX_TIMEOUT_S;

    gboolean caller_set_accept = FALSE;
    if (extra_headers) {
        for (guint i = 0; i < extra_headers->len; i++) {
            const char *h = g_ptr_array_index(extra_headers, i);
            if (h && g_ascii_strncasecmp(h, "Accept:", 7) == 0) {
                caller_set_accept = TRUE;
                break;
            }
        }
    }

    struct curl_slist *headers = NULL;
    {
        char *h = g_strdup_printf("Accept-Language: %s", accept_language);
        headers = curl_slist_append(headers, h);
        g_free(h);
    }
    const char *fetch_dest = is_navigation
        ? "document" : ns_net_fetch_destination(extra_headers);
    gboolean navigates = is_navigation ||
                         ns_net_dest_is_nested_navigation(fetch_dest);
    if (!caller_set_accept) {
        headers = curl_slist_append(headers, navigates
            ? "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
            : "Accept: */*");
    }
    if (!cfg || cfg->do_not_track)
        headers = curl_slist_append(headers, "DNT: 1");

    if (*request_origin)
        headers = curl_slist_append(headers, vary_origin);

    if (cached && cached->etag) {
        char *h = g_strdup_printf("If-None-Match: %s", cached->etag);
        headers = curl_slist_append(headers, h);
        g_free(h);
    }
    if (cached && cached->last_modified) {
        char *h = g_strdup_printf("If-Modified-Since: %s", cached->last_modified);
        headers = curl_slist_append(headers, h);
        g_free(h);
    }

    if (request_http) {
        const char *fetch_site;
        if (!top_url || !*top_url) {
            fetch_site = "none";
        } else if (ns_url_same_origin(top_url, url)) {
            fetch_site = "same-origin";
        } else if (ns_url_is_same_site(top_url, url)) {
            fetch_site = "same-site";
        } else {
            fetch_site = "cross-site";
        }
        char *site_h = g_strdup_printf("Sec-Fetch-Site: %s", fetch_site);
        headers = curl_slist_append(headers, site_h);
        g_free(site_h);

        const char *fetch_mode;
        if (navigates) {
            fetch_mode = "navigate";
        } else if (ns_net_dest_is_worker(fetch_dest)) {
            fetch_mode = "same-origin";
        } else if (method && *method &&
                   g_ascii_strcasecmp(method, "GET") != 0 &&
                   g_ascii_strcasecmp(method, "HEAD") != 0) {
            fetch_mode = "cors";
        } else {
            fetch_mode = "no-cors";
        }
        char *mode_h = g_strdup_printf("Sec-Fetch-Mode: %s", fetch_mode);
        headers = curl_slist_append(headers, mode_h);
        g_free(mode_h);

        char *dest_h = g_strdup_printf("Sec-Fetch-Dest: %s", fetch_dest);
        headers = curl_slist_append(headers, dest_h);
        g_free(dest_h);

        if (is_navigation && user_activated)
            headers = curl_slist_append(headers, "Sec-Fetch-User: ?1");
        if (navigates) {
            headers = curl_slist_append(headers,
                                        "Upgrade-Insecure-Requests: 1");
        }

        gboolean chromium_ua = ns_user_agent_has_client_hints(effective_ua);
        if (chromium_ua) {
            headers = curl_slist_append(headers,
                "Sec-CH-UA: \"Southstar\";v=\"1\", "
                "\"Not=A?Brand\";v=\"24\"");
            char *ua_mobile = g_strdup_printf("Sec-CH-UA-Mobile: ?%d",
                                              ns_net_is_mobile_mode() ? 1 : 0);
            headers = curl_slist_append(headers, ua_mobile);
            g_free(ua_mobile);
            char *ua_plat = g_strdup_printf("Sec-CH-UA-Platform: \"%s\"",
                                            ns_net_ua_hint_platform());
            headers = curl_slist_append(headers, ua_plat);
            g_free(ua_plat);
        }

        if (!cfg || cfg->global_privacy_control) {
            headers = curl_slist_append(headers, "Sec-GPC: 1");
        }
    }

    gboolean method_is_post = method && g_ascii_strcasecmp(method, "POST") == 0;
    gboolean method_is_get  = !method || !*method ||
                              g_ascii_strcasecmp(method, "GET") == 0;
    gboolean has_body = body && body_len > 0;
    gboolean extra_has_ct = FALSE;
    if (extra_headers) {
        for (guint i = 0; i < extra_headers->len; i++) {
            const char *h = g_ptr_array_index(extra_headers, i);
            if (h && g_ascii_strncasecmp(h, "Content-Type:", 13) == 0) {
                extra_has_ct = TRUE;
                break;
            }
        }
    }
    if (!extra_has_ct && (method_is_post || (has_body && !method_is_get && method))) {
        /* A form navigation without a type posts as a form; a script's
         * request sends the type its body has, if any. "Content-Type:"
         * keeps libcurl from adding its own form type. */
        char *ct_hdr = content_type && *content_type
            ? g_strdup_printf("Content-Type: %s", content_type)
            : g_strdup(is_navigation && has_body
                       ? "Content-Type: application/x-www-form-urlencoded"
                       : "Content-Type:");
        headers = curl_slist_append(headers, ct_hdr);
        g_free(ct_hdr);
    }

    if (extra_headers) {
        for (guint i = 0; i < extra_headers->len; i++) {
            const char *h = g_ptr_array_index(extra_headers, i);
            if (!h || !*h) continue;
            if (g_ascii_strncasecmp(h, "X-ND-", 5) == 0) continue;
            if (strpbrk(h, "\r\n")) continue;
            /* libcurl drops a header written "Name:" with nothing after the
             * colon; "Name;" is how it sends one with an empty value. */
            const char *colon = strchr(h, ':');
            const char *v = colon ? colon + 1 : NULL;
            while (v && (*v == ' ' || *v == '\t')) v++;
            if (colon && colon > h && v && !*v) {
                char *empty = g_strdup_printf("%.*s;", (int)(colon - h), h);
                headers = curl_slist_append(headers, empty);
                g_free(empty);
                continue;
            }
            headers = curl_slist_append(headers, h);
        }
    }

    ns_write_ctx write_ctx;
    ns_body_sink_init(&write_ctx, resp->body);

    ns_header_ctx header_ctx = {0};
    header_ctx.content_type_out = &resp->content_type;
    header_ctx.content_disposition_out = &resp->content_disposition;
    header_ctx.csp_out          = &resp->csp_header;
    header_ctx.xframe_options_out = &resp->xframe_options;
    header_ctx.x_content_type_options_out = &resp->x_content_type_options;
    header_ctx.cors_allow_origin_out = &resp->cors_allow_origin;
    header_ctx.refresh_out = &resp->refresh;
    header_ctx.content_language_out = &resp->content_language;

    gboolean initial_https = g_str_has_prefix(url, "https://");
    char *cookie_js_path = cookie_partition_path
        ? ns_net_cookie_js_path_for_partition(partition_key) : NULL;

    ns_hop_req req = {
        .url = url,
        .method = method,
        .body = body,
        .body_len = body_len,
        .headers = headers,
        .user_agent = effective_ua,
        .referer = referer,
        .referer_policy = cfg ? (int)cfg->referer_policy
                              : (int)NS_REFERER_STRICT_ORIGIN_WHEN_CROSS,
        .accept_encoding = ns_net_accept_encoding() ? ns_net_accept_encoding() : "",
        .timeout_s = fetch_timeout,
        .connect_timeout_s = is_navigation ? 15L : 6L,
        .proxy = ns_net_pick_configured_proxy(url),
        .no_proxy = ns_net_configured_no_proxy(),
        .cookie_jar_path = cookie_partition_path,
        .cookie_js_path = cookie_js_path,
        .follow_redirects = follow_redirects,
        .max_redirs = max_redirs,
        .is_navigation = is_navigation,
        .request_ftp = request_ftp,
        .initial_https = initial_https,
        .http_version_pref = ns_net_http_version(),
    };

    ns_hop_out out = {0};
    gint64 fetch_start_us = g_get_monotonic_time();
    double fetch_start_real_ms = (double)g_get_real_time() / 1000.0;
    gboolean produced = ns_hop_transport(&req, &write_ctx, &header_ctx, &out,
                                         cancellable);
    g_free(cookie_js_path);

    if (!produced) {
        if (out.cancelled)
            g_set_error_literal(error, G_IO_ERROR, G_IO_ERROR_CANCELLED,
                                "fetch cancelled");
        else
            g_set_error_literal(error, NS_NET_DOMAIN, 1,
                                out.error_message ? out.error_message
                                                  : "transport init failed");
        ns_hop_out_clear(&out);
        if (headers) curl_slist_free_all(headers);
        g_free(header_ctx.etag);
        g_free(header_ctx.last_modified);
        g_free(header_ctx.cache_control);
        g_free(header_ctx.vary);
        g_free(header_ctx.expires);
        g_free(header_ctx.location);
        if (header_ctx.raw) g_string_free(header_ctx.raw, TRUE);
        ns_cache_entry_free(cached);
        ns_response_free(resp);
        if (origin_held) ns_net_release_origin_slot(origin_slot);
        g_free(origin_slot);
        g_free(referer);
        g_free(cache_partition);
        g_free(cookie_partition_path);
        g_free(top_origin);
        g_free(top_site);
        g_free(hsts_upgraded);
        return NULL;
    }

    gboolean transport_ok = out.ok;
    resp->status = out.status;
    resp->final_url = g_strdup(out.effective_url ? out.effective_url : url);
    resp->redirect_count = 0;
    resp->request_start_us = fetch_start_us;
    resp->request_start_real_ms = fetch_start_real_ms;
    resp->domain_lookup_ms = out.t_namelookup_ms;
    resp->connect_ms = out.t_connect_ms;
    resp->tls_ms = out.t_appconnect_ms;
    resp->pretransfer_ms = out.t_pretransfer_ms;
    resp->response_start_ms = out.t_starttransfer_ms;
    resp->response_end_ms = out.t_total_ms;
    if (out.remote_ip)
        resp->remote_ip = g_strdup(out.remote_ip);
    if (out.http_version)
        resp->next_hop_protocol =
            g_strdup(ns_net_http_version_name(out.http_version));
    if (out.tls_warning)
        resp->tls_warning = g_strdup(out.tls_warning);
    {
        const char *sec_url = resp->final_url ? resp->final_url : url;
        if (g_str_has_prefix(sec_url, "https://")) {
            if (out.tls_verify_failed || resp->tls_warning)
                resp->security = NS_SEC_INVALID;
            else if (transport_ok)
                resp->security = NS_SEC_SECURE;
        } else if (g_str_has_prefix(sec_url, "http://")) {
            resp->security = NS_SEC_PLAIN;
        }
    }
    if (ns_net_log_fetches_enabled())
        ns_net_conn_stat_record(resp->final_url, out.http_version,
                                out.num_connects);
    if (transport_ok && request_ftp) {
        ns_net_finish_ftp_response(resp);
    }

    {
        char *reach_host = ns_url_origin_from_any(url);
        if (reach_host && *reach_host) {
            if (transport_ok || out.status > 0)
                ns_net_host_mark_alive(reach_host);
            else if (out.status == 0 && out.connect_failed)
                ns_net_host_mark_dead(reach_host);
        }
        g_free(reach_host);
    }

    if (!transport_ok) {
        if (write_ctx.exceeded)
            resp->error = g_strdup_printf(
                "response would exhaust available memory (stopped at %llu MiB)",
                (unsigned long long)(write_ctx.total >> 20));
        else
            resp->error = g_strdup(out.error_message ? out.error_message
                                                     : "transport error");
    }
    ns_hop_out_clear(&out);

    if (transport_ok && request_http && is_simple_get(method) &&
        !header_ctx.set_cookie_seen &&
        !resp->tls_warning) {
        if (resp->status == 304 && cached && cached->body) {
            ns_cache_promote_304(url, cache_partition, cache_request_headers,
                                 header_ctx.cache_control, header_ctx.expires);
            g_byte_array_set_size(resp->body, 0);
            g_byte_array_append(resp->body, cached->body->data, cached->body->len);
            resp->status = cached->status;
            g_free(resp->content_type);
            resp->content_type = g_strdup(cached->content_type);
            g_free(resp->cors_allow_origin);
            resp->cors_allow_origin = g_strdup(cached->cors_allow_origin);
        } else if (resp->status > 0 && resp->status < 300 &&
                   resp->body && resp->body->len > 0) {
            ns_cache_put(url, cache_partition,
                         resp->final_url, resp->status,
                         resp->content_type,
                         resp->cors_allow_origin,
                         header_ctx.etag, header_ctx.last_modified,
                         header_ctx.cache_control, header_ctx.expires,
                         header_ctx.vary, cache_request_headers,
                         resp->body->data, resp->body->len);
        }
    }
    g_free(referer);
    g_free(cache_partition);
    g_free(cookie_partition_path);
    g_free(top_origin);
    g_free(top_site);

    g_free(header_ctx.etag);
    g_free(header_ctx.last_modified);
    g_free(header_ctx.cache_control);
    g_free(header_ctx.vary);
    g_free(header_ctx.expires);
    if (location_out)
        *location_out = header_ctx.location;
    else
        g_free(header_ctx.location);
    if (header_ctx.raw) {
        g_free(resp->raw_headers);
        resp->raw_headers = g_string_free(header_ctx.raw, FALSE);
    }
    ns_cache_entry_free(cached);

    {
        gint64 fetch_end_us = g_get_monotonic_time();
        ns_net_perf_record(fetch_start_us, fetch_end_us,
                           resp->body ? (guint64)resp->body->len : 0);
        char *req_hdrs = ns_net_slist_serialize(headers);
        ns_net_log_record(method, url, resp->status, resp->content_type,
                          resp->body ? (guint64)resp->body->len : 0,
                          (fetch_end_us - fetch_start_us) / 1000.0,
                          req_hdrs, resp->raw_headers, resp->error);
        g_free(req_hdrs);
    }

    if (headers) curl_slist_free_all(headers);
    if (origin_held) ns_net_release_origin_slot(origin_slot);
    g_free(origin_slot);
    g_free(hsts_upgraded);
    return resp;
}

static gboolean
ns_fetch_is_navigation(const char *top_url, GPtrArray *extra_headers)
{
    (void)top_url;
    if (!extra_headers) return FALSE;
    for (guint i = 0; i < extra_headers->len; i++) {
        const char *h = g_ptr_array_index(extra_headers, i);
        if (h && g_ascii_strncasecmp(h, "X-ND-Navigate:", 14) == 0)
            return TRUE;
    }
    return FALSE;
}

static void
ns_headers_strip_sensitive(GPtrArray *headers)
{
    static const char *const sensitive[] = {
        "authorization", "cookie", "proxy-authorization", NULL,
    };
    for (guint i = 0; i < headers->len; ) {
        const char *h = g_ptr_array_index(headers, i);
        const char *colon = h ? strchr(h, ':') : NULL;
        gboolean drop = FALSE;
        if (colon) {
            size_t nlen = (size_t)(colon - h);
            for (int s = 0; sensitive[s]; s++)
                if (strlen(sensitive[s]) == nlen &&
                    g_ascii_strncasecmp(h, sensitive[s], nlen) == 0) {
                    drop = TRUE;
                    break;
                }
        }
        if (drop)
            g_ptr_array_remove_index(headers, i);
        else
            i++;
    }
}

static ns_response *
ns_fetch_sync(const char *url, const char *top_url, const char *method,
              const void *body, gsize body_len, const char *content_type,
              GPtrArray *extra_headers,
              GCancellable *cancellable, GError **error)
{
    if (!ns_fetch_is_navigation(top_url, extra_headers) &&
        ns_ext_should_block(url, top_url)) {
        ns_response *blocked = g_new0(ns_response, 1);
        blocked->body = g_byte_array_new();
        blocked->final_url = g_strdup(url);
        blocked->status = 0;
        blocked->error = g_strdup("blocked by extension");
        return blocked;
    }

    const ns_config *cfg = ns_config_get();
    long max_redirs = cfg ? (long)cfg->max_redirects : (long)NS_MAX_REDIRECTS;
    if (max_redirs < 0)                       max_redirs = 0;
    if (max_redirs > (long)NS_MAX_REDIRECTS)  max_redirs = (long)NS_MAX_REDIRECTS;

    char *cur_url = g_strdup(url);
    char *cur_top = g_strdup(top_url);
    char *cur_method = g_strdup(method && *method ? method : "GET");
    const void *cur_body = body;
    gsize cur_len = body_len;
    const char *cur_ct = content_type;
    gboolean started_https = g_str_has_prefix(url, "https://");
    int hops = 0;
    ns_response *resp = NULL;

    GPtrArray *hop_headers = NULL;
    if (extra_headers) {
        hop_headers = g_ptr_array_sized_new(extra_headers->len);
        for (guint i = 0; i < extra_headers->len; i++)
            g_ptr_array_add(hop_headers, g_ptr_array_index(extra_headers, i));
    }

    for (;;) {
        char *location = NULL;
        resp = ns_fetch_sync_hop(cur_url, cur_top, cur_method,
                                 cur_body, cur_len, cur_ct,
                                 hop_headers, cancellable, error,
                                 FALSE, &location);
        if (!resp) {
            g_free(location);
            break;
        }
        gboolean is_redirect = resp->status == 301 || resp->status == 302 ||
                               resp->status == 303 || resp->status == 307 ||
                               resp->status == 308;
        if (!is_redirect || resp->error || !location || !*location) {
            g_free(location);
            break;
        }
        if (hops >= max_redirs) {
            g_free(resp->error);
            resp->error = g_strdup("too many redirects");
            g_free(location);
            break;
        }
        const char *base = resp->final_url ? resp->final_url : cur_url;
        char *next = ns_url_resolve(base, location);
        g_free(location);
        if (!next) break;
        if (!ns_url_is_http_or_https(next) ||
            (started_https && !g_str_has_prefix(next, "https://") &&
             !ns_fetch_is_navigation(cur_top, extra_headers))) {
            g_free(resp->error);
            resp->error = g_strdup("redirect to a disallowed URL blocked");
            g_free(next);
            break;
        }
        if (hop_headers) {
            char *from_origin = ns_url_origin_from(base);
            char *to_origin = ns_url_origin_from(next);
            if (g_strcmp0(from_origin, to_origin) != 0)
                ns_headers_strip_sensitive(hop_headers);
            g_free(from_origin);
            g_free(to_origin);
        }
        /* Fetch's HTTP-redirect fetch: 301 and 302 turn a POST into a GET,
         * 303 turns anything but GET and HEAD into one, and the request
         * then loses its body and the headers that describe it. */
        gboolean was_post = g_ascii_strcasecmp(cur_method, "POST") == 0;
        gboolean get_or_head = g_ascii_strcasecmp(cur_method, "GET") == 0 ||
                               g_ascii_strcasecmp(cur_method, "HEAD") == 0;
        if (((resp->status == 301 || resp->status == 302) && was_post) ||
            (resp->status == 303 && !get_or_head)) {
            g_free(cur_method);
            cur_method = g_strdup("GET");
            cur_body = NULL;
            cur_len = 0;
            cur_ct = NULL;
            static const char *const body_headers[] = {
                "Content-Encoding:", "Content-Language:", "Content-Location:",
                "Content-Type:",
            };
            for (guint i = 0; hop_headers && i < hop_headers->len; ) {
                const char *h = g_ptr_array_index(hop_headers, i);
                gboolean drop = FALSE;
                for (gsize k = 0; h && k < G_N_ELEMENTS(body_headers); k++)
                    if (g_ascii_strncasecmp(h, body_headers[k],
                                            strlen(body_headers[k])) == 0)
                        drop = TRUE;
                if (drop) g_ptr_array_remove_index(hop_headers, i);
                else i++;
            }
        }
        if (ns_fetch_is_navigation(cur_top, extra_headers) &&
            g_ascii_strcasecmp(cur_method, "GET") == 0) {
            g_free(cur_top);
            cur_top = NULL;
        }
        g_free(cur_url);
        cur_url = next;
        hops++;
        ns_response_free(resp);
        resp = NULL;
        if (cancellable && g_cancellable_is_cancelled(cancellable)) {
            g_set_error_literal(error, G_IO_ERROR, G_IO_ERROR_CANCELLED,
                                "fetch cancelled");
            break;
        }
    }
    if (resp) resp->redirect_count = hops;
    if (hop_headers) g_ptr_array_free(hop_headers, TRUE);
    g_free(cur_url);
    g_free(cur_top);
    g_free(cur_method);
    return resp;
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
