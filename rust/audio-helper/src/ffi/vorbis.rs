//! Southstar — decoding Ogg/Vorbis to PCM with libvorbisfile, its struct layout computed by meson.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long, c_void};
use core::ptr;

use southstar_av::Field;
use southstar_av::layout::lookup;

use crate::decode::{Collector, Decoded};

const LAYOUT: Option<&str> = option_env!("NS_VORBIS_LAYOUT");

const fn value(key: &str) -> i64 {
    match LAYOUT {
        Some(layout) => lookup(layout, key),
        None => 0,
    }
}

#[repr(C)]
struct VorbisInfo {
    _private: [u8; 0],
}

const FILE_BYTES: usize = value("sizeof.OggVorbis_File") as usize;
const INFO_CHANNELS: Field<VorbisInfo, c_int> = Field::at(value("vorbis_info.channels"));
const INFO_RATE: Field<VorbisInfo, c_long> = Field::at(value("vorbis_info.rate"));

#[repr(C)]
struct Callbacks {
    read: Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut c_void) -> usize>,
    seek: Option<unsafe extern "C" fn(*mut c_void, i64, c_int) -> c_int>,
    close: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    tell: Option<unsafe extern "C" fn(*mut c_void) -> c_long>,
}

unsafe extern "C" {
    fn ov_open_callbacks(
        datasource: *mut c_void,
        vf: *mut c_void,
        initial: *const c_char,
        ibytes: c_long,
        callbacks: Callbacks,
    ) -> c_int;
    fn ov_info(vf: *mut c_void, link: c_int) -> *mut VorbisInfo;
    fn ov_read_float(
        vf: *mut c_void,
        pcm_channels: *mut *mut *mut f32,
        samples: c_int,
        bitstream: *mut c_int,
    ) -> c_long;
    fn ov_clear(vf: *mut c_void) -> c_int;
}

struct Memory<'a> {
    data: &'a [u8],
    pos: usize,
}

unsafe extern "C" fn read(out: *mut c_void, size: usize, nmemb: usize, src: *mut c_void) -> usize {
    // SAFETY: src is the Memory decode() passed, alive for the whole decode and used by this one file only.
    let m = unsafe { &mut *src.cast::<Memory>() };
    let want = size.saturating_mul(nmemb).min(m.data.len() - m.pos);
    // SAFETY: vorbisfile hands a buffer with room for size * nmemb bytes, and want is no more than that.
    let out = unsafe { core::slice::from_raw_parts_mut(out.cast::<u8>(), want) };
    out.copy_from_slice(&m.data[m.pos..m.pos + want]);
    m.pos += want;
    want.checked_div(size).unwrap_or(0)
}

unsafe extern "C" fn seek(src: *mut c_void, offset: i64, whence: c_int) -> c_int {
    // SAFETY: src is the Memory decode() passed, alive for the whole decode and used by this one file only.
    let m = unsafe { &mut *src.cast::<Memory>() };
    let base = match whence {
        0 => 0,
        1 => m.pos as i64,
        2 => m.data.len() as i64,
        _ => return -1,
    };
    let np = base.saturating_add(offset);
    if np < 0 || np > m.data.len() as i64 {
        return -1;
    }
    m.pos = np as usize;
    0
}

unsafe extern "C" fn tell(src: *mut c_void) -> c_long {
    // SAFETY: src is the Memory decode() passed, alive for the whole decode.
    let m = unsafe { &*src.cast::<Memory>() };
    m.pos as c_long
}

struct File(Vec<u64>);

impl File {
    fn ptr(&mut self) -> *mut c_void {
        self.0.as_mut_ptr().cast()
    }

    fn info(&mut self, link: c_int) -> Option<(c_long, c_int)> {
        // SAFETY: self holds an opened OggVorbis_File; the result is null or info owned by it.
        let info = unsafe { ov_info(self.ptr(), link) };
        if info.is_null() {
            return None;
        }
        // SAFETY: info is a live vorbis_info and the offset comes from the installed headers.
        let rate = unsafe { INFO_RATE.get(info) };
        // SAFETY: info is a live vorbis_info and the offset comes from the installed headers.
        let channels = unsafe { INFO_CHANNELS.get(info) };
        Some((rate, channels))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        // SAFETY: self holds an opened OggVorbis_File, cleared exactly once.
        unsafe { ov_clear(self.ptr()) };
    }
}

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    if FILE_BYTES == 0 {
        return None;
    }
    let mut mem = Memory {
        data: bytes,
        pos: 0,
    };
    let callbacks = Callbacks {
        read: Some(read),
        seek: Some(seek),
        close: None,
        tell: Some(tell),
    };
    let mut vf = vec![0u64; FILE_BYTES.div_ceil(8)];
    // SAFETY: vf has room for an OggVorbis_File, mem outlives it (the file is cleared before decode returns) and the callbacks only read mem.
    let opened = unsafe {
        ov_open_callbacks(
            (&raw mut mem).cast(),
            vf.as_mut_ptr().cast(),
            ptr::null(),
            0,
            callbacks,
        )
    };
    if opened < 0 {
        return None;
    }
    let mut vf = File(vf);
    let (rate, channels) = vf.info(-1)?;
    if rate <= 0 || channels < 1 {
        return None;
    }
    let rate = rate as c_int;
    let mut pcm = Collector::new(rate, channels);
    let mut bitstream: c_int = 0;
    let mut interleaved = Vec::new();
    loop {
        let mut chans: *mut *mut f32 = ptr::null_mut();
        // SAFETY: vf holds an opened OggVorbis_File; chans and bitstream are writable outputs.
        let got = unsafe { ov_read_float(vf.ptr(), &mut chans, 4096, &mut bitstream) };
        if got == 0 {
            break;
        }
        if got < 0 {
            continue;
        }
        if vf.info(bitstream).is_none_or(|(_, link)| link != channels) {
            continue;
        }
        let (got, ch) = (got as usize, channels as usize);
        interleaved.clear();
        interleaved.resize(got * ch, 0f32);
        for c in 0..ch {
            // SAFETY: chans is vorbisfile's array of one plane pointer per channel of the current link, which has ch channels.
            let plane = unsafe { chans.wrapping_add(c).read() };
            // SAFETY: each plane holds got samples, valid until the next read.
            let plane = unsafe { core::slice::from_raw_parts(plane, got) };
            for (i, &s) in plane.iter().enumerate() {
                interleaved[i * ch + c] = s;
            }
        }
        if pcm.push(&interleaved)? {
            break;
        }
    }
    drop(vf);
    pcm.finish(rate, channels)
}
