//! Southstar — Media Foundation's entry points loaded from mfplat.dll at runtime, and owners of the COM interfaces the decoders call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};
use core::ptr::{self, NonNull};
use std::cell::Cell;
use std::sync::OnceLock;

pub type Hresult = i32;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Guid(u32, u16, u16, [u8; 8]);

const fn media_subtype(fourcc: u32) -> Guid {
    Guid(
        fourcc,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71],
    )
}

pub const IID_IMF_TRANSFORM: Guid = Guid(
    0xbf94c121,
    0x5b05,
    0x4e6f,
    [0x80, 0x00, 0xba, 0x59, 0x89, 0x61, 0x41, 0x4d],
);
pub const IID_IMF_2D_BUFFER: Guid = Guid(
    0x7dc9d5f9,
    0x9ed9,
    0x44ec,
    [0x9b, 0xbf, 0x06, 0x00, 0xbb, 0x58, 0x9f, 0xbb],
);
pub const CATEGORY_VIDEO_DECODER: Guid = Guid(
    0xd6c02d4b,
    0x6833,
    0x45b4,
    [0x97, 0x1a, 0x05, 0xa4, 0xb0, 0x4b, 0xab, 0x91],
);
pub const CATEGORY_AUDIO_DECODER: Guid = Guid(
    0x9ea73fb4,
    0xef7a,
    0x4559,
    [0x8d, 0x5d, 0x71, 0x9d, 0x8f, 0x04, 0x26, 0xc7],
);
pub const MEDIA_TYPE_VIDEO: Guid = media_subtype(0x73646976);
pub const MEDIA_TYPE_AUDIO: Guid = media_subtype(0x73647561);
pub const VIDEO_FORMAT_H264: Guid = media_subtype(0x34363248);
pub const VIDEO_FORMAT_NV12: Guid = media_subtype(0x3231564e);
pub const AUDIO_FORMAT_AAC: Guid = media_subtype(0x1610);
pub const AUDIO_FORMAT_FLOAT: Guid = media_subtype(0x0003);
pub const AUDIO_FORMAT_PCM: Guid = media_subtype(0x0001);
pub const MT_MAJOR_TYPE: Guid = Guid(
    0x48eba18e,
    0xf8c9,
    0x4687,
    [0xbf, 0x11, 0x0a, 0x74, 0xc9, 0xf9, 0x6a, 0x8f],
);
pub const MT_SUBTYPE: Guid = Guid(
    0xf7e34c9a,
    0x42e8,
    0x4714,
    [0xb7, 0x4b, 0xcb, 0x29, 0xd7, 0x2c, 0x35, 0xe5],
);
pub const MT_FRAME_SIZE: Guid = Guid(
    0x1652c33d,
    0xd6b2,
    0x4012,
    [0xb8, 0x34, 0x72, 0x03, 0x08, 0x49, 0xa3, 0x7d],
);
pub const MT_INTERLACE_MODE: Guid = Guid(
    0xe2724bb8,
    0xe676,
    0x4806,
    [0xb4, 0xb2, 0xa8, 0xd6, 0xef, 0xb4, 0x4c, 0xcd],
);
pub const MT_MINIMUM_DISPLAY_APERTURE: Guid = Guid(
    0xd7388766,
    0x18fe,
    0x48c6,
    [0xa1, 0x77, 0xee, 0x89, 0x48, 0x67, 0xc8, 0xc4],
);
pub const MT_AUDIO_SAMPLES_PER_SECOND: Guid = Guid(
    0x5faeeae7,
    0x0290,
    0x4c31,
    [0x9e, 0x8a, 0xc5, 0x34, 0xf6, 0x8d, 0x9d, 0xba],
);
pub const MT_AUDIO_NUM_CHANNELS: Guid = Guid(
    0x37e48bf5,
    0x645e,
    0x4c5b,
    [0x89, 0xde, 0xad, 0xa9, 0xe2, 0x9b, 0x69, 0x6a],
);
pub const MT_AUDIO_BITS_PER_SAMPLE: Guid = Guid(
    0xf2deb57f,
    0x40fa,
    0x4764,
    [0xaa, 0x33, 0xed, 0x4f, 0x2d, 0x1f, 0xf6, 0x69],
);
pub const MT_AAC_PAYLOAD_TYPE: Guid = Guid(
    0xbfbabe79,
    0x7434,
    0x4d1c,
    [0x94, 0xf0, 0x72, 0xa3, 0xb9, 0xe1, 0x71, 0x88],
);
pub const MT_USER_DATA: Guid = Guid(
    0xb6bc765f,
    0x4c3b,
    0x40a4,
    [0xbd, 0x51, 0x25, 0x35, 0xb6, 0x6f, 0xe0, 0x9d],
);
pub const SAMPLE_DISCONTINUITY: Guid = Guid(
    0x9cdf01d9,
    0xa0f0,
    0x43ba,
    [0xb0, 0x77, 0xea, 0xa0, 0x6c, 0xbd, 0x72, 0x8a],
);

pub const E_NEED_MORE_INPUT: Hresult = 0xc00d6d72_u32 as Hresult;
pub const E_STREAM_CHANGE: Hresult = 0xc00d6d61_u32 as Hresult;
pub const E_NOT_ACCEPTING: Hresult = 0xc00d36b5_u32 as Hresult;
const E_CHANGED_MODE: Hresult = 0x80010106_u32 as Hresult;
const MF_VERSION: u32 = 0x0002_0070;
const MFSTARTUP_LITE: u32 = 1;
const COINIT_MULTITHREADED: u32 = 0;
const ENUM_FLAGS: u32 = 0x01 | 0x10 | 0x40;

pub const MESSAGE_FLUSH: u32 = 0;
pub const MESSAGE_DRAIN: u32 = 1;
pub const MESSAGE_BEGIN_STREAMING: u32 = 0x1000_0000;
pub const MESSAGE_END_STREAMING: u32 = 0x1000_0001;
pub const MESSAGE_START_OF_STREAM: u32 = 0x1000_0003;
pub const OUTPUT_PROVIDES_SAMPLES: u32 = 0x100 | 0x200;
pub const INTERLACE_MIXED: u32 = 7;

pub fn succeeded(hr: Hresult) -> bool {
    hr >= 0
}

#[repr(C)]
struct TypeInfo {
    major: Guid,
    subtype: Guid,
}

#[repr(C)]
#[derive(Default)]
pub struct OutputStreamInfo {
    pub flags: u32,
    pub size: u32,
    pub alignment: u32,
}

#[repr(C)]
pub struct OutputDataBuffer {
    stream_id: u32,
    sample: *mut c_void,
    status: u32,
    events: *mut c_void,
}

#[repr(C)]
#[derive(Default)]
pub struct VideoArea {
    pub offset_x: i32,
    pub offset_y: i32,
    pub width: i32,
    pub height: i32,
}

type Startup = unsafe extern "system" fn(u32, u32) -> Hresult;
type EnumEx = unsafe extern "system" fn(
    Guid,
    u32,
    *const TypeInfo,
    *const TypeInfo,
    *mut *mut *mut c_void,
    *mut u32,
) -> Hresult;
type Create = unsafe extern "system" fn(*mut *mut c_void) -> Hresult;
type CreateBuffer = unsafe extern "system" fn(u32, *mut *mut c_void) -> Hresult;
type CoInitializeEx = unsafe extern "system" fn(*mut c_void, u32) -> Hresult;
type CoTaskMemFree = unsafe extern "system" fn(*mut c_void);

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

struct Api {
    enum_ex: EnumEx,
    create_media_type: Create,
    create_sample: Create,
    create_memory_buffer: CreateBuffer,
    co_initialize: CoInitializeEx,
    co_task_mem_free: CoTaskMemFree,
}

fn load_library(name: &str) -> Option<NonNull<c_void>> {
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    // SAFETY: wide is a NUL-terminated UTF-16 module name that outlives the call.
    NonNull::new(unsafe { LoadLibraryW(wide.as_ptr()) })
}

fn symbol(module: NonNull<c_void>, name: &CStr) -> Option<NonNull<c_void>> {
    // SAFETY: module is a loaded module handle and name a NUL-terminated export name.
    NonNull::new(unsafe { GetProcAddress(module.as_ptr(), name.as_ptr().cast()) })
}

macro_rules! entry {
    ($module:expr, $name:expr, $type:ty) => {{
        let address = symbol($module, $name)?;
        // SAFETY: the export named here has exactly this signature in the Windows SDK headers.
        unsafe { core::mem::transmute::<*mut c_void, $type>(address.as_ptr()) }
    }};
}

fn load() -> Option<Api> {
    let mfplat = load_library("mfplat.dll")?;
    let ole32 = load_library("ole32.dll")?;
    let startup = entry!(mfplat, c"MFStartup", Startup);
    let api = Api {
        enum_ex: entry!(mfplat, c"MFTEnumEx", EnumEx),
        create_media_type: entry!(mfplat, c"MFCreateMediaType", Create),
        create_sample: entry!(mfplat, c"MFCreateSample", Create),
        create_memory_buffer: entry!(mfplat, c"MFCreateMemoryBuffer", CreateBuffer),
        co_initialize: entry!(ole32, c"CoInitializeEx", CoInitializeEx),
        co_task_mem_free: entry!(ole32, c"CoTaskMemFree", CoTaskMemFree),
    };
    if !initialize_thread(&api) {
        return None;
    }
    // SAFETY: MFStartup takes the SDK version and a flags word; COM is initialized on this thread.
    let started = unsafe { startup(MF_VERSION, MFSTARTUP_LITE) };
    succeeded(started).then_some(api)
}

thread_local! {
    static THREAD_READY: Cell<bool> = const { Cell::new(false) };
}

fn initialize_thread(api: &Api) -> bool {
    if THREAD_READY.get() {
        return true;
    }
    // SAFETY: CoInitializeEx takes a reserved null pointer and the apartment flags.
    let hr = unsafe { (api.co_initialize)(ptr::null_mut(), COINIT_MULTITHREADED) };
    let ready = succeeded(hr) || hr == E_CHANGED_MODE;
    THREAD_READY.set(ready);
    ready
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    let api = API.get_or_init(load).as_ref()?;
    initialize_thread(api).then_some(api)
}

pub struct Unknown(NonNull<c_void>);

impl Unknown {
    /// # Safety
    ///
    /// `raw` must be null or a COM interface pointer whose reference the new value takes over.
    unsafe fn adopt(raw: *mut c_void) -> Option<Unknown> {
        NonNull::new(raw).map(Unknown)
    }

    fn raw(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    /// # Safety
    ///
    /// `index` must be a slot of this interface's vtable, and `F` the exact function-pointer type stored there.
    unsafe fn method<F: Copy>(&self, index: usize) -> F {
        let vtable = self.0.as_ptr().cast::<*const *const c_void>();
        // SAFETY: a COM interface pointer points at its vtable pointer.
        let table = unsafe { vtable.read() };
        // SAFETY: the caller guarantees index lies within the vtable.
        let entry = unsafe { table.wrapping_add(index).read() };
        // SAFETY: the caller guarantees F is the slot's function-pointer type, which has the size of a pointer.
        unsafe { core::mem::transmute_copy::<*const c_void, F>(&entry) }
    }

    fn query(&self, iid: &Guid) -> Option<Unknown> {
        type QueryInterface =
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult;
        // SAFETY: slot 0 of every COM vtable is QueryInterface.
        let query: QueryInterface = unsafe { self.method(0) };
        let mut out = ptr::null_mut();
        // SAFETY: self is a live interface, iid a GUID and out receives a referenced pointer or null.
        let hr = unsafe { query(self.raw(), iid, &mut out) };
        // SAFETY: on success out holds a reference QueryInterface handed to us.
        succeeded(hr)
            .then(|| unsafe { Unknown::adopt(out) })
            .flatten()
    }
}

impl Drop for Unknown {
    fn drop(&mut self) {
        type Release = unsafe extern "system" fn(*mut c_void) -> u32;
        // SAFETY: slot 2 of every COM vtable is Release.
        let release: Release = unsafe { self.method(2) };
        // SAFETY: self holds one reference to a live interface, released exactly once.
        unsafe { release(self.raw()) };
    }
}

macro_rules! call {
    ($self:expr, $slot:expr, fn($($arg:ty),*) -> $ret:ty, $($value:expr),*) => {{
        type Method = unsafe extern "system" fn(*mut c_void $(, $arg)*) -> $ret;
        // SAFETY: the slot number and signature were checked against the Windows SDK vtable layout for this interface.
        let method: Method = unsafe { $self.0.method($slot) };
        // SAFETY: self is a live interface of the type this slot belongs to, and the arguments match the signature.
        unsafe { method($self.0.raw() $(, $value)*) }
    }};
}

pub trait Attributes {
    fn unknown(&self) -> &Unknown;

    fn get_u32(&self, key: &Guid) -> Option<u32> {
        let this = Wrap(self.unknown());
        let mut value = 0;
        let hr = call!(
            this,
            7,
            fn(*const Guid, *mut u32) -> Hresult,
            key,
            &mut value
        );
        succeeded(hr).then_some(value)
    }

    fn get_u64(&self, key: &Guid) -> Option<u64> {
        let this = Wrap(self.unknown());
        let mut value = 0;
        let hr = call!(
            this,
            8,
            fn(*const Guid, *mut u64) -> Hresult,
            key,
            &mut value
        );
        succeeded(hr).then_some(value)
    }

    fn get_guid(&self, key: &Guid) -> Option<Guid> {
        let this = Wrap(self.unknown());
        let mut value = Guid(0, 0, 0, [0; 8]);
        let hr = call!(
            this,
            10,
            fn(*const Guid, *mut Guid) -> Hresult,
            key,
            &mut value
        );
        succeeded(hr).then_some(value)
    }

    fn get_area(&self, key: &Guid) -> Option<VideoArea> {
        let this = Wrap(self.unknown());
        let mut area = VideoArea::default();
        let mut size = 0u32;
        let length = core::mem::size_of::<VideoArea>() as u32;
        let hr = call!(
            this,
            15,
            fn(*const Guid, *mut u8, u32, *mut u32) -> Hresult,
            key,
            (&raw mut area).cast(),
            length,
            &mut size
        );
        (succeeded(hr) && size == length).then_some(area)
    }

    fn set_u32(&self, key: &Guid, value: u32) {
        let this = Wrap(self.unknown());
        call!(this, 21, fn(*const Guid, u32) -> Hresult, key, value);
    }

    fn set_u64(&self, key: &Guid, value: u64) {
        let this = Wrap(self.unknown());
        call!(this, 22, fn(*const Guid, u64) -> Hresult, key, value);
    }

    fn set_guid(&self, key: &Guid, value: &Guid) {
        let this = Wrap(self.unknown());
        call!(
            this,
            24,
            fn(*const Guid, *const Guid) -> Hresult,
            key,
            value
        );
    }

    fn set_blob(&self, key: &Guid, value: &[u8]) {
        let this = Wrap(self.unknown());
        let length = value.len() as u32;
        call!(
            this,
            26,
            fn(*const Guid, *const u8, u32) -> Hresult,
            key,
            value.as_ptr(),
            length
        );
    }
}

struct Wrap<'a>(&'a Unknown);

pub struct MediaType(Unknown);

impl Attributes for MediaType {
    fn unknown(&self) -> &Unknown {
        &self.0
    }
}

impl MediaType {
    pub fn new() -> Option<MediaType> {
        let api = api()?;
        let mut out = ptr::null_mut();
        // SAFETY: MFCreateMediaType writes a referenced IMFMediaType pointer into out.
        let hr = unsafe { (api.create_media_type)(&mut out) };
        // SAFETY: on success out holds the new media type's reference.
        succeeded(hr)
            .then(|| unsafe { Unknown::adopt(out) })
            .flatten()
            .map(MediaType)
    }
}

pub struct Sample(Unknown);

impl Attributes for Sample {
    fn unknown(&self) -> &Unknown {
        &self.0
    }
}

impl Sample {
    pub fn with_buffer(buffer: &Buffer) -> Option<Sample> {
        let api = api()?;
        let mut out = ptr::null_mut();
        // SAFETY: MFCreateSample writes a referenced IMFSample pointer into out.
        let hr = unsafe { (api.create_sample)(&mut out) };
        // SAFETY: on success out holds the new sample's reference.
        let sample = Sample(
            succeeded(hr)
                .then(|| unsafe { Unknown::adopt(out) })
                .flatten()?,
        );
        let added = call!(sample, 42, fn(*mut c_void) -> Hresult, buffer.0.raw());
        succeeded(added).then_some(sample)
    }

    pub fn time(&self) -> Option<i64> {
        let mut time = 0i64;
        let hr = call!(self, 35, fn(*mut i64) -> Hresult, &mut time);
        succeeded(hr).then_some(time)
    }

    pub fn set_time(&self, time: i64) {
        call!(self, 36, fn(i64) -> Hresult, time);
    }

    pub fn contiguous(&self) -> Option<Buffer> {
        let mut out = ptr::null_mut();
        let hr = call!(self, 41, fn(*mut *mut c_void) -> Hresult, &mut out);
        // SAFETY: on success out holds a referenced IMFMediaBuffer.
        succeeded(hr)
            .then(|| unsafe { Unknown::adopt(out) })
            .flatten()
            .map(Buffer)
    }
}

pub struct Buffer(Unknown);

impl Buffer {
    pub fn new(capacity: u32) -> Option<Buffer> {
        let api = api()?;
        let mut out = ptr::null_mut();
        // SAFETY: MFCreateMemoryBuffer writes a referenced IMFMediaBuffer of capacity bytes into out.
        let hr = unsafe { (api.create_memory_buffer)(capacity, &mut out) };
        // SAFETY: on success out holds the new buffer's reference.
        succeeded(hr)
            .then(|| unsafe { Unknown::adopt(out) })
            .flatten()
            .map(Buffer)
    }

    pub fn filled(parts: &[&[u8]]) -> Option<Buffer> {
        let total: usize = parts.iter().map(|part| part.len()).sum();
        let capacity = u32::try_from(total).ok().filter(|&total| total > 0)?;
        let buffer = Buffer::new(capacity)?;
        let written = buffer.with_locked(|bytes| {
            let mut at = 0;
            for part in parts {
                bytes[at..at + part.len()].copy_from_slice(part);
                at += part.len();
            }
        });
        written?;
        call!(buffer, 6, fn(u32) -> Hresult, capacity);
        Some(buffer)
    }

    fn with_locked<R>(&self, use_bytes: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
        let mut base = ptr::null_mut();
        let mut capacity = 0u32;
        let mut current = 0u32;
        let hr = call!(
            self,
            3,
            fn(*mut *mut u8, *mut u32, *mut u32) -> Hresult,
            &mut base,
            &mut capacity,
            &mut current
        );
        if !succeeded(hr) || base.is_null() {
            return None;
        }
        // SAFETY: Lock returned base addressing capacity bytes that stay valid and exclusively ours until Unlock.
        let bytes = unsafe { core::slice::from_raw_parts_mut(base, capacity as usize) };
        let result = use_bytes(bytes);
        call!(self, 4, fn() -> Hresult,);
        Some(result)
    }

    pub fn read<R>(&self, use_bytes: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let mut base = ptr::null_mut();
        let mut current = 0u32;
        let hr = call!(
            self,
            3,
            fn(*mut *mut u8, *mut u32, *mut u32) -> Hresult,
            &mut base,
            ptr::null_mut(),
            &mut current
        );
        if !succeeded(hr) || base.is_null() {
            return None;
        }
        // SAFETY: Lock returned base addressing at least current valid bytes, readable until Unlock.
        let bytes = unsafe { core::slice::from_raw_parts(base, current as usize) };
        let result = use_bytes(bytes);
        call!(self, 4, fn() -> Hresult,);
        Some(result)
    }

    pub fn two_dimensional(&self) -> Option<Buffer2d> {
        self.0.query(&IID_IMF_2D_BUFFER).map(Buffer2d)
    }
}

pub struct Buffer2d(Unknown);

impl Buffer2d {
    pub fn read<R>(&self, rows: usize, use_bytes: impl FnOnce(&[u8], usize) -> R) -> Option<R> {
        let mut base = ptr::null_mut();
        let mut pitch = 0i32;
        let hr = call!(
            self,
            3,
            fn(*mut *mut u8, *mut i32) -> Hresult,
            &mut base,
            &mut pitch
        );
        if !succeeded(hr) {
            return None;
        }
        let result = usize::try_from(pitch)
            .ok()
            .filter(|&pitch| pitch > 0 && !base.is_null())
            .map(|pitch| {
                // SAFETY: Lock2D returned a top-down surface of pitch-byte rows starting at base; the frame's rows rows are readable until Unlock2D.
                let bytes = unsafe { core::slice::from_raw_parts(base, pitch * rows) };
                use_bytes(bytes, pitch)
            });
        call!(self, 4, fn() -> Hresult,);
        result
    }
}

pub struct Transform(Unknown);

impl Transform {
    pub fn find(category: &Guid, major: &Guid, subtype: &Guid) -> Option<Transform> {
        let mut found = None;
        each_activator(category, major, subtype, |activate| {
            if found.is_none() {
                let mut out = ptr::null_mut();
                let hr = call!(
                    activate,
                    33,
                    fn(*const Guid, *mut *mut c_void) -> Hresult,
                    &IID_IMF_TRANSFORM,
                    &mut out
                );
                if succeeded(hr) {
                    // SAFETY: on success ActivateObject wrote a referenced IMFTransform.
                    found = unsafe { Unknown::adopt(out) }.map(Transform);
                }
            }
        })?;
        found
    }

    pub fn output_stream_info(&self) -> OutputStreamInfo {
        let mut info = OutputStreamInfo::default();
        call!(
            self,
            7,
            fn(u32, *mut OutputStreamInfo) -> Hresult,
            0,
            &mut info
        );
        info
    }

    pub fn output_type(&self, index: u32) -> Option<MediaType> {
        let mut out = ptr::null_mut();
        let hr = call!(
            self,
            14,
            fn(u32, u32, *mut *mut c_void) -> Hresult,
            0,
            index,
            &mut out
        );
        // SAFETY: on success out holds a referenced IMFMediaType.
        succeeded(hr)
            .then(|| unsafe { Unknown::adopt(out) })
            .flatten()
            .map(MediaType)
    }

    pub fn set_input_type(&self, media_type: &MediaType) -> bool {
        let hr = call!(
            self,
            15,
            fn(u32, *mut c_void, u32) -> Hresult,
            0,
            media_type.0.raw(),
            0
        );
        succeeded(hr)
    }

    pub fn set_output_type(&self, media_type: &MediaType) -> bool {
        let hr = call!(
            self,
            16,
            fn(u32, *mut c_void, u32) -> Hresult,
            0,
            media_type.0.raw(),
            0
        );
        succeeded(hr)
    }

    pub fn message(&self, message: u32) {
        call!(self, 23, fn(u32, usize) -> Hresult, message, 0);
    }

    pub fn process_input(&self, sample: &Sample) -> Hresult {
        call!(
            self,
            24,
            fn(u32, *mut c_void, u32) -> Hresult,
            0,
            sample.0.raw(),
            0
        )
    }

    pub fn process_output(&self, provided: Option<&Sample>) -> (Hresult, Option<Sample>) {
        let mut buffer = OutputDataBuffer {
            stream_id: 0,
            sample: provided.map_or(ptr::null_mut(), |sample| sample.0.raw()),
            status: 0,
            events: ptr::null_mut(),
        };
        let mut status = 0u32;
        let hr = call!(
            self,
            25,
            fn(u32, u32, *mut OutputDataBuffer, *mut u32) -> Hresult,
            0,
            1,
            &mut buffer,
            &mut status
        );
        // SAFETY: an event collection ProcessOutput returns carries a reference the caller releases.
        drop(unsafe { Unknown::adopt(buffer.events) });
        let produced = if provided.is_some() {
            None
        } else {
            // SAFETY: a sample the transform allocated itself carries a reference handed to the caller.
            unsafe { Unknown::adopt(buffer.sample) }.map(Sample)
        };
        (hr, produced)
    }
}

pub fn decoder_exists(category: &Guid, major: &Guid, subtype: &Guid) -> bool {
    let mut count = 0;
    each_activator(category, major, subtype, |_| count += 1).is_some() && count > 0
}

struct Activate(Unknown);

fn each_activator(
    category: &Guid,
    major: &Guid,
    subtype: &Guid,
    mut visit: impl FnMut(&Activate),
) -> Option<()> {
    let api = api()?;
    let input = TypeInfo {
        major: *major,
        subtype: *subtype,
    };
    let mut array: *mut *mut c_void = ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: MFTEnumEx writes a CoTaskMemAlloc'd array of count referenced IMFActivate pointers into array.
    let hr = unsafe {
        (api.enum_ex)(
            *category,
            ENUM_FLAGS,
            &input,
            ptr::null(),
            &mut array,
            &mut count,
        )
    };
    if !succeeded(hr) || array.is_null() {
        return None;
    }
    for index in 0..count as usize {
        // SAFETY: index is below the count MFTEnumEx returned, so the slot holds one activator reference.
        let raw = unsafe { array.wrapping_add(index).read() };
        // SAFETY: each activator's reference is taken over and released when the owner drops.
        if let Some(activate) = unsafe { Unknown::adopt(raw) }.map(Activate) {
            visit(&activate);
        }
    }
    // SAFETY: array came from MFTEnumEx, which allocates it with CoTaskMemAlloc.
    unsafe { (api.co_task_mem_free)(array.cast()) };
    Some(())
}
