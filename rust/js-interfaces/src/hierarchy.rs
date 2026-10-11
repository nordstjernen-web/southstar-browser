//! Southstar — the prototype chains of the node interfaces and the per-tag HTML element prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, DomProtos, Members};
use crate::{DATA_PROPERTY, chain_proto, define_element_unscopables, instof, proto_of};

const ELEMENT_ONLY_METHODS: &[&str] = &["matches", "closest", "webkitMatchesSelector"];

const PARENT_QUERY_METHODS: &[&str] = &["querySelector", "querySelectorAll"];

const DOCUMENT_LIKE: &[&str] = &[
    "Document",
    "HTMLDocument",
    "XMLDocument",
    "DocumentFragment",
];

const ELEMENT_SPEC_METHODS: &[&str] = &[
    "setAttribute",
    "getAttribute",
    "hasAttribute",
    "removeAttribute",
    "toggleAttribute",
    "getAttributeNS",
    "setAttributeNS",
    "removeAttributeNS",
    "hasAttributeNS",
    "getAttributeNames",
    "attachShadow",
    "querySelector",
    "querySelectorAll",
    "getElementsByTagName",
    "getElementsByClassName",
    "insertAdjacentElement",
    "insertAdjacentHTML",
    "insertAdjacentText",
    "getBoundingClientRect",
    "getClientRects",
    "scrollIntoView",
    "append",
    "prepend",
    "before",
    "after",
    "remove",
    "replaceWith",
    "replaceChildren",
];

const ELEMENT_SPEC_ACCESSORS: &[&str] = &[
    "children",
    "firstElementChild",
    "lastElementChild",
    "childElementCount",
    "nextElementSibling",
    "previousElementSibling",
    "innerHTML",
    "outerHTML",
    "id",
    "className",
    "classList",
    "attributes",
    "tagName",
    "localName",
    "namespaceURI",
    "scrollTop",
    "scrollLeft",
    "scrollWidth",
    "scrollHeight",
    "clientTop",
    "clientLeft",
    "clientWidth",
    "clientHeight",
    "shadowRoot",
    "assignedSlot",
];

const MISPLACED_ELEMENT_MEMBERS: &[&str] =
    &["attachShadow", "shadowRoot", "assignedSlot", "default"];

struct TagMembers {
    tag: &'static str,
    text: bool,
    data: bool,
    value: bool,
}

const fn tag_members(tag: &'static str, text: bool, data: bool, value: bool) -> TagMembers {
    TagMembers {
        tag,
        text,
        data,
        value,
    }
}

const TAG_MEMBERS: &[TagMembers] = &[
    tag_members("option", true, false, true),
    tag_members("a", true, false, false),
    tag_members("script", true, false, false),
    tag_members("title", true, false, false),
    tag_members("object", false, true, true),
    tag_members("input", false, false, true),
    tag_members("select", false, false, true),
    tag_members("textarea", false, false, true),
    tag_members("button", false, false, true),
    tag_members("output", false, false, true),
    tag_members("progress", false, false, true),
    tag_members("meter", false, false, true),
    tag_members("li", false, false, true),
    tag_members("param", false, false, true),
    tag_members("data", false, false, true),
];

const WRITABLE_ONLY: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

fn delete_names(scope: &mut Scope<'_>, proto: &Value, names: &[&str]) {
    if !proto.is_object() {
        return;
    }
    for name in names {
        let _ = scope.delete(proto, name);
    }
}

fn delete_from_document_likes(scope: &mut Scope<'_>, global: &Value, names: &[&str]) {
    for name in DOCUMENT_LIKE {
        let proto = proto_of(scope, global, name);
        delete_names(scope, &proto, names);
    }
}

fn set_ctor_proto(scope: &mut Scope<'_>, global: &Value, ctor_name: &str, proto: &Value) {
    let Ok(ctor) = scope.get(global, ctor_name) else {
        return;
    };
    if ctor.is_object() && proto.is_object() {
        let _ = scope.define(&ctor, "prototype", proto.clone(), WRITABLE_ONLY);
        let _ = scope.define(proto, "constructor", ctor, Attributes::METHOD);
    }
}

fn copy_element_members(scope: &mut Scope<'_>, node: &Value, element: &Value) {
    for name in ELEMENT_SPEC_METHODS {
        if let Ok(function) = scope.get(node, name)
            && scope.is_function(&function)
        {
            let _ = scope.define(element, name, function, Attributes::METHOD);
        }
    }
    for name in ELEMENT_SPEC_ACCESSORS {
        let key = scope.string(name);
        let Ok(Some(descriptor)) = scope.own_property(node, &key) else {
            continue;
        };
        if descriptor.accessor {
            let _ = scope.define_accessor(
                element,
                name,
                Some(&descriptor.getter),
                Some(&descriptor.setter),
                Attributes::CONFIGURABLE,
            );
        } else {
            let _ = scope.define(element, name, descriptor.value, Attributes::METHOD);
        }
    }
}

fn move_parent_node_members(scope: &mut Scope<'_>, global: &Value, node: &Value) {
    delete_names(scope, node, ELEMENT_ONLY_METHODS);
    delete_names(scope, node, PARENT_QUERY_METHODS);
    delete_from_document_likes(scope, global, ELEMENT_ONLY_METHODS);
    if let Ok(move_before) = scope.get(node, "moveBefore")
        && scope.is_function(&move_before)
    {
        for name in DOCUMENT_LIKE {
            let proto = proto_of(scope, global, name);
            if proto.is_object() {
                let _ = scope.define(&proto, "moveBefore", move_before.clone(), DATA_PROPERTY);
            }
        }
    }
    delete_names(scope, node, &["moveBefore"]);
}

fn new_proto(scope: &mut Scope<'_>, parent: &Value) -> Value {
    let proto = scope.new_object();
    let _ = scope.set_prototype(&proto, parent);
    proto
}

fn with_members(scope: &mut Scope<'_>, global: &Value, ctor_name: &str, members: Members) {
    let proto = proto_of(scope, global, ctor_name);
    if proto.is_object() {
        ffi::install_members(scope, &proto, members);
    }
}

fn install_tag_protos(scope: &mut Scope<'_>, global: &Value, html_element: &Value) {
    let mut by_tag: Vec<(&'static str, Value)> = Vec::new();
    for (ctor, tags) in instof::tag_interfaces() {
        let proto = proto_of(scope, global, ctor);
        if !proto.is_object() {
            continue;
        }
        let _ = scope.set_prototype(&proto, html_element);
        for tag in tags.split(' ').filter(|tag| !tag.is_empty()) {
            ffi::register_tag_proto(scope, tag, &proto);
            by_tag.retain(|(known, _)| *known != tag);
            by_tag.push((tag, proto.clone()));
        }
    }

    let media = proto_of(scope, global, "HTMLMediaElement");
    if media.is_object() {
        let _ = scope.set_prototype(&media, html_element);
        chain_proto(scope, global, "HTMLAudioElement", &media);
        chain_proto(scope, global, "HTMLVideoElement", &media);
    }

    for entry in TAG_MEMBERS {
        let known = by_tag
            .iter()
            .find(|(tag, _)| *tag == entry.tag)
            .map(|(_, proto)| proto.clone());
        let is_new = known.is_none();
        let proto = known.unwrap_or_else(|| new_proto(scope, html_element));
        if entry.text {
            ffi::install_members(scope, &proto, Members::TextProperty);
        }
        if entry.data {
            ffi::install_members(scope, &proto, Members::DataProperty);
        }
        if entry.value {
            ffi::install_members(scope, &proto, Members::ValueProperty);
        }
        if entry.tag == "select" {
            ffi::install_members(scope, &proto, Members::OptionsMethods);
        }
        if is_new {
            ffi::register_tag_proto(scope, entry.tag, &proto);
        }
    }
}

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    let node = proto_of(scope, global, "Node");
    if !node.is_object() {
        return;
    }

    let element = new_proto(scope, &node);
    ffi::install_members(scope, &element, Members::ElementFunctions);
    define_element_unscopables(scope, &element);
    ffi::install_members(scope, &element, Members::ElementClass);
    copy_element_members(scope, &node, &element);
    move_parent_node_members(scope, global, &node);

    set_ctor_proto(scope, global, "Node", &node);
    set_ctor_proto(scope, global, "Element", &element);

    let html_element = new_proto(scope, &element);
    set_ctor_proto(scope, global, "HTMLElement", &html_element);
    ffi::install_members(scope, &html_element, Members::HtmlElement);
    chain_proto(scope, global, "SVGElement", &element);
    chain_proto(scope, global, "SVGSVGElement", &element);
    let svg_element = proto_of(scope, global, "SVGElement");
    chain_proto(scope, global, "SVGAElement", &svg_element);
    let svga_element = proto_of(scope, global, "SVGAElement");
    if svg_element.is_object() {
        ffi::install_members(scope, &svg_element, Members::Dataset);
    }
    chain_proto(scope, global, "MathMLElement", &element);
    let mathml_element = proto_of(scope, global, "MathMLElement");
    if mathml_element.is_object() {
        ffi::install_members(scope, &mathml_element, Members::Dataset);
    }

    let chardata = proto_of(scope, global, "CharacterData");
    if chardata.is_object() {
        let _ = scope.set_prototype(&chardata, &node);
        chain_proto(scope, global, "Text", &chardata);
        chain_proto(scope, global, "Comment", &chardata);
        chain_proto(scope, global, "ProcessingInstruction", &chardata);
    }
    let text = proto_of(scope, global, "Text");
    let comment = proto_of(scope, global, "Comment");
    let pi = proto_of(scope, global, "ProcessingInstruction");
    let cdata = proto_of(scope, global, "CDATASection");
    let doctype = proto_of(scope, global, "DocumentType");
    let docfrag = proto_of(scope, global, "DocumentFragment");
    if text.is_object() {
        chain_proto(scope, global, "CDATASection", &text);
        ffi::install_members(scope, &text, Members::Text);
    }
    if doctype.is_object() {
        let _ = scope.set_prototype(&doctype, &node);
    }
    if docfrag.is_object() {
        let _ = scope.set_prototype(&docfrag, &node);
        ffi::install_members(scope, &docfrag, Members::DocumentFragment);
    }
    let shadow = proto_of(scope, global, "ShadowRoot");
    if shadow.is_object() && docfrag.is_object() {
        let _ = scope.set_prototype(&shadow, &docfrag);
    }

    delete_names(scope, &node, MISPLACED_ELEMENT_MEMBERS);
    delete_from_document_likes(scope, global, MISPLACED_ELEMENT_MEMBERS);

    let document = proto_of(scope, global, "Document");
    if document.is_object() {
        let _ = scope.set_prototype(&document, &node);
    }
    let html_document = proto_of(scope, global, "HTMLDocument");
    if html_document.is_object() && document.is_object() {
        let _ = scope.set_prototype(&html_document, &document);
    }
    let document = if html_document.is_object() {
        html_document
    } else {
        document
    };

    let html_unknown_element = proto_of(scope, global, "HTMLUnknownElement");
    if html_unknown_element.is_object() {
        let _ = scope.set_prototype(&html_unknown_element, &html_element);
    }
    let attr = proto_of(scope, global, "Attr");
    if attr.is_object() {
        let _ = scope.set_prototype(&attr, &node);
    }

    with_members(scope, global, "HTMLTrackElement", Members::Track);
    with_members(scope, global, "HTMLDialogElement", Members::Dialog);
    with_members(scope, global, "HTMLButtonElement", Members::Button);

    install_tag_protos(scope, global, &html_element);
    if chardata.is_object() {
        ffi::install_members(scope, &chardata, Members::DataProperty);
    }

    ffi::adopt_dom_protos(
        scope,
        DomProtos {
            node,
            element,
            html_element,
            html_unknown_element,
            svg_element,
            svga_element,
            mathml_element,
            chardata,
            text,
            comment,
            cdata,
            pi,
            doctype,
            docfrag,
            document,
        },
    );
}
