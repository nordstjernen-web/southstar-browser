//! Southstar — the texture type: shared pixels, one typed cache slot its users attach derived data to, and the counted TextureRef handle.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::ops::Deref;
use std::sync::{Arc, Mutex};

use crate::Pixels;

pub struct Texture {
    pixels: Pixels,
    cache: Mutex<Option<Box<dyn Any + Send>>>,
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

    pub fn with_cache<T: Any + Send, R>(
        &self,
        init: impl FnOnce() -> T,
        f: impl FnOnce(&mut T) -> R,
    ) -> R {
        let mut slot = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = slot.as_mut().and_then(|c| c.downcast_mut::<T>()) {
            return f(cached);
        }
        let mut fresh = Box::new(init());
        let result = f(&mut fresh);
        *slot = Some(fresh);
        result
    }
}

#[derive(Clone)]
pub struct TextureRef(Arc<Texture>);

impl TextureRef {
    pub fn new(width: i32, height: i32, bgra: &[u8], stride: usize) -> Option<TextureRef> {
        let pixels = Pixels::copy_from(width, height, bgra, stride)?;
        Some(TextureRef(Arc::new(Texture {
            pixels,
            cache: Mutex::new(None),
        })))
    }

    pub fn ptr_eq(&self, other: &TextureRef) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for TextureRef {
    type Target = Texture;

    fn deref(&self) -> &Texture {
        &self.0
    }
}
