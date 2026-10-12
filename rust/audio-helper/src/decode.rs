//! Southstar — sniffing a media file and decoding it whole to 44.1 kHz stereo PCM through the first decoder that accepts it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::ffi::sdl::AudioStream;
use crate::ffi::{mp3, plm};
use crate::mixer::{DEVICE_RATE, MAX_FLOATS, MAX_SECONDS};

pub const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub struct Decoded {
    pub pcm: Vec<f32>,
    pub rate: c_int,
    pub channels: c_int,
}

pub fn max_floats(rate: c_int, channels: c_int) -> usize {
    (rate.max(0) as usize)
        .saturating_mul(channels.max(0) as usize)
        .saturating_mul(MAX_SECONDS)
        .min(MAX_FLOATS)
}

pub struct Collector {
    pcm: Vec<f32>,
    max: usize,
}

impl Collector {
    pub fn new(rate: c_int, channels: c_int) -> Collector {
        Collector {
            pcm: Vec::new(),
            max: max_floats(rate, channels),
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Option<bool> {
        self.pcm.try_reserve(samples.len()).ok()?;
        self.pcm.extend_from_slice(samples);
        Some(self.pcm.len() >= self.max)
    }

    pub fn finish(self, rate: c_int, channels: c_int) -> Option<Decoded> {
        (!self.pcm.is_empty()).then_some(Decoded {
            pcm: self.pcm,
            rate,
            channels,
        })
    }
}

#[cfg(feature = "libav")]
pub trait Sink {
    fn frames(&self) -> usize;
    fn append(&mut self, stereo: &[f32]) -> bool;
}

#[cfg(feature = "libav")]
pub fn append_stereo(pcm: &mut Option<Vec<f32>>, stereo: &[f32]) -> bool {
    if stereo.is_empty() {
        return true;
    }
    let have = pcm.as_ref().map_or(0, Vec::len);
    if have + stereo.len() > MAX_FLOATS {
        return false;
    }
    let pcm = pcm.get_or_insert_with(Vec::new);
    if pcm.try_reserve(stereo.len()).is_err() {
        return false;
    }
    pcm.extend_from_slice(stereo);
    true
}

#[cfg(unix)]
pub fn path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub fn path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub fn file_size(bytes: &[u8]) -> i64 {
    std::fs::metadata(path(bytes)).map_or(-1, |m| m.len() as i64)
}

fn read_file(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut file = File::open(path(bytes)).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 || len > MAX_BYTES {
        return None;
    }
    let mut data = Vec::new();
    data.try_reserve_exact(len as usize).ok()?;
    file.read_to_end(&mut data).ok()?;
    (data.len() as u64 == len).then_some(data)
}

fn is_mpeg1(b: &[u8]) -> bool {
    b.len() >= 4 && b[..3] == [0, 0, 1] && (b[3] == 0xBA || b[3] == 0xB3)
}

#[cfg(feature = "vorbisfile")]
fn is_ogg(b: &[u8]) -> bool {
    b.starts_with(b"OggS")
}

#[cfg(feature = "libav")]
fn is_container(b: &[u8]) -> bool {
    b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
        || b.starts_with(b"OggS")
        || (b.len() >= 8 && &b[4..8] == b"ftyp")
}

#[cfg(feature = "libav")]
pub fn file_is_container(bytes: &[u8]) -> bool {
    let Ok(file) = File::open(path(bytes)) else {
        return false;
    };
    let mut head = Vec::with_capacity(12);
    file.take(12).read_to_end(&mut head).is_ok() && is_container(&head)
}

#[cfg(feature = "libav")]
fn decode_container(bytes: &[u8]) -> Option<Decoded> {
    if is_container(bytes) {
        crate::ffi::av::decode_memory(bytes)
    } else {
        None
    }
}

#[cfg(not(feature = "libav"))]
fn decode_container(_bytes: &[u8]) -> Option<Decoded> {
    None
}

fn decode(bytes: &[u8]) -> Option<Decoded> {
    let mut decoded = if is_mpeg1(bytes) {
        plm::decode(bytes)
    } else {
        decode_container(bytes)
    };
    decoded = decoded.or_else(|| mp3::decode(bytes));
    #[cfg(feature = "libav")]
    {
        decoded = decoded.or_else(|| crate::ffi::av::decode_memory(bytes));
    }
    #[cfg(feature = "vorbisfile")]
    {
        decoded = decoded.or_else(|| {
            if is_ogg(bytes) {
                crate::ffi::vorbis::decode(bytes)
            } else {
                None
            }
        });
    }
    #[cfg(feature = "opusfile")]
    {
        decoded = decoded.or_else(|| crate::ffi::opus::decode(bytes));
    }
    decoded
}

fn resample(src: &Decoded) -> Option<Vec<f32>> {
    if src.channels < 1 || src.rate <= 0 || src.pcm.is_empty() {
        return None;
    }
    if core::mem::size_of_val(src.pcm.as_slice()) > c_int::MAX as usize {
        return None;
    }
    let mut stream = AudioStream::to_stereo(src.channels, src.rate, DEVICE_RATE as c_int)?;
    let frames = src.pcm.len() / src.channels as usize;
    if !stream.put(&src.pcm[..frames * src.channels as usize]) {
        return None;
    }
    stream.flush();
    stream.get_all()
}

pub fn load(path: &[u8]) -> Option<Vec<f32>> {
    let bytes = read_file(path)?;
    let decoded = decode(&bytes);
    drop(bytes);
    resample(&decoded?)
}
