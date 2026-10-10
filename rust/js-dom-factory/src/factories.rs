//! Southstar — the Document node factories: createElement(NS), createTextNode, createComment, createCDATASection, createProcessingInstruction, createAttribute(NS), createDocumentFragment and createEvent.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::node::{self, KIND_COMMENT, KIND_DOCUMENT, KIND_TEXT};
use southstar_dom::{FLAG_FOREIGN_NS, FLAG_PI, FLAG_SVG_NS, Node};
use southstar_glib as glib;
use southstar_js_engine::{Scope, Value};

use crate::adopt::{doc_is_xhtml, doc_is_xml, tag_owner_document};
use crate::ffi::{self, AttrName};
use crate::names::{self, HTML_NS, NOT_SUPPORTED_ERR, SVG_NS};
use crate::namespaces;
use crate::{
    Element, FLAG_CDATA, FLAG_FRAGMENT, FLAG_KEEP_CASE, FLAG_NOT_PARSER_INSERTED, FLAG_XML_DOC,
    HIDDEN, JsResult, c_text, optional_text, split_prefix,
};

fn finish(scope: &mut Scope<'_>, this: &Value, node: Element) -> Value {
    ffi::track_orphan(ffi::js_of(scope), node);
    let wrapper = ffi::wrap(scope, node);
    tag_owner_document(scope, this, &wrapper);
    wrapper
}

fn text_node(kind: core::ffi::c_uint, text: &[u8]) -> Element {
    node::new_text(kind, glib::strdup(text), text.len() as u32)
}

fn require(scope: &mut Scope<'_>, args: &[Value], count: usize, message: &str) -> JsResult<()> {
    if args.len() < count {
        return Err(scope.type_error(message));
    }
    Ok(())
}

pub(crate) fn create_element(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_none() || args.is_empty() {
        return Ok(Value::null());
    }
    let name = scope.to_bytes(&args[0])?;
    if name.contains(&0) || !names::valid_element_local_name(&name) {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"invalid element name"),
        ));
    }
    let is_xml = doc_is_xml(scope, this);
    let html_ns = !is_xml || doc_is_xhtml(scope, this);
    let stored = if is_xml {
        name
    } else {
        name.to_ascii_lowercase()
    };
    let el = node::new_element(glib::strdup(&stored));
    el.add_flags(FLAG_NOT_PARSER_INSERTED);
    if is_xml {
        el.add_flags(FLAG_KEEP_CASE);
        if !html_ns {
            el.add_flags(FLAG_FOREIGN_NS);
        }
    }
    if let Some(options) = args.get(1).filter(|o| o.is_object()) {
        let is = scope.get(options, "is")?;
        if is.is_string() {
            let is = c_text(scope, &is)?;
            if !is.is_empty() {
                ffi::set_attr(el, c"is", &is);
            }
        }
    }
    let wrapper = finish(scope, this, el);
    ffi::ce_upgrade_element(js, el);
    Ok(wrapper)
}

pub(crate) fn create_text_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() || args.is_empty() {
        return Ok(Value::null());
    }
    let text = c_text(scope, &args[0])?;
    let node = text_node(KIND_TEXT, &text);
    Ok(finish(scope, this, node))
}

pub(crate) fn create_element_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() || args.len() < 2 {
        return Ok(Value::null());
    }
    let ns = optional_text(scope, &args[0])?
        .map(|ns| ffi::c_prefix(&ns).to_vec())
        .filter(|ns| !ns.is_empty());
    let name = scope.to_bytes(&args[1])?;
    if let Err(error) = names::validate_element_ns(ns.as_deref(), &name) {
        return Err(ffi::name_error(scope, error));
    }
    let (prefix, local) = split_prefix(&name);
    let is_svg = ns.as_deref() == Some(SVG_NS);
    let is_html = ns.as_deref() == Some(HTML_NS);
    let el = node::new_element(glib::strdup(if is_html { local } else { &name }));
    el.add_flags(FLAG_NOT_PARSER_INSERTED);
    if is_svg {
        el.add_flags(FLAG_SVG_NS);
    } else if !is_html {
        el.add_flags(FLAG_FOREIGN_NS);
    }
    if let Some(ns) = ns.as_deref().filter(|_| !is_html) {
        ffi::set_attr(el, c"data-nd-ns-uri", ns);
    }
    if let Some(prefix) = prefix.filter(|_| is_html) {
        ffi::set_attr(el, c"data-nd-ns-prefix", prefix);
    }
    if doc_is_xml(scope, this) {
        el.add_flags(FLAG_XML_DOC);
    }
    Ok(finish(scope, this, el))
}

pub(crate) fn create_comment(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if ffi::js_of(scope).is_none() || args.is_empty() {
        return Ok(Value::null());
    }
    let text = c_text(scope, &args[0])?;
    let node = text_node(KIND_COMMENT, &text);
    Ok(finish(scope, this, node))
}

pub(crate) fn create_cdata_section(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    require(scope, args, 1, "1 argument required, but only 0 present")?;
    if !doc_is_xml(scope, this) {
        return Err(ffi::dom_exception(
            scope,
            c"NotSupportedError",
            NOT_SUPPORTED_ERR,
            c"createCDATASection is not supported on HTML documents",
        ));
    }
    let data = c_text(scope, &args[0])?;
    if data.windows(3).any(|w| w == b"]]>") {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"CDATA section data must not contain ']]>'"),
        ));
    }
    let node = text_node(KIND_TEXT, &data);
    node.add_flags(FLAG_CDATA);
    Ok(finish(scope, this, node))
}

pub(crate) fn create_processing_instruction(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    require(scope, args, 2, "2 arguments required")?;
    let target = c_text(scope, &args[0])?;
    if !names::is_xml_name(&target) {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"invalid processing instruction target"),
        ));
    }
    let data = c_text(scope, &args[1])?;
    if data.windows(2).any(|w| w == b"?>") {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"processing instruction data must not contain '?>'"),
        ));
    }
    let node = text_node(KIND_COMMENT, &data);
    node.adopt_name(glib::strdup(&target));
    node.add_flags(FLAG_PI);
    ffi::track_orphan(ffi::js_of(scope), node);
    let wrapper = ffi::wrap(scope, node);
    let target = scope.string_from_bytes(&target);
    let _ = scope.define(&wrapper, "target", target, HIDDEN);
    tag_owner_document(scope, this, &wrapper);
    Ok(wrapper)
}

pub(crate) fn attr_node(scope: &mut Scope<'_>, qname: AttrName<'_>) -> JsResult {
    let out = ffi::attr_object(scope, qname)?;
    let methods: [(&str, southstar_js_engine::NativeFn); 3] = [
        ("lookupNamespaceURI", namespaces::attr_lookup_namespace_uri),
        ("lookupPrefix", namespaces::attr_lookup_prefix),
        ("isDefaultNamespace", namespaces::attr_is_default_namespace),
    ];
    for (name, f) in methods {
        let function = scope.function(name, 1, f);
        scope.set(&out, name, function)?;
    }
    Ok(out)
}

pub(crate) fn create_attribute(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    require(scope, args, 1, "1 argument required, but only 0 present")?;
    let raw = scope.to_bytes(&args[0])?;
    if raw.contains(&0) || !names::valid_attr_name(&raw) {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"createAttribute: invalid attribute name"),
        ));
    }
    let name = if doc_is_xml(scope, this) {
        raw
    } else {
        raw.to_ascii_lowercase()
    };
    let out = attr_node(
        scope,
        AttrName {
            namespace_uri: None,
            prefix: None,
            local_name: &name,
            name: &name,
        },
    )?;
    tag_owner_document(scope, this, &out);
    Ok(out)
}

pub(crate) fn create_attribute_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    require(scope, args, 2, "2 arguments required")?;
    let ns = optional_text(scope, &args[0])?.map(|ns| ffi::c_prefix(&ns).to_vec());
    let qname = scope.to_bytes(&args[1])?;
    if qname.contains(&0) {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"invalid qualified name"),
        ));
    }
    if let Err(error) = names::validate_attr_ns(ns.as_deref(), &qname) {
        return Err(ffi::name_error(scope, error));
    }
    let (prefix, local) = split_prefix(&qname);
    let out = attr_node(
        scope,
        AttrName {
            namespace_uri: ns.as_deref(),
            prefix,
            local_name: local,
            name: &qname,
        },
    )?;
    tag_owner_document(scope, this, &out);
    Ok(out)
}

const EVENT_ALIASES: &[(&[u8], &str)] = &[
    (b"beforeunloadevent", "BeforeUnloadEvent"),
    (b"compositionevent", "CompositionEvent"),
    (b"customevent", "CustomEvent"),
    (b"devicemotionevent", "DeviceMotionEvent"),
    (b"deviceorientationevent", "DeviceOrientationEvent"),
    (b"dragevent", "DragEvent"),
    (b"event", "Event"),
    (b"events", "Event"),
    (b"focusevent", "FocusEvent"),
    (b"hashchangeevent", "HashChangeEvent"),
    (b"htmlevents", "Event"),
    (b"keyboardevent", "KeyboardEvent"),
    (b"messageevent", "MessageEvent"),
    (b"mouseevent", "MouseEvent"),
    (b"mouseevents", "MouseEvent"),
    (b"storageevent", "StorageEvent"),
    (b"svgevents", "Event"),
    (b"textevent", "TextEvent"),
    (b"touchevent", "TouchEvent"),
    (b"uievent", "UIEvent"),
    (b"uievents", "UIEvent"),
];

pub(crate) fn create_event(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    require(scope, args, 1, "1 argument required, but only 0 present")?;
    let lower = c_text(scope, &args[0])?.to_ascii_lowercase();
    let interface = EVENT_ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower.as_slice())
        .map(|(_, interface)| *interface);
    let constructor = match interface {
        Some(interface) => {
            let global = scope.global();
            scope.get(&global, interface)?
        }
        None => Value::undefined(),
    };
    if !scope.is_function(&constructor) {
        return Err(ffi::dom_exception(
            scope,
            c"NotSupportedError",
            NOT_SUPPORTED_ERR,
            c"The provided event type is not supported.",
        ));
    }
    let empty = scope.string("");
    let event = scope.construct(&constructor, &[empty])?;
    scope.set(&event, "_is_trusted", Value::boolean(false))?;
    scope.set(&event, "_initialized", Value::boolean(false))?;
    Ok(event)
}

pub(crate) fn create_document_fragment(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    if ffi::js_of(scope).is_none() {
        return Ok(Value::null());
    }
    let fragment = Node::alloc(KIND_DOCUMENT);
    fragment.add_flags(FLAG_FRAGMENT);
    Ok(finish(scope, this, fragment))
}
