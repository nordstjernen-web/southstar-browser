//! Southstar — the GLib buffers the raster (PNG/GIF/BMP/JPEG), ICO and WebP decoders hand their pixels in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "avif")]
pub(crate) mod libavif;
pub(crate) mod libwebp;

use core::ffi::c_void;
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use southstar_glib::g_free;

unsafe extern "C" {
    fn g_try_malloc(n_bytes: usize) -> *mut c_void;
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
