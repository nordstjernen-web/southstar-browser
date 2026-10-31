//! Southstar — the owner of the libswscale scaler that turns decoded frames into BGRA.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr::{self, NonNull};

use crate::decode::Frame;
use crate::{self as av, SwsContext, frame};

#[derive(Default)]
pub struct Scaler {
    context: Option<NonNull<SwsContext>>,
    width: u32,
    height: u32,
}

impl Scaler {
    pub fn prepare(&mut self, source: &Frame, width: u32, height: u32) -> bool {
        let previous = self.context.map_or(ptr::null_mut(), NonNull::as_ptr);
        // SAFETY: previous is null or the scaler this value owns, which libav reuses or frees; the filters and parameters are null.
        let context = unsafe {
            av::sws_getCachedContext(
                previous,
                source.get(&frame::WIDTH),
                source.get(&frame::HEIGHT),
                source.get(&frame::FORMAT),
                width as c_int,
                height as c_int,
                av::AV_PIX_FMT_BGRA,
                av::SWS_BILINEAR,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        self.context = NonNull::new(context);
        self.width = width;
        self.height = height;
        self.context.is_some()
    }

    pub fn scale_into(&mut self, source: &Frame, target: &mut [u8], stride: usize) -> bool {
        let row = self.width as usize * 4;
        let needed = stride.checked_mul(self.height as usize);
        if stride < row || needed.is_none_or(|needed| target.len() < needed) {
            return false;
        }
        // SAFETY: target is an exclusive slice of at least height rows of stride bytes, each stride holding width * 4 bytes.
        unsafe { self.scale_to(source, target.as_mut_ptr(), stride, self.width, self.height) }
    }

    /// # Safety
    ///
    /// `target` must be writable for `height` rows of `stride` bytes, each row holding at least `width * 4` bytes, with nothing else accessing those bytes during the call.
    pub unsafe fn scale_to(
        &mut self,
        source: &Frame,
        target: *mut u8,
        stride: usize,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(context) = self.context else {
            return false;
        };
        if width != self.width || height != self.height {
            return false;
        }
        let data = source.get(&frame::DATA);
        let linesize = source.get(&frame::LINESIZE);
        let planes = [target, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()];
        let strides: [c_int; 4] = [stride as c_int, 0, 0, 0];
        // SAFETY: the scaler was prepared for source's size and format and for this width and height; the caller guarantees target holds height rows of stride bytes of at least width * 4 BGRA bytes each.
        unsafe {
            av::sws_scale(
                context.as_ptr(),
                data.as_ptr().cast(),
                linesize.as_ptr(),
                0,
                source.get(&frame::HEIGHT),
                planes.as_ptr(),
                strides.as_ptr(),
            )
        };
        true
    }
}

impl Drop for Scaler {
    fn drop(&mut self) {
        if let Some(context) = self.context {
            // SAFETY: context is the scaler sws_getCachedContext returned, freed exactly once.
            unsafe { av::sws_freeContext(context.as_ptr()) };
        }
    }
}
