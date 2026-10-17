//! Southstar — WebGL's entry points and the engine, canvas, cairo and GL-context calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr::{self, NonNull};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_canvas::host as canvas;
use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api::{self, NsJs};

use crate::context::WebGl;

unsafe extern "C" {
    fn cairo_image_surface_create(format: c_int, w: c_int, h: c_int) -> *mut c_void;
    fn cairo_surface_status(s: *mut c_void) -> c_int;
    fn cairo_surface_destroy(s: *mut c_void);
    fn cairo_surface_flush(s: *mut c_void);
    fn cairo_surface_mark_dirty(s: *mut c_void);
    fn cairo_image_surface_get_data(s: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(s: *mut c_void) -> c_int;
    fn cairo_image_surface_get_height(s: *mut c_void) -> c_int;
}

const CAIRO_FORMAT_ARGB32: c_int = 0;
const CAIRO_STATUS_SUCCESS: c_int = 0;

pub(crate) use southstar_glctx::GlContext;

pub(crate) struct Surface(NonNull<c_void>);

impl Surface {
    pub(crate) fn new(w: i32, h: i32) -> Option<Surface> {
        let raw = unsafe { cairo_image_surface_create(CAIRO_FORMAT_ARGB32, w, h) };
        let surface = Surface(NonNull::new(raw)?);
        (unsafe { cairo_surface_status(raw) } == CAIRO_STATUS_SUCCESS).then_some(surface)
    }

    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub(crate) fn flush(&self) {
        unsafe { cairo_surface_flush(self.as_ptr()) };
    }

    pub(crate) fn mark_dirty(&self) {
        unsafe { cairo_surface_mark_dirty(self.as_ptr()) };
    }

    pub(crate) fn pixels(&mut self) -> Option<(&mut [u8], usize)> {
        let raw = self.as_ptr();
        let data = unsafe { cairo_image_surface_get_data(raw) };
        let stride = unsafe { cairo_image_surface_get_stride(raw) };
        let height = unsafe { cairo_image_surface_get_height(raw) };
        if data.is_null() || stride <= 0 || height <= 0 {
            return None;
        }
        let len = stride as usize * height as usize;
        Some((
            unsafe { core::slice::from_raw_parts_mut(data, len) },
            stride as usize,
        ))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.as_ptr()) };
    }
}

pub(crate) fn canvas_node(canvas: usize) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(canvas as *const NsNode) }
}

pub(crate) fn request_repaint(js: usize) {
    api::request_repaint(js as *const NsJs);
}

pub(crate) fn page_url_and_origin(js: usize) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let url = api::current_url(js as *const NsJs).into_bytes();
    let origin = southstar_url::bytes::origin_from(&url);
    (Some(url), origin)
}

pub(crate) fn api_proto(scope: &mut Scope<'_>, canvas: usize, iface: &str) -> Value {
    canvas::api_proto(scope, canvas, iface)
}

pub(crate) fn api_interface(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    ctor: Value,
    parent: Option<&str>,
) -> Value {
    canvas::api_interface(scope, global, name, ctor, parent)
}

pub(crate) fn new_info(scope: &mut Scope<'_>, canvas: usize, kind: i32, iface: &str) -> Value {
    canvas::new_hidden(scope, canvas, kind, iface)
}

pub(crate) fn hidden_set(scope: &mut Scope<'_>, object: &Value, name: &str, value: Value) {
    canvas::hidden_set(scope, object, name, value);
}

pub(crate) fn hidden_is(object: &Value, kind: i32) -> bool {
    canvas::hidden_is(object, kind)
}

pub(crate) fn hidden_get(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
) -> Result<Value, Value> {
    canvas::hidden_get(scope, object, name)
}

pub(crate) struct SourceImage {
    surface: Surface,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

impl SourceImage {
    pub(crate) fn pixels(&mut self) -> Option<(&[u8], usize)> {
        self.surface.pixels().map(|(data, stride)| (&*data, stride))
    }
}

pub(crate) fn drawimage_source(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<SourceImage>, Value> {
    let source = canvas::drawimage_source_surface(scope, src)?;
    Ok(source.and_then(|(raw, w, h)| {
        NonNull::new(raw).map(|s| SourceImage {
            surface: Surface(s),
            w,
            h,
        })
    }))
}

pub(crate) struct GlObject {
    pub(crate) kind: u8,
    pub(crate) name: u32,
}

pub(crate) fn object_of(value: &Value) -> Option<(u8, u32)> {
    value.with_host(|o: &GlObject| (o.kind, o.name))
}

pub(crate) struct ContextHost {
    pub(crate) gl: Rc<WebGl>,
    pub(crate) canvas_obj: Value,
}

impl southstar_js_engine::Trace for ContextHost {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.canvas_obj);
    }
}

pub(crate) fn context_of(value: &Value) -> Option<Rc<WebGl>> {
    value.with_host(|h: &ContextHost| h.gl.clone())
}

pub(crate) fn canvas_object_of(value: &Value) -> Option<Value> {
    value.with_host(|h: &ContextHost| h.canvas_obj.clone())
}

struct Registered {
    gl: Weak<WebGl>,
    object: *mut c_void,
}

thread_local! {
    static BY_NODE: RefCell<HashMap<usize, Registered>> = RefCell::new(HashMap::new());
}

fn registered(canvas: usize) -> Option<(Rc<WebGl>, *mut c_void)> {
    BY_NODE.with(|map| {
        let map = map.borrow();
        let entry = map.get(&canvas)?;
        Some((entry.gl.upgrade()?, entry.object))
    })
}

pub(crate) fn context_count() -> usize {
    BY_NODE.with(|map| map.borrow().len())
}

pub(crate) fn forget_context(canvas: usize) {
    let _ = BY_NODE.try_with(|map| {
        if let Ok(mut map) = map.try_borrow_mut() {
            map.remove(&canvas);
        }
    });
}

#[unsafe(no_mangle)]
pub fn nsr_webgl_get_context(
    scope: &mut Scope<'_>,
    js: usize,
    canvas_obj: &Value,
    canvas: usize,
    version: i32,
    attrs: &Value,
) -> Result<Value, Value> {
    if let Some((_, object)) = registered(canvas) {
        return Ok(scope.object_from_identity(object));
    }
    let Some((gl, proto)) = crate::install::new_context(scope, js, canvas, version, attrs) else {
        return Ok(Value::null());
    };
    let weak = Rc::downgrade(&gl);
    let host = ContextHost {
        gl,
        canvas_obj: canvas_obj.clone(),
    };
    let obj = scope.new_traced_host_object(proto.is_object().then_some(&proto), host);
    let object = obj.object_identity();
    BY_NODE.with(|map| {
        map.borrow_mut()
            .insert(canvas, Registered { gl: weak, object })
    });
    Ok(obj)
}

#[unsafe(no_mangle)]
pub fn nsr_webgl_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install(scope, global);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgl_canvas_surface(canvas: *const NsNode) -> *mut c_void {
    let Some((gl, _)) = registered(canvas as usize) else {
        return ptr::null_mut();
    };
    gl.canvas_surface()
}
