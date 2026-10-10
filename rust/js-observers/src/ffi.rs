//! Southstar — the C ABI of the observers as declared in src/js_internal.h, and the js.c and layout calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_layout::{BoxRef, NsBox, Style};

use crate::mutation::{self, Emit};
use crate::{intersection, page, resize};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

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

#[derive(Clone, Copy)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

unsafe extern "C" {
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_log_line(js: *const NsJs, line: *const c_char);
    fn ns_js_layout_root(js: *const NsJs) -> *const NsBox;
    fn ns_observer_schedule_tick(js: *const NsJs);
    fn ns_js_call_observer(
        js: *const NsJs,
        ctx: *mut JSContext,
        callback: JSValue,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
        report_type: *const c_char,
        fresh_budget: GBoolean,
    ) -> JSValue;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_unwrap_element(value: JSValue) -> *const NsNode;
    fn ns_node_arm_js_invalidate(node: *const NsNode);
    fn ns_make_dom_rect(ctx: *mut JSContext, x: f64, y: f64, w: f64, h: f64) -> JSValue;
    fn ns_perf_realm_now_ms(ctx: *mut JSContext) -> f64;
    fn ns_box_for_this(ctx: *mut JSContext, this_val: JSValue) -> *const NsBox;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_box_border_box(b: *const NsBox, x: *mut f64, y: *mut f64, w: *mut f64, h: *mut f64);
    fn ns_box_visual_border_box(
        b: *const NsBox,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    );
    fn ns_box_visual_padding_box(
        b: *const NsBox,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    );
    fn ns_css_prop_id(name: *const c_char) -> c_int;
    fn ns_style_keyword(style: *const Style, prop: c_int) -> *const c_char;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

pub(crate) fn has_main_context(js: Js) -> bool {
    !js.is_null() && !unsafe { ns_js_main_context(js.ptr()) }.is_null()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    if js.is_null() {
        return;
    }
    if let Ok(line) = CString::new(line) {
        unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) };
    }
}

pub(crate) fn schedule_tick(js: Js) {
    if !js.is_null() {
        unsafe { ns_observer_schedule_tick(js.ptr()) };
    }
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    unsafe { BoxRef::from_ptr(ns_js_layout_root(js.ptr())) }
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
    let mut raw_args: Vec<JSValue> = args.iter().map(quickjs::raw).collect();
    let raw = unsafe {
        ns_js_call_observer(
            js.ptr(),
            quickjs::raw_context(scope),
            quickjs::raw(callback),
            quickjs::raw(this),
            raw_args.len() as c_int,
            raw_args.as_mut_ptr(),
            report_type.map_or(core::ptr::null(), CStr::as_ptr),
            southstar_glib::boolean(fresh_budget),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn make_element(scope: &mut Scope<'_>, node: usize) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node as *const NsNode) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn node_address(value: &Value) -> usize {
    unsafe { ns_unwrap_element(quickjs::raw(value)) as usize }
}

pub(crate) fn arm_invalidate(node: usize) {
    unsafe { ns_node_arm_js_invalidate(node as *const NsNode) };
}

pub(crate) fn dom_rect(scope: &mut Scope<'_>, rect: Rect) -> Value {
    let raw =
        unsafe { ns_make_dom_rect(quickjs::raw_context(scope), rect.x, rect.y, rect.w, rect.h) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn realm_now_ms(scope: &mut Scope<'_>) -> f64 {
    unsafe { ns_perf_realm_now_ms(quickjs::raw_context(scope)) }
}

pub(crate) fn box_for(scope: &mut Scope<'_>, value: &Value) -> Option<BoxRef<'static>> {
    unsafe {
        BoxRef::from_ptr(ns_box_for_this(
            quickjs::raw_context(scope),
            quickjs::raw(value),
        ))
    }
}

pub(crate) fn find_by_dom(root: BoxRef<'_>, node: usize) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node as *const NsNode)) }
}

type BoxGeometry = unsafe extern "C" fn(*const NsBox, *mut f64, *mut f64, *mut f64, *mut f64);

fn geometry(b: BoxRef<'_>, f: BoxGeometry) -> Rect {
    let mut rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };
    unsafe {
        f(
            b.as_ptr(),
            &mut rect.x,
            &mut rect.y,
            &mut rect.w,
            &mut rect.h,
        )
    };
    rect
}

pub(crate) fn border_box(b: BoxRef<'_>) -> Rect {
    geometry(b, ns_box_border_box)
}

pub(crate) fn visual_border_box(b: BoxRef<'_>) -> Rect {
    geometry(b, ns_box_visual_border_box)
}

pub(crate) fn visual_padding_box(b: BoxRef<'_>) -> Rect {
    geometry(b, ns_box_visual_padding_box)
}

pub(crate) fn style_keyword(b: BoxRef<'_>, property: &CStr) -> Option<&'static CStr> {
    let prop = unsafe { ns_css_prop_id(property.as_ptr()) };
    let keyword = unsafe { ns_style_keyword(b.style(), prop) };
    (!keyword.is_null()).then(|| unsafe { CStr::from_ptr(keyword) })
}

unsafe fn bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_bytes())
    }
}

unsafe fn nodes(array: *const GPtrArray) -> Option<Vec<usize>> {
    let array = unsafe { array.as_ref() }?;
    if array.len == 0 {
        return None;
    }
    Some(
        (0..array.len as usize)
            .map(|i| unsafe { *array.pdata.add(i) } as usize)
            .collect(),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mut_record_emit(
    js: *const NsJs,
    kind: *const c_char,
    target: *const NsNode,
    added: *const NsNode,
    removed: *const NsNode,
    previous_sibling: *const NsNode,
    next_sibling: *const NsNode,
    attr_name: *const c_char,
    attr_namespace: *const c_char,
    old_value: *const c_char,
) {
    let emit = Emit {
        kind: unsafe { bytes(kind) },
        target: target as usize,
        added: (!added.is_null()).then(|| vec![added as usize]),
        removed: (!removed.is_null()).then(|| vec![removed as usize]),
        previous_sibling: previous_sibling as usize,
        next_sibling: next_sibling as usize,
        attribute_name: unsafe { bytes(attr_name) },
        attribute_namespace: unsafe { bytes(attr_namespace) },
        old_value: unsafe { bytes(old_value) },
    };
    mutation::emit(Js::of(js), &emit);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mut_record_emit_child_list_arrays(
    js: *const NsJs,
    target: *const NsNode,
    added: *const GPtrArray,
    removed: *const GPtrArray,
    previous_sibling: *const NsNode,
    next_sibling: *const NsNode,
) {
    mutation::emit_child_list(
        Js::of(js),
        target as usize,
        unsafe { nodes(added) },
        unsafe { nodes(removed) },
        previous_sibling as usize,
        next_sibling as usize,
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mut_scrub_node(js: *const NsJs, node: *const NsNode) {
    if !node.is_null() {
        mutation::scrub_node(Js::of(js), node as usize);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_mutation_drain_pending(js: *const NsJs) -> GBoolean {
    southstar_glib::boolean(page(Js::of(js)).is_some_and(|page| page.drain_scheduled.get()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_intersection_observers_tick(js: *const NsJs) {
    intersection::tick(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_resize_observers_tick(js: *const NsJs) {
    resize::tick(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_observers_reset(js: *const NsJs) {
    crate::reset(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_observers_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

export_native! {
    ns_mutation_observer_ctor => mutation::constructor,
    ns_mutation_observer_observe => mutation::observe,
    ns_mutation_observer_disconnect => mutation::disconnect,
    ns_mutation_observer_takeRecords => mutation::take_records,
    ns_intersection_observer_ctor => intersection::constructor,
    ns_intersection_observer_observe => intersection::observe,
    ns_intersection_observer_unobserve => intersection::unobserve,
    ns_intersection_observer_disconnect => intersection::disconnect,
    ns_intersection_observer_takeRecords => intersection::take_records,
    ns_resize_observer_ctor => resize::constructor,
    ns_resize_observer_observe => resize::observe,
    ns_resize_observer_unobserve => resize::unobserve,
    ns_resize_observer_disconnect => resize::disconnect,
}
