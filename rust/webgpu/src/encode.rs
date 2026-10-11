//! Southstar — GPUCommandEncoder, GPURenderPassEncoder and GPUComputePassEncoder.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use std::rc::Rc;

use southstar_js_engine::{Scope, Value};

use crate::ffi::sys::*;
use crate::ffi::wgpu::{
    BindGroup, CommandEncoder, ComputePass, ComputePipeline, QuerySet, RenderPass, RenderPipeline,
    TextureView, Zeroed,
};
use crate::objects::{DeviceState, make_command_buffer, make_compute_pass, make_render_pass};
use crate::pipeline::{read_extent, read_texture_copy};
use crate::util::{
    JsResult, arg, bind, defined, f64_of, host, i32_of, i64_of, index, prop, str_prop, u32_of,
};

pub(crate) fn device_create_command_encoder(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> JsResult {
    let Some(d) = host::<DeviceState>(this) else {
        return Ok(Value::undefined());
    };
    let Some(encoder) = d.device.create_command_encoder() else {
        return Ok(Value::undefined());
    };
    let obj = scope.new_host_object(None, Rc::new(encoder));
    bind(scope, &obj, "beginRenderPass", encoder_begin_render_pass, 1);
    bind(
        scope,
        &obj,
        "beginComputePass",
        encoder_begin_compute_pass,
        1,
    );
    bind(
        scope,
        &obj,
        "copyTextureToTexture",
        encoder_copy_texture_to_texture,
        3,
    );
    bind(
        scope,
        &obj,
        "copyBufferToBuffer",
        encoder_copy_buffer_to_buffer,
        5,
    );
    bind(scope, &obj, "resolveQuerySet", encoder_resolve_query_set, 5);
    bind(scope, &obj, "finish", encoder_finish, 0);
    Ok(obj)
}

fn color_component(scope: &mut Scope<'_>, color: &Value, key: &str, i: u32) -> f64 {
    if color.is_array() {
        let e = index(scope, color, i);
        f64_of(scope, &e)
    } else if color.is_object() {
        let e = prop(scope, color, key);
        f64_of(scope, &e)
    } else {
        0.0
    }
}

fn view_of(value: &Value) -> Option<Rc<TextureView>> {
    host::<TextureView>(value)
}

fn color_attachment(
    scope: &mut Scope<'_>,
    desc: &Value,
    held: &mut Vec<Rc<TextureView>>,
) -> RenderPassColorAttachment {
    let mut color = RenderPassColorAttachment::zeroed();
    color.depth_slice = UNDEFINED_U32;
    color.load_op = LOAD_OP_CLEAR;
    color.store_op = STORE_OP_STORE;
    let attachments = prop(scope, desc, "colorAttachments");
    if !attachments.is_array() {
        return color;
    }
    let a0 = index(scope, &attachments, 0);
    if !a0.is_object() {
        return color;
    }
    if let Some(view) = view_of(&prop(scope, &a0, "view")) {
        color.view = view.raw();
        held.push(view);
    }
    if str_prop(scope, &a0, "loadOp").as_deref() == Some("load") {
        color.load_op = LOAD_OP_LOAD;
    }
    if str_prop(scope, &a0, "storeOp").as_deref() == Some("discard") {
        color.store_op = STORE_OP_DISCARD;
    }
    let clear = prop(scope, &a0, "clearValue");
    color.clear_value.r = color_component(scope, &clear, "r", 0);
    color.clear_value.g = color_component(scope, &clear, "g", 1);
    color.clear_value.b = color_component(scope, &clear, "b", 2);
    color.clear_value.a = if clear.is_undefined() {
        1.0
    } else {
        color_component(scope, &clear, "a", 3)
    };
    if let Some(view) = view_of(&prop(scope, &a0, "resolveTarget")) {
        color.resolve_target = view.raw();
        held.push(view);
    }
    color
}

fn depth_attachment(
    scope: &mut Scope<'_>,
    desc: &Value,
    held: &mut Vec<Rc<TextureView>>,
) -> Option<RenderPassDepthStencilAttachment> {
    let ds = prop(scope, desc, "depthStencilAttachment");
    if !ds.is_object() {
        return None;
    }
    let view = view_of(&prop(scope, &ds, "view"))?;
    let mut depth = RenderPassDepthStencilAttachment::zeroed();
    depth.view = view.raw();
    held.push(view);
    depth.depth_load_op = LOAD_OP_CLEAR;
    depth.depth_store_op = STORE_OP_STORE;
    depth.depth_clear_value = 1.0;
    if str_prop(scope, &ds, "depthLoadOp").as_deref() == Some("load") {
        depth.depth_load_op = LOAD_OP_LOAD;
    }
    if str_prop(scope, &ds, "depthStoreOp").as_deref() == Some("discard") {
        depth.depth_store_op = STORE_OP_DISCARD;
    }
    let clear = prop(scope, &ds, "depthClearValue");
    if !clear.is_undefined() {
        depth.depth_clear_value = scope.to_number(&clear).unwrap_or(1.0) as f32;
    }
    Some(depth)
}

fn encoder_begin_render_pass(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    let desc = arg(args, 0);
    if !desc.is_object() {
        return Ok(Value::undefined());
    }
    let mut held = Vec::new();
    let color = color_attachment(scope, &desc, &mut held);
    if color.view.is_null() {
        return Ok(Value::undefined());
    }
    let depth = depth_attachment(scope, &desc, &mut held);
    let mut descriptor = RenderPassDescriptor::zeroed();
    descriptor.color_attachment_count = 1;
    descriptor.color_attachments = &color;
    descriptor.depth_stencil_attachment = depth.as_ref().map_or(ptr::null(), |d| {
        d as *const RenderPassDepthStencilAttachment
    });
    let pass = encoder.begin_render_pass(&descriptor);
    drop(held);
    match pass {
        Some(pass) => Ok(make_render_pass(scope, pass)),
        None => Ok(Value::undefined()),
    }
}

fn encoder_begin_compute_pass(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    match encoder.begin_compute_pass() {
        Some(pass) => Ok(make_compute_pass(scope, pass)),
        None => Ok(Value::undefined()),
    }
}

fn encoder_copy_texture_to_texture(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    let (src, src_tex) = read_texture_copy(scope, &args[0]);
    let (dst, dst_tex) = read_texture_copy(scope, &args[1]);
    if !src.texture.is_null() && !dst.texture.is_null() {
        let size = read_extent(scope, &args[2]);
        encoder.copy_texture_to_texture(&src, &dst, &size);
    }
    drop((src_tex, dst_tex));
    Ok(Value::undefined())
}

fn encoder_copy_buffer_to_buffer(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 5 {
        return Ok(Value::undefined());
    }
    let (Some(src), Some(dst)) = (
        crate::gpu::buffer_of(&args[0]),
        crate::gpu::buffer_of(&args[2]),
    ) else {
        return Ok(Value::undefined());
    };
    let source_offset = i64_of(scope, &args[1]);
    let destination_offset = i64_of(scope, &args[3]);
    let size = i64_of(scope, &args[4]);
    if let (Some(s), Some(d)) = (crate::gpu::raw_buffer(&src), crate::gpu::raw_buffer(&dst)) {
        encoder.copy_buffer_to_buffer(
            s,
            source_offset as u64,
            d,
            destination_offset as u64,
            size as u64,
        );
    }
    Ok(Value::undefined())
}

fn encoder_resolve_query_set(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 5 {
        return Ok(Value::undefined());
    }
    let query_set = host::<Option<QuerySet>>(&args[0]);
    let dst = crate::gpu::buffer_of(&args[3]);
    let (Some(query_set), Some(dst)) = (query_set, dst) else {
        return Ok(Value::undefined());
    };
    let (Some(query_set), Some(buffer)) = (query_set.as_ref(), crate::gpu::raw_buffer(&dst)) else {
        return Ok(Value::undefined());
    };
    let first = u32_of(scope, &args[1]);
    let count = u32_of(scope, &args[2]);
    let offset = i64_of(scope, &args[4]);
    encoder.resolve_query_set(query_set, first, count, buffer, offset as u64);
    Ok(Value::undefined())
}

fn encoder_finish(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(encoder) = host::<CommandEncoder>(this) else {
        return Ok(Value::undefined());
    };
    match encoder.finish() {
        Some(command) => Ok(make_command_buffer(scope, command)),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn pass_end(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(pass) = host::<RenderPass>(this) {
        pass.end();
    }
    Ok(Value::undefined())
}

pub(crate) fn pass_noop(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn pass_set_pipeline(_: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let (Some(pass), Some(pipeline)) = (
        host::<RenderPass>(this),
        args.first().and_then(host::<RenderPipeline>),
    ) {
        pass.set_pipeline(&pipeline);
    }
    Ok(Value::undefined())
}

fn bound_range(scope: &mut Scope<'_>, args: &[Value], total: u64) -> (u64, u64) {
    let offset = args.get(2).map_or(0, |v| i64_of(scope, v));
    let mut size = total.saturating_sub(offset as u64);
    if let Some(v) = defined(args, 3) {
        let s = i64_of(scope, v);
        if s > 0 {
            size = s as u64;
        }
    }
    (offset as u64, size)
}

pub(crate) fn pass_set_vertex_buffer(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(pass) = host::<RenderPass>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let slot = i32_of(scope, &args[0]);
    let Some(b) = crate::gpu::buffer_of(&args[1]) else {
        return Ok(Value::undefined());
    };
    let Some(buffer) = crate::gpu::raw_buffer(&b) else {
        return Ok(Value::undefined());
    };
    let (offset, size) = bound_range(scope, args, b.size);
    pass.set_vertex_buffer(slot as u32, buffer, offset, size);
    Ok(Value::undefined())
}

pub(crate) fn pass_set_index_buffer(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(pass) = host::<RenderPass>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let Some(b) = crate::gpu::buffer_of(&args[0]) else {
        return Ok(Value::undefined());
    };
    let Some(buffer) = crate::gpu::raw_buffer(&b) else {
        return Ok(Value::undefined());
    };
    let format = if crate::util::string_of(scope, &args[1]).as_deref() == Some("uint16") {
        INDEX_FORMAT_UINT16
    } else {
        INDEX_FORMAT_UINT32
    };
    let (offset, size) = bound_range(scope, args, b.size);
    pass.set_index_buffer(buffer, format, offset, size);
    Ok(Value::undefined())
}

pub(crate) fn pass_set_bind_group(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(pass) = host::<RenderPass>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let index = u32_of(scope, &args[0]);
    let group = host::<BindGroup>(&args[1]);
    pass.set_bind_group(index, group.as_deref());
    Ok(Value::undefined())
}

pub(crate) fn pass_draw(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(pass) = host::<RenderPass>(this) else {
        return Ok(Value::undefined());
    };
    let vertices = args.first().map_or(0, |v| u32_of(scope, v));
    let instances = defined(args, 1).map_or(1, |v| u32_of(scope, v));
    let first = args.get(2).map_or(0, |v| u32_of(scope, v));
    let first_instance = args.get(3).map_or(0, |v| u32_of(scope, v));
    pass.draw(vertices, instances, first, first_instance);
    Ok(Value::undefined())
}

pub(crate) fn pass_draw_indexed(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(pass) = host::<RenderPass>(this) else {
        return Ok(Value::undefined());
    };
    let indices = args.first().map_or(0, |v| u32_of(scope, v));
    let instances = defined(args, 1).map_or(1, |v| u32_of(scope, v));
    let first = args.get(2).map_or(0, |v| u32_of(scope, v));
    let base_vertex = args.get(3).map_or(0, |v| i32_of(scope, v));
    let first_instance = args.get(4).map_or(0, |v| u32_of(scope, v));
    pass.draw_indexed(indices, instances, first, base_vertex, first_instance);
    Ok(Value::undefined())
}

pub(crate) fn compute_set_pipeline(_: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let (Some(pass), Some(pipeline)) = (
        host::<ComputePass>(this),
        args.first().and_then(host::<ComputePipeline>),
    ) {
        pass.set_pipeline(&pipeline);
    }
    Ok(Value::undefined())
}

pub(crate) fn compute_set_bind_group(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(pass) = host::<ComputePass>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let index = u32_of(scope, &args[0]);
    let group = host::<BindGroup>(&args[1]);
    pass.set_bind_group(index, group.as_deref());
    Ok(Value::undefined())
}

pub(crate) fn compute_dispatch(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(pass) = host::<ComputePass>(this) else {
        return Ok(Value::undefined());
    };
    let x = args.first().map_or(1, |v| u32_of(scope, v));
    let y = defined(args, 1).map_or(1, |v| u32_of(scope, v));
    let z = defined(args, 2).map_or(1, |v| u32_of(scope, v));
    pass.dispatch(x, y, z);
    Ok(Value::undefined())
}

pub(crate) fn compute_end(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(pass) = host::<ComputePass>(this) {
        pass.end();
    }
    Ok(Value::undefined())
}
