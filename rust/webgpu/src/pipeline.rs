//! Southstar — shader modules, pipelines, layouts, bind groups, samplers, textures and query sets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;
use std::rc::Rc;

use southstar_js_engine::{Scope, Value};

use crate::ffi::host::{drawimage_source, internal_error};
use crate::ffi::sys::*;
use crate::ffi::wgpu::{
    BindGroupLayout, ComputePipeline, PipelineLayout, QuerySet, Queue, RenderPipeline, Sampler,
    ShaderModule, TextureView, Zeroed, string_view,
};
use crate::objects::{DeviceState, TextureState, make_bind_group_layout, make_texture, make_view};
use crate::parse;
use crate::util::{
    JsResult, arg, array_len, bind, defined_u32_prop, host, i64_prop, index, prop, resolved, set,
    str_prop, u32_of, u32_prop, uint,
};

const MAX_VERTEX_BUFFERS: usize = 16;
const MAX_ATTRIBUTES: usize = 16;
const MAX_TARGETS: usize = 8;
const MAX_ENTRIES: usize = 32;

fn device_and_descriptor(this: &Value, args: &[Value]) -> Option<(Rc<DeviceState>, Value)> {
    let device = host::<DeviceState>(this)?;
    let desc = arg(args, 0);
    desc.is_object().then_some((device, desc))
}

fn shader_compilation_info(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let info = scope.new_object();
    let messages = scope.new_array();
    set(scope, &info, "messages", messages);
    resolved(scope, info)
}

pub(crate) fn device_create_shader_module(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createShaderModule: descriptor required"));
    };
    let Some(code) = str_prop(scope, &desc, "code") else {
        return Err(scope.type_error("createShaderModule: code required"));
    };
    let Some(module) = d.device.create_shader_module(&code) else {
        return Err(internal_error(scope, "createShaderModule failed"));
    };
    let obj = scope.new_host_object(None, Rc::new(module));
    bind(
        scope,
        &obj,
        "getCompilationInfo",
        shader_compilation_info,
        0,
    );
    Ok(obj)
}

fn shader_of(value: &Value) -> Option<Rc<ShaderModule>> {
    host::<ShaderModule>(value)
}

fn pipeline_layout_of(value: &Value) -> Option<Rc<PipelineLayout>> {
    host::<PipelineLayout>(value)
}

fn blend_component(scope: &mut Scope<'_>, value: &Value) -> BlendComponent {
    if !value.is_object() {
        return BlendComponent {
            operation: BLEND_OPERATION_ADD,
            src_factor: BLEND_FACTOR_ONE,
            dst_factor: BLEND_FACTOR_ZERO,
        };
    }
    let operation = str_prop(scope, value, "operation");
    let src = str_prop(scope, value, "srcFactor");
    let dst = str_prop(scope, value, "dstFactor");
    BlendComponent {
        operation: parse::blend_operation(operation.as_deref()),
        src_factor: parse::blend_factor(src.as_deref()),
        dst_factor: if dst.is_some() {
            parse::blend_factor(dst.as_deref())
        } else {
            BLEND_FACTOR_ZERO
        },
    }
}

struct VertexInput {
    layouts: Vec<VertexBufferLayout>,
    attributes: Vec<[VertexAttribute; MAX_ATTRIBUTES]>,
}

fn vertex_buffers(scope: &mut Scope<'_>, list: &Value) -> VertexInput {
    let count = (array_len(scope, list) as usize).min(MAX_VERTEX_BUFFERS);
    let mut input = VertexInput {
        layouts: vec![VertexBufferLayout::zeroed(); count],
        attributes: vec![[VertexAttribute::zeroed(); MAX_ATTRIBUTES]; count],
    };
    for i in 0..count {
        let jb = index(scope, list, i as u32);
        if !jb.is_object() {
            continue;
        }
        let stride = i64_prop(scope, &jb, "arrayStride");
        input.layouts[i].array_stride = stride as u64;
        input.layouts[i].step_mode =
            if str_prop(scope, &jb, "stepMode").as_deref() == Some("instance") {
                VERTEX_STEP_MODE_INSTANCE
            } else {
                VERTEX_STEP_MODE_VERTEX
            };
        let attrs = prop(scope, &jb, "attributes");
        let mut na = 0;
        if attrs.is_array() {
            na = (array_len(scope, &attrs) as usize).min(MAX_ATTRIBUTES);
            for k in 0..na {
                let ja = index(scope, &attrs, k as u32);
                let format = str_prop(scope, &ja, "format");
                let attr = &mut input.attributes[i][k];
                attr.format = parse::vertex_format(format.as_deref());
                let offset = i64_prop(scope, &ja, "offset");
                let location = u32_prop(scope, &ja, "shaderLocation");
                let attr = &mut input.attributes[i][k];
                attr.offset = offset as u64;
                attr.shader_location = location;
            }
        }
        input.layouts[i].attribute_count = na;
    }
    for (layout, attributes) in input.layouts.iter_mut().zip(&input.attributes) {
        layout.attributes = attributes.as_ptr();
    }
    input
}

struct Fragment {
    targets: Vec<ColorTargetState>,
    blends: Vec<BlendState>,
    blended: Vec<bool>,
}

fn fragment_targets(scope: &mut Scope<'_>, fragment: &Value) -> Fragment {
    let list = prop(scope, fragment, "targets");
    let mut out = Fragment {
        targets: Vec::new(),
        blends: Vec::new(),
        blended: Vec::new(),
    };
    if !list.is_array() {
        return out;
    }
    let count = (array_len(scope, &list) as usize).min(MAX_TARGETS);
    out.targets = vec![ColorTargetState::zeroed(); count];
    out.blends = vec![BlendState::zeroed(); count];
    out.blended = vec![false; count];
    for i in 0..count {
        let target = index(scope, &list, i as u32);
        let format = str_prop(scope, &target, "format");
        out.targets[i].format = parse::texture_format(format.as_deref());
        out.targets[i].write_mask = COLOR_WRITE_MASK_ALL;
        if let Some(mask) = defined_u32_prop(scope, &target, "writeMask") {
            out.targets[i].write_mask = u64::from(mask);
        }
        let blend = prop(scope, &target, "blend");
        if blend.is_object() {
            let color = prop(scope, &blend, "color");
            out.blends[i].color = blend_component(scope, &color);
            let alpha = prop(scope, &blend, "alpha");
            out.blends[i].alpha = blend_component(scope, &alpha);
            out.blended[i] = true;
        }
    }
    for ((target, blend), blended) in out.targets.iter_mut().zip(&out.blends).zip(&out.blended) {
        if *blended {
            target.blend = blend;
        }
    }
    out
}

pub(crate) fn device_create_render_pipeline(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createRenderPipeline: descriptor required"));
    };
    let mut descriptor = RenderPipelineDescriptor::zeroed();
    descriptor.primitive.topology = PRIMITIVE_TOPOLOGY_TRIANGLE_LIST;
    descriptor.multisample.count = 1;
    descriptor.multisample.mask = 0xFFFF_FFFF;

    let layout = pipeline_layout_of(&prop(scope, &desc, "layout"));
    if let Some(layout) = &layout {
        descriptor.layout = layout.raw();
    }

    let vertex = prop(scope, &desc, "vertex");
    let mut vertex_module = None;
    let mut vertex_entry = None;
    let mut vertex_input = None;
    if vertex.is_object() {
        vertex_module = shader_of(&prop(scope, &vertex, "module"));
        if let Some(module) = &vertex_module {
            descriptor.vertex.module = module.raw();
        }
        vertex_entry = str_prop(scope, &vertex, "entryPoint");
        descriptor.vertex.entry_point = string_view(vertex_entry.as_deref());
        let buffers = prop(scope, &vertex, "buffers");
        if buffers.is_array() {
            let input = vertex_buffers(scope, &buffers);
            descriptor.vertex.buffer_count = input.layouts.len();
            descriptor.vertex.buffers = input.layouts.as_ptr();
            vertex_input = Some(input);
        }
    }

    let primitive = prop(scope, &desc, "primitive");
    if primitive.is_object() {
        let topology = str_prop(scope, &primitive, "topology");
        descriptor.primitive.topology = parse::topology(topology.as_deref());
        match str_prop(scope, &primitive, "cullMode").as_deref() {
            Some("back") => descriptor.primitive.cull_mode = CULL_MODE_BACK,
            Some("front") => descriptor.primitive.cull_mode = CULL_MODE_FRONT,
            _ => {}
        }
    }

    let mut depth_stencil = DepthStencilState::zeroed();
    let ds = prop(scope, &desc, "depthStencil");
    if ds.is_object() {
        let format = str_prop(scope, &ds, "format");
        depth_stencil.format = parse::texture_format(format.as_deref());
        let write = prop(scope, &ds, "depthWriteEnabled");
        depth_stencil.depth_write_enabled = if scope.to_bool(&write) {
            OPTIONAL_BOOL_TRUE
        } else {
            OPTIONAL_BOOL_FALSE
        };
        let compare = str_prop(scope, &ds, "depthCompare");
        depth_stencil.depth_compare = parse::compare_function(compare.as_deref());
        if depth_stencil.depth_compare == COMPARE_FUNCTION_UNDEFINED {
            depth_stencil.depth_compare = COMPARE_FUNCTION_ALWAYS;
        }
        depth_stencil.stencil_front.compare = COMPARE_FUNCTION_ALWAYS;
        depth_stencil.stencil_back.compare = COMPARE_FUNCTION_ALWAYS;
        descriptor.depth_stencil = &depth_stencil;
    }

    let multisample = prop(scope, &desc, "multisample");
    if multisample.is_object()
        && let Some(count) = defined_u32_prop(scope, &multisample, "count")
        && count >= 1
    {
        descriptor.multisample.count = count;
    }

    let fragment_value = prop(scope, &desc, "fragment");
    let mut fragment = FragmentState::zeroed();
    let mut fragment_module = None;
    let mut fragment_entry = None;
    let mut targets = None;
    if fragment_value.is_object() {
        fragment_module = shader_of(&prop(scope, &fragment_value, "module"));
        if let Some(module) = &fragment_module {
            fragment.module = module.raw();
        }
        fragment_entry = str_prop(scope, &fragment_value, "entryPoint");
        fragment.entry_point = string_view(fragment_entry.as_deref());
        let out = fragment_targets(scope, &fragment_value);
        fragment.target_count = out.targets.len();
        fragment.targets = if out.targets.is_empty() {
            ptr::null()
        } else {
            out.targets.as_ptr()
        };
        targets = Some(out);
        descriptor.fragment = &fragment;
    }

    let pipeline = d.device.create_render_pipeline(&descriptor);
    drop((
        layout,
        vertex_module,
        fragment_module,
        vertex_input,
        targets,
    ));
    drop((vertex_entry, fragment_entry));
    let Some(pipeline) = pipeline else {
        return Err(internal_error(scope, "createRenderPipeline failed"));
    };
    let obj = scope.new_host_object(None, Rc::new(pipeline));
    bind(
        scope,
        &obj,
        "getBindGroupLayout",
        render_pipeline_bind_group_layout,
        1,
    );
    Ok(obj)
}

fn render_pipeline_bind_group_layout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(pipeline) = host::<RenderPipeline>(this) else {
        return Ok(Value::undefined());
    };
    let Some(index) = args.first() else {
        return Ok(Value::undefined());
    };
    let index = u32_of(scope, index);
    match pipeline.bind_group_layout(index) {
        Some(layout) => Ok(make_bind_group_layout(scope, layout)),
        None => Ok(Value::undefined()),
    }
}

fn compute_pipeline_bind_group_layout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(pipeline) = host::<ComputePipeline>(this) else {
        return Ok(Value::undefined());
    };
    let Some(index) = args.first() else {
        return Ok(Value::undefined());
    };
    let index = u32_of(scope, index);
    match pipeline.bind_group_layout(index) {
        Some(layout) => Ok(make_bind_group_layout(scope, layout)),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn device_create_compute_pipeline(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createComputePipeline: descriptor"));
    };
    let mut descriptor = ComputePipelineDescriptor::zeroed();
    let layout = pipeline_layout_of(&prop(scope, &desc, "layout"));
    if let Some(layout) = &layout {
        descriptor.layout = layout.raw();
    }
    let compute = prop(scope, &desc, "compute");
    let mut module = None;
    let mut entry = None;
    if compute.is_object() {
        module = shader_of(&prop(scope, &compute, "module"));
        if let Some(module) = &module {
            descriptor.compute.module = module.raw();
        }
        entry = str_prop(scope, &compute, "entryPoint");
        descriptor.compute.entry_point = string_view(entry.as_deref());
    }
    let pipeline = d.device.create_compute_pipeline(&descriptor);
    drop((layout, module, entry));
    let Some(pipeline) = pipeline else {
        return Err(internal_error(scope, "createComputePipeline failed"));
    };
    let obj = scope.new_host_object(None, Rc::new(pipeline));
    bind(
        scope,
        &obj,
        "getBindGroupLayout",
        compute_pipeline_bind_group_layout,
        1,
    );
    Ok(obj)
}

pub(crate) fn device_create_bind_group_layout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createBindGroupLayout: descriptor"));
    };
    let mut entries = [BindGroupLayoutEntry::zeroed(); MAX_ENTRIES];
    let mut n = 0;
    let list = prop(scope, &desc, "entries");
    if list.is_array() {
        n = (array_len(scope, &list) as usize).min(MAX_ENTRIES);
        for (i, entry) in entries.iter_mut().enumerate().take(n) {
            let e = index(scope, &list, i as u32);
            entry.binding = u32_prop(scope, &e, "binding");
            entry.visibility = u64::from(u32_prop(scope, &e, "visibility"));
            let buffer = prop(scope, &e, "buffer");
            let sampler = prop(scope, &e, "sampler");
            let texture = prop(scope, &e, "texture");
            if buffer.is_object() {
                let kind = str_prop(scope, &buffer, "type");
                entry.buffer.kind = parse::buffer_binding_type(kind.as_deref());
            } else if sampler.is_object() {
                let kind = str_prop(scope, &sampler, "type");
                entry.sampler.kind = parse::sampler_binding_type(kind.as_deref());
            } else if texture.is_object() {
                let kind = str_prop(scope, &texture, "sampleType");
                entry.texture.sample_type = parse::sample_type(kind.as_deref());
                entry.texture.view_dimension = TEXTURE_VIEW_DIMENSION_2_D;
            }
        }
    }
    let mut descriptor = BindGroupLayoutDescriptor::zeroed();
    descriptor.entry_count = n;
    descriptor.entries = entries.as_ptr();
    let Some(layout) = d.device.create_bind_group_layout(&descriptor) else {
        return Err(internal_error(scope, "createBindGroupLayout failed"));
    };
    Ok(make_bind_group_layout(scope, layout))
}

pub(crate) fn device_create_pipeline_layout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createPipelineLayout: descriptor"));
    };
    let mut layouts = [ptr::null_mut(); MAX_ENTRIES];
    let mut held = Vec::new();
    let mut n = 0;
    let list = prop(scope, &desc, "bindGroupLayouts");
    if list.is_array() {
        n = (array_len(scope, &list) as usize).min(MAX_ENTRIES);
        for (i, slot) in layouts.iter_mut().enumerate().take(n) {
            let e = index(scope, &list, i as u32);
            if let Some(layout) = host::<BindGroupLayout>(&e) {
                *slot = layout.raw();
                held.push(layout);
            }
        }
    }
    let mut descriptor = PipelineLayoutDescriptor::zeroed();
    descriptor.bind_group_layout_count = n;
    descriptor.bind_group_layouts = layouts.as_ptr();
    let layout = d.device.create_pipeline_layout(&descriptor);
    drop(held);
    let Some(layout) = layout else {
        return Err(internal_error(scope, "createPipelineLayout failed"));
    };
    Ok(scope.new_host_object(None, Rc::new(layout)))
}

pub(crate) fn device_create_bind_group(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createBindGroup: descriptor"));
    };
    let Some(layout) = host::<BindGroupLayout>(&prop(scope, &desc, "layout")) else {
        return Err(scope.type_error("createBindGroup: layout"));
    };
    let mut entries = [BindGroupEntry::zeroed(); MAX_ENTRIES];
    let mut views = Vec::new();
    let mut samplers = Vec::new();
    let mut buffers = Vec::new();
    let mut n = 0;
    let list = prop(scope, &desc, "entries");
    if list.is_array() {
        n = (array_len(scope, &list) as usize).min(MAX_ENTRIES);
        for (i, entry) in entries.iter_mut().enumerate().take(n) {
            let e = index(scope, &list, i as u32);
            entry.binding = u32_prop(scope, &e, "binding");
            let resource = prop(scope, &e, "resource");
            if let Some(view) = host::<TextureView>(&resource) {
                entry.texture_view = view.raw();
                views.push(view);
            } else if let Some(sampler) = host::<Sampler>(&resource) {
                entry.sampler = sampler.raw();
                samplers.push(sampler);
            } else if resource.is_object()
                && let Some(b) = crate::gpu::buffer_of(&prop(scope, &resource, "buffer"))
            {
                entry.buffer = crate::gpu::raw_buffer(&b).map_or(ptr::null_mut(), |b| b.raw());
                let mut offset = 0;
                let mut size = b.size as i64;
                let jo = prop(scope, &resource, "offset");
                if !jo.is_undefined() {
                    offset = crate::util::i64_of(scope, &jo);
                }
                let js = prop(scope, &resource, "size");
                if !js.is_undefined() {
                    size = crate::util::i64_of(scope, &js);
                }
                entry.offset = offset as u64;
                entry.size = size as u64;
                buffers.push(b);
            }
        }
    }
    let mut descriptor = BindGroupDescriptor::zeroed();
    descriptor.layout = layout.raw();
    descriptor.entry_count = n;
    descriptor.entries = entries.as_ptr();
    let group = d.device.create_bind_group(&descriptor);
    drop((views, samplers, buffers, layout));
    let Some(group) = group else {
        return Err(internal_error(scope, "createBindGroup failed"));
    };
    Ok(scope.new_host_object(None, Rc::new(group)))
}

pub(crate) fn device_create_sampler(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(d) = host::<DeviceState>(this) else {
        return Ok(Value::undefined());
    };
    let mut descriptor = SamplerDescriptor::zeroed();
    descriptor.address_mode_u = ADDRESS_MODE_CLAMP_TO_EDGE;
    descriptor.address_mode_v = ADDRESS_MODE_CLAMP_TO_EDGE;
    descriptor.address_mode_w = ADDRESS_MODE_CLAMP_TO_EDGE;
    descriptor.mag_filter = FILTER_MODE_NEAREST;
    descriptor.min_filter = FILTER_MODE_NEAREST;
    descriptor.mipmap_filter = MIPMAP_FILTER_MODE_NEAREST;
    descriptor.lod_min_clamp = 0.0;
    descriptor.lod_max_clamp = 32.0;
    descriptor.max_anisotropy = 1;
    let desc = arg(args, 0);
    if desc.is_object() {
        if let Some(mode) = str_prop(scope, &desc, "addressModeU") {
            descriptor.address_mode_u = parse::address_mode(&mode);
        }
        if let Some(mode) = str_prop(scope, &desc, "addressModeV") {
            descriptor.address_mode_v = parse::address_mode(&mode);
        }
        if let Some(mode) = str_prop(scope, &desc, "addressModeW") {
            descriptor.address_mode_w = parse::address_mode(&mode);
        }
        if str_prop(scope, &desc, "magFilter").as_deref() == Some("linear") {
            descriptor.mag_filter = FILTER_MODE_LINEAR;
        }
        if str_prop(scope, &desc, "minFilter").as_deref() == Some("linear") {
            descriptor.min_filter = FILTER_MODE_LINEAR;
        }
        if str_prop(scope, &desc, "mipmapFilter").as_deref() == Some("linear") {
            descriptor.mipmap_filter = MIPMAP_FILTER_MODE_LINEAR;
        }
        if let Some(compare) = str_prop(scope, &desc, "compare") {
            descriptor.compare = parse::compare_function(Some(&compare));
        }
        if let Some(a) = defined_u32_prop(scope, &desc, "maxAnisotropy") {
            descriptor.max_anisotropy = a.max(1) as u16;
        }
    }
    let Some(sampler) = d.device.create_sampler(&descriptor) else {
        return Err(internal_error(scope, "createSampler failed"));
    };
    Ok(scope.new_host_object(None, Rc::new(sampler)))
}

pub(crate) fn read_extent(scope: &mut Scope<'_>, value: &Value) -> Extent3D {
    let mut out = Extent3D {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    if value.is_array() {
        let n = array_len(scope, value);
        let mut vals = [1u32; 3];
        for (i, v) in vals.iter_mut().enumerate().take(n.min(3) as usize) {
            let e = index(scope, value, i as u32);
            *v = u32_of(scope, &e);
        }
        out.width = vals[0];
        out.height = vals[1];
        out.depth_or_array_layers = vals[2];
    } else if value.is_object() {
        out.width = u32_prop(scope, value, "width");
        if let Some(h) = defined_u32_prop(scope, value, "height") {
            out.height = h;
        }
        if let Some(d) = defined_u32_prop(scope, value, "depthOrArrayLayers") {
            out.depth_or_array_layers = d;
        }
    }
    out
}

pub(crate) fn read_texture_copy(
    scope: &mut Scope<'_>,
    value: &Value,
) -> (TexelCopyTextureInfo, Option<Rc<TextureState>>) {
    let mut out = TexelCopyTextureInfo::zeroed();
    out.aspect = TEXTURE_ASPECT_ALL;
    if !value.is_object() {
        return (out, None);
    }
    let texture = host::<TextureState>(&prop(scope, value, "texture"));
    if let Some(t) = &texture {
        out.texture = t.texture.raw();
    }
    if let Some(mip) = defined_u32_prop(scope, value, "mipLevel") {
        out.mip_level = mip;
    }
    let origin = prop(scope, value, "origin");
    if origin.is_object() {
        let e = read_extent(scope, &origin);
        out.origin.x = e.width;
        out.origin.y = e.height;
        out.origin.z = 0;
        if let Some(z) = defined_u32_prop(scope, &origin, "z") {
            out.origin.z = z;
        }
    }
    (out, texture)
}

pub(crate) fn device_create_texture(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Err(scope.type_error("createTexture: descriptor required"));
    };
    let mut descriptor = TextureDescriptor::zeroed();
    descriptor.dimension = TEXTURE_DIMENSION_2_D;
    descriptor.mip_level_count = 1;
    descriptor.sample_count = 1;
    let size = prop(scope, &desc, "size");
    descriptor.size = read_extent(scope, &size);
    descriptor.usage = u64::from(u32_prop(scope, &desc, "usage"));
    let format = str_prop(scope, &desc, "format");
    descriptor.format = parse::texture_format(format.as_deref());
    if let Some(m) = defined_u32_prop(scope, &desc, "mipLevelCount") {
        descriptor.mip_level_count = if m != 0 { m } else { 1 };
    }
    if let Some(m) = defined_u32_prop(scope, &desc, "sampleCount") {
        descriptor.sample_count = if m != 0 { m } else { 1 };
    }
    match str_prop(scope, &desc, "dimension").as_deref() {
        Some("3d") => descriptor.dimension = TEXTURE_DIMENSION_3_D,
        Some("1d") => descriptor.dimension = TEXTURE_DIMENSION_1_D,
        _ => {}
    }
    let Some(texture) = d.device.create_texture(&descriptor) else {
        return Err(internal_error(scope, "createTexture failed"));
    };
    Ok(make_texture(
        scope,
        texture,
        descriptor.size.width,
        descriptor.size.height,
        descriptor.format,
    ))
}

pub(crate) fn texture_create_view(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(t) = host::<TextureState>(this) else {
        return Ok(Value::undefined());
    };
    let desc = arg(args, 0);
    let view = if desc.is_object() {
        let mut descriptor = TextureViewDescriptor::zeroed();
        descriptor.mip_level_count = UNDEFINED_U32;
        descriptor.array_layer_count = UNDEFINED_U32;
        descriptor.aspect = TEXTURE_ASPECT_ALL;
        if let Some(format) = str_prop(scope, &desc, "format") {
            descriptor.format = parse::texture_format(Some(&format));
        }
        if let Some(dimension) = str_prop(scope, &desc, "dimension") {
            descriptor.dimension = parse::view_dimension(&dimension);
        }
        match str_prop(scope, &desc, "aspect").as_deref() {
            Some("depth-only") => descriptor.aspect = TEXTURE_ASPECT_DEPTH_ONLY,
            Some("stencil-only") => descriptor.aspect = TEXTURE_ASPECT_STENCIL_ONLY,
            _ => {}
        }
        if let Some(v) = defined_u32_prop(scope, &desc, "baseMipLevel") {
            descriptor.base_mip_level = v;
        }
        if let Some(v) = defined_u32_prop(scope, &desc, "mipLevelCount") {
            descriptor.mip_level_count = v;
        }
        if let Some(v) = defined_u32_prop(scope, &desc, "baseArrayLayer") {
            descriptor.base_array_layer = v;
        }
        if let Some(v) = defined_u32_prop(scope, &desc, "arrayLayerCount") {
            descriptor.array_layer_count = v;
        }
        t.texture.create_view(Some(&descriptor))
    } else {
        t.texture.create_view(None)
    };
    match view {
        Some(view) => Ok(make_view(scope, view)),
        None => Ok(Value::undefined()),
    }
}

pub(crate) fn texture_destroy(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(t) = host::<TextureState>(this) {
        t.texture.destroy();
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_write_texture(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(queue) = host::<Queue>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 4 || !args[0].is_object() {
        return Ok(Value::undefined());
    }
    let Some(texture) = host::<TextureState>(&prop(scope, &args[0], "texture")) else {
        return Ok(Value::undefined());
    };
    let mut destination = TexelCopyTextureInfo::zeroed();
    destination.texture = texture.texture.raw();
    destination.aspect = TEXTURE_ASPECT_ALL;
    let mut layout = TexelCopyBufferLayout::zeroed();
    if args[2].is_object() {
        if let Some(b) = defined_u32_prop(scope, &args[2], "bytesPerRow") {
            layout.bytes_per_row = b;
        }
        if let Some(r) = defined_u32_prop(scope, &args[2], "rowsPerImage") {
            layout.rows_per_image = r;
        }
    }
    let size = read_extent(scope, &args[3]);
    scope.with_buffer_bytes_mut(&args[1], |bytes| {
        queue.write_texture(&destination, bytes, &layout, &size);
    });
    Ok(Value::undefined())
}

fn unpremultiply(c: u32, a: u32) -> u32 {
    ((c * 255 + a / 2) / a).min(255)
}

pub(crate) fn queue_copy_external_image(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(queue) = host::<Queue>(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 3 || !args[0].is_object() || !args[1].is_object() {
        return Ok(Value::undefined());
    }
    let source = prop(scope, &args[0], "source");
    let flip = prop(scope, &args[0], "flipY");
    let flip_y = scope.to_bool(&flip);
    let Some(image) = drawimage_source(scope, &source)? else {
        return Ok(Value::undefined());
    };
    let (w, h) = (image.w, image.h);
    if w <= 0 || h <= 0 {
        return Ok(Value::undefined());
    }
    let (destination, texture) = read_texture_copy(scope, &args[1]);
    if destination.texture.is_null() {
        return Ok(Value::undefined());
    }
    let to_rgba = texture.as_ref().is_some_and(|t| {
        t.format == TEXTURE_FORMAT_RGBA8_UNORM || t.format == TEXTURE_FORMAT_RGBA8_UNORM_SRGB
    });
    let premultiplied = prop(scope, &args[0], "premultipliedAlpha");
    let premultiply = scope.to_bool(&premultiplied);
    let rows = image.with_pixels(|data, stride| {
        source_rows(data, stride, (w, h), flip_y, premultiply, to_rgba)
    });
    let Some(out) = rows.flatten() else {
        return Ok(Value::undefined());
    };
    let mut layout = TexelCopyBufferLayout::zeroed();
    layout.bytes_per_row = (w as u32) * 4;
    layout.rows_per_image = h as u32;
    let size = Extent3D {
        width: w as u32,
        height: h as u32,
        depth_or_array_layers: 1,
    };
    queue.write_texture(&destination, &out, &layout, &size);
    drop(texture);
    Ok(Value::undefined())
}

fn source_rows(
    data: &[u8],
    stride: usize,
    (w, h): (i32, i32),
    flip_y: bool,
    premultiply: bool,
    to_rgba: bool,
) -> Option<Vec<u8>> {
    let (wu, hu) = (w as usize, h as usize);
    let bpr = wu * 4;
    if stride < bpr || data.len() < (hu - 1) * stride + bpr {
        return None;
    }
    let mut out = vec![0u8; bpr * hu];
    for y in 0..hu {
        let src_row = if flip_y { hu - 1 - y } else { y };
        let srow = &data[src_row * stride..];
        let orow = &mut out[y * bpr..(y + 1) * bpr];
        for (x, o) in orow.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let p = &srow[x * 4..x * 4 + 4];
            let (mut b, mut g, mut r) = (u32::from(p[0]), u32::from(p[1]), u32::from(p[2]));
            let a = u32::from(p[3]);
            if !premultiply && a > 0 && a < 255 {
                r = unpremultiply(r, a);
                g = unpremultiply(g, a);
                b = unpremultiply(b, a);
            }
            if to_rgba {
                o[0] = r as u8;
                o[1] = g as u8;
                o[2] = b as u8;
            } else {
                o[0] = b as u8;
                o[1] = g as u8;
                o[2] = r as u8;
            }
            o[3] = a as u8;
        }
    }
    Some(out)
}

fn query_set_destroy(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    if let Some(query_set) = host::<Option<QuerySet>>(this)
        && let Some(query_set) = query_set.as_ref()
    {
        query_set.destroy();
    }
    Ok(Value::undefined())
}

pub(crate) fn device_create_query_set(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some((d, desc)) = device_and_descriptor(this, args) else {
        return Ok(Value::undefined());
    };
    let kind = str_prop(scope, &desc, "type");
    let count = u32_prop(scope, &desc, "count");
    let query_set = if kind.as_deref() == Some("occlusion") {
        let mut descriptor = QuerySetDescriptor::zeroed();
        descriptor.kind = QUERY_TYPE_OCCLUSION;
        descriptor.count = count;
        d.device.create_query_set(&descriptor)
    } else {
        None
    };
    let obj = scope.new_host_object(None, Rc::new(query_set));
    bind(scope, &obj, "destroy", query_set_destroy, 0);
    set(scope, &obj, "count", uint(count));
    Ok(obj)
}
