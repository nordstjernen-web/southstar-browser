//! Southstar — the C ABI and Rust-ABI entry points of the geometry and scrolling bindings, and the engine, layout and style calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};
use southstar_mat4::Mat4;
use southstar_style::{StyleRef, StyleTable};

use crate::{Element, JsResult, Rect};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn addr(self) -> usize {
        self.0 as usize
    }
}

unsafe extern "C" {
    fn ns_element_get_attr_len(
        node: *const NsNode,
        name: *const c_char,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_js_note_viewport_scroll(js: *mut NsJs, x: f64, y: f64);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_queue_scrollend(js: *mut NsJs, node: *const NsNode);
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_box_hit_test_local(
        root: *const NsBox,
        x: f64,
        y: f64,
        local_x: *mut f64,
        local_y: *mut f64,
    ) -> *const NsBox;
    fn ns_box_image_map_area(b: *const NsBox, local_x: f64, local_y: f64) -> *const NsNode;
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
    fn g_ascii_strtod(text: *const c_char, end: *mut *mut c_char) -> f64;
    fn ns_js_computed_text(
        ctx: ContextHandle,
        node: *const NsNode,
        name: *const c_char,
    ) -> *mut c_char;
}

unsafe extern "Rust" {
    safe fn nsr_dommatrix_make(scope: &mut Scope<'_>, m: [f64; 6]) -> Value;
    safe fn nsr_element_int_attr_getter(
        scope: &mut Scope<'_>,
        this: &Value,
        magic: i32,
    ) -> Result<Value, Value>;
    safe fn nsr_element_img_natural_width(
        scope: &mut Scope<'_>,
        this: &Value,
    ) -> Result<Value, Value>;
    safe fn nsr_element_img_natural_height(
        scope: &mut Scope<'_>,
        this: &Value,
    ) -> Result<Value, Value>;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn dom_rect(scope: &mut Scope<'_>, r: Rect) -> Value {
    crate::boxes::make_dom_rect(scope, r)
}

pub(crate) fn dom_matrix(scope: &mut Scope<'_>, m: [f64; 6]) -> Value {
    nsr_dommatrix_make(scope, m)
}

pub(crate) fn int_attr_getter(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    nsr_element_int_attr_getter(scope, this, magic)
}

pub(crate) fn img_natural_size(scope: &mut Scope<'_>, this: &Value, width: bool) -> JsResult {
    if width {
        nsr_element_img_natural_width(scope, this)
    } else {
        nsr_element_img_natural_height(scope, this)
    }
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
    api::flush_layout(js.0);
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    unsafe { BoxRef::from_ptr(api::layout_root(js.0).cast::<NsBox>()) }
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(api::style_table(js.0)) }
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
    api::scroll_viewport(js.0, x, y);
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    unsafe { ns_js_note_viewport_scroll(js.0, x, y) };
}

pub(crate) fn request_repaint(js: Js) {
    api::request_repaint(js.0);
}

pub(crate) fn notify_scroll_to(js: Js, node: Element) {
    api::notify_scroll_to(js.0, node);
}

pub(crate) fn dispatch_scroll(js: Js, node: Element) {
    unsafe {
        ns_js_dispatch_event(js.0, node.as_ptr(), c"scroll".as_ptr(), ptr::null_mut());
        ns_js_queue_scrollend(js.0, node.as_ptr());
    }
}

pub(crate) fn observer_schedule_tick(js: Js) {
    api::observer_schedule_tick(js.0);
}

pub(crate) fn set_attr_recorded(js: Js, node: Element, name: &[u8], value: &[u8]) {
    let name = CString::new(name).unwrap_or_default();
    let value = CString::new(value).unwrap_or_default();
    unsafe { ns_js_set_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn find_by_dom<'a>(root: BoxRef<'a>, node: Element) -> Option<BoxRef<'a>> {
    crate::lookup::find_by_dom(root, node.as_ptr().cast())
}

pub(crate) fn active_js() -> Option<Js> {
    let js = api::active();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn computed_text(scope: &mut Scope<'_>, node: Element, name: &CStr) -> Option<CString> {
    let text = unsafe { ns_js_computed_text(scope.handle(), node.as_ptr(), name.as_ptr()) };
    if text.is_null() {
        return None;
    }
    let owned = unsafe { CStr::from_ptr(text) }.to_owned();
    unsafe { southstar_glib::g_free(text.cast()) };
    Some(owned)
}

pub(crate) fn hit_test_local(
    root: BoxRef<'_>,
    x: f64,
    y: f64,
) -> Option<(BoxRef<'static>, f64, f64)> {
    let (mut local_x, mut local_y) = (0.0, 0.0);
    let hit = unsafe { ns_box_hit_test_local(root.as_ptr(), x, y, &mut local_x, &mut local_y) };
    unsafe { BoxRef::from_ptr(hit) }.map(|b| (b, local_x, local_y))
}

pub(crate) fn image_map_area(b: BoxRef<'_>, local_x: f64, local_y: f64) -> Option<Element> {
    unsafe { Node::from_ptr(ns_box_image_map_area(b.as_ptr(), local_x, local_y)) }
}

pub(crate) fn current_doc(js: Js) -> Option<Element> {
    api::current_document(js.0)
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
    let eff = southstar_style::effective_transform(Some(style), None);
    (eff.n_ops > 0).then(|| southstar_style::transform_to_mat4(&eff, bw, bh))
}

pub(crate) fn viewport_w() -> f64 {
    southstar_css::viewport().0
}

pub(crate) fn viewport_h() -> f64 {
    southstar_css::viewport().1
}

pub(crate) fn ascii_strtod(text: &CStr, at: usize) -> Option<(f64, usize)> {
    let start = text.as_ptr().wrapping_add(at);
    let mut end: *mut c_char = ptr::null_mut();
    let v = unsafe { g_ascii_strtod(start, &mut end) };
    let used = (end as usize).wrapping_sub(start as usize);
    (!end.is_null() && used > 0).then_some((v, at + used))
}

pub(crate) fn ascii_strtod_prefix(text: &CStr) -> (f64, usize) {
    let start = text.as_ptr();
    let mut end: *mut c_char = ptr::null_mut();
    let v = unsafe { g_ascii_strtod(start, &mut end) };
    let used = if end.is_null() {
        0
    } else {
        (end as usize).wrapping_sub(start as usize)
    };
    (v, used.min(text.to_bytes().len()))
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
pub fn nsr_box_for_this(scope: &mut Scope<'_>, this: &Value) -> *const NsBox {
    crate::boxes::box_for_this(scope, this).map_or(ptr::null(), BoxRef::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_scroll_prop(ctx: ContextHandle, prop: *const c_char) -> f64 {
    if prop.is_null() {
        return 0.0;
    }
    let prop = unsafe { CStr::from_ptr(prop) }.to_str().unwrap_or_default();
    ctx.with(|scope| crate::boxes::window_scroll(scope, prop))
        .unwrap_or(0.0)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
                $f(scope, this, &[])
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

natives! {
    nsr_element_getBoundingClientRect => crate::metrics::get_bounding_client_rect,
    nsr_element_getClientRects => crate::metrics::get_client_rects,
    nsr_element_scroll_to => crate::scroll::element_scroll_to,
    nsr_element_scroll_by => crate::scroll::element_scroll_by,
    nsr_element_scrollIntoView => crate::scroll::scroll_into_view,
    nsr_window_scroll_to => crate::scroll::window_scroll_to,
    nsr_window_scroll_by => crate::scroll::window_scroll_by,
    nsr_window_scroll_by_lines => crate::scroll::window_scroll_by_lines,
    nsr_window_scroll_by_pages => crate::scroll::window_scroll_by_pages,
    nsr_element_getBBox => crate::svg::get_bbox,
    nsr_element_getCTM => crate::svg::get_ctm,
    nsr_element_getScreenCTM => crate::svg::get_screen_ctm,
    nsr_element_getTotalLength => crate::svg::get_total_length,
    nsr_element_getPointAtLength => crate::svg::get_point_at_length,
    nsr_element_createSVGPoint => crate::svg::create_svg_point,
    nsr_element_createSVGRect => crate::svg::create_svg_rect,
    nsr_element_createSVGMatrix => crate::svg::create_svg_matrix,
    nsr_element_createSVGTransform => crate::svg::create_svg_transform,
    nsr_element_checkVisibility => crate::visibility::check_visibility,
    nsr_svg_beginElement => crate::svg::begin_element,
    nsr_svg_setCurrentTime => crate::svg::set_current_time,
    nsr_document_element_from_point => crate::hit::element_from_point,
    nsr_document_elements_from_point => crate::hit::elements_from_point,
}

getters! {
    nsr_element_get_offsetWidth => crate::metrics::get_offset_width,
    nsr_element_get_offsetHeight => crate::metrics::get_offset_height,
    nsr_element_get_offsetTop => crate::metrics::get_offset_top,
    nsr_element_get_offsetLeft => crate::metrics::get_offset_left,
    nsr_element_get_offsetParent => crate::metrics::get_offset_parent,
    nsr_element_get_clientWidth => crate::metrics::get_client_width,
    nsr_element_get_clientHeight => crate::metrics::get_client_height,
    nsr_element_get_clientTop => crate::metrics::get_client_top,
    nsr_element_get_clientLeft => crate::metrics::get_client_left,
    nsr_element_get_scrollTop => crate::metrics::get_scroll_top,
    nsr_element_get_scrollLeft => crate::metrics::get_scroll_left,
    nsr_element_get_scrollWidth => crate::metrics::get_scroll_width,
    nsr_element_get_scrollHeight => crate::metrics::get_scroll_height,
    nsr_element_get_ownerSVGElement => crate::svg::get_owner_svg_element,
}

setters! {
    nsr_element_set_scrollTop => crate::scroll::set_scroll_top,
    nsr_element_set_scrollLeft => crate::scroll::set_scroll_left,
}

#[unsafe(no_mangle)]
pub fn nsr_make_dom_rect(scope: &mut Scope<'_>, x: f64, y: f64, w: f64, h: f64) -> Value {
    crate::boxes::make_dom_rect(scope, Rect { x, y, w, h })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_box_find_by_dom(
    root: *const NsBox,
    target: *const NsNode,
) -> *const NsBox {
    let Some(root) = (unsafe { BoxRef::from_ptr(root) }) else {
        return ptr::null();
    };
    if target.is_null() {
        return ptr::null();
    }
    crate::lookup::find_by_dom(root, target.cast()).map_or(ptr::null(), BoxRef::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_box_for_node(js: *mut NsJs, n: *const NsNode) -> *const NsBox {
    let Some(root) = layout_root(Js(js)) else {
        return ptr::null();
    };
    if n.is_null() {
        return ptr::null();
    }
    crate::lookup::find_by_dom(root, n.cast()).map_or(ptr::null(), BoxRef::as_ptr)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_box_lookup_forget(js: *mut NsJs) {
    crate::lookup::forget(Js(js));
}
