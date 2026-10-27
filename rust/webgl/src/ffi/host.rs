//! Southstar — WebGL's entry points and the engine, canvas, cairo and GL-context calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_canvas::host as canvas;
use southstar_dom::{Node, WeakNode};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api::{self, Page};

use crate::context::WebGl;

pub(crate) use southstar_glctx::GlContext;

pub(crate) struct Surface(southstar_cairo::Surface);

impl Surface {
    pub(crate) fn new(w: i32, h: i32) -> Option<Surface> {
        southstar_cairo::Surface::image(southstar_cairo::Format::Argb32, w, h).map(Surface)
    }

    pub(crate) fn shared(&self) -> southstar_cairo::Surface {
        self.0.clone()
    }

    pub(crate) fn with_pixels_mut<R>(&self, f: impl FnOnce(&mut [u8], usize) -> R) -> Option<R> {
        self.0.as_ref().with_data_mut(f)
    }
}

pub(crate) fn canvas_node(canvas: Option<WeakNode>) -> Option<Node<'static>> {
    canvas.and_then(WeakNode::get)
}

pub(crate) fn request_repaint(js: Option<Page>) {
    api::request_repaint(js);
}

pub(crate) fn page_url_and_origin(js: Option<Page>) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let url = api::current_url(js).into_bytes();
    let origin = southstar_url::bytes::origin_from(&url);
    (Some(url), origin)
}

pub(crate) fn api_proto(scope: &mut Scope<'_>, canvas: Option<WeakNode>, iface: &str) -> Value {
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

pub(crate) fn new_info(
    scope: &mut Scope<'_>,
    canvas: Option<WeakNode>,
    kind: i32,
    iface: &str,
) -> Value {
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
    pub(crate) fn with_pixels<R>(&self, f: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        self.surface.0.as_ref().with_data(f)
    }
}

pub(crate) fn drawimage_source(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<SourceImage>, Value> {
    let source = canvas::drawimage_source_surface(scope, src)?;
    Ok(source.map(|(surface, w, h)| SourceImage {
        surface: Surface(surface),
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
    object: *mut c_void,
}

thread_local! {
    static BY_NODE: RefCell<HashMap<Option<WeakNode>, Registered>> = RefCell::new(HashMap::new());
}

fn registered(canvas: Option<WeakNode>) -> Option<(Rc<WebGl>, *mut c_void)> {
    BY_NODE.with(|map| {
        let map = map.borrow();
        let entry = map.get(&canvas)?;
        Some((entry.gl.upgrade()?, entry.object))
    })
}

pub(crate) fn context_count() -> usize {
    BY_NODE.with(|map| map.borrow().len())
}

pub(crate) fn forget_context(canvas: Option<WeakNode>) {
    let _ = BY_NODE.try_with(|map| {
        if let Ok(mut map) = map.try_borrow_mut() {
            map.remove(&canvas);
        }
    });
}

pub fn nsr_webgl_get_context(
    scope: &mut Scope<'_>,
    js: Option<Page>,
    canvas_obj: &Value,
    canvas: Option<WeakNode>,
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

pub fn nsr_webgl_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install(scope, global);
}

pub(crate) fn canvas_surface(canvas: Option<WeakNode>) -> Option<southstar_cairo::Surface> {
    registered(canvas).and_then(|(gl, _)| gl.canvas_surface())
}
