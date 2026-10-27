//! Southstar — the C ABI and Rust-ABI entry points of the observers, and the realm, geometry and layout calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use crate::mutation::{self, Emit};
use crate::{intersection, page, resize};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *const NsJs {
        self.0 as *const NsJs
    }
}

pub(crate) use southstar_js_geometry::Rect;

use southstar_js_geometry::nsr_box_for_this;
use southstar_js_geometry::nsr_make_dom_rect;

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque() as usize)
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.ptr()).with(f)
}

pub(crate) fn has_main_context(js: Js) -> bool {
    !api::main_context(js.ptr()).is_null()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    if let Ok(line) = CString::new(line) {
        api::log_line(js.ptr(), &line);
    }
}

pub(crate) fn schedule_tick(js: Js) {
    api::observer_schedule_tick(js.ptr());
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    unsafe { BoxRef::from_ptr(api::layout_root(js.ptr()).cast::<NsBox>()) }
}

pub(crate) fn call_observer(
    scope: &mut Scope<'_>,
    callback: &Value,
    this: &Value,
    args: &[Value],
    report_type: Option<&CStr>,
    fresh_budget: bool,
) -> Result<Value, Value> {
    let js = js_of(scope);
    let report = report_type.map(|text| text.to_string_lossy().into_owned());
    api::call_observer(
        js.ptr(),
        scope.handle(),
        callback,
        this,
        args,
        report.as_deref(),
        fresh_budget,
    )
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: usize) -> Value {
    api::make_element(scope, node as *const NsNode).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    api::unwrap_element(value)
}

pub(crate) fn node_address(value: &Value) -> usize {
    Node::ptr_or_null(api::unwrap_element(value)) as usize
}

pub(crate) fn arm_invalidate(node: usize) {
    if let Some(node) = unsafe { Node::from_ptr(node as *const NsNode) } {
        api::arm_js_invalidate(node);
    }
}

pub(crate) fn dom_rect(scope: &mut Scope<'_>, rect: Rect) -> Value {
    nsr_make_dom_rect(scope, rect.x, rect.y, rect.w, rect.h)
}

pub(crate) fn realm_now_ms(scope: &mut Scope<'_>) -> f64 {
    southstar_js_perf::context_now_ms(scope.handle())
}

pub(crate) fn box_for(scope: &mut Scope<'_>, value: &Value) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(nsr_box_for_this(scope, value)) }
}

pub(crate) fn find_by_dom(root: BoxRef<'_>, node: usize) -> Option<BoxRef<'_>> {
    southstar_js_geometry::find_box_by_dom(root, node as *const c_void)
}

pub(crate) use southstar_js_geometry::{
    box_border_box as border_box, box_visual_border_box as visual_border_box,
    box_visual_padding_box as visual_padding_box,
};

pub(crate) fn style_keyword<'a>(b: BoxRef<'a>, property: &CStr) -> Option<&'a CStr> {
    let prop = southstar_css::Prop::from_name(property.to_bytes())?;
    southstar_style::box_style(b)?.keyword_of(prop)
}

fn address(node: Option<Node<'_>>) -> usize {
    Node::ptr_or_null(node) as usize
}

fn addresses(nodes: &[Node<'_>]) -> Option<Vec<usize>> {
    (!nodes.is_empty()).then(|| nodes.iter().map(|&n| address(Some(n))).collect())
}

fn emit(js: *const NsJs, emit: &Emit<'_>) {
    mutation::emit(Js::of(js), emit);
}

pub fn record_attributes(
    js: *const NsJs,
    target: Node<'_>,
    name: Option<&[u8]>,
    namespace: Option<&[u8]>,
    old_value: Option<&[u8]>,
) {
    emit(
        js,
        &Emit {
            kind: Some(b"attributes"),
            target: address(Some(target)),
            added: None,
            removed: None,
            previous_sibling: 0,
            next_sibling: 0,
            attribute_name: name,
            attribute_namespace: namespace,
            old_value,
        },
    );
}

pub fn record_character_data(js: *const NsJs, target: Node<'_>, old_value: Option<&[u8]>) {
    emit(
        js,
        &Emit {
            kind: Some(b"characterData"),
            target: address(Some(target)),
            added: None,
            removed: None,
            previous_sibling: 0,
            next_sibling: 0,
            attribute_name: None,
            attribute_namespace: None,
            old_value,
        },
    );
}

pub fn record_child(
    js: *const NsJs,
    target: Node<'_>,
    added: Option<Node<'_>>,
    removed: Option<Node<'_>>,
    previous_sibling: Option<Node<'_>>,
    next_sibling: Option<Node<'_>>,
) {
    emit(
        js,
        &Emit {
            kind: Some(b"childList"),
            target: address(Some(target)),
            added: added.map(|n| vec![address(Some(n))]),
            removed: removed.map(|n| vec![address(Some(n))]),
            previous_sibling: address(previous_sibling),
            next_sibling: address(next_sibling),
            attribute_name: None,
            attribute_namespace: None,
            old_value: None,
        },
    );
}

pub fn record_child_list(
    js: *const NsJs,
    target: Node<'_>,
    added: &[Node<'_>],
    removed: &[Node<'_>],
    previous_sibling: Option<Node<'_>>,
    next_sibling: Option<Node<'_>>,
) {
    mutation::emit_child_list(
        Js::of(js),
        address(Some(target)),
        addresses(added),
        addresses(removed),
        address(previous_sibling),
        address(next_sibling),
    );
}

pub fn mutation_drain_pending(js: *const NsJs) -> bool {
    page(Js::of(js)).is_some_and(|page| page.drain_scheduled.get())
}

pub fn reset_observers(js: *const NsJs) {
    crate::reset(Js::of(js));
}

macro_rules! nsr_natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            pub fn $name(
                scope: &mut Scope<'_>,
                this: &Value,
                args: &[Value],
            ) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

nsr_natives! {
    nsr_mutation_observer_ctor => mutation::constructor,
    nsr_mutation_observer_observe => mutation::observe,
    nsr_mutation_observer_disconnect => mutation::disconnect,
    nsr_mutation_observer_take_records => mutation::take_records,
    nsr_intersection_observer_ctor => intersection::constructor,
    nsr_intersection_observer_observe => intersection::observe,
    nsr_intersection_observer_unobserve => intersection::unobserve,
    nsr_intersection_observer_disconnect => intersection::disconnect,
    nsr_intersection_observer_take_records => intersection::take_records,
    nsr_resize_observer_ctor => resize::constructor,
    nsr_resize_observer_observe => resize::observe,
    nsr_resize_observer_unobserve => resize::unobserve,
    nsr_resize_observer_disconnect => resize::disconnect,
}

fn scrub_node(js: *const NsJs, node: Node<'_>) {
    mutation::scrub_node(Js::of(js), node.as_ptr() as usize);
}

fn intersection_tick(js: *const NsJs) {
    intersection::tick(Js::of(js));
}

fn resize_tick(js: *const NsJs) {
    resize::tick(Js::of(js));
}

fn teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}

pub fn install_runtime_hooks() {
    southstar_js_runtime::api::set_observers_hooks(southstar_js_runtime::api::ObserversHooks {
        intersection_tick,
        resize_tick,
        scrub_node,
        teardown,
    });
    southstar_js_runtime::links::set_js_observers_links(
        southstar_js_runtime::links::JsObserversLinks {
            resize_observer_disconnect: crate::ffi::nsr_resize_observer_disconnect,
            resize_observer_unobserve: crate::ffi::nsr_resize_observer_unobserve,
            resize_observer_observe: crate::ffi::nsr_resize_observer_observe,
            resize_observer_ctor: crate::ffi::nsr_resize_observer_ctor,
            intersection_observer_take_records: crate::ffi::nsr_intersection_observer_take_records,
            intersection_observer_disconnect: crate::ffi::nsr_intersection_observer_disconnect,
            intersection_observer_unobserve: crate::ffi::nsr_intersection_observer_unobserve,
            intersection_observer_observe: crate::ffi::nsr_intersection_observer_observe,
            intersection_observer_ctor: crate::ffi::nsr_intersection_observer_ctor,
            mutation_observer_take_records: crate::ffi::nsr_mutation_observer_take_records,
            mutation_observer_disconnect: crate::ffi::nsr_mutation_observer_disconnect,
            mutation_observer_observe: crate::ffi::nsr_mutation_observer_observe,
            mutation_observer_ctor: crate::ffi::nsr_mutation_observer_ctor,
        },
    );
}
