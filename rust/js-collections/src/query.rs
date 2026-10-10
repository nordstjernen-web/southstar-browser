//! Southstar — selector queries: querySelector() and querySelectorAll() with their id, class and tag fast paths, the document-index key lookup and the per-page cache, matches(), closest() and getElementById().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_css::{MatchContext, SelectorList};
use southstar_dom::index::{class_lookup, find_by_id, tag_lookup};
use southstar_dom::serialize::is_embedded_doc;
use southstar_dom::{Kind, MAX_DEPTH, NodeArray};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{
    current_document, current_url_fragment, document_root_for, focused_node, js_of,
    selector_syntax_error, unwrap_node, with_text, wrap_node,
};
use crate::{
    Element, FLAG_FRAGMENT, JsResult, element_children, first_element, has_class, id_is,
    is_ancestor_or_self, is_shadow_root, nodelist, page, walk_elements,
};

struct ListBuilder {
    array: Value,
    length: u32,
}

impl ListBuilder {
    fn new(scope: &mut Scope<'_>) -> ListBuilder {
        ListBuilder {
            array: scope.new_array(),
            length: 0,
        }
    }

    fn push(&mut self, scope: &mut Scope<'_>, node: Element) {
        let wrapper = wrap_node(scope, Some(node));
        let _ = scope.set_index(&self.array, self.length, wrapper);
        self.length += 1;
    }

    fn finish(self, scope: &mut Scope<'_>) -> Value {
        nodelist::finalize(scope, &self.array, self.length);
        self.array
    }
}

fn no_match(scope: &mut Scope<'_>, want_all: bool) -> Value {
    if want_all {
        nodelist::empty(scope)
    } else {
        Value::null()
    }
}

fn ident_only(s: &[u8]) -> bool {
    let Some((&first, rest)) = s.split_first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_' || first == b'-')
        && rest
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn uses_doc_index(root: Element, doc: Element) -> bool {
    for p in southstar_dom::ancestors_and_self(root) {
        if p == doc {
            return true;
        }
        if is_embedded_doc(p) || is_shadow_root(p) {
            return false;
        }
    }
    false
}

fn name_is_ci(node: Element, tag: &[u8]) -> bool {
    node.is_element()
        && node
            .name()
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag))
}

fn first_indexed(
    list: &NodeArray,
    root: Element,
    doc: Element,
    include_self: bool,
    test: impl Fn(Element) -> bool,
) -> Option<Element> {
    (0..list.len()).map(|k| list.get(k)).find(|&n| {
        (root == doc || is_ancestor_or_self(n, root)) && (include_self || n != root) && test(n)
    })
}

fn all_indexed(
    scope: &mut Scope<'_>,
    list: &NodeArray,
    root: Element,
    doc: Element,
    include_self: bool,
) -> Value {
    let mut out = ListBuilder::new(scope);
    for n in (0..list.len()).map(|k| list.get(k)) {
        if root != doc && !is_ancestor_or_self(n, root) {
            continue;
        }
        if !include_self && n == root {
            continue;
        }
        out.push(scope, n);
    }
    out.finish(scope)
}

fn indexed_lookup(
    scope: &mut Scope<'_>,
    list: &NodeArray,
    root: Element,
    doc: Element,
    want_all: bool,
    include_self: bool,
    cache: (u8, &[u8]),
) -> Value {
    if !want_all {
        let hit = first_indexed(list, root, doc, include_self, |_| true);
        return wrap_node(scope, hit);
    }
    let js = js_of(scope);
    let (kind, key) = cache;
    if let Some(cached) = page::qcache_get(js, root.as_ptr() as usize, kind, key) {
        return cached;
    }
    let result = all_indexed(scope, list, root, doc, include_self);
    page::qcache_put(js, root.as_ptr() as usize, kind, key, &result);
    result
}

fn by_id(
    scope: &mut Scope<'_>,
    root: Element,
    id: &CStr,
    want_all: bool,
    include_self: bool,
) -> Value {
    let doc = current_document(js_of(scope));
    let mut hit = match doc.filter(|&doc| doc.id_table().is_some() && uses_doc_index(root, doc)) {
        Some(doc) => find_by_id(doc, id)
            .filter(|&n| is_ancestor_or_self(n, root))
            .or_else(|| find_by_id(root, id)),
        None => find_by_id(root, id),
    };
    if hit == Some(root) && !include_self {
        hit = element_children(root).find_map(|c| find_by_id(c, id));
    }
    if !want_all {
        return wrap_node(scope, hit);
    }
    let id = id.to_bytes();
    let mut out = ListBuilder::new(scope);
    if include_self && id_is(root, id) {
        out.push(scope, root);
    }
    for child in element_children(root) {
        walk_elements(child, 0, false, &mut |n| {
            if id_is(n, id) {
                out.push(scope, n);
            }
        });
    }
    out.finish(scope)
}

fn by_class(
    scope: &mut Scope<'_>,
    root: Element,
    class: &CStr,
    want_all: bool,
    include_self: bool,
) -> Value {
    let doc = current_document(js_of(scope));
    if let Some(doc) = doc.filter(|&doc| doc.class_table().is_some() && uses_doc_index(root, doc)) {
        let found = class_lookup(doc, class, |list| {
            indexed_lookup(
                scope,
                list,
                root,
                doc,
                want_all,
                include_self,
                (b'c', class.to_bytes()),
            )
        });
        return found.unwrap_or_else(|| no_match(scope, want_all));
    }
    let class = class.to_bytes();
    if !want_all {
        let mut matches = |n: Element| has_class(n, class);
        let mut hit = None;
        if include_self {
            hit = first_element(root, 0, false, &mut matches);
        }
        if hit.is_none() {
            hit = element_children(root).find_map(|c| first_element(c, 0, false, &mut matches));
        }
        return wrap_node(scope, hit);
    }
    let mut out = ListBuilder::new(scope);
    if include_self && has_class(root, class) {
        out.push(scope, root);
    }
    for child in element_children(root) {
        walk_elements(child, 0, false, &mut |n| {
            if has_class(n, class) {
                out.push(scope, n);
            }
        });
    }
    out.finish(scope)
}

fn by_tag(
    scope: &mut Scope<'_>,
    root: Element,
    tag: &CStr,
    want_all: bool,
    include_self: bool,
) -> Value {
    let doc = current_document(js_of(scope));
    if let Some(doc) = doc.filter(|&doc| uses_doc_index(root, doc) && doc.tag_table().is_some()) {
        let found = tag_lookup(doc, tag, |list| {
            indexed_lookup(
                scope,
                list,
                root,
                doc,
                want_all,
                include_self,
                (b'q', tag.to_bytes()),
            )
        });
        if let Some(found) = found {
            return found;
        }
    }
    let tag = tag.to_bytes();
    let self_hit = include_self && name_is_ci(root, tag);
    if !want_all {
        let hit = if self_hit {
            Some(root)
        } else {
            element_children(root)
                .find_map(|c| first_element(c, 0, false, &mut |n| name_is_ci(n, tag)))
        };
        return wrap_node(scope, hit);
    }
    let mut out = ListBuilder::new(scope);
    if self_hit {
        out.push(scope, root);
    }
    for child in element_children(root) {
        walk_elements(child, 0, false, &mut |n| {
            if name_is_ci(n, tag) {
                out.push(scope, n);
            }
        });
    }
    out.finish(scope)
}

fn simple(
    scope: &mut Scope<'_>,
    root: Element,
    selector: &CStr,
    want_all: bool,
    include_self: bool,
) -> Option<Value> {
    let bytes = selector.to_bytes();
    let (&first, rest) = bytes.split_first()?;
    let tail = || CStr::from_bytes_with_nul(&selector.to_bytes_with_nul()[1..]).ok();
    if first == b'#' && ident_only(rest) {
        return Some(by_id(scope, root, tail()?, want_all, include_self));
    }
    if first == b'.' && ident_only(rest) {
        return Some(by_class(scope, root, tail()?, want_all, include_self));
    }
    if ident_only(bytes) {
        return Some(by_tag(scope, root, selector, want_all, include_self));
    }
    None
}

fn key_index(
    scope: &mut Scope<'_>,
    root: Element,
    list: &SelectorList,
    want_all: bool,
    include_self: bool,
) -> Option<Value> {
    let selector = list.single()?;
    let doc = current_document(js_of(scope))?;
    if !uses_doc_index(root, doc) {
        return None;
    }
    let key = selector.key()?;
    if let Some(id) = key.id.filter(|_| doc.id_table().is_some()) {
        let Some(single) = find_by_id(doc, id) else {
            return Some(no_match(scope, want_all));
        };
        if want_all
            || !(include_self || single != root)
            || !is_ancestor_or_self(single, root)
            || !selector.matches(single)
        {
            return None;
        }
        return Some(wrap_node(scope, Some(single)));
    }
    if root != doc {
        return None;
    }
    let candidates = |scope: &mut Scope<'_>, list: &NodeArray| {
        let nodes = (0..list.len()).map(|k| list.get(k));
        if !want_all {
            let hit = nodes.clone().find(|&n| selector.matches(n));
            return wrap_node(scope, hit);
        }
        let mut out = ListBuilder::new(scope);
        for n in nodes {
            if selector.matches(n) {
                out.push(scope, n);
            }
        }
        out.finish(scope)
    };
    let found = if let Some(class) = key.class.filter(|_| doc.class_table().is_some()) {
        class_lookup(doc, class, |list| candidates(scope, list))
    } else {
        let tag = key.type_name.filter(|_| doc.tag_table().is_some())?;
        tag_lookup(doc, tag, |list| candidates(scope, list))
    };
    Some(found.unwrap_or_else(|| no_match(scope, want_all)))
}

fn filter_nulls(raw: &[u8]) -> CString {
    let mut out = Vec::with_capacity(raw.len() + 16);
    for &c in raw {
        if c == 0 {
            out.extend_from_slice("\u{fffd}".as_bytes());
        } else {
            out.push(c);
        }
    }
    CString::new(out).unwrap_or_default()
}

fn target_fragment(scope: &mut Scope<'_>, el: Element) -> Option<Vec<u8>> {
    let doc = southstar_dom::ancestors_and_self(el)
        .find(|p| p.kind() == Kind::Document && p.flags() & FLAG_FRAGMENT == 0);
    if let Some(doc) = doc {
        let wrapper = wrap_node(scope, Some(doc));
        if wrapper.is_object() {
            let url = scope
                .get(&wrapper, "URL")
                .unwrap_or_else(|_| Value::undefined());
            if url.is_string()
                && let Ok(url) = scope.to_bytes(&url)
            {
                let url = url.split(|&c| c == 0).next().unwrap_or_default();
                return url
                    .iter()
                    .position(|&c| c == b'#')
                    .map(|hash| &url[hash + 1..])
                    .filter(|fragment| !fragment.is_empty())
                    .map(<[u8]>::to_vec);
            }
        }
    }
    current_url_fragment(js_of(scope))
}

fn enter_match(scope: &mut Scope<'_>, el: Element, batch: bool) -> MatchContext {
    let fragment = target_fragment(scope, el);
    let focus = focused_node(js_of(scope));
    MatchContext::enter(el, focus, fragment.as_deref(), batch)
}

fn query(
    scope: &mut Scope<'_>,
    root: Option<Element>,
    args: &[Value],
    want_all: bool,
    include_self: bool,
) -> JsResult {
    let Some(argument) = args.first() else {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    };
    let Some(root) = root else {
        return Ok(no_match(scope, want_all));
    };
    with_text(scope, argument, |scope, bytes, text| {
        if bytes.contains(&0) {
            query_text(scope, root, &filter_nulls(bytes), want_all, include_self)
        } else {
            query_text(scope, root, text, want_all, include_self)
        }
    })?
}

fn query_text(
    scope: &mut Scope<'_>,
    root: Element,
    selector: &CStr,
    want_all: bool,
    include_self: bool,
) -> JsResult {
    if let Some(found) = simple(scope, root, selector, want_all, include_self) {
        return Ok(found);
    }
    let Some(list) = SelectorList::parse(selector.to_bytes()) else {
        return Err(selector_syntax_error(scope, selector.to_bytes()));
    };
    let _context = enter_match(scope, root, true);
    if let Some(found) = key_index(scope, root, &list, want_all, include_self) {
        return Ok(found);
    }
    let self_match = include_self && root.kind() == Kind::Element && list.matches_any(root);
    if want_all {
        let mut out = ListBuilder::new(scope);
        if self_match {
            out.push(scope, root);
        }
        for child in element_children(root) {
            walk_elements(child, 0, true, &mut |n| {
                if list.matches_any(n) {
                    out.push(scope, n);
                }
            });
        }
        return Ok(out.finish(scope));
    }
    let hit = if self_match {
        Some(root)
    } else {
        element_children(root).find_map(|c| first_element(c, 0, true, &mut |n| list.matches_any(n)))
    };
    Ok(wrap_node(scope, hit))
}

pub(crate) fn element_query_selector(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    query(scope, unwrap_node(this), args, false, false)
}

pub(crate) fn element_query_selector_all(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    query(scope, unwrap_node(this), args, true, false)
}

pub(crate) fn document_query_selector(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let root = document_root_for(scope, this);
    query(scope, root, args, false, true)
}

pub(crate) fn document_query_selector_all(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let root = document_root_for(scope, this);
    query(scope, root, args, true, true)
}

fn ident_start_ok(s: &[u8]) -> bool {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let c = at(0);
    if c == 0 {
        return false;
    }
    if c >= 0x80 || c.is_ascii_alphabetic() || c == b'_' {
        return true;
    }
    if c == b'\\' {
        return at(1) != 0;
    }
    if c == b'-' {
        let c2 = at(1);
        if c2 >= 0x80 || c2.is_ascii_alphabetic() || c2 == b'_' || c2 == b'-' {
            return true;
        }
        if c2 == b'\\' {
            return at(2) != 0;
        }
    }
    false
}

fn simple_name_after(sel: &[u8], sigil: u8) -> Option<&[u8]> {
    let (&first, rest) = sel.split_first()?;
    if first != sigil || rest.is_empty() || !ident_start_ok(rest) {
        return None;
    }
    rest.iter()
        .all(|&c| c > b' ' && !b".#:[>+~,*()".contains(&c))
        .then_some(rest)
}

fn simple_tag(sel: &[u8]) -> bool {
    sel.split_first().is_some_and(|(&first, rest)| {
        first.is_ascii_alphabetic() && rest.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

fn is_scope_selector(sel: &[u8]) -> bool {
    let start = sel
        .iter()
        .position(|&c| c != b' ' && c != b'\t')
        .unwrap_or(sel.len());
    let Some(rest) = sel[start..].strip_prefix(b":scope") else {
        return false;
    };
    let rest = &rest[rest
        .iter()
        .position(|&c| c != b' ' && c != b'\t')
        .unwrap_or(rest.len())..];
    rest.is_empty() || rest[0] == b','
}

enum Simple<'a> {
    Scope,
    Class(&'a [u8]),
    Tag(&'a [u8]),
    Id(&'a [u8]),
}

fn classify(sel: &[u8]) -> Option<Simple<'_>> {
    if is_scope_selector(sel) {
        Some(Simple::Scope)
    } else if let Some(class) = simple_name_after(sel, b'.') {
        Some(Simple::Class(class))
    } else if simple_tag(sel) {
        Some(Simple::Tag(sel))
    } else {
        simple_name_after(sel, b'#').map(Simple::Id)
    }
}

fn simple_matches(el: Element, simple: &Simple<'_>) -> bool {
    match *simple {
        Simple::Scope => true,
        Simple::Class(class) => has_class(el, class),
        Simple::Tag(tag) => el
            .name()
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag)),
        Simple::Id(id) => el.attr(c"id").is_some_and(|v| v.to_bytes() == id),
    }
}

pub(crate) fn matches(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let el = unwrap_node(this);
    let Some(argument) = args.first() else {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    };
    let Some(el) = el.filter(|el| el.kind() == Kind::Element) else {
        return Ok(Value::boolean(false));
    };
    with_text(scope, argument, |scope, _, text| {
        let selector = text.to_bytes();
        if let Some(simple) = classify(selector) {
            return Ok(Value::boolean(simple_matches(el, &simple)));
        }
        let Some(list) = SelectorList::parse(selector) else {
            return Err(selector_syntax_error(scope, selector));
        };
        let _context = enter_match(scope, el, false);
        Ok(Value::boolean(list.matches_any(el)))
    })?
}

fn closest_chain(el: Element) -> impl Iterator<Item = Element> {
    southstar_dom::ancestors_and_self(el)
        .take_while(|cur| cur.kind() == Kind::Element && !is_shadow_root(*cur))
        .take(MAX_DEPTH as usize)
}

pub(crate) fn closest(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(el), Some(argument)) = (unwrap_node(this), args.first()) else {
        return Ok(Value::null());
    };
    with_text(scope, argument, |scope, _, text| {
        let selector = text.to_bytes();
        if let Some(simple) = classify(selector) {
            let hit = match simple {
                Simple::Scope => Some(el),
                _ => closest_chain(el).find(|&cur| simple_matches(cur, &simple)),
            };
            return Ok(wrap_node(scope, hit));
        }
        let Some(list) = SelectorList::parse(selector) else {
            return Err(selector_syntax_error(scope, selector));
        };
        let context = enter_match(scope, el, false);
        let hit = closest_chain(el).find(|&cur| list.matches_any(cur));
        drop(context);
        Ok(wrap_node(scope, hit))
    })?
}

fn get_element_by_id(scope: &mut Scope<'_>, root: Option<Element>, args: &[Value]) -> JsResult {
    let (Some(root), Some(argument)) = (root, args.first()) else {
        return Ok(Value::null());
    };
    with_text(scope, argument, |scope, bytes, text| {
        let found = if bytes.contains(&0) {
            None
        } else {
            find_by_id(root, text)
        };
        wrap_node(scope, found)
    })
}

pub(crate) fn element_get_element_by_id(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    get_element_by_id(scope, unwrap_node(this), args)
}

pub(crate) fn document_get_element_by_id(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let root = document_root_for(scope, this);
    get_element_by_id(scope, root, args)
}
