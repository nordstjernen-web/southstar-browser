//! Southstar — the C ABI of the canvas objects' WebIDL surface, as declared in src/js_internal.h, and the canvas C it dispatches to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_double, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::cairo::Surface;
use super::cairo::{Cairo, Context};
use super::text;
use crate::bitmap::{ImageBitmap, Source};
use crate::hidden::{self, Hidden};
use crate::path2d::Path2D;

macro_rules! jscfunctions {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(
                pub(crate) fn $name(
                    ctx: *mut JSContext,
                    this_val: JSValue,
                    argc: c_int,
                    argv: *mut JSValue,
                ) -> JSValue;
            )*
        }
    };
}

#[allow(non_snake_case)]
pub(crate) mod c {
    use super::{JSContext, JSValue, c_int};

    jscfunctions!(ns_ctx_fillText, ns_ctx_measureText, ns_ctx_strokeText,);
}

#[repr(C)]
pub struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
pub struct NsNode {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn ns_ctx_drawimage_source(
        ctx: *mut JSContext,
        src: JSValue,
        out_w: *mut c_int,
        out_h: *mut c_int,
        origin_clean: *mut c_int,
    ) -> *mut c_void;
    fn ns_image_decode_bytes_to_pixels(
        data: *const u8,
        len: usize,
        out_w: *mut c_int,
        out_h: *mut c_int,
        out_stride: *mut usize,
        out_buf_len: *mut usize,
        out_format: *mut c_int,
    ) -> *mut u8;
    fn ns_js_realm_for_node(js: *mut NsJs, node: *const NsNode) -> *mut JSContext;
    fn ns_js_computed_text(
        ctx: *mut JSContext,
        node: *const NsNode,
        name: *const c_char,
    ) -> *mut c_char;
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
}

pub(crate) fn with_hidden<R>(value: &Value, f: impl FnOnce(&Hidden) -> R) -> Option<R> {
    unsafe { quickjs::with_host::<Hidden, R>(quickjs::raw(value), f) }
}

pub(crate) fn is_path2d(value: &Value) -> bool {
    path2d_context(value).is_some()
}

pub(crate) fn path2d_context(value: &Value) -> Option<Context> {
    unsafe { quickjs::with_host::<Path2D, Context>(quickjs::raw(value), |p| p.recording.context()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_value_is_path2d(v: JSValue) -> c_int {
    let found = unsafe { quickjs::with_host::<Path2D, ()>(v, |_| ()) };
    c_int::from(found.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_replay_path2d(target: *mut Cairo, path: JSValue) {
    let Some(target) = (unsafe { Context::from_raw(target) }) else {
        return;
    };
    let src = unsafe { quickjs::with_host::<Path2D, Context>(path, |p| p.recording.context()) };
    if let Some(src) = src {
        let copy = src.copy_path();
        target.new_path();
        target.append_path(&copy);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_round_rect_subpath(
    cr: *mut Cairo,
    x: c_double,
    y: c_double,
    w: c_double,
    h: c_double,
    rtl: c_double,
    rtr: c_double,
    rbr: c_double,
    rbl: c_double,
) {
    if let Some(cr) = unsafe { Context::from_raw(cr) } {
        crate::path2d::round_rect_subpath(cr, [x, y, w, h], [rtl, rtr, rbr, rbl]);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_extract_radii(
    ctx: *mut JSContext,
    v: JSValue,
    rtl: *mut c_double,
    rtr: *mut c_double,
    rbr: *mut c_double,
    rbl: *mut c_double,
) -> c_int {
    let (radii, valid) = unsafe {
        quickjs::with_context(ctx, |scope| {
            let v = quickjs::borrow_value(scope, v);
            crate::path2d::extract_radii(scope, &v)
        })
    };
    unsafe {
        *rtl = radii[0];
        *rtr = radii[1];
        *rbr = radii[2];
        *rbl = radii[3];
    }
    c_int::from(valid)
}

pub(crate) fn with_bitmap<R>(value: &Value, f: impl FnOnce(&ImageBitmap) -> R) -> Option<R> {
    unsafe { quickjs::with_host::<ImageBitmap, R>(quickjs::raw(value), f) }
}

pub(crate) struct Decoded {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
}

pub(crate) fn decode_image(bytes: &[u8]) -> Option<Decoded> {
    let (mut w, mut h, mut stride, mut buf_len, mut format) = (0, 0, 0usize, 0usize, 0);
    let pixels = unsafe {
        ns_image_decode_bytes_to_pixels(
            bytes.as_ptr(),
            bytes.len(),
            &mut w,
            &mut h,
            &mut stride,
            &mut buf_len,
            &mut format,
        )
    };
    if pixels.is_null() {
        return None;
    }
    let copy = unsafe { core::slice::from_raw_parts(pixels, buf_len) }.to_vec();
    unsafe { glib::g_free(pixels.cast()) };
    Some(Decoded {
        pixels: copy,
        width: w,
        height: h,
        stride,
    })
}

pub(crate) fn drawimage_source(scope: &mut Scope<'_>, src: &Value) -> Option<Source> {
    let (mut w, mut h, mut clean) = (0, 0, 1);
    let ctx = quickjs::raw_context(scope);
    let surface =
        unsafe { ns_ctx_drawimage_source(ctx, quickjs::raw(src), &mut w, &mut h, &mut clean) };
    let surface = unsafe { Surface::from_raw(surface) }?;
    Some(Source {
        surface,
        size: (w, h),
        origin_clean: clean != 0,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_bitmap_is(v: JSValue) -> c_int {
    let found = unsafe { quickjs::with_host::<ImageBitmap, ()>(v, |_| ()) };
    c_int::from(found.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_bitmap_make(
    ctx: *mut JSContext,
    surface: *mut c_void,
    w: c_int,
    h: c_int,
    origin_clean: c_int,
) -> JSValue {
    let surface = unsafe { Surface::from_raw(surface) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let bitmap = crate::bitmap::make(scope, surface, (w, h), origin_clean != 0);
            quickjs::into_raw(bitmap)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_bitmap_surface(
    v: JSValue,
    out_w: *mut c_int,
    out_h: *mut c_int,
    origin_clean: *mut c_int,
) -> *mut c_void {
    let Some(source) = (unsafe { bitmap_source(v) }) else {
        return ptr::null_mut();
    };
    unsafe {
        *out_w = source.size.0;
        *out_h = source.size.1;
        *origin_clean = c_int::from(source.origin_clean);
    }
    source.surface.into_raw()
}

unsafe fn bitmap_source(v: JSValue) -> Option<Source> {
    unsafe { quickjs::with_host::<ImageBitmap, Option<Source>>(v, ImageBitmap::source) }.flatten()
}

fn js_of(scope: &Scope<'_>) -> *mut NsJs {
    quickjs::context_opaque(scope).cast()
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

fn this_realm(scope: &Scope<'_>, this: &Value) -> *mut JSContext {
    canvas_realm(quickjs::raw_context(scope), hidden::ptr(this))
}

pub(crate) fn new_gradient(scope: &mut Scope<'_>, this: &Value, kind: &[u8]) -> Value {
    let realm = this_realm(scope, this);
    let obj = unsafe { new_in_realm(realm, hidden::KIND_GRADIENT, "CanvasGradient") };
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
    let obj = unsafe { new_in_realm(realm, hidden::KIND_PATTERN, "CanvasPattern") };
    crate::api::pattern_finish(scope, &obj, source, repetition);
    obj
}

pub(crate) fn new_imagedata(
    scope: &mut Scope<'_>,
    this: &Value,
    size: (i32, i32),
) -> Result<Value, Value> {
    let realm = this_realm(scope, this);
    unsafe {
        quickjs::with_context(realm, |realm| {
            crate::api::imagedata_new(scope, realm, size, None)
        })
    }
}

pub(crate) fn new_imagedata_from(
    scope: &mut Scope<'_>,
    this: &Value,
    size: (i32, i32),
    rgba: &[u8],
) -> Result<Value, Value> {
    let realm = this_realm(scope, this);
    unsafe {
        quickjs::with_context(realm, |realm| {
            crate::api::imagedata_new(scope, realm, size, Some(rgba))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drawimage_source_surface(
    ctx: *mut JSContext,
    src: JSValue,
    out_w: *mut c_int,
    out_h: *mut c_int,
    threw: *mut c_int,
) -> *mut c_void {
    unsafe {
        *out_w = 0;
        *out_h = 0;
        *threw = 0;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let src = quickjs::borrow_value(scope, src);
            let Some(source) = drawimage_source(scope, &src) else {
                return ptr::null_mut();
            };
            if !source.origin_clean {
                drop(source);
                *threw = 1;
                let error = crate::api::throw_dom(
                    scope,
                    "SecurityError",
                    "The image source is not origin-clean.",
                );
                let _ = quickjs::result_raw(scope, Err(error));
                return ptr::null_mut();
            }
            *out_w = source.size.0;
            *out_h = source.size.1;
            source.surface.into_raw()
        })
    }
}

pub(crate) fn canvas_state_for(scope: &Scope<'_>, el: usize) {
    let js = super::state::js_of(scope);
    let _ = crate::state::state_for(js, super::state::Node::from_addr(el));
}

pub(crate) fn computed_color(scope: &Scope<'_>, el: usize) -> Option<Vec<u8>> {
    if el == 0 {
        return None;
    }
    let ctx = quickjs::raw_context(scope);
    let computed = unsafe { ns_js_computed_text(ctx, el as *const NsNode, c"color".as_ptr()) };
    let text = unsafe { glib::GStr::take(computed) }?;
    Some(text.to_bytes().to_vec())
}

pub(crate) fn element_attr(el: usize, name: &str) -> Option<Vec<u8>> {
    let name = CString::new(name).ok()?;
    unsafe { text(ns_element_get_attr(el as *const NsNode, name.as_ptr())) }.map(<[u8]>::to_vec)
}

pub(crate) fn set_element_attr(el: usize, name: &str, value: &str) {
    let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
        return;
    };
    unsafe { ns_element_set_attr(el as *mut NsNode, name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn new_offscreen_canvas_node(scope: &Scope<'_>) -> usize {
    let el = unsafe { ns_node_new_element(glib::strdup(b"canvas")) };
    let js = super::state::js_of(scope);
    if let Some(st) = crate::state::state_for(js, super::state::Node::from_addr(el as usize)) {
        unsafe { (*st).owned_node = el.cast() };
    }
    el as usize
}

fn canvas_realm(ctx: *mut JSContext, el: usize) -> *mut JSContext {
    let js = unsafe { quickjs::with_context(ctx, |scope| js_of(scope)) };
    if js.is_null() || el == 0 {
        return ctx;
    }
    let realm = unsafe { ns_js_realm_for_node(js, el as *const NsNode) };
    if realm.is_null() { ctx } else { realm }
}

unsafe fn name<'a>(s: *const c_char) -> &'a str {
    unsafe { text(s) }
        .and_then(|b| core::str::from_utf8(b).ok())
        .unwrap_or_default()
}

pub(crate) unsafe extern "C" fn ns_pattern_set_transform(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, crate::api::pattern_set_transform) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_register_classes(rt: *mut c_void) {
    let _ = rt;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hidden_new(
    realm: *mut JSContext,
    kind: c_int,
    proto: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(realm, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            quickjs::into_raw(hidden::new(scope, kind, &proto))
        })
    }
}

fn raw_kind(v: JSValue) -> Option<i32> {
    unsafe { quickjs::with_host::<Hidden, i32>(v, |h| h.kind) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hidden_is(v: JSValue, kind: c_int) -> c_int {
    c_int::from(raw_kind(v) == Some(kind))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx2d_is(v: JSValue) -> c_int {
    let kind = raw_kind(v);
    c_int::from(matches!(
        kind,
        Some(hidden::KIND_CTX2D | hidden::KIND_OFFSCREEN_CTX2D)
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hidden_ptr(v: JSValue) -> *mut c_void {
    let ptr = unsafe { quickjs::with_host::<Hidden, usize>(v, |h| h.ptr.get()) };
    ptr.unwrap_or(0) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hidden_set_ptr(v: JSValue, ptr: *mut c_void) {
    unsafe { quickjs::with_host::<Hidden, ()>(v, |h| h.ptr.set(ptr as usize)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hget(ctx: *mut JSContext, obj: JSValue, key: *const c_char) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            let result = hidden::get(scope, &obj, name(key));
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hset(
    ctx: *mut JSContext,
    obj: JSValue,
    key: *const c_char,
    val: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let val = quickjs::take_value(scope, val);
            let obj = quickjs::borrow_value(scope, obj);
            hidden::set(scope, &obj, name(key), val);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_realm(ctx: *mut JSContext, el: *const NsNode) -> *mut JSContext {
    canvas_realm(ctx, el as usize)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_realm(ctx: *mut JSContext, this_val: JSValue) -> *mut JSContext {
    let el = unsafe { ns_hidden_ptr(this_val) };
    canvas_realm(ctx, el as usize)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_api_proto(realm: *mut JSContext, iface: *const c_char) -> JSValue {
    unsafe {
        quickjs::with_context(realm, |scope| {
            quickjs::into_raw(crate::api::api_proto(scope, name(iface)))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_api_proto_of_ctor(
    ctx: *mut JSContext,
    new_target: JSValue,
    iface: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let new_target = quickjs::borrow_value(scope, new_target);
            let proto = crate::api::api_proto_of_ctor(scope, &new_target, name(iface));
            quickjs::into_raw(proto)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_api_throw_new_required(
    ctx: *mut JSContext,
    iface: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let error = crate::api::new_required(scope, name(iface));
            quickjs::result_raw(scope, Err(error))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_throw_dom(
    ctx: *mut JSContext,
    dom_name: *const c_char,
    msg: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let dom_name = String::from_utf8_lossy(text(dom_name).unwrap_or_default()).into_owned();
            let msg = String::from_utf8_lossy(text(msg).unwrap_or_default()).into_owned();
            let error = crate::api::throw_dom(scope, &dom_name, &msg);
            quickjs::result_raw(scope, Err(error))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_api_interface(
    ctx: *mut JSContext,
    global: JSValue,
    iface: *const c_char,
    ctor: JSValue,
    parent: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let ctor = quickjs::take_value(scope, ctor);
            let parent = (!parent.is_null()).then(|| name(parent));
            let proto = crate::api::interface(scope, &global, name(iface), ctor, parent);
            quickjs::into_raw(proto)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_install(ctx: *mut JSContext, global: JSValue, window: c_int) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::api::install(scope, &global, window != 0);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx2d_init_state(ctx: *mut JSContext, obj: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            crate::api::ctx2d_init_state(scope, &obj);
        });
    }
}

unsafe fn new_in_realm(realm: *mut JSContext, kind: i32, iface: &str) -> Value {
    unsafe {
        quickjs::with_context(realm, |scope| {
            let proto = crate::api::api_proto(scope, iface);
            hidden::new(scope, kind, &proto)
        })
    }
}

pub(crate) fn ctx2d_new(
    scope: &mut Scope<'_>,
    el: usize,
    canvas: &Value,
    offscreen: bool,
    attrs: Value,
) -> Value {
    let realm = canvas_realm(quickjs::raw_context(scope), el);
    let (kind, iface) = if offscreen {
        (
            hidden::KIND_OFFSCREEN_CTX2D,
            "OffscreenCanvasRenderingContext2D",
        )
    } else {
        (hidden::KIND_CTX2D, "CanvasRenderingContext2D")
    };
    let obj = unsafe { new_in_realm(realm, kind, iface) };
    crate::api::ctx2d_finish(scope, &obj, el, canvas, attrs);
    obj
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx2d_new(
    ctx: *mut JSContext,
    el: *const NsNode,
    canvas_obj: JSValue,
    offscreen: c_int,
    attrs: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let attrs = quickjs::take_value(scope, attrs);
            let canvas = quickjs::borrow_value(scope, canvas_obj);
            quickjs::into_raw(ctx2d_new(
                scope,
                el as usize,
                &canvas,
                offscreen != 0,
                attrs,
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_gradient_new(
    ctx: *mut JSContext,
    realm: *mut JSContext,
    kind: *const c_char,
) -> JSValue {
    let obj = unsafe { new_in_realm(realm, hidden::KIND_GRADIENT, "CanvasGradient") };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::api::gradient_finish(scope, &obj, text(kind).unwrap_or_default());
            quickjs::into_raw(obj)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_pattern_new(
    ctx: *mut JSContext,
    realm: *mut JSContext,
    source: JSValue,
    repetition: *const c_char,
) -> JSValue {
    let obj = unsafe { new_in_realm(realm, hidden::KIND_PATTERN, "CanvasPattern") };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let source = quickjs::borrow_value(scope, source);
            let repetition = text(repetition).unwrap_or_default();
            crate::api::pattern_finish(scope, &obj, &source, repetition);
            quickjs::into_raw(obj)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_textmetrics_new(
    ctx: *mut JSContext,
    realm: *mut JSContext,
    v: *const c_double,
) -> JSValue {
    let obj = unsafe { new_in_realm(realm, hidden::KIND_TEXTMETRICS, "TextMetrics") };
    let values: [f64; 10] = unsafe { *v.cast::<[f64; 10]>() };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::api::textmetrics_finish(scope, &obj, &values);
            quickjs::into_raw(obj)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_imagedata_wrap(
    ctx: *mut JSContext,
    realm: *mut JSContext,
    proto: JSValue,
    w: c_int,
    h: c_int,
    data: JSValue,
    color_space: *const c_char,
) -> JSValue {
    let obj = unsafe {
        quickjs::with_context(realm, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            let proto = if proto.is_object() {
                proto
            } else {
                crate::api::api_proto(scope, "ImageData")
            };
            hidden::new(scope, hidden::KIND_IMAGEDATA, &proto)
        })
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let data = quickjs::take_value(scope, data);
            let space = text(color_space).unwrap_or_default();
            crate::api::imagedata_finish(scope, &obj, (w, h), data, space);
            quickjs::into_raw(obj)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_imagedata_new(
    ctx: *mut JSContext,
    realm: *mut JSContext,
    w: c_int,
    h: c_int,
    rgba: *const u8,
) -> JSValue {
    let rgba = (!rgba.is_null() && w > 0 && h > 0)
        .then(|| unsafe { core::slice::from_raw_parts(rgba, w as usize * h as usize * 4) });
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let result = quickjs::with_context(realm, |realm| {
                crate::api::imagedata_new(scope, realm, (w, h), rgba)
            });
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_imagedata_construct(
    ctx: *mut JSContext,
    new_target: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, new_target, argc, argv, crate::api::imagedata_construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_offscreen_construct(
    ctx: *mut JSContext,
    new_target: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, new_target, argc, argv, crate::api::offscreen_construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_offscreen_node(obj: JSValue) -> *const NsNode {
    if raw_kind(obj) == Some(hidden::KIND_OFFSCREEN) {
        unsafe { ns_hidden_ptr(obj) }.cast_const().cast()
    } else {
        ptr::null()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_offscreen_sync_size(ctx: *mut JSContext, obj: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            crate::api::offscreen_sync_size(scope, &obj);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_clone_object(ctx: *mut JSContext, v: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let v = quickjs::borrow_value(scope, v);
            let result = crate::api::clone_object(scope, &v);
            quickjs::result_raw(scope, result)
        })
    }
}
