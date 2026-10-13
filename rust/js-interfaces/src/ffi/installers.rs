//! Southstar — installs the DOM property tables: the engine classes and their prototypes, per-interface members, the document's own functions and the proto tables the page setup asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_attrs as attrs;
use southstar_js_bind::bind_native;
use southstar_js_collections as collections;
use southstar_js_cssom as cssom;
use southstar_js_engine::{ContextHandle, NativeFn, Prop, Scope, Value};
use southstar_js_runtime::api;
use southstar_js_storage as storage;
use southstar_js_window as window;

use super::tables::*;

const CLASS_ELEMENT: c_int = 0;
const CLASS_ATTR: c_int = 1;
const CLASS_STYLE: c_int = 2;
const CLASS_TOKEN_LIST: c_int = 3;
const CLASS_LIVE: c_int = 4;
const CLASS_DATASET: c_int = 5;
const CLASS_STORAGE: c_int = 6;
const CLASS_WINDOW_NAMED: c_int = 7;

const HOST_PROTO_STYLE: c_int = 0;
const HOST_PROTO_DATASET: c_int = 1;
const HOST_PROTO_TOKEN_LIST: c_int = 2;

#[cfg(feature = "quickjs")]
unsafe extern "C" {
    fn ns_element_class_id() -> u32;
    fn ns_attr_class_id() -> u32;
    fn ns_js_brand_node_interfaces(ctx: ContextHandle, element_cid: u32, attr_cid: u32);
}

fn bind_all(scope: &mut Scope<'_>, proto: &Value, functions: &[(&str, NativeFn)]) {
    for &(name, f) in functions {
        bind_native(scope, proto, name, 1, f);
    }
}

pub fn register_class(scope: &mut Scope<'_>, which: c_int) -> Value {
    match which {
        CLASS_ELEMENT => {
            let proto = scope.new_object();
            scope.install_props(&proto, &ELEMENT_PROTO_FUNCS);
            crate::define_element_unscopables(scope, &proto);
            api::register_element_class(scope, proto.clone());
            proto
        }
        CLASS_ATTR => {
            let proto = scope.new_object();
            attrs::class::register_attr(scope, proto);
            Value::undefined()
        }
        CLASS_STYLE => {
            let proto = scope.new_object();
            let _ = cssom::declaration::install_style_proto(scope, &proto);
            let _ = scope.define_to_string_tag(&proto, "CSSStyleDeclaration");
            cssom::class::register(scope, proto);
            Value::undefined()
        }
        CLASS_TOKEN_LIST => {
            let proto = scope.new_object();
            scope.install_props(&proto, &TLIST_PROTO);
            attrs::class::register(scope, proto.clone());
            proto
        }
        CLASS_LIVE => {
            collections::class::register(scope);
            Value::undefined()
        }
        CLASS_DATASET => {
            let proto = scope.new_object();
            attrs::class::register_dataset(scope, proto);
            Value::undefined()
        }
        CLASS_STORAGE => {
            let proto = scope.new_object();
            storage::binding::install_proto(scope, &proto);
            storage::class::register(scope, proto.clone());
            proto
        }
        CLASS_WINDOW_NAMED => {
            window::class::register(scope);
            Value::undefined()
        }
        _ => Value::undefined(),
    }
}

pub fn install_dom_members(scope: &mut Scope<'_>, proto: &Value, which: c_int) {
    let props: &'static [Prop] = match which {
        0 => &ELEMENT_PROTO_FUNCS,
        1 => {
            api::register_element_class(scope, proto.clone());
            bind_all(scope, proto, &ELEMENT_CLASS_FUNCTIONS);
            return;
        }
        2 => &HTML_ELEMENT_MEMBERS,
        3 => &DATASET_MEMBERS,
        4 => &ASSIGNED_SLOT_MEMBERS,
        5 => {
            bind_all(scope, proto, &FRAGMENT_FUNCTIONS);
            return;
        }
        6 => &TRACK_ACCESSORS,
        7 => &DIALOG_MEMBERS,
        8 => &BUTTON_MEMBERS,
        9 => &TEXT_PROPERTY_MEMBERS,
        10 => &DATA_PROPERTY_MEMBERS,
        11 => &VALUE_PROPERTY_MEMBERS,
        12 => &OPTIONS_MEMBERS,
        _ => return,
    };
    scope.install_props(proto, props);
}

pub fn document_install_funcs(scope: &mut Scope<'_>, doc: &Value) {
    scope.install_props(doc, &DOCUMENT_FUNCS);
    super::install_event_handler_props(scope, doc);
}

pub fn install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: c_int) {
    let props: &'static [Prop] = match table {
        0 => &DOCUMENT_PROTO_METHODS,
        1 => &DOCUMENT_PROTO_ACCESSORS,
        2 => &NAMEDMAP_PROTO_FUNCS,
        3 => &SRC_PROTO,
        4 => &IMAGE_PROTO_FUNCS,
        5 => &SOURCE_PROTO,
        6 => &MEDIA_PROTO_FUNCS,
        7 => &VIDEO_PROTO_FUNCS,
        8 => &IFRAME_PROTO_FUNCS,
        9 => &ANCHOR_PROTO_FUNCS,
        10 => &HREF_PROTO_FUNCS,
        11 => &DOM_IMPLEMENTATION_PROTO_FUNCS,
        12 => &VALIDITY_PROTO_FUNCS,
        13 => &ELEMENT_PROTO_FUNCS,
        _ => return,
    };
    scope.install_props(proto, props);
}

pub fn host_class_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    match which {
        HOST_PROTO_STYLE => cssom::class::prototype(scope),
        HOST_PROTO_DATASET => attrs::class::dataset_prototype(scope),
        HOST_PROTO_TOKEN_LIST => attrs::class::prototype(scope),
        _ => Value::undefined(),
    }
}

#[cfg(feature = "quickjs")]
pub fn brand_node_classes(ctx: ContextHandle) {
    unsafe { ns_js_brand_node_interfaces(ctx, ns_element_class_id(), ns_attr_class_id()) };
}

#[cfg(not(feature = "quickjs"))]
pub fn brand_node_classes(_ctx: ContextHandle) {}

pub fn link_interfaces(scope: &mut Scope<'_>) {
    let _ = scope.eval_native_script(crate::sources::INTERFACE_LINKS, "<interface-links>");
}
