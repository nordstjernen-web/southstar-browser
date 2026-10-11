//! Southstar — the wgpu-native entry points WebGPU calls, owned handles that release themselves, and the blocking waits on its callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use core::sync::atomic::{AtomicPtr, Ordering};

use southstar_js_engine::{Scope, Value};

use super::sys::*;

#[repr(C)]
struct Future {
    id: u64,
}

type Raw = *mut c_void;

unsafe extern "C" {
    fn wgpuCreateInstance(descriptor: *const c_void) -> Raw;
    fn wgpuInstanceRequestAdapter(
        instance: Raw,
        options: *const c_void,
        info: RequestAdapterCallbackInfo,
    ) -> Future;
    fn wgpuInstanceProcessEvents(instance: Raw);
    fn wgpuAdapterGetInfo(adapter: Raw, info: *mut AdapterInfo) -> u32;
    fn wgpuAdapterInfoFreeMembers(info: AdapterInfo);
    fn wgpuAdapterGetLimits(adapter: Raw, limits: *mut Limits) -> u32;
    fn wgpuAdapterRequestDevice(
        adapter: Raw,
        descriptor: *const DeviceDescriptor,
        info: RequestDeviceCallbackInfo,
    ) -> Future;
    fn wgpuAdapterRelease(adapter: Raw);
    fn wgpuDeviceGetQueue(device: Raw) -> Raw;
    fn wgpuDeviceCreateBuffer(device: Raw, descriptor: *const BufferDescriptor) -> Raw;
    fn wgpuDeviceCreateCommandEncoder(device: Raw, descriptor: *const c_void) -> Raw;
    fn wgpuDeviceCreateShaderModule(device: Raw, descriptor: *const ShaderModuleDescriptor) -> Raw;
    fn wgpuDeviceCreateRenderPipeline(
        device: Raw,
        descriptor: *const RenderPipelineDescriptor,
    ) -> Raw;
    fn wgpuDeviceCreateComputePipeline(
        device: Raw,
        descriptor: *const ComputePipelineDescriptor,
    ) -> Raw;
    fn wgpuDeviceCreateBindGroupLayout(
        device: Raw,
        descriptor: *const BindGroupLayoutDescriptor,
    ) -> Raw;
    fn wgpuDeviceCreatePipelineLayout(
        device: Raw,
        descriptor: *const PipelineLayoutDescriptor,
    ) -> Raw;
    fn wgpuDeviceCreateBindGroup(device: Raw, descriptor: *const BindGroupDescriptor) -> Raw;
    fn wgpuDeviceCreateSampler(device: Raw, descriptor: *const SamplerDescriptor) -> Raw;
    fn wgpuDeviceCreateTexture(device: Raw, descriptor: *const TextureDescriptor) -> Raw;
    fn wgpuDeviceCreateQuerySet(device: Raw, descriptor: *const QuerySetDescriptor) -> Raw;
    fn wgpuDevicePoll(device: Raw, wait: u32, submission_index: *const u64) -> u32;
    fn wgpuDeviceAddRef(device: Raw);
    fn wgpuDeviceRelease(device: Raw);
    fn wgpuQueueWriteBuffer(queue: Raw, buffer: Raw, offset: u64, data: *const c_void, size: usize);
    fn wgpuQueueWriteTexture(
        queue: Raw,
        destination: *const TexelCopyTextureInfo,
        data: *const c_void,
        size: usize,
        layout: *const TexelCopyBufferLayout,
        write_size: *const Extent3D,
    );
    fn wgpuQueueSubmit(queue: Raw, count: usize, commands: *const Raw);
    fn wgpuQueueAddRef(queue: Raw);
    fn wgpuQueueRelease(queue: Raw);
    fn wgpuBufferMapAsync(
        buffer: Raw,
        mode: u64,
        offset: usize,
        size: usize,
        info: BufferMapCallbackInfo,
    ) -> Future;
    fn wgpuBufferGetMappedRange(buffer: Raw, offset: usize, size: usize) -> *mut c_void;
    fn wgpuBufferGetConstMappedRange(buffer: Raw, offset: usize, size: usize) -> *const c_void;
    fn wgpuBufferUnmap(buffer: Raw);
    fn wgpuBufferDestroy(buffer: Raw);
    fn wgpuBufferRelease(buffer: Raw);
    fn wgpuTextureCreateView(texture: Raw, descriptor: *const TextureViewDescriptor) -> Raw;
    fn wgpuTextureDestroy(texture: Raw);
    fn wgpuTextureAddRef(texture: Raw);
    fn wgpuTextureRelease(texture: Raw);
    fn wgpuTextureViewRelease(view: Raw);
    fn wgpuCommandEncoderBeginRenderPass(
        encoder: Raw,
        descriptor: *const RenderPassDescriptor,
    ) -> Raw;
    fn wgpuCommandEncoderBeginComputePass(encoder: Raw, descriptor: *const c_void) -> Raw;
    fn wgpuCommandEncoderCopyTextureToTexture(
        encoder: Raw,
        source: *const TexelCopyTextureInfo,
        destination: *const TexelCopyTextureInfo,
        size: *const Extent3D,
    );
    fn wgpuCommandEncoderCopyBufferToBuffer(
        encoder: Raw,
        source: Raw,
        source_offset: u64,
        destination: Raw,
        destination_offset: u64,
        size: u64,
    );
    fn wgpuCommandEncoderCopyTextureToBuffer(
        encoder: Raw,
        source: *const TexelCopyTextureInfo,
        destination: *const TexelCopyBufferInfo,
        size: *const Extent3D,
    );
    fn wgpuCommandEncoderResolveQuerySet(
        encoder: Raw,
        query_set: Raw,
        first: u32,
        count: u32,
        destination: Raw,
        offset: u64,
    );
    fn wgpuCommandEncoderFinish(encoder: Raw, descriptor: *const c_void) -> Raw;
    fn wgpuCommandEncoderRelease(encoder: Raw);
    fn wgpuCommandBufferRelease(command_buffer: Raw);
    fn wgpuRenderPassEncoderEnd(pass: Raw);
    fn wgpuRenderPassEncoderSetPipeline(pass: Raw, pipeline: Raw);
    fn wgpuRenderPassEncoderSetVertexBuffer(
        pass: Raw,
        slot: u32,
        buffer: Raw,
        offset: u64,
        size: u64,
    );
    fn wgpuRenderPassEncoderSetIndexBuffer(
        pass: Raw,
        buffer: Raw,
        format: u32,
        offset: u64,
        size: u64,
    );
    fn wgpuRenderPassEncoderSetBindGroup(
        pass: Raw,
        index: u32,
        group: Raw,
        offset_count: usize,
        offsets: *const u32,
    );
    fn wgpuRenderPassEncoderDraw(
        pass: Raw,
        vertices: u32,
        instances: u32,
        first: u32,
        first_instance: u32,
    );
    fn wgpuRenderPassEncoderDrawIndexed(
        pass: Raw,
        indices: u32,
        instances: u32,
        first: u32,
        base_vertex: i32,
        first_instance: u32,
    );
    fn wgpuRenderPassEncoderRelease(pass: Raw);
    fn wgpuShaderModuleRelease(module: Raw);
    fn wgpuRenderPipelineGetBindGroupLayout(pipeline: Raw, index: u32) -> Raw;
    fn wgpuRenderPipelineRelease(pipeline: Raw);
    fn wgpuComputePipelineGetBindGroupLayout(pipeline: Raw, index: u32) -> Raw;
    fn wgpuComputePipelineRelease(pipeline: Raw);
    fn wgpuComputePassEncoderSetPipeline(pass: Raw, pipeline: Raw);
    fn wgpuComputePassEncoderSetBindGroup(
        pass: Raw,
        index: u32,
        group: Raw,
        offset_count: usize,
        offsets: *const u32,
    );
    fn wgpuComputePassEncoderDispatchWorkgroups(pass: Raw, x: u32, y: u32, z: u32);
    fn wgpuComputePassEncoderEnd(pass: Raw);
    fn wgpuComputePassEncoderRelease(pass: Raw);
    fn wgpuBindGroupLayoutRelease(layout: Raw);
    fn wgpuPipelineLayoutRelease(layout: Raw);
    fn wgpuBindGroupRelease(group: Raw);
    fn wgpuSamplerRelease(sampler: Raw);
    fn wgpuQuerySetDestroy(query_set: Raw);
    fn wgpuQuerySetRelease(query_set: Raw);
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
}

const G_LOG_LEVEL_WARNING: c_int = 1 << 4;

pub(crate) unsafe trait Zeroed: Copy {
    fn zeroed() -> Self {
        unsafe { core::mem::zeroed() }
    }
}

macro_rules! zeroed {
    ($($t:ty),*) => { $(unsafe impl Zeroed for $t {})* };
}

zeroed!(
    BufferDescriptor,
    Limits,
    AdapterInfo,
    TextureDescriptor,
    TextureViewDescriptor,
    RenderPassColorAttachment,
    RenderPassDepthStencilAttachment,
    RenderPassDescriptor,
    ShaderSourceWGSL,
    ShaderModuleDescriptor,
    VertexAttribute,
    VertexBufferLayout,
    DepthStencilState,
    BlendState,
    ColorTargetState,
    FragmentState,
    RenderPipelineDescriptor,
    BindGroupLayoutEntry,
    BindGroupLayoutDescriptor,
    PipelineLayoutDescriptor,
    BindGroupEntry,
    BindGroupDescriptor,
    SamplerDescriptor,
    TexelCopyTextureInfo,
    TexelCopyBufferLayout,
    TexelCopyBufferInfo,
    QuerySetDescriptor,
    ComputePipelineDescriptor,
    DeviceDescriptor
);

pub(crate) fn string_view(text: Option<&str>) -> StringView {
    match text {
        Some(text) => StringView {
            data: text.as_ptr().cast(),
            length: text.len(),
        },
        None => StringView {
            data: ptr::null(),
            length: 0,
        },
    }
}

fn view_text(view: &StringView) -> String {
    if view.data.is_null() || view.length == 0 {
        return String::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(view.data.cast::<u8>(), view.length) };
    String::from_utf8_lossy(bytes).into_owned()
}

macro_rules! handle {
    ($name:ident, $release:ident) => {
        pub(crate) struct $name(NonNull<c_void>);

        impl $name {
            fn from_raw(raw: Raw) -> Option<$name> {
                NonNull::new(raw).map($name)
            }

            pub(crate) fn raw(&self) -> Raw {
                self.0.as_ptr()
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe { $release(self.raw()) };
            }
        }
    };
    ($name:ident, $release:ident, $add_ref:ident) => {
        handle!($name, $release);

        impl Clone for $name {
            fn clone(&self) -> $name {
                unsafe { $add_ref(self.raw()) };
                $name(self.0)
            }
        }
    };
}

handle!(Adapter, wgpuAdapterRelease);
handle!(Device, wgpuDeviceRelease, wgpuDeviceAddRef);
handle!(Queue, wgpuQueueRelease, wgpuQueueAddRef);
handle!(Buffer, wgpuBufferRelease);
handle!(Texture, wgpuTextureRelease, wgpuTextureAddRef);
handle!(TextureView, wgpuTextureViewRelease);
handle!(CommandEncoder, wgpuCommandEncoderRelease);
handle!(CommandBuffer, wgpuCommandBufferRelease);
handle!(RenderPass, wgpuRenderPassEncoderRelease);
handle!(ComputePass, wgpuComputePassEncoderRelease);
handle!(ShaderModule, wgpuShaderModuleRelease);
handle!(RenderPipeline, wgpuRenderPipelineRelease);
handle!(ComputePipeline, wgpuComputePipelineRelease);
handle!(BindGroupLayout, wgpuBindGroupLayoutRelease);
handle!(PipelineLayout, wgpuPipelineLayoutRelease);
handle!(BindGroup, wgpuBindGroupRelease);
handle!(Sampler, wgpuSamplerRelease);
handle!(QuerySet, wgpuQuerySetRelease);

static INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

pub(crate) fn instance() -> Option<Raw> {
    let mut raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        raw = unsafe { wgpuCreateInstance(ptr::null()) };
        INSTANCE.store(raw, Ordering::Release);
    }
    (!raw.is_null()).then_some(raw)
}

fn process_events() {
    if let Some(raw) = instance() {
        unsafe { wgpuInstanceProcessEvents(raw) };
    }
}

struct Wait {
    done: bool,
    handle: Raw,
}

impl Wait {
    fn new() -> Wait {
        Wait {
            done: false,
            handle: ptr::null_mut(),
        }
    }
}

unsafe extern "C" fn on_adapter(status: u32, adapter: Raw, _: StringView, wait: Raw, _: Raw) {
    let wait = unsafe { &mut *wait.cast::<Wait>() };
    wait.handle = if status == REQUEST_ADAPTER_STATUS_SUCCESS {
        adapter
    } else {
        ptr::null_mut()
    };
    wait.done = true;
}

unsafe extern "C" fn on_device(status: u32, device: Raw, _: StringView, wait: Raw, _: Raw) {
    let wait = unsafe { &mut *wait.cast::<Wait>() };
    wait.handle = if status == REQUEST_DEVICE_STATUS_SUCCESS {
        device
    } else {
        ptr::null_mut()
    };
    wait.done = true;
}

unsafe extern "C" fn on_map(_: u32, _: StringView, wait: Raw, _: Raw) {
    unsafe { &mut *wait.cast::<Wait>() }.done = true;
}

unsafe extern "C" fn on_uncaptured_error(
    _: *const Raw,
    _: u32,
    message: StringView,
    _: Raw,
    _: Raw,
) {
    let text = if message.data.is_null() {
        c"uncaptured error".as_ptr()
    } else {
        message.data
    };
    unsafe {
        g_log(
            ptr::null(),
            G_LOG_LEVEL_WARNING,
            c"[webgpu] %.*s".as_ptr(),
            message.length as c_int,
            text,
        )
    };
}

pub(crate) fn request_adapter(instance: Raw) -> Option<Adapter> {
    let mut wait = Wait::new();
    let info = RequestAdapterCallbackInfo {
        next_in_chain: ptr::null(),
        mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
        callback: Some(on_adapter),
        userdata1: (&raw mut wait).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe { wgpuInstanceRequestAdapter(instance, ptr::null(), info) };
    for _ in 0..2000 {
        if wait.done {
            break;
        }
        unsafe { wgpuInstanceProcessEvents(instance) };
    }
    Adapter::from_raw(wait.handle)
}

pub(crate) struct Info {
    pub(crate) vendor: String,
    pub(crate) architecture: String,
    pub(crate) device: String,
    pub(crate) description: String,
}

impl Adapter {
    pub(crate) fn info(&self) -> Option<Info> {
        let mut info = AdapterInfo::zeroed();
        if unsafe { wgpuAdapterGetInfo(self.raw(), &mut info) } != STATUS_SUCCESS {
            return None;
        }
        let out = Info {
            vendor: view_text(&info.vendor),
            architecture: view_text(&info.architecture),
            device: view_text(&info.device),
            description: view_text(&info.description),
        };
        unsafe { wgpuAdapterInfoFreeMembers(info) };
        Some(out)
    }

    pub(crate) fn limits(&self) -> Limits {
        let mut limits = Limits::zeroed();
        unsafe { wgpuAdapterGetLimits(self.raw(), &mut limits) };
        limits
    }

    pub(crate) fn request_device(&self) -> Option<Device> {
        let mut wait = Wait::new();
        let info = RequestDeviceCallbackInfo {
            next_in_chain: ptr::null(),
            mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
            callback: Some(on_device),
            userdata1: (&raw mut wait).cast(),
            userdata2: ptr::null_mut(),
        };
        let mut descriptor = DeviceDescriptor::zeroed();
        descriptor.uncaptured_error_callback_info.callback = Some(on_uncaptured_error);
        unsafe { wgpuAdapterRequestDevice(self.raw(), &descriptor, info) };
        for _ in 0..2000 {
            if wait.done {
                break;
            }
            process_events();
        }
        Device::from_raw(wait.handle)
    }
}

impl Device {
    pub(crate) fn queue(&self) -> Option<Queue> {
        Queue::from_raw(unsafe { wgpuDeviceGetQueue(self.raw()) })
    }

    pub(crate) fn create_buffer(&self, descriptor: &BufferDescriptor) -> Option<Buffer> {
        Buffer::from_raw(unsafe { wgpuDeviceCreateBuffer(self.raw(), descriptor) })
    }

    pub(crate) fn create_command_encoder(&self) -> Option<CommandEncoder> {
        CommandEncoder::from_raw(unsafe { wgpuDeviceCreateCommandEncoder(self.raw(), ptr::null()) })
    }

    pub(crate) fn create_shader_module(&self, code: &str) -> Option<ShaderModule> {
        let mut source = ShaderSourceWGSL::zeroed();
        source.chain.s_type = S_TYPE_SHADER_SOURCE_WGSL;
        source.code = string_view(Some(code));
        let mut descriptor = ShaderModuleDescriptor::zeroed();
        descriptor.next_in_chain = (&raw const source).cast();
        ShaderModule::from_raw(unsafe { wgpuDeviceCreateShaderModule(self.raw(), &descriptor) })
    }

    pub(crate) fn create_render_pipeline(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Option<RenderPipeline> {
        RenderPipeline::from_raw(unsafe { wgpuDeviceCreateRenderPipeline(self.raw(), descriptor) })
    }

    pub(crate) fn create_compute_pipeline(
        &self,
        descriptor: &ComputePipelineDescriptor,
    ) -> Option<ComputePipeline> {
        ComputePipeline::from_raw(unsafe {
            wgpuDeviceCreateComputePipeline(self.raw(), descriptor)
        })
    }

    pub(crate) fn create_bind_group_layout(
        &self,
        descriptor: &BindGroupLayoutDescriptor,
    ) -> Option<BindGroupLayout> {
        BindGroupLayout::from_raw(unsafe {
            wgpuDeviceCreateBindGroupLayout(self.raw(), descriptor)
        })
    }

    pub(crate) fn create_pipeline_layout(
        &self,
        descriptor: &PipelineLayoutDescriptor,
    ) -> Option<PipelineLayout> {
        PipelineLayout::from_raw(unsafe { wgpuDeviceCreatePipelineLayout(self.raw(), descriptor) })
    }

    pub(crate) fn create_bind_group(&self, descriptor: &BindGroupDescriptor) -> Option<BindGroup> {
        BindGroup::from_raw(unsafe { wgpuDeviceCreateBindGroup(self.raw(), descriptor) })
    }

    pub(crate) fn create_sampler(&self, descriptor: &SamplerDescriptor) -> Option<Sampler> {
        Sampler::from_raw(unsafe { wgpuDeviceCreateSampler(self.raw(), descriptor) })
    }

    pub(crate) fn create_texture(&self, descriptor: &TextureDescriptor) -> Option<Texture> {
        Texture::from_raw(unsafe { wgpuDeviceCreateTexture(self.raw(), descriptor) })
    }

    pub(crate) fn create_query_set(&self, descriptor: &QuerySetDescriptor) -> Option<QuerySet> {
        QuerySet::from_raw(unsafe { wgpuDeviceCreateQuerySet(self.raw(), descriptor) })
    }

    fn poll(&self) {
        unsafe { wgpuDevicePoll(self.raw(), 1, ptr::null()) };
    }
}

impl Queue {
    pub(crate) fn write_buffer(&self, buffer: &Buffer, offset: u64, bytes: &[u8]) {
        unsafe {
            wgpuQueueWriteBuffer(
                self.raw(),
                buffer.raw(),
                offset,
                bytes.as_ptr().cast(),
                bytes.len(),
            )
        };
    }

    pub(crate) fn write_texture(
        &self,
        destination: &TexelCopyTextureInfo,
        bytes: &[u8],
        layout: &TexelCopyBufferLayout,
        size: &Extent3D,
    ) {
        unsafe {
            wgpuQueueWriteTexture(
                self.raw(),
                destination,
                bytes.as_ptr().cast(),
                bytes.len(),
                layout,
                size,
            )
        };
    }

    pub(crate) fn submit(&self, commands: &[&CommandBuffer]) {
        let raw: Vec<Raw> = commands.iter().map(|c| c.raw()).collect();
        unsafe { wgpuQueueSubmit(self.raw(), raw.len(), raw.as_ptr()) };
    }
}

impl Buffer {
    pub(crate) fn map_and_wait(
        &self,
        mode: u64,
        offset: usize,
        size: usize,
        device: Option<&Device>,
    ) -> bool {
        let mut wait = Wait::new();
        let info = BufferMapCallbackInfo {
            next_in_chain: ptr::null(),
            mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
            callback: Some(on_map),
            userdata1: (&raw mut wait).cast(),
            userdata2: ptr::null_mut(),
        };
        unsafe { wgpuBufferMapAsync(self.raw(), mode, offset, size, info) };
        for _ in 0..4000 {
            if wait.done {
                break;
            }
            if let Some(device) = device {
                device.poll();
            }
            process_events();
        }
        wait.done
    }

    pub(crate) fn mapped_array_buffer(
        &self,
        scope: &mut Scope<'_>,
        offset: usize,
        size: usize,
    ) -> Option<Result<Value, Value>> {
        let data = unsafe { wgpuBufferGetMappedRange(self.raw(), offset, size) };
        if data.is_null() {
            return None;
        }
        Some(unsafe { scope.external_array_buffer(data.cast(), size) })
    }

    pub(crate) fn with_const_mapped<R>(
        &self,
        size: usize,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        let data = unsafe { wgpuBufferGetConstMappedRange(self.raw(), 0, size) };
        if data.is_null() {
            return None;
        }
        let result = f(unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) });
        unsafe { wgpuBufferUnmap(self.raw()) };
        Some(result)
    }

    pub(crate) fn unmap(&self) {
        unsafe { wgpuBufferUnmap(self.raw()) };
    }

    pub(crate) fn destroy(&self) {
        unsafe { wgpuBufferDestroy(self.raw()) };
    }
}

impl Texture {
    pub(crate) fn create_view(
        &self,
        descriptor: Option<&TextureViewDescriptor>,
    ) -> Option<TextureView> {
        let descriptor = descriptor.map_or(ptr::null(), |d| d as *const TextureViewDescriptor);
        TextureView::from_raw(unsafe { wgpuTextureCreateView(self.raw(), descriptor) })
    }

    pub(crate) fn destroy(&self) {
        unsafe { wgpuTextureDestroy(self.raw()) };
    }
}

impl CommandEncoder {
    pub(crate) fn begin_render_pass(
        &self,
        descriptor: &RenderPassDescriptor,
    ) -> Option<RenderPass> {
        RenderPass::from_raw(unsafe { wgpuCommandEncoderBeginRenderPass(self.raw(), descriptor) })
    }

    pub(crate) fn begin_compute_pass(&self) -> Option<ComputePass> {
        ComputePass::from_raw(unsafe {
            wgpuCommandEncoderBeginComputePass(self.raw(), ptr::null())
        })
    }

    pub(crate) fn copy_texture_to_texture(
        &self,
        source: &TexelCopyTextureInfo,
        destination: &TexelCopyTextureInfo,
        size: &Extent3D,
    ) {
        unsafe { wgpuCommandEncoderCopyTextureToTexture(self.raw(), source, destination, size) };
    }

    pub(crate) fn copy_buffer_to_buffer(
        &self,
        source: &Buffer,
        source_offset: u64,
        destination: &Buffer,
        destination_offset: u64,
        size: u64,
    ) {
        unsafe {
            wgpuCommandEncoderCopyBufferToBuffer(
                self.raw(),
                source.raw(),
                source_offset,
                destination.raw(),
                destination_offset,
                size,
            )
        };
    }

    pub(crate) fn copy_texture_to_buffer(
        &self,
        source: &TexelCopyTextureInfo,
        destination: &TexelCopyBufferInfo,
        size: &Extent3D,
    ) {
        unsafe { wgpuCommandEncoderCopyTextureToBuffer(self.raw(), source, destination, size) };
    }

    pub(crate) fn resolve_query_set(
        &self,
        query_set: &QuerySet,
        first: u32,
        count: u32,
        destination: &Buffer,
        offset: u64,
    ) {
        unsafe {
            wgpuCommandEncoderResolveQuerySet(
                self.raw(),
                query_set.raw(),
                first,
                count,
                destination.raw(),
                offset,
            )
        };
    }

    pub(crate) fn finish(&self) -> Option<CommandBuffer> {
        CommandBuffer::from_raw(unsafe { wgpuCommandEncoderFinish(self.raw(), ptr::null()) })
    }
}

impl RenderPass {
    pub(crate) fn end(&self) {
        unsafe { wgpuRenderPassEncoderEnd(self.raw()) };
    }

    pub(crate) fn set_pipeline(&self, pipeline: &RenderPipeline) {
        unsafe { wgpuRenderPassEncoderSetPipeline(self.raw(), pipeline.raw()) };
    }

    pub(crate) fn set_vertex_buffer(&self, slot: u32, buffer: &Buffer, offset: u64, size: u64) {
        unsafe {
            wgpuRenderPassEncoderSetVertexBuffer(self.raw(), slot, buffer.raw(), offset, size)
        };
    }

    pub(crate) fn set_index_buffer(&self, buffer: &Buffer, format: u32, offset: u64, size: u64) {
        unsafe {
            wgpuRenderPassEncoderSetIndexBuffer(self.raw(), buffer.raw(), format, offset, size)
        };
    }

    pub(crate) fn set_bind_group(&self, index: u32, group: Option<&BindGroup>) {
        let group = group.map_or(ptr::null_mut(), BindGroup::raw);
        unsafe { wgpuRenderPassEncoderSetBindGroup(self.raw(), index, group, 0, ptr::null()) };
    }

    pub(crate) fn draw(&self, vertices: u32, instances: u32, first: u32, first_instance: u32) {
        unsafe {
            wgpuRenderPassEncoderDraw(self.raw(), vertices, instances, first, first_instance)
        };
    }

    pub(crate) fn draw_indexed(
        &self,
        indices: u32,
        instances: u32,
        first: u32,
        base_vertex: i32,
        first_instance: u32,
    ) {
        unsafe {
            wgpuRenderPassEncoderDrawIndexed(
                self.raw(),
                indices,
                instances,
                first,
                base_vertex,
                first_instance,
            )
        };
    }
}

impl ComputePass {
    pub(crate) fn set_pipeline(&self, pipeline: &ComputePipeline) {
        unsafe { wgpuComputePassEncoderSetPipeline(self.raw(), pipeline.raw()) };
    }

    pub(crate) fn set_bind_group(&self, index: u32, group: Option<&BindGroup>) {
        let group = group.map_or(ptr::null_mut(), BindGroup::raw);
        unsafe { wgpuComputePassEncoderSetBindGroup(self.raw(), index, group, 0, ptr::null()) };
    }

    pub(crate) fn dispatch(&self, x: u32, y: u32, z: u32) {
        unsafe { wgpuComputePassEncoderDispatchWorkgroups(self.raw(), x, y, z) };
    }

    pub(crate) fn end(&self) {
        unsafe { wgpuComputePassEncoderEnd(self.raw()) };
    }
}

impl RenderPipeline {
    pub(crate) fn bind_group_layout(&self, index: u32) -> Option<BindGroupLayout> {
        BindGroupLayout::from_raw(unsafe {
            wgpuRenderPipelineGetBindGroupLayout(self.raw(), index)
        })
    }
}

impl ComputePipeline {
    pub(crate) fn bind_group_layout(&self, index: u32) -> Option<BindGroupLayout> {
        BindGroupLayout::from_raw(unsafe {
            wgpuComputePipelineGetBindGroupLayout(self.raw(), index)
        })
    }
}

impl QuerySet {
    pub(crate) fn destroy(&self) {
        unsafe { wgpuQuerySetDestroy(self.raw()) };
    }
}
