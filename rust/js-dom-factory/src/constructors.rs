//! Southstar — the window's node constructors (DocumentFragment, Text, Comment, Image, Audio, Option), DOMParser with parseFromString, and CloseWatcher.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_dom::node::{self, KIND_COMMENT, KIND_DOCUMENT, KIND_TEXT};
use southstar_glib as glib;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::factories::text_node;
use crate::ffi::{self, TargetMethod};
use crate::{Element, FLAG_FRAGMENT, JsResult, PLAIN, arg, c_text};

fn orphan(scope: &mut Scope<'_>, node: Element) -> Value {
    ffi::track_orphan(ffi::js_of(scope), node);
    ffi::wrap(scope, node)
}

pub(crate) fn document_fragment(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let fragment = Node::alloc(KIND_DOCUMENT);
    fragment.add_flags(FLAG_FRAGMENT);
    Ok(orphan(scope, fragment))
}

fn character_data(scope: &mut Scope<'_>, args: &[Value], kind: core::ffi::c_uint) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let data = arg(args, 0);
    let text = if data.is_undefined() {
        Vec::new()
    } else {
        scope.to_bytes(&data)?
    };
    let wrapper = orphan(scope, text_node(kind, &text));
    ffi::tag_caller_document(scope, &wrapper);
    Ok(wrapper)
}

pub(crate) fn text(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    character_data(scope, args, KIND_TEXT)
}

pub(crate) fn comment(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    character_data(scope, args, KIND_COMMENT)
}

fn set_dimension(
    scope: &mut Scope<'_>,
    el: Element,
    name: &core::ffi::CStr,
    value: &Value,
) -> JsResult<()> {
    let size = scope.to_int32(value)?;
    if size > 0 {
        ffi::set_attr(el, name, size.to_string().as_bytes());
    }
    Ok(())
}

pub(crate) fn image(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let el = node::new_element(glib::strdup(b"img"));
    if let Some(width) = args.first() {
        set_dimension(scope, el, c"width", width)?;
    }
    if let Some(height) = args.get(1) {
        set_dimension(scope, el, c"height", height)?;
    }
    Ok(orphan(scope, el))
}

pub(crate) fn audio(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let el = node::new_element(glib::strdup(b"audio"));
    if let Some(src) = args.first() {
        let src = c_text(scope, src)?;
        ffi::set_attr(el, c"src", &src);
    }
    Ok(orphan(scope, el))
}

pub(crate) fn option(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let el = node::new_element(glib::strdup(b"option"));
    if let Some(text) = args.first() {
        let text = c_text(scope, text)?;
        el.append(text_node(KIND_TEXT, &text));
    }
    if let Some(value) = args.get(1) {
        let value = c_text(scope, value)?;
        ffi::set_attr(el, c"value", &value);
    }
    let default_selected = args.get(2).is_some_and(|v| scope.to_bool(v));
    let selected = args.get(3).is_some_and(|v| scope.to_bool(v));
    if default_selected || selected {
        ffi::set_attr(el, c"selected", b"");
    }
    Ok(orphan(scope, el))
}

fn object_from_new_target(scope: &mut Scope<'_>, new_target: &Value) -> Value {
    let proto = if new_target.is_object() {
        scope
            .get(new_target, "prototype")
            .unwrap_or_else(|_| Value::null())
    } else {
        Value::null()
    };
    if proto.is_object() {
        scope.new_object_with_proto(&proto)
    } else {
        scope.new_object()
    }
}

pub(crate) fn dom_parser(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let parser = object_from_new_target(scope, this);
    let global = scope.global();
    let document = scope
        .get(&global, "document")
        .unwrap_or_else(|_| Value::undefined());
    let url = if document.is_object() {
        scope
            .get(&document, "URL")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    if url.is_string() {
        let _ = scope.define(&parser, "__ns_creator_url", url, Attributes::METHOD);
    }
    Ok(parser)
}

fn has_root_element(doc: Element) -> bool {
    southstar_dom::children(doc).any(|child| child.is_element())
}

fn parse_xml(source: &[u8]) -> Option<Element> {
    let (doc, line, column) = ffi::parse_xml_reporting(source);
    if let Some(doc) = doc {
        if has_root_element(doc) {
            return Some(doc);
        }
        ffi::free_node(doc);
    }
    let report = format!(
        "<parsererror xmlns=\"http://www.mozilla.org/newlayout/xml/parsererror.xml\">error on line {line} at column {column}: XML parsing error</parsererror>"
    );
    ffi::parse_xml(report.as_bytes()).or_else(|| ffi::parse_html_fragment(source))
}

pub(crate) fn parse_from_string(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(source) = args.first() else {
        return Ok(Value::null());
    };
    let source = c_text(scope, source)?;
    let mime = match args.get(1) {
        Some(mime) => Some(c_text(scope, mime)?),
        None => None,
    };
    let as_xml = mime
        .as_deref()
        .is_some_and(|m| m.windows(3).any(|w| w == b"xml") || m.windows(3).any(|w| w == b"svg"));
    let doc = if as_xml {
        parse_xml(&source)
    } else {
        ffi::parse_html(&source)
    };
    let Some(doc) = doc else {
        return Ok(Value::null());
    };
    ffi::mark_scripts_already_started(doc);
    ffi::track_orphan(ffi::js_of(scope), doc);
    let creator_url = scope
        .get(this, "__ns_creator_url")
        .unwrap_or_else(|_| Value::undefined());
    let url = if creator_url.is_string() {
        c_text(scope, &creator_url).ok()
    } else {
        None
    };
    let wrapper = ffi::realm_document(
        scope,
        doc,
        ffi::RealmDocument {
            url: url.as_deref(),
            charset: Some(b"UTF-8"),
            content_type: mime.as_deref(),
            is_xml: as_xml,
        },
    );
    if wrapper.is_object() {
        let _ = scope.define(&wrapper, "defaultView", Value::null(), PLAIN);
    }
    Ok(wrapper)
}

fn close_watcher_fire(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let event = ffi::new_event(scope);
    let kind = scope.string("close");
    let _ = scope.set(&event, "type", kind);
    let _ = scope.set(&event, "target", this.clone());
    let dispatch = scope
        .get(this, "dispatchEvent")
        .unwrap_or_else(|_| Value::undefined());
    if scope.is_function(&dispatch) {
        let _ = scope.call(&dispatch, this, &[event]);
    }
    Ok(Value::undefined())
}

fn close_watcher_destroy(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn close_watcher(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let watcher = object_from_new_target(scope, this);
    let listeners = scope.new_array();
    let _ = scope.set(&watcher, "_listeners", listeners);
    let _ = scope.set(&watcher, "oncancel", Value::null());
    let _ = scope.set(&watcher, "onclose", Value::null());
    for (name, arity, method) in [
        ("addEventListener", 2, TargetMethod::AddEventListener),
        ("removeEventListener", 2, TargetMethod::RemoveEventListener),
        ("dispatchEvent", 1, TargetMethod::DispatchEvent),
    ] {
        let function = ffi::target_method(scope, name, arity, method);
        let _ = scope.set(&watcher, name, function);
    }
    for name in ["requestClose", "close"] {
        let function = scope.function(name, 0, close_watcher_fire);
        let _ = scope.set(&watcher, name, function);
    }
    let destroy = scope.function("destroy", 0, close_watcher_destroy);
    let _ = scope.set(&watcher, "destroy", destroy);
    Ok(watcher)
}
