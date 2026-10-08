//! Southstar — the C ABI of the camera, as declared in src/camera.h, and the V4L2 ioctls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::sync::Mutex;

use southstar_glib::{self as glib, GPtrArray};

#[repr(C)]
pub struct CameraInfo {
    device: *mut c_char,
    label: *mut c_char,
}

unsafe extern "C" {
    fn g_ptr_array_new() -> *mut GPtrArray;
    fn ns_js_current_url(js: *const c_void) -> *const c_char;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
}

unsafe fn take_string(text: *mut c_char) -> Option<Vec<u8>> {
    let bytes = unsafe { glib::bytes(text) }.map(<[u8]>::to_vec);
    unsafe { glib::g_free(text.cast()) };
    bytes
}

#[cfg(target_os = "linux")]
mod v4l2 {
    use core::ffi::{c_char, c_int, c_long, c_ulong, c_void};
    use core::mem::size_of;
    use core::ptr;
    use std::ffi::CString;

    use southstar_glib as glib;

    const VIDEO_CAPTURE: u32 = 1;
    const CAP_VIDEO_CAPTURE: u32 = 0x1;
    const FIELD_ANY: u32 = 0;
    const MEMORY_MMAP: u32 = 1;
    const BUFFERS: u32 = 4;
    const O_RDWR: c_int = 0o2;
    const O_NONBLOCK: c_int = 0o4000;
    const O_CLOEXEC: c_int = 0o2000000;
    const PROT_READ_WRITE: c_int = 3;
    const MAP_SHARED: c_int = 1;
    const EINTR: i32 = 4;
    pub(super) const PIX_FMT_MJPEG: u32 = u32::from_le_bytes(*b"MJPG");
    pub(super) const PIX_FMT_YUYV: u32 = u32::from_le_bytes(*b"YUYV");

    #[repr(C)]
    struct Capability {
        driver: [u8; 16],
        card: [u8; 32],
        bus_info: [u8; 32],
        version: u32,
        capabilities: u32,
        device_caps: u32,
        reserved: [u32; 3],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct PixFormat {
        width: u32,
        height: u32,
        pixelformat: u32,
        field: u32,
        bytesperline: u32,
        sizeimage: u32,
        colorspace: u32,
        private: u32,
        flags: u32,
        encoding: u32,
        quantization: u32,
        xfer_func: u32,
    }

    #[repr(C)]
    union FormatUnion {
        pix: PixFormat,
        raw: [u8; 200],
        align: [usize; 0],
    }

    #[repr(C)]
    struct Format {
        kind: u32,
        fmt: FormatUnion,
    }

    #[repr(C)]
    struct RequestBuffers {
        count: u32,
        kind: u32,
        memory: u32,
        capabilities: u32,
        flags: u8,
        reserved: [u8; 3],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    union BufferLocation {
        offset: u32,
        userptr: c_ulong,
        fd: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) struct Buffer {
        index: u32,
        kind: u32,
        bytesused: u32,
        flags: u32,
        field: u32,
        timestamp: [c_long; 2],
        timecode: [u32; 4],
        sequence: u32,
        memory: u32,
        location: BufferLocation,
        length: u32,
        reserved2: u32,
        request_fd: i32,
    }

    const fn ioc(dir: c_ulong, nr: c_ulong, size: usize) -> c_ulong {
        (dir << 30) | ((size as c_ulong) << 16) | ((b'V' as c_ulong) << 8) | nr
    }

    const READ: c_ulong = 2;
    const WRITE: c_ulong = 1;
    const QUERYCAP: c_ulong = ioc(READ, 0, size_of::<Capability>());
    const S_FMT: c_ulong = ioc(READ | WRITE, 5, size_of::<Format>());
    const REQBUFS: c_ulong = ioc(READ | WRITE, 8, size_of::<RequestBuffers>());
    const QUERYBUF: c_ulong = ioc(READ | WRITE, 9, size_of::<Buffer>());
    const QBUF: c_ulong = ioc(READ | WRITE, 15, size_of::<Buffer>());
    const DQBUF: c_ulong = ioc(READ | WRITE, 17, size_of::<Buffer>());
    const STREAMON: c_ulong = ioc(WRITE, 18, size_of::<c_int>());
    const STREAMOFF: c_ulong = ioc(WRITE, 19, size_of::<c_int>());

    unsafe extern "C" {
        fn open(path: *const c_char, flags: c_int, ...) -> c_int;
        fn close(fd: c_int) -> c_int;
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
        fn mmap(
            addr: *mut c_void,
            len: usize,
            prot: c_int,
            flags: c_int,
            fd: c_int,
            offset: i64,
        ) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> c_int;
    }

    fn zeroed<T>() -> T {
        unsafe { core::mem::zeroed() }
    }

    struct Device(c_int);

    impl Device {
        fn open(path: &CString) -> Option<Device> {
            let fd = unsafe { open(path.as_ptr(), O_RDWR | O_NONBLOCK | O_CLOEXEC) };
            (fd >= 0).then_some(Device(fd))
        }

        fn ioctl<T>(&self, request: c_ulong, arg: &mut T) -> bool {
            loop {
                let status = unsafe { ioctl(self.0, request, (arg as *mut T).cast::<c_void>()) };
                if status >= 0 {
                    return true;
                }
                if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
                    return false;
                }
            }
        }

        fn capability(&self) -> Option<Capability> {
            let mut cap: Capability = zeroed();
            (self.ioctl(QUERYCAP, &mut cap) && cap.capabilities & CAP_VIDEO_CAPTURE != 0)
                .then_some(cap)
        }

        fn set_format(&self, pixelformat: u32) -> Option<PixFormat> {
            let mut format: Format = zeroed();
            format.kind = VIDEO_CAPTURE;
            format.fmt.pix = PixFormat {
                width: 640,
                height: 480,
                pixelformat,
                field: FIELD_ANY,
                ..unsafe { format.fmt.pix }
            };
            let accepted = self.ioctl(S_FMT, &mut format);
            accepted.then_some(unsafe { format.fmt.pix })
        }

        fn stream(&self, on: bool) -> bool {
            let mut kind = VIDEO_CAPTURE as c_int;
            self.ioctl(if on { STREAMON } else { STREAMOFF }, &mut kind)
        }

        fn buffer(index: u32) -> Buffer {
            let mut buffer: Buffer = zeroed();
            buffer.kind = VIDEO_CAPTURE;
            buffer.memory = MEMORY_MMAP;
            buffer.index = index;
            buffer
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { close(self.0) };
        }
    }

    struct Mapping {
        start: *mut c_void,
        length: usize,
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe { munmap(self.start, self.length) };
        }
    }

    pub struct Camera {
        mappings: Vec<Mapping>,
        streaming: bool,
        device: Device,
        pub(super) name: CString,
        pub(super) width: i32,
        pub(super) height: i32,
        pub(super) pixelformat: u32,
    }

    impl Drop for Camera {
        fn drop(&mut self) {
            if self.streaming {
                self.device.stream(false);
            }
            self.mappings.clear();
        }
    }

    pub(super) struct Frame<'a> {
        pub(super) data: &'a [u8],
        pub(super) used: usize,
        buffer: Buffer,
    }

    impl Camera {
        pub(super) fn open(path: &[u8]) -> Option<Box<Camera>> {
            let name = CString::new(path).ok()?;
            let device = Device::open(&name)?;
            device.capability()?;
            let format = device
                .set_format(PIX_FMT_MJPEG)
                .or_else(|| device.set_format(PIX_FMT_YUYV))?;
            if format.pixelformat != PIX_FMT_MJPEG && format.pixelformat != PIX_FMT_YUYV {
                return None;
            }
            let mut request = RequestBuffers {
                count: BUFFERS,
                kind: VIDEO_CAPTURE,
                memory: MEMORY_MMAP,
                ..zeroed()
            };
            if !device.ioctl(REQBUFS, &mut request) || request.count < 2 {
                return None;
            }
            let mut camera = Box::new(Camera {
                mappings: Vec::new(),
                streaming: false,
                width: format.width as i32,
                height: format.height as i32,
                pixelformat: format.pixelformat,
                name,
                device,
            });
            for index in 0..request.count.min(BUFFERS) {
                let mut buffer = Device::buffer(index);
                if !camera.device.ioctl(QUERYBUF, &mut buffer) {
                    break;
                }
                let length = buffer.length as usize;
                let offset = i64::from(unsafe { buffer.location.offset });
                let start = unsafe {
                    mmap(
                        ptr::null_mut(),
                        length,
                        PROT_READ_WRITE,
                        MAP_SHARED,
                        camera.device.0,
                        offset,
                    )
                };
                if start as isize == -1 {
                    break;
                }
                camera.mappings.push(Mapping { start, length });
            }
            if camera.mappings.len() < 2 {
                return None;
            }
            for index in 0..camera.mappings.len() as u32 {
                let mut buffer = Device::buffer(index);
                if !camera.device.ioctl(QBUF, &mut buffer) {
                    return None;
                }
            }
            if !camera.device.stream(true) {
                return None;
            }
            camera.streaming = true;
            Some(camera)
        }

        pub(super) fn dequeue(&self) -> Option<Frame<'_>> {
            if !self.streaming {
                return None;
            }
            let mut buffer = Device::buffer(0);
            if !self.device.ioctl(DQBUF, &mut buffer) {
                return None;
            }
            let mapping = self.mappings.get(buffer.index as usize)?;
            let data =
                unsafe { core::slice::from_raw_parts(mapping.start.cast::<u8>(), mapping.length) };
            Some(Frame {
                data,
                used: buffer.bytesused as usize,
                buffer,
            })
        }

        pub(super) fn requeue(&self, frame: Frame<'_>) {
            let mut buffer = frame.buffer;
            self.device.ioctl(QBUF, &mut buffer);
        }
    }

    pub(super) fn label(path: &[u8]) -> Option<Vec<u8>> {
        let name = CString::new(path).ok()?;
        let device = Device::open(&name)?;
        let cap = device.capability()?;
        if cap.card[0] == 0 {
            return None;
        }
        let tail = unsafe {
            core::slice::from_raw_parts(
                cap.card.as_ptr(),
                size_of::<Capability>() - core::mem::offset_of!(Capability, card),
            )
        };
        let end = tail.iter().position(|&c| c == 0).unwrap_or(tail.len());
        Some(tail[..end].to_vec())
    }

    pub(super) fn exists(path: &[u8]) -> bool {
        let Ok(name) = CString::new(path) else {
            return false;
        };
        unsafe { glib::g_file_test(name.as_ptr(), glib::FILE_TEST_EXISTS) != 0 }
    }
}

#[cfg(target_os = "linux")]
pub use v4l2::Camera;

#[cfg(not(target_os = "linux"))]
pub struct Camera {
    _private: [u8; 0],
}

struct Active {
    camera: *mut Camera,
    refs: i32,
}

unsafe impl Send for Active {}

static ACTIVE: Mutex<Active> = Mutex::new(Active {
    camera: ptr::null_mut(),
    refs: 0,
});

fn active() -> std::sync::MutexGuard<'static, Active> {
    ACTIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_camera_info_free(info: *mut CameraInfo) {
    if info.is_null() {
        return;
    }
    unsafe {
        glib::g_free((*info).device.cast());
        glib::g_free((*info).label.cast());
        glib::g_free(info.cast());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_camera_take_pending_origin() -> *mut c_char {
    crate::take_pending_origin().map_or(ptr::null_mut(), |origin| glib::strdup(&origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_camera_set_decision(origin: *const c_char, allow: c_int) {
    if let Some(origin) = unsafe { glib::bytes(origin) } {
        crate::set_decision(origin, allow != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_camera_permission(js: *const c_void) -> c_int {
    let url = unsafe { ns_js_current_url(js) };
    let origin = unsafe { take_string(ns_url_origin_from(url)) };
    crate::permission(unsafe { glib::bytes(url) }, origin)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_camera_acquire() -> *mut Camera {
    let mut state = active();
    if state.camera.is_null() {
        state.camera = unsafe { ns_camera_open(ptr::null()) };
    }
    if !state.camera.is_null() {
        state.refs += 1;
    }
    state.camera
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_camera_release() {
    let mut state = active();
    if state.refs > 0 {
        state.refs -= 1;
        if state.refs == 0 {
            unsafe { ns_camera_close(state.camera) };
            state.camera = ptr::null_mut();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_camera_active() -> *mut Camera {
    active().camera
}

#[cfg(target_os = "linux")]
mod linux {
    use core::ffi::{c_char, c_int, c_void};
    use core::ptr;

    use southstar_glib::{self as glib, GPtrArray};

    use super::v4l2::{self, Camera, PIX_FMT_MJPEG, PIX_FMT_YUYV};
    use super::{CameraInfo, g_ptr_array_new};

    #[repr(C)]
    struct GBytes {
        _private: [u8; 0],
    }

    const TEXTURE_BGRA_PREMULTIPLIED: c_int = 0;

    unsafe extern "C" {
        fn g_bytes_new_take(data: *mut c_void, size: usize) -> *mut GBytes;
        fn g_bytes_unref(bytes: *mut GBytes);
        fn ns_image_decode_bytes(
            data: *const u8,
            len: usize,
            width: *mut c_int,
            height: *mut c_int,
        ) -> *mut c_void;
        fn ns_texture_new(
            width: c_int,
            height: c_int,
            format: c_int,
            bytes: *mut GBytes,
            stride: usize,
        ) -> *mut c_void;
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_open(device: *const c_char) -> *mut Camera {
        let path = unsafe { glib::bytes(device) }
            .filter(|path| !path.is_empty())
            .unwrap_or(b"/dev/video0");
        Camera::open(path).map_or(ptr::null_mut(), Box::into_raw)
    }

    fn yuyv_texture(width: i32, height: i32, data: &[u8]) -> *mut c_void {
        if width <= 0 || height <= 0 {
            return ptr::null_mut();
        }
        let (width, height) = (width as usize, height as usize);
        let stride = width * 4;
        let mut bgra = vec![0u8; stride * height];
        crate::yuyv_to_bgra(data, width, height, width * 2, &mut bgra, stride);
        unsafe {
            let copy = glib::g_malloc(bgra.len()).cast::<u8>();
            ptr::copy_nonoverlapping(bgra.as_ptr(), copy, bgra.len());
            let bytes = g_bytes_new_take(copy.cast(), bgra.len());
            let texture = ns_texture_new(
                width as c_int,
                height as c_int,
                TEXTURE_BGRA_PREMULTIPLIED,
                bytes,
                stride,
            );
            g_bytes_unref(bytes);
            texture
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_next_frame(camera: *mut Camera) -> *mut c_void {
        let Some(camera) = (unsafe { camera.as_mut() }) else {
            return ptr::null_mut();
        };
        let Some(frame) = camera.dequeue() else {
            return ptr::null_mut();
        };
        let mut texture = ptr::null_mut();
        let mut size = None;
        if camera.pixelformat == PIX_FMT_MJPEG {
            let (mut width, mut height) = (0, 0);
            texture = unsafe {
                ns_image_decode_bytes(frame.data.as_ptr(), frame.used, &mut width, &mut height)
            };
            if !texture.is_null() {
                size = Some((width, height));
            }
        } else if camera.pixelformat == PIX_FMT_YUYV {
            texture = yuyv_texture(camera.width, camera.height, frame.data);
        }
        camera.requeue(frame);
        if let Some((width, height)) = size {
            camera.width = width;
            camera.height = height;
        }
        texture
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_device(camera: *const Camera) -> *const c_char {
        unsafe { camera.as_ref() }.map_or(ptr::null(), |camera| camera.name.as_ptr())
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_close(camera: *mut Camera) {
        if !camera.is_null() {
            drop(unsafe { Box::from_raw(camera) });
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn ns_camera_enumerate() -> *mut GPtrArray {
        let list = unsafe { g_ptr_array_new() };
        for index in 0..64 {
            let path = format!("/dev/video{index}").into_bytes();
            if !v4l2::exists(&path) {
                continue;
            }
            let Some(label) = v4l2::label(&path) else {
                continue;
            };
            unsafe {
                let info = glib::g_malloc0(core::mem::size_of::<CameraInfo>()).cast::<CameraInfo>();
                (*info).device = glib::strdup(&path);
                (*info).label = glib::strdup(&label);
                glib::g_ptr_array_add(list, info.cast());
            }
        }
        list
    }
}

#[cfg(not(target_os = "linux"))]
mod other {
    use core::ffi::{c_char, c_void};
    use core::ptr;

    use southstar_glib::GPtrArray;

    use super::{Camera, g_ptr_array_new};

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_open(_device: *const c_char) -> *mut Camera {
        ptr::null_mut()
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_next_frame(_camera: *mut Camera) -> *mut c_void {
        ptr::null_mut()
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_device(_camera: *const Camera) -> *const c_char {
        ptr::null()
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ns_camera_close(_camera: *mut Camera) {}

    #[unsafe(no_mangle)]
    pub extern "C" fn ns_camera_enumerate() -> *mut GPtrArray {
        unsafe { g_ptr_array_new() }
    }
}

#[cfg(target_os = "linux")]
use linux::{ns_camera_close, ns_camera_open};
#[cfg(not(target_os = "linux"))]
use other::{ns_camera_close, ns_camera_open};
