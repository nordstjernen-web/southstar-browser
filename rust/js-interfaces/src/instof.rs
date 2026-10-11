//! Southstar — the DOM interface table, each interface's [Symbol.hasInstance] matching wrappers by node kind, and the toStringTag of each prototype.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, Node};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{JsResult, ffi, proto_of, well_known_symbol};

const FLAG_FRAGMENT: u32 = 1 << 2;
const SHADOW_ATTR: &core::ffi::CStr = c"data-nd-shadow-root";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Match {
    Tag(&'static str),
    Node,
    Element,
    Document,
    Fragment,
    Text,
    Comment,
    CharData,
    Doctype,
    HtmlCollection,
    NodeList,
    HtmlElement,
    Shadow,
}

pub(crate) const INTERFACES: &[(&str, Match)] = &[
    ("HTMLAnchorElement", Match::Tag("a")),
    ("HTMLAreaElement", Match::Tag("area")),
    ("HTMLAudioElement", Match::Tag("audio")),
    ("HTMLBRElement", Match::Tag("br")),
    ("HTMLBaseElement", Match::Tag("base")),
    ("HTMLBodyElement", Match::Tag("body")),
    ("HTMLButtonElement", Match::Tag("button")),
    ("HTMLCanvasElement", Match::Tag("canvas")),
    ("HTMLDListElement", Match::Tag("dl")),
    ("HTMLDataElement", Match::Tag("data")),
    ("HTMLDataListElement", Match::Tag("datalist")),
    ("HTMLDetailsElement", Match::Tag("details")),
    ("HTMLDialogElement", Match::Tag("dialog")),
    ("HTMLDivElement", Match::Tag("div")),
    ("HTMLEmbedElement", Match::Tag("embed")),
    ("HTMLFieldSetElement", Match::Tag("fieldset")),
    ("HTMLFormElement", Match::Tag("form")),
    ("HTMLHRElement", Match::Tag("hr")),
    ("HTMLHeadElement", Match::Tag("head")),
    ("HTMLHeadingElement", Match::Tag("h1 h2 h3 h4 h5 h6")),
    ("HTMLHtmlElement", Match::Tag("html")),
    ("HTMLIFrameElement", Match::Tag("iframe")),
    ("HTMLDirectoryElement", Match::Tag("dir")),
    ("HTMLFontElement", Match::Tag("font")),
    ("HTMLFrameElement", Match::Tag("frame")),
    ("HTMLFrameSetElement", Match::Tag("frameset")),
    ("HTMLParamElement", Match::Tag("param")),
    ("HTMLMarqueeElement", Match::Tag("marquee")),
    ("HTMLImageElement", Match::Tag("img")),
    ("HTMLInputElement", Match::Tag("input")),
    ("HTMLLIElement", Match::Tag("li")),
    ("HTMLLabelElement", Match::Tag("label")),
    ("HTMLLegendElement", Match::Tag("legend")),
    ("HTMLLinkElement", Match::Tag("link")),
    ("HTMLMapElement", Match::Tag("map")),
    ("HTMLMenuElement", Match::Tag("menu")),
    ("HTMLMetaElement", Match::Tag("meta")),
    ("HTMLMeterElement", Match::Tag("meter")),
    ("HTMLModElement", Match::Tag("ins del")),
    ("HTMLOListElement", Match::Tag("ol")),
    ("HTMLObjectElement", Match::Tag("object")),
    ("HTMLOptGroupElement", Match::Tag("optgroup")),
    ("HTMLOptionElement", Match::Tag("option")),
    ("HTMLOutputElement", Match::Tag("output")),
    ("HTMLParagraphElement", Match::Tag("p")),
    ("HTMLPictureElement", Match::Tag("picture")),
    ("HTMLPreElement", Match::Tag("pre listing xmp")),
    ("HTMLProgressElement", Match::Tag("progress")),
    ("HTMLQuoteElement", Match::Tag("q blockquote")),
    ("HTMLScriptElement", Match::Tag("script")),
    ("HTMLSelectElement", Match::Tag("select")),
    ("HTMLSlotElement", Match::Tag("slot")),
    ("HTMLSourceElement", Match::Tag("source")),
    ("HTMLSpanElement", Match::Tag("span")),
    ("HTMLStyleElement", Match::Tag("style")),
    ("HTMLTableCaptionElement", Match::Tag("caption")),
    ("HTMLTableCellElement", Match::Tag("td th")),
    ("HTMLTableColElement", Match::Tag("col colgroup")),
    ("HTMLTableElement", Match::Tag("table")),
    ("HTMLTableRowElement", Match::Tag("tr")),
    ("HTMLTableSectionElement", Match::Tag("thead tbody tfoot")),
    ("HTMLTemplateElement", Match::Tag("template")),
    ("HTMLTextAreaElement", Match::Tag("textarea")),
    ("HTMLTimeElement", Match::Tag("time")),
    ("HTMLTitleElement", Match::Tag("title")),
    ("HTMLTrackElement", Match::Tag("track")),
    ("HTMLUListElement", Match::Tag("ul")),
    ("HTMLVideoElement", Match::Tag("video")),
    ("Node", Match::Node),
    ("Element", Match::Element),
    ("HTMLElement", Match::Element),
    ("HTMLDocument", Match::Document),
    ("Document", Match::Document),
    ("HTMLCollection", Match::HtmlCollection),
    ("NodeList", Match::NodeList),
    ("DocumentFragment", Match::Fragment),
    ("Text", Match::Text),
    ("CDATASection", Match::Text),
    ("ProcessingInstruction", Match::CharData),
    ("Comment", Match::Comment),
    ("CharacterData", Match::CharData),
    ("DocumentType", Match::Doctype),
    ("HTMLElement", Match::HtmlElement),
    ("ShadowRoot", Match::Shadow),
];

const BASE_INTERFACES: &[&str] = &[
    "HTMLElement",
    "HTMLUnknownElement",
    "Element",
    "Node",
    "CharacterData",
    "Text",
    "Comment",
    "CDATASection",
    "ProcessingInstruction",
    "Document",
    "HTMLDocument",
    "XMLDocument",
    "DocumentFragment",
    "ShadowRoot",
    "DocumentType",
    "Attr",
    "SVGElement",
    "SVGAElement",
    "SVGSVGElement",
    "MathMLElement",
    "Event",
    "UIEvent",
    "MouseEvent",
    "KeyboardEvent",
    "FocusEvent",
    "InputEvent",
    "CompositionEvent",
    "TextEvent",
    "TouchEvent",
    "PointerEvent",
    "WheelEvent",
    "DragEvent",
    "CustomEvent",
    "ProgressEvent",
    "ErrorEvent",
    "MessageEvent",
    "CloseEvent",
    "PopStateEvent",
    "HashChangeEvent",
    "StorageEvent",
    "AnimationEvent",
    "TransitionEvent",
    "ClipboardEvent",
    "SubmitEvent",
    "BeforeUnloadEvent",
];

pub(crate) fn tag_interfaces() -> impl Iterator<Item = (&'static str, &'static str)> {
    INTERFACES.iter().filter_map(|&(ctor, kind)| match kind {
        Match::Tag(tags) => Some((ctor, tags)),
        _ => None,
    })
}

fn is_html(node: Node<'_>) -> bool {
    node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
}

fn tag_match(tags: &str, node: Node<'_>) -> bool {
    if node.kind() != Kind::Element || !is_html(node) {
        return false;
    }
    node.name()
        .is_some_and(|name| tags.split(' ').any(|tag| tag.as_bytes() == name.to_bytes()))
}

fn is_shadow_host_attr(node: Node<'_>) -> bool {
    node.kind() == Kind::Element && node.attr(SHADOW_ATTR).is_some()
}

fn kind_match(kind: Match, node: Node<'_>) -> bool {
    match kind {
        Match::Node => true,
        Match::Tag(tags) => tag_match(tags, node),
        Match::Text => node.kind() == Kind::Text,
        Match::Comment => node.kind() == Kind::Comment,
        Match::CharData => matches!(node.kind(), Kind::Text | Kind::Comment),
        Match::Doctype => node.kind() == Kind::Doctype,
        Match::Element => node.kind() == Kind::Element,
        Match::Document => node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT == 0,
        Match::Fragment => node.flags() & FLAG_FRAGMENT != 0 || is_shadow_host_attr(node),
        Match::Shadow => is_shadow_host_attr(node),
        Match::HtmlElement => node.kind() == Kind::Element && is_html(node),
        Match::HtmlCollection | Match::NodeList => false,
    }
}

fn node_match(kind: Match, node: Node<'_>) -> bool {
    if ffi::is_shadow_root(node) {
        return matches!(kind, Match::Node | Match::Fragment | Match::Shadow);
    }
    kind_match(kind, node)
}

fn collection_match(scope: &mut Scope<'_>, kind: Match, value: &Value) -> bool {
    let live = ffi::live_collection_kind(value);
    if kind == Match::HtmlCollection {
        return live == 1;
    }
    if live == 0 {
        return true;
    }
    if !value.is_object() {
        return false;
    }
    scope
        .get(value, "__nsNodeList")
        .is_ok_and(|marker| scope.to_bool(&marker))
}

fn is_entry_ctor(scope: &mut Scope<'_>, ctor: &Value, name: &str) -> bool {
    if !ctor.is_object() {
        return false;
    }
    match scope.get(ctor, "name") {
        Ok(value) if value.is_string() => scope.to_string(&value).is_ok_and(|n| n == name),
        _ => false,
    }
}

fn ordinary_has_instance(scope: &mut Scope<'_>, ctor: &Value, value: &Value) -> JsResult {
    if !value.is_object() || !scope.is_function(ctor) {
        return Ok(Value::boolean(false));
    }
    let proto = scope.get(ctor, "prototype")?;
    if !proto.is_object() {
        return Err(scope.type_error("operand 'prototype' property is not an object"));
    }
    let mut current = value.clone();
    loop {
        let next = scope.get_prototype(&current)?;
        if !next.is_object() {
            return Ok(Value::boolean(false));
        }
        if next.same_object(&proto) {
            return Ok(Value::boolean(true));
        }
        current = next;
    }
}

fn has_instance(scope: &mut Scope<'_>, this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let Some(value) = args.first() else {
        return Ok(Value::boolean(false));
    };
    let index = match data.first() {
        Some(index) => scope.to_int32(index)?,
        None => -1,
    };
    let Some(&(ctor, kind)) = usize::try_from(index).ok().and_then(|i| INTERFACES.get(i)) else {
        return Ok(Value::boolean(false));
    };
    if scope.is_native_function(this) && !is_entry_ctor(scope, this, ctor) {
        return ordinary_has_instance(scope, this, value);
    }
    if matches!(kind, Match::HtmlCollection | Match::NodeList) {
        return Ok(Value::boolean(collection_match(scope, kind, value)));
    }
    match ffi::unwrap_node(value) {
        Some(node) => Ok(Value::boolean(node_match(kind, node))),
        None => ordinary_has_instance(scope, this, value),
    }
}

pub fn install_has_instance(scope: &mut Scope<'_>, global: &Value) {
    let Some(key) = well_known_symbol(scope, "hasInstance") else {
        return;
    };
    for (index, &(ctor_name, _)) in INTERFACES.iter().enumerate() {
        let Ok(ctor) = scope.get(global, ctor_name) else {
            continue;
        };
        if !ctor.is_object() {
            continue;
        }
        let function = scope.bound_function(
            "[Symbol.hasInstance]",
            1,
            has_instance,
            &[Value::int(index as i32)],
        );
        let _ = scope.define_with_key(&ctor, &key, function, Attributes::CONFIGURABLE);
    }
}

pub fn install_to_string_tag(scope: &mut Scope<'_>, global: &Value) {
    if well_known_symbol(scope, "toStringTag").is_none() {
        return;
    }
    let tag_ctors = tag_interfaces().map(|(ctor, _)| ctor);
    for name in BASE_INTERFACES.iter().copied().chain(tag_ctors) {
        let proto = proto_of(scope, global, name);
        if proto.is_object() {
            let _ = scope.define_to_string_tag(&proto, name);
        }
    }
}
