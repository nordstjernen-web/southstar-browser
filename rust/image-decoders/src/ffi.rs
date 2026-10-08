//! Southstar — the C ABI of the ICO and WebP decoders, as declared in src/image.h, and the texture and Wuffs calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod libwebp;

use core::ffi::{c_int, c_uint, c_void};
use core::ops::{Deref, DerefMut};
use core::ptr::{self, NonNull};
use southstar_glib::{FALSE, GBoolean, TRUE, g_free};

use crate::{Decoded, Frame, Pixels};

#[repr(C)]
struct NsTexture {
    _private: [u8; 0],
}

#[repr(C)]
struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct PixelFrame {
    pixels: *mut u8,
    pixels_len: usize,
    stride: usize,
    format: c_int,
    width: c_int,
    height: c_int,
    delay_ms: c_int,
}

const TEXTURE_BGRA_PREMULTIPLIED: c_int = 0;

unsafe extern "C" {
    fn g_try_malloc(n_bytes: usize) -> *mut c_void;
    fn g_bytes_new_take(data: *mut c_void, size: usize) -> *mut GBytes;
    fn g_bytes_unref(bytes: *mut GBytes);
    fn g_array_new(zero_terminated: GBoolean, clear: GBoolean, element_size: c_uint)
    -> *mut GArray;
    fn g_array_set_clear_func(array: *mut GArray, clear_func: unsafe extern "C" fn(*mut c_void));
    fn g_array_append_vals(array: *mut GArray, data: *const c_void, len: c_uint) -> *mut GArray;
    fn ns_texture_new(
        width: c_int,
        height: c_int,
        format: c_int,
        bytes: *mut GBytes,
        stride: usize,
    ) -> *mut NsTexture;
    fn ns_texture_unref(texture: *mut NsTexture);
    fn ns_image_pixel_frame_clear(data: *mut c_void);
    fn ns_image_decode_wuffs(
        data: *const u8,
        len: usize,
        out_w: *mut c_int,
        out_h: *mut c_int,
    ) -> *mut NsTexture;
    fn ns_image_wuffs_decode_to_bgra(
        data: *const u8,
        len: usize,
        out_w: *mut c_int,
        out_h: *mut c_int,
        out_stride: *mut usize,
        out_buf_len: *mut usize,
    ) -> *mut u8;
}

pub struct Buffer {
    ptr: NonNull<u8>,
    len: usize,
}

impl Buffer {
    pub fn try_new(len: usize) -> Option<Buffer> {
        let ptr = NonNull::new(unsafe { g_try_malloc(len) }.cast())?;
        Some(Buffer { ptr, len })
    }

    unsafe fn from_glib(ptr: *mut u8, len: usize) -> Option<Buffer> {
        NonNull::new(ptr).map(|ptr| Buffer { ptr, len })
    }

    fn into_raw(self) -> *mut u8 {
        let ptr = self.ptr.as_ptr();
        core::mem::forget(self);
        ptr
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe { g_free(self.ptr.as_ptr().cast()) };
    }
}

pub struct Texture(NonNull<NsTexture>);

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { ns_texture_unref(self.0.as_ptr()) };
    }
}

impl Texture {
    pub fn new_premultiplied(
        width: i32,
        height: i32,
        buffer: Buffer,
        stride: usize,
    ) -> Option<Texture> {
        let len = buffer.len;
        let bytes = unsafe { g_bytes_new_take(buffer.into_raw().cast(), len) };
        let texture =
            unsafe { ns_texture_new(width, height, TEXTURE_BGRA_PREMULTIPLIED, bytes, stride) };
        unsafe { g_bytes_unref(bytes) };
        NonNull::new(texture).map(Texture)
    }

    fn into_raw(self) -> *mut NsTexture {
        let ptr = self.0.as_ptr();
        core::mem::forget(self);
        ptr
    }
}

pub fn wuffs_decode(data: &[u8]) -> Option<Decoded> {
    let (mut width, mut height) = (0, 0);
    let texture =
        unsafe { ns_image_decode_wuffs(data.as_ptr(), data.len(), &mut width, &mut height) };
    NonNull::new(texture).map(|texture| Decoded {
        texture: Texture(texture),
        width,
        height,
    })
}

pub fn wuffs_decode_to_bgra(data: &[u8]) -> Option<Pixels> {
    let (mut width, mut height, mut stride, mut len) = (0, 0, 0, 0);
    let ptr = unsafe {
        ns_image_wuffs_decode_to_bgra(
            data.as_ptr(),
            data.len(),
            &mut width,
            &mut height,
            &mut stride,
            &mut len,
        )
    };
    let buffer = unsafe { Buffer::from_glib(ptr, len) }?;
    Some(Pixels {
        buffer,
        width,
        height,
        stride,
    })
}

unsafe fn set<T>(out: *mut T, value: T) {
    if !out.is_null() {
        unsafe { *out = value };
    }
}

unsafe fn texture_result(
    decoded: Option<Decoded>,
    out_w: *mut c_int,
    out_h: *mut c_int,
) -> *mut NsTexture {
    match decoded {
        Some(decoded) => unsafe {
            set(out_w, decoded.width);
            set(out_h, decoded.height);
            decoded.texture.into_raw()
        },
        None => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_decode_ico(
    data: *const u8,
    len: usize,
    out_w: *mut c_int,
    out_h: *mut c_int,
) -> *mut c_void {
    if data.is_null() {
        return ptr::null_mut();
    }
    let data = unsafe { southstar_glib::slice(data, len) };
    unsafe { texture_result(crate::decode_ico(data), out_w, out_h) }.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_webp_supports_bytes(data: *const u8, len: usize) -> GBoolean {
    let supported =
        !data.is_null() && crate::webp_supports(unsafe { southstar_glib::slice(data, len) });
    if supported { TRUE } else { FALSE }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_decode_webp(
    data: *const u8,
    len: usize,
    out_w: *mut c_int,
    out_h: *mut c_int,
) -> *mut c_void {
    let data = unsafe { southstar_glib::slice(data, len) };
    let decoded = crate::decode_webp(data).and_then(Pixels::into_texture);
    unsafe { texture_result(decoded, out_w, out_h) }.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_webp_decode_to_bgra(
    data: *const u8,
    len: usize,
    out_w: *mut c_int,
    out_h: *mut c_int,
    out_stride: *mut usize,
    out_buf_len: *mut usize,
) -> *mut u8 {
    let data = unsafe { southstar_glib::slice(data, len) };
    let Some(pixels) = crate::decode_webp(data) else {
        return ptr::null_mut();
    };
    unsafe {
        set(out_w, pixels.width);
        set(out_h, pixels.height);
        set(out_stride, pixels.stride);
        set(out_buf_len, pixels.stride * pixels.height as usize);
    }
    pixels.buffer.into_raw()
}

fn pixel_frame(frame: Frame) -> PixelFrame {
    let Frame { pixels, delay_ms } = frame;
    let pixels_len = pixels.buffer.len;
    PixelFrame {
        pixels: pixels.buffer.into_raw(),
        pixels_len,
        stride: pixels.stride,
        format: TEXTURE_BGRA_PREMULTIPLIED,
        width: pixels.width,
        height: pixels.height,
        delay_ms,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_decode_webp_anim_to_pixels(
    data: *const u8,
    len: usize,
    out_w: *mut c_int,
    out_h: *mut c_int,
) -> *mut GArray {
    let data = unsafe { southstar_glib::slice(data, len) };
    let Some((frames, width, height)) = crate::decode_webp_animation(data) else {
        return ptr::null_mut();
    };
    let array = unsafe { g_array_new(FALSE, FALSE, core::mem::size_of::<PixelFrame>() as c_uint) };
    unsafe { g_array_set_clear_func(array, ns_image_pixel_frame_clear) };
    for frame in frames {
        let frame = pixel_frame(frame);
        unsafe { g_array_append_vals(array, (&raw const frame).cast(), 1) };
    }
    unsafe {
        set(out_w, width);
        set(out_h, height);
    }
    array
}
