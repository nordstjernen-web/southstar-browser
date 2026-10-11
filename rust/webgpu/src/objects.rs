//! Southstar — the WebGPU host objects: what each wraps and how it is built with its methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use southstar_js_engine::{Scope, Value};

use crate::ffi::sys::Limits;
use crate::ffi::wgpu::{
    Adapter, BindGroupLayout, Buffer, CommandBuffer, ComputePass, Device, Queue, RenderPass,
    Texture, TextureView, Zeroed,
};
use crate::parse;
use crate::util::{bind, feature_set, set, set_str, uint};
use crate::{encode, gpu, pipeline};

pub(crate) struct DeviceState {
    pub(crate) queue: Queue,
    pub(crate) device: Device,
}

pub(crate) struct BufferState {
    pub(crate) buffer: Option<Buffer>,
    pub(crate) size: u64,
    pub(crate) device: Device,
    pub(crate) ranges: RefCell<Vec<Value>>,
    pub(crate) escaped: Cell<bool>,
}

impl Drop for BufferState {
    fn drop(&mut self) {
        if !self.ranges.get_mut().is_empty() {
            core::mem::forget(self.buffer.take());
        }
    }
}

pub(crate) struct TextureState {
    pub(crate) texture: Texture,
    pub(crate) format: u32,
}

pub(crate) fn limits_object(scope: &mut Scope<'_>, l: &Limits) -> Value {
    let o = scope.new_object();
    set(
        scope,
        &o,
        "maxTextureDimension1D",
        uint(l.max_texture_dimension1_d),
    );
    set(
        scope,
        &o,
        "maxTextureDimension2D",
        uint(l.max_texture_dimension2_d),
    );
    set(
        scope,
        &o,
        "maxTextureDimension3D",
        uint(l.max_texture_dimension3_d),
    );
    set(
        scope,
        &o,
        "maxTextureArrayLayers",
        uint(l.max_texture_array_layers),
    );
    set(scope, &o, "maxBindGroups", uint(l.max_bind_groups));
    set(
        scope,
        &o,
        "maxBindingsPerBindGroup",
        uint(l.max_bindings_per_bind_group),
    );
    let uniform = Value::number(l.max_uniform_buffer_binding_size as f64);
    set(scope, &o, "maxUniformBufferBindingSize", uniform);
    let storage = Value::number(l.max_storage_buffer_binding_size as f64);
    set(scope, &o, "maxStorageBufferBindingSize", storage);
    set(
        scope,
        &o,
        "maxBufferSize",
        Value::number(l.max_buffer_size as f64),
    );
    set(
        scope,
        &o,
        "maxComputeWorkgroupSizeX",
        uint(l.max_compute_workgroup_size_x),
    );
    set(
        scope,
        &o,
        "maxComputeWorkgroupSizeY",
        uint(l.max_compute_workgroup_size_y),
    );
    set(
        scope,
        &o,
        "maxComputeWorkgroupSizeZ",
        uint(l.max_compute_workgroup_size_z),
    );
    let invocations = uint(l.max_compute_invocations_per_workgroup);
    set(scope, &o, "maxComputeInvocationsPerWorkgroup", invocations);
    o
}

fn adapter_info(scope: &mut Scope<'_>, adapter: &Adapter) -> Value {
    let o = scope.new_object();
    if let Some(info) = adapter.info() {
        set_str(scope, &o, "vendor", &info.vendor);
        set_str(scope, &o, "architecture", &info.architecture);
        set_str(scope, &o, "device", &info.device);
        set_str(scope, &o, "description", &info.description);
    }
    o
}

pub(crate) fn make_adapter(scope: &mut Scope<'_>, adapter: Adapter) -> Value {
    let info = adapter_info(scope, &adapter);
    let limits = adapter.limits();
    let obj = scope.new_host_object(None, Rc::new(adapter));
    set(scope, &obj, "info", info);
    let features = feature_set(scope);
    set(scope, &obj, "features", features);
    let limits = limits_object(scope, &limits);
    set(scope, &obj, "limits", limits);
    set(scope, &obj, "isFallbackAdapter", Value::boolean(false));
    bind(scope, &obj, "requestDevice", gpu::adapter_request_device, 1);
    obj
}

pub(crate) fn make_queue(scope: &mut Scope<'_>, queue: Queue) -> Value {
    let obj = scope.new_host_object(None, Rc::new(queue));
    bind(scope, &obj, "writeBuffer", gpu::queue_write_buffer, 5);
    bind(
        scope,
        &obj,
        "writeTexture",
        pipeline::queue_write_texture,
        4,
    );
    bind(
        scope,
        &obj,
        "copyExternalImageToTexture",
        pipeline::queue_copy_external_image,
        3,
    );
    bind(scope, &obj, "submit", gpu::queue_submit, 1);
    set_str(scope, &obj, "label", "");
    obj
}

pub(crate) fn make_device(scope: &mut Scope<'_>, device: Device) -> Value {
    let Some(queue) = device.queue() else {
        return scope.new_host_object(None, ());
    };
    let state = DeviceState {
        queue: queue.clone(),
        device,
    };
    let obj = scope.new_host_object(None, Rc::new(state));
    let queue = make_queue(scope, queue);
    set(scope, &obj, "queue", queue);
    if let Ok((lost, _, _)) = scope.new_promise() {
        set(scope, &obj, "lost", lost);
    }
    let features = feature_set(scope);
    set(scope, &obj, "features", features);
    let limits = limits_object(scope, &Limits::zeroed());
    set(scope, &obj, "limits", limits);
    set_str(scope, &obj, "label", "");
    bind(scope, &obj, "createBuffer", gpu::device_create_buffer, 1);
    bind(
        scope,
        &obj,
        "createCommandEncoder",
        encode::device_create_command_encoder,
        1,
    );
    bind(
        scope,
        &obj,
        "createShaderModule",
        pipeline::device_create_shader_module,
        1,
    );
    bind(
        scope,
        &obj,
        "createRenderPipeline",
        pipeline::device_create_render_pipeline,
        1,
    );
    bind(
        scope,
        &obj,
        "createComputePipeline",
        pipeline::device_create_compute_pipeline,
        1,
    );
    bind(
        scope,
        &obj,
        "createBindGroupLayout",
        pipeline::device_create_bind_group_layout,
        1,
    );
    bind(
        scope,
        &obj,
        "createPipelineLayout",
        pipeline::device_create_pipeline_layout,
        1,
    );
    bind(
        scope,
        &obj,
        "createBindGroup",
        pipeline::device_create_bind_group,
        1,
    );
    bind(
        scope,
        &obj,
        "createSampler",
        pipeline::device_create_sampler,
        1,
    );
    bind(
        scope,
        &obj,
        "createTexture",
        pipeline::device_create_texture,
        1,
    );
    bind(
        scope,
        &obj,
        "createQuerySet",
        pipeline::device_create_query_set,
        1,
    );
    bind(
        scope,
        &obj,
        "pushErrorScope",
        gpu::device_push_error_scope,
        1,
    );
    bind(scope, &obj, "popErrorScope", gpu::device_pop_error_scope, 0);
    bind(scope, &obj, "getQueue", gpu::device_get_queue, 0);
    bind(scope, &obj, "destroy", gpu::device_destroy, 0);
    obj
}

pub(crate) fn make_buffer(scope: &mut Scope<'_>, state: BufferState, usage: u32) -> Value {
    let size = state.size;
    let obj = scope.new_host_object(None, Rc::new(state));
    bind(scope, &obj, "destroy", gpu::buffer_destroy, 0);
    bind(
        scope,
        &obj,
        "getMappedRange",
        gpu::buffer_get_mapped_range,
        2,
    );
    bind(scope, &obj, "unmap", gpu::buffer_unmap, 0);
    bind(scope, &obj, "mapAsync", gpu::buffer_map_async, 3);
    set(scope, &obj, "size", Value::number(size as f64));
    set(scope, &obj, "usage", uint(usage));
    set_str(scope, &obj, "label", "");
    obj
}

pub(crate) fn make_texture(
    scope: &mut Scope<'_>,
    texture: Texture,
    w: u32,
    h: u32,
    format: u32,
) -> Value {
    let obj = scope.new_host_object(None, Rc::new(TextureState { texture, format }));
    bind(scope, &obj, "createView", pipeline::texture_create_view, 1);
    bind(scope, &obj, "destroy", pipeline::texture_destroy, 0);
    set(scope, &obj, "width", uint(w));
    set(scope, &obj, "height", uint(h));
    set(scope, &obj, "depthOrArrayLayers", Value::int(1));
    set_str(scope, &obj, "format", parse::format_name(format));
    obj
}

pub(crate) fn make_view(scope: &mut Scope<'_>, view: TextureView) -> Value {
    scope.new_host_object(None, Rc::new(view))
}

pub(crate) fn make_bind_group_layout(scope: &mut Scope<'_>, layout: BindGroupLayout) -> Value {
    scope.new_host_object(None, Rc::new(layout))
}

pub(crate) fn make_command_buffer(scope: &mut Scope<'_>, command: CommandBuffer) -> Value {
    scope.new_host_object(None, Rc::new(command))
}

pub(crate) fn make_render_pass(scope: &mut Scope<'_>, pass: RenderPass) -> Value {
    let obj = scope.new_host_object(None, Rc::new(pass));
    bind(scope, &obj, "end", encode::pass_end, 0);
    bind(scope, &obj, "setPipeline", encode::pass_set_pipeline, 1);
    bind(scope, &obj, "setBindGroup", encode::pass_set_bind_group, 2);
    bind(
        scope,
        &obj,
        "setVertexBuffer",
        encode::pass_set_vertex_buffer,
        2,
    );
    bind(
        scope,
        &obj,
        "setIndexBuffer",
        encode::pass_set_index_buffer,
        2,
    );
    bind(scope, &obj, "setViewport", encode::pass_noop, 6);
    bind(scope, &obj, "setScissorRect", encode::pass_noop, 4);
    bind(scope, &obj, "draw", encode::pass_draw, 4);
    bind(scope, &obj, "drawIndexed", encode::pass_draw_indexed, 5);
    obj
}

pub(crate) fn make_compute_pass(scope: &mut Scope<'_>, pass: ComputePass) -> Value {
    let obj = scope.new_host_object(None, Rc::new(pass));
    bind(scope, &obj, "setPipeline", encode::compute_set_pipeline, 1);
    bind(
        scope,
        &obj,
        "setBindGroup",
        encode::compute_set_bind_group,
        2,
    );
    bind(
        scope,
        &obj,
        "dispatchWorkgroups",
        encode::compute_dispatch,
        3,
    );
    bind(scope, &obj, "end", encode::compute_end, 0);
    obj
}
