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

typedef struct ns_hostobj {
    ns_ho_kind kind;
    JSValue    state;
    gpointer   native;
    void     (*free_native)(JSRuntime *rt, gpointer native);
} ns_hostobj;
static ns_hostobj *ns_ho_of(JSValueConst v, ns_ho_kind kind);

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

gboolean
ns_js_value_is_form_data(JSContext *ctx, JSValueConst v)
{
    (void)ctx;
    return ns_ho_of(v, NS_HO_FORM_DATA) != NULL;
}

static gboolean
ns_node_name_is_any_of(const ns_node *n, const char *const *tags)
{
    if (!n || n->kind != NS_NODE_ELEMENT || !n->name) return FALSE;
    for (; *tags; tags++)
        if (g_ascii_strcasecmp(n->name, *tags) == 0) return TRUE;
    return FALSE;
}



static JSClassID ns_element_class_id;

static ns_node *ns_unwrap_element_mut(JSValueConst val);

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

static const JSCFunctionListEntry ns_validity_proto_funcs[] = {
    JS_CGETSET_DEF("valid", ns_validity_get_valid, NULL),
};

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
    ns_js_brand_node_interfaces(fctx, ns_element_class_id, ns_attr_class_id());
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
        ns_attr_register_class(ctx, JS_NewObject(ctx));
        return JS_UNDEFINED;
    case NS_JS_CLASS_STYLE:
        proto = JS_NewObject(ctx);
        ns_cssom_install_style_proto(ctx, proto);
        ns_set_tostring_tag(ctx, proto, "CSSStyleDeclaration");
        ns_cssom_register_style_class(ctx, proto);
        return JS_UNDEFINED;
    case NS_JS_CLASS_TOKEN_LIST:
        proto = JS_NewObject(ctx);
        JS_SetPropertyFunctionList(ctx, proto, ns_tlist_proto_funcs,
                                   G_N_ELEMENTS(ns_tlist_proto_funcs));
        ns_token_list_register_class(ctx, JS_DupValue(ctx, proto));
        return proto;
    case NS_JS_CLASS_LIVE:
        ns_live_register_class(ctx);
        return JS_UNDEFINED;
    case NS_JS_CLASS_DATASET:
        ns_dataset_register_class(ctx, JS_NewObject(ctx));
        return JS_UNDEFINED;
    case NS_JS_CLASS_STORAGE:
        proto = JS_NewObject(ctx);
        ns_storage_install_proto(ctx, proto);
        ns_storage_register_class(ctx, JS_DupValue(ctx, proto));
        return proto;
    case NS_JS_CLASS_WINDOW_NAMED:
        ns_window_named_register_class(ctx);
        return JS_UNDEFINED;
    default:
        return JS_UNDEFINED;
    }
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
    case NS_HOST_PROTO_STYLE:      return ns_style_class_proto(ctx);
    case NS_HOST_PROTO_DATASET:    return ns_dataset_class_proto(ctx);
    case NS_HOST_PROTO_TOKEN_LIST: return ns_token_list_class_proto(ctx);
    default:                       return JS_UNDEFINED;
    }
}

void
ns_js_brand_node_classes(JSContext *ctx)
{
    ns_js_brand_node_interfaces(ctx, ns_element_class_id, ns_attr_class_id());
}

JSValue
ns_js_new_document_object(ns_js *js)
{
    JSValue document = JS_NewObjectClass(js->ctx, ns_element_class_id);
    if (js->current_doc) JS_SetOpaque(document, js->current_doc);
    return document;
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

JSValue
ns_js_net_host_state(JSContext *ctx, JSValueConst v, int kind)
{
    ns_hostobj *d = ns_ho_data(v);
    return d && (int)d->kind == kind ? JS_DupValue(ctx, d->state) : JS_UNDEFINED;
}
