//! Southstar — the Rust API other crates call for a page's frames: their realms, URLs, sandbox flags and load queue.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CStr;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api::Page;

use crate::ffi::ctx::Ctx;
use crate::ffi::{Js, frame_node, scope_ctx};
use crate::realm::frame_of;
use crate::{cloner, origin, owner, page, queue, realm, sandbox, schedule};

fn live(js: Option<Page>) -> Option<Js> {
    let js = Js::of(js);
    (!js.is_null()).then_some(js)
}

pub fn frame_context(js: Option<Page>, frame: Node<'_>) -> ContextHandle {
    live(js)
        .and_then(|js| page::frame_entry(js, frame_of(frame), |entry| entry.realm).flatten())
        .map_or(ContextHandle::NULL, Ctx::handle)
}

pub fn frame_of_realm(js: Option<Page>, realm: ContextHandle) -> Option<Node<'static>> {
    let js = live(js)?;
    let realm = Ctx::from_handle(realm)?;
    page::frame_of_realm(js, realm).and_then(frame_node)
}

pub fn frame_of_window(js: Option<Page>, window: &Value) -> Option<Node<'static>> {
    let js = live(js)?;
    if !window.is_object() {
        return None;
    }
    page::realms(js)
        .into_iter()
        .find(|(_, realm)| realm.global().same_object(window))
        .and_then(|(frame, _)| frame_node(frame))
}

pub fn node_realm_context(js: Option<Page>, node: Node<'_>) -> ContextHandle {
    live(js)
        .and_then(|js| realm::node_realm(js, node))
        .map_or(ContextHandle::NULL, Ctx::handle)
}

pub fn frame_url(js: Option<Page>, frame: Node<'_>) -> Option<Vec<u8>> {
    let js = live(js)?;
    page::frame_entry(js, frame_of(frame), |entry| {
        entry.url.as_ref().map(|url| url.as_bytes().to_vec())
    })
    .flatten()
}

pub fn frame_referrer(js: Option<Page>, frame: Node<'_>) -> Option<Vec<u8>> {
    let js = live(js)?;
    page::frame_entry(js, frame_of(frame), |entry| {
        entry
            .referrer
            .as_ref()
            .map(|referrer| referrer.as_bytes().to_vec())
    })
    .flatten()
}

pub fn frame_realm_window(js: Option<Page>, frame: Node<'_>) -> Option<Value> {
    realm::stored_window(Js::of(js), frame)
}

pub fn iframe_realm_window(scope: &mut Scope<'_>, element: &Value, iframe: Node<'_>) -> Value {
    realm::realm_window(scope_ctx(scope), element, iframe)
}

pub fn iframe_cross_origin_window(scope: &mut Scope<'_>, target: &Value) -> Value {
    realm::cross_origin_window(scope_ctx(scope), target.clone())
}

pub fn child_frame_window(
    scope: &mut Scope<'_>,
    doc: Option<Node<'_>>,
    index: u32,
    name: Option<&CStr>,
    raw: bool,
) -> Result<Value, Value> {
    owner::child_frame_window(scope, doc, index, name, raw)
}

pub fn clear_initial_blank_frames(js: Option<Page>) {
    page::peek_mut(Js::of(js), |page| page.initial_blank.clear());
}

pub fn reset_frames(js: Option<Page>) {
    let js = Js::of(js);
    let frames = page::peek_mut(js, |page| core::mem::take(&mut page.frames));
    drop(frames);
    let cloners = cloner::clear(js);
    drop(cloners);
}

pub fn iframe_origin_is_opaque(frame: Node<'_>) -> bool {
    sandbox::origin_is_opaque(frame)
}

pub fn sandbox_blocks_forms(node: Node<'_>) -> bool {
    sandbox::blocks_forms(node)
}

pub fn pending_iframe_count(js: Option<Page>) -> usize {
    live(js).map_or(0, queue::pending_count)
}

pub fn add_pending_iframe(js: Option<Page>, frame: Node<'_>) -> bool {
    live(js).is_some_and(|js| queue::add_pending(js, frame_of(frame)))
}

pub fn promote_deferred_iframes(js: Option<Page>) {
    if let Some(js) = live(js) {
        queue::promote_deferred(js);
    }
}

pub fn iframe_document_node(iframe: Node<'_>) -> Option<Node<'_>> {
    owner::document_node(iframe)
}

pub fn schedule_iframe_load(js: Option<Page>, iframe: Node<'_>, force: bool) {
    schedule::schedule(Js::of(js), iframe, force);
}

pub fn schedule_static_iframes(js: Option<Page>, root: Node<'_>) {
    schedule::schedule_static(Js::of(js), root, 0);
}

pub fn iframe_is_cross_origin(js: Option<Page>, iframe: Node<'_>) -> bool {
    origin::is_cross_origin(Js::of(js), iframe)
}
