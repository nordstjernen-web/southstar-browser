//! Southstar — live collections: the HTMLCollection, NodeList and RadioNodeList objects behind children, childNodes, getElementsBy*, form.elements, links, labels and attributes, rebuilt when the DOM generation moves, with indexed and named access.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::serialize::is_embedded_doc;
use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, MAX_DEPTH};
use southstar_js_engine::quickjs::{self, JSValue};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{
    build_attributes, build_labels, current_document, document_root_for, form_elements_named,
    form_listed_controls, form_owner, js_of, live_new_object, live_of, unwrap_node, with_text,
    wrap_node,
};
use crate::nodelist::{self, array_length};
use crate::page::{self, LiveProtos};
use crate::{
    Element, FLAG_QUIRKS, FLAG_XML_DOC, FOREIGN, JsResult, arg, class_tokens, element_children,
    hidden_child, is_named, is_shadow_root, is_template, push, walk_elements,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveKind {
    Children,
    ChildNodes,
    ByTag,
    DocTag,
    ByTagNs,
    DocTagNs,
    ByClass,
    DocClass,
    ByName,
    FormElements,
    Links,
    RadioNodeList,
    Attributes,
    Labels,
}

impl LiveKind {
    pub(crate) fn from_raw(raw: c_int) -> Option<LiveKind> {
        const ALL: [LiveKind; 14] = [
            LiveKind::Children,
            LiveKind::ChildNodes,
            LiveKind::ByTag,
            LiveKind::DocTag,
            LiveKind::ByTagNs,
            LiveKind::DocTagNs,
            LiveKind::ByClass,
            LiveKind::DocClass,
            LiveKind::ByName,
            LiveKind::FormElements,
            LiveKind::Links,
            LiveKind::RadioNodeList,
            LiveKind::Attributes,
            LiveKind::Labels,
        ];
        usize::try_from(raw).ok().and_then(|i| ALL.get(i).copied())
    }

    fn document_scoped(self) -> bool {
        matches!(
            self,
            LiveKind::ByName
                | LiveKind::Links
                | LiveKind::DocTag
                | LiveKind::DocTagNs
                | LiveKind::DocClass
        )
    }
}

struct Snapshot {
    items: Value,
    length: u32,
    generation: u64,
}

pub(crate) struct Live {
    owner: Value,
    kind: LiveKind,
    param: Option<Box<[u8]>>,
    param_lower: Option<Box<[u8]>>,
    param2: Option<Box<[u8]>>,
    html_collection: bool,
    cache: RefCell<Option<Snapshot>>,
}

pub(crate) enum Key<'a> {
    Index(u32),
    Name(&'a CStr),
}

impl Live {
    pub(crate) fn owner(&self) -> &Value {
        &self.owner
    }

    pub(crate) fn cache_raw(&self) -> JSValue {
        match self.cache.try_borrow() {
            Ok(cache) => cache
                .as_ref()
                .map_or(quickjs::UNDEFINED, |snap| quickjs::raw(&snap.items)),
            Err(_) => quickjs::UNDEFINED,
        }
    }

    pub(crate) fn collection_kind(&self) -> c_int {
        if self.kind == LiveKind::Attributes {
            -1
        } else {
            self.html_collection as c_int
        }
    }

    pub(crate) fn named_access(&self) -> bool {
        self.html_collection || self.kind == LiveKind::Attributes
    }

    fn param(&self) -> &[u8] {
        self.param.as_deref().unwrap_or_default()
    }
}

fn truncated(bytes: &[u8]) -> Box<[u8]> {
    bytes.split(|&c| c == 0).next().unwrap_or_default().into()
}

pub(crate) fn make(
    scope: &mut Scope<'_>,
    owner: &Value,
    kind: LiveKind,
    param: Option<&[u8]>,
    param2: Option<&[u8]>,
) -> Value {
    let html_collection = !matches!(
        kind,
        LiveKind::ChildNodes
            | LiveKind::ByName
            | LiveKind::Attributes
            | LiveKind::RadioNodeList
            | LiveKind::Labels
    );
    let param = param.map(truncated);
    let param_lower = param.as_deref().map(|p| p.to_ascii_lowercase().into());
    let back = Box::new(Live {
        owner: owner.clone(),
        kind,
        param,
        param_lower,
        param2: param2.map(truncated),
        html_collection,
        cache: RefCell::new(None),
    });
    let object = live_new_object(scope, back);
    if object.is_object()
        && let Some(protos) = page::live_protos(js_of(scope))
    {
        let proto = if kind == LiveKind::RadioNodeList {
            protos.radio
        } else if html_collection {
            protos.html
        } else {
            protos.node
        };
        let _ = scope.set_prototype(&object, &proto);
    }
    object
}

fn tag_matches(node: Element, tag: &[u8], lower: &[u8]) -> bool {
    if tag == b"*" {
        return true;
    }
    let Some(name) = node.name().map(CStr::to_bytes) else {
        return false;
    };
    if node.flags() & FOREIGN != 0 {
        return tag == name;
    }
    match node.attr(c"data-nd-ns-prefix").map(CStr::to_bytes) {
        Some(prefix) => lower
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix(b":"))
            .is_some_and(|local| local == name),
        None => lower == name,
    }
}

fn element_namespace(node: Element) -> Option<&'static [u8]> {
    if let Some(stored) = node.attr(c"data-nd-ns-uri") {
        let stored = stored.to_bytes();
        return (!stored.is_empty()).then_some(stored);
    }
    if node.flags() & FLAG_SVG_NS != 0 {
        return Some(b"http://www.w3.org/2000/svg");
    }
    if node.flags() & FLAG_FOREIGN_NS != 0 {
        return None;
    }
    Some(b"http://www.w3.org/1999/xhtml")
}

fn local_name(node: Element) -> Option<&'static [u8]> {
    let name = node.name()?.to_bytes();
    Some(match name.iter().position(|&c| c == b':') {
        Some(colon) => &name[colon + 1..],
        None => name,
    })
}

fn has_class_list(node: Element, wanted: &[u8], case_insensitive: bool) -> bool {
    let Some(list) = node.attr(c"class").map(CStr::to_bytes) else {
        return false;
    };
    let mut any = false;
    for token in class_tokens(wanted) {
        any = true;
        let found = class_tokens(list).any(|have| {
            if case_insensitive {
                have.eq_ignore_ascii_case(token)
            } else {
                have == token
            }
        });
        if !found {
            return false;
        }
    }
    any
}

fn name_in(node: Element, names: &[&[u8]]) -> bool {
    node.is_element()
        && node.name().is_some_and(|name| {
            let name = name.to_bytes();
            names.iter().any(|n| name.eq_ignore_ascii_case(n))
        })
}

const LISTED: &[&[u8]] = &[
    b"input",
    b"select",
    b"textarea",
    b"button",
    b"fieldset",
    b"output",
    b"object",
];

const RADIO_CONTROLS: &[&[u8]] = &[
    b"input",
    b"select",
    b"textarea",
    b"button",
    b"fieldset",
    b"output",
    b"object",
    b"img",
];

fn fieldset_listed(scan: Element, depth: i32, visit: &mut dyn FnMut(Element)) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in element_children(scan) {
        if hidden_child(child) {
            continue;
        }
        if name_in(child, LISTED) {
            visit(child);
        }
        fieldset_listed(child, depth + 1, visit);
    }
}

fn radio_nodes(
    form: Element,
    scan: Element,
    doc: Element,
    name: &[u8],
    depth: i32,
    visit: &mut dyn FnMut(Element),
) {
    if depth >= MAX_DEPTH || name.is_empty() {
        return;
    }
    for child in element_children(scan) {
        if hidden_child(child) {
            continue;
        }
        if name_in(child, RADIO_CONTROLS) && form_owner(child, doc) == Some(form) {
            let id = child.attr(c"id").map(CStr::to_bytes);
            let nm = child.attr(c"name").map(CStr::to_bytes);
            if id == Some(name) || nm == Some(name) {
                visit(child);
            }
        }
        radio_nodes(form, child, doc, name, depth + 1, visit);
    }
}

fn links(node: Element, depth: i32, visit: &mut dyn FnMut(Element)) {
    if depth >= MAX_DEPTH || hidden_child(node) {
        return;
    }
    for child in element_children(node) {
        if (is_named(child, b"a") || is_named(child, b"area")) && child.attr(c"href").is_some() {
            visit(child);
        }
        links(child, depth + 1, visit);
    }
}

fn build(scope: &mut Scope<'_>, back: &Live) -> (Value, u32) {
    let js = js_of(scope);
    let root = unwrap_node(&back.owner).or_else(|| {
        back.kind
            .document_scoped()
            .then(|| current_document(js))
            .flatten()
    });
    let array = scope.new_array();
    let Some(root) = root else {
        return (array, 0);
    };
    let mut length = 0u32;
    let mut add = |scope: &mut Scope<'_>, node: Element| {
        let wrapper = wrap_node(scope, Some(node));
        push(scope, &array, &mut length, wrapper);
    };
    match back.kind {
        LiveKind::Children => {
            if !is_template(root) {
                for child in element_children(root) {
                    if child.is_element() && !is_shadow_root(child) {
                        add(scope, child);
                    }
                }
            }
        }
        LiveKind::ChildNodes => {
            if !is_template(root) {
                for child in element_children(root) {
                    if !is_embedded_doc(child) && !is_shadow_root(child) {
                        add(scope, child);
                    }
                }
            }
        }
        LiveKind::ByTag | LiveKind::DocTag => {
            let (tag, lower) = match (&back.param, &back.param_lower) {
                (Some(tag), Some(lower)) => (&tag[..], &lower[..]),
                _ => (&b"*"[..], &b"*"[..]),
            };
            for child in element_children(root) {
                walk_elements(child, 0, true, &mut |n| {
                    if tag_matches(n, tag, lower) {
                        add(scope, n);
                    }
                });
            }
        }
        LiveKind::ByTagNs | LiveKind::DocTagNs => {
            let local = back.param.as_deref().unwrap_or(b"*");
            let namespace = back.param2.as_deref();
            let ns_wild = namespace == Some(b"*");
            let local_wild = local == b"*";
            for child in element_children(root) {
                walk_elements(child, 0, true, &mut |n| {
                    if n.name().is_none() {
                        return;
                    }
                    let ns_ok = ns_wild || element_namespace(n) == namespace;
                    if ns_ok && (local_wild || local_name(n) == Some(local)) {
                        add(scope, n);
                    }
                });
            }
        }
        LiveKind::ByClass | LiveKind::DocClass => {
            let quirks = current_document(js).is_some_and(|doc| doc.flags() & FLAG_QUIRKS != 0);
            let wanted = back.param();
            for child in element_children(root) {
                walk_elements(child, 0, true, &mut |n| {
                    if has_class_list(n, wanted, quirks) {
                        add(scope, n);
                    }
                });
            }
        }
        LiveKind::ByName => {
            let wanted = back.param();
            walk_elements(root, 0, true, &mut |n| {
                if n.attr(c"name").is_some_and(|v| v.to_bytes() == wanted) {
                    add(scope, n);
                }
            });
        }
        LiveKind::FormElements => {
            if is_named(root, b"form") {
                form_listed_controls(root, |control| add(scope, control));
            } else if is_named(root, b"fieldset") {
                fieldset_listed(root, 0, &mut |n| add(scope, n));
            }
        }
        LiveKind::RadioNodeList => {
            if is_named(root, b"form") {
                let doc = root.root();
                radio_nodes(root, doc, doc, back.param(), 0, &mut |n| add(scope, n));
            }
        }
        LiveKind::Links => links(root, 0, &mut |n| add(scope, n)),
        LiveKind::Attributes => {
            let built = build_attributes(scope, &back.owner);
            let length = array_length(scope, &built);
            return (built, length);
        }
        LiveKind::Labels => {
            let built = build_labels(scope, root);
            let length = array_length(scope, &built);
            return (built, length);
        }
    }
    (array, length)
}

fn snapshot_with_length(scope: &mut Scope<'_>, back: &Live) -> (Value, u32) {
    let generation = page::dom_gen(js_of(scope));
    if back.kind != LiveKind::Attributes
        && let Some(snap) = back
            .cache
            .borrow()
            .as_ref()
            .filter(|snap| snap.generation == generation)
    {
        return (snap.items.clone(), snap.length);
    }
    let (items, length) = build(scope, back);
    let stale = back.cache.replace(Some(Snapshot {
        items: items.clone(),
        length,
        generation,
    }));
    drop(stale);
    (items, length)
}

pub(crate) fn snapshot(scope: &mut Scope<'_>, back: &Live) -> Value {
    snapshot_with_length(scope, back).0
}

pub(crate) fn length(scope: &mut Scope<'_>, back: &Live) -> u32 {
    snapshot_with_length(scope, back).1
}

fn html_in_html(back: &Live) -> bool {
    unwrap_node(&back.owner)
        .is_some_and(|root| root.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS | FLAG_XML_DOC) == 0)
}

fn string_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = scope.get(object, key).ok()?;
    scope.to_bytes(&value).ok()
}

fn named(
    scope: &mut Scope<'_>,
    back: &Live,
    snap: &Value,
    length: u32,
    name: &CStr,
) -> Option<Value> {
    let wanted = name.to_bytes();
    if wanted.is_empty() {
        return None;
    }
    if back.kind == LiveKind::Attributes {
        if html_in_html(back) && wanted.iter().any(u8::is_ascii_uppercase) {
            return None;
        }
        for i in 0..length {
            let attr = scope.get_index(snap, i).ok()?;
            let candidate = string_prop(scope, &attr, "name");
            if candidate
                .as_deref()
                .map(|c| c.split(|&b| b == 0).next().unwrap_or_default())
                == Some(wanted)
            {
                return Some(attr);
            }
        }
        return None;
    }
    if back.kind == LiveKind::FormElements {
        let found = form_elements_named(scope, snap, name);
        return (!found.is_null() && !found.is_undefined()).then_some(found);
    }
    for i in 0..length {
        let element = scope.get_index(snap, i).ok()?;
        let Some(n) = unwrap_node(&element) else {
            continue;
        };
        let id = n.attr(c"id").map(CStr::to_bytes);
        let html_ns = n.flags() & FOREIGN == 0;
        let nm = if html_ns {
            n.attr(c"name").map(CStr::to_bytes)
        } else {
            None
        };
        if id == Some(wanted) || nm == Some(wanted) {
            return Some(element);
        }
    }
    None
}

fn proto_has(scope: &mut Scope<'_>, proto: &Value, name: &[u8]) -> bool {
    proto.is_object()
        && scope
            .has_property(proto, &String::from_utf8_lossy(name))
            .unwrap_or(false)
}

pub(crate) fn get_own(scope: &mut Scope<'_>, back: &Live, key: Key<'_>) -> Option<Value> {
    let (snap, length) = snapshot_with_length(scope, back);
    match key {
        Key::Index(index) => {
            if index >= length {
                return None;
            }
            scope.get_index(&snap, index).ok()
        }
        Key::Name(name) => {
            if !back.named_access() {
                return None;
            }
            named(scope, back, &snap, length, name)
        }
    }
}

pub(crate) fn deletable(scope: &mut Scope<'_>, back: &Live, key: Key<'_>) -> bool {
    let (snap, length) = snapshot_with_length(scope, back);
    match key {
        Key::Index(index) => index >= length,
        Key::Name(name) => {
            !(back.named_access() && named(scope, back, &snap, length, name).is_some())
        }
    }
}

pub(crate) fn named_exists(scope: &mut Scope<'_>, back: &Live, key: Key<'_>) -> bool {
    let Key::Name(name) = key else {
        return false;
    };
    if !back.named_access() {
        return false;
    }
    let (snap, length) = snapshot_with_length(scope, back);
    named(scope, back, &snap, length, name).is_some()
}

pub(crate) fn own_names(
    scope: &mut Scope<'_>,
    back: &Live,
    obj: &Value,
    mut emit: impl FnMut(&[u8]),
) -> u32 {
    let (snap, length) = snapshot_with_length(scope, back);
    if !back.named_access() {
        return length;
    }
    let proto = scope
        .get_prototype(obj)
        .unwrap_or_else(|_| Value::undefined());
    let mut names: Vec<Vec<u8>> = Vec::new();
    let mut offer = |scope: &mut Scope<'_>, name: &[u8]| {
        if !name.is_empty()
            && !proto_has(scope, &proto, name)
            && !names.iter().any(|have| have[..] == *name)
        {
            names.push(name.to_vec());
        }
    };
    if back.kind == LiveKind::Attributes {
        let html = html_in_html(back);
        for i in 0..length {
            let attr = scope
                .get_index(&snap, i)
                .unwrap_or_else(|_| Value::undefined());
            let Some(name) = string_prop(scope, &attr, "name") else {
                continue;
            };
            let name = name.split(|&b| b == 0).next().unwrap_or_default();
            if !html || !name.iter().any(u8::is_ascii_uppercase) {
                offer(scope, name);
            }
        }
    } else {
        for i in 0..length {
            let element = scope
                .get_index(&snap, i)
                .unwrap_or_else(|_| Value::undefined());
            let Some(n) = unwrap_node(&element) else {
                continue;
            };
            if let Some(id) = n.attr(c"id") {
                offer(scope, id.to_bytes());
            }
            if n.flags() & FOREIGN == 0
                && let Some(nm) = n.attr(c"name")
            {
                offer(scope, nm.to_bytes());
            }
        }
    }
    for name in &names {
        emit(name);
    }
    length
}

pub(crate) fn item(scope: &mut Scope<'_>, this: &Value, index: Option<&Value>) -> JsResult {
    let (Some(back), Some(index)) = (live_of(this), index) else {
        return Ok(Value::null());
    };
    let Ok(index) = scope.to_int32(index) else {
        return Ok(Value::null());
    };
    let (snap, length) = snapshot_with_length(scope, back);
    if index < 0 || index as u32 >= length {
        return Ok(Value::null());
    }
    scope.get_index(&snap, index as u32)
}

fn named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(back), Some(name)) = (live_of(this), args.first()) else {
        return Ok(Value::null());
    };
    let Ok(name) = scope.to_bytes(name) else {
        return Ok(Value::null());
    };
    let name = CString::new(truncated(&name).into_vec()).unwrap_or_default();
    let (snap, length) = snapshot_with_length(scope, back);
    Ok(named(scope, back, &snap, length, &name).unwrap_or_else(Value::null))
}

const OPERATION: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

const PROTO_DECORATOR: &str = "(function(hc, nl, rnl){ var A = Array.prototype; function def(o,k,v){   Object.defineProperty(o,k,{value:v,writable:true,configurable:true}); } def(hc, Symbol.toStringTag, 'HTMLCollection'); Object.defineProperty(hc, Symbol.iterator,   {value:A.values, writable:true, configurable:true}); def(nl, Symbol.toStringTag, 'NodeList'); def(nl, 'entries', A.entries); def(nl, 'keys',    A.keys); def(nl, 'values',  A.values); def(nl, 'forEach', A.forEach); Object.defineProperty(nl, Symbol.iterator,   {value:A.values, writable:true, configurable:true}); def(rnl, Symbol.toStringTag, 'RadioNodeList');})";

fn define_length(scope: &mut Scope<'_>, proto: &Value) {
    let getter = quickjs::c_function(scope, "get length", 0, crate::ffi::ns_live_length_get);
    let _ = scope.define_accessor(
        proto,
        "length",
        Some(&getter),
        None,
        Attributes::CONFIGURABLE,
    );
}

pub(crate) fn install_protos(scope: &mut Scope<'_>) {
    let html = scope.new_object();
    let item_fn = quickjs::c_function(scope, "item", 1, crate::ffi::live_item_native);
    let _ = scope.define(&html, "item", item_fn, OPERATION);
    let named_fn = scope.function("namedItem", 1, named_item);
    let _ = scope.define(&html, "namedItem", named_fn, OPERATION);
    let node = scope.new_object();
    let item_fn = quickjs::c_function(scope, "item", 1, crate::ffi::live_item_native);
    let _ = scope.define(&node, "item", item_fn, OPERATION);
    let radio = scope.new_object();
    let _ = scope.set_prototype(&radio, &node);
    define_length(scope, &html);
    define_length(scope, &node);
    if let Ok(decorator) = scope.eval_native_script(PROTO_DECORATOR, "<live-proto>") {
        let _ = scope.call(
            &decorator,
            &Value::undefined(),
            &[html.clone(), node.clone(), radio.clone()],
        );
    }
    let js = js_of(scope);
    if !js.is_null() {
        page::set_live_protos(js, LiveProtos { html, node, radio });
    }
}

pub(crate) fn proto(js: crate::ffi::Js, which: c_int) -> Option<Value> {
    let protos = page::live_protos(js)?;
    match which {
        0 => Some(protos.html),
        1 => Some(protos.node),
        2 => Some(protos.radio),
        _ => None,
    }
}

pub(crate) fn wire_constructors(scope: &mut Scope<'_>, global: &Value) {
    let Some(protos) = page::live_protos(js_of(scope)) else {
        return;
    };
    let wiring = [
        ("HTMLCollection", protos.html),
        ("NodeList", protos.node),
        ("RadioNodeList", protos.radio),
    ];
    for (name, proto) in wiring {
        let Ok(constructor) = scope.get(global, name) else {
            continue;
        };
        if constructor.is_object() {
            let _ = scope.set(&constructor, "prototype", proto.clone());
            let _ = scope.define(&proto, "constructor", constructor, Attributes::METHOD);
        }
    }
}

fn param_bytes(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    scope.to_bytes(value)
}

fn make_with_text(scope: &mut Scope<'_>, owner: &Value, kind: LiveKind, param: &Value) -> JsResult {
    with_text(scope, param, |scope, _, text| {
        make(scope, owner, kind, Some(text.to_bytes()), None)
    })
}

pub(crate) fn element_by_tag(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(_), Some(tag)) = (unwrap_node(this), args.first()) else {
        return Ok(nodelist::empty(scope));
    };
    make_with_text(scope, this, LiveKind::ByTag, tag)
}

pub(crate) fn document_by_tag(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(_), Some(tag)) = (document_root_for(scope, this), args.first()) else {
        return Ok(nodelist::empty(scope));
    };
    make_with_text(scope, this, LiveKind::DocTag, tag)
}

fn by_tag_ns(scope: &mut Scope<'_>, owner: &Value, kind: LiveKind, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Ok(make(scope, owner, kind, Some(b"*"), None));
    }
    let namespace = arg(args, 0);
    let namespace = if namespace.is_null() || namespace.is_undefined() {
        None
    } else {
        Some(param_bytes(scope, &namespace)?)
    };
    let local = param_bytes(scope, &args[1])?;
    let namespace = namespace.filter(|ns| !ns.is_empty() && ns[0] != 0);
    Ok(make(scope, owner, kind, Some(&local), namespace.as_deref()))
}

pub(crate) fn element_by_tag_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    by_tag_ns(scope, this, LiveKind::ByTagNs, args)
}

pub(crate) fn document_by_tag_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    by_tag_ns(scope, this, LiveKind::DocTagNs, args)
}

pub(crate) fn element_by_class(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(_), Some(class)) = (unwrap_node(this), args.first()) else {
        return Ok(scope.new_array());
    };
    make_with_text(scope, this, LiveKind::ByClass, class)
}

pub(crate) fn document_by_class(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(_), Some(class)) = (document_root_for(scope, this), args.first()) else {
        return Ok(nodelist::empty(scope));
    };
    make_with_text(scope, this, LiveKind::DocClass, class)
}

pub(crate) fn document_by_name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(_), Some(name)) = (document_root_for(scope, this), args.first()) else {
        return Ok(nodelist::empty(scope));
    };
    make_with_text(scope, this, LiveKind::ByName, name)
}
