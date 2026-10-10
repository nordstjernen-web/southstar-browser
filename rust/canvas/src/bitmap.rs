//! Southstar — ImageBitmap: decoded or copied pixels on a cairo surface, createImageBitmap() and the bitmap's accessors, close() and cloning.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi;
use crate::ffi::cairo::Surface;

const MAX_SIDE: i32 = 32767;

const MAX_BLOB_BYTES: u32 = 256 * 1024 * 1024;

pub(crate) struct ImageBitmap {
    surface: RefCell<Option<Surface>>,
    size: Cell<(i32, i32)>,
    origin_clean: bool,
}

pub(crate) struct Source {
    pub surface: Surface,
    pub size: (i32, i32),
    pub origin_clean: bool,
}

impl ImageBitmap {
    pub fn source(&self) -> Option<Source> {
        let surface = self.surface.borrow().as_ref().map(Surface::reference)?;
        Some(Source {
            surface,
            size: self.size.get(),
            origin_clean: self.origin_clean,
        })
    }
}

pub(crate) fn surface_of(value: &Value) -> Option<Source> {
    ffi::with_bitmap(value, ImageBitmap::source).flatten()
}

pub(crate) fn is(value: &Value) -> bool {
    ffi::with_bitmap(value, |_| ()).is_some()
}

pub(crate) fn make(
    scope: &mut Scope<'_>,
    surface: Option<Surface>,
    size: (i32, i32),
    origin_clean: bool,
) -> Value {
    let Some(surface) = surface.filter(|_| size.0 > 0 && size.1 > 0) else {
        return Value::null();
    };
    let bitmap = ImageBitmap {
        surface: RefCell::new(Some(surface)),
        size: Cell::new(size),
        origin_clean,
    };
    let proto = crate::api::api_proto(scope, "ImageBitmap");
    let proto = proto.is_object().then_some(&proto);
    scope.new_host_object(proto, bitmap)
}

pub(crate) fn close(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let closed = ffi::with_bitmap(this, |b| {
        let surface = b.surface.borrow_mut().take();
        if surface.is_some() {
            b.size.set((0, 0));
        }
        surface
    });
    match closed {
        Some(surface) => {
            drop(surface);
            Ok(Value::undefined())
        }
        None => Err(scope.type_error("Illegal invocation")),
    }
}

fn size_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let height = data
        .first()
        .and_then(|v| scope.to_int32(v).ok())
        .unwrap_or(0)
        != 0;
    match ffi::with_bitmap(this, |b| b.size.get()) {
        Some((w, h)) => Ok(Value::int(if height { h } else { w })),
        None => Err(scope.type_error("Illegal invocation")),
    }
}

pub(crate) fn define_members(scope: &mut Scope<'_>) {
    let proto = crate::api::api_proto(scope, "ImageBitmap");
    let accessor = Attributes {
        writable: false,
        enumerable: true,
        configurable: true,
    };
    for (i, name) in ["width", "height"].into_iter().enumerate() {
        let getter = scope.bound_function(
            &format!("get {name}"),
            0,
            size_getter,
            &[Value::int(i as i32)],
        );
        let _ = scope.define_accessor(&proto, name, Some(&getter), None, accessor);
    }
}

pub(crate) fn clone(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
    let Some(source) = surface_of(value) else {
        return Err(crate::api::throw_dom(
            scope,
            "DataCloneError",
            "The ImageBitmap is closed.",
        ));
    };
    let (w, h) = source.size;
    let copy = Surface::image(w, h);
    source.surface.paint_onto(&copy, (0.0, 0.0), true);
    Ok(make(scope, Some(copy), source.size, source.origin_clean))
}

fn premultiply(r: u8, g: u8, b: u8, a: u8) -> [u8; 4] {
    let scale = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
    match a {
        0 => [0, 0, 0, 0],
        255 => [b, g, r, a],
        _ => [scale(b), scale(g), scale(r), a],
    }
}

fn from_imagedata(scope: &mut Scope<'_>, src: &Value) -> Option<(Surface, i32, i32)> {
    let w = scope
        .get(src, "width")
        .unwrap_or_else(|_| Value::undefined());
    let h = scope
        .get(src, "height")
        .unwrap_or_else(|_| Value::undefined());
    let data = scope
        .get(src, "data")
        .unwrap_or_else(|_| Value::undefined());
    let iw = scope.to_int32(&w).unwrap_or(0);
    let ih = scope.to_int32(&h).unwrap_or(0);
    if iw <= 0 || ih <= 0 || iw > MAX_SIDE || ih > MAX_SIDE {
        return None;
    }
    let row = iw as usize * 4;
    let needed = row * ih as usize;
    let rgba = scope
        .with_typed_array(&data, |t| {
            (t.bytes.len() >= needed).then(|| t.bytes[..needed].to_vec())
        })
        .flatten()?;
    let surface = Surface::image(iw, ih);
    if !surface.is_ok() {
        return None;
    }
    surface.write_pixels(|dst, stride| {
        for (y, src_row) in rgba.chunks_exact(row).enumerate() {
            let dst_row = &mut dst[y * stride..y * stride + row];
            let (dst_px, _) = dst_row.as_chunks_mut::<4>();
            let (src_px, _) = src_row.as_chunks::<4>();
            for (d, &[r, g, b, a]) in dst_px.iter_mut().zip(src_px) {
                *d = premultiply(r, g, b, a);
            }
        }
    });
    Some((surface, iw, ih))
}

fn crop(src: &Surface, origin: (i32, i32), size: (i32, i32)) -> Option<Surface> {
    let out = Surface::image(size.0, size.1);
    if !out.is_ok() {
        return None;
    }
    src.paint_onto(&out, (-f64::from(origin.0), -f64::from(origin.1)), false);
    Some(out)
}

fn blob_bytes(scope: &mut Scope<'_>, src: &Value) -> Option<Vec<u8>> {
    let bytes = scope
        .get(src, "__ndBlobBytes")
        .ok()
        .filter(Value::is_object)?;
    let length = scope.get(&bytes, "length").ok()?;
    let len = scope.to_int32(&length).map_or(0, |n| n as u32);
    if len == 0 || len > MAX_BLOB_BYTES {
        return None;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(len as usize).ok()?;
    for i in 0..len {
        let byte = scope
            .get_index(&bytes, i)
            .ok()
            .and_then(|v| scope.to_int32(&v).ok())
            .unwrap_or(0);
        out.push((byte & 0xff) as u8);
    }
    Some(out)
}

fn from_blob(scope: &mut Scope<'_>, src: &Value) -> Option<(Surface, i32, i32)> {
    let bytes = blob_bytes(scope, src)?;
    let decoded = ffi::decode_image(&bytes)?;
    let (w, h) = (decoded.width, decoded.height);
    if w <= 0 || h <= 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    let row = w as usize * 4;
    if decoded.stride < row || h as usize > decoded.pixels.len() / decoded.stride {
        return None;
    }
    let surface = Surface::image(w, h);
    if !surface.is_ok() {
        return None;
    }
    surface.write_pixels(|dst, stride| {
        for y in 0..h as usize {
            let from = &decoded.pixels[y * decoded.stride..y * decoded.stride + row];
            dst[y * stride..y * stride + row].copy_from_slice(from);
        }
    });
    Some((surface, w, h))
}

pub(crate) fn promise_reject(scope: &mut Scope<'_>, reject: &Value, message: &str) {
    let error = scope.new_error();
    let name_len = message.find(':').unwrap_or(message.len());
    let name = &message[..name_len];
    let is_dom_name = name_len > 5
        && name_len < 64
        && name.ends_with("Error")
        && name.bytes().all(|c| c.is_ascii_alphanumeric());
    if is_dom_name {
        let rest = message[name_len..].strip_prefix(':').unwrap_or(message);
        let rest = rest.trim_start_matches(' ');
        let name_value = scope.string(name);
        let _ = scope.set(&error, "name", name_value);
        let message_value = scope.string(rest);
        let _ = scope.set(&error, "message", message_value);
    } else {
        let message_value = scope.string(message);
        let _ = scope.set(&error, "message", message_value);
    }
    let _ = scope.call(reject, &Value::undefined(), &[error]);
}

fn reject_undecodable(scope: &mut Scope<'_>, reject: &Value) {
    let global = scope.global();
    let ctor = scope
        .get(&global, "DOMException")
        .unwrap_or_else(|_| Value::undefined());
    let args = [
        scope.string("The source image could not be decoded."),
        scope.string("InvalidStateError"),
    ];
    let exception = match scope.construct(&ctor, &args) {
        Ok(exception) if exception.is_object() => exception,
        _ => {
            let error = scope.new_error();
            let name = scope.string("InvalidStateError");
            let _ = scope.set(&error, "name", name);
            error
        }
    };
    let _ = scope.call(reject, &Value::undefined(), &[exception]);
}

pub(crate) fn create(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (promise, resolve, reject) = scope.new_promise()?;
    let Some(source) = args.first().filter(|v| v.is_object()) else {
        promise_reject(scope, &reject, "createImageBitmap: source required");
        return Ok(promise);
    };
    let is_imagedata = scope.get(source, "data").is_ok_and(|v| v.is_object());
    let is_blob = !is_imagedata
        && scope
            .get(source, "__ndBlobBytes")
            .is_ok_and(|v| v.is_object());
    let mut origin_clean = true;
    let decoded = if is_imagedata {
        from_imagedata(scope, source)
    } else if is_blob {
        from_blob(scope, source)
    } else {
        ffi::drawimage_source(scope, source).map(|s| {
            origin_clean = s.origin_clean;
            (s.surface, s.size.0, s.size.1)
        })
    };
    let Some((mut surface, mut sw, mut sh)) = decoded else {
        reject_undecodable(scope, &reject);
        return Ok(promise);
    };
    if args.len() >= 5 {
        let mut crop_args = [0i32, 0, sw, sh];
        for (slot, arg) in crop_args.iter_mut().zip(&args[1..5]) {
            *slot = scope.to_int32(arg).unwrap_or(0);
        }
        let [sx, sy, rw, rh] = crop_args;
        if rw <= 0 || rh <= 0 || rw > MAX_SIDE || rh > MAX_SIDE {
            drop(surface);
            promise_reject(scope, &reject, "createImageBitmap: invalid crop size");
            return Ok(promise);
        }
        let Some(out) = crop(&surface, (sx, sy), (rw, rh)) else {
            drop(surface);
            promise_reject(scope, &reject, "createImageBitmap: crop failed");
            return Ok(promise);
        };
        surface = out;
        sw = rw;
        sh = rh;
    }
    let bitmap = make(scope, Some(surface), (sw, sh), origin_clean);
    let _ = scope.call(&resolve, &Value::undefined(), &[bitmap]);
    Ok(promise)
}
