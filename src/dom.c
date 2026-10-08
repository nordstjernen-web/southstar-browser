/* Southstar — DOM data structure.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#include "dom.h"

#include "css.h"

#include <string.h>

static void ns_class_set_clear(ns_node *el);

static gboolean
ns_str_is_ascii_lower(const char *s)
{
    for (; *s; s++)
        if (*s >= 'A' && *s <= 'Z') return FALSE;
    return TRUE;
}

static ns_node *
ns_node_new(ns_node_kind kind)
{
    ns_node *n = g_new0(ns_node, 1);
    n->kind = kind;
    return n;
}

ns_node *
ns_node_new_document(void)
{
    return ns_node_new(NS_NODE_DOCUMENT);
}

ns_node *
ns_node_new_element(char *name)
{
    ns_node *n = ns_node_new(NS_NODE_ELEMENT);
    n->name = name;
    n->flags |= NS_NODE_OWN_NAME;
    return n;
}

ns_node *
ns_node_new_text_len(char *text, guint32 len)
{
    ns_node *n = ns_node_new(NS_NODE_TEXT);
    n->text = text;
    n->text_len = len;
    n->flags |= NS_NODE_OWN_TEXT;
    return n;
}

ns_node *
ns_node_new_text(char *text)
{
    return ns_node_new_text_len(text, text ? (guint32)strlen(text) : 0);
}

ns_node *
ns_node_new_comment_len(char *text, guint32 len)
{
    ns_node *n = ns_node_new(NS_NODE_COMMENT);
    n->text = text;
    n->text_len = len;
    n->flags |= NS_NODE_OWN_TEXT;
    return n;
}

ns_node *
ns_node_new_comment(char *text)
{
    return ns_node_new_comment_len(text, text ? (guint32)strlen(text) : 0);
}

void
ns_node_set_name_borrow(ns_node *n, const char *name)
{
    if (!n) return;
    if (n->flags & NS_NODE_OWN_NAME)
        g_free(n->name);
    n->name = (char *)name;
    n->flags &= ~NS_NODE_OWN_NAME;
}

void
ns_node_set_name_owned(ns_node *n, char *name)
{
    if (!n) {
        g_free(name);
        return;
    }
    if (n->flags & NS_NODE_OWN_NAME)
        g_free(n->name);
    n->name = name;
    n->flags |= NS_NODE_OWN_NAME;
}

void
ns_node_add_flags(ns_node *n, guint32 flags)
{
    if (n) n->flags |= flags;
}

void
ns_node_mark_doctype(ns_node *n)
{
    if (n) n->kind = NS_NODE_DOCTYPE;
}

void
ns_node_set_text_borrow(ns_node *n, const char *text)
{
    if (!n) return;
    if (n->flags & NS_NODE_OWN_TEXT)
        g_free(n->text);
    n->text = (char *)text;
    n->text_len = text ? (guint32)strlen(text) : 0;
    n->flags &= ~NS_NODE_OWN_TEXT;
}

void
ns_node_replace_text_len_owned(ns_node *n, char *text, guint32 len)
{
    if (!n) {
        g_free(text);
        return;
    }
    if (n->flags & NS_NODE_OWN_TEXT)
        g_free(n->text);
    n->text = text;
    n->text_len = len;
    n->flags |= NS_NODE_OWN_TEXT;
}

void
ns_node_replace_text_owned(ns_node *n, char *text)
{
    ns_node_replace_text_len_owned(n, text, text ? (guint32)strlen(text) : 0);
}

static void
ns_node_own_strings_one(ns_node *n)
{
    ns_class_set_clear(n);
    if (n->name && !(n->flags & NS_NODE_OWN_NAME)) {
        n->name = g_strdup(n->name);
        n->flags |= NS_NODE_OWN_NAME;
    }
    if (n->text && !(n->flags & NS_NODE_OWN_TEXT)) {
        n->text = g_strdup(n->text);
        n->flags |= NS_NODE_OWN_TEXT;
    }
    for (ns_attr *a = n->attrs; a; a = a->next) {
        if (a->name && !(a->flags & NS_ATTR_OWN_NAME)) {
            a->name = g_strdup(a->name);
            a->flags |= NS_ATTR_OWN_NAME;
        }
        if (a->value && !(a->flags & NS_ATTR_OWN_VALUE)) {
            a->value = ns_value_dup_len(a->value, a->value_len);
            a->flags |= NS_ATTR_OWN_VALUE;
        }
    }
}

void
ns_node_own_strings_deep(ns_node *n)
{
    if (!n) return;
    GPtrArray *stack = g_ptr_array_new();
    g_ptr_array_add(stack, n);
    while (stack->len > 0) {
        ns_node *cur = g_ptr_array_index(stack, stack->len - 1);
        g_ptr_array_set_size(stack, stack->len - 1);
        ns_node_own_strings_one(cur);
        if (cur->tpl_content)
            g_ptr_array_add(stack, cur->tpl_content);
        for (ns_node *c = cur->first_child; c; c = c->next_sibling)
            g_ptr_array_add(stack, c);
    }
    g_ptr_array_free(stack, TRUE);
}

void
ns_element_append_attr_borrow(ns_node *el, const char *name, const char *value)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !name) return;
    if (el->class_set) ns_class_set_clear(el);
    el->attr_bloom = 0;
    el->attr_gen++;
    ns_attr *a = g_new0(ns_attr, 1);
    a->name  = (char *)name;
    a->value = (char *)(value ? value : "");
    a->value_len = value ? (guint)strlen(value) : 0;
    a->flags = ns_str_is_ascii_lower(name) ? NS_ATTR_NAME_LOWER : 0;
    ns_attr *tail = NULL;
    for (ns_attr *cur = el->attrs; cur; cur = cur->next) tail = cur;
    if (tail) tail->next = a;
    else      el->attrs = a;
}

void
ns_node_attach_backing(ns_node *root, void *backing, void (*destroy)(void *))
{
    if (!root) {
        if (backing && destroy) destroy(backing);
        return;
    }
    if (root->backing && root->backing_free)
        root->backing_free(root->backing);
    root->backing = backing;
    root->backing_free = destroy;
}

static void
ns_attr_free_one(ns_attr *a)
{
    if (!a) return;
    if (a->flags & NS_ATTR_OWN_NAME)  g_free(a->name);
    if (a->flags & NS_ATTR_OWN_VALUE) g_free(a->value);
    g_free(a->namespace_uri);
    g_free(a->prefix);
    g_free(a->local_name);
    g_free(a);
}

static void
ns_attr_free(ns_attr *a)
{
    while (a) {
        ns_attr *next = a->next;
        ns_attr_free_one(a);
        a = next;
    }
}

typedef struct ns_class_set {
    guint n;
    struct { const char *p; guint len; } tok[];
} ns_class_set;

static ns_class_set g_nd_empty_class_set;

static inline gboolean
ns_clsset_ws(char c)
{
    return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f';
}

static ns_class_set *
ns_class_set_build(const char *cls)
{
    guint n = 0;
    for (const char *s = cls; *s; ) {
        while (*s && ns_clsset_ws(*s)) s++;
        if (!*s) break;
        while (*s && !ns_clsset_ws(*s)) s++;
        n++;
    }
    if (n == 0) return &g_nd_empty_class_set;
    ns_class_set *cs = g_malloc(sizeof *cs + (gsize)n * sizeof cs->tok[0]);
    cs->n = 0;
    for (const char *s = cls; *s; ) {
        while (*s && ns_clsset_ws(*s)) s++;
        if (!*s) break;
        const char *t = s;
        while (*s && !ns_clsset_ws(*s)) s++;
        cs->tok[cs->n].p = t;
        cs->tok[cs->n].len = (guint)(s - t);
        cs->n++;
    }
    return cs;
}

static void
ns_class_set_clear(ns_node *el)
{
    if (el->class_set && el->class_set != &g_nd_empty_class_set)
        g_free(el->class_set);
    el->class_set = NULL;
}

gboolean
ns_node_has_class(const ns_node *el, const char *name, gsize len)
{
    if (!el || el->kind != NS_NODE_ELEMENT) return FALSE;
    ns_class_set *cs = el->class_set;
    if (!cs) {
        const char *cls = ns_element_get_attr(el, "class");
        cs = (cls && *cls) ? ns_class_set_build(cls) : &g_nd_empty_class_set;
        ((ns_node *)el)->class_set = cs;
    }
    for (guint i = 0; i < cs->n; i++)
        if (cs->tok[i].len == len && memcmp(cs->tok[i].p, name, len) == 0)
            return TRUE;
    return FALSE;
}

void
ns_node_free(ns_node *node)
{
    if (!node)
        return;

    GPtrArray *stack = g_ptr_array_new();
    g_ptr_array_add(stack, node);

    while (stack->len > 0) {
        ns_node *cur = g_ptr_array_index(stack, stack->len - 1);
        if (cur->tpl_content) {
            ns_node *tc = cur->tpl_content;
            cur->tpl_content = NULL;
            g_ptr_array_add(stack, tc);
            continue;
        }
        if (cur->first_child) {
            ns_node *c = cur->first_child;
            cur->first_child = NULL;
            while (c) {
                ns_node *next = c->next_sibling;
                c->next_sibling = NULL;
                c->parent = NULL;
                g_ptr_array_add(stack, c);
                c = next;
            }
            continue;
        }
        g_ptr_array_set_size(stack, stack->len - 1);
        if (cur->js_invalidate)
            cur->js_invalidate(cur);
        ns_css_forget_node(cur);
        if (cur->flags & NS_NODE_OWN_NAME) g_free(cur->name);
        if (cur->flags & NS_NODE_OWN_TEXT) g_free(cur->text);
        ns_class_set_clear(cur);
        ns_attr_free(cur->attrs);
        if (cur->backing && cur->backing_free)
            cur->backing_free(cur->backing);
        if (cur->id_index) {
            g_hash_table_destroy(cur->id_index);
            cur->id_index = NULL;
        }
        if (cur->class_index) {
            g_hash_table_destroy(cur->class_index);
            cur->class_index = NULL;
        }
        if (cur->tag_index) {
            g_hash_table_destroy(cur->tag_index);
            cur->tag_index = NULL;
        }
        g_free(cur);
    }
    g_ptr_array_free(stack, TRUE);
}

static void
ns_node_detach(ns_node *child)
{
    ns_node *p = child->parent;
    if (!p)
        return;
    if (child->prev_sibling)
        child->prev_sibling->next_sibling = child->next_sibling;
    else
        p->first_child = child->next_sibling;
    if (child->next_sibling)
        child->next_sibling->prev_sibling = child->prev_sibling;
    else
        p->last_child = child->prev_sibling;
    child->parent = NULL;
    child->prev_sibling = NULL;
    child->next_sibling = NULL;
}

void
ns_node_append_child(ns_node *parent, ns_node *child)
{
    g_return_if_fail(parent != NULL);
    g_return_if_fail(child != NULL);

    ns_node_detach(child);
    child->parent = parent;
    child->prev_sibling = parent->last_child;
    if (parent->last_child)
        parent->last_child->next_sibling = child;
    else
        parent->first_child = child;
    parent->last_child = child;
}

void
ns_node_remove(ns_node *child)
{
    ns_node_detach(child);
}

ns_node *
ns_node_next_in_subtree(const ns_node *node, const ns_node *root,
                        gboolean descend)
{
    if (descend && node->first_child)
        return node->first_child;
    for (const ns_node *n = node; n && n != root; n = n->parent)
        if (n->next_sibling)
            return n->next_sibling;
    return NULL;
}

gboolean
ns_attr_name_is_internal(const char *name)
{
    return name && g_ascii_strncasecmp(name, "data-nd-", 8) == 0;
}

const char *
ns_attr_local_name(const ns_attr *attr)
{
    if (!attr) return "";
    return attr->local_name ? attr->local_name : (attr->name ? attr->name : "");
}

static const char *
ns_attr_normalize_namespace(const char *namespace_uri)
{
    return (namespace_uri && *namespace_uri) ? namespace_uri : NULL;
}

static gboolean
ns_attr_namespace_equal(const char *a, const char *b)
{
    a = ns_attr_normalize_namespace(a);
    b = ns_attr_normalize_namespace(b);
    if (!a || !b) return a == b;
    return strcmp(a, b) == 0;
}

static gboolean
ns_attr_matches_ns(const ns_attr *attr, const char *namespace_uri,
                   const char *local_name)
{
    if (!attr || !local_name) return FALSE;
    return ns_attr_namespace_equal(attr->namespace_uri, namespace_uri) &&
           strcmp(ns_attr_local_name(attr), local_name) == 0;
}

char *
ns_value_dup_len(const char *value, gsize len)
{
    char *v = g_malloc(len + 1);
    if (len && value) memcpy(v, value, len);
    v[len] = '\0';
    return v;
}

void
ns_element_set_attr_len(ns_node *el, const char *name,
                        const char *value, gssize len)
{
    g_return_if_fail(el != NULL);
    g_return_if_fail(el->kind == NS_NODE_ELEMENT);
    g_return_if_fail(name != NULL);

    gsize vlen = len < 0 ? (value ? strlen(value) : 0) : (gsize)len;
    if (el->class_set && g_ascii_strcasecmp(name, "class") == 0)
        ns_class_set_clear(el);
    el->attr_bloom = 0;
    el->attr_gen++;

    ns_attr *tail = NULL;
    for (ns_attr *a = el->attrs; a; a = a->next) {
        if (g_ascii_strcasecmp(a->name, name) == 0) {
            if (a->flags & NS_ATTR_OWN_VALUE) g_free(a->value);
            a->value = ns_value_dup_len(value, vlen);
            a->value_len = (guint)vlen;
            a->flags |= NS_ATTR_OWN_VALUE;
            return;
        }
        tail = a;
    }
    ns_attr *a = g_new0(ns_attr, 1);
    a->name = g_strdup(name);
    a->value = ns_value_dup_len(value, vlen);
    a->value_len = (guint)vlen;
    a->flags = NS_ATTR_OWN_NAME | NS_ATTR_OWN_VALUE |
               (ns_str_is_ascii_lower(name) ? NS_ATTR_NAME_LOWER : 0);
    a->next = NULL;
    if (tail) tail->next = a;
    else      el->attrs = a;
}

void
ns_element_set_attr(ns_node *el, const char *name, const char *value)
{
    ns_element_set_attr_len(el, name, value, -1);
}

void
ns_element_set_attr_ns(ns_node *el, const char *namespace_uri,
                       const char *prefix, const char *local_name,
                       const char *name, const char *value)
{
    g_return_if_fail(el != NULL);
    g_return_if_fail(el->kind == NS_NODE_ELEMENT);
    g_return_if_fail(local_name != NULL);

    const char *ns = ns_attr_normalize_namespace(namespace_uri);
    const char *pfx = prefix && *prefix ? prefix : NULL;
    const char *qualified = name && *name ? name : local_name;

    if (el->class_set && (g_ascii_strcasecmp(local_name, "class") == 0 ||
                          g_ascii_strcasecmp(qualified, "class") == 0))
        ns_class_set_clear(el);
    el->attr_bloom = 0;
    el->attr_gen++;

    gsize vlen = value ? strlen(value) : 0;
    ns_attr *tail = NULL;
    for (ns_attr *a = el->attrs; a; a = a->next) {
        if (ns_attr_matches_ns(a, ns, local_name)) {
            if (a->flags & NS_ATTR_OWN_VALUE) g_free(a->value);
            a->value = ns_value_dup_len(value, vlen);
            a->value_len = (guint)vlen;
            a->flags |= NS_ATTR_OWN_VALUE;
            return;
        }
        tail = a;
    }

    ns_attr *a = g_new0(ns_attr, 1);
    a->name = g_strdup(qualified);
    a->value = ns_value_dup_len(value, vlen);
    a->value_len = (guint)vlen;
    a->namespace_uri = ns ? g_strdup(ns) : NULL;
    a->prefix = pfx ? g_strdup(pfx) : NULL;
    a->local_name = g_strdup(local_name);
    a->flags = NS_ATTR_OWN_NAME | NS_ATTR_OWN_VALUE |
               (ns_str_is_ascii_lower(qualified) ? NS_ATTR_NAME_LOWER : 0);
    if (tail) tail->next = a;
    else      el->attrs = a;
}

void
ns_element_remove_attr(ns_node *el, const char *name)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !name) return;
    if (el->class_set && g_ascii_strcasecmp(name, "class") == 0)
        ns_class_set_clear(el);
    el->attr_bloom = 0;
    el->attr_gen++;
    ns_attr **link = &el->attrs;
    while (*link) {
        if (g_ascii_strcasecmp((*link)->name, name) == 0) {
            ns_attr *dead = *link;
            *link = dead->next;
            ns_attr_free_one(dead);
            return;
        }
        link = &(*link)->next;
    }
}

void
ns_element_remove_attr_ns(ns_node *el, const char *namespace_uri,
                          const char *local_name)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !local_name) return;
    const char *ns = ns_attr_normalize_namespace(namespace_uri);
    if (el->class_set && g_ascii_strcasecmp(local_name, "class") == 0)
        ns_class_set_clear(el);
    el->attr_bloom = 0;
    el->attr_gen++;
    ns_attr **link = &el->attrs;
    while (*link) {
        if (ns_attr_matches_ns(*link, ns, local_name)) {
            ns_attr *dead = *link;
            *link = dead->next;
            ns_attr_free_one(dead);
            return;
        }
        link = &(*link)->next;
    }
}

#define NS_DOM_MAX_DEPTH 512

static ns_node *
ns_node_clone_depth(const ns_node *src, gboolean deep, int depth)
{
    if (!src || depth >= NS_DOM_MAX_DEPTH) return NULL;
    ns_node *out = NULL;
    switch (src->kind) {
    case NS_NODE_ELEMENT:
        out = ns_node_new_element(src->name ? g_strdup(src->name) : g_strdup(""));
        out->flags |= src->flags & (NS_NODE_SVG_NS | NS_NODE_FOREIGN_NS |
                                    NS_NODE_KEEP_CASE |
                                    NS_NODE_INPUT_INDETERMINATE);
        for (const ns_attr *a = src->attrs; a; a = a->next)
            ns_element_set_attr_ns(out, a->namespace_uri, a->prefix,
                                   ns_attr_local_name(a), a->name,
                                   a->value ? a->value : "");
        break;
    case NS_NODE_TEXT:
        out = ns_node_new_text_len(src->text ? g_memdup2(src->text, src->text_len + 1) : g_strdup(""),
                                  src->text_len);
        break;
    case NS_NODE_DOCTYPE:
        out = ns_node_new_element(src->name ? g_strdup(src->name) : g_strdup(""));
        for (const ns_attr *a = src->attrs; a; a = a->next)
            ns_element_set_attr_ns(out, a->namespace_uri, a->prefix,
                                   ns_attr_local_name(a), a->name,
                                   a->value ? a->value : "");
        out->kind = NS_NODE_DOCTYPE;
        break;
    case NS_NODE_DOCUMENT:
    case NS_NODE_COMMENT:
        out = ns_node_new(src->kind);
        if (src->text) {
            out->text = g_memdup2(src->text, src->text_len + 1);
            out->text_len = src->text_len;
            out->flags |= NS_NODE_OWN_TEXT;
        }
        if (src->name) {
            out->name = g_strdup(src->name);
            out->flags |= NS_NODE_OWN_NAME;
        }
        for (const ns_attr *a = src->attrs; a; a = a->next)
            ns_element_set_attr_ns(out, a->namespace_uri, a->prefix,
                                   ns_attr_local_name(a), a->name,
                                   a->value ? a->value : "");
        break;
    }
    if (out) out->flags |= src->flags & (NS_NODE_FRAGMENT | NS_NODE_CDATA |
                                         NS_NODE_PI);
    if (deep && out) {
        for (const ns_node *c = src->first_child; c; c = c->next_sibling) {
            if (ns_node_is_embedded_doc(c)) continue;
            ns_node *cc = ns_node_clone_depth(c, TRUE, depth + 1);
            if (cc) ns_node_append_child(out, cc);
        }
        if (src->tpl_content)
            out->tpl_content = ns_node_clone_depth(src->tpl_content, TRUE,
                                                   depth + 1);
    }
    return out;
}

ns_node *
ns_node_clone(const ns_node *src, gboolean deep)
{
    return ns_node_clone_depth(src, deep, 0);
}

ns_node *
ns_template_content_get(ns_node *tpl)
{
    if (!tpl) return NULL;
    if (!tpl->tpl_content) {
        ns_node *frag = ns_node_new_document();
        if (!frag) return NULL;
        frag->flags |= NS_NODE_FRAGMENT | NS_NODE_TEMPLATE_CONTENT;
        tpl->tpl_content = frag;
    }
    return tpl->tpl_content;
}

guint64
ns_attr_name_bloom_bit(const char *name)
{
    guint32 h = 2166136261u;
    for (const unsigned char *p = (const unsigned char *)name; *p; p++) {
        h ^= *p;
        h *= 16777619u;
    }
    return (guint64)1 << (h & 63);
}

guint64
ns_node_attr_bloom(const ns_node *el)
{
    if (el->attr_bloom) return el->attr_bloom;
    guint64 b = 0;
    for (const ns_attr *a = el->attrs; a; a = a->next)
        if (a->name) b |= ns_attr_name_bloom_bit(a->name);
    ((ns_node *)el)->attr_bloom = b;
    return b;
}

const ns_attr *
ns_element_find_attr(const ns_node *el, const char *name)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !name)
        return NULL;
    if (el->flags & NS_NODE_SVG_NS) {
        for (const ns_attr *a = el->attrs; a; a = a->next)
            if (a->name && strcmp(a->name, name) == 0)
                return a;
        return NULL;
    }
    if (ns_str_is_ascii_lower(name)) {
        char c0 = name[0];
        for (const ns_attr *a = el->attrs; a; a = a->next) {
            const char *an = a->name;
            if (!an) continue;
            if (a->flags & NS_ATTR_NAME_LOWER) {
                if (an[0] == c0 && strcmp(an, name) == 0) return a;
            } else if (g_ascii_strcasecmp(an, name) == 0) {
                return a;
            }
        }
        return NULL;
    }
    for (const ns_attr *a = el->attrs; a; a = a->next) {
        if (g_ascii_strcasecmp(a->name, name) == 0)
            return a;
    }
    return NULL;
}

const char *
ns_element_get_attr(const ns_node *el, const char *name)
{
    const ns_attr *a = ns_element_find_attr(el, name);
    return a ? a->value : NULL;
}

const char *
ns_element_get_attr_len(const ns_node *el, const char *name, gsize *out_len)
{
    const ns_attr *a = ns_element_find_attr(el, name);
    if (!a) { if (out_len) *out_len = 0; return NULL; }
    if (out_len) *out_len = a->value_len;
    return a->value;
}

const ns_attr *
ns_element_find_attr_ns(const ns_node *el, const char *namespace_uri,
                        const char *local_name)
{
    if (!el || el->kind != NS_NODE_ELEMENT || !local_name)
        return NULL;
    const char *ns = ns_attr_normalize_namespace(namespace_uri);
    for (const ns_attr *a = el->attrs; a; a = a->next)
        if (ns_attr_matches_ns(a, ns, local_name))
            return a;
    return NULL;
}
