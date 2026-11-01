//! Southstar — H.264 and AAC decoding through the Windows Media Foundation transforms, with an inert stand-in on other systems.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![cfg_attr(not(windows), forbid(unsafe_code))]

pub mod bitstream;
#[cfg(not(windows))]
mod stub;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use stub as platform;
#[cfg(windows)]
use windows as platform;

pub use platform::{AudioDecoder, VideoDecoder, aac_available, h264_available};

pub enum Sent {
    Accepted,
    Full,
    Failed,
}

pub enum Received<T> {
    Output(T),
    NeedInput,
    Failed,
}

pub struct Picture<'a> {
    pub luma: &'a [u8],
    pub chroma: &'a [u8],
    pub stride: usize,
    pub width: u32,
    pub height: u32,
    pub time: Option<i64>,
}

impl Picture<'_> {
    pub fn luma_row(&self, row: usize) -> Option<&[u8]> {
        let start = row.checked_mul(self.stride)?;
        self.luma.get(start..start + self.width as usize)
    }

    pub fn chroma_row(&self, row: usize) -> Option<&[u8]> {
        let start = row.checked_mul(self.stride)?;
        let width = (self.width as usize).div_ceil(2) * 2;
        self.chroma.get(start..start + width)
    }
}

pub struct Pcm<'a> {
    bytes: &'a [u8],
    float: bool,
    pub rate: u32,
    pub channels: u32,
    pub time: Option<i64>,
}

impl Pcm<'_> {
    pub fn frames(&self) -> usize {
        let width = if self.float { 4 } else { 2 };
        self.bytes.len() / (width * self.channels.max(1) as usize)
    }

    pub fn write_floats(&self, out: &mut [f32]) {
        if self.float {
            for (sample, bytes) in out.iter_mut().zip(self.bytes.chunks_exact(4)) {
                *sample = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            }
        } else {
            for (sample, bytes) in out.iter_mut().zip(self.bytes.chunks_exact(2)) {
                *sample = f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0;
            }
        }
    }
}
