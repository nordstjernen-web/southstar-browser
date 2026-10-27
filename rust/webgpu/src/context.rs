//! Southstar — GPUCanvasContext: canvas.getContext('webgpu') and the readback that paints it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_js_engine::{Scope, Value};

use crate::ffi::host::{Surface, canvas_attr, internal_error};
use crate::ffi::sys::*;
use crate::ffi::wgpu::{Device, Queue, Texture, Zeroed};
use crate::objects::{DeviceState, make_texture};
use crate::parse;
use crate::util::{JsResult, arg, bind, host, prop, set, set_str, str_prop};

pub(crate) static INSTALLED: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Ctx {
    id: u64,
    canvas: usize,
    target: Option<Texture>,
    surf: Option<Surface>,
    device: Option<Device>,
    queue: Option<Queue>,
    format: u32,
    w: i32,
    h: i32,
    configured: bool,
    opaque: bool,
}

type Registered = (u64, Weak<RefCell<Ctx>>);

thread_local! {
    static BY_NODE: RefCell<HashMap<usize, Registered>> = RefCell::new(HashMap::new());
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let (id, canvas) = (self.id, self.canvas);
        BY_NODE
            .try_with(|map| {
                if let Ok(mut map) = map.try_borrow_mut()
                    && map.get(&canvas).is_some_and(|(owner, _)| *owner == id)
                {
                    map.remove(&canvas);
                }
            })
            .ok();
        self.release_gpu();
    }
}

impl Ctx {
    fn release_gpu(&mut self) {
        self.target = None;
        self.surf = None;
    }

    fn ensure_target(&mut self) -> bool {
        let w = canvas_dim(self.canvas, c"width", 300);
        let h = canvas_dim(self.canvas, c"height", 150);
        if self.target.is_some() && w == self.w && h == self.h {
            return true;
        }
        self.release_gpu();
        self.w = w;
        self.h = h;
        let Some(device) = &self.device else {
            return false;
        };
        let mut descriptor = TextureDescriptor::zeroed();
        descriptor.usage = TEXTURE_USAGE_RENDER_ATTACHMENT
            | TEXTURE_USAGE_COPY_SRC
            | TEXTURE_USAGE_TEXTURE_BINDING;
        descriptor.dimension = TEXTURE_DIMENSION_2_D;
        descriptor.size = Extent3D {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        };
        descriptor.format = self.format;
        descriptor.mip_level_count = 1;
        descriptor.sample_count = 1;
        self.target = device.create_texture(&descriptor);
        self.target.is_some()
    }
}

fn parse_long(text: &[u8]) -> i64 {
    let mut rest = text;
    while let Some((&c, tail)) = rest.split_first() {
        if matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r') {
            rest = tail;
        } else {
            break;
        }
    }
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'-' | b'+')) {
        rest = &rest[1..];
    }
    let mut value: i64 = 0;
    for &c in rest.iter().take_while(|c| c.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(i64::from(c - b'0'));
    }
    if negative { -value } else { value }
}

fn canvas_dim(canvas: usize, name: &CStr, default: i32) -> i32 {
    let Some(text) = canvas_attr(canvas, name) else {
        return default;
    };
    if text.is_empty() {
        return default;
    }
    let v = parse_long(&text);
    if v <= 0 {
        return default;
    }
    v.min(8192) as i32
}

fn context_of(value: &Value) -> Option<Rc<RefCell<Ctx>>> {
    host::<RefCell<Ctx>>(value)
}

fn configure(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let c = context_of(this);
    let desc = arg(args, 0);
    let Some(c) = c.filter(|_| desc.is_object()) else {
        return Err(scope.type_error("configure: descriptor required"));
    };
    let Some(d) = host::<DeviceState>(&prop(scope, &desc, "device")) else {
        return Err(scope.type_error("configure: valid device required"));
    };
    let format = str_prop(scope, &desc, "format");
    let alpha = str_prop(scope, &desc, "alphaMode");
    let mut c = c.borrow_mut();
    c.device = None;
    c.queue = None;
    c.device = Some(d.device.clone());
    c.queue = d.device.queue();
    c.format = parse::texture_format(format.as_deref());
    c.opaque = alpha.as_deref() != Some("premultiplied");
    c.configured = true;
    c.release_gpu();
    Ok(Value::undefined())
}

fn unconfigure(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(c) = context_of(this) {
        let mut c = c.borrow_mut();
        c.configured = false;
        c.release_gpu();
    }
    Ok(Value::undefined())
}

fn get_current_texture(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let c = context_of(this);
    let Some(c) = c.filter(|c| {
        let c = c.borrow();
        c.configured && c.device.is_some()
    }) else {
        return Err(scope.type_error("InvalidStateError: getCurrentTexture: not configured"));
    };
    let mut c = c.borrow_mut();
    if !c.ensure_target() {
        return Err(internal_error(scope, "getCurrentTexture: no target"));
    }
    let Some(target) = c.target.clone() else {
        return Err(internal_error(scope, "getCurrentTexture: no target"));
    };
    let (w, h, format) = (c.w as u32, c.h as u32, c.format);
    drop(c);
    Ok(make_texture(scope, target, w, h, format))
}

fn get_configuration(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(c) = context_of(this) else {
        return Ok(Value::null());
    };
    let (configured, format, opaque) = {
        let c = c.borrow();
        (c.configured, c.format, c.opaque)
    };
    if !configured {
        return Ok(Value::null());
    }
    let o = scope.new_object();
    set_str(scope, &o, "format", parse::format_name(format));
    set_str(
        scope,
        &o,
        "alphaMode",
        if opaque { "opaque" } else { "premultiplied" },
    );
    Ok(o)
}

pub(crate) fn get_context(scope: &mut Scope<'_>, canvas_obj: &Value, canvas: usize) -> Value {
    if !crate::gpu::allowed() || !INSTALLED.load(Ordering::Relaxed) {
        return Value::null();
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let ctx = Rc::new(RefCell::new(Ctx {
        id,
        canvas,
        target: None,
        surf: None,
        device: None,
        queue: None,
        format: TEXTURE_FORMAT_BGRA8_UNORM,
        w: 0,
        h: 0,
        configured: false,
        opaque: true,
    }));
    let weak = Rc::downgrade(&ctx);
    let obj = scope.new_host_object(None, ctx);
    BY_NODE.with(|map| map.borrow_mut().insert(canvas, (id, weak)));
    bind(scope, &obj, "configure", configure, 1);
    bind(scope, &obj, "unconfigure", unconfigure, 0);
    bind(scope, &obj, "getCurrentTexture", get_current_texture, 0);
    bind(scope, &obj, "getConfiguration", get_configuration, 0);
    set(scope, &obj, "canvas", canvas_obj.clone());
    obj
}

fn registered(canvas: usize) -> Option<Rc<RefCell<Ctx>>> {
    BY_NODE.with(|map| {
        map.borrow()
            .get(&canvas)
            .and_then(|(_, weak)| weak.upgrade())
    })
}

pub(crate) fn canvas_surface(canvas: usize) -> Option<*mut c_void> {
    let ctx = registered(canvas)?;
    let mut c = ctx.try_borrow_mut().ok()?;
    let c = &mut *c;
    if !c.configured {
        return None;
    }
    let (Some(target), Some(device), Some(queue)) = (&c.target, &c.device, &c.queue) else {
        return None;
    };
    let (w, h) = (c.w, c.h);
    if w <= 0 || h <= 0 {
        return None;
    }
    let bytes_per_row = ((w as u32) * 4 + 255) & !255;
    let size = u64::from(bytes_per_row) * h as u64;
    let mut descriptor = BufferDescriptor::zeroed();
    descriptor.usage = BUFFER_USAGE_MAP_READ | BUFFER_USAGE_COPY_DST;
    descriptor.size = size;
    let surf_ptr = |surf: &Option<Surface>| surf.as_ref().map(Surface::as_ptr);
    let Some(readback) = device.create_buffer(&descriptor) else {
        return surf_ptr(&c.surf);
    };
    let encoder = device.create_command_encoder();
    let mut command = None;
    if let Some(encoder) = &encoder {
        let mut source = TexelCopyTextureInfo::zeroed();
        source.texture = target.raw();
        source.aspect = TEXTURE_ASPECT_ALL;
        let mut destination = TexelCopyBufferInfo::zeroed();
        destination.buffer = readback.raw();
        destination.layout.bytes_per_row = bytes_per_row;
        destination.layout.rows_per_image = h as u32;
        let extent = Extent3D {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_buffer(&source, &destination, &extent);
        command = encoder.finish();
        if let Some(command) = &command {
            queue.submit(&[command]);
        }
    }
    let done = readback.map_and_wait(MAP_MODE_READ, 0, size as usize, Some(device));
    if done {
        let (opaque, swap) = (c.opaque, c.format == TEXTURE_FORMAT_RGBA8_UNORM);
        let surf = &mut c.surf;
        readback.with_const_mapped(size as usize, |map| {
            if surf.is_none() {
                *surf = Surface::new(w, h);
            }
            if let Some(surf) = surf.as_ref() {
                surf.with_pixels_mut(|pixels, stride| {
                    copy_rows(
                        map,
                        pixels,
                        stride,
                        bytes_per_row as usize,
                        (w as usize, h as usize),
                        swap,
                        opaque,
                    );
                });
            }
        });
    }
    drop(command);
    drop(encoder);
    drop(readback);
    surf_ptr(&c.surf)
}

fn copy_rows(
    map: &[u8],
    pixels: &mut [u8],
    stride: usize,
    bytes_per_row: usize,
    (w, h): (usize, usize),
    swap: bool,
    opaque: bool,
) {
    for y in 0..h {
        let src = &map[y * bytes_per_row..y * bytes_per_row + w * 4];
        let dst = &mut pixels[y * stride..y * stride + w * 4];
        for (&[r, g, b, a], d) in src
            .as_chunks::<4>()
            .0
            .iter()
            .zip(dst.as_chunks_mut::<4>().0)
        {
            if swap {
                d[0] = b;
                d[1] = g;
                d[2] = r;
            } else {
                d[0] = r;
                d[1] = g;
                d[2] = b;
            }
            d[3] = if opaque { 255 } else { a };
        }
    }
}
