//! Southstar — the C ABI and Rust-ABI entry points of DOM change recording, and the page calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::attrs::QualifiedName;
use crate::{Element, attribute_maps, attrs, change, orphans};

pub(crate) use southstar_js_runtime::api::NsJs;
pub(crate) use southstar_js_runtime::api::Page;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub(crate) fn of(js: *mut NsJs) -> Option<Js> {
        (!js.is_null()).then_some(Js(js))
    }

    pub(crate) fn of_page(page: Page) -> Js {
        Js(page.as_ptr().cast_mut())
    }
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

pub(crate) fn qcache_invalidate(js: Option<Js>) {
    southstar_js_collections::qcache_invalidate(raw_js(js));
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    southstar_dom::serialize::is_shadow_root_marked(node)
}

fn page_context(js: Js) -> ContextHandle {
    api::main_context(js.0).or(api::main_realm(js.0))
}

pub(crate) fn has_main_context(js: Js) -> bool {
    !api::main_context(js.0).is_null()
}

pub(crate) fn with_page_context(js: Js, f: impl FnOnce(&mut Scope<'_>)) {
    page_context(js).with(f);
}

pub(crate) fn element_value(scope: &mut Scope<'_>, node: Element) -> Option<Value> {
    api::make_element(scope, node.as_ptr()).ok()
}

pub(crate) fn mark_childlist_dirty(parent: Element, added: Option<Element>) {
    southstar_css::mark_childlist_dirty(parent, added);
}

pub(crate) fn mark_attr_dirty(target: Option<Element>, name: Option<&CStr>, old: Option<&CStr>) {
    if let Some(target) = target {
        southstar_css::mark_attr_dirty(target, name.map(CStr::to_bytes), old.map(CStr::to_bytes));
    }
}

pub(crate) fn mark_restyle_dirty(node: Option<Element>) {
    if let Some(node) = node {
        southstar_css::mark_restyle_dirty(node);
    }
}

pub(crate) fn link_resource_changed(js: Js, link: Element) {
    southstar_js_loader::link_resource_changed(js.0, link);
}

pub(crate) fn attr_may_affect_style(target: Element, name: &CStr) -> bool {
    southstar_css::attr_may_affect_style(target, Some(name.to_bytes()))
}

pub(crate) fn emit_attributes(
    js: Option<Js>,
    target: Option<Element>,
    name: Option<&CStr>,
    namespace_uri: Option<&CStr>,
    old: Option<&CStr>,
) {
    if let Some(target) = target {
        southstar_js_observers::record_attributes(
            raw_js(js),
            target,
            name.map(CStr::to_bytes),
            namespace_uri.map(CStr::to_bytes),
            old.map(CStr::to_bytes),
        );
    }
}

pub(crate) fn emit_character_data(js: Option<Js>, target: Option<Element>, old: Option<&CStr>) {
    if let Some(target) = target {
        southstar_js_observers::record_character_data(raw_js(js), target, old.map(CStr::to_bytes));
    }
}

pub(crate) fn emit_child(
    js: Option<Js>,
    target: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_observers::record_child(raw_js(js), target, added, removed, previous, next);
}

pub(crate) fn emit_child_list(
    js: Option<Js>,
    target: Element,
    added: Option<&[Element]>,
    removed: &[Element],
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_observers::record_child_list(
        raw_js(js),
        target,
        added.unwrap_or_default(),
        removed,
        previous,
        next,
    );
}

pub(crate) fn ce_attr_changed(
    js: Js,
    node: Element,
    name: &CStr,
    old: Option<&CStr>,
    new: Option<&CStr>,
) {
    api::ce_attr_changed(js.0, node, name, old, new);
}

pub(crate) fn ce_disconnect_subtree(js: Js, root: Element) {
    southstar_js_custom_elements::disconnect_subtree(js.0, root);
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn request_repaint(js: Js) {
    api::request_repaint(js.0);
}

pub(crate) fn attr_detach_matching(
    js: Option<Js>,
    owner: Element,
    namespace_uri: Option<&CStr>,
    local_name: &CStr,
) {
    (api::attrs_hooks().detach_matching)(raw_js(js), owner, namespace_uri, local_name);
}

pub(crate) fn set_attr(el: Element, name: &CStr, value: &CStr) {
    southstar_dom::attrs::set_attribute(el, name, Some(value.to_bytes()));
}

pub(crate) fn set_attr_len(el: Element, name: &CStr, value: &[u8]) {
    southstar_dom::attrs::set_attribute(el, name, Some(value));
}

pub(crate) fn set_attr_ns(el: Element, qname: &QualifiedName, value: &CStr) {
    if el.is_element() {
        let qname = southstar_dom::attrs::NsName {
            namespace_uri: qname.namespace_uri,
            prefix: qname.prefix,
            local_name: qname.local_name,
            name: Some(qname.name),
        };
        southstar_dom::attrs::set_ns(el, qname, Some(value));
    }
}

pub(crate) fn remove_attr(el: Element, name: &CStr) {
    southstar_dom::attrs::remove_attribute(el, name);
}

pub(crate) fn remove_attr_ns(el: Element, namespace_uri: Option<&CStr>, local_name: &CStr) {
    if el.is_element() {
        southstar_dom::attrs::remove_ns(el, namespace_uri, local_name);
    }
}

pub(crate) fn purge_subtree_rafs(js: Js, root: Element) {
    api::purge_subtree_rafs(js.0, root);
}

pub(crate) fn images_forget_node(js: Js, node: Element) {
    southstar_js_images::forget_node(js.0, node);
}

pub(crate) fn dispatch_forget_node(js: Js, node: Element) {
    (api::dispatch_hooks().forget_node)(js.0, node);
}

pub(crate) fn purge_media_rafs(js: Js, root: Element) {
    (api::lifecycle_hooks().purge_media_rafs)(js.0, root);
}

pub(crate) fn focus_forget_subtree(js: Js, root: Element) {
    (api::focus_hooks().forget_subtree)(js.0, root);
}

pub(crate) fn editable_value<'a>(el: Element) -> Option<&'a CStr> {
    Some(southstar_dom::controls::editable_value(el))
}

pub(crate) fn dispatch_event(js: Js, target: Element, kind: &CStr) {
    southstar_js_runtime::events::dispatch_event(js.0, target, &kind.to_string_lossy());
}

pub(crate) fn arm_js_invalidate(el: Element) {
    api::arm_js_invalidate(el);
}

pub fn nsr_attribute_map_lookup(js: Page, owner: Node<'_>) -> Option<Value> {
    attribute_maps::lookup(Js::of_page(js), element(owner)?)
}

pub fn nsr_attribute_map_remember(js: Page, owner: Node<'_>, map: &Value) {
    if let Some(owner) = element(owner) {
        attribute_maps::remember(Js::of_page(js), owner, map.clone());
    }
}

fn element(node: Node<'_>) -> Option<Element> {
    node.downgrade().get()
}

fn init(js: *const NsJs) {
    if let Some(js) = Js::of(js.cast_mut()) {
        crate::init(js);
    }
}

fn teardown(js: *const NsJs) {
    if let Some(js) = Js::of(js.cast_mut()) {
        orphans::destroy(js);
    }
}

fn orphan_count(js: *const NsJs) -> usize {
    Js::of(js.cast_mut()).map_or(0, orphans::count)
}

fn collect_orphans(js: *const NsJs) -> Vec<Element> {
    Js::of(js.cast_mut())
        .and_then(orphans::collect_unreachable)
        .unwrap_or_default()
}

fn orphans_snapshot(js: *const NsJs) -> Option<Vec<Element>> {
    Js::of(js.cast_mut()).and_then(orphans::snapshot)
}

fn orphan_tracked(js: *const NsJs, node: Node<'_>) -> bool {
    match (Js::of(js.cast_mut()), element(node)) {
        (Some(js), Some(node)) => orphans::tracked(js, node),
        _ => false,
    }
}

fn track_orphan(js: *const NsJs, node: Node<'_>) {
    crate::track_orphan(js.cast_mut(), node);
}

fn unorphan_node(js: *const NsJs, node: Node<'_>) {
    if let Some(node) = element(node) {
        crate::unorphan_node(js.cast_mut(), node);
    }
}

fn purge_subtree_node_refs(js: *const NsJs, root: Node<'_>) {
    if let (Some(js), Some(root)) = (Js::of(js.cast_mut()), element(root)) {
        orphans::purge_node_refs(js, root);
    }
}

fn forget_pending_change(js: *const NsJs) {
    crate::forget_pending_change(js.cast_mut());
}

fn forget_pending_change_for(js: *const NsJs, el: Node<'_>) {
    if let (Some(js), Some(el)) = (Js::of(js.cast_mut()), element(el)) {
        change::forget_node(js, el);
    }
}

fn attribute_map_release_owner(js: *const NsJs, owner: Node<'_>) {
    if let (Some(js), Some(owner)) = (Js::of(js.cast_mut()), element(owner)) {
        attribute_maps::release_owner(js, owner);
    }
}

fn attribute_maps_teardown(js: *const NsJs) {
    if let Some(js) = Js::of(js.cast_mut()) {
        attribute_maps::teardown(js);
    }
}

fn set_attr_recorded(js: *const NsJs, node: Node<'_>, name: &CStr, value: &[u8]) {
    if let Some(node) = element(node) {
        crate::set_attr_recorded(js.cast_mut(), node, name, value);
    }
}

fn set_attr_recorded_paint_only(js: *const NsJs, node: Node<'_>, name: &CStr, value: &CStr) {
    if let Some(node) = element(node) {
        attrs::set_recorded_paint_only(Js::of(js.cast_mut()), node, name, value);
    }
}

pub fn install_runtime_hooks() {
    api::set_mutation_hooks(api::MutationHooks {
        init,
        teardown,
        orphan_count,
        collect_orphans,
        orphans_snapshot,
        orphan_tracked,
        track_orphan,
        unorphan_node,
        purge_subtree_node_refs,
        forget_pending_change,
        forget_pending_change_for,
        attribute_map_release_owner,
        attribute_maps_teardown,
        set_attr_recorded,
        set_attr_recorded_paint_only,
        index_child_removed,
        record_child_added,
    });
}

fn index_child_removed(js: *const NsJs, parent: Option<Node<'_>>, removed: Node<'_>) {
    match parent.and_then(element) {
        Some(parent) => crate::index_child_change(js.cast_mut(), parent, None, element(removed)),
        None => qcache_invalidate(Js::of(js.cast_mut())),
    }
}

fn record_child_added(
    js: *const NsJs,
    parent: Node<'_>,
    added: Node<'_>,
    previous: Option<Node<'_>>,
) {
    match element(parent) {
        Some(parent) => crate::record_child_change(
            js.cast_mut(),
            parent,
            element(added),
            None,
            previous.and_then(element),
            None,
        ),
        None => qcache_invalidate(Js::of(js.cast_mut())),
    }
}
