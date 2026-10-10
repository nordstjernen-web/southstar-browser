//! Southstar — the C ABI of the geometry and scrolling bindings as declared in src/js_internal.h, and the js.c, layout and style calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GHashTable};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_mat4::Mat4;
use southstar_style::{StyleRef, StyleTable, Transform};

use crate::{Element, Rect};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_make_dom_rect(ctx: *mut JSContext, x: f64, y: f64, w: f64, h: f64) -> JSValue;
    fn ns_dommatrix_make(
        ctx: *mut JSContext,
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        e: f64,
        f: f64,
    ) -> JSValue;
    fn ns_make_svg_animated_length(
        ctx: *mut JSContext,
        node: *const NsNode,
        attr: *const c_char,
    ) -> JSValue;
    fn ns_element_int_attr_getter(ctx: *mut JSContext, this_val: JSValue, magic: c_int) -> JSValue;
    fn ns_element_img_natural_width(ctx: *mut JSContext, this_val: JSValue) -> JSValue;
    fn ns_element_img_natural_height(ctx: *mut JSContext, this_val: JSValue) -> JSValue;
    fn ns_element_get_attr_len(
        node: *const NsNode,
        name: *const c_char,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_js_flush_layout(js: *mut NsJs);
    fn ns_js_layout_root(js: *const NsJs) -> *const NsBox;
    fn ns_js_style_table(js: *const NsJs) -> *mut GHashTable;
    fn ns_js_scroll_viewport(js: *mut NsJs, x: f64, y: f64);
    fn ns_js_note_viewport_scroll(js: *mut NsJs, x: f64, y: f64);
    fn ns_js_request_repaint(js: *mut NsJs);
    fn ns_js_notify_scroll_to(js: *mut NsJs, node: *const NsNode);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_queue_scrollend(js: *mut NsJs, node: *const NsNode);
    fn ns_observer_schedule_tick(js: *mut NsJs);
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_box_inline_rect_for_dom(
        root: *const NsBox,
        target: *const NsNode,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    ) -> GBoolean;
    fn ns_box_hit_offset(b: *const NsBox, dx: *mut f64, dy: *mut f64);
    fn ns_box_scroll_snap(b: *mut NsBox);
    fn ns_css_style_effective_transform(
        style: *const Style,
        transform_override: *const Transform,
        out: *mut Transform,
    );
    fn ns_css_transform_to_mat4(tf: *const Transform, bw: f64, bh: f64, out: *mut Mat4);
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn g_ascii_strtod(text: *const c_char, end: *mut *mut c_char) -> f64;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope).cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_rect(scope: &mut Scope<'_>, r: Rect) -> Value {
    let raw = unsafe { ns_make_dom_rect(quickjs::raw_context(scope), r.x, r.y, r.w, r.h) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_matrix(scope: &mut Scope<'_>, m: [f64; 6]) -> Value {
    let raw = unsafe {
        ns_dommatrix_make(
            quickjs::raw_context(scope),
            m[0],
            m[1],
            m[2],
            m[3],
            m[4],
            m[5],
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn svg_animated_length(scope: &mut Scope<'_>, node: Element, attr: &CStr) -> Value {
    let raw = unsafe {
        ns_make_svg_animated_length(quickjs::raw_context(scope), node.as_ptr(), attr.as_ptr())
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn int_attr_getter(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsValueResult {
    let raw = unsafe {
        ns_element_int_attr_getter(quickjs::raw_context(scope), quickjs::raw(this), magic)
    };
    checked(scope, raw)
}

pub(crate) fn img_natural_size(scope: &mut Scope<'_>, this: &Value, width: bool) -> JsValueResult {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe {
        if width {
            ns_element_img_natural_width(ctx, quickjs::raw(this))
        } else {
            ns_element_img_natural_height(ctx, quickjs::raw(this))
        }
    };
    checked(scope, raw)
}

type JsValueResult = Result<Value, Value>;

fn checked(scope: &mut Scope<'_>, raw: JSValue) -> JsValueResult {
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn attr_bytes(node: Element, name: &CStr) -> Option<&'static [u8]> {
    let mut len = 0usize;
    let v = unsafe { ns_element_get_attr_len(node.as_ptr(), name.as_ptr(), &mut len) };
    if v.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(v.cast::<u8>(), len) })
}

pub(crate) fn flush_layout(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_flush_layout(js.0) };
    }
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    unsafe { BoxRef::from_ptr(ns_js_layout_root(js.0)) }
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(ns_js_style_table(js.0)) }
}

pub(crate) fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'static>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub(crate) fn svg_styles(b: BoxRef<'_>) -> StyleTable {
    unsafe { StyleTable::from_ptr(b.svg_styles()) }
}

pub(crate) fn box_dom(b: BoxRef<'_>) -> Option<Element> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub(crate) fn scroll_viewport(js: Js, x: f64, y: f64) {
    if !js.is_null() {
        unsafe { ns_js_scroll_viewport(js.0, x, y) };
    }
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    unsafe { ns_js_note_viewport_scroll(js.0, x, y) };
}

pub(crate) fn request_repaint(js: Js) {
    unsafe { ns_js_request_repaint(js.0) };
}

pub(crate) fn notify_scroll_to(js: Js, node: Element) {
    unsafe { ns_js_notify_scroll_to(js.0, node.as_ptr()) };
}

pub(crate) fn dispatch_scroll(js: Js, node: Element) {
    unsafe {
        ns_js_dispatch_event(js.0, node.as_ptr(), c"scroll".as_ptr(), ptr::null_mut());
        ns_js_queue_scrollend(js.0, node.as_ptr());
    }
}

pub(crate) fn observer_schedule_tick(js: Js) {
    unsafe { ns_observer_schedule_tick(js.0) };
}

pub(crate) fn set_attr_recorded(js: Js, node: Element, name: &[u8], value: &[u8]) {
    let name = CString::new(name).unwrap_or_default();
    let value = CString::new(value).unwrap_or_default();
    unsafe { ns_js_set_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn find_by_dom(root: BoxRef<'_>, node: Element) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node.as_ptr())) }
}

pub(crate) fn inline_rect_for_dom(root: BoxRef<'_>, node: Element) -> Option<Rect> {
    let mut r = Rect::default();
    let found = unsafe {
        ns_box_inline_rect_for_dom(
            root.as_ptr(),
            node.as_ptr(),
            &mut r.x,
            &mut r.y,
            &mut r.w,
            &mut r.h,
        )
    };
    (found != 0).then_some(r)
}

pub(crate) fn hit_offset(b: BoxRef<'_>) -> (f64, f64) {
    let (mut dx, mut dy) = (0.0, 0.0);
    unsafe { ns_box_hit_offset(b.as_ptr(), &mut dx, &mut dy) };
    (dx, dy)
}

pub(crate) fn scroll_snap(b: BoxRef<'_>) {
    unsafe { ns_box_scroll_snap(b.as_ptr().cast_mut()) };
}

pub(crate) fn effective_transform_matrix(style: StyleRef<'_>, bw: f64, bh: f64) -> Option<Mat4> {
    let mut eff = Transform::default();
    unsafe { ns_css_style_effective_transform(style.as_ptr().cast(), ptr::null(), &mut eff) };
    if eff.n_ops <= 0 {
        return None;
    }
    let mut out = Mat4::IDENTITY;
    unsafe { ns_css_transform_to_mat4(&eff, bw, bh, &mut out) };
    Some(out)
}

pub(crate) fn viewport_w() -> f64 {
    unsafe { ns_css_viewport_w() }
}

pub(crate) fn viewport_h() -> f64 {
    unsafe { ns_css_viewport_h() }
}

pub(crate) fn ascii_strtod(text: &CStr, at: usize) -> Option<(f64, usize)> {
    let start = text.as_ptr().wrapping_add(at);
    let mut end: *mut c_char = ptr::null_mut();
    let v = unsafe { g_ascii_strtod(start, &mut end) };
    let used = (end as usize).wrapping_sub(start as usize);
    (!end.is_null() && used > 0).then_some((v, at + used))
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

unsafe fn getter(ctx: *mut JSContext, this_val: JSValue, f: NativeFn) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

unsafe fn setter(ctx: *mut JSContext, this_val: JSValue, val: JSValue, f: NativeFn) -> JSValue {
    let mut argv = [val];
    unsafe { quickjs::call_native(ctx, this_val, 1, argv.as_mut_ptr(), f) }
}

fn write_rect(r: Rect, x: *mut f64, y: *mut f64, w: *mut f64, h: *mut f64) {
    unsafe {
        *x = r.x;
        *y = r.y;
        *w = r.w;
        *h = r.h;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_border_box(
    b: *const NsBox,
    x: *mut f64,
    y: *mut f64,
    w: *mut f64,
    h: *mut f64,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        write_rect(crate::boxes::border_box(b), x, y, w, h);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_visual_border_box(
    b: *const NsBox,
    x: *mut f64,
    y: *mut f64,
    w: *mut f64,
    h: *mut f64,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        write_rect(crate::boxes::visual_border_box(b), x, y, w, h);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_visual_padding_box(
    b: *const NsBox,
    x: *mut f64,
    y: *mut f64,
    w: *mut f64,
    h: *mut f64,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        write_rect(crate::boxes::visual_padding_box(b), x, y, w, h);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_for_this(ctx: *mut JSContext, this_val: JSValue) -> *const NsBox {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            crate::boxes::box_for_this(scope, &this).map_or(ptr::null(), BoxRef::as_ptr)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_scroll_prop(ctx: *mut JSContext, prop: *const c_char) -> f64 {
    if prop.is_null() {
        return 0.0;
    }
    let prop = unsafe { CStr::from_ptr(prop) }.to_str().unwrap_or_default();
    unsafe { quickjs::with_context(ctx, |scope| crate::boxes::window_scroll(scope, prop)) }
}

macro_rules! natives {
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

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { getter(ctx, this_val, $f) }
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, val, $f) }
            }
        )*
    };
}

natives! {
    ns_element_getBoundingClientRect => crate::metrics::get_bounding_client_rect,
    ns_element_getClientRects => crate::metrics::get_client_rects,
    ns_element_scroll_to => crate::scroll::element_scroll_to,
    ns_element_scroll_by => crate::scroll::element_scroll_by,
    ns_element_scrollIntoView => crate::scroll::scroll_into_view,
    ns_window_scroll_to => crate::scroll::window_scroll_to,
    ns_window_scroll_by => crate::scroll::window_scroll_by,
    ns_window_scroll_by_lines => crate::scroll::window_scroll_by_lines,
    ns_window_scroll_by_pages => crate::scroll::window_scroll_by_pages,
    ns_element_getBBox => crate::svg::get_bbox,
    ns_element_getCTM => crate::svg::get_ctm,
    ns_element_getScreenCTM => crate::svg::get_screen_ctm,
    ns_element_getTotalLength => crate::svg::get_total_length,
    ns_element_getPointAtLength => crate::svg::get_point_at_length,
    ns_element_createSVGPoint => crate::svg::create_svg_point,
    ns_element_createSVGRect => crate::svg::create_svg_rect,
    ns_element_createSVGMatrix => crate::svg::create_svg_matrix,
    ns_element_createSVGTransform => crate::svg::create_svg_transform,
    ns_svg_beginElement => crate::svg::begin_element,
    ns_svg_setCurrentTime => crate::svg::set_current_time,
}

getters! {
    ns_element_get_offsetWidth => crate::metrics::get_offset_width,
    ns_element_get_offsetHeight => crate::metrics::get_offset_height,
    ns_element_get_offsetTop => crate::metrics::get_offset_top,
    ns_element_get_offsetLeft => crate::metrics::get_offset_left,
    ns_element_get_offsetParent => crate::metrics::get_offset_parent,
    ns_element_get_clientWidth => crate::metrics::get_client_width,
    ns_element_get_clientHeight => crate::metrics::get_client_height,
    ns_element_get_clientTop => crate::metrics::get_client_top,
    ns_element_get_clientLeft => crate::metrics::get_client_left,
    ns_element_get_scrollTop => crate::metrics::get_scroll_top,
    ns_element_get_scrollLeft => crate::metrics::get_scroll_left,
    ns_element_get_scrollWidth => crate::metrics::get_scroll_width,
    ns_element_get_scrollHeight => crate::metrics::get_scroll_height,
    ns_element_get_ownerSVGElement => crate::svg::get_owner_svg_element,
}

setters! {
    ns_element_set_scrollTop => crate::scroll::set_scroll_top,
    ns_element_set_scrollLeft => crate::scroll::set_scroll_left,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_dimension_getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
            crate::metrics::dimension(scope, this, magic)
        })
    }
}
