//! Southstar — the C ABI of textures, as declared in src/texture.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr;
use std::sync::{Arc, Mutex};

use southstar_glib::GDestroyNotify;

use crate::Pixels;

#[repr(C)]
pub struct GBytes {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
}

struct UserData {
    data: *mut c_void,
    destroy: GDestroyNotify,
}

pub struct Texture {
    pixels: Pixels,
    user_data: Mutex<UserData>,
}

unsafe impl Send for Texture {}
unsafe impl Sync for Texture {}

impl Drop for Texture {
    fn drop(&mut self) {
        let user = self.user_data.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Some(destroy) = user.destroy {
            if !user.data.is_null() {
                unsafe { destroy(user.data) };
            }
        }
    }
}

unsafe fn borrow<'a>(texture: *mut Texture) -> Option<&'a Texture> {
    unsafe { texture.as_ref() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_new(
    width: c_int,
    height: c_int,
    _format: c_int,
    bytes: *mut GBytes,
    stride: usize,
) -> *mut Texture {
    if width <= 0 || height <= 0 || bytes.is_null() {
        return ptr::null_mut();
    }
    let mut len = 0;
    let data = unsafe { g_bytes_get_data(bytes, &mut len) };
    if data.is_null() {
        return ptr::null_mut();
    }
    let src = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) };
    let Some(pixels) = Pixels::copy_from(width, height, src, stride) else {
        return ptr::null_mut();
    };
    let texture = Texture {
        pixels,
        user_data: Mutex::new(UserData {
            data: ptr::null_mut(),
            destroy: None,
        }),
    };
    Arc::into_raw(Arc::new(texture)).cast_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_ref(texture: *mut Texture) -> *mut Texture {
    if !texture.is_null() {
        unsafe { Arc::increment_strong_count(texture.cast_const()) };
    }
    texture
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_unref(texture: *mut Texture) {
    if !texture.is_null() {
        unsafe { Arc::decrement_strong_count(texture.cast_const()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_clear(texture: *mut *mut Texture) {
    if let Some(slot) = unsafe { texture.as_mut() } {
        if !slot.is_null() {
            unsafe { ns_texture_unref(*slot) };
            *slot = ptr::null_mut();
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_get_width(texture: *mut Texture) -> c_int {
    unsafe { borrow(texture) }.map_or(0, |t| t.pixels.width)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_get_height(texture: *mut Texture) -> c_int {
    unsafe { borrow(texture) }.map_or(0, |t| t.pixels.height)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_download(
    texture: *mut Texture,
    dst: *mut u8,
    dst_stride: usize,
) {
    let Some(texture) = (unsafe { borrow(texture) }) else {
        return;
    };
    if dst.is_null() {
        return;
    }
    let pixels = &texture.pixels;
    let row = (pixels.width as usize * 4)
        .min(dst_stride)
        .min(pixels.stride);
    let len = (pixels.height as usize - 1) * dst_stride + row;
    let dst = unsafe { core::slice::from_raw_parts_mut(dst, len) };
    pixels.download(dst, dst_stride);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_get_user_data(texture: *mut Texture) -> *mut c_void {
    unsafe { borrow(texture) }.map_or(ptr::null_mut(), |t| {
        t.user_data.lock().unwrap_or_else(|e| e.into_inner()).data
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_texture_set_user_data(
    texture: *mut Texture,
    data: *mut c_void,
    destroy: GDestroyNotify,
) {
    let Some(texture) = (unsafe { borrow(texture) }) else {
        if let Some(destroy) = destroy {
            if !data.is_null() {
                unsafe { destroy(data) };
            }
        }
        return;
    };
    let previous = {
        let mut user = texture.user_data.lock().unwrap_or_else(|e| e.into_inner());
        let previous = (user.data, user.destroy);
        user.data = data;
        user.destroy = destroy;
        previous
    };
    if let (old, Some(old_destroy)) = previous {
        if !old.is_null() && old != data {
            unsafe { old_destroy(old) };
        }
    }
}
