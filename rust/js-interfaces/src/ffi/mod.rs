//! Southstar — the interface installers' links to the rest of the engine: the realm record, the other binding crates, and the exports the page and worker setup call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod adapt;
mod installers;
mod natives;
mod tables;

use core::ffi::c_int;

use southstar_dom::Node;
use southstar_js_collections as collections;
use southstar_js_dispatch as dispatch;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api::{self, NsJs};

pub(crate) use api::AdoptedProtos as DomProtos;
pub use installers::{
    document_install_funcs, host_class_proto, install_dom_members, install_proto_table,
    link_interfaces, register_class,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) enum Members {
    ElementFunctions = 0,
    ElementClass = 1,
    HtmlElement = 2,
    Dataset = 3,
    Text = 4,
    DocumentFragment = 5,
    Track = 6,
    Dialog = 7,
    Button = 8,
    TextProperty = 9,
    DataProperty = 10,
    ValueProperty = 11,
    OptionsMethods = 12,
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Node<'static>> {
    api::unwrap_element(value)
}

pub(crate) fn live_collection_kind(value: &Value) -> c_int {
    collections::collection_kind(value)
}

pub(crate) fn is_shadow_root(node: Node<'_>) -> bool {
    southstar_js_shadow::is_shadow_root(node)
}

pub(crate) fn install_members(scope: &mut Scope<'_>, proto: &Value, members: Members) {
    install_dom_members(scope, proto, members as c_int);
}

pub(crate) fn register_tag_proto(scope: &mut Scope<'_>, tag: &str, proto: &Value) {
    api::register_tag_proto(scope, tag.as_bytes(), proto.clone());
}

pub(crate) fn adopt_dom_protos(scope: &mut Scope<'_>, protos: DomProtos) {
    api::adopt_dom_protos(scope, protos);
}

fn page_record(scope: &Scope<'_>) -> *const NsJs {
    scope.context_opaque().cast::<NsJs>().cast_const()
}

pub(crate) fn navigator_brand(scope: &Scope<'_>) -> Value {
    let js = page_record(scope);
    if js.is_null() {
        return Value::undefined();
    }
    api::navigator_brand(js)
}

pub(crate) fn page_clock(scope: &Scope<'_>) -> (f64, f64) {
    let js = page_record(scope);
    (
        api::time_origin_real_ms(js),
        southstar_js_perf::page_now_ms(js),
    )
}

pub(crate) fn install_event_handler_props(scope: &mut Scope<'_>, global: &Value) {
    dispatch::handlers::install_props(scope, global);
}

pub(crate) fn hide_shared_array_buffer(scope: &mut Scope<'_>, global: &Value) {
    southstar_js_realm::hide_shared_array_buffer(scope, global);
}

pub(crate) fn install_intl(scope: &mut Scope<'_>, global: &Value) {
    southstar_js_intl::install(scope, global);
}

pub(crate) fn install_temporal(scope: &mut Scope<'_>, global: &Value) {
    let _ = southstar_js_temporal::install(scope, global);
}

fn with_handle(ctx: ContextHandle, f: impl FnOnce(&mut Scope<'_>)) {
    let _ = ctx.with(f);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_install_pdf_plugins(ctx: ContextHandle) {
    with_handle(ctx, crate::install_pdf_plugins);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_link_interface_ctors(ctx: ContextHandle) {
    with_handle(ctx, crate::link_interface_ctors);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_lock_global_prototypes(ctx: ContextHandle) {
    with_handle(ctx, crate::lock_global_prototypes);
}

#[unsafe(no_mangle)]
pub fn nsr_install_hasinstance(scope: &mut Scope<'_>, global: &Value) {
    crate::install_has_instance(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_tostringtag(scope: &mut Scope<'_>, global: &Value) {
    crate::install_to_string_tag(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_dom_hierarchy(scope: &mut Scope<'_>, global: &Value) {
    crate::install_dom_hierarchy(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_window_chrome(scope: &mut Scope<'_>, global: &Value) {
    crate::install_window_chrome(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_window_compat(scope: &mut Scope<'_>, global: &Value) {
    crate::compat::install_window_compat(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_install_namespace_object(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    object: Value,
    tag: &str,
) {
    crate::install_namespace_object(scope, global, name, object, tag);
}

#[unsafe(no_mangle)]
pub fn nsr_document_install_funcs(scope: &mut Scope<'_>, doc: &Value) {
    document_install_funcs(scope, doc);
}

#[unsafe(no_mangle)]
pub fn nsr_js_host_class_proto(scope: &mut Scope<'_>, which: c_int) -> Value {
    host_class_proto(scope, which)
}

#[unsafe(no_mangle)]
pub fn nsr_js_register_class(scope: &mut Scope<'_>, which: c_int) -> Value {
    register_class(scope, which)
}

#[unsafe(no_mangle)]
pub fn nsr_js_install_proto_table(scope: &mut Scope<'_>, proto: &Value, table: c_int) {
    install_proto_table(scope, proto, table);
}
