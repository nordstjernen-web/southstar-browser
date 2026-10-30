//! Southstar — the wgpu-native entry points WebGPU calls, owned handles that release themselves, and the blocking waits on its callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::c_void;
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
}

/// # Safety
///
/// The all-zero bit pattern must be a valid value of the implementing type.
pub(crate) unsafe trait Zeroed: Copy {
    fn zeroed() -> Self {
        // SAFETY: the trait's contract is that all-zero bytes are a valid `Self`.
        unsafe { core::mem::zeroed() }
    }
}

macro_rules! zeroed {
    ($($t:ty),*) => {
        $(
            // SAFETY: the type is a #[repr(C)] struct of integers, floats, raw pointers, `Option` of fn pointers and other such structs, so zero is 0, null or None in every field.
            unsafe impl Zeroed for $t {}
        )*
    };
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
    // SAFETY: every view read here comes from wgpu-native (adapter info, error callbacks), which hands out `length` initialized bytes at `data` that stay valid until the info is freed or the callback returns, and this copies them before either.
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
                // SAFETY: the handle is non-null and this owner holds one reference to it, given up exactly once here.
                unsafe { $release(self.raw()) };
            }
        }
    };
    ($name:ident, $release:ident, $add_ref:ident) => {
        handle!($name, $release);

        impl Clone for $name {
            fn clone(&self) -> $name {
                // SAFETY: the handle is live while `self` holds its reference; the new owner's reference is released by its own Drop.
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

#[derive(Clone, Copy)]
pub(crate) struct Instance(NonNull<c_void>);

pub(crate) fn instance() -> Option<Instance> {
    let mut raw = INSTANCE.load(Ordering::Acquire);
    if raw.is_null() {
        // SAFETY: a null descriptor asks for the default instance; the result is checked for null and kept for the life of the process.
        raw = unsafe { wgpuCreateInstance(ptr::null()) };
        INSTANCE.store(raw, Ordering::Release);
    }
    NonNull::new(raw).map(Instance)
}

fn process_events() {
    if let Some(instance) = instance() {
        instance.process_events();
    }
}

struct Wait {
    done: Cell<bool>,
    handle: Cell<Raw>,
}

fn wait_for(start: impl FnOnce(Raw), mut pump: impl FnMut(), rounds: usize) -> Option<Raw> {
    let wait = Box::new(Wait {
        done: Cell::new(false),
        handle: Cell::new(ptr::null_mut()),
    });
    start(ptr::from_ref::<Wait>(&wait).cast_mut().cast());
    for _ in 0..rounds {
        if wait.done.get() {
            break;
        }
        pump();
    }
    if wait.done.get() {
        Some(wait.handle.get())
    } else {
        core::mem::forget(wait);
        None
    }
}

/// # Safety
///
/// `wait` must point at a `Wait` allocated by `wait_for`, which frees it only after this ran.
unsafe extern "C" fn on_request(status: u32, handle: Raw, _: StringView, wait: Raw, _: Raw) {
    // SAFETY: the caller's contract makes `wait` a live `Wait`; its fields are Cells, so a shared reference suffices.
    let wait = unsafe { &*wait.cast::<Wait>() };
    if status == REQUEST_STATUS_SUCCESS {
        wait.handle.set(handle);
    }
    wait.done.set(true);
}

/// # Safety
///
/// `wait` must point at a `Wait` allocated by `wait_for`, which frees it only after this ran.
unsafe extern "C" fn on_map(_: u32, _: StringView, wait: Raw, _: Raw) {
    // SAFETY: the caller's contract makes `wait` a live `Wait`; its fields are Cells, so a shared reference suffices.
    unsafe { &*wait.cast::<Wait>() }.done.set(true);
}

/// # Safety
///
/// `message` must be a string view wgpu-native keeps valid for the duration of the call.
unsafe extern "C" fn on_uncaptured_error(
    _: *const Raw,
    _: u32,
    message: StringView,
    _: Raw,
    _: Raw,
) {
    let text = view_text(&message);
    let text = if text.is_empty() {
        "uncaptured error"
    } else {
        &text
    };
    southstar_glib::warning(format!("[webgpu] {text}"));
}

impl Instance {
    fn process_events(self) {
        // SAFETY: the instance is non-null and never released, so it is live for the whole process.
        unsafe { wgpuInstanceProcessEvents(self.0.as_ptr()) };
    }

    pub(crate) fn request_adapter(self) -> Option<Adapter> {
        let handle = wait_for(
            |wait| {
                let info = RequestAdapterCallbackInfo {
                    next_in_chain: ptr::null(),
                    mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
                    callback: Some(on_request),
                    userdata1: wait,
                    userdata2: ptr::null_mut(),
                };
                // SAFETY: the instance is live, null options request the default adapter and `wait` is the boxed Wait `on_request` expects.
                unsafe { wgpuInstanceRequestAdapter(self.0.as_ptr(), ptr::null(), info) };
            },
            || self.process_events(),
            2000,
        );
        Adapter::from_raw(handle?)
    }
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
        // SAFETY: the adapter is live and `info` is a zeroed AdapterInfo wgpu-native fills in.
        if unsafe { wgpuAdapterGetInfo(self.raw(), &mut info) } != STATUS_SUCCESS {
            return None;
        }
        let out = Info {
            vendor: view_text(&info.vendor),
            architecture: view_text(&info.architecture),
            device: view_text(&info.device),
            description: view_text(&info.description),
        };
        // SAFETY: `info` was filled by a successful wgpuAdapterGetInfo and its members are freed once, after the strings were copied.
        unsafe { wgpuAdapterInfoFreeMembers(info) };
        Some(out)
    }

    pub(crate) fn limits(&self) -> Limits {
        let mut limits = Limits::zeroed();
        // SAFETY: the adapter is live and `limits` is a zeroed Limits with no extension chain for wgpu-native to fill.
        unsafe { wgpuAdapterGetLimits(self.raw(), &mut limits) };
        limits
    }

    pub(crate) fn request_device(&self) -> Option<Device> {
        let mut descriptor = DeviceDescriptor::zeroed();
        descriptor.uncaptured_error_callback_info.callback = Some(on_uncaptured_error);
        let handle = wait_for(
            |wait| {
                let info = RequestDeviceCallbackInfo {
                    next_in_chain: ptr::null(),
                    mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
                    callback: Some(on_request),
                    userdata1: wait,
                    userdata2: ptr::null_mut(),
                };
                // SAFETY: the adapter is live, the descriptor holds only null pointers and one callback, and `wait` is the boxed Wait `on_request` expects.
                unsafe { wgpuAdapterRequestDevice(self.raw(), &descriptor, info) };
            },
            process_events,
            2000,
        );
        Device::from_raw(handle?)
    }
}

impl Device {
    pub(crate) fn queue(&self) -> Option<Queue> {
        // SAFETY: the device is live; the queue comes back with its own reference, owned by the Queue.
        Queue::from_raw(unsafe { wgpuDeviceGetQueue(self.raw()) })
    }

    pub(crate) fn create_buffer(&self, descriptor: &BufferDescriptor) -> Option<Buffer> {
        // SAFETY: the device is live and the descriptor is plain data with a null chain and label.
        Buffer::from_raw(unsafe { wgpuDeviceCreateBuffer(self.raw(), descriptor) })
    }

    pub(crate) fn create_command_encoder(&self) -> Option<CommandEncoder> {
        // SAFETY: the device is live and a null descriptor asks for the defaults.
        CommandEncoder::from_raw(unsafe { wgpuDeviceCreateCommandEncoder(self.raw(), ptr::null()) })
    }

    pub(crate) fn create_shader_module(&self, code: &str) -> Option<ShaderModule> {
        let mut source = ShaderSourceWGSL::zeroed();
        source.chain.s_type = S_TYPE_SHADER_SOURCE_WGSL;
        source.code = string_view(Some(code));
        let mut descriptor = ShaderModuleDescriptor::zeroed();
        descriptor.next_in_chain = (&raw const source).cast();
        // SAFETY: the device is live, the descriptor chains to `source` on this frame and its view covers exactly `code`, all of which outlive the call.
        ShaderModule::from_raw(unsafe { wgpuDeviceCreateShaderModule(self.raw(), &descriptor) })
    }

    pub(crate) fn create_render_pipeline(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Option<RenderPipeline> {
        // SAFETY: the device is live; pipeline.rs builds the descriptor from its own arrays and held handles, with each count set to its array's length, and keeps them alive across this call.
        RenderPipeline::from_raw(unsafe { wgpuDeviceCreateRenderPipeline(self.raw(), descriptor) })
    }

    pub(crate) fn create_compute_pipeline(
        &self,
        descriptor: &ComputePipelineDescriptor,
    ) -> Option<ComputePipeline> {
        // SAFETY: the device is live; pipeline.rs builds the descriptor from a held module and layout and borrowed strings that outlive this call.
        ComputePipeline::from_raw(unsafe {
            wgpuDeviceCreateComputePipeline(self.raw(), descriptor)
        })
    }

    pub(crate) fn create_bind_group_layout(
        &self,
        descriptor: &BindGroupLayoutDescriptor,
    ) -> Option<BindGroupLayout> {
        // SAFETY: the device is live; the descriptor points at an entries array of at least `entry_count` entries that outlives this call.
        BindGroupLayout::from_raw(unsafe {
            wgpuDeviceCreateBindGroupLayout(self.raw(), descriptor)
        })
    }

    pub(crate) fn create_pipeline_layout(
        &self,
        descriptor: &PipelineLayoutDescriptor,
    ) -> Option<PipelineLayout> {
        // SAFETY: the device is live; the descriptor points at `bind_group_layout_count` non-null layouts the caller holds across this call.
        PipelineLayout::from_raw(unsafe { wgpuDeviceCreatePipelineLayout(self.raw(), descriptor) })
    }

    pub(crate) fn create_bind_group(&self, descriptor: &BindGroupDescriptor) -> Option<BindGroup> {
        // SAFETY: the device is live; the descriptor's layout and its `entry_count` entries, with the buffers, views and samplers they name, are held by the caller across this call.
        BindGroup::from_raw(unsafe { wgpuDeviceCreateBindGroup(self.raw(), descriptor) })
    }

    pub(crate) fn create_sampler(&self, descriptor: &SamplerDescriptor) -> Option<Sampler> {
        // SAFETY: the device is live and the descriptor is plain data with a null chain and label.
        Sampler::from_raw(unsafe { wgpuDeviceCreateSampler(self.raw(), descriptor) })
    }

    pub(crate) fn create_texture(&self, descriptor: &TextureDescriptor) -> Option<Texture> {
        // SAFETY: the device is live and the descriptor is plain data with a null chain, label and view-format list.
        Texture::from_raw(unsafe { wgpuDeviceCreateTexture(self.raw(), descriptor) })
    }

    pub(crate) fn create_query_set(&self, descriptor: &QuerySetDescriptor) -> Option<QuerySet> {
        // SAFETY: the device is live and the descriptor is plain data with a null chain and label.
        QuerySet::from_raw(unsafe { wgpuDeviceCreateQuerySet(self.raw(), descriptor) })
    }

    fn poll(&self) {
        // SAFETY: the device is live and a null submission index waits for all submitted work.
        unsafe { wgpuDevicePoll(self.raw(), 1, ptr::null()) };
    }
}

impl Queue {
    pub(crate) fn write_buffer(&self, buffer: &Buffer, offset: u64, bytes: &[u8]) {
        // SAFETY: queue and buffer are live and the pointer and length are those of `bytes`, which wgpu-native copies before returning.
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
        // SAFETY: the queue and the destination's held texture are live, the pointer and length are those of `bytes`, and wgpu-native checks the layout against that length and copies before returning.
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
        // SAFETY: the queue is live and `raw` holds `raw.len()` live command buffers borrowed from `commands`.
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
        wait_for(
            |wait| {
                let info = BufferMapCallbackInfo {
                    next_in_chain: ptr::null(),
                    mode: CALLBACK_MODE_ALLOW_PROCESS_EVENTS,
                    callback: Some(on_map),
                    userdata1: wait,
                    userdata2: ptr::null_mut(),
                };
                // SAFETY: the buffer is live, wgpu-native validates the range, and `wait` is the boxed Wait `on_map` expects.
                unsafe { wgpuBufferMapAsync(self.raw(), mode, offset, size, info) };
            },
            || {
                if let Some(device) = device {
                    device.poll();
                }
                process_events();
            },
            4000,
        )
        .is_some()
    }

    pub(crate) fn mapped_array_buffer(
        &self,
        scope: &mut Scope<'_>,
        offset: usize,
        size: usize,
    ) -> Option<Result<Value, Value>> {
        // SAFETY: the buffer is live; wgpu-native returns null unless `offset..offset + size` lies inside the mapped range.
        let data = unsafe { wgpuBufferGetMappedRange(self.raw(), offset, size) };
        if data.is_null() {
            return None;
        }
        // SAFETY: `data` addresses `size` mapped bytes; gpu.rs records the ArrayBuffer and detaches it before unmapping or destroying the buffer, and BufferState leaks the buffer rather than release it while a range is outstanding.
        Some(unsafe { scope.external_array_buffer(data.cast(), size, None) })
    }

    pub(crate) fn with_const_mapped<R>(
        &self,
        size: usize,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        // SAFETY: the buffer is live; wgpu-native returns null unless `0..size` lies inside the mapped range.
        let data = unsafe { wgpuBufferGetConstMappedRange(self.raw(), 0, size) };
        if data.is_null() {
            return None;
        }
        // SAFETY: `data` addresses `size` mapped bytes that stay mapped until the unmap below, after the slice's last use.
        let result = f(unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) });
        self.unmap();
        Some(result)
    }

    pub(crate) fn unmap(&self) {
        // SAFETY: the buffer is live; unmapping an unmapped buffer is a validation error, not undefined behavior.
        unsafe { wgpuBufferUnmap(self.raw()) };
    }

    pub(crate) fn destroy(&self) {
        // SAFETY: the buffer is live; destroying frees its memory but keeps the handle valid until released.
        unsafe { wgpuBufferDestroy(self.raw()) };
    }
}

impl Texture {
    pub(crate) fn create_view(
        &self,
        descriptor: Option<&TextureViewDescriptor>,
    ) -> Option<TextureView> {
        let descriptor = descriptor.map_or(ptr::null(), |d| d as *const TextureViewDescriptor);
        // SAFETY: the texture is live and the descriptor is null or plain data with a null chain and label.
        TextureView::from_raw(unsafe { wgpuTextureCreateView(self.raw(), descriptor) })
    }

    pub(crate) fn destroy(&self) {
        // SAFETY: the texture is live; destroying frees its memory but keeps the handle valid until released.
        unsafe { wgpuTextureDestroy(self.raw()) };
    }
}

impl CommandEncoder {
    pub(crate) fn begin_render_pass(
        &self,
        descriptor: &RenderPassDescriptor,
    ) -> Option<RenderPass> {
        // SAFETY: the encoder is live; encode.rs points the descriptor at `color_attachment_count` attachments and an optional depth attachment on its frame, whose views it holds across this call.
        RenderPass::from_raw(unsafe { wgpuCommandEncoderBeginRenderPass(self.raw(), descriptor) })
    }

    pub(crate) fn begin_compute_pass(&self) -> Option<ComputePass> {
        // SAFETY: the encoder is live and a null descriptor asks for the defaults.
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
        // SAFETY: the encoder and the textures the caller holds in both infos are live; the infos and extent are plain data.
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
        // SAFETY: the encoder and both buffers are live; wgpu-native validates the ranges.
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
        // SAFETY: the encoder and the texture and buffer the caller holds in the infos are live; the infos and extent are plain data.
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
        // SAFETY: the encoder, query set and buffer are live; wgpu-native validates the query and buffer ranges.
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
        // SAFETY: the encoder is live and a null descriptor asks for the defaults.
        CommandBuffer::from_raw(unsafe { wgpuCommandEncoderFinish(self.raw(), ptr::null()) })
    }
}

impl RenderPass {
    pub(crate) fn end(&self) {
        // SAFETY: the pass is live; ending it twice is a validation error, not undefined behavior.
        unsafe { wgpuRenderPassEncoderEnd(self.raw()) };
    }

    pub(crate) fn set_pipeline(&self, pipeline: &RenderPipeline) {
        // SAFETY: the pass and the pipeline are live handles.
        unsafe { wgpuRenderPassEncoderSetPipeline(self.raw(), pipeline.raw()) };
    }

    pub(crate) fn set_vertex_buffer(&self, slot: u32, buffer: &Buffer, offset: u64, size: u64) {
        // SAFETY: the pass and the buffer are live; wgpu-native validates slot and range.
        unsafe {
            wgpuRenderPassEncoderSetVertexBuffer(self.raw(), slot, buffer.raw(), offset, size)
        };
    }

    pub(crate) fn set_index_buffer(&self, buffer: &Buffer, format: u32, offset: u64, size: u64) {
        // SAFETY: the pass and the buffer are live; wgpu-native validates format and range.
        unsafe {
            wgpuRenderPassEncoderSetIndexBuffer(self.raw(), buffer.raw(), format, offset, size)
        };
    }

    pub(crate) fn set_bind_group(&self, index: u32, group: Option<&BindGroup>) {
        let group = group.map_or(ptr::null_mut(), BindGroup::raw);
        // SAFETY: the pass is live, the group is live or null (which unbinds), and zero dynamic offsets come with a null array.
        unsafe { wgpuRenderPassEncoderSetBindGroup(self.raw(), index, group, 0, ptr::null()) };
    }

    pub(crate) fn draw(&self, vertices: u32, instances: u32, first: u32, first_instance: u32) {
        // SAFETY: the pass is live and the arguments are plain counts wgpu-native validates.
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
        // SAFETY: the pass is live and the arguments are plain counts wgpu-native validates.
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
        // SAFETY: the pass and the pipeline are live handles.
        unsafe { wgpuComputePassEncoderSetPipeline(self.raw(), pipeline.raw()) };
    }

    pub(crate) fn set_bind_group(&self, index: u32, group: Option<&BindGroup>) {
        let group = group.map_or(ptr::null_mut(), BindGroup::raw);
        // SAFETY: the pass is live, the group is live or null (which unbinds), and zero dynamic offsets come with a null array.
        unsafe { wgpuComputePassEncoderSetBindGroup(self.raw(), index, group, 0, ptr::null()) };
    }

    pub(crate) fn dispatch(&self, x: u32, y: u32, z: u32) {
        // SAFETY: the pass is live and the workgroup counts are plain values wgpu-native validates.
        unsafe { wgpuComputePassEncoderDispatchWorkgroups(self.raw(), x, y, z) };
    }

    pub(crate) fn end(&self) {
        // SAFETY: the pass is live; ending it twice is a validation error, not undefined behavior.
        unsafe { wgpuComputePassEncoderEnd(self.raw()) };
    }
}

impl RenderPipeline {
    pub(crate) fn bind_group_layout(&self, index: u32) -> Option<BindGroupLayout> {
        // SAFETY: the pipeline is live; the layout comes back with its own reference, owned by the BindGroupLayout.
        BindGroupLayout::from_raw(unsafe {
            wgpuRenderPipelineGetBindGroupLayout(self.raw(), index)
        })
    }
}

impl ComputePipeline {
    pub(crate) fn bind_group_layout(&self, index: u32) -> Option<BindGroupLayout> {
        // SAFETY: the pipeline is live; the layout comes back with its own reference, owned by the BindGroupLayout.
        BindGroupLayout::from_raw(unsafe {
            wgpuComputePipelineGetBindGroupLayout(self.raw(), index)
        })
    }
}

impl QuerySet {
    pub(crate) fn destroy(&self) {
        // SAFETY: the query set is live; destroying frees its storage but keeps the handle valid until released.
        unsafe { wgpuQuerySetDestroy(self.raw()) };
    }
}
