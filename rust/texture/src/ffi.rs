//! Southstar — the texture type: shared pixels, the user-data slot whose GLib destroy notify runs on release, and the counted TextureRef handle.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::ops::Deref;
use core::ptr::{self, NonNull};
use std::sync::{Arc, Mutex};

use southstar_glib::GDestroyNotify;

use crate::Pixels;

struct UserData {
    data: *mut c_void,
    destroy: GDestroyNotify,
}

pub struct Texture {
    pixels: Pixels,
    user_data: Mutex<UserData>,
}

// SAFETY: the pixels are immutable and the user-data slot is only reached through its mutex.
unsafe impl Send for Texture {}
// SAFETY: as for Send; every shared access to the slot locks the mutex.
unsafe impl Sync for Texture {}

impl Drop for Texture {
    fn drop(&mut self) {
        let user = self.user_data.get_mut().unwrap_or_else(|e| e.into_inner());
        if let Some(destroy) = user.destroy
            && !user.data.is_null()
        {
            // SAFETY: set_user_data's caller promised destroy may be called once with data.
            unsafe { destroy(user.data) };
        }
    }
}

impl Texture {
    pub fn width(&self) -> i32 {
        self.pixels.width
    }

    pub fn height(&self) -> i32 {
        self.pixels.height
    }

    pub fn download(&self, dst: &mut [u8], dst_stride: usize) {
        self.pixels.download(dst, dst_stride);
    }

    pub fn user_data(&self) -> *mut c_void {
        self.user_data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .data
    }

    /// # Safety
    ///
    /// `destroy`, when given, must be sound to call exactly once with `data` from any thread,
    /// when the texture drops or the slot is replaced by different data.
    pub unsafe fn set_user_data(&self, data: *mut c_void, destroy: GDestroyNotify) {
        let previous = {
            let mut user = self.user_data.lock().unwrap_or_else(|e| e.into_inner());
            let previous = (user.data, user.destroy);
            user.data = data;
            user.destroy = destroy;
            previous
        };
        if let (old, Some(old_destroy)) = previous
            && !old.is_null()
            && old != data
        {
            // SAFETY: the caller that stored old promised old_destroy may be called once with it.
            unsafe { old_destroy(old) };
        }
    }

    /// # Safety
    ///
    /// `raw` is null or a texture kept alive by some reference for the lifetime `'a`.
    pub unsafe fn borrow<'a>(raw: *const Texture) -> Option<&'a Texture> {
        // SAFETY: the caller guarantees raw is null or live for 'a.
        unsafe { raw.as_ref() }
    }
}

#[repr(transparent)]
pub struct TextureRef(NonNull<Texture>);

// SAFETY: a TextureRef is one counted reference to a Texture, which is Send and Sync.
unsafe impl Send for TextureRef {}
// SAFETY: as for Send; shared access only reads the texture.
unsafe impl Sync for TextureRef {}

impl TextureRef {
    pub fn new(width: i32, height: i32, bgra: &[u8], stride: usize) -> Option<TextureRef> {
        let pixels = Pixels::copy_from(width, height, bgra, stride)?;
        let texture = Arc::new(Texture {
            pixels,
            user_data: Mutex::new(UserData {
                data: ptr::null_mut(),
                destroy: None,
            }),
        });
        NonNull::new(Arc::into_raw(texture).cast_mut()).map(TextureRef)
    }

    pub fn as_ptr(&self) -> *mut Texture {
        self.0.as_ptr()
    }

    pub fn into_raw(self) -> *mut Texture {
        ManuallyDrop::new(self).0.as_ptr()
    }

    /// # Safety
    ///
    /// `raw` is null or a reference `into_raw` produced that the caller owns; it is taken over.
    pub unsafe fn from_raw(raw: *mut Texture) -> Option<TextureRef> {
        NonNull::new(raw).map(TextureRef)
    }

    /// # Safety
    ///
    /// `raw` is null or a reference `into_raw` produced that the caller owns and gives up.
    pub unsafe fn release(raw: *mut Texture) {
        // SAFETY: the caller gives up the reference raw stands for.
        drop(unsafe { TextureRef::from_raw(raw) });
    }
}

impl Deref for TextureRef {
    type Target = Texture;

    fn deref(&self) -> &Texture {
        // SAFETY: the reference this handle counts keeps the texture alive.
        unsafe { self.0.as_ref() }
    }
}

impl Clone for TextureRef {
    fn clone(&self) -> TextureRef {
        // SAFETY: the pointer came from Arc::into_raw and this handle keeps its count above zero.
        unsafe { Arc::increment_strong_count(self.0.as_ptr().cast_const()) };
        TextureRef(self.0)
    }
}

impl Drop for TextureRef {
    fn drop(&mut self) {
        // SAFETY: the pointer came from Arc::into_raw and this handle owns one of its counts.
        unsafe { Arc::decrement_strong_count(self.0.as_ptr().cast_const()) };
    }
}
