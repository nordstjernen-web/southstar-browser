//! Southstar — the C ABI and Rust-ABI entry points of DOM change recording, and the page calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::attrs::QualifiedName;
use crate::{Element, attribute_maps, attrs, change, orphans};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_attr_detach_matching(
        js: *mut NsJs,
        owner: *mut NsNode,
        namespace_uri: *const c_char,
        local_name: *const c_char,
    );
    fn ns_dispatch_forget_node(js: *mut NsJs, node: *const NsNode);
    fn ns_js_purge_media_rafs(js: *mut NsJs, root: *const NsNode);
    fn ns_focus_forget_subtree(js: *const NsJs, root: *const NsNode);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
}

impl Js {
    pub(crate) fn of(js: *mut NsJs) -> Option<Js> {
        (!js.is_null()).then_some(Js(js))
    }
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

fn c_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

unsafe fn node(n: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(n) }
}

unsafe fn c_str<'a>(text: *const c_char) -> Option<&'a CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

fn ptr_array(nodes: &[Element]) -> *mut GPtrArray {
    let array = unsafe { glib::g_ptr_array_new() };
    for node in nodes {
        unsafe { glib::g_ptr_array_add(array, node.as_mut_ptr().cast()) };
    }
    array
}

fn into_ptr_array(nodes: Option<Vec<Element>>) -> *mut GPtrArray {
    nodes.map_or(ptr::null_mut(), |nodes| ptr_array(&nodes))
}

pub(crate) fn qcache_invalidate(js: Option<Js>) {
    southstar_js_collections::qcache_invalidate(raw_js(js));
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    unsafe { ns_node_is_shadow_root(node.as_ptr()) != 0 }
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
    unsafe {
        ns_attr_detach_matching(
            raw_js(js),
            owner.as_mut_ptr(),
            c_ptr(namespace_uri),
            local_name.as_ptr(),
        )
    };
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
    unsafe { ns_dispatch_forget_node(js.0, node.as_ptr()) };
}

pub(crate) fn purge_media_rafs(js: Js, root: Element) {
    unsafe { ns_js_purge_media_rafs(js.0, root.as_ptr()) };
}

pub(crate) fn focus_forget_subtree(js: Js, root: Element) {
    unsafe { ns_focus_forget_subtree(js.0, root.as_ptr()) };
}

pub(crate) fn editable_value<'a>(el: Element) -> Option<&'a CStr> {
    Some(southstar_dom::controls::editable_value(el))
}

pub(crate) fn dispatch_event(js: Js, target: Element, kind: &CStr) {
    unsafe { ns_js_dispatch_event(js.0, target.as_ptr(), kind.as_ptr(), ptr::null_mut()) };
}

pub(crate) fn arm_js_invalidate(el: Element) {
    api::arm_js_invalidate(el);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mutation_init(js: *mut NsJs) {
    if let Some(js) = Js::of(js) {
        crate::init(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_index_child_change(
    js: *mut NsJs,
    parent: *mut NsNode,
    added: *mut NsNode,
    removed: *mut NsNode,
) {
    // SAFETY: the caller passes null or live nodes of this thread's store.
    let nodes = unsafe { (node(parent), node(added), node(removed)) };
    match nodes {
        (Some(parent), added, removed) => crate::index_child_change(js, parent, added, removed),
        (None, ..) => qcache_invalidate(Js::of(js)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_record_child_change(
    js: *mut NsJs,
    parent: *mut NsNode,
    added: *mut NsNode,
    removed: *mut NsNode,
    previous_sibling: *mut NsNode,
    next_sibling: *mut NsNode,
) {
    // SAFETY: the caller passes null or live nodes of this thread's store.
    let (added, removed) = unsafe { (node(added), node(removed)) };
    // SAFETY: as above.
    let (previous, next) = unsafe { (node(previous_sibling), node(next_sibling)) };
    // SAFETY: as above.
    match unsafe { node(parent) } {
        Some(parent) => crate::record_child_change(js, parent, added, removed, previous, next),
        None => qcache_invalidate(Js::of(js)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_attr_recorded_len(
    js: *mut NsJs,
    n: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
    len: isize,
) {
    let (Some(n), Some(name)) = (unsafe { node(n) }, unsafe { c_str(name) }) else {
        return;
    };
    let value: &[u8] = match (value.is_null(), usize::try_from(len)) {
        (true, _) => b"",
        (false, Ok(len)) => unsafe { glib::slice(value.cast(), len) },
        (false, Err(_)) => unsafe { CStr::from_ptr(value) }.to_bytes(),
    };
    crate::set_attr_recorded(js, n, name, value);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_attr_recorded(
    js: *mut NsJs,
    n: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
) {
    unsafe { ns_js_set_attr_recorded_len(js, n, name, value, -1) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_attr_recorded_paint_only(
    js: *mut NsJs,
    n: *mut NsNode,
    name: *const c_char,
    value: *const c_char,
) {
    let (Some(n), Some(name)) = (unsafe { node(n) }, unsafe { c_str(name) }) else {
        return;
    };
    let value = unsafe { c_str(value) }.unwrap_or(c"");
    attrs::set_recorded_paint_only(Js::of(js), n, name, value);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_track_orphan(js: *mut NsJs, n: *mut NsNode) {
    if let Some(n) = unsafe { node(n) } {
        crate::track_orphan(js, n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_unorphan_node(js: *mut NsJs, n: *mut NsNode) {
    if let Some(n) = unsafe { node(n) } {
        crate::unorphan_node(js, n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_orphan_tracked(js: *mut NsJs, n: *const NsNode) -> GBoolean {
    let tracked = match (Js::of(js), unsafe { node(n) }) {
        (Some(js), Some(n)) => orphans::tracked(js, n),
        _ => false,
    };
    glib::boolean(tracked)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_orphan_count(js: *mut NsJs) -> c_int {
    Js::of(js).map_or(0, |js| orphans::count(js).min(c_int::MAX as usize) as c_int)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_node_refs(js: *mut NsJs, root: *mut NsNode) {
    if let (Some(js), Some(root)) = (Js::of(js), unsafe { node(root) }) {
        orphans::purge_node_refs(js, root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_orphans_collect(js: *mut NsJs) -> *mut GPtrArray {
    into_ptr_array(Js::of(js).and_then(orphans::collect_unreachable))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_orphans_snapshot(js: *mut NsJs) -> *mut GPtrArray {
    into_ptr_array(Js::of(js).and_then(orphans::snapshot))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_mutation_teardown(js: *mut NsJs) {
    if let Some(js) = Js::of(js) {
        orphans::destroy(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_forget_pending_change(js: *mut NsJs) {
    crate::forget_pending_change(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_forget_pending_change_for(js: *mut NsJs, el: *const NsNode) {
    if let (Some(js), Some(el)) = (Js::of(js), unsafe { node(el) }) {
        change::forget_node(js, el);
    }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_attribute_map_lookup(js: *mut NsJs, owner: *const NsNode) -> Option<Value> {
    match (Js::of(js), unsafe { node(owner) }) {
        (Some(js), Some(owner)) => attribute_maps::lookup(js, owner),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_attribute_map_remember(js: *mut NsJs, owner: *const NsNode, map: &Value) {
    if let (Some(js), Some(owner)) = (Js::of(js), unsafe { node(owner) }) {
        attribute_maps::remember(js, owner, map.clone());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attribute_map_release_owner(js: *mut NsJs, owner: *const NsNode) {
    if let (Some(js), Some(owner)) = (Js::of(js), unsafe { node(owner) }) {
        attribute_maps::release_owner(js, owner);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attribute_maps_teardown(js: *mut NsJs) {
    if let Some(js) = Js::of(js) {
        attribute_maps::teardown(js);
    }
}
