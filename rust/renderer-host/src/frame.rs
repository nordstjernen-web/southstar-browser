//! Southstar — the memory a renderer session paints into: its own buffer, or memory the shell shares, released when the session ends.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr::NonNull;
use std::sync::Arc;

use southstar_renderer_client::SharedFrame;

pub type Release = unsafe fn(NonNull<u8>, usize);

enum Backing {
    Owned(Vec<u8>),
    Shared(Arc<SharedFrame>),
    Foreign {
        pixels: NonNull<u8>,
        len: usize,
        release: Release,
    },
}

pub struct FrameMemory(Backing);

impl FrameMemory {
    pub fn owned(len: usize) -> FrameMemory {
        FrameMemory(Backing::Owned(vec![0u8; len]))
    }

    /// # Safety
    ///
    /// `pixels` must be valid for reads and writes of `len` bytes until
    /// `release(pixels, len)` runs when the returned value drops, nothing else
    /// in this process may access those bytes meanwhile, and `release` must be
    /// sound to call exactly once with these arguments.
    pub unsafe fn foreign(pixels: *mut u8, len: usize, release: Release) -> Option<FrameMemory> {
        let pixels = NonNull::new(pixels)?;
        Some(FrameMemory(Backing::Foreign {
            pixels,
            len,
            release,
        }))
    }

    pub fn shared(frame: Arc<SharedFrame>) -> FrameMemory {
        FrameMemory(Backing::Shared(frame))
    }

    pub fn len(&self) -> usize {
        match &self.0 {
            Backing::Owned(v) => v.len(),
            Backing::Shared(frame) => frame.len(),
            Backing::Foreign { len, .. } => *len,
        }
    }

    pub fn bytes(&mut self) -> &mut [u8] {
        match &mut self.0 {
            Backing::Owned(v) => v,
            // SAFETY: the frame's bytes live while the Arc does, and the window only reads them between the paints this session reports.
            Backing::Shared(frame) => unsafe {
                core::slice::from_raw_parts_mut(frame.as_ptr(), frame.len())
            },
            // SAFETY: `foreign`'s contract keeps `len` bytes at `pixels` valid and unaliased while self lives.
            Backing::Foreign { pixels, len, .. } => unsafe {
                core::slice::from_raw_parts_mut(pixels.as_ptr(), *len)
            },
        }
    }
}

impl Drop for FrameMemory {
    fn drop(&mut self) {
        if let Backing::Foreign {
            pixels,
            len,
            release,
        } = self.0
        {
            // SAFETY: `foreign`'s contract makes this the single release of memory nothing uses any more.
            unsafe { release(pixels, len) };
        }
    }
}
