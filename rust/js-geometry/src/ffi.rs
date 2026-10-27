//! Southstar — the C ABI and Rust-ABI entry points of the geometry and scrolling bindings, and the engine, layout and style calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::BoxRef;
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

use southstar_canvas::nsr_dommatrix_make;

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
    southstar_dom::attrs::find(node, name).and_then(southstar_dom::Attr::value_bytes)
}

pub(crate) fn flush_layout(js: Js) {
    api::flush_layout(js.0);
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    if js.is_null() {
        return None;
    }
    api::layout_root(js.0)
}

pub(crate) fn style_table(js: Js) -> StyleTable {
    if js.is_null() {
        return StyleTable::none();
    }
    unsafe { StyleTable::from_ptr(api::style_table(js.0)) }
}

pub(crate) fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'static>> {
    southstar_style::box_style(b.into_static())
}

pub(crate) fn svg_styles(b: BoxRef<'_>) -> StyleTable {
    unsafe { StyleTable::from_ptr(b.svg_styles()) }
}

pub(crate) fn box_dom(b: BoxRef<'_>) -> Option<Element> {
    b.into_static().dom()
}

pub(crate) fn scroll_viewport(js: Js, x: f64, y: f64) {
    api::scroll_viewport(js.0, x, y);
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    (api::window_hooks().note_viewport_scroll)(js.0, x, y);
}

pub(crate) fn request_repaint(js: Js) {
    api::request_repaint(js.0);
}

pub(crate) fn notify_scroll_to(js: Js, node: Element) {
    api::notify_scroll_to(js.0, node);
}

pub(crate) fn dispatch_scroll(js: Js, node: Element) {
    southstar_js_runtime::events::dispatch_event(js.0, node, "scroll");
    (api::lifecycle_hooks().queue_scrollend)(js.0, node);
}

pub(crate) fn observer_schedule_tick(js: Js) {
    api::observer_schedule_tick(js.0);
}

pub(crate) fn set_attr_recorded(js: Js, node: Element, name: &[u8], value: &[u8]) {
    let name = CString::new(name).unwrap_or_default();
    let value = CString::new(value).unwrap_or_default();
    (api::mutation_hooks().set_attr_recorded)(js.0, node, &name, value.to_bytes());
}

pub(crate) fn find_by_dom<'a>(root: BoxRef<'a>, node: Element) -> Option<BoxRef<'a>> {
    crate::lookup::find_by_dom(root, node.as_ptr().cast())
}

pub(crate) fn active_js() -> Option<Js> {
    let js = api::active();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn computed_text(scope: &mut Scope<'_>, node: Element, name: &CStr) -> Option<CString> {
    let text = southstar_js_cssom::computed_text(scope, node, &name.to_string_lossy())?;
    let end = text.find('\0').unwrap_or(text.len());
    CString::new(&text[..end]).ok()
}

pub(crate) use southstar_layout::image_map_area;
pub(crate) use southstar_layout_hit::{hit_offset, hit_test_local};

pub(crate) fn current_doc(js: Js) -> Option<Element> {
    api::current_document(js.0)
}

pub(crate) fn inline_rect_for_dom(root: BoxRef<'_>, node: Element) -> Option<Rect> {
    let (x, y, w, h) = southstar_layout_query::inline_rect_for_dom(root, node)?;
    Some(Rect { x, y, w, h })
}

use crate::links::nsr_element_img_natural_height;
use crate::links::nsr_element_img_natural_width;
use crate::links::nsr_element_int_attr_getter;
pub(crate) use southstar_layout_query::scroll_snap;

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
    let (v, end) = southstar_glib::ascii_strtod_at(text, at);
    (end > at.min(text.to_bytes().len())).then_some((v, end))
}

pub(crate) fn ascii_strtod_prefix(text: &CStr) -> (f64, usize) {
    southstar_glib::ascii_strtod_prefix(text.to_bytes())
}

pub fn box_border_box(b: BoxRef<'_>) -> Rect {
    crate::boxes::border_box(b)
}

pub fn box_visual_border_box(b: BoxRef<'_>) -> Rect {
    crate::boxes::visual_border_box(b)
}

pub fn box_visual_padding_box(b: BoxRef<'_>) -> Rect {
    crate::boxes::visual_padding_box(b)
}

pub fn find_box_by_dom<'a>(root: BoxRef<'a>, target: *const c_void) -> Option<BoxRef<'a>> {
    (!target.is_null())
        .then(|| crate::lookup::find_by_dom(root, target))
        .flatten()
}

pub fn window_scroll_prop(scope: &mut Scope<'_>, prop: &str) -> f64 {
    crate::boxes::window_scroll(scope, prop)
}

pub fn nsr_box_for_this(scope: &mut Scope<'_>, this: &Value) -> Option<BoxRef<'static>> {
    crate::boxes::box_for_this(scope, this)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
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
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

natives! {
    nsr_window_scroll_to => crate::scroll::window_scroll_to,
    nsr_window_scroll_by => crate::scroll::window_scroll_by,
    nsr_window_scroll_by_lines => crate::scroll::window_scroll_by_lines,
    nsr_window_scroll_by_pages => crate::scroll::window_scroll_by_pages,
    nsr_document_element_from_point => crate::hit::element_from_point,
    nsr_document_elements_from_point => crate::hit::elements_from_point,
}

getters! {}

setters! {}

pub fn nsr_make_dom_rect(scope: &mut Scope<'_>, x: f64, y: f64, w: f64, h: f64) -> Value {
    crate::boxes::make_dom_rect(scope, Rect { x, y, w, h })
}

fn box_for_node(scope: &Scope<'_>, node: Node<'_>) -> Option<BoxRef<'static>> {
    crate::lookup::find_by_dom(layout_root(js_of(scope))?, node.as_ptr().cast())
}

fn forget_box_lookup(js: *const NsJs) {
    crate::lookup::forget(Js(js.cast_mut()));
}

pub fn install_runtime_hooks() {
    api::set_geometry_hooks(api::GeometryHooks {
        forget_box_lookup,
        find_box_by_dom: box_find_by_dom,
    });
    southstar_canvas::links::set_js_geometry_links(southstar_canvas::links::JsGeometryLinks {
        box_for_node,
    });
    southstar_js_runtime::links::set_js_geometry_links(
        southstar_js_runtime::links::JsGeometryLinks {
            window_scroll_by_pages: crate::ffi::nsr_window_scroll_by_pages,
            window_scroll_by_lines: crate::ffi::nsr_window_scroll_by_lines,
            window_scroll_by: crate::ffi::nsr_window_scroll_by,
            window_scroll_to: crate::ffi::nsr_window_scroll_to,
        },
    );
}

fn box_find_by_dom<'a>(root: BoxRef<'a>, target: Node<'_>) -> Option<BoxRef<'a>> {
    find_box_by_dom(root, target.as_ptr().cast())
}
