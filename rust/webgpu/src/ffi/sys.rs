//! Southstar — the webgpu.h and wgpu.h types and constants WebGPU uses, declared against the vendored headers with their layouts checked.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};

pub type MapCallback = unsafe extern "C" fn(u32, StringView, *mut c_void, *mut c_void);
pub type RequestCallback =
    unsafe extern "C" fn(u32, *mut c_void, StringView, *mut c_void, *mut c_void);
pub type DeviceEventCallback =
    unsafe extern "C" fn(*const *mut c_void, u32, StringView, *mut c_void, *mut c_void);

pub const UNDEFINED_U32: u32 = u32::MAX;

pub const ADDRESS_MODE_CLAMP_TO_EDGE: u32 = 1;
pub const ADDRESS_MODE_MIRROR_REPEAT: u32 = 3;
pub const ADDRESS_MODE_REPEAT: u32 = 2;
pub const BLEND_FACTOR_CONSTANT: u32 = 12;
pub const BLEND_FACTOR_DST: u32 = 7;
pub const BLEND_FACTOR_DST_ALPHA: u32 = 9;
pub const BLEND_FACTOR_ONE: u32 = 2;
pub const BLEND_FACTOR_ONE_MINUS_CONSTANT: u32 = 13;
pub const BLEND_FACTOR_ONE_MINUS_DST: u32 = 8;
pub const BLEND_FACTOR_ONE_MINUS_DST_ALPHA: u32 = 10;
pub const BLEND_FACTOR_ONE_MINUS_SRC: u32 = 4;
pub const BLEND_FACTOR_ONE_MINUS_SRC_ALPHA: u32 = 6;
pub const BLEND_FACTOR_SRC: u32 = 3;
pub const BLEND_FACTOR_SRC_ALPHA: u32 = 5;
pub const BLEND_FACTOR_SRC_ALPHA_SATURATED: u32 = 11;
pub const BLEND_FACTOR_ZERO: u32 = 1;
pub const BLEND_OPERATION_ADD: u32 = 1;
pub const BLEND_OPERATION_MAX: u32 = 5;
pub const BLEND_OPERATION_MIN: u32 = 4;
pub const BLEND_OPERATION_REVERSE_SUBTRACT: u32 = 3;
pub const BLEND_OPERATION_SUBTRACT: u32 = 2;
pub const BUFFER_BINDING_TYPE_READ_ONLY_STORAGE: u32 = 4;
pub const BUFFER_BINDING_TYPE_STORAGE: u32 = 3;
pub const BUFFER_BINDING_TYPE_UNIFORM: u32 = 2;
pub const BUFFER_USAGE_COPY_DST: u64 = 8;
pub const BUFFER_USAGE_MAP_READ: u64 = 1;
pub const CALLBACK_MODE_ALLOW_PROCESS_EVENTS: u32 = 2;
pub const COLOR_WRITE_MASK_ALL: u64 = 15;
pub const COMPARE_FUNCTION_ALWAYS: u32 = 8;
pub const COMPARE_FUNCTION_EQUAL: u32 = 3;
pub const COMPARE_FUNCTION_GREATER: u32 = 5;
pub const COMPARE_FUNCTION_GREATER_EQUAL: u32 = 7;
pub const COMPARE_FUNCTION_LESS: u32 = 2;
pub const COMPARE_FUNCTION_LESS_EQUAL: u32 = 4;
pub const COMPARE_FUNCTION_NEVER: u32 = 1;
pub const COMPARE_FUNCTION_UNDEFINED: u32 = 0;
pub const CULL_MODE_BACK: u32 = 3;
pub const CULL_MODE_FRONT: u32 = 2;
pub const FILTER_MODE_LINEAR: u32 = 2;
pub const FILTER_MODE_NEAREST: u32 = 1;
pub const INDEX_FORMAT_UINT16: u32 = 1;
pub const INDEX_FORMAT_UINT32: u32 = 2;
pub const LOAD_OP_CLEAR: u32 = 2;
pub const LOAD_OP_LOAD: u32 = 1;
pub const MAP_MODE_READ: u64 = 1;
pub const MIPMAP_FILTER_MODE_LINEAR: u32 = 2;
pub const MIPMAP_FILTER_MODE_NEAREST: u32 = 1;
pub const OPTIONAL_BOOL_FALSE: u32 = 0;
pub const OPTIONAL_BOOL_TRUE: u32 = 1;
pub const PRIMITIVE_TOPOLOGY_LINE_LIST: u32 = 2;
pub const PRIMITIVE_TOPOLOGY_LINE_STRIP: u32 = 3;
pub const PRIMITIVE_TOPOLOGY_POINT_LIST: u32 = 1;
pub const PRIMITIVE_TOPOLOGY_TRIANGLE_LIST: u32 = 4;
pub const PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP: u32 = 5;
pub const QUERY_TYPE_OCCLUSION: u32 = 1;
pub const REQUEST_STATUS_SUCCESS: u32 = 1;
pub const S_TYPE_SHADER_SOURCE_WGSL: u32 = 2;
pub const SAMPLER_BINDING_TYPE_COMPARISON: u32 = 4;
pub const SAMPLER_BINDING_TYPE_FILTERING: u32 = 2;
pub const SAMPLER_BINDING_TYPE_NON_FILTERING: u32 = 3;
pub const STATUS_SUCCESS: u32 = 1;
pub const STORE_OP_DISCARD: u32 = 2;
pub const STORE_OP_STORE: u32 = 1;
pub const TEXTURE_ASPECT_ALL: u32 = 1;
pub const TEXTURE_ASPECT_DEPTH_ONLY: u32 = 3;
pub const TEXTURE_ASPECT_STENCIL_ONLY: u32 = 2;
pub const TEXTURE_DIMENSION_1_D: u32 = 1;
pub const TEXTURE_DIMENSION_2_D: u32 = 2;
pub const TEXTURE_DIMENSION_3_D: u32 = 3;
pub const TEXTURE_FORMAT_BGRA8_UNORM: u32 = 27;
pub const TEXTURE_FORMAT_BGRA8_UNORM_SRGB: u32 = 28;
pub const TEXTURE_FORMAT_DEPTH16_UNORM: u32 = 45;
pub const TEXTURE_FORMAT_DEPTH24_PLUS: u32 = 46;
pub const TEXTURE_FORMAT_DEPTH24_PLUS_STENCIL8: u32 = 47;
pub const TEXTURE_FORMAT_DEPTH32_FLOAT: u32 = 48;
pub const TEXTURE_FORMAT_R16_FLOAT: u32 = 9;
pub const TEXTURE_FORMAT_R32_FLOAT: u32 = 14;
pub const TEXTURE_FORMAT_R8_UNORM: u32 = 1;
pub const TEXTURE_FORMAT_RG16_FLOAT: u32 = 21;
pub const TEXTURE_FORMAT_RG32_FLOAT: u32 = 33;
pub const TEXTURE_FORMAT_RG8_UNORM: u32 = 10;
pub const TEXTURE_FORMAT_RGB10_A2_UNORM: u32 = 30;
pub const TEXTURE_FORMAT_RGBA16_FLOAT: u32 = 40;
pub const TEXTURE_FORMAT_RGBA32_FLOAT: u32 = 41;
pub const TEXTURE_FORMAT_RGBA8_UNORM: u32 = 22;
pub const TEXTURE_FORMAT_RGBA8_UNORM_SRGB: u32 = 23;
pub const TEXTURE_SAMPLE_TYPE_DEPTH: u32 = 4;
pub const TEXTURE_SAMPLE_TYPE_FLOAT: u32 = 2;
pub const TEXTURE_SAMPLE_TYPE_UINT: u32 = 6;
pub const TEXTURE_SAMPLE_TYPE_UNFILTERABLE_FLOAT: u32 = 3;
pub const TEXTURE_USAGE_COPY_SRC: u64 = 1;
pub const TEXTURE_USAGE_RENDER_ATTACHMENT: u64 = 16;
pub const TEXTURE_USAGE_TEXTURE_BINDING: u64 = 4;
pub const TEXTURE_VIEW_DIMENSION_1_D: u32 = 1;
pub const TEXTURE_VIEW_DIMENSION_2_D: u32 = 2;
pub const TEXTURE_VIEW_DIMENSION_2_D_ARRAY: u32 = 3;
pub const TEXTURE_VIEW_DIMENSION_3_D: u32 = 6;
pub const TEXTURE_VIEW_DIMENSION_CUBE: u32 = 4;
pub const TEXTURE_VIEW_DIMENSION_CUBE_ARRAY: u32 = 5;
pub const TEXTURE_VIEW_DIMENSION_UNDEFINED: u32 = 0;
pub const VERTEX_FORMAT_FLOAT32: u32 = 28;
pub const VERTEX_FORMAT_FLOAT32X2: u32 = 29;
pub const VERTEX_FORMAT_FLOAT32X3: u32 = 30;
pub const VERTEX_FORMAT_FLOAT32X4: u32 = 31;
pub const VERTEX_FORMAT_UINT32: u32 = 32;
pub const VERTEX_STEP_MODE_INSTANCE: u32 = 2;
pub const VERTEX_STEP_MODE_VERTEX: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringView {
    pub data: *const c_char,
    pub length: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Extent3D {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Origin3D {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BufferDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub usage: u64,
    pub size: u64,
    pub mapped_at_creation: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BufferMapCallbackInfo {
    pub next_in_chain: *const ChainedStruct,
    pub mode: u32,
    pub callback: Option<MapCallback>,
    pub userdata1: *mut c_void,
    pub userdata2: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RequestAdapterCallbackInfo {
    pub next_in_chain: *const ChainedStruct,
    pub mode: u32,
    pub callback: Option<RequestCallback>,
    pub userdata1: *mut c_void,
    pub userdata2: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RequestDeviceCallbackInfo {
    pub next_in_chain: *const ChainedStruct,
    pub mode: u32,
    pub callback: Option<RequestCallback>,
    pub userdata1: *mut c_void,
    pub userdata2: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UncapturedErrorCallbackInfo {
    pub next_in_chain: *const ChainedStruct,
    pub callback: Option<DeviceEventCallback>,
    pub userdata1: *mut c_void,
    pub userdata2: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceLostCallbackInfo {
    pub next_in_chain: *const ChainedStruct,
    pub mode: u32,
    pub callback: Option<DeviceEventCallback>,
    pub userdata1: *mut c_void,
    pub userdata2: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DeviceDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub required_feature_count: usize,
    pub required_features: *const c_void,
    pub required_limits: *const Limits,
    pub default_queue: QueueDescriptor,
    pub device_lost_callback_info: DeviceLostCallbackInfo,
    pub uncaptured_error_callback_info: UncapturedErrorCallbackInfo,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct QueueDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Limits {
    pub next_in_chain: *const ChainedStruct,
    pub max_texture_dimension1_d: u32,
    pub max_texture_dimension2_d: u32,
    pub max_texture_dimension3_d: u32,
    pub max_texture_array_layers: u32,
    pub max_bind_groups: u32,
    pub max_bind_groups_plus_vertex_buffers: u32,
    pub max_bindings_per_bind_group: u32,
    pub max_dynamic_uniform_buffers_per_pipeline_layout: u32,
    pub max_dynamic_storage_buffers_per_pipeline_layout: u32,
    pub max_sampled_textures_per_shader_stage: u32,
    pub max_samplers_per_shader_stage: u32,
    pub max_storage_buffers_per_shader_stage: u32,
    pub max_storage_textures_per_shader_stage: u32,
    pub max_uniform_buffers_per_shader_stage: u32,
    pub max_uniform_buffer_binding_size: u64,
    pub max_storage_buffer_binding_size: u64,
    pub min_uniform_buffer_offset_alignment: u32,
    pub min_storage_buffer_offset_alignment: u32,
    pub max_vertex_buffers: u32,
    pub max_buffer_size: u64,
    pub max_vertex_attributes: u32,
    pub max_vertex_buffer_array_stride: u32,
    pub max_inter_stage_shader_variables: u32,
    pub max_color_attachments: u32,
    pub max_color_attachment_bytes_per_sample: u32,
    pub max_compute_workgroup_storage_size: u32,
    pub max_compute_invocations_per_workgroup: u32,
    pub max_compute_workgroup_size_x: u32,
    pub max_compute_workgroup_size_y: u32,
    pub max_compute_workgroup_size_z: u32,
    pub max_compute_workgroups_per_dimension: u32,
    pub max_immediate_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AdapterInfo {
    pub next_in_chain: *const ChainedStruct,
    pub vendor: StringView,
    pub architecture: StringView,
    pub device: StringView,
    pub description: StringView,
    pub backend_type: u32,
    pub adapter_type: u32,
    pub vendor_id: u32,
    pub device_id: u32,
    pub subgroup_min_size: u32,
    pub subgroup_max_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextureDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub usage: u64,
    pub dimension: u32,
    pub size: Extent3D,
    pub format: u32,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub view_format_count: usize,
    pub view_formats: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextureViewDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub format: u32,
    pub dimension: u32,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
    pub aspect: u32,
    pub usage: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderPassColorAttachment {
    pub next_in_chain: *const ChainedStruct,
    pub view: *mut c_void,
    pub depth_slice: u32,
    pub resolve_target: *mut c_void,
    pub load_op: u32,
    pub store_op: u32,
    pub clear_value: Color,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderPassDepthStencilAttachment {
    pub next_in_chain: *const ChainedStruct,
    pub view: *mut c_void,
    pub depth_load_op: u32,
    pub depth_store_op: u32,
    pub depth_clear_value: f32,
    pub depth_read_only: u32,
    pub stencil_load_op: u32,
    pub stencil_store_op: u32,
    pub stencil_clear_value: u32,
    pub stencil_read_only: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderPassDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub color_attachment_count: usize,
    pub color_attachments: *const RenderPassColorAttachment,
    pub depth_stencil_attachment: *const RenderPassDepthStencilAttachment,
    pub occlusion_query_set: *mut c_void,
    pub timestamp_writes: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ChainedStruct {
    pub next: *const ChainedStruct,
    pub s_type: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShaderSourceWGSL {
    pub chain: ChainedStruct,
    pub code: StringView,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ShaderModuleDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VertexAttribute {
    pub next_in_chain: *const ChainedStruct,
    pub format: u32,
    pub offset: u64,
    pub shader_location: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VertexBufferLayout {
    pub next_in_chain: *const ChainedStruct,
    pub step_mode: u32,
    pub array_stride: u64,
    pub attribute_count: usize,
    pub attributes: *const VertexAttribute,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VertexState {
    pub next_in_chain: *const ChainedStruct,
    pub module: *mut c_void,
    pub entry_point: StringView,
    pub constant_count: usize,
    pub constants: *const c_void,
    pub buffer_count: usize,
    pub buffers: *const VertexBufferLayout,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PrimitiveState {
    pub next_in_chain: *const ChainedStruct,
    pub topology: u32,
    pub strip_index_format: u32,
    pub front_face: u32,
    pub cull_mode: u32,
    pub unclipped_depth: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StencilFaceState {
    pub compare: u32,
    pub fail_op: u32,
    pub depth_fail_op: u32,
    pub pass_op: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DepthStencilState {
    pub next_in_chain: *const ChainedStruct,
    pub format: u32,
    pub depth_write_enabled: u32,
    pub depth_compare: u32,
    pub stencil_front: StencilFaceState,
    pub stencil_back: StencilFaceState,
    pub stencil_read_mask: u32,
    pub stencil_write_mask: u32,
    pub depth_bias: i32,
    pub depth_bias_slope_scale: f32,
    pub depth_bias_clamp: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MultisampleState {
    pub next_in_chain: *const ChainedStruct,
    pub count: u32,
    pub mask: u32,
    pub alpha_to_coverage_enabled: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlendComponent {
    pub operation: u32,
    pub src_factor: u32,
    pub dst_factor: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ColorTargetState {
    pub next_in_chain: *const ChainedStruct,
    pub format: u32,
    pub blend: *const BlendState,
    pub write_mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FragmentState {
    pub next_in_chain: *const ChainedStruct,
    pub module: *mut c_void,
    pub entry_point: StringView,
    pub constant_count: usize,
    pub constants: *const c_void,
    pub target_count: usize,
    pub targets: *const ColorTargetState,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RenderPipelineDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub layout: *mut c_void,
    pub vertex: VertexState,
    pub primitive: PrimitiveState,
    pub depth_stencil: *const DepthStencilState,
    pub multisample: MultisampleState,
    pub fragment: *const FragmentState,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BufferBindingLayout {
    pub next_in_chain: *const ChainedStruct,
    pub kind: u32,
    pub has_dynamic_offset: u32,
    pub min_binding_size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SamplerBindingLayout {
    pub next_in_chain: *const ChainedStruct,
    pub kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TextureBindingLayout {
    pub next_in_chain: *const ChainedStruct,
    pub sample_type: u32,
    pub view_dimension: u32,
    pub multisampled: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct StorageTextureBindingLayout {
    pub next_in_chain: *const ChainedStruct,
    pub access: u32,
    pub format: u32,
    pub view_dimension: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BindGroupLayoutEntry {
    pub next_in_chain: *const ChainedStruct,
    pub binding: u32,
    pub visibility: u64,
    pub binding_array_size: u32,
    pub buffer: BufferBindingLayout,
    pub sampler: SamplerBindingLayout,
    pub texture: TextureBindingLayout,
    pub storage_texture: StorageTextureBindingLayout,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BindGroupLayoutDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub entry_count: usize,
    pub entries: *const BindGroupLayoutEntry,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PipelineLayoutDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub bind_group_layout_count: usize,
    pub bind_group_layouts: *const *mut c_void,
    pub immediate_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BindGroupEntry {
    pub next_in_chain: *const ChainedStruct,
    pub binding: u32,
    pub buffer: *mut c_void,
    pub offset: u64,
    pub size: u64,
    pub sampler: *mut c_void,
    pub texture_view: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BindGroupDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub layout: *mut c_void,
    pub entry_count: usize,
    pub entries: *const BindGroupEntry,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SamplerDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub address_mode_u: u32,
    pub address_mode_v: u32,
    pub address_mode_w: u32,
    pub mag_filter: u32,
    pub min_filter: u32,
    pub mipmap_filter: u32,
    pub lod_min_clamp: f32,
    pub lod_max_clamp: f32,
    pub compare: u32,
    pub max_anisotropy: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TexelCopyTextureInfo {
    pub texture: *mut c_void,
    pub mip_level: u32,
    pub origin: Origin3D,
    pub aspect: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TexelCopyBufferLayout {
    pub offset: u64,
    pub bytes_per_row: u32,
    pub rows_per_image: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TexelCopyBufferInfo {
    pub layout: TexelCopyBufferLayout,
    pub buffer: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct QuerySetDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub kind: u32,
    pub count: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ComputeState {
    pub next_in_chain: *const ChainedStruct,
    pub module: *mut c_void,
    pub entry_point: StringView,
    pub constant_count: usize,
    pub constants: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ComputePipelineDescriptor {
    pub next_in_chain: *const ChainedStruct,
    pub label: StringView,
    pub layout: *mut c_void,
    pub compute: ComputeState,
}

#[cfg(target_pointer_width = "64")]
mod layout {
    use super::*;
    use core::mem::offset_of;

    const _: () = assert!(
        size_of::<StringView>() == 16
            && offset_of!(StringView, data) == 0
            && offset_of!(StringView, length) == 8
    );
    const _: () = assert!(
        size_of::<Extent3D>() == 12
            && offset_of!(Extent3D, width) == 0
            && offset_of!(Extent3D, height) == 4
            && offset_of!(Extent3D, depth_or_array_layers) == 8
    );
    const _: () = assert!(
        size_of::<Origin3D>() == 12
            && offset_of!(Origin3D, x) == 0
            && offset_of!(Origin3D, y) == 4
            && offset_of!(Origin3D, z) == 8
    );
    const _: () = assert!(
        size_of::<Color>() == 32
            && offset_of!(Color, r) == 0
            && offset_of!(Color, g) == 8
            && offset_of!(Color, b) == 16
            && offset_of!(Color, a) == 24
    );
    const _: () = assert!(
        size_of::<BufferDescriptor>() == 48
            && offset_of!(BufferDescriptor, next_in_chain) == 0
            && offset_of!(BufferDescriptor, label) == 8
            && offset_of!(BufferDescriptor, usage) == 24
            && offset_of!(BufferDescriptor, size) == 32
            && offset_of!(BufferDescriptor, mapped_at_creation) == 40
    );
    const _: () = assert!(
        size_of::<BufferMapCallbackInfo>() == 40
            && offset_of!(BufferMapCallbackInfo, next_in_chain) == 0
            && offset_of!(BufferMapCallbackInfo, mode) == 8
            && offset_of!(BufferMapCallbackInfo, callback) == 16
            && offset_of!(BufferMapCallbackInfo, userdata1) == 24
            && offset_of!(BufferMapCallbackInfo, userdata2) == 32
    );
    const _: () = assert!(
        size_of::<RequestAdapterCallbackInfo>() == 40
            && offset_of!(RequestAdapterCallbackInfo, next_in_chain) == 0
            && offset_of!(RequestAdapterCallbackInfo, mode) == 8
            && offset_of!(RequestAdapterCallbackInfo, callback) == 16
            && offset_of!(RequestAdapterCallbackInfo, userdata1) == 24
            && offset_of!(RequestAdapterCallbackInfo, userdata2) == 32
    );
    const _: () = assert!(
        size_of::<RequestDeviceCallbackInfo>() == 40
            && offset_of!(RequestDeviceCallbackInfo, next_in_chain) == 0
            && offset_of!(RequestDeviceCallbackInfo, mode) == 8
            && offset_of!(RequestDeviceCallbackInfo, callback) == 16
            && offset_of!(RequestDeviceCallbackInfo, userdata1) == 24
            && offset_of!(RequestDeviceCallbackInfo, userdata2) == 32
    );
    const _: () = assert!(
        size_of::<UncapturedErrorCallbackInfo>() == 32
            && offset_of!(UncapturedErrorCallbackInfo, next_in_chain) == 0
            && offset_of!(UncapturedErrorCallbackInfo, callback) == 8
            && offset_of!(UncapturedErrorCallbackInfo, userdata1) == 16
            && offset_of!(UncapturedErrorCallbackInfo, userdata2) == 24
    );
    const _: () = assert!(
        size_of::<DeviceLostCallbackInfo>() == 40
            && offset_of!(DeviceLostCallbackInfo, next_in_chain) == 0
            && offset_of!(DeviceLostCallbackInfo, mode) == 8
            && offset_of!(DeviceLostCallbackInfo, callback) == 16
            && offset_of!(DeviceLostCallbackInfo, userdata1) == 24
            && offset_of!(DeviceLostCallbackInfo, userdata2) == 32
    );
    const _: () = assert!(
        size_of::<DeviceDescriptor>() == 144
            && offset_of!(DeviceDescriptor, next_in_chain) == 0
            && offset_of!(DeviceDescriptor, label) == 8
            && offset_of!(DeviceDescriptor, required_feature_count) == 24
            && offset_of!(DeviceDescriptor, required_features) == 32
            && offset_of!(DeviceDescriptor, required_limits) == 40
            && offset_of!(DeviceDescriptor, default_queue) == 48
            && offset_of!(DeviceDescriptor, device_lost_callback_info) == 72
            && offset_of!(DeviceDescriptor, uncaptured_error_callback_info) == 112
    );
    const _: () = assert!(
        size_of::<QueueDescriptor>() == 24
            && offset_of!(QueueDescriptor, next_in_chain) == 0
            && offset_of!(QueueDescriptor, label) == 8
    );
    const _: () = assert!(
        size_of::<Limits>() == 152
            && offset_of!(Limits, next_in_chain) == 0
            && offset_of!(Limits, max_texture_dimension1_d) == 8
            && offset_of!(Limits, max_texture_dimension2_d) == 12
            && offset_of!(Limits, max_texture_dimension3_d) == 16
            && offset_of!(Limits, max_texture_array_layers) == 20
            && offset_of!(Limits, max_bind_groups) == 24
            && offset_of!(Limits, max_bind_groups_plus_vertex_buffers) == 28
            && offset_of!(Limits, max_bindings_per_bind_group) == 32
            && offset_of!(Limits, max_dynamic_uniform_buffers_per_pipeline_layout) == 36
            && offset_of!(Limits, max_dynamic_storage_buffers_per_pipeline_layout) == 40
            && offset_of!(Limits, max_sampled_textures_per_shader_stage) == 44
            && offset_of!(Limits, max_samplers_per_shader_stage) == 48
            && offset_of!(Limits, max_storage_buffers_per_shader_stage) == 52
            && offset_of!(Limits, max_storage_textures_per_shader_stage) == 56
            && offset_of!(Limits, max_uniform_buffers_per_shader_stage) == 60
            && offset_of!(Limits, max_uniform_buffer_binding_size) == 64
            && offset_of!(Limits, max_storage_buffer_binding_size) == 72
            && offset_of!(Limits, min_uniform_buffer_offset_alignment) == 80
            && offset_of!(Limits, min_storage_buffer_offset_alignment) == 84
            && offset_of!(Limits, max_vertex_buffers) == 88
            && offset_of!(Limits, max_buffer_size) == 96
            && offset_of!(Limits, max_vertex_attributes) == 104
            && offset_of!(Limits, max_vertex_buffer_array_stride) == 108
            && offset_of!(Limits, max_inter_stage_shader_variables) == 112
            && offset_of!(Limits, max_color_attachments) == 116
            && offset_of!(Limits, max_color_attachment_bytes_per_sample) == 120
            && offset_of!(Limits, max_compute_workgroup_storage_size) == 124
            && offset_of!(Limits, max_compute_invocations_per_workgroup) == 128
            && offset_of!(Limits, max_compute_workgroup_size_x) == 132
            && offset_of!(Limits, max_compute_workgroup_size_y) == 136
            && offset_of!(Limits, max_compute_workgroup_size_z) == 140
            && offset_of!(Limits, max_compute_workgroups_per_dimension) == 144
            && offset_of!(Limits, max_immediate_size) == 148
    );
    const _: () = assert!(
        size_of::<AdapterInfo>() == 96
            && offset_of!(AdapterInfo, next_in_chain) == 0
            && offset_of!(AdapterInfo, vendor) == 8
            && offset_of!(AdapterInfo, architecture) == 24
            && offset_of!(AdapterInfo, device) == 40
            && offset_of!(AdapterInfo, description) == 56
            && offset_of!(AdapterInfo, backend_type) == 72
            && offset_of!(AdapterInfo, adapter_type) == 76
            && offset_of!(AdapterInfo, vendor_id) == 80
            && offset_of!(AdapterInfo, device_id) == 84
            && offset_of!(AdapterInfo, subgroup_min_size) == 88
            && offset_of!(AdapterInfo, subgroup_max_size) == 92
    );
    const _: () = assert!(
        size_of::<TextureDescriptor>() == 80
            && offset_of!(TextureDescriptor, next_in_chain) == 0
            && offset_of!(TextureDescriptor, label) == 8
            && offset_of!(TextureDescriptor, usage) == 24
            && offset_of!(TextureDescriptor, dimension) == 32
            && offset_of!(TextureDescriptor, size) == 36
            && offset_of!(TextureDescriptor, format) == 48
            && offset_of!(TextureDescriptor, mip_level_count) == 52
            && offset_of!(TextureDescriptor, sample_count) == 56
            && offset_of!(TextureDescriptor, view_format_count) == 64
            && offset_of!(TextureDescriptor, view_formats) == 72
    );
    const _: () = assert!(
        size_of::<TextureViewDescriptor>() == 64
            && offset_of!(TextureViewDescriptor, next_in_chain) == 0
            && offset_of!(TextureViewDescriptor, label) == 8
            && offset_of!(TextureViewDescriptor, format) == 24
            && offset_of!(TextureViewDescriptor, dimension) == 28
            && offset_of!(TextureViewDescriptor, base_mip_level) == 32
            && offset_of!(TextureViewDescriptor, mip_level_count) == 36
            && offset_of!(TextureViewDescriptor, base_array_layer) == 40
            && offset_of!(TextureViewDescriptor, array_layer_count) == 44
            && offset_of!(TextureViewDescriptor, aspect) == 48
            && offset_of!(TextureViewDescriptor, usage) == 56
    );
    const _: () = assert!(
        size_of::<RenderPassColorAttachment>() == 72
            && offset_of!(RenderPassColorAttachment, next_in_chain) == 0
            && offset_of!(RenderPassColorAttachment, view) == 8
            && offset_of!(RenderPassColorAttachment, depth_slice) == 16
            && offset_of!(RenderPassColorAttachment, resolve_target) == 24
            && offset_of!(RenderPassColorAttachment, load_op) == 32
            && offset_of!(RenderPassColorAttachment, store_op) == 36
            && offset_of!(RenderPassColorAttachment, clear_value) == 40
    );
    const _: () = assert!(
        size_of::<RenderPassDepthStencilAttachment>() == 48
            && offset_of!(RenderPassDepthStencilAttachment, next_in_chain) == 0
            && offset_of!(RenderPassDepthStencilAttachment, view) == 8
            && offset_of!(RenderPassDepthStencilAttachment, depth_load_op) == 16
            && offset_of!(RenderPassDepthStencilAttachment, depth_store_op) == 20
            && offset_of!(RenderPassDepthStencilAttachment, depth_clear_value) == 24
            && offset_of!(RenderPassDepthStencilAttachment, depth_read_only) == 28
            && offset_of!(RenderPassDepthStencilAttachment, stencil_load_op) == 32
            && offset_of!(RenderPassDepthStencilAttachment, stencil_store_op) == 36
            && offset_of!(RenderPassDepthStencilAttachment, stencil_clear_value) == 40
            && offset_of!(RenderPassDepthStencilAttachment, stencil_read_only) == 44
    );
    const _: () = assert!(
        size_of::<RenderPassDescriptor>() == 64
            && offset_of!(RenderPassDescriptor, next_in_chain) == 0
            && offset_of!(RenderPassDescriptor, label) == 8
            && offset_of!(RenderPassDescriptor, color_attachment_count) == 24
            && offset_of!(RenderPassDescriptor, color_attachments) == 32
            && offset_of!(RenderPassDescriptor, depth_stencil_attachment) == 40
            && offset_of!(RenderPassDescriptor, occlusion_query_set) == 48
            && offset_of!(RenderPassDescriptor, timestamp_writes) == 56
    );
    const _: () = assert!(
        size_of::<ChainedStruct>() == 16
            && offset_of!(ChainedStruct, next) == 0
            && offset_of!(ChainedStruct, s_type) == 8
    );
    const _: () = assert!(
        size_of::<ShaderSourceWGSL>() == 32
            && offset_of!(ShaderSourceWGSL, chain) == 0
            && offset_of!(ShaderSourceWGSL, code) == 16
    );
    const _: () = assert!(
        size_of::<ShaderModuleDescriptor>() == 24
            && offset_of!(ShaderModuleDescriptor, next_in_chain) == 0
            && offset_of!(ShaderModuleDescriptor, label) == 8
    );
    const _: () = assert!(
        size_of::<VertexAttribute>() == 32
            && offset_of!(VertexAttribute, next_in_chain) == 0
            && offset_of!(VertexAttribute, format) == 8
            && offset_of!(VertexAttribute, offset) == 16
            && offset_of!(VertexAttribute, shader_location) == 24
    );
    const _: () = assert!(
        size_of::<VertexBufferLayout>() == 40
            && offset_of!(VertexBufferLayout, next_in_chain) == 0
            && offset_of!(VertexBufferLayout, step_mode) == 8
            && offset_of!(VertexBufferLayout, array_stride) == 16
            && offset_of!(VertexBufferLayout, attribute_count) == 24
            && offset_of!(VertexBufferLayout, attributes) == 32
    );
    const _: () = assert!(
        size_of::<VertexState>() == 64
            && offset_of!(VertexState, next_in_chain) == 0
            && offset_of!(VertexState, module) == 8
            && offset_of!(VertexState, entry_point) == 16
            && offset_of!(VertexState, constant_count) == 32
            && offset_of!(VertexState, constants) == 40
            && offset_of!(VertexState, buffer_count) == 48
            && offset_of!(VertexState, buffers) == 56
    );
    const _: () = assert!(
        size_of::<PrimitiveState>() == 32
            && offset_of!(PrimitiveState, next_in_chain) == 0
            && offset_of!(PrimitiveState, topology) == 8
            && offset_of!(PrimitiveState, strip_index_format) == 12
            && offset_of!(PrimitiveState, front_face) == 16
            && offset_of!(PrimitiveState, cull_mode) == 20
            && offset_of!(PrimitiveState, unclipped_depth) == 24
    );
    const _: () = assert!(
        size_of::<StencilFaceState>() == 16
            && offset_of!(StencilFaceState, compare) == 0
            && offset_of!(StencilFaceState, fail_op) == 4
            && offset_of!(StencilFaceState, depth_fail_op) == 8
            && offset_of!(StencilFaceState, pass_op) == 12
    );
    const _: () = assert!(
        size_of::<DepthStencilState>() == 72
            && offset_of!(DepthStencilState, next_in_chain) == 0
            && offset_of!(DepthStencilState, format) == 8
            && offset_of!(DepthStencilState, depth_write_enabled) == 12
            && offset_of!(DepthStencilState, depth_compare) == 16
            && offset_of!(DepthStencilState, stencil_front) == 20
            && offset_of!(DepthStencilState, stencil_back) == 36
            && offset_of!(DepthStencilState, stencil_read_mask) == 52
            && offset_of!(DepthStencilState, stencil_write_mask) == 56
            && offset_of!(DepthStencilState, depth_bias) == 60
            && offset_of!(DepthStencilState, depth_bias_slope_scale) == 64
            && offset_of!(DepthStencilState, depth_bias_clamp) == 68
    );
    const _: () = assert!(
        size_of::<MultisampleState>() == 24
            && offset_of!(MultisampleState, next_in_chain) == 0
            && offset_of!(MultisampleState, count) == 8
            && offset_of!(MultisampleState, mask) == 12
            && offset_of!(MultisampleState, alpha_to_coverage_enabled) == 16
    );
    const _: () = assert!(
        size_of::<BlendComponent>() == 12
            && offset_of!(BlendComponent, operation) == 0
            && offset_of!(BlendComponent, src_factor) == 4
            && offset_of!(BlendComponent, dst_factor) == 8
    );
    const _: () = assert!(
        size_of::<BlendState>() == 24
            && offset_of!(BlendState, color) == 0
            && offset_of!(BlendState, alpha) == 12
    );
    const _: () = assert!(
        size_of::<ColorTargetState>() == 32
            && offset_of!(ColorTargetState, next_in_chain) == 0
            && offset_of!(ColorTargetState, format) == 8
            && offset_of!(ColorTargetState, blend) == 16
            && offset_of!(ColorTargetState, write_mask) == 24
    );
    const _: () = assert!(
        size_of::<FragmentState>() == 64
            && offset_of!(FragmentState, next_in_chain) == 0
            && offset_of!(FragmentState, module) == 8
            && offset_of!(FragmentState, entry_point) == 16
            && offset_of!(FragmentState, constant_count) == 32
            && offset_of!(FragmentState, constants) == 40
            && offset_of!(FragmentState, target_count) == 48
            && offset_of!(FragmentState, targets) == 56
    );
    const _: () = assert!(
        size_of::<RenderPipelineDescriptor>() == 168
            && offset_of!(RenderPipelineDescriptor, next_in_chain) == 0
            && offset_of!(RenderPipelineDescriptor, label) == 8
            && offset_of!(RenderPipelineDescriptor, layout) == 24
            && offset_of!(RenderPipelineDescriptor, vertex) == 32
            && offset_of!(RenderPipelineDescriptor, primitive) == 96
            && offset_of!(RenderPipelineDescriptor, depth_stencil) == 128
            && offset_of!(RenderPipelineDescriptor, multisample) == 136
            && offset_of!(RenderPipelineDescriptor, fragment) == 160
    );
    const _: () = assert!(
        size_of::<BufferBindingLayout>() == 24
            && offset_of!(BufferBindingLayout, next_in_chain) == 0
            && offset_of!(BufferBindingLayout, kind) == 8
            && offset_of!(BufferBindingLayout, has_dynamic_offset) == 12
            && offset_of!(BufferBindingLayout, min_binding_size) == 16
    );
    const _: () = assert!(
        size_of::<SamplerBindingLayout>() == 16
            && offset_of!(SamplerBindingLayout, next_in_chain) == 0
            && offset_of!(SamplerBindingLayout, kind) == 8
    );
    const _: () = assert!(
        size_of::<TextureBindingLayout>() == 24
            && offset_of!(TextureBindingLayout, next_in_chain) == 0
            && offset_of!(TextureBindingLayout, sample_type) == 8
            && offset_of!(TextureBindingLayout, view_dimension) == 12
            && offset_of!(TextureBindingLayout, multisampled) == 16
    );
    const _: () = assert!(
        size_of::<StorageTextureBindingLayout>() == 24
            && offset_of!(StorageTextureBindingLayout, next_in_chain) == 0
            && offset_of!(StorageTextureBindingLayout, access) == 8
            && offset_of!(StorageTextureBindingLayout, format) == 12
            && offset_of!(StorageTextureBindingLayout, view_dimension) == 16
    );
    const _: () = assert!(
        size_of::<BindGroupLayoutEntry>() == 120
            && offset_of!(BindGroupLayoutEntry, next_in_chain) == 0
            && offset_of!(BindGroupLayoutEntry, binding) == 8
            && offset_of!(BindGroupLayoutEntry, visibility) == 16
            && offset_of!(BindGroupLayoutEntry, binding_array_size) == 24
            && offset_of!(BindGroupLayoutEntry, buffer) == 32
            && offset_of!(BindGroupLayoutEntry, sampler) == 56
            && offset_of!(BindGroupLayoutEntry, texture) == 72
            && offset_of!(BindGroupLayoutEntry, storage_texture) == 96
    );
    const _: () = assert!(
        size_of::<BindGroupLayoutDescriptor>() == 40
            && offset_of!(BindGroupLayoutDescriptor, next_in_chain) == 0
            && offset_of!(BindGroupLayoutDescriptor, label) == 8
            && offset_of!(BindGroupLayoutDescriptor, entry_count) == 24
            && offset_of!(BindGroupLayoutDescriptor, entries) == 32
    );
    const _: () = assert!(
        size_of::<PipelineLayoutDescriptor>() == 48
            && offset_of!(PipelineLayoutDescriptor, next_in_chain) == 0
            && offset_of!(PipelineLayoutDescriptor, label) == 8
            && offset_of!(PipelineLayoutDescriptor, bind_group_layout_count) == 24
            && offset_of!(PipelineLayoutDescriptor, bind_group_layouts) == 32
            && offset_of!(PipelineLayoutDescriptor, immediate_size) == 40
    );
    const _: () = assert!(
        size_of::<BindGroupEntry>() == 56
            && offset_of!(BindGroupEntry, next_in_chain) == 0
            && offset_of!(BindGroupEntry, binding) == 8
            && offset_of!(BindGroupEntry, buffer) == 16
            && offset_of!(BindGroupEntry, offset) == 24
            && offset_of!(BindGroupEntry, size) == 32
            && offset_of!(BindGroupEntry, sampler) == 40
            && offset_of!(BindGroupEntry, texture_view) == 48
    );
    const _: () = assert!(
        size_of::<BindGroupDescriptor>() == 48
            && offset_of!(BindGroupDescriptor, next_in_chain) == 0
            && offset_of!(BindGroupDescriptor, label) == 8
            && offset_of!(BindGroupDescriptor, layout) == 24
            && offset_of!(BindGroupDescriptor, entry_count) == 32
            && offset_of!(BindGroupDescriptor, entries) == 40
    );
    const _: () = assert!(
        size_of::<SamplerDescriptor>() == 64
            && offset_of!(SamplerDescriptor, next_in_chain) == 0
            && offset_of!(SamplerDescriptor, label) == 8
            && offset_of!(SamplerDescriptor, address_mode_u) == 24
            && offset_of!(SamplerDescriptor, address_mode_v) == 28
            && offset_of!(SamplerDescriptor, address_mode_w) == 32
            && offset_of!(SamplerDescriptor, mag_filter) == 36
            && offset_of!(SamplerDescriptor, min_filter) == 40
            && offset_of!(SamplerDescriptor, mipmap_filter) == 44
            && offset_of!(SamplerDescriptor, lod_min_clamp) == 48
            && offset_of!(SamplerDescriptor, lod_max_clamp) == 52
            && offset_of!(SamplerDescriptor, compare) == 56
            && offset_of!(SamplerDescriptor, max_anisotropy) == 60
    );
    const _: () = assert!(
        size_of::<TexelCopyTextureInfo>() == 32
            && offset_of!(TexelCopyTextureInfo, texture) == 0
            && offset_of!(TexelCopyTextureInfo, mip_level) == 8
            && offset_of!(TexelCopyTextureInfo, origin) == 12
            && offset_of!(TexelCopyTextureInfo, aspect) == 24
    );
    const _: () = assert!(
        size_of::<TexelCopyBufferLayout>() == 16
            && offset_of!(TexelCopyBufferLayout, offset) == 0
            && offset_of!(TexelCopyBufferLayout, bytes_per_row) == 8
            && offset_of!(TexelCopyBufferLayout, rows_per_image) == 12
    );
    const _: () = assert!(
        size_of::<TexelCopyBufferInfo>() == 24
            && offset_of!(TexelCopyBufferInfo, layout) == 0
            && offset_of!(TexelCopyBufferInfo, buffer) == 16
    );
    const _: () = assert!(
        size_of::<QuerySetDescriptor>() == 32
            && offset_of!(QuerySetDescriptor, next_in_chain) == 0
            && offset_of!(QuerySetDescriptor, label) == 8
            && offset_of!(QuerySetDescriptor, kind) == 24
            && offset_of!(QuerySetDescriptor, count) == 28
    );
    const _: () = assert!(
        size_of::<ComputeState>() == 48
            && offset_of!(ComputeState, next_in_chain) == 0
            && offset_of!(ComputeState, module) == 8
            && offset_of!(ComputeState, entry_point) == 16
            && offset_of!(ComputeState, constant_count) == 32
            && offset_of!(ComputeState, constants) == 40
    );
    const _: () = assert!(
        size_of::<ComputePipelineDescriptor>() == 80
            && offset_of!(ComputePipelineDescriptor, next_in_chain) == 0
            && offset_of!(ComputePipelineDescriptor, label) == 8
            && offset_of!(ComputePipelineDescriptor, layout) == 24
            && offset_of!(ComputePipelineDescriptor, compute) == 32
    );
}
