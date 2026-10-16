//! Southstar — the canvas drawing state: its cairo surface and context, the 2D context object it belongs to, its lifetime and its C ABI.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;
use southstar_js_engine::ContextHandle;

use super::cairo::{Cairo, Surface};

#[repr(C)]
pub struct CanvasState {
    pub w: c_int,
    pub h: c_int,
    pub surf: *mut c_void,
    pub cr: *mut Cairo,
    pub fill: [c_double; 4],
    pub stroke: [c_double; 4],
    pub line_width: c_double,
    pub font: *mut c_char,
    pub fill_pattern: *mut c_void,
    pub stroke_pattern: *mut c_void,
    pub shadow: [c_double; 4],
    pub shadow_blur: c_double,
    pub shadow_ox: c_double,
    pub shadow_oy: c_double,
    pub origin_clean: c_int,
    pub ctx2d: *mut c_void,
    pub jsctx: ContextHandle,
    pub owned_node: *mut c_void,
    pub context_kind: c_int,
}

pub use southstar_js_runtime::api::NsJs;

#[repr(C)]
pub struct NsNode {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

#[derive(Clone, Copy)]
pub(crate) struct Node(*const NsNode);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn addr(self) -> usize {
        self.0 as usize
    }

    pub fn ptr(self) -> *mut NsJs {
        self.0
    }
}

impl Node {
    pub fn from_addr(addr: usize) -> Node {
        Node(addr as *const NsNode)
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn addr(self) -> usize {
        self.0 as usize
    }

    pub fn ptr(self) -> *const NsNode {
        self.0
    }
}

unsafe extern "C" {
    fn cairo_create(target: *mut c_void) -> *mut Cairo;
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_pattern_destroy(pattern: *mut c_void);
    fn cairo_set_operator(cr: *mut Cairo, op: c_int);
    fn cairo_paint(cr: *mut Cairo);
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_node_free(node: *mut c_void);
    fn ns_webgl_canvas_surface(canvas: *const NsNode) -> *mut c_void;
}

#[cfg(feature = "webgpu")]
unsafe extern "C" {
    fn ns_webgpu_canvas_surface(canvas: *const NsNode) -> *mut c_void;
}

const OPERATOR_CLEAR: c_int = 0;

pub(crate) fn element_attr(el: Node, name: &str) -> Option<Vec<u8>> {
    let name = CString::new(name).ok()?;
    let value = unsafe { ns_element_get_attr(el.0, name.as_ptr()) };
    (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) }.to_bytes().to_vec())
}

pub(crate) fn parse_int(text: &[u8], default: i32, min: i32, max: i32) -> i32 {
    let text = CString::new(text).unwrap_or_default();
    southstar_dom::controls::parse_int(Some(&text), default, min, max)
}

pub(crate) fn size(st: *mut CanvasState) -> (i32, i32) {
    let st = unsafe { &*st };
    (st.w, st.h)
}

fn release_drawing(st: &mut CanvasState) {
    unsafe {
        if !st.fill_pattern.is_null() {
            cairo_pattern_destroy(st.fill_pattern);
            st.fill_pattern = ptr::null_mut();
        }
        if !st.stroke_pattern.is_null() {
            cairo_pattern_destroy(st.stroke_pattern);
            st.stroke_pattern = ptr::null_mut();
        }
        if !st.cr.is_null() {
            cairo_destroy(st.cr);
        }
        if !st.surf.is_null() {
            drop(Surface::from_raw(st.surf));
        }
        glib::g_free(st.font.cast());
    }
}

pub(crate) fn reset(st: *mut CanvasState, w: i32, h: i32) {
    let st = unsafe { &mut *st };
    release_drawing(st);
    st.w = w;
    st.h = h;
    st.surf = Surface::image(w, h).into_raw();
    st.cr = unsafe { cairo_create(st.surf) };
    st.fill = [0.0, 0.0, 0.0, 1.0];
    st.stroke = [0.0, 0.0, 0.0, 1.0];
    st.line_width = 1.0;
    st.font = glib::strdup(b"10px sans-serif");
    st.shadow = [0.0; 4];
    st.shadow_blur = 0.0;
    st.shadow_ox = 0.0;
    st.shadow_oy = 0.0;
    st.origin_clean = 1;
    if !st.ctx2d.is_null() {
        let ctx2d = st.ctx2d;
        st.jsctx.with(|scope| {
            let obj = scope.object_from_identity(ctx2d);
            crate::api::ctx2d_init_state(scope, &obj);
        });
    }
}

pub(crate) fn new_state(w: i32, h: i32) -> Box<CanvasState> {
    let mut st = Box::new(CanvasState {
        w: 0,
        h: 0,
        surf: ptr::null_mut(),
        cr: ptr::null_mut(),
        fill: [0.0; 4],
        stroke: [0.0; 4],
        line_width: 0.0,
        font: ptr::null_mut(),
        fill_pattern: ptr::null_mut(),
        stroke_pattern: ptr::null_mut(),
        shadow: [0.0; 4],
        shadow_blur: 0.0,
        shadow_ox: 0.0,
        shadow_oy: 0.0,
        origin_clean: 0,
        ctx2d: ptr::null_mut(),
        jsctx: ContextHandle::NULL,
        owned_node: ptr::null_mut(),
        context_kind: 0,
    });
    reset(&mut *st, w, h);
    st
}

impl Drop for CanvasState {
    fn drop(&mut self) {
        release_drawing(self);
        let ctx2d = core::mem::replace(&mut self.ctx2d, ptr::null_mut());
        if !ctx2d.is_null() {
            self.jsctx
                .with(|scope| unsafe { scope.release_identity(ctx2d) });
        }
        if !self.owned_node.is_null() {
            unsafe { ns_node_free(self.owned_node) };
        }
    }
}

fn clear_surface(st: &CanvasState) {
    if st.surf.is_null() {
        return;
    }
    unsafe {
        let cr = cairo_create(st.surf);
        cairo_set_operator(cr, OPERATOR_CLEAR);
        cairo_paint(cr);
        cairo_destroy(cr);
    }
}

pub(crate) fn canvas_surface(js: Js, el: Node) -> *mut c_void {
    if el.is_null() {
        return ptr::null_mut();
    }
    let gl = unsafe { ns_webgl_canvas_surface(el.0) };
    if !gl.is_null() {
        return gl;
    }
    #[cfg(feature = "webgpu")]
    {
        let gpu = unsafe { ns_webgpu_canvas_surface(el.0) };
        if !gpu.is_null() {
            return gpu;
        }
    }
    if js.is_null() {
        return ptr::null_mut();
    }
    match crate::state::lookup(js, el) {
        Some(st) => unsafe { (*st).surf },
        None => ptr::null_mut(),
    }
}

pub(crate) fn js_of(scope: &southstar_js_engine::Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn transfer_to_image_bitmap(
    scope: &mut southstar_js_engine::Scope<'_>,
    this: &southstar_js_engine::Value,
    _: &[southstar_js_engine::Value],
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    use southstar_js_engine::Value;
    let js = js_of(scope);
    let el = Node(crate::api::offscreen_node(this) as *const NsNode);
    if js.is_null() || el.is_null() {
        return Ok(Value::null());
    }
    let Some(src) = (unsafe { Surface::from_borrowed(canvas_surface(js, el)) }) else {
        return Ok(Value::null());
    };
    let (w, h) = src.size();
    if w <= 0 || h <= 0 {
        return Ok(Value::null());
    }
    let copy = Surface::image(w, h);
    if !copy.is_ok() {
        return Ok(Value::null());
    }
    src.paint_onto(&copy, (0.0, 0.0), true);
    let st = crate::state::lookup(js, el);
    if let Some(st) = st {
        clear_surface(unsafe { &*st });
    }
    let origin_clean = st.is_none_or(|st| unsafe { (*st).origin_clean } != 0);
    Ok(crate::bitmap::make(scope, Some(copy), (w, h), origin_clean))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_states_teardown(js: *mut NsJs) {
    crate::state::teardown(Js(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_canvas_surface(js: *mut NsJs, el: *const NsNode) -> *mut c_void {
    canvas_surface(Js(js), Node(el))
}

pub(crate) fn clear_patterns(st: &mut CanvasState) {
    unsafe {
        if !st.fill_pattern.is_null() {
            cairo_pattern_destroy(st.fill_pattern);
            st.fill_pattern = ptr::null_mut();
        }
        if !st.stroke_pattern.is_null() {
            cairo_pattern_destroy(st.stroke_pattern);
            st.stroke_pattern = ptr::null_mut();
        }
    }
}

pub(crate) fn set_font(st: &mut CanvasState, font: &[u8]) {
    unsafe { glib::g_free(st.font.cast()) };
    st.font = glib::strdup(font);
}
