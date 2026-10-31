//! Southstar — the owner of the libswresample resampler that turns decoded audio frames into interleaved floats.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::ptr::NonNull;

use crate::decode::{Codec, Frame};
use crate::{self as av, AVChannelLayout, SwrContext, frame};

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
