//! Southstar — the interface objects a fresh document's window gets: constructor stubs, prototype member tables, host class prototypes and the platform installers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_bind::{attach_prototype, bind_function, bind_int_constants, prototype_of};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, HostProto, Installer, Native, ProtoTable, ValueInstaller};

const SHIM_CONSTRUCTORS: &[(&str, u32)] = &[
    ("XMLDocument", 0),
    ("XSLTProcessor", 0),
    ("Range", 0),
    ("NodeFilter", 0),
    ("DOMTokenList", 0),
    ("NodeList", 0),
    ("HTMLCollection", 0),
    ("CSSStyleSheet", 0),
    ("CSSStyleDeclaration", 0),
    ("CSSRule", 0),
    ("CSSStyleRule", 0),
    ("MediaList", 0),
    ("MediaQueryList", 0),
    ("Selection", 0),
    ("Animation", 0),
    ("FileList", 0),
    ("HTMLInputElement", 0),
    ("HTMLAnchorElement", 0),
    ("HTMLImageElement", 0),
    ("HTMLFormElement", 0),
    ("HTMLSelectElement", 0),
    ("HTMLOptionElement", 0),
    ("HTMLButtonElement", 0),
    ("HTMLDivElement", 0),
    ("HTMLSpanElement", 0),
    ("HTMLTableElement", 0),
    ("HTMLTableRowElement", 0),
    ("HTMLTableCellElement", 0),
    ("HTMLLabelElement", 0),
    ("HTMLTextAreaElement", 0),
    ("HTMLVideoElement", 0),
    ("HTMLAudioElement", 0),
    ("HTMLMediaElement", 0),
    ("HTMLDialogElement", 0),
    ("HTMLDetailsElement", 0),
    ("HTMLScriptElement", 0),
    ("HTMLLinkElement", 0),
    ("HTMLMetaElement", 0),
    ("HTMLStyleElement", 0),
    ("HTMLBodyElement", 0),
    ("HTMLHtmlElement", 0),
    ("HTMLHeadElement", 0),
    ("HTMLIFrameElement", 0),
    ("HTMLCanvasElement", 0),
    ("HTMLAreaElement", 0),
    ("HTMLBaseElement", 0),
    ("HTMLBRElement", 0),
    ("HTMLDataElement", 0),
    ("HTMLDataListElement", 0),
    ("HTMLDListElement", 0),
    ("HTMLEmbedElement", 0),
    ("HTMLFieldSetElement", 0),
    ("HTMLHeadingElement", 0),
    ("HTMLHRElement", 0),
    ("HTMLLegendElement", 0),
    ("HTMLLIElement", 0),
    ("HTMLMapElement", 0),
    ("HTMLMenuElement", 0),
    ("HTMLMeterElement", 0),
    ("HTMLModElement", 0),
    ("HTMLObjectElement", 0),
    ("HTMLOListElement", 0),
    ("HTMLOptGroupElement", 0),
    ("HTMLOutputElement", 0),
    ("HTMLParagraphElement", 0),
    ("HTMLPictureElement", 0),
    ("HTMLPreElement", 0),
    ("HTMLProgressElement", 0),
    ("HTMLQuoteElement", 0),
    ("HTMLSlotElement", 0),
    ("HTMLSourceElement", 0),
    ("HTMLTableCaptionElement", 0),
    ("HTMLTableColElement", 0),
    ("HTMLTableSectionElement", 0),
    ("HTMLTemplateElement", 0),
    ("HTMLTimeElement", 0),
    ("HTMLTitleElement", 0),
    ("HTMLTrackElement", 0),
    ("HTMLUListElement", 0),
    ("HTMLUnknownElement", 0),
    ("HTMLFontElement", 0),
    ("HTMLMarqueeElement", 0),
    ("HTMLFrameElement", 0),
    ("HTMLFrameSetElement", 0),
    ("HTMLParamElement", 0),
    ("HTMLDirectoryElement", 0),
    ("CharacterData", 0),
    ("CDATASection", 0),
    ("ProcessingInstruction", 0),
    ("Attr", 0),
    ("DocumentType", 0),
    ("HTMLOptionsCollection", 0),
    ("HTMLAllCollection", 0),
    ("RadioNodeList", 0),
    ("ValidityState", 0),
    ("DOMStringList", 0),
    ("DOMStringMap", 0),
    ("NamedNodeMap", 0),
    ("TreeWalker", 0),
    ("NodeIterator", 0),
    ("MutationRecord", 0),
    ("IntersectionObserverEntry", 0),
    ("ResizeObserverEntry", 0),
    ("PerformanceEntry", 0),
    ("PerformanceMark", 0),
    ("PerformanceMeasure", 0),
    ("PerformanceResourceTiming", 0),
    ("PerformanceNavigationTiming", 0),
    ("FontFaceSet", 0),
    ("ReadableStream", 1),
    ("WritableStream", 1),
    ("TransformStream", 1),
    ("ByteLengthQueuingStrategy", 1),
    ("CountQueuingStrategy", 1),
    ("Geolocation", 0),
    ("Permissions", 0),
    ("Crypto", 0),
    ("SubtleCrypto", 0),
    ("CryptoKey", 0),
];

const NAVIGATOR_CONSTRUCTORS: &[&str] = &[
    "NavigatorUAData",
    "PluginArray",
    "MimeTypeArray",
    "Plugin",
    "MimeType",
    "MediaDevices",
    "MediaCapabilities",
    "NetworkInformation",
    "UserActivation",
    "StorageManager",
    "WakeLock",
];

const MEDIA_CONSTANTS: &[(&str, i32)] = &[
    ("NETWORK_EMPTY", 0),
    ("NETWORK_IDLE", 1),
    ("NETWORK_LOADING", 2),
    ("NETWORK_NO_SOURCE", 3),
    ("HAVE_NOTHING", 0),
    ("HAVE_METADATA", 1),
    ("HAVE_CURRENT_DATA", 2),
    ("HAVE_FUTURE_DATA", 3),
    ("HAVE_ENOUGH_DATA", 4),
];

const ELEMENT_TABLES: &[(&str, ProtoTable)] = &[
    ("HTMLScriptElement", ProtoTable::Src),
    ("HTMLImageElement", ProtoTable::Image),
    ("HTMLSourceElement", ProtoTable::Source),
    ("HTMLTrackElement", ProtoTable::Src),
    ("HTMLMediaElement", ProtoTable::Media),
    ("HTMLVideoElement", ProtoTable::Video),
    ("HTMLIFrameElement", ProtoTable::Iframe),
    ("HTMLEmbedElement", ProtoTable::Src),
    ("HTMLFrameElement", ProtoTable::Src),
    ("HTMLAnchorElement", ProtoTable::Anchor),
    ("HTMLAreaElement", ProtoTable::Href),
    ("HTMLLinkElement", ProtoTable::Href),
];

const HOST_PROTOS: &[(&str, HostProto)] = &[
    ("CSSStyleDeclaration", HostProto::Style),
    ("DOMStringMap", HostProto::Dataset),
    ("DOMTokenList", HostProto::TokenList),
];

const EVENT_HANDLER_TARGETS: &[&str] = &[
    "HTMLElement",
    "SVGElement",
    "MathMLElement",
    "Document",
    "Window",
];

const NAMED_NODE_MAP_ITERATION: &str = "(function(p){ var A=Array.prototype; function d(k,v){Object.defineProperty(p,k,{value:v,writable:true,configurable:true});} d('entries',A.entries);d('keys',A.keys);d('values',A.values);d('forEach',A.forEach); Object.defineProperty(p,Symbol.iterator,{value:A.values,writable:true,configurable:true});})(NamedNodeMap.prototype)";

const PROTOTYPE_SLOT: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

fn bind_constructor(scope: &mut Scope<'_>, global: &Value, name: &str, arity: u32, native: Native) {
    let constructor = ffi::native_constructor(scope, name, arity, native);
    let _ = attach_prototype(scope, &constructor, name);
    let _ = scope.set(global, name, constructor);
}

fn install_table_on(scope: &mut Scope<'_>, global: &Value, name: &str, table: ProtoTable) -> Value {
    let proto = prototype_of(scope, global, name);
    if proto.is_object() {
        ffi::install_proto_table(scope, &proto, table);
    }
    proto
}

fn install_named_node_map(scope: &mut Scope<'_>, global: &Value) {
    let proto = install_table_on(scope, global, "NamedNodeMap", ProtoTable::NamedNodeMap);
    if proto.is_object() {
        let getter = ffi::native(scope, "get length", 0, Native::LiveLength);
        let _ = scope.define_accessor(
            &proto,
            "length",
            Some(&getter),
            None,
            Attributes::CONFIGURABLE,
        );
    }
    let _ = scope.eval_native_script(NAMED_NODE_MAP_ITERATION, "<namednodemap>");
}

fn link_host_proto(scope: &mut Scope<'_>, global: &Value, name: &str, which: HostProto) {
    let constructor = scope
        .get(global, name)
        .unwrap_or_else(|_| Value::undefined());
    let proto = ffi::host_class_proto(scope, which);
    if constructor.is_object() && proto.is_object() {
        let _ = scope.define(
            &proto,
            "constructor",
            constructor.clone(),
            Attributes::METHOD,
        );
        let _ = scope.define(&constructor, "prototype", proto, PROTOTYPE_SLOT);
    }
}

fn link_xml_document(scope: &mut Scope<'_>, global: &Value) {
    let prototype = |scope: &mut Scope<'_>, name: &str| {
        let constructor = scope
            .get(global, name)
            .unwrap_or_else(|_| Value::undefined());
        scope
            .get(&constructor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    };
    let document_proto = prototype(scope, "Document");
    let xml_proto = prototype(scope, "XMLDocument");
    if document_proto.is_object() && xml_proto.is_object() {
        let _ = scope.set_prototype(&xml_proto, &document_proto);
    }
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    for &(name, arity) in SHIM_CONSTRUCTORS {
        bind_constructor(scope, global, name, arity, Native::EventConstructor);
    }
    install_named_node_map(scope, global);
    bind_constructor(scope, global, "ShadowRoot", 0, Native::IllegalConstructor);
    let _ = bind_int_constants(
        scope,
        global,
        "HTMLMediaElement",
        MEDIA_CONSTANTS.iter().copied(),
    );
    for &name in NAVIGATOR_CONSTRUCTORS {
        bind_constructor(scope, global, name, 0, Native::IllegalConstructor);
    }
    bind_constructor(
        scope,
        global,
        "DOMImplementation",
        0,
        Native::IllegalConstructor,
    );
    ffi::install_on(scope, Installer::PerfEntryList, global);
    for &(name, table) in ELEMENT_TABLES {
        install_table_on(scope, global, name, table);
    }
    install_table_on(
        scope,
        global,
        "DOMImplementation",
        ProtoTable::DomImplementation,
    );
    let tree_walker_proto = prototype_of(scope, global, "TreeWalker");
    ffi::install_value(
        scope,
        ValueInstaller::TreeWalkerProto,
        &tree_walker_proto,
        core::ptr::null(),
    );
    ffi::install_on(scope, Installer::LiveConstructors, global);
    let validity_proto = install_table_on(scope, global, "ValidityState", ProtoTable::Validity);
    if validity_proto.is_object() {
        let _ = scope.define_to_string_tag(&validity_proto, "ValidityState");
    }
    link_xml_document(scope, global);
    ffi::install_on(scope, Installer::DomHierarchy, global);
    southstar_js_interfaces::install_web_api_shapes(scope);
    bind_constructor(scope, global, "FontFace", 3, Native::FontFaceConstructor);
    for &(name, which) in HOST_PROTOS {
        link_host_proto(scope, global, name, which);
    }
    bind_constructor(
        scope,
        global,
        "XMLSerializer",
        0,
        Native::XmlSerializerConstructor,
    );
    ffi::install_on(scope, Installer::Compression, global);
    let native_range = ffi::native(scope, "__ndNativeRange", 0, Native::Range);
    let _ = bind_function(scope, global, "__ndNativeRange", native_range);
    ffi::install_on(scope, Installer::HasInstance, global);
    ffi::install_on(scope, Installer::ToStringTag, global);
    for &name in EVENT_HANDLER_TARGETS {
        let proto = prototype_of(scope, global, name);
        ffi::install_value(
            scope,
            ValueInstaller::EventHandlerAccessors,
            &proto,
            core::ptr::null(),
        );
    }
    ffi::install_value(
        scope,
        ValueInstaller::EventHandlerAccessors,
        global,
        core::ptr::null(),
    );
    ffi::install_on(scope, Installer::IndexedDb, global);
    ffi::install_on(scope, Installer::NetInterfaces, global);
}
