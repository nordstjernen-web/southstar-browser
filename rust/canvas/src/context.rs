//! Southstar — getContext() on canvas and OffscreenCanvas, the 2D context attributes, PNG export and convertToBlob(), and DOMMatrix results.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::Surface;
use crate::ffi::context as host;
use crate::ffi::state::{CanvasState, Node};

const KIND_2D: i32 = 1;
const KIND_WEBGL: i32 = 2;
const KIND_WEBGPU: i32 = 3;

#[derive(Clone, Copy, PartialEq)]
enum ContextType {
    Unknown,
    TwoD,
    WebGl,
    WebGl2,
    WebGpu,
    Bitmap,
}

fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

fn context_type(scope: &mut Scope<'_>, v: &Value) -> Result<ContextType, Value> {
    let t = c_text(scope.to_bytes(v)?);
    Ok(match t.as_slice() {
        b"2d" => ContextType::TwoD,
        b"webgl" | b"experimental-webgl" => ContextType::WebGl,
        b"webgl2" if !cfg!(target_os = "macos") => ContextType::WebGl2,
        b"webgpu" => ContextType::WebGpu,
        b"bitmaprenderer" => ContextType::Bitmap,
        _ => ContextType::Unknown,
    })
}

fn attrs_2d(scope: &mut Scope<'_>, options: &Value) -> (Value, bool) {
    let attrs = scope.new_object();
    let srgb = scope.string("srgb");
    let unorm8 = scope.string("unorm8");
    let _ = scope.set(&attrs, "alpha", Value::boolean(true));
    let _ = scope.set(&attrs, "colorSpace", srgb);
    let _ = scope.set(&attrs, "colorType", unorm8);
    let _ = scope.set(&attrs, "desynchronized", Value::boolean(false));
    let _ = scope.set(&attrs, "willReadFrequently", Value::boolean(false));
    let mut opaque = false;
    if !options.is_object() {
        return (attrs, opaque);
    }
    let undefined = || Value::undefined();
    let alpha = scope.get(options, "alpha").unwrap_or_else(|_| undefined());
    if !alpha.is_undefined() {
        let alpha = scope.to_bool(&alpha);
        opaque = !alpha;
        let _ = scope.set(&attrs, "alpha", Value::boolean(alpha));
    }
    let frequent = scope
        .get(options, "willReadFrequently")
        .unwrap_or_else(|_| undefined());
    if scope.to_bool(&frequent) {
        let _ = scope.set(&attrs, "willReadFrequently", Value::boolean(true));
    }
    let space = scope
        .get(options, "colorSpace")
        .unwrap_or_else(|_| undefined());
    if space.is_string() {
        let _ = scope.set(&attrs, "colorSpace", space);
    }
    (attrs, opaque)
}

fn get_2d(
    scope: &mut Scope<'_>,
    st: *mut CanvasState,
    el: Node,
    canvas: &Value,
    offscreen: bool,
    options: &Value,
) -> Value {
    if let Some(existing) = host::ctx2d_of(scope, st) {
        return existing;
    }
    if host::context_kind(st) != 0 {
        return Value::null();
    }
    let (attrs, opaque) = attrs_2d(scope, options);
    if opaque {
        if let Some(surface) = host::surface_of(st) {
            surface.fill_opaque_black();
        }
    }
    let obj = crate::ffi::ctx2d_new(scope, el.addr(), canvas, offscreen, attrs);
    host::attach_ctx2d(scope, st, &obj, KIND_2D);
    obj
}

fn get_by_type(
    scope: &mut Scope<'_>,
    st: *mut CanvasState,
    el: Node,
    canvas: &Value,
    offscreen: bool,
    kind: ContextType,
    options: &Value,
) -> Result<Value, Value> {
    match kind {
        ContextType::TwoD => Ok(get_2d(scope, st, el, canvas, offscreen, options)),
        ContextType::WebGl | ContextType::WebGl2 => {
            let current = host::context_kind(st);
            if current == KIND_2D || current == KIND_WEBGPU || host::is_worker(scope) {
                return Ok(Value::null());
            }
            let version = if kind == ContextType::WebGl { 1 } else { 2 };
            let gl = host::webgl_context(scope, canvas, el, version, options)?;
            if !gl.is_null() {
                host::set_context_kind(st, KIND_WEBGL);
            }
            Ok(gl)
        }
        ContextType::WebGpu => {
            let current = host::context_kind(st);
            if current != 0 && current != KIND_WEBGPU {
                return Ok(Value::null());
            }
            let Some(gpu) = host::webgpu_context(scope, canvas, el)? else {
                return Ok(Value::null());
            };
            if !gpu.is_null() {
                host::set_context_kind(st, KIND_WEBGPU);
            }
            Ok(gpu)
        }
        ContextType::Unknown | ContextType::Bitmap => {
            if !offscreen || kind == ContextType::Bitmap {
                return Ok(Value::null());
            }
            Err(scope.type_error(
                "Failed to execute 'getContext' on 'OffscreenCanvas': The provided value is not \
                 a valid enum value of type OffscreenRenderingContextId.",
            ))
        }
    }
}

fn get_context(
    scope: &mut Scope<'_>,
    el: Node,
    canvas: &Value,
    offscreen: bool,
    args: &[Value],
) -> Result<Value, Value> {
    let js = crate::ffi::state::js_of(scope);
    if js.is_null() || el.is_null() {
        return Ok(Value::null());
    }
    let Some(st) = crate::state::state_for(js, el) else {
        return Ok(Value::null());
    };
    let undefined = Value::undefined();
    let kind = context_type(scope, args.first().unwrap_or(&undefined))?;
    let options = args.get(1).unwrap_or(&undefined);
    get_by_type(scope, st, el, canvas, offscreen, kind, options)
}

pub(crate) fn element_get_context(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let el = host::unwrap_element(this);
    if el.is_null() || crate::ffi::state::js_of(scope).is_null() {
        return Ok(Value::null());
    }
    if args.is_empty() {
        return Err(scope.type_error(
            "Failed to execute 'getContext' on 'HTMLCanvasElement': 1 argument required, but \
             only 0 present.",
        ));
    }
    get_context(scope, el, this, false, args)
}

pub(crate) fn offscreen_get_context(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let el = Node::from_addr(crate::api::offscreen_node(this));
    get_context(scope, el, this, true, args)
}

const CONTEXT_ATTRIBUTE_KEYS: [&str; 4] = ["alpha", "colorSpace", "colorType", "desynchronized"];

pub(crate) fn get_attributes(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let saved = crate::hidden::get(scope, this, "_attrs")?;
    let out = scope.new_object();
    for key in CONTEXT_ATTRIBUTE_KEYS {
        let value = scope.get(&saved, key)?;
        let _ = scope.set(&out, key, value);
    }
    let tone = scope.new_object();
    let standard = scope.string("standard");
    let _ = scope.set(&tone, "mode", standard);
    let _ = scope.set(&out, "toneMapping", tone);
    let frequent = scope.get(&saved, "willReadFrequently")?;
    let _ = scope.set(&out, "willReadFrequently", frequent);
    Ok(out)
}

pub(crate) fn is_context_lost(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(Value::boolean(false))
}

pub(crate) fn draw_focus_if_needed(
    _: &mut Scope<'_>,
    _: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    Ok(Value::undefined())
}

fn make_blob(scope: &mut Scope<'_>, data: &[u8], kind: &str) -> Result<Value, Value> {
    let global = scope.global();
    let blob_ctor = scope.get(&global, "Blob")?;
    let u8_ctor = scope.get(&global, "Uint8Array")?;
    let buffer = southstar_js_engine::quickjs::array_buffer_copy(scope, data)?;
    let bytes = scope.construct(&u8_ctor, &[buffer])?;
    let parts = scope.new_array();
    scope.set_index(&parts, 0, bytes)?;
    let options = scope.new_object();
    let kind = scope.string(kind);
    scope.set(&options, "type", kind)?;
    scope.construct(&blob_ctor, &[parts, options])
}

fn png_blob(scope: &mut Scope<'_>, surface: &Surface) -> Result<Value, Value> {
    match surface.png() {
        Some(png) => make_blob(scope, &png, "image/png"),
        None => Ok(Value::null()),
    }
}

pub(crate) fn convert_to_blob(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let (promise, resolve, reject) = scope.new_promise()?;
    let el = Node::from_addr(crate::api::offscreen_node(this));
    let js = crate::ffi::state::js_of(scope);
    let mut outcome: Result<Value, Value> = Ok(Value::null());
    if !el.is_null() && !js.is_null() {
        if let Some(st) = crate::state::state_for(js, el) {
            if !host::origin_clean(st) {
                outcome = Err(crate::api::throw_dom(
                    scope,
                    "SecurityError",
                    "Tainted canvases may not be exported.",
                ));
            } else if let Some(surface) = host::surface_of(st) {
                outcome = png_blob(scope, &surface);
            }
        }
    }
    let (settle, value) = match outcome {
        Ok(value) => (resolve, value),
        Err(error) => (reject, error),
    };
    let _ = scope.call(&settle, &Value::undefined(), &[value]);
    Ok(promise)
}

const MATRIX_KEYS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

pub(crate) fn dommatrix(scope: &mut Scope<'_>, m: [f64; 6]) -> Value {
    let init = scope.new_array();
    for (i, v) in m.iter().enumerate() {
        let _ = scope.set_index(&init, i as u32, Value::number(*v));
    }
    let global = scope.global();
    let ctor = scope
        .get(&global, "DOMMatrix")
        .unwrap_or_else(|_| Value::undefined());
    if scope.is_function(&ctor) {
        if let Ok(matrix) = scope.construct(&ctor, &[init]) {
            if matrix.is_object() {
                return matrix;
            }
        }
    }
    let plain = scope.new_object();
    for (key, v) in MATRIX_KEYS.iter().zip(m) {
        let _ = scope.set(&plain, key, Value::number(v));
    }
    plain
}
