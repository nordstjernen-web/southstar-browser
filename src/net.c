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
#include <errno.h>
#include <stdio.h>
#include <string.h>
#include <time.h>

#include <glib/gstdio.h>
#include <gmodule.h>

#ifdef G_OS_WIN32
#include <windows.h>
#endif

#ifdef __APPLE__
#include <mach-o/dyld.h>
#include <stdlib.h>
#include <sys/sysctl.h>
#endif

char    *ns_url_to_ascii(const char *url);
char    *ns_url_origin_from_any(const char *url);
char    *ns_url_site_from(const char *url);
gboolean ns_url_is_same_site(const char *a, const char *b);
const char *ns_net_cookie_dir(void);
const char *ns_net_hsts_curl_path(void);
const char *ns_net_altsvc_path(void);
char    *ns_net_cookie_path_for_partition(const char *top_origin);
char    *ns_net_cookie_js_path_for_partition(const char *top_origin);
void     ns_net_storage_shutdown(void);

static char *g_ca_bundle;
static gboolean g_has_http3;
static const char *g_ec_curves = "X25519:P-256:P-384";
static char *g_accept_encoding;
static char *g_proxy_override;
static CURLSH *g_share;
static GMutex g_fetch_throttle_mutex;
static GCond  g_fetch_idle_cond;
static int    g_fetch_active;
static int    g_preconnect_active;
static gint   g_net_aborting;
static GQueue g_fetch_queue = G_QUEUE_INIT;
static GMutex g_share_locks[CURL_LOCK_DATA_LAST];
static GMutex      g_conn_stats_lock;
static GHashTable *g_conn_stats;

#define NS_DEAD_HOST_TTL_US ((gint64)120 * G_USEC_PER_SEC)
/* Origins (scheme, host and port) a connection recently failed to.  A
 * refused port says nothing about the host's other ports, so the key is
 * the origin, not the host. */
static GHashTable *g_dead_hosts;
static GMutex      g_dead_hosts_lock;

static gboolean
ns_net_host_recently_dead(const char *host)
{
    gboolean dead = FALSE;
    g_mutex_lock(&g_dead_hosts_lock);
    if (g_dead_hosts) {
        gint64 *expiry = g_hash_table_lookup(g_dead_hosts, host);
        if (expiry) {
            if (g_get_monotonic_time() < *expiry)
                dead = TRUE;
            else
                g_hash_table_remove(g_dead_hosts, host);
        }
    }
    g_mutex_unlock(&g_dead_hosts_lock);
    return dead;
}

static void
ns_net_host_mark_dead(const char *host)
{
    g_mutex_lock(&g_dead_hosts_lock);
    if (!g_dead_hosts)
        g_dead_hosts = g_hash_table_new_full(g_str_hash, g_str_equal,
                                             g_free, g_free);
    gint64 *expiry = g_new(gint64, 1);
    *expiry = g_get_monotonic_time() + NS_DEAD_HOST_TTL_US;
    g_hash_table_replace(g_dead_hosts, g_strdup(host), expiry);
    g_mutex_unlock(&g_dead_hosts_lock);
}

static void
ns_net_host_mark_alive(const char *host)
{
    g_mutex_lock(&g_dead_hosts_lock);
    if (g_dead_hosts)
        g_hash_table_remove(g_dead_hosts, host);
    g_mutex_unlock(&g_dead_hosts_lock);
}

#define NS_NET_MAX_PER_ORIGIN 6

typedef struct ns_origin_slot {
    int   in_use;
    GCond cond;
} ns_origin_slot;

static GMutex      g_origin_slots_lock;
static GHashTable *g_origin_slots;

static void
ns_origin_slot_free(gpointer p)
{
    ns_origin_slot *s = p;
    g_cond_clear(&s->cond);
    g_free(s);
}

static char *
origin_slot_key(const char *origin)
{
    return (origin && *origin) ? g_ascii_strdown(origin, -1) : NULL;
}

static gboolean
ns_net_acquire_origin_slot(const char *origin, GCancellable *cancellable)
{
    char *key = origin_slot_key(origin);
    if (!key) return FALSE;
    g_mutex_lock(&g_origin_slots_lock);
    if (!g_origin_slots)
        g_origin_slots = g_hash_table_new_full(g_str_hash, g_str_equal,
                                               g_free, ns_origin_slot_free);
    ns_origin_slot *s = g_hash_table_lookup(g_origin_slots, key);
    if (!s) {
        s = g_new0(ns_origin_slot, 1);
        g_cond_init(&s->cond);
        g_hash_table_insert(g_origin_slots, key, s);
        key = NULL;
    }
    while (s->in_use >= NS_NET_MAX_PER_ORIGIN) {
        if (cancellable && g_cancellable_is_cancelled(cancellable)) {
            g_mutex_unlock(&g_origin_slots_lock);
            g_free(key);
            return FALSE;
        }
        gint64 wakeup = g_get_monotonic_time() + 250 * G_TIME_SPAN_MILLISECOND;
        g_cond_wait_until(&s->cond, &g_origin_slots_lock, wakeup);
    }
    s->in_use++;
    g_mutex_unlock(&g_origin_slots_lock);
    g_free(key);
    return TRUE;
}

static void
ns_net_release_origin_slot(const char *origin)
{
    char *key = origin_slot_key(origin);
    if (!key) return;
    g_mutex_lock(&g_origin_slots_lock);
    if (g_origin_slots) {
        ns_origin_slot *s = g_hash_table_lookup(g_origin_slots, key);
        if (s && s->in_use > 0) {
            s->in_use--;
            g_cond_signal(&s->cond);
        }
    }
    g_mutex_unlock(&g_origin_slots_lock);
    g_free(key);
}

typedef struct ns_multi_xfer {
    CURL     *easy;
    GCond     cond;
    gboolean  done;
    CURLcode  result;
} ns_multi_xfer;

static CURLM      *g_multi;
static GThread    *g_multi_thread;
static gboolean    g_multi_quit;
static GMutex      g_multi_lock;
static GQueue      g_multi_incoming = G_QUEUE_INIT;
static GHashTable *g_multi_active;

static void
ns_net_multi_finish_locked(ns_multi_xfer *x, CURLcode result)
{
    x->result = result;
    x->done = TRUE;
    g_cond_signal(&x->cond);
}

static gpointer
ns_net_multi_loop(gpointer data)
{
    (void)data;
    for (;;) {
        g_mutex_lock(&g_multi_lock);
        if (g_multi_quit) {
            GHashTableIter it;
            gpointer key, val;
            g_hash_table_iter_init(&it, g_multi_active);
            while (g_hash_table_iter_next(&it, &key, &val)) {
                ns_multi_xfer *x = val;
                curl_multi_remove_handle(g_multi, x->easy);
                ns_net_multi_finish_locked(x, CURLE_ABORTED_BY_CALLBACK);
            }
            g_hash_table_remove_all(g_multi_active);
            for (ns_multi_xfer *x; (x = g_queue_pop_head(&g_multi_incoming)); )
                ns_net_multi_finish_locked(x, CURLE_ABORTED_BY_CALLBACK);
            g_mutex_unlock(&g_multi_lock);
            break;
        }
        for (ns_multi_xfer *x; (x = g_queue_pop_head(&g_multi_incoming)); ) {
            if (curl_multi_add_handle(g_multi, x->easy) == CURLM_OK)
                g_hash_table_insert(g_multi_active, x->easy, x);
            else
                ns_net_multi_finish_locked(x, CURLE_FAILED_INIT);
        }
        g_mutex_unlock(&g_multi_lock);

        int running = 0;
        curl_multi_perform(g_multi, &running);

        int nmsgs = 0;
        CURLMsg *m;
        while ((m = curl_multi_info_read(g_multi, &nmsgs))) {
            if (m->msg != CURLMSG_DONE) continue;
            CURL *easy = m->easy_handle;
            CURLcode res = m->data.result;
            curl_multi_remove_handle(g_multi, easy);
            g_mutex_lock(&g_multi_lock);
            ns_multi_xfer *x = g_hash_table_lookup(g_multi_active, easy);
            if (x) {
                g_hash_table_remove(g_multi_active, easy);
                ns_net_multi_finish_locked(x, res);
            }
            g_mutex_unlock(&g_multi_lock);
        }

        long timeo = -1;
        curl_multi_timeout(g_multi, &timeo);
        int wait_ms = (timeo < 0 || timeo > 1000) ? 1000 : (int)timeo;
        curl_multi_poll(g_multi, NULL, 0, wait_ms, NULL);
    }
    return NULL;
}

static void
ns_net_multi_start(void)
{
    g_mutex_lock(&g_multi_lock);
    if (!g_multi_thread) {
        g_multi = curl_multi_init();
        if (g_multi) {
            curl_multi_setopt(g_multi, CURLMOPT_PIPELINING,
                              (long)CURLPIPE_MULTIPLEX);
            g_multi_active = g_hash_table_new(g_direct_hash, g_direct_equal);
            g_multi_thread = g_thread_new("ns-net-multi",
                                          ns_net_multi_loop, NULL);
        }
    }
    g_mutex_unlock(&g_multi_lock);
}

static CURLcode
ns_net_multi_perform(CURL *easy, GCancellable *cancellable)
{
    (void)cancellable;
    ns_net_multi_start();
    if (!g_multi) return curl_easy_perform(easy);

    ns_multi_xfer x = { .easy = easy, .done = FALSE, .result = CURLE_OK };
    g_cond_init(&x.cond);

    g_mutex_lock(&g_multi_lock);
    if (g_multi_quit) {
        g_mutex_unlock(&g_multi_lock);
        g_cond_clear(&x.cond);
        return curl_easy_perform(easy);
    }
    g_queue_push_tail(&g_multi_incoming, &x);
    curl_multi_wakeup(g_multi);
    while (!x.done) {
        gint64 wakeup = g_get_monotonic_time() + 250 * G_TIME_SPAN_MILLISECOND;
        g_cond_wait_until(&x.cond, &g_multi_lock, wakeup);
    }
    g_mutex_unlock(&g_multi_lock);

    g_cond_clear(&x.cond);
    return x.result;
}

static void
ns_net_multi_shutdown(void)
{
    g_mutex_lock(&g_multi_lock);
    GThread *t = g_multi_thread;
    if (t) {
        g_multi_quit = TRUE;
        curl_multi_wakeup(g_multi);
    }
    g_mutex_unlock(&g_multi_lock);
    if (t) {
        g_thread_join(t);
        g_multi_thread = NULL;
    }
    if (g_multi) { curl_multi_cleanup(g_multi); g_multi = NULL; }
    if (g_multi_active) {
        g_hash_table_destroy(g_multi_active);
        g_multi_active = NULL;
    }
    g_multi_quit = FALSE;
}

static gboolean
ns_url_is_ftp(const char *url)
{
    return url && g_str_has_prefix(url, "ftp://");
}

static long
ns_net_http_version(void)
{
#ifdef CURL_VERSION_HTTP3
    static gsize once = 0;
    static long version = CURL_HTTP_VERSION_2TLS;
    if (g_once_init_enter(&once)) {
        const curl_version_info_data *info = curl_version_info(CURLVERSION_NOW);
        if (info && (info->features & CURL_VERSION_HTTP3) &&
            g_getenv("NS_FORCE_HTTP3"))
            version = CURL_HTTP_VERSION_3;
        g_once_init_leave(&once, 1);
    }
    return version;
#else
    return CURL_HTTP_VERSION_2TLS;
#endif
}

#define NS_NET_DOMAIN ns_net_error_quark()

static GQuark
ns_net_error_quark(void)
{
    return g_quark_from_static_string("nd-net-error");
}

static int
ns_xferinfo_cb(void *clientp, curl_off_t dltotal, curl_off_t dlnow,
               curl_off_t ultotal, curl_off_t ulnow)
{
    (void)dltotal; (void)dlnow; (void)ultotal; (void)ulnow;
    if (g_atomic_int_get(&g_net_aborting)) return 1;
    GCancellable *c = clientp;
    return (c && g_cancellable_is_cancelled(c)) ? 1 : 0;
}

extern const char *ns_app_self_exe(void);

static char *
ns_net_exe_dir(void)
{
    const char *self = ns_app_self_exe();
    if (self && *self)
        return g_path_get_dirname(self);
#ifdef G_OS_WIN32
    DWORD cap = MAX_PATH;
    wchar_t *buf = g_new(wchar_t, cap);
    DWORD n = GetModuleFileNameW(NULL, buf, cap);
    while (n >= cap && cap < 32768) {
        cap *= 2;
        wchar_t *bigger = g_renew(wchar_t, buf, cap);
        buf = bigger;
        n = GetModuleFileNameW(NULL, buf, cap);
    }
    char *utf8 = NULL;
    if (n > 0 && n < cap)
        utf8 = g_utf16_to_utf8((gunichar2 *)buf, -1, NULL, NULL, NULL);
    g_free(buf);
    if (!utf8) return NULL;
    char *dir = g_path_get_dirname(utf8);
    g_free(utf8);
    return dir;
#elif defined(__APPLE__)
    uint32_t size = 0;
    _NSGetExecutablePath(NULL, &size);
    if (size == 0 || size > 32768) return NULL;
    char *raw = g_malloc(size);
    if (_NSGetExecutablePath(raw, &size) != 0) { g_free(raw); return NULL; }
    char *dir = g_path_get_dirname(raw);
    g_free(raw);
    return dir;
#elif defined(__linux__)
    char *exe = g_file_read_link("/proc/self/exe", NULL);
    if (!exe) return NULL;
    char *dir = g_path_get_dirname(exe);
    g_free(exe);
    return dir;
#else
    return NULL;
#endif
}

static gboolean
ns_net_try_ca_bundle(const char *path)
{
    if (!path || !*path) return FALSE;
    if (!g_file_test(path, G_FILE_TEST_EXISTS)) return FALSE;
    g_ca_bundle = g_strdup(path);
    return TRUE;
}

static void
ns_net_resolve_ca_bundle(void)
{
    if (g_ca_bundle) return;
    const char *env = g_getenv("CURL_CA_BUNDLE");
    if (!env) env = g_getenv("SSL_CERT_FILE");
    if (ns_net_try_ca_bundle(env)) return;

    char *dir = ns_net_exe_dir();
    if (dir) {
        const char *rels[] = {
            "etc/ssl/certs/ca-bundle.crt",
            "ssl/certs/ca-bundle.crt",
            "ca-bundle.crt",
            "cert.pem",
            "../etc/ca-certificates/cert.pem",
            "../etc/openssl@3/cert.pem",
            "../etc/openssl/cert.pem",
            NULL,
        };
        for (int i = 0; rels[i]; i++) {
            char *cand = g_build_filename(dir, rels[i], NULL);
            gboolean ok = ns_net_try_ca_bundle(cand);
            g_free(cand);
            if (ok) break;
        }
        g_free(dir);
        if (g_ca_bundle) return;
    }

#if defined(__linux__) || defined(__FreeBSD__) || defined(__NetBSD__)
    const char *unix_paths[] = {
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
        "/var/lib/ca-certificates/ca-bundle.pem",
        "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
        "/etc/ssl/cert.pem",
        "/usr/local/share/certs/ca-root-nss.crt",
        NULL,
    };
    for (int i = 0; unix_paths[i]; i++)
        if (ns_net_try_ca_bundle(unix_paths[i])) return;
#endif

#ifdef __APPLE__
    const char *mac_paths[] = {
        "/opt/homebrew/etc/ca-certificates/cert.pem",
        "/opt/homebrew/etc/openssl@3/cert.pem",
        "/usr/local/etc/ca-certificates/cert.pem",
        "/usr/local/etc/openssl@3/cert.pem",
        "/usr/local/etc/openssl/cert.pem",
        "/etc/ssl/cert.pem",
        NULL,
    };
    for (int i = 0; mac_paths[i]; i++)
        if (ns_net_try_ca_bundle(mac_paths[i])) return;
#endif

#ifdef G_OS_WIN32
    const char *win_paths[] = {
        "C:/msys64/mingw64/etc/ssl/certs/ca-bundle.crt",
        "C:/msys64/mingw64/etc/ssl/cert.pem",
        "C:/msys64/ucrt64/etc/ssl/certs/ca-bundle.crt",
        "C:/msys64/clang64/etc/ssl/certs/ca-bundle.crt",
        NULL,
    };
    for (int i = 0; win_paths[i]; i++)
        if (ns_net_try_ca_bundle(win_paths[i])) return;

    g_info("ns_net: no CA bundle file found; relying on "
           "CURLSSLOPT_NATIVE_CA via the Windows certificate store. "
           "If HTTPS fails, install mingw-w64-x86_64-ca-certificates or "
           "set CURL_CA_BUNDLE.");
#endif
}

static void
ns_share_lock(CURL *handle, curl_lock_data data,
              curl_lock_access access, void *user_data)
{
    (void)handle; (void)access; (void)user_data;
    if (data < CURL_LOCK_DATA_LAST)
        g_mutex_lock(&g_share_locks[data]);
}

static void
ns_share_unlock(CURL *handle, curl_lock_data data, void *user_data)
{
    (void)handle; (void)user_data;
    if (data < CURL_LOCK_DATA_LAST)
        g_mutex_unlock(&g_share_locks[data]);
}

static gpointer
ns_rng_warmup_thread(gpointer data)
{
    (void)data;
    int (*rand_bytes)(unsigned char *, int) = NULL;
    GModule *self = g_module_open(NULL, G_MODULE_BIND_LAZY);
    if (self &&
        g_module_symbol(self, "RAND_bytes", (gpointer *)&rand_bytes) &&
        rand_bytes) {
        unsigned char buf[32];
        rand_bytes(buf, (int)sizeof buf);
    }
    if (self) g_module_close(self);
    return NULL;
}

static GThread *g_rng_warmup_thread;

static void
ns_net_warm_rng(void)
{
    if (!g_module_supported()) return;
    g_rng_warmup_thread = g_thread_try_new("nd-rng-warmup",
                                           ns_rng_warmup_thread, NULL, NULL);
}

static void
ns_net_join_rng(void)
{
    if (g_rng_warmup_thread) {
        g_thread_join(g_rng_warmup_thread);
        g_rng_warmup_thread = NULL;
    }
}

void
ns_net_init(void)
{
    ns_net_resolve_ca_bundle();
    curl_global_init(CURL_GLOBAL_DEFAULT);
    curl_version_info_data *vi = curl_version_info(CURLVERSION_NOW);
    g_has_http3 = vi && (vi->features & CURL_VERSION_HTTP3) != 0;

    unsigned ossl_major = 0, ossl_minor = 0;
    if (vi && vi->ssl_version &&
        sscanf(vi->ssl_version, "OpenSSL/%u.%u", &ossl_major, &ossl_minor) == 2 &&
        (ossl_major > 3 || (ossl_major == 3 && ossl_minor >= 5)))
        g_ec_curves = "X25519MLKEM768:X25519:P-256:P-384";

    GString *enc = g_string_new(NULL);
    if (vi && (vi->features & CURL_VERSION_LIBZ) != 0)
        g_string_append(enc, "gzip, deflate");
#ifdef CURL_VERSION_BROTLI
    if (vi && (vi->features & CURL_VERSION_BROTLI) != 0) {
        if (enc->len) g_string_append(enc, ", ");
        g_string_append(enc, "br");
    }
#endif
#ifdef CURL_VERSION_ZSTD
    if (vi && (vi->features & CURL_VERSION_ZSTD) != 0) {
        if (enc->len) g_string_append(enc, ", ");
        g_string_append(enc, "zstd");
    }
#endif
    g_free(g_accept_encoding);
    g_accept_encoding = g_string_free(enc, FALSE);

    g_share = curl_share_init();
    if (g_share) {
        curl_share_setopt(g_share, CURLSHOPT_SHARE, CURL_LOCK_DATA_DNS);
        curl_share_setopt(g_share, CURLSHOPT_SHARE, CURL_LOCK_DATA_SSL_SESSION);
#ifdef CURL_LOCK_DATA_CONNECT
        curl_share_setopt(g_share, CURLSHOPT_SHARE, CURL_LOCK_DATA_CONNECT);
#endif
#ifdef CURL_LOCK_DATA_PSL
        curl_share_setopt(g_share, CURLSHOPT_SHARE, CURL_LOCK_DATA_PSL);
#endif
#ifdef CURL_LOCK_DATA_HSTS
        curl_share_setopt(g_share, CURLSHOPT_SHARE, CURL_LOCK_DATA_HSTS);
#endif
        curl_share_setopt(g_share, CURLSHOPT_LOCKFUNC,   ns_share_lock);
        curl_share_setopt(g_share, CURLSHOPT_UNLOCKFUNC, ns_share_unlock);
    }

    ns_net_warm_rng();

    ns_net_hsts_curl_path();
    ns_net_altsvc_path();
    ns_net_cookie_dir();
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
    g_atomic_int_set(&g_net_aborting, 1);
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
    if (g_conn_stats) {
        g_hash_table_destroy(g_conn_stats);
        g_conn_stats = NULL;
    }
    if (g_share) { curl_share_cleanup(g_share); g_share = NULL; }
    curl_global_cleanup();
    g_free(g_accept_encoding);
    g_accept_encoding = NULL;
    g_free(g_proxy_override);
    g_proxy_override = NULL;
    ns_net_storage_shutdown();
    g_free(g_ca_bundle);
    g_ca_bundle = NULL;
    if (g_origin_slots) {
        g_hash_table_destroy(g_origin_slots);
        g_origin_slots = NULL;
    }
}

void
ns_net_set_proxy_override(const char *proxy_url)
{
    g_free(g_proxy_override);
    g_proxy_override = (proxy_url && *proxy_url) ? g_strdup(proxy_url) : NULL;
}

static gboolean g_log_fetches = FALSE;

void
ns_net_set_log_fetches(gboolean on)
{
    g_log_fetches = on;
}

typedef struct ns_conn_stat {
    guint64 requests;
    guint64 connections;
} ns_conn_stat;

static const char *
ns_net_http_version_name(long v)
{
    switch (v) {
    case CURL_HTTP_VERSION_1_0: return "http/1.0";
    case CURL_HTTP_VERSION_1_1: return "http/1.1";
    case CURL_HTTP_VERSION_2_0: return "h2";
#ifdef CURL_HTTP_VERSION_3
    case CURL_HTTP_VERSION_3:   return "h3";
#endif
    default:                    return "http/?";
    }
}

static GMutex   g_perf_lock;
static guint64  g_perf_fetch_count;
static guint64  g_perf_fetch_bytes;
static gint64   g_perf_fetch_sum_us;
static gint64   g_perf_fetch_first_us;
static gint64   g_perf_fetch_last_us;

static void
ns_net_perf_record(gint64 start_us, gint64 end_us, guint64 bytes)
{
    if (!g_log_fetches) return;
    g_mutex_lock(&g_perf_lock);
    if (g_perf_fetch_count == 0 || start_us < g_perf_fetch_first_us)
        g_perf_fetch_first_us = start_us;
    if (end_us > g_perf_fetch_last_us)
        g_perf_fetch_last_us = end_us;
    g_perf_fetch_count++;
    g_perf_fetch_bytes += bytes;
    g_perf_fetch_sum_us += end_us - start_us;
    g_mutex_unlock(&g_perf_lock);
}

void
ns_net_perf_snapshot(guint64 *fetches, guint64 *bytes,
                     double *sum_ms, double *span_ms)
{
    g_mutex_lock(&g_perf_lock);
    if (fetches) *fetches = g_perf_fetch_count;
    if (bytes)   *bytes   = g_perf_fetch_bytes;
    if (sum_ms)  *sum_ms  = g_perf_fetch_sum_us / 1000.0;
    if (span_ms) *span_ms = g_perf_fetch_count
                            ? (g_perf_fetch_last_us - g_perf_fetch_first_us) / 1000.0
                            : 0.0;
    g_mutex_unlock(&g_perf_lock);
}

static void
ns_net_conn_stat_record(const char *url, long http_version, long new_connections)
{
    if (!g_log_fetches) return;
    char *origin = ns_url_origin_from(url);
    if (!origin) return;

    g_mutex_lock(&g_conn_stats_lock);
    if (!g_conn_stats)
        g_conn_stats = g_hash_table_new_full(g_str_hash, g_str_equal,
                                             g_free, g_free);
    ns_conn_stat *s = g_hash_table_lookup(g_conn_stats, origin);
    if (!s) {
        s = g_new0(ns_conn_stat, 1);
        g_hash_table_insert(g_conn_stats, g_strdup(origin), s);
    }
    s->requests++;
    if (new_connections > 0)
        s->connections += (guint64)new_connections;
    guint64 reqs = s->requests, conns = s->connections;
    g_mutex_unlock(&g_conn_stats_lock);

    ns_debug_log_emit(NS_DLOG_NET, "conn", "%s new=%ld origin=%s reqs=%"
                      G_GUINT64_FORMAT " conns=%" G_GUINT64_FORMAT,
                      ns_net_http_version_name(http_version),
                      new_connections, origin, reqs, conns);
    g_free(origin);
}

static const char *
ns_net_pick_configured_proxy(const char *url)
{
    if (g_proxy_override && *g_proxy_override) return g_proxy_override;
    const ns_config *cfg = ns_config_get();
    if (!cfg) return NULL;
    gboolean https = g_str_has_prefix(url, "https://") ||
                     g_str_has_prefix(url, "wss://");
    if (https && cfg->https_proxy && *cfg->https_proxy) return cfg->https_proxy;
    if (cfg->http_proxy && *cfg->http_proxy)            return cfg->http_proxy;
    return NULL;
}

static const char *
ns_net_configured_no_proxy(void)
{
    const ns_config *cfg = ns_config_get();
    if (cfg && cfg->no_proxy && *cfg->no_proxy) return cfg->no_proxy;
    return NULL;
}

void
ns_net_apply_curl_proxy(void *curl_handle, const char *url)
{
    CURL *curl = curl_handle;
    const char *proxy = ns_net_pick_configured_proxy(url);
    if (proxy && *proxy)
        curl_easy_setopt(curl, CURLOPT_PROXY, proxy);
    const char *no_proxy = ns_net_configured_no_proxy();
    if (no_proxy && *no_proxy)
        curl_easy_setopt(curl, CURLOPT_NOPROXY, no_proxy);
}

const char *
ns_net_proxy_override(void)
{
    return g_proxy_override;
}

const char *
ns_net_http_proxy(void)
{
    const ns_config *cfg = ns_config_get();
    return cfg ? cfg->http_proxy : NULL;
}

const char *
ns_net_https_proxy(void)
{
    const ns_config *cfg = ns_config_get();
    return cfg ? cfg->https_proxy : NULL;
}

const char *
ns_net_no_proxy(void)
{
    const ns_config *cfg = ns_config_get();
    return cfg ? cfg->no_proxy : NULL;
}

const char *
ns_net_ca_bundle_path(void)
{
    return g_ca_bundle;
}

const char *
ns_net_ec_curves(void)
{
    return g_ec_curves;
}

gboolean
ns_net_aborting(void)
{
    return g_atomic_int_get(&g_net_aborting) != 0;
}

void
ns_net_apply_curl_tls(void *curl_handle)
{
    CURL *curl = curl_handle;
    curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 1L);
    curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 2L);
    curl_easy_setopt(curl, CURLOPT_SSL_CIPHER_LIST,
        "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:"
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:"
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:"
        "ECDHE-RSA-AES128-SHA:ECDHE-RSA-AES256-SHA:"
        "AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA");
#ifdef CURLOPT_TLS13_CIPHERS
    curl_easy_setopt(curl, CURLOPT_TLS13_CIPHERS,
        "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:"
        "TLS_CHACHA20_POLY1305_SHA256");
#endif
    curl_easy_setopt(curl, CURLOPT_SSL_EC_CURVES, g_ec_curves);
#if LIBCURL_VERSION_NUM >= 0x080800
    {
        const curl_version_info_data *info = curl_version_info(CURLVERSION_NOW);
        const char *const *feat = info ? info->feature_names : NULL;
        for (; feat && *feat; feat++) {
            if (!g_ascii_strcasecmp(*feat, "ECH")) {
                curl_easy_setopt(curl, CURLOPT_ECH, "true");
                break;
            }
        }
    }
#endif
    if (g_ca_bundle)
        curl_easy_setopt(curl, CURLOPT_CAINFO, g_ca_bundle);
#ifdef G_OS_WIN32
    curl_easy_setopt(curl, CURLOPT_SSL_OPTIONS, (long)CURLSSLOPT_NATIVE_CA);
#endif
#ifdef CURLOPT_DOH_URL
    const ns_config *cfg = ns_config_get();
    if (cfg && cfg->doh_url && g_str_has_prefix(cfg->doh_url, "https://"))
        curl_easy_setopt(curl, CURLOPT_DOH_URL, cfg->doh_url);
#endif
}

void
ns_response_free(ns_response *resp)
{
    if (!resp)
        return;
    g_free(resp->final_url);
    g_free(resp->content_type);
    g_free(resp->content_disposition);
    g_free(resp->csp_header);
    g_free(resp->xframe_options);
    g_free(resp->x_content_type_options);
    g_free(resp->cors_allow_origin);
    g_free(resp->refresh);
    g_free(resp->content_language);
    g_free(resp->raw_headers);
    if (resp->body)
        g_byte_array_unref(resp->body);
    g_free(resp->error);
    g_free(resp->tls_warning);
    g_free(resp->remote_ip);
    g_free(resp->next_hop_protocol);
    g_free(resp);
}

static ns_response *
ns_response_copy(const ns_response *src)
{
    if (!src) return NULL;
    ns_response *r = g_new0(ns_response, 1);
    *r = *src;
    r->final_url = g_strdup(src->final_url);
    r->content_type = g_strdup(src->content_type);
    r->content_disposition = g_strdup(src->content_disposition);
    r->csp_header = g_strdup(src->csp_header);
    r->xframe_options = g_strdup(src->xframe_options);
    r->x_content_type_options = g_strdup(src->x_content_type_options);
    r->cors_allow_origin = g_strdup(src->cors_allow_origin);
    r->refresh = g_strdup(src->refresh);
    r->content_language = g_strdup(src->content_language);
    r->raw_headers = g_strdup(src->raw_headers);
    r->error = g_strdup(src->error);
    r->tls_warning = g_strdup(src->tls_warning);
    r->remote_ip = g_strdup(src->remote_ip);
    r->next_hop_protocol = g_strdup(src->next_hop_protocol);
    r->body = g_byte_array_new();
    if (src->body && src->body->len)
        g_byte_array_append(r->body, src->body->data, src->body->len);
    return r;
}

typedef struct {
    char   *method;
    char   *url;
    long    status;
    char   *content_type;
    guint64 body_len;
    double  duration_ms;
    char   *req_headers;
    char   *resp_headers;
    char   *error;
} ns_net_log_entry;

#define NS_NET_LOG_CAP 256

static GMutex      ns_net_log_lock;
static GPtrArray  *ns_net_log;

static void
ns_net_log_entry_free(gpointer data)
{
    ns_net_log_entry *e = data;
    if (!e)
        return;
    g_free(e->method);
    g_free(e->url);
    g_free(e->content_type);
    g_free(e->req_headers);
    g_free(e->resp_headers);
    g_free(e->error);
    g_free(e);
}

static void
ns_net_log_record(const char *method, const char *url, long status,
                  const char *content_type, guint64 body_len,
                  double duration_ms, const char *req_headers,
                  const char *resp_headers, const char *error)
{
    if (!url || !*url)
        return;
    if (g_str_has_prefix(url, "data:") || g_str_has_prefix(url, "about:"))
        return;
    ns_net_log_entry *e = g_new0(ns_net_log_entry, 1);
    e->method = g_strdup(method && *method ? method : "GET");
    e->url = g_strdup(url);
    e->status = status;
    e->content_type = g_strdup(content_type ? content_type : "");
    e->body_len = body_len;
    e->duration_ms = duration_ms;
    e->req_headers = g_strdup(req_headers ? req_headers : "");
    e->resp_headers = g_strdup(resp_headers ? resp_headers : "");
    e->error = error && *error ? g_strdup(error) : NULL;

    g_mutex_lock(&ns_net_log_lock);
    if (!ns_net_log)
        ns_net_log = g_ptr_array_new_with_free_func(ns_net_log_entry_free);
    if (ns_net_log->len >= NS_NET_LOG_CAP)
        g_ptr_array_remove_index(ns_net_log, 0);
    g_ptr_array_add(ns_net_log, e);
    g_mutex_unlock(&ns_net_log_lock);
}

void
ns_net_log_clear(void)
{
    g_mutex_lock(&ns_net_log_lock);
    if (ns_net_log)
        g_ptr_array_set_size(ns_net_log, 0);
    g_mutex_unlock(&ns_net_log_lock);
}

static void
ns_net_log_append_headers(GString *out, const char *headers)
{
    if (!headers || !*headers)
        return;
    char **lines = g_strsplit(headers, "\n", -1);
    for (guint i = 0; lines && lines[i]; i++) {
        char *line = g_strchomp(lines[i]);
        if (*line)
            g_string_append_printf(out, "    %s\n", line);
    }
    g_strfreev(lines);
}

char *
ns_net_log_dump(void)
{
    GString *out = g_string_new(NULL);
    g_mutex_lock(&ns_net_log_lock);
    guint n = ns_net_log ? ns_net_log->len : 0;
    g_string_append_printf(out, "%u network request%s\n\n", n,
                           n == 1 ? "" : "s");
    for (guint i = 0; i < n; i++) {
        ns_net_log_entry *e = g_ptr_array_index(ns_net_log, i);
        if (e->status > 0)
            g_string_append_printf(out, "[%ld] %s %s\n", e->status,
                                   e->method, e->url);
        else
            g_string_append_printf(out, "[---] %s %s\n", e->method, e->url);
        g_string_append_printf(out, "    %.0f ms, %llu bytes",
                               e->duration_ms,
                               (unsigned long long)e->body_len);
        if (e->content_type && *e->content_type)
            g_string_append_printf(out, ", %s", e->content_type);
        g_string_append_c(out, '\n');
        if (e->error)
            g_string_append_printf(out, "    error: %s\n", e->error);
        if (e->req_headers && *e->req_headers) {
            g_string_append(out, "  Request headers:\n");
            ns_net_log_append_headers(out, e->req_headers);
        }
        if (e->resp_headers && *e->resp_headers) {
            g_string_append(out, "  Response headers:\n");
            ns_net_log_append_headers(out, e->resp_headers);
        }
        g_string_append_c(out, '\n');
    }
    g_mutex_unlock(&ns_net_log_lock);
    return g_string_free(out, FALSE);
}

static char *
ns_net_slist_serialize(struct curl_slist *list)
{
    if (!list)
        return NULL;
    GString *out = g_string_new(NULL);
    for (struct curl_slist *n = list; n; n = n->next) {
        if (!n->data)
            continue;
        if (g_ascii_strncasecmp(n->data, "X-ND-", 5) == 0)
            continue;
        g_string_append(out, n->data);
        g_string_append_c(out, '\n');
    }
    return g_string_free(out, FALSE);
}

#define NS_NET_RESPONSE_MIN_BUDGET (64ULL * 1024ULL * 1024ULL)
#define NS_NET_RESPONSE_RECHECK_BYTES (16ULL * 1024ULL * 1024ULL)
#define NS_NET_MAX_RAW_HEADER_BYTES   (1ULL * 1024ULL * 1024ULL)

static guint64
ns_net_available_memory_bytes(void)
{
#if defined(G_OS_WIN32)
    MEMORYSTATUSEX m = { .dwLength = sizeof(m) };
    if (GlobalMemoryStatusEx(&m))
        return (guint64)m.ullAvailPhys;
#elif defined(__linux__)
    FILE *f = fopen("/proc/meminfo", "re");
    if (f) {
        char line[256];
        guint64 kb = 0;
        while (fgets(line, sizeof(line), f)) {
            if (sscanf(line, "MemAvailable: %" G_GUINT64_FORMAT " kB", &kb) == 1) {
                fclose(f);
                return kb * 1024ULL;
            }
        }
        fclose(f);
    }
#elif defined(__APPLE__)
    uint64_t mem = 0;
    size_t len = sizeof mem;
    if (sysctlbyname("hw.memsize", &mem, &len, NULL, 0) == 0 && mem > 0)
        return (guint64)mem;
#elif defined(_SC_AVPHYS_PAGES) && defined(_SC_PAGESIZE)
    long pages = sysconf(_SC_AVPHYS_PAGES);
    long psize = sysconf(_SC_PAGESIZE);
    if (pages > 0 && psize > 0)
        return (guint64)pages * (guint64)psize;
#endif
    return 0;
}

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

guint64
ns_net_response_budget(void)
{
    guint64 avail = ns_net_available_memory_bytes();
    if (avail == 0) return NS_NET_RESPONSE_MIN_BUDGET;
    guint64 half = avail / 2;
    return half < NS_NET_RESPONSE_MIN_BUDGET ? NS_NET_RESPONSE_MIN_BUDGET : half;
}

static size_t
ns_write_cb(char *data, size_t size, size_t nmemb, void *userdata)
{
    ns_write_ctx *ctx = userdata;
    if (size != 0 && nmemb > G_MAXSIZE / size)
        return 0;
    size_t bytes = size * nmemb;

    if (bytes == 0)
        return 0;
    if (bytes > G_MAXUINT)
        return 0;
    if (ctx->total >= ctx->next_recheck) {
        ctx->budget = ns_net_response_budget();
        ctx->next_recheck = ctx->total + NS_NET_RESPONSE_RECHECK_BYTES;
    }
    if (ctx->total + bytes > ctx->budget) {
        ctx->exceeded = TRUE;
        return 0;
    }
    if (ctx->total + bytes > G_MAXUINT) {
        ctx->exceeded = TRUE;
        return 0;
    }
    g_byte_array_append(ctx->body, (const guint8 *)data, bytes);
    ctx->total += bytes;
    return bytes;
}

void
ns_body_sink_init(ns_write_ctx *ctx, GByteArray *body)
{
    ctx->body = body;
    ctx->total = 0;
    ctx->budget = ns_net_response_budget();
    ctx->next_recheck = NS_NET_RESPONSE_RECHECK_BYTES;
    ctx->exceeded = FALSE;
}

gboolean
ns_body_sink_write(ns_write_ctx *ctx, const void *data, size_t len)
{
    if (len == 0)
        return TRUE;
    return ns_write_cb((char *)data, 1, len, ctx) == len;
}

static char *
header_value_dup(const char *line, size_t bytes, size_t prefix_len)
{
    const char *v = line + prefix_len;
    size_t vlen = bytes - prefix_len;
    while (vlen > 0 && (*v == ' ' || *v == '\t')) { v++; vlen--; }
    while (vlen > 0 &&
           (v[vlen - 1] == '\r' || v[vlen - 1] == '\n' ||
            v[vlen - 1] == ' '  || v[vlen - 1] == '\t')) vlen--;
    return g_strndup(v, vlen);
}

static gboolean
header_capture(const char *buffer, size_t bytes,
               const char *name, char **slot)
{
    size_t name_len = strlen(name);
    if (bytes < name_len ||
        g_ascii_strncasecmp(buffer, name, name_len) != 0)
        return FALSE;
    if (slot) {
        g_free(*slot);
        *slot = header_value_dup(buffer, bytes, name_len);
    }
    return TRUE;
}

static gboolean
header_append(const char *buffer, size_t bytes,
              const char *name, char **slot)
{
    size_t name_len = strlen(name);
    if (bytes < name_len ||
        g_ascii_strncasecmp(buffer, name, name_len) != 0)
        return FALSE;
    if (slot) {
        char *val = header_value_dup(buffer, bytes, name_len);
        if (*slot && **slot && val && *val) {
            char *joined = g_strconcat(*slot, ", ", val, NULL);
            g_free(*slot);
            g_free(val);
            *slot = joined;
        } else if (val && *val) {
            g_free(*slot);
            *slot = val;
        } else {
            g_free(val);
        }
    }
    return TRUE;
}

static size_t
ns_header_cb(char *buffer, size_t size, size_t nitems, void *userdata)
{
    ns_header_ctx *hc = userdata;
    if (size != 0 && nitems > G_MAXSIZE / size)
        return 0;
    size_t bytes = size * nitems;

    if (bytes >= 5 && g_ascii_strncasecmp(buffer, "HTTP/", 5) == 0) {
        if (hc->raw) g_string_set_size(hc->raw, 0);
    } else if (bytes > 2) {
        gboolean set_cookie =
            (bytes >= 11 && g_ascii_strncasecmp(buffer, "Set-Cookie:", 11) == 0) ||
            (bytes >= 12 && g_ascii_strncasecmp(buffer, "Set-Cookie2:", 12) == 0);
        if (!set_cookie) {
            if (!hc->raw) hc->raw = g_string_new(NULL);
            if (hc->raw->len + bytes <= NS_NET_MAX_RAW_HEADER_BYTES)
                g_string_append_len(hc->raw, buffer, bytes);
        }
    }

    if      (header_capture(buffer, bytes, "Content-Type:",    hc->content_type_out))         {}
    else if (header_capture(buffer, bytes, "ETag:",            &hc->etag))                    {}
    else if (header_capture(buffer, bytes, "Last-Modified:",   &hc->last_modified))           {}
    else if (header_capture(buffer, bytes, "Cache-Control:",   &hc->cache_control))           {}
    else if (header_capture(buffer, bytes, "Vary:",            &hc->vary))                    {}
    else if (header_capture(buffer, bytes, "Expires:",         &hc->expires))                 {}
    else if (header_append(buffer, bytes, "Content-Security-Policy:",
                            hc->csp_out))                                                     {}
    else if (header_capture(buffer, bytes, "X-Frame-Options:", hc->xframe_options_out))       {}
    else if (header_capture(buffer, bytes, "X-Content-Type-Options:",
                            hc->x_content_type_options_out))                                  {}
    else if (header_capture(buffer, bytes, "Access-Control-Allow-Origin:",
                            hc->cors_allow_origin_out))                                       {}
    else if (header_capture(buffer, bytes, "Content-Disposition:",
                            hc->content_disposition_out))                                     {}
    else if (header_capture(buffer, bytes, "Content-Language:", hc->content_language_out))      {}
    else if (header_capture(buffer, bytes, "Refresh:", hc->refresh_out))                       {}
    else if (header_capture(buffer, bytes, "Location:", &hc->location))                        {}
    else if (header_capture(buffer, bytes, "Set-Cookie:", NULL))
        hc->set_cookie_seen = TRUE;

    return bytes;
}

void
ns_header_sink_feed(ns_header_ctx *ctx, const char *line, size_t len)
{
    if (line && len)
        ns_header_cb((char *)line, 1, len, ctx);
}

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
    if (g_share) curl_easy_setopt(curl, CURLOPT_SHARE, g_share);

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
        .accept_encoding = g_accept_encoding ? g_accept_encoding : "",
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
    if (g_log_fetches)
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
            if (g_atomic_int_get(&g_net_aborting) ||
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
        if (g_log_fetches)
            ns_debug_log_emit(NS_DLOG_NET, "fetch", "failed %s: %s",
                              ctx->url, err ? err->message : "unknown error");
        g_task_return_error(task, err);
    } else if (g_log_fetches) {
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
    CURL *curl = g_atomic_int_get(&g_net_aborting) ? NULL : curl_easy_init();
    if (curl) {
        curl_easy_setopt(curl, CURLOPT_URL, url);
        curl_easy_setopt(curl, CURLOPT_CONNECT_ONLY, 1L);
        if (g_share) curl_easy_setopt(curl, CURLOPT_SHARE, g_share);
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

char *
ns_multipart_boundary(void)
{
    guint32 r[4];
    if (!ns_security_csprng_fill(r, sizeof r)) {
        r[0] = g_random_int(); r[1] = g_random_int();
        r[2] = g_random_int(); r[3] = g_random_int();
    }
    return g_strdup_printf("----SouthstarFormBoundary%08x%08x%08x%08x",
                           r[0], r[1], r[2], r[3]);
}

void
ns_multipart_quote_field(GString *out, const char *s)
{
    if (!out || !s) return;
    for (const char *p = s; *p; p++) {
        unsigned char c = (unsigned char)*p;
        if      (c == '"')  g_string_append(out, "%22");
        else if (c == '\r') g_string_append(out, "%0D");
        else if (c == '\n') g_string_append(out, "%0A");
        else                g_string_append_c(out, (char)c);
    }
}

static char *g_form_submission_charset;

void
ns_form_set_submission_charset(const char *charset)
{
    g_free(g_form_submission_charset);
    g_form_submission_charset = NULL;
    if (!charset || !*charset) return;
    char *first = g_strdup(charset);
    g_strstrip(first);
    for (char *p = first; *p; p++)
        if (*p == ' ' || *p == ',' || *p == '\t') { *p = '\0'; break; }
    if (*first && g_ascii_strcasecmp(first, "UTF-8") != 0 &&
        g_ascii_strcasecmp(first, "UTF8") != 0 &&
        g_ascii_strcasecmp(first, "UTF-16LE") != 0 &&
        g_ascii_strcasecmp(first, "UTF-16BE") != 0)
        g_form_submission_charset = first;
    else
        g_free(first);
}

void
ns_form_urlencoded_append(GString *out, const char *s)
{
    if (!out || !s) return;
    char *converted = NULL;
    if (g_form_submission_charset) {
        converted = g_convert(s, -1, g_form_submission_charset, "UTF-8",
                              NULL, NULL, NULL);
        if (converted) s = converted;
    }
    for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
        unsigned char c = *p;
        if (g_ascii_isalnum(c) || c == '*' || c == '-' || c == '.' || c == '_')
            g_string_append_c(out, (char)c);
        else if (c == ' ')
            g_string_append_c(out, '+');
        else
            g_string_append_printf(out, "%%%02X", c);
    }
    g_free(converted);
}

void
ns_form_urlencoded_append_pair(GString *out, gboolean *first,
                               const char *name, const char *value)
{
    if (!out || !first || !name) return;
    if (!*first) g_string_append_c(out, '&');
    *first = FALSE;
    ns_form_urlencoded_append(out, name);
    g_string_append_c(out, '=');
    ns_form_urlencoded_append(out, value);
}
