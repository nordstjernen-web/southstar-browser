//! Southstar — navigator.gpu, GPUAdapter, GPUDevice, GPUQueue and GPUBuffer methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use southstar_js_engine::{Scope, Value};

use crate::ffi::host::internal_error;
use crate::ffi::sys::BufferDescriptor;
use crate::ffi::wgpu::{self, Adapter, Buffer, CommandBuffer, Queue, Zeroed};
use crate::objects::{
    BufferState, DeviceState, make_adapter, make_buffer, make_device, make_queue,
};
use crate::util::{
    JsResult, arg, array_len, defined, host, i64_of, i64_prop, index, prop, rejected, resolved,
    u32_prop,
};

pub(crate) fn allowed() -> bool {
    std::env::var_os("NS_WEBGPU_ALLOW").is_some_and(|v| v.as_encoded_bytes().first() == Some(&b'1'))
}

pub(crate) fn request_adapter(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    if !allowed() {
        return resolved(scope, Value::null());
    }
    let Some(instance) = wgpu::instance() else {
        return resolved(scope, Value::null());
    };
    match wgpu::request_adapter(instance) {
        Some(adapter) => {
            let adapter = make_adapter(scope, adapter);
            resolved(scope, adapter)
        }
        None => resolved(scope, Value::null()),
    }
}

pub(crate) fn preferred_canvas_format(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.string("bgra8unorm"))
}

pub(crate) fn adapter_request_device(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(adapter) = host::<Adapter>(this) else {
        return rejected(scope, "requestDevice: invalid adapter");
    };
    let Some(device) = adapter.request_device() else {
        return rejected(scope, "requestDevice: no device");
    };
    let device = make_device(scope, device);
    resolved(scope, device)
}

pub(crate) fn device_get_queue(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(d) = host::<DeviceState>(this) else {
        return Ok(Value::undefined());
    };
    Ok(make_queue(scope, d.queue.clone()))
}

pub(crate) fn device_destroy(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn device_push_error_scope(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn device_pop_error_scope(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    resolved(scope, Value::null())
}

pub(crate) fn device_create_buffer(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let d = host::<DeviceState>(this);
    let desc = arg(args, 0);
    let Some(d) = d.filter(|_| desc.is_object()) else {
        return rejected(scope, "createBuffer: descriptor required");
    };
    let size = i64_prop(scope, &desc, "size");
    let usage = u32_prop(scope, &desc, "usage");
    let mapped = prop(scope, &desc, "mappedAtCreation");
    let mapped = scope.to_bool(&mapped);
    let mut descriptor = BufferDescriptor::zeroed();
    descriptor.size = size.max(0) as u64;
    descriptor.usage = u64::from(usage);
    descriptor.mapped_at_creation = u32::from(mapped);
    let Some(buffer) = d.device.create_buffer(&descriptor) else {
        return Err(internal_error(scope, "createBuffer: wgpu returned null"));
    };
    let state = BufferState {
        buffer: Some(buffer),
        size: descriptor.size,
        device: d.device.clone(),
        ranges: RefCell::new(Vec::new()),
        escaped: Cell::new(false),
    };
    Ok(make_buffer(scope, state, usage))
}

fn detach_ranges(scope: &mut Scope<'_>, b: &BufferState) -> bool {
    let ranges = core::mem::take(&mut *b.ranges.borrow_mut());
    for range in ranges {
        if scope.array_buffer_bytes(&range).is_none() {
            b.escaped.set(true);
        }
        scope.detach_array_buffer(&range).ok();
    }
    !b.escaped.get()
}

pub(crate) fn buffer_destroy(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(b) = host::<BufferState>(this)
        && detach_ranges(scope, &b)
        && let Some(buffer) = &b.buffer
    {
        buffer.destroy();
    }
    Ok(Value::undefined())
}

pub(crate) fn buffer_unmap(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(b) = host::<BufferState>(this)
        && detach_ranges(scope, &b)
        && let Some(buffer) = &b.buffer
    {
        buffer.unmap();
    }
    Ok(Value::undefined())
}

fn range_args(scope: &mut Scope<'_>, args: &[Value], first: usize) -> (i64, i64) {
    let offset = defined(args, first).map_or(0, |v| i64_of(scope, v));
    let size = defined(args, first + 1).map_or(-1, |v| i64_of(scope, v));
    (offset, size)
}

fn range_size(total: u64, offset: i64, size: i64) -> Option<usize> {
    if offset < 0 || offset as u64 > total {
        return None;
    }
    Some(if size < 0 {
        (total - offset as u64) as usize
    } else {
        size as usize
    })
}

pub(crate) fn buffer_get_mapped_range(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(b) = host::<BufferState>(this) else {
        return Ok(Value::undefined());
    };
    let Some(buffer) = &b.buffer else {
        return Ok(Value::undefined());
    };
    let (offset, size) = range_args(scope, args, 0);
    let Some(size) = range_size(b.size, offset, size) else {
        return Ok(Value::undefined());
    };
    let Some(range) = buffer.mapped_array_buffer(scope, offset as usize, size) else {
        return Err(internal_error(scope, "getMappedRange failed"));
    };
    let range = range?;
    b.ranges.borrow_mut().push(range.clone());
    Ok(range)
}

pub(crate) fn buffer_map_async(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(b) = host::<BufferState>(this) else {
        return rejected(scope, "mapAsync: buffer");
    };
    let Some(buffer) = &b.buffer else {
        return rejected(scope, "mapAsync: buffer");
    };
    let mode = args.first().map_or(1, |v| i64_of(scope, v));
    let (offset, size) = range_args(scope, args, 1);
    let Some(size) = range_size(b.size, offset, size) else {
        return rejected(scope, "mapAsync: range");
    };
    buffer.map_and_wait(mode as u64, offset as usize, size, Some(&b.device));
    resolved(scope, Value::undefined())
}

fn element_size(scope: &mut Scope<'_>, value: &Value) -> usize {
    scope.typed_array_element(value).map_or(1, |e| e.size())
}

pub(crate) fn queue_write_buffer(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(queue) = host::<Queue>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    let Some(b) = host::<BufferState>(&args[0]) else {
        return Ok(Value::undefined());
    };
    let buffer_offset = i64_of(scope, &args[1]);
    let data_offset = defined(args, 3).map_or(0, |v| i64_of(scope, v));
    let size = defined(args, 4).map_or(-1, |v| i64_of(scope, v));
    let elem = element_size(scope, &args[2]);
    let Some(buffer) = &b.buffer else {
        return Ok(Value::undefined());
    };
    scope.with_buffer_bytes_mut(&args[2], |bytes| {
        if buffer_offset < 0 || data_offset < 0 || size < -1 {
            return;
        }
        let view_elems = (bytes.len() / elem) as u64;
        if data_offset as u64 > view_elems {
            return;
        }
        let avail = view_elems - data_offset as u64;
        let write_elems = if size < 0 { avail } else { size as u64 };
        if write_elems > avail {
            return;
        }
        let start = data_offset as usize * elem;
        let end = start + write_elems as usize * elem;
        queue.write_buffer(buffer, buffer_offset as u64, &bytes[start..end]);
    });
    Ok(Value::undefined())
}

pub(crate) fn queue_submit(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(queue) = host::<Queue>(this) else {
        return Ok(Value::undefined());
    };
    let Some(list) = args.first() else {
        return Ok(Value::undefined());
    };
    let len = array_len(scope, list).min(4096);
    if len == 0 {
        return Ok(Value::undefined());
    }
    let mut commands: Vec<Rc<CommandBuffer>> = Vec::new();
    for i in 0..len {
        let entry = index(scope, list, i);
        if let Some(command) = host::<CommandBuffer>(&entry) {
            commands.push(command);
        }
    }
    if !commands.is_empty() {
        let refs: Vec<&CommandBuffer> = commands.iter().map(|c| &**c).collect();
        queue.submit(&refs);
    }
    Ok(Value::undefined())
}

pub(crate) fn buffer_of(value: &Value) -> Option<Rc<BufferState>> {
    host::<BufferState>(value)
}

pub(crate) fn raw_buffer(state: &BufferState) -> Option<&Buffer> {
    state.buffer.as_ref()
}
