//! Southstar — the C ABI of webgl.h and the engine, canvas, cairo and GL-context calls WebGL makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::{Rc, Weak};

use southstar_dom::{Node, NsNode};
use southstar_glib as glib;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::context::WebGl;

unsafe extern "C" {
    fn ns_gl_context_create() -> *mut c_void;
    fn ns_gl_context_make_current(c: *mut c_void) -> c_int;
    fn ns_gl_context_release(c: *mut c_void);
    fn ns_gl_context_destroy(c: *mut c_void);
    fn ns_js_current_url(js: *const c_void) -> *const c_char;
    fn ns_js_request_repaint(js: *mut c_void);
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_canvas_realm(ctx: *mut JSContext, el: *const NsNode) -> *mut JSContext;
    fn ns_api_proto(realm: *mut JSContext, iface: *const c_char) -> JSValue;
    fn ns_api_interface(
        ctx: *mut JSContext,
        global: JSValue,
        name: *const c_char,
        ctor: JSValue,
        parent: *const c_char,
    ) -> JSValue;
    fn ns_hidden_new(realm: *mut JSContext, kind: c_int, proto: JSValue) -> JSValue;
    fn ns_hidden_is(v: JSValue, kind: c_int) -> c_int;
    fn ns_hget(ctx: *mut JSContext, obj: JSValue, name: *const c_char) -> JSValue;
    fn ns_hset(ctx: *mut JSContext, obj: JSValue, name: *const c_char, val: JSValue);
    fn ns_js_drawimage_source_surface(
        ctx: *mut JSContext,
        src: JSValue,
        out_w: *mut c_int,
        out_h: *mut c_int,
        threw: *mut c_int,
    ) -> *mut c_void;
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
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

pub(crate) struct GlContext(NonNull<c_void>);

impl GlContext {
    pub(crate) fn create() -> Option<GlContext> {
        NonNull::new(unsafe { ns_gl_context_create() }).map(GlContext)
    }

    pub(crate) fn make_current(&self) -> bool {
        unsafe { ns_gl_context_make_current(self.0.as_ptr()) != 0 }
    }

    pub(crate) fn release(&self) {
        unsafe { ns_gl_context_release(self.0.as_ptr()) };
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe { ns_gl_context_destroy(self.0.as_ptr()) };
    }
}

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
    unsafe { ns_js_request_repaint(js as *mut c_void) };
}

pub(crate) fn page_url_and_origin(js: usize) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let url = unsafe { ns_js_current_url(js as *const c_void) };
    let origin = unsafe { ns_url_origin_from(url) };
    let origin_bytes = unsafe { glib::bytes(origin) }.map(<[u8]>::to_vec);
    unsafe { glib::g_free(origin.cast()) };
    let url_bytes = unsafe { glib::bytes(url) }.map(<[u8]>::to_vec);
    (url_bytes, origin_bytes)
}

fn c_text(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}

fn canvas_realm(scope: &Scope<'_>, canvas: usize) -> *mut JSContext {
    unsafe { ns_canvas_realm(quickjs::raw_context(scope), canvas as *const NsNode) }
}

pub(crate) fn api_proto(scope: &mut Scope<'_>, canvas: usize, iface: &str) -> Value {
    let realm = canvas_realm(scope, canvas);
    let name = c_text(iface);
    let raw = unsafe { ns_api_proto(realm, name.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn api_interface(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    ctor: Value,
    parent: Option<&str>,
) -> Value {
    let name = c_text(name);
    let parent = parent.map(c_text);
    let raw = unsafe {
        ns_api_interface(
            quickjs::raw_context(scope),
            quickjs::raw(global),
            name.as_ptr(),
            quickjs::into_raw(ctor),
            parent.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn new_info(scope: &mut Scope<'_>, canvas: usize, kind: i32, iface: &str) -> Value {
    let realm = canvas_realm(scope, canvas);
    let name = c_text(iface);
    let proto = unsafe { ns_api_proto(realm, name.as_ptr()) };
    let proto = unsafe { quickjs::take_value(scope, proto) };
    let raw = unsafe { ns_hidden_new(realm, kind, quickjs::raw(&proto)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn hidden_set(scope: &mut Scope<'_>, object: &Value, name: &str, value: Value) {
    let name = c_text(name);
    unsafe {
        ns_hset(
            quickjs::raw_context(scope),
            quickjs::raw(object),
            name.as_ptr(),
            quickjs::into_raw(value),
        )
    };
}

pub(crate) fn hidden_is(object: &Value, kind: i32) -> bool {
    unsafe { ns_hidden_is(quickjs::raw(object), kind) != 0 }
}

pub(crate) fn hidden_get(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
) -> Result<Value, Value> {
    let name = c_text(name);
    let raw = unsafe {
        ns_hget(
            quickjs::raw_context(scope),
            quickjs::raw(object),
            name.as_ptr(),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

fn pending_exception(scope: &Scope<'_>) -> Value {
    let raw = unsafe { JS_GetException(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
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
    let (mut w, mut h, mut threw) = (0, 0, 0);
    let raw = unsafe {
        ns_js_drawimage_source_surface(
            quickjs::raw_context(scope),
            quickjs::raw(src),
            &mut w,
            &mut h,
            &mut threw,
        )
    };
    if threw != 0 {
        return Err(pending_exception(scope));
    }
    Ok(NonNull::new(raw).map(|s| SourceImage {
        surface: Surface(s),
        w,
        h,
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
    object: JSValue,
}

thread_local! {
    static BY_NODE: RefCell<HashMap<usize, Registered>> = RefCell::new(HashMap::new());
}

fn registered(canvas: usize) -> Option<(Rc<WebGl>, JSValue)> {
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
    BY_NODE.with(|map| {
        if let Ok(mut map) = map.try_borrow_mut() {
            map.remove(&canvas);
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgl_get_context(
    ctx: *mut JSContext,
    js: *mut c_void,
    canvas_obj: JSValue,
    canvas: *const NsNode,
    version: c_int,
    attrs: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            if let Some((_, object)) = registered(canvas as usize) {
                return quickjs::into_raw(quickjs::borrow_value(scope, object));
            }
            let canvas_obj = quickjs::borrow_value(scope, canvas_obj);
            let attrs = quickjs::borrow_value(scope, attrs);
            let Some((gl, proto)) =
                crate::install::new_context(scope, js as usize, canvas as usize, version, &attrs)
            else {
                return quickjs::into_raw(Value::null());
            };
            let weak = Rc::downgrade(&gl);
            let host = ContextHost { gl, canvas_obj };
            let obj = scope.new_traced_host_object(proto.is_object().then_some(&proto), host);
            let object = quickjs::raw(&obj);
            BY_NODE.with(|map| {
                map.borrow_mut()
                    .insert(canvas as usize, Registered { gl: weak, object })
            });
            quickjs::into_raw(obj)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgl_install(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::install::install(scope, &global);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgl_canvas_surface(canvas: *const NsNode) -> *mut c_void {
    let Some((gl, _)) = registered(canvas as usize) else {
        return ptr::null_mut();
    };
    gl.canvas_surface()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_webgl_take_pending_origin() -> *mut c_char {
    crate::permission::take_pending_origin().map_or(ptr::null_mut(), |origin| glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgl_set_decision(origin: *const c_char, allow: c_int) {
    if origin.is_null() {
        return;
    }
    let origin = unsafe { CStr::from_ptr(origin) }.to_bytes();
    crate::permission::set_decision(origin, allow != 0);
}
