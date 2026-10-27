//! Southstar — the canvas objects' host data, the realm each canvas object is created in, and the DOM, CSS and image calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use std::ffi::CString;

use southstar_js_engine::{ContextHandle, Scope, Value};

use super::cairo::Context;
use super::drawimage_source;
use crate::bitmap::ImageBitmap;
use crate::hidden::{self, Hidden};
use crate::path2d::Path2D;

#[repr(C)]
pub struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
pub struct NsNode {
    _private: [u8; 0],
}

pub(crate) fn with_hidden<R>(value: &Value, f: impl FnOnce(&Hidden) -> R) -> Option<R> {
    value.with_host::<Hidden, R>(f)
}

pub(crate) fn is_path2d(value: &Value) -> bool {
    path2d_context(value).is_some()
}

pub(crate) fn path2d_context(value: &Value) -> Option<Context> {
    value.with_host::<Path2D, Context>(|p| p.recording.context())
}

pub(crate) fn with_bitmap<R>(value: &Value, f: impl FnOnce(&ImageBitmap) -> R) -> Option<R> {
    value.with_host::<ImageBitmap, R>(f)
}

pub(crate) fn is_image_bitmap(value: &Value) -> bool {
    with_bitmap(value, |_| ()).is_some()
}

pub(crate) struct Decoded {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
}

pub(crate) fn decode_image(bytes: &[u8]) -> Option<Decoded> {
    let decoded = southstar_image::decode_bytes_to_pixels(bytes)?;
    Some(Decoded {
        pixels: decoded.pixels,
        width: decoded.width,
        height: decoded.height,
        stride: decoded.stride,
    })
}

fn js_of(scope: &Scope<'_>) -> *mut NsJs {
    scope.context_opaque().cast()
}

pub(crate) fn context_cairo(scope: &Scope<'_>, this: &Value) -> Option<super::cairo::Context> {
    let js = super::state::js_of(scope);
    if js.is_null() || !hidden::is_ctx2d(this) {
        return None;
    }
    let el = super::state::Node::from_addr(hidden::ptr(this));
    let st = crate::state::state_for(js, el)?;
    unsafe { super::cairo::Context::from_raw((*st).cr) }
}

pub(crate) fn in_realm<R>(
    scope: &mut Scope<'_>,
    realm: ContextHandle,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> R {
    if realm.is_null() || realm == scope.handle() {
        return f(scope);
    }
    let mut f = Some(f);
    let entered = realm.with(|realm| f.take().map(|f| f(realm)));
    match (entered.flatten(), f) {
        (Some(result), _) => result,
        (None, Some(f)) => f(scope),
        (None, None) => unreachable!(),
    }
}

fn this_realm(scope: &Scope<'_>, this: &Value) -> ContextHandle {
    canvas_realm(scope, hidden::ptr(this))
}

pub(crate) fn new_textmetrics(scope: &mut Scope<'_>, this: &Value, values: &[f64; 10]) -> Value {
    let realm = this_realm(scope, this);
    let obj = new_in_realm(scope, realm, hidden::KIND_TEXTMETRICS, "TextMetrics");
    crate::api::textmetrics_finish(scope, &obj, values);
    obj
}

pub(crate) fn new_gradient(scope: &mut Scope<'_>, this: &Value, kind: &[u8]) -> Value {
    let realm = this_realm(scope, this);
    let obj = new_in_realm(scope, realm, hidden::KIND_GRADIENT, "CanvasGradient");
    crate::api::gradient_finish(scope, &obj, kind);
    obj
}

pub(crate) fn new_pattern(
    scope: &mut Scope<'_>,
    this: &Value,
    source: &Value,
    repetition: &[u8],
) -> Value {
    let realm = this_realm(scope, this);
    let obj = new_in_realm(scope, realm, hidden::KIND_PATTERN, "CanvasPattern");
    crate::api::pattern_finish(scope, &obj, source, repetition);
    obj
}

pub(crate) fn new_imagedata(
    scope: &mut Scope<'_>,
    this: &Value,
    size: (i32, i32),
) -> Result<Value, Value> {
    let realm = this_realm(scope, this);
    crate::api::imagedata_new(scope, realm, size, None)
}

pub(crate) fn new_imagedata_from(
    scope: &mut Scope<'_>,
    this: &Value,
    size: (i32, i32),
    rgba: &[u8],
) -> Result<Value, Value> {
    let realm = this_realm(scope, this);
    crate::api::imagedata_new(scope, realm, size, Some(rgba))
}

pub(crate) fn drawimage_source_surface(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<(*mut c_void, i32, i32)>, Value> {
    let Some(source) = drawimage_source(scope, src) else {
        return Ok(None);
    };
    if !source.origin_clean {
        drop(source);
        return Err(crate::api::throw_dom(
            scope,
            "SecurityError",
            "The image source is not origin-clean.",
        ));
    }
    let (w, h) = source.size;
    Ok(Some((source.surface.into_raw(), w, h)))
}

pub(crate) fn canvas_state_for(scope: &Scope<'_>, el: usize) {
    let js = super::state::js_of(scope);
    let _ = crate::state::state_for(js, super::state::Node::from_addr(el));
}

pub(crate) fn computed_color(scope: &Scope<'_>, el: usize) -> Option<Vec<u8>> {
    let text = crate::links::computed_text(scope, dom_element(el)?, "color")?;
    Some(text.into_bytes())
}

fn dom_element(el: usize) -> Option<southstar_dom::Node<'static>> {
    // SAFETY: canvas bindings pass 0 or the address of a node of this thread's DOM store.
    unsafe { southstar_dom::Node::from_ptr(el as *const southstar_dom::NsNode) }
        .filter(|el| el.is_element())
}

pub(crate) fn element_attr(el: usize, name: &str) -> Option<Vec<u8>> {
    let name = CString::new(name).ok()?;
    dom_element(el)?
        .attr(&name)
        .map(|value| value.to_bytes().to_vec())
}

pub(crate) fn set_element_attr(el: usize, name: &str, value: &str) {
    let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
        return;
    };
    if let Some(el) = dom_element(el) {
        southstar_dom::attrs::set_attribute(el, &name, Some(value.as_bytes()));
    }
}

pub(crate) fn new_offscreen_canvas_node(scope: &Scope<'_>) -> usize {
    let el = southstar_dom::node::new_element(b"canvas")
        .as_mut_ptr()
        .cast::<NsNode>();
    let js = super::state::js_of(scope);
    if let Some(st) = crate::state::state_for(js, super::state::Node::from_addr(el as usize)) {
        unsafe { (*st).owned_node = el.cast() };
    }
    el as usize
}

pub(crate) fn canvas_realm(scope: &Scope<'_>, el: usize) -> ContextHandle {
    let current = scope.handle();
    if js_of(scope).is_null() {
        return current;
    }
    dom_element(el).map_or(current, |el| {
        crate::links::realm_for_node(scope, el).or(current)
    })
}

pub(crate) fn new_in_realm(
    scope: &mut Scope<'_>,
    realm: ContextHandle,
    kind: i32,
    iface: &str,
) -> Value {
    in_realm(scope, realm, |scope| {
        let proto = crate::api::api_proto(scope, iface);
        hidden::new(scope, kind, &proto)
    })
}

pub(crate) fn ctx2d_new(
    scope: &mut Scope<'_>,
    el: usize,
    canvas: &Value,
    offscreen: bool,
    attrs: Value,
) -> Value {
    let realm = canvas_realm(scope, el);
    let (kind, iface) = if offscreen {
        (
            hidden::KIND_OFFSCREEN_CTX2D,
            "OffscreenCanvasRenderingContext2D",
        )
    } else {
        (hidden::KIND_CTX2D, "CanvasRenderingContext2D")
    };
    let obj = new_in_realm(scope, realm, kind, iface);
    crate::api::ctx2d_finish(scope, &obj, el, canvas, attrs);
    obj
}

pub(crate) fn offscreen_node(obj: &Value) -> *const NsNode {
    if hidden::is(obj, hidden::KIND_OFFSCREEN) {
        hidden::ptr(obj) as *const NsNode
    } else {
        core::ptr::null()
    }
}
