//! Southstar — the canvas drawing state: its cairo surface and context, the 2D context object it belongs to, its lifetime and the surface the painter composites.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, Ref, RefCell};
use core::ffi::c_void;
use core::ptr;
use std::ffi::CString;

use southstar_js_engine::ContextHandle;

use super::cairo::{Context, Pattern, Surface};

const DEFAULT_FONT: &[u8] = b"10px sans-serif";

pub struct CanvasState {
    w: Cell<i32>,
    h: Cell<i32>,
    surf: RefCell<Surface>,
    cr: RefCell<Context>,
    pub fill: Cell<[f64; 4]>,
    pub stroke: Cell<[f64; 4]>,
    pub line_width: Cell<f64>,
    font: RefCell<Vec<u8>>,
    fill_pattern: RefCell<Option<Pattern>>,
    stroke_pattern: RefCell<Option<Pattern>>,
    pub shadow: Cell<[f64; 4]>,
    pub shadow_blur: Cell<f64>,
    pub shadow_ox: Cell<f64>,
    pub shadow_oy: Cell<f64>,
    pub origin_clean: Cell<bool>,
    ctx2d: Cell<*mut c_void>,
    jsctx: Cell<ContextHandle>,
    owned_node: Cell<Option<southstar_dom::Node<'static>>>,
    context_kind: Cell<i32>,
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

pub(crate) fn element_attr(el: Node, name: &str) -> Option<Vec<u8>> {
    super::element_attr(el.addr(), name)
}

pub(crate) fn parse_int(text: &[u8], default: i32, min: i32, max: i32) -> i32 {
    let text = CString::new(text).unwrap_or_default();
    southstar_dom::controls::parse_int(Some(&text), default, min, max)
}

impl CanvasState {
    pub(crate) fn new(w: i32, h: i32) -> CanvasState {
        let surf = Surface::image(w, h);
        let cr = Context::on(&surf);
        CanvasState {
            w: Cell::new(w),
            h: Cell::new(h),
            surf: RefCell::new(surf),
            cr: RefCell::new(cr),
            fill: Cell::new([0.0, 0.0, 0.0, 1.0]),
            stroke: Cell::new([0.0, 0.0, 0.0, 1.0]),
            line_width: Cell::new(1.0),
            font: RefCell::new(DEFAULT_FONT.to_vec()),
            fill_pattern: RefCell::new(None),
            stroke_pattern: RefCell::new(None),
            shadow: Cell::new([0.0; 4]),
            shadow_blur: Cell::new(0.0),
            shadow_ox: Cell::new(0.0),
            shadow_oy: Cell::new(0.0),
            origin_clean: Cell::new(true),
            ctx2d: Cell::new(ptr::null_mut()),
            jsctx: Cell::new(ContextHandle::NULL),
            owned_node: Cell::new(None),
            context_kind: Cell::new(0),
        }
    }

    pub(crate) fn size(&self) -> (i32, i32) {
        (self.w.get(), self.h.get())
    }

    pub(crate) fn surface(&self) -> Surface {
        self.surf.borrow().reference()
    }

    pub(crate) fn context(&self) -> Context {
        self.cr.borrow().clone()
    }

    pub(crate) fn is_main(&self, cr: &Context) -> bool {
        self.cr.borrow().same(cr)
    }

    pub(crate) fn font(&self) -> Ref<'_, [u8]> {
        Ref::map(self.font.borrow(), Vec::as_slice)
    }

    pub(crate) fn set_font(&self, font: &[u8]) {
        let font = &font[..font.iter().position(|&b| b == 0).unwrap_or(font.len())];
        *self.font.borrow_mut() = font.to_vec();
    }

    pub(crate) fn clear_patterns(&self) {
        self.fill_pattern.take();
        self.stroke_pattern.take();
    }

    pub(crate) fn set_patterns(&self, fill: Option<Pattern>, stroke: Option<Pattern>) {
        *self.fill_pattern.borrow_mut() = fill;
        *self.stroke_pattern.borrow_mut() = stroke;
    }

    pub(crate) fn apply_fill_pattern(&self, cr: &Context) -> bool {
        self.fill_pattern
            .borrow()
            .as_ref()
            .map(|p| cr.set_source_pattern(p))
            .is_some()
    }

    pub(crate) fn apply_stroke_pattern(&self, cr: &Context) -> bool {
        self.stroke_pattern
            .borrow()
            .as_ref()
            .map(|p| cr.set_source_pattern(p))
            .is_some()
    }

    pub(crate) fn ctx2d(&self) -> *mut c_void {
        self.ctx2d.get()
    }

    pub(crate) fn context_kind(&self) -> i32 {
        self.context_kind.get()
    }

    pub(crate) fn set_context_kind(&self, kind: i32) {
        self.context_kind.set(kind);
    }

    pub(crate) fn set_owned_node(&self, node: southstar_dom::Node<'static>) {
        self.owned_node.set(Some(node));
    }

    pub(crate) fn attach_ctx2d(
        &self,
        scope: &mut southstar_js_engine::Scope<'_>,
        obj: &southstar_js_engine::Value,
        kind: i32,
    ) {
        let old = self.ctx2d.replace(ptr::null_mut());
        if !old.is_null() {
            // SAFETY: old is the identity this state retained for its previous 2D context and releases once, here.
            unsafe { scope.release_identity(old) };
        }
        self.ctx2d.set(scope.retain_identity(obj));
        self.jsctx
            .set(southstar_js_runtime::api::main_context(js_of(scope).ptr()));
        self.context_kind.set(kind);
    }

    pub(crate) fn reset(&self, w: i32, h: i32) {
        self.clear_patterns();
        let surf = Surface::image(w, h);
        *self.cr.borrow_mut() = Context::on(&surf);
        *self.surf.borrow_mut() = surf;
        self.w.set(w);
        self.h.set(h);
        self.fill.set([0.0, 0.0, 0.0, 1.0]);
        self.stroke.set([0.0, 0.0, 0.0, 1.0]);
        self.line_width.set(1.0);
        self.set_font(DEFAULT_FONT);
        self.shadow.set([0.0; 4]);
        self.shadow_blur.set(0.0);
        self.shadow_ox.set(0.0);
        self.shadow_oy.set(0.0);
        self.origin_clean.set(true);
        let ctx2d = self.ctx2d.get();
        if !ctx2d.is_null() {
            self.jsctx.get().with(|scope| {
                let obj = scope.object_from_identity(ctx2d);
                crate::api::ctx2d_init_state(scope, &obj);
            });
        }
    }
}

impl Drop for CanvasState {
    fn drop(&mut self) {
        let ctx2d = self.ctx2d.replace(ptr::null_mut());
        if !ctx2d.is_null() {
            self.jsctx.get().with(|scope| {
                // SAFETY: ctx2d is the identity this state retained for its 2D context and releases once, here.
                unsafe { scope.release_identity(ctx2d) }
            });
        }
        if let Some(node) = self.owned_node.take() {
            node.free_tree();
        }
    }
}

pub(crate) fn canvas_surface(js: Js, el: Node) -> Option<Surface> {
    if el.is_null() {
        return None;
    }
    let gl = crate::links::webgl_canvas_surface(el.addr());
    if !gl.is_null() {
        // SAFETY: the WebGL link returns null or the canvas's live readback surface, referenced before anything can replace it.
        return unsafe { Surface::from_borrowed(gl) };
    }
    #[cfg(feature = "webgpu")]
    {
        let gpu = crate::links::webgpu_canvas_surface(el.addr());
        if !gpu.is_null() {
            // SAFETY: the WebGPU link returns null or the canvas's live readback surface, referenced before anything can replace it.
            return unsafe { Surface::from_borrowed(gpu) };
        }
    }
    if js.is_null() {
        return None;
    }
    crate::state::lookup(js, el).map(|st| st.surface())
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
    let Some(src) = canvas_surface(js, el) else {
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
    if let Some(st) = &st {
        st.surface().clear();
    }
    let origin_clean = st.is_none_or(|st| st.origin_clean.get());
    Ok(crate::bitmap::make(scope, Some(copy), (w, h), origin_clean))
}

pub fn canvas_surface_for(
    js: *mut c_void,
    el: southstar_dom::Node<'_>,
) -> Option<southstar_cairo::Surface> {
    canvas_surface(Js(js.cast()), Node(el.as_ptr().cast())).map(Surface::into_shared)
}

fn teardown(js: *const NsJs) {
    crate::state::teardown(Js(js.cast_mut()));
}

pub fn install_runtime_hooks() {
    southstar_js_runtime::api::set_canvas_hooks(southstar_js_runtime::api::PageStateHooks {
        init: |_| {},
        teardown,
    });
    southstar_js_runtime::links::set_canvas_links(southstar_js_runtime::links::CanvasLinks {
        canvas_install: crate::ffi::exports::nsr_canvas_install,
    });
}
