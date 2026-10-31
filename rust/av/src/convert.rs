//! Southstar — owners of the libswscale scaler that turns decoded frames into BGRA and the libswresample resampler that turns them into interleaved floats.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr::{self, NonNull};

use crate::decode::{Codec, Frame};
use crate::{self as av, AVChannelLayout, SwrContext, SwsContext, frame};

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

pub struct Resampler(NonNull<SwrContext>);

impl Resampler {
    pub fn to_float(codec: &Codec, rate: c_int, sample_fmt: c_int) -> Option<Resampler> {
        // SAFETY: allocates an unconfigured resampler or returns null.
        let resampler = Resampler(NonNull::new(unsafe { av::swr_alloc() })?);
        let layout = codec.channel_layout();
        resampler.set_layout(c"in_chlayout", layout);
        resampler.set_layout(c"out_chlayout", layout);
        resampler.set_int(c"in_sample_rate", i64::from(rate));
        resampler.set_int(c"out_sample_rate", i64::from(rate));
        resampler.set_sample_fmt(c"in_sample_fmt", sample_fmt);
        resampler.set_sample_fmt(c"out_sample_fmt", av::AV_SAMPLE_FMT_FLT);
        // SAFETY: resampler.0 is an allocated, configured resampler.
        let initialized = unsafe { av::swr_init(resampler.0.as_ptr()) };
        (initialized >= 0).then_some(resampler)
    }

    fn set_layout(&self, name: &CStr, layout: *const AVChannelLayout) {
        // SAFETY: self.0 is an allocated resampler, name is NUL-terminated and layout points at a live channel layout libav copies.
        unsafe { av::av_opt_set_chlayout(self.0.as_ptr().cast(), name.as_ptr(), layout, 0) };
    }

    fn set_int(&self, name: &CStr, value: i64) {
        // SAFETY: self.0 is an allocated resampler and name is NUL-terminated.
        unsafe { av::av_opt_set_int(self.0.as_ptr().cast(), name.as_ptr(), value, 0) };
    }

    fn set_sample_fmt(&self, name: &CStr, format: c_int) {
        // SAFETY: self.0 is an allocated resampler and name is NUL-terminated.
        unsafe { av::av_opt_set_sample_fmt(self.0.as_ptr().cast(), name.as_ptr(), format, 0) };
    }

    pub fn out_samples(&self, frame: &Frame) -> c_int {
        // SAFETY: self.0 is an initialized resampler.
        unsafe { av::swr_get_out_samples(self.0.as_ptr(), frame.get(&frame::NB_SAMPLES)) }
    }

    pub fn convert_into(
        &self,
        frame: &Frame,
        channels: c_int,
        dst: &mut Vec<f32>,
        out_samples: c_int,
    ) -> c_int {
        let at = dst.len();
        let room = dst.capacity() - at;
        let out = dst.spare_capacity_mut().as_mut_ptr().cast::<u8>();
        let out_samples = out_samples.min((room / channels.max(1) as usize) as c_int);
        // SAFETY: self.0 was initialized for frame's layout and format; out has room for out_samples interleaved float frames of channels samples, and frame's planes hold nb_samples samples.
        let got = unsafe {
            av::swr_convert(
                self.0.as_ptr(),
                &out,
                out_samples,
                frame.get(&frame::EXTENDED_DATA).cast::<*const u8>(),
                frame.get(&frame::NB_SAMPLES),
            )
        };
        if got > 0 {
            let filled = (got as usize * channels as usize).min(room);
            // SAFETY: swr_convert initialized filled floats past at, all within the capacity.
            unsafe { dst.set_len(at + filled) };
        }
        got
    }
}

impl Drop for Resampler {
    fn drop(&mut self) {
        let mut context = self.0.as_ptr();
        // SAFETY: context is the resampler swr_alloc returned, freed exactly once.
        unsafe { av::swr_free(&mut context) };
    }
}
