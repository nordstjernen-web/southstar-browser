//! Southstar — the shared camera the page's capture streams use, and the V4L2 ioctls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::{Mutex, MutexGuard};

use southstar_js_runtime::api::Page;
use southstar_texture::TextureRef;

#[cfg(target_os = "linux")]
mod v4l2 {
    use core::ffi::{c_int, c_long, c_ulong, c_void};
    use core::mem::size_of;
    use core::ptr;
    use std::ffi::CString;
    use std::fs::File;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    use southstar_glib as glib;

    const VIDEO_CAPTURE: u32 = 1;
    const CAP_VIDEO_CAPTURE: u32 = 0x1;
    const FIELD_ANY: u32 = 0;
    const MEMORY_MMAP: u32 = 1;
    const BUFFERS: u32 = 4;
    const O_NONBLOCK: c_int = 0o4000;
    const PROT_READ_WRITE: c_int = 3;
    const MAP_SHARED: c_int = 1;
    const EINTR: i32 = 4;
    pub(super) const PIX_FMT_MJPEG: u32 = u32::from_le_bytes(*b"MJPG");
    pub(super) const PIX_FMT_YUYV: u32 = u32::from_le_bytes(*b"YUYV");

    #[repr(C)]
    #[derive(Default)]
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
    #[derive(Clone, Copy, Default)]
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
    #[derive(Default)]
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

    struct Device(File);

    impl Device {
        fn open(path: &CString) -> Option<Device> {
            let path = glib::path_from_bytes(path.to_bytes())?;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(O_NONBLOCK)
                .open(path)
                .ok()
                .map(Device)
        }

        fn fd(&self) -> c_int {
            self.0.as_raw_fd()
        }

        fn ioctl<T>(&self, request: c_ulong, arg: &mut T) -> bool {
            let arg = (arg as *mut T).cast::<c_void>();
            loop {
                // SAFETY: each request this module issues is paired with the repr(C) struct its V4L2 ioctl number encodes, and arg is that struct, exclusively borrowed for the call.
                let status = unsafe { ioctl(self.fd(), request, arg) };
                if status >= 0 {
                    return true;
                }
                if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
                    return false;
                }
            }
        }

        fn capability(&self) -> Option<Capability> {
            let mut cap = Capability::default();
            (self.ioctl(QUERYCAP, &mut cap) && cap.capabilities & CAP_VIDEO_CAPTURE != 0)
                .then_some(cap)
        }

        fn set_format(&self, pixelformat: u32) -> Option<PixFormat> {
            let mut format = Format {
                kind: VIDEO_CAPTURE,
                fmt: FormatUnion { raw: [0; 200] },
            };
            format.fmt.pix = PixFormat {
                width: 640,
                height: 480,
                pixelformat,
                field: FIELD_ANY,
                ..PixFormat::default()
            };
            if !self.ioctl(S_FMT, &mut format) {
                return None;
            }
            // SAFETY: pix was written above, and VIDIOC_S_FMT on a capture format fills that member, which is plain integers.
            Some(unsafe { format.fmt.pix })
        }

        fn stream(&self, on: bool) -> bool {
            let mut kind = VIDEO_CAPTURE as c_int;
            self.ioctl(if on { STREAMON } else { STREAMOFF }, &mut kind)
        }

        fn buffer(index: u32) -> Buffer {
            Buffer {
                index,
                kind: VIDEO_CAPTURE,
                bytesused: 0,
                flags: 0,
                field: 0,
                timestamp: [0; 2],
                timecode: [0; 4],
                sequence: 0,
                memory: MEMORY_MMAP,
                location: BufferLocation { userptr: 0 },
                length: 0,
                reserved2: 0,
                request_fd: 0,
            }
        }
    }

    struct Mapping {
        start: *mut c_void,
        length: usize,
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            // SAFETY: start and length are the mapping mmap returned for this Mapping, unmapped only here, and every Frame slice of it borrows the Camera that owns it.
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
                ..RequestBuffers::default()
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
                // SAFETY: VIDIOC_QUERYBUF on an MMAP buffer sets the offset member of the location union, which is a plain integer.
                let offset = i64::from(unsafe { buffer.location.offset });
                let fd = camera.device.fd();
                // SAFETY: a new shared mapping of the device at the offset and length the driver reported for this buffer; it replaces no existing memory.
                let start = unsafe {
                    mmap(
                        ptr::null_mut(),
                        length,
                        PROT_READ_WRITE,
                        MAP_SHARED,
                        fd,
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
            let start = mapping.start.cast::<u8>().cast_const();
            // SAFETY: the mapping is length bytes of live shared memory owned by self, and the Frame borrows self, so the slice cannot outlive the mapping.
            let data = unsafe { core::slice::from_raw_parts(start, mapping.length) };
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
        let mut tail = [&cap.card[..], &cap.bus_info[..]].concat();
        for word in [cap.version, cap.capabilities, cap.device_caps]
            .into_iter()
            .chain(cap.reserved)
        {
            tail.extend_from_slice(&word.to_ne_bytes());
        }

        let end = tail.iter().position(|&c| c == 0).unwrap_or(tail.len());
        Some(tail[..end].to_vec())
    }

    pub(super) fn exists(path: &[u8]) -> bool {
        glib::path_from_bytes(path).is_some_and(|path| path.exists())
    }
}

#[cfg(target_os = "linux")]
use v4l2::Camera;

#[cfg(not(target_os = "linux"))]
enum Camera {}

pub struct CameraDevice {
    pub device: Vec<u8>,
    pub label: Vec<u8>,
}

struct Active {
    camera: Option<Box<Camera>>,
    refs: i32,
}

// SAFETY: the camera is reached only through the ACTIVE mutex, so one thread uses it at a time.
unsafe impl Send for Active {}

static ACTIVE: Mutex<Active> = Mutex::new(Active {
    camera: None,
    refs: 0,
});

fn active() -> MutexGuard<'static, Active> {
    ACTIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn page_permission(js: Option<Page>) -> Option<bool> {
    let url = southstar_js_runtime::api::current_url(js);
    let url = url.to_bytes();
    match crate::permission(Some(url), southstar_url::bytes::origin_from(url)) {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

pub fn acquire() -> bool {
    let mut state = active();
    if state.camera.is_none() {
        state.camera = platform::open_default();
    }
    let opened = state.camera.is_some();
    if opened {
        state.refs += 1;
    }
    opened
}

pub fn release() {
    let mut state = active();
    if state.refs > 0 {
        state.refs -= 1;
        if state.refs == 0 {
            state.camera = None;
        }
    }
}

pub fn is_active() -> bool {
    active().camera.is_some()
}

pub fn active_device() -> Option<Vec<u8>> {
    active().camera.as_deref().map(platform::device_name)
}

pub fn next_frame() -> Option<Option<TextureRef>> {
    let mut state = active();
    let camera = state.camera.as_deref_mut()?;
    Some(platform::next_frame(camera))
}

pub fn devices() -> Vec<CameraDevice> {
    platform::devices()
}

#[cfg(target_os = "linux")]
mod platform {
    use core::ffi::c_int;

    use southstar_texture::TextureRef;

    use super::CameraDevice;
    use super::v4l2::{self, Camera, PIX_FMT_MJPEG, PIX_FMT_YUYV};

    pub(super) fn open_default() -> Option<Box<Camera>> {
        Camera::open(b"/dev/video0")
    }

    pub(super) fn device_name(camera: &Camera) -> Vec<u8> {
        camera.name.to_bytes().to_vec()
    }

    fn yuyv_texture(width: i32, height: i32, data: &[u8]) -> Option<TextureRef> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let (width, height) = (width as usize, height as usize);
        let stride = width * 4;
        let mut bgra = vec![0u8; stride * height];
        crate::yuyv_to_bgra(data, width, height, width * 2, &mut bgra, stride);
        TextureRef::new(width as c_int, height as c_int, &bgra, stride)
    }

    pub(super) fn next_frame(camera: &mut Camera) -> Option<TextureRef> {
        let frame = camera.dequeue()?;
        let mut size = None;
        let texture = if camera.pixelformat == PIX_FMT_MJPEG {
            let jpeg = frame.data.get(..frame.used).unwrap_or(frame.data);
            southstar_image::decode_bytes(jpeg).map(|(texture, width, height)| {
                size = Some((width, height));
                texture
            })
        } else if camera.pixelformat == PIX_FMT_YUYV {
            yuyv_texture(camera.width, camera.height, frame.data)
        } else {
            None
        };
        camera.requeue(frame);
        if let Some((width, height)) = size {
            camera.width = width;
            camera.height = height;
        }
        texture
    }

    pub(super) fn devices() -> Vec<CameraDevice> {
        (0..64)
            .map(|index| format!("/dev/video{index}").into_bytes())
            .filter(|path| v4l2::exists(path))
            .filter_map(|device| {
                let label = v4l2::label(&device)?;
                Some(CameraDevice { device, label })
            })
            .collect()
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use southstar_texture::TextureRef;

    use super::{Camera, CameraDevice};

    pub(super) fn open_default() -> Option<Box<Camera>> {
        None
    }

    pub(super) fn device_name(camera: &Camera) -> Vec<u8> {
        match *camera {}
    }

    pub(super) fn next_frame(camera: &mut Camera) -> Option<TextureRef> {
        match *camera {}
    }

    pub(super) fn devices() -> Vec<CameraDevice> {
        Vec::new()
    }
}
