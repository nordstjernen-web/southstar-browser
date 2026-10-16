//! Southstar — the GLib buffers and texture calls the raster (PNG/GIF/BMP/JPEG), ICO and WebP decoders hand their pixels to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "avif")]
pub(crate) mod libavif;
pub(crate) mod libwebp;

use core::ffi::{c_int, c_void};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use southstar_glib::g_free;

#[repr(C)]
struct NsTexture {
    _private: [u8; 0],
}

#[repr(C)]
struct GBytes {
    _private: [u8; 0],
}

const TEXTURE_BGRA_PREMULTIPLIED: c_int = 0;

unsafe extern "C" {
    fn g_try_malloc(n_bytes: usize) -> *mut c_void;
    fn g_bytes_new_take(data: *mut c_void, size: usize) -> *mut GBytes;
    fn g_bytes_unref(bytes: *mut GBytes);
    fn ns_texture_new(
        width: c_int,
        height: c_int,
        format: c_int,
        bytes: *mut GBytes,
        stride: usize,
    ) -> *mut NsTexture;
    fn ns_texture_unref(texture: *mut NsTexture);
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

    pub fn into_raw(self) -> *mut u8 {
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

    pub fn into_raw(self) -> *mut c_void {
        let ptr = self.0.as_ptr();
        core::mem::forget(self);
        ptr.cast()
    }
}
