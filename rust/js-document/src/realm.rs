//! Southstar — the documents built for frames, DOMParser and DOMImplementation: their properties, getters, cookie access and buffered open/write/close.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::write::{append_args, c_string, dynamic_markup_active, throw_during_construction};
use crate::{Element, JsResult, element_named, until_nul};

const REALM_MARK: &str = "\u{fffd}realmdoc";
const WRITE_BUFFER: &str = "\u{fffd}wbuf";
const INVALID_ACCESS_ERR: i32 = 15;
const MAX_COOKIE_LEN: usize = 4096;

const OPEN: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

const LISTED_ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) struct RealmDocument<'a> {
    pub url: &'a [u8],
    pub charset: &'a [u8],
    pub content_type: Option<&'a [u8]>,
    pub is_xml: bool,
    pub inert: bool,
}

fn prototype_of(scope: &mut Scope<'_>, constructor: &str) -> Option<Value> {
    let global = scope.global();
    let constructor = scope.get(&global, constructor).ok()?;
    scope
        .get(&constructor, "prototype")
        .ok()
        .filter(Value::is_object)
}

fn use_prototype(scope: &mut Scope<'_>, object: &Value, constructor: &str) {
    if let Some(proto) = prototype_of(scope, constructor) {
        let _ = scope.set_prototype(object, &proto);
    }
}

fn use_xml_prototype(scope: &mut Scope<'_>, object: &Value) {
    let proto = prototype_of(scope, "XMLDocument");
    let document_proto = prototype_of(scope, "Document");
    if let (Some(proto), Some(document_proto)) = (&proto, &document_proto) {
        let _ = scope.set_prototype(proto, document_proto);
    }
    if let Some(proto) = proto {
        let _ = scope.set_prototype(object, &proto);
    }
}

fn define_text(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    let _ = scope.define(object, key, value, OPEN);
}

pub(crate) fn define_getter(scope: &mut Scope<'_>, object: &Value, name: &str, get: NativeFn) {
    let getter = scope.function(name, 0, get);
    let _ = scope.define_accessor(object, name, Some(&getter), None, Attributes::CONFIGURABLE);
}

fn define_accessor(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    get: NativeFn,
    set: NativeFn,
) {
    let getter = scope.function(name, 0, get);
    let setter = scope.function(name, 1, set);
    let _ = scope.define_accessor(
        object,
        name,
        Some(&getter),
        Some(&setter),
        Attributes::CONFIGURABLE,
    );
}

fn define_method(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    let _ = scope.define(object, name, function, OPEN);
}

pub(crate) fn make(scope: &mut Scope<'_>, doc: Element, options: &RealmDocument<'_>) -> Value {
    let w = ffi::wrap_node(scope, Some(doc));
    if !w.is_object() {
        return w;
    }
    if options.is_xml {
        use_xml_prototype(scope, &w);
        let _ = scope.define(&w, "__ndXmlDoc", Value::boolean(true), HIDDEN);
    } else {
        use_prototype(scope, &w, "HTMLDocument");
    }
    let content_type = options.content_type.unwrap_or(if options.is_xml {
        b"application/xml"
    } else {
        b"text/html"
    });
    let url = options.url;
    let charset = options.charset;
    define_text(scope, &w, "URL", url);
    define_text(scope, &w, "documentURI", url);
    define_text(scope, &w, "baseURI", url);
    define_text(scope, &w, "compatMode", b"CSS1Compat");
    define_text(scope, &w, "characterSet", charset);
    define_text(scope, &w, "charset", charset);
    define_text(scope, &w, "inputEncoding", charset);
    define_text(scope, &w, "contentType", content_type);
    let _ = scope.define(&w, "nodeType", Value::int(9), OPEN);
    define_text(scope, &w, "nodeName", b"#document");
    let _ = scope.define(&w, "ownerDocument", Value::null(), OPEN);
    let host = ffi::url_host(url).unwrap_or_default();
    define_text(scope, &w, "domain", &host);
    let _ = scope.define(&w, "xmlEncoding", Value::null(), OPEN);
    let _ = scope.define(&w, "xmlStandalone", Value::boolean(false), OPEN);
    let sheets = scope.new_array();
    let _ = scope.define(&w, "adoptedStyleSheets", sheets, OPEN);
    if options.inert {
        let _ = scope.define(&w, "location", Value::null(), OPEN);
    }
    define_getter(scope, &w, "documentElement", get_document_element);
    define_getter(scope, &w, "doctype", get_doctype);
    define_accessor(scope, &w, "body", get_body, set_body);
    define_getter(scope, &w, "head", get_head);
    define_accessor(scope, &w, "title", get_title, set_title);
    define_getter(scope, &w, "forms", get_forms);
    define_getter(scope, &w, "images", get_images);
    let implementation = scope.function("get implementation", 0, get_implementation);
    let _ = scope.define_accessor(
        &w,
        "implementation",
        Some(&implementation),
        None,
        Attributes::CONFIGURABLE,
    );
    install_api(scope, &w);
    define_cookie(scope, &w, (!options.inert).then_some(url));
    for (name, arity, f) in ffi::REALM_DOCUMENT_METHODS {
        ffi::bind_c_function(scope, &w, name, arity, f);
    }
    let by_id = scope.function("getElementById", 1, get_element_by_id);
    let _ = scope.set(&w, "getElementById", by_id);
    define_method(scope, &w, "open", 0, open);
    define_method(scope, &w, "close", 0, close);
    define_method(scope, &w, "write", 1, write);
    define_method(scope, &w, "writeln", 1, writeln);
    for (name, arity, f) in ffi::REALM_QUERY_METHODS {
        ffi::bind_c_function(scope, &w, name, arity, f);
    }
    w
}

pub(crate) fn synth_xml(scope: &mut Scope<'_>) -> Value {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Value::null();
    }
    let Some(doc) = ffi::new_document() else {
        return Value::null();
    };
    ffi::orphan_node(js, doc);
    let options = RealmDocument {
        url: b"about:blank",
        charset: b"UTF-8",
        content_type: Some(b"application/xml"),
        is_xml: true,
        inert: true,
    };
    let wrapper = make(scope, doc, &options);
    if wrapper.is_object() {
        let _ = scope.define(&wrapper, "defaultView", Value::null(), OPEN);
    }
    wrapper
}

pub(crate) fn document_ctor(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let doc = synth_xml(scope);
    if doc.is_object() {
        use_prototype(scope, &doc, "Document");
    }
    Ok(doc)
}

pub(crate) fn is_realm_document(scope: &mut Scope<'_>, doc: &Value) -> bool {
    doc.is_object()
        && scope
            .get(doc, REALM_MARK)
            .is_ok_and(|mark| !mark.is_undefined())
}

fn install_api(scope: &mut Scope<'_>, doc: &Value) {
    if is_realm_document(scope, doc) {
        return;
    }
    let _ = scope.define(doc, REALM_MARK, Value::boolean(true), HIDDEN);
    ffi::install_document_funcs(scope, doc);
}

pub(crate) fn lift_methods_to_proto(scope: &mut Scope<'_>, document: &Value) {
    let Ok(proto) = scope.get_prototype(document) else {
        return;
    };
    if !proto.is_object() {
        return;
    }
    let Ok(keys) = scope.own_property_keys(document, true) else {
        return;
    };
    for key in keys {
        let Ok(Some(own)) = scope.own_property(document, &key) else {
            continue;
        };
        if own.accessor || !scope.is_function(&own.value) {
            continue;
        }
        if !matches!(scope.own_property(&proto, &key), Ok(Some(_))) {
            let _ = scope.define_with_key(&proto, &key, own.value, Attributes::METHOD);
        }
        let _ = scope.delete_key(document, &key);
    }
}

fn empty_cookie(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.string(""))
}

fn ignore_cookie(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

fn bound_url(scope: &mut Scope<'_>, data: &[Value]) -> Option<Vec<u8>> {
    let url = data.first().filter(|url| url.is_string())?;
    scope.to_bytes(url).ok()
}

fn cookie_get(scope: &mut Scope<'_>, _: &Value, _: &[Value], data: &[Value]) -> JsResult {
    let cookies = bound_url(scope, data)
        .and_then(|url| ffi::cookies_for_js(&url))
        .unwrap_or_default();
    Ok(scope.string_from_bytes(&cookies))
}

fn cookie_set(scope: &mut Scope<'_>, _: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let Some(value) = args.first() else {
        return Ok(Value::undefined());
    };
    let url = bound_url(scope, data);
    let value = scope.to_bytes(value).ok();
    if let (Some(url), Some(value)) = (url, value) {
        let value = until_nul(&value);
        if value.len() <= MAX_COOKIE_LEN {
            ffi::cookie_store_from_js(&url, value);
        }
    }
    Ok(Value::undefined())
}

fn define_cookie(scope: &mut Scope<'_>, doc: &Value, url: Option<&[u8]>) {
    let (getter, setter) = match url {
        None => (
            scope.function("get cookie", 0, empty_cookie),
            scope.function("set cookie", 1, ignore_cookie),
        ),
        Some(url) => {
            let url = scope.string_from_bytes(url);
            let data = [url];
            (
                scope.bound_function("", 0, cookie_get, &data),
                scope.bound_function("", 1, cookie_set, &data),
            )
        }
    };
    let _ = scope.define_accessor(doc, "cookie", Some(&getter), Some(&setter), LISTED_ACCESSOR);
}

pub(crate) fn deny_cookie(scope: &mut Scope<'_>, doc: &Value) {
    if !doc.is_object() {
        return;
    }
    let getter = scope.function("get cookie", 0, empty_cookie);
    let _ = scope.define_accessor(doc, "cookie", Some(&getter), None, Attributes::CONFIGURABLE);
}

fn first_child_of_kind(node: Element, kind: Kind) -> Option<Element> {
    southstar_dom::children(node).find(|c| c.kind() == kind)
}

fn get_document_element(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(doc) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    match first_child_of_kind(doc, Kind::Element) {
        Some(root) => Ok(ffi::wrap_node(scope, Some(root))),
        None => Ok(Value::null()),
    }
}

pub(crate) fn get_doctype(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(doc) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    match first_child_of_kind(doc, Kind::Doctype) {
        Some(doctype) => Ok(ffi::wrap_node(scope, Some(doctype))),
        None => Ok(Value::null()),
    }
}

fn get_body(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::elements::get_body(scope, this, args)
}

fn set_body(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Ok(Value::undefined());
    }
    crate::elements::set_body(scope, this, args)
}

fn get_head(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let head = ffi::unwrap_node(this)
        .and_then(|doc| first_child_of_kind(doc, Kind::Element))
        .and_then(|html| southstar_dom::children(html).find(|c| element_named(*c, "head")));
    match head {
        Some(head) => Ok(ffi::wrap_node(scope, Some(head))),
        None => Ok(Value::null()),
    }
}

fn get_title(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let title = ffi::unwrap_node(this)
        .and_then(|doc| southstar_dom::index::find_first_element(doc, c"title"));
    let text = southstar_dom::serialize::collect_text(title);
    Ok(scope.string_from_bytes(&text))
}

fn set_title(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(doc), Some(value)) = (ffi::unwrap_node(this), args.first()) else {
        return Ok(Value::undefined());
    };
    let Ok(text) = scope.to_bytes(value) else {
        return Ok(Value::undefined());
    };
    let text = until_nul(&text);
    let title = match southstar_dom::index::find_first_element(doc, c"title") {
        Some(title) => title,
        None => {
            let head = southstar_dom::index::find_first_element(doc, c"head").unwrap_or(doc);
            let title = ffi::new_element(b"title");
            ffi::append_child(head, title);
            title
        }
    };
    let js = ffi::js_of(scope);
    ffi::clear_children(js, title);
    ffi::append_child(title, ffi::new_text(text));
    if !js.is_null() {
        ffi::mark_mutated(js);
    }
    Ok(Value::undefined())
}

fn get_forms(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(ffi::live_doc_tag(scope, this, c"form"))
}

fn get_images(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(ffi::live_doc_tag(scope, this, c"img"))
}

fn get_implementation(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(ffi::implementation(scope, this))
}

fn get_element_by_id(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(id), Some(root)) = (args.first(), ffi::unwrap_node(this)) else {
        return Ok(Value::null());
    };
    let Ok(id) = scope.to_bytes(id) else {
        return Ok(Value::null());
    };
    let id = c_string(&id);
    match southstar_dom::index::find_by_id(root, &id) {
        Some(found) => Ok(ffi::wrap_node(scope, Some(found))),
        None => Ok(Value::null()),
    }
}

fn frame_host(doc: Element) -> Option<Element> {
    doc.parent()
        .filter(|host| element_named(*host, "iframe") || element_named(*host, "frame"))
}

pub(crate) fn open(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() >= 3 {
        return Err(ffi::dom_exception(
            scope,
            "InvalidAccessError",
            INVALID_ACCESS_ERR,
            "document.open(url, name, features) requires a window",
        ));
    }
    let js = ffi::js_of(scope);
    if dynamic_markup_active(js) {
        return Err(throw_during_construction(scope, "open"));
    }
    if let Some(doc) = ffi::unwrap_node(this)
        && !js.is_null()
    {
        ffi::orphan_children(js, doc);
        let empty = scope.string("");
        let _ = scope.set(this, WRITE_BUFFER, empty);
        if let Some(host) = frame_host(doc) {
            ffi::set_attr(host, c"data-nd-doc-written", c"1");
        }
        ffi::mark_mutated(js);
    }
    Ok(this.clone())
}

fn append_write(scope: &mut Scope<'_>, this: &Value, args: &[Value], newline: bool) {
    let js = ffi::js_of(scope);
    let Some(doc) = ffi::unwrap_node(this) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let current = scope.get(this, WRITE_BUFFER).ok().filter(Value::is_string);
    if current.is_none() && ffi::ignore_destructive_writes(js) {
        return;
    }
    let mut buffer = Vec::new();
    match current {
        Some(current) => {
            if let Ok(text) = scope.to_bytes(&current) {
                buffer.extend_from_slice(until_nul(&text));
            }
        }
        None => ffi::orphan_children(js, doc),
    }
    append_args(scope, &mut buffer, args);
    if newline {
        buffer.push(b'\n');
    }
    let buffer = scope.string_from_bytes(&buffer);
    let _ = scope.set(this, WRITE_BUFFER, buffer);
}

pub(crate) fn write(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    append_write(scope, this, args, false);
    Ok(Value::undefined())
}

pub(crate) fn writeln(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    append_write(scope, this, args, true);
    Ok(Value::undefined())
}

fn replace_children(js: Js, doc: Element, markup: &[u8]) {
    let Some(parsed) = ffi::parse_html(markup) else {
        return;
    };
    ffi::orphan_children(js, doc);
    let mut child = parsed.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        southstar_dom::node::own_strings_deep(c);
        ffi::remove_node(c);
        ffi::append_child(doc, c);
        ffi::index_child_change(js, doc, c);
    }
    ffi::free_node(parsed);
    ffi::mark_mutated(js);
}

pub(crate) fn close(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    let Some(doc) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let Some(current) = scope.get(this, WRITE_BUFFER).ok().filter(Value::is_string) else {
        return Ok(Value::undefined());
    };
    if let Ok(markup) = scope.to_bytes(&current) {
        replace_children(js, doc, until_nul(&markup));
    }
    let _ = scope.set(this, WRITE_BUFFER, Value::undefined());
    if let Some(host) = frame_host(doc) {
        ffi::set_attr(host, c"data-nd-doc-written", c"1");
        ffi::pending_iframe_add(js, host);
        ffi::mark_mutated(js);
    }
    Ok(Value::undefined())
}
