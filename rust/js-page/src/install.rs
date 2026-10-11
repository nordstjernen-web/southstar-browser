//! Southstar — installing a document into the page's realm: the runtime reset, the document object, location and the window's finishing shapes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_bind::prototype_of;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::Element;
use crate::ffi::{self, Js, Native, ProtoTable, Shaper, ValueInstaller};
use crate::interfaces;

const LOCATION_FORWARD: &str = "(function(){ var loc = location; var d = { configurable: true, enumerable: true,   get: function(){ return loc; },   set: function(v){ loc.href = v; } }; Object.defineProperty(globalThis, 'location', d); try { Object.defineProperty(document, 'location', d); } catch(e){}})();";

const ALL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

fn reset_runtime_state(js: Js) {
    js.reset_page_state();
    js.free_orphans();
    js.reset_frames_and_tasks();
    js.drain_microtasks();
    js.scope(|scope| scope.gc());
}

pub(crate) fn install_document(js: Js, doc: Option<Element>, base_url: Option<&CStr>) {
    reset_runtime_state(js);
    js.set_document(doc, base_url.unwrap_or(c""));
    if let Some(doc) = doc {
        ffi::build_document_indexes(doc);
    }
    let url = js.current_url_text();
    let mut message = b"document ".to_vec();
    message.extend_from_slice(url.to_bytes());
    ffi::debug_js(c"install", &message);
    js.load_storage_and_cookies();
    js.scope(|scope| {
        install_globals(js, scope);
        js.eval_polyfills();
        js.drain_microtasks();
        finish_shapes(scope);
    });
}

fn install_globals(js: Js, scope: &mut Scope<'_>) {
    let global = scope.global();
    let url_owned = js.current_url_text().to_owned();
    let url_bytes = url_owned.as_c_str();
    let url = String::from_utf8_lossy(url_bytes.to_bytes()).into_owned();
    let document = js.new_document_object(scope);
    define_document_fields(js, scope, &global, &document, url_bytes, &url);
    set_document_prototype(scope, &global, &document);
    for name in ["Document", "HTMLDocument"] {
        let proto = prototype_of(scope, &global, name);
        if proto.is_object() {
            install_document_proto(scope, &proto);
        }
    }
    ffi::install_value(
        scope,
        ValueInstaller::LegacyNamed,
        &document,
        js.current_document(),
    );
    let _ = scope.set(&global, "document", document.clone());
    install_location(scope, &global, &document, url_bytes);
    interfaces::install(scope, &global);
}

fn define_document_fields(
    js: Js,
    scope: &mut Scope<'_>,
    global: &Value,
    document: &Value,
    url_bytes: &CStr,
    url: &str,
) {
    for key in ["URL", "documentURI", "baseURI"] {
        let value = scope.string(url);
        let _ = scope.set(document, key, value);
    }
    let value = scope.string("UTF-8");
    let _ = scope.set(document, "characterSet", value);
    let value = scope.string("UTF-8");
    let _ = scope.define(document, "charset", value, ALL);
    let value = scope.string("UTF-8");
    let _ = scope.set(document, "inputEncoding", value);
    let value = scope.string("text/html");
    let _ = scope.set(document, "contentType", value);
    let host = ffi::url_host(url_bytes).unwrap_or_default();
    let value = scope.string(&String::from_utf8_lossy(&host));
    let _ = scope.set(document, "domain", value);
    let _ = scope.set(document, "defaultView", global.clone());
    let _ = scope.set(document, "ownerDocument", Value::null());
    let value = scope.string("#document");
    let _ = scope.set(document, "nodeName", value);
    let _ = scope.set(document, "nodeType", Value::int(9));
    ffi::install_value(
        scope,
        ValueInstaller::DoctypeGetter,
        document,
        js.current_document(),
    );
    let value = scope.string("1.0");
    let _ = scope.set(document, "xmlVersion", value);
    let _ = scope.set(document, "xmlEncoding", Value::null());
    let _ = scope.set(document, "xmlStandalone", Value::boolean(false));
    ffi::install_value(
        scope,
        ValueInstaller::DocumentFunctions,
        document,
        js.current_document(),
    );
}

fn set_document_prototype(scope: &mut Scope<'_>, global: &Value, document: &Value) {
    let mut constructor = scope
        .get(global, "HTMLDocument")
        .unwrap_or_else(|_| Value::undefined());
    if !constructor.is_object() {
        constructor = scope
            .get(global, "Document")
            .unwrap_or_else(|_| Value::undefined());
    }
    if !constructor.is_object() {
        return;
    }
    let proto = scope
        .get(&constructor, "prototype")
        .unwrap_or_else(|_| Value::undefined());
    if proto.is_object() {
        let _ = scope.set_prototype(document, &proto);
    }
}

fn install_document_proto(scope: &mut Scope<'_>, proto: &Value) {
    ffi::install_proto_table(scope, proto, ProtoTable::DocumentMethods);
    ffi::install_proto_table(scope, proto, ProtoTable::DocumentAccessors);
    for (name, arity, native) in [
        ("open", 0, Native::DocumentOpen),
        ("close", 0, Native::DocumentClose),
        ("write", 1, Native::DocumentWrite),
        ("writeln", 1, Native::DocumentWriteln),
    ] {
        let function = ffi::native(scope, name, arity, native);
        let _ = scope.define(proto, name, function, Attributes::METHOD);
    }
}

fn install_location(scope: &mut Scope<'_>, global: &Value, document: &Value, url: &CStr) {
    let location = ffi::make_location(scope);
    let _ = scope.set(global, "location", location.clone());
    let _ = scope.set(document, "location", location);
    let origin = ffi::url_origin(url).unwrap_or_default();
    let value = scope.string(&String::from_utf8_lossy(&origin));
    let _ = scope.set(global, "origin", value);
    let secure = url.to_bytes().starts_with(b"https:");
    let _ = scope.set(global, "isSecureContext", Value::boolean(secure));
    let _ = scope.eval_native_script(LOCATION_FORWARD, "<location-forward>");
}

fn finish_shapes(scope: &mut Scope<'_>) {
    let global = scope.global();
    ffi::install_on(scope, ffi::Installer::EventAttributeGetters, &global);
    ffi::shape(scope, Shaper::NavigatorShape);
    ffi::shape(scope, Shaper::PdfPlugins);
    ffi::shape(scope, Shaper::LinkInterfaces);
    ffi::shape(scope, Shaper::NameEngineMembers);
    ffi::shape(scope, Shaper::ShapeWindowGlobal);
    ffi::shape(scope, Shaper::LinkInterfaceConstructors);
    ffi::shape(scope, Shaper::LockGlobalPrototypes);
    let document = scope
        .get(&global, "document")
        .unwrap_or_else(|_| Value::undefined());
    ffi::install_value(
        scope,
        ValueInstaller::LiftDocumentMethods,
        &document,
        core::ptr::null(),
    );
    ffi::shape(scope, Shaper::NodeShapes);
    ffi::shape(scope, Shaper::ElementShapes);
    ffi::shape(scope, Shaper::BrandNodeClasses);
}
