//! Southstar — DOMImplementation: the per-document implementation object, createHTMLDocument, createDocument, createDocumentType and hasFeature.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::node::{self, KIND_DOCTYPE, KIND_DOCUMENT, KIND_TEXT};
use southstar_dom::{FLAG_SCRIPTING_DISABLED, Node};
use southstar_glib as glib;
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::adopt::{adopt_owner_walk, doctype_arg, tag_owner_document};
use crate::factories::create_element_ns;
use crate::ffi::{self, RealmDocument};
use crate::names::{self, HTML_NS, SVG_NS};
use crate::{Element, HIDDEN, JsResult, PLAIN, c_text, is_nullish};

fn has_feature(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::boolean(true))
}

pub(crate) fn of_document(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    if this.is_object() {
        let cached = scope.get(this, "__ndImplementation")?;
        if cached.is_object() {
            return Ok(cached);
        }
    }
    let implementation = scope.new_object();
    let global = scope.global();
    let constructor = scope.get(&global, "DOMImplementation")?;
    let proto = scope.get(&constructor, "prototype")?;
    if proto.is_object() {
        scope.set_prototype(&implementation, &proto)?;
    }
    if this.is_object() {
        scope.define(&implementation, "__ndDocument", this.clone(), HIDDEN)?;
    }
    let methods: [(&str, u32, NativeFn); 4] = [
        ("hasFeature", 2, has_feature),
        ("createHTMLDocument", 1, create_html_document),
        ("createDocument", 3, create_document),
        ("createDocumentType", 3, create_document_type),
    ];
    for (name, arity, f) in methods {
        let function = scope.function(name, arity, f);
        scope.set(&implementation, name, function)?;
    }
    if this.is_object() {
        scope.define(this, "__ndImplementation", implementation.clone(), HIDDEN)?;
    }
    Ok(implementation)
}

fn element(name: &[u8]) -> Element {
    node::new_element(glib::strdup(name))
}

pub(crate) fn create_html_document(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(js) = ffi::js_of(scope) else {
        return Ok(Value::null());
    };
    let title = match args.first().filter(|t| !t.is_undefined()) {
        Some(title) => Some(c_text(scope, title)?),
        None => None,
    };
    let doc = Node::alloc(KIND_DOCUMENT);
    doc.add_flags(FLAG_SCRIPTING_DISABLED);
    let doctype = element(b"html");
    ffi::set_attr(doctype, c"publicId", b"");
    ffi::set_attr(doctype, c"systemId", b"");
    doctype.set_kind(KIND_DOCTYPE);
    let html = element(b"html");
    let head = element(b"head");
    let body = element(b"body");
    doc.append(doctype);
    doc.append(html);
    html.append(head);
    if let Some(title) = title {
        let title_el = element(b"title");
        title_el.append(node::new_text(
            KIND_TEXT,
            glib::strdup(&title),
            title.len() as u32,
        ));
        head.append(title_el);
    }
    html.append(body);
    ffi::track_orphan(Some(js), doc);
    let wrapper = ffi::realm_document(
        scope,
        doc,
        RealmDocument {
            url: Some(b"about:blank"),
            charset: Some(b"UTF-8"),
            content_type: Some(b"text/html"),
            is_xml: false,
        },
    );
    if wrapper.is_object() {
        let _ = scope.define(&wrapper, "defaultView", Value::null(), PLAIN);
        adopt_owner_walk(scope, &wrapper, doc);
    }
    Ok(wrapper)
}

pub(crate) fn create_document_type(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(js) = ffi::js_of(scope) else {
        return Ok(Value::null());
    };
    if args.len() < 3 {
        return Err(scope.type_error("3 arguments required"));
    }
    let name = scope.to_bytes(&args[0])?;
    if !names::valid_doctype_name(&name) {
        return Err(ffi::name_error(
            scope,
            names::invalid_character(c"invalid doctype name"),
        ));
    }
    let public_id = c_text(scope, &args[1])?;
    let system_id = c_text(scope, &args[2])?;
    let doctype = element(&name);
    ffi::set_attr(doctype, c"publicId", &public_id);
    ffi::set_attr(doctype, c"systemId", &system_id);
    doctype.set_kind(KIND_DOCTYPE);
    ffi::track_orphan(Some(js), doctype);
    let wrapper = ffi::wrap(scope, doctype);
    let owner = scope.get(this, "__ndDocument")?;
    if owner.is_object() {
        tag_owner_document(scope, &owner, &wrapper);
    }
    Ok(wrapper)
}

fn append_child(scope: &mut Scope<'_>, doc: &Value, child: &Value) -> JsResult<()> {
    let append = scope.get(doc, "appendChild")?;
    scope.call(&append, doc, core::slice::from_ref(child))?;
    Ok(())
}

pub(crate) fn create_document(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Err(scope.type_error("2 arguments required"));
    }
    let wrapper = ffi::synth_xml_document(scope);
    if !wrapper.is_object() {
        return Ok(wrapper);
    }
    if !is_nullish(&args[0]) {
        let ns = c_text(scope, &args[0])?;
        let content_type = match ns.as_slice() {
            HTML_NS => "application/xhtml+xml",
            SVG_NS => "image/svg+xml",
            _ => "application/xml",
        };
        let content_type = scope.string(content_type);
        scope.define(&wrapper, "contentType", content_type, PLAIN)?;
    }
    let has_root = !args[1].is_null() && !scope.to_bytes(&args[1])?.is_empty();
    let root = if has_root {
        Some(create_element_ns(scope, &wrapper, &args[..2])?)
    } else {
        None
    };
    if let Some(doctype) = args.get(2).filter(|d| !is_nullish(d)) {
        if doctype_arg(args, 2).is_none() {
            return Err(scope.type_error("doctype must be a DocumentType"));
        }
        append_child(scope, &wrapper, doctype)?;
    }
    if let Some(root) = root {
        append_child(scope, &wrapper, &root)?;
    }
    Ok(wrapper)
}
