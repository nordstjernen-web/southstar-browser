//! Southstar — inline video decoding: MPEG-1 through rust/plmpeg and, with libav, Matroska/WebM and ISO-BMFF containers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "libav")]
mod lav;
mod mpeg1;

use crate::ffi::Texture;

const MAX_DIMENSION: i32 = 16384;

pub enum Next {
    Frame(Option<Texture>, f64),
    Stalled,
    Ended,
}

enum Backend {
    Mpeg1(mpeg1::Mpeg1),
    #[cfg(feature = "libav")]
    Lav(lav::Demuxer),
}

pub struct Decoder {
    backend: Backend,
    width: i32,
    height: i32,
    has_audio: bool,
    duration: f64,
    demuxed_end: f64,
    cur_tex: Option<Texture>,
    cur_time: f64,
    pending_tex: Option<Texture>,
    pending_time: f64,
}

pub struct Player(Box<Decoder>);

fn bytes_are_mpeg1(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..3] == [0x00, 0x00, 0x01] && matches!(bytes[3], 0xBA | 0xB3)
}

#[cfg(feature = "libav")]
fn bytes_are_matroska(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
}

#[cfg(feature = "libav")]
fn bytes_are_isobmff(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp"
}

fn dimensions_valid(w: i32, h: i32) -> bool {
    w > 0 && h > 0 && w <= MAX_DIMENSION && h <= MAX_DIMENSION
}

pub fn pixel_buffer(len: usize, fill: u8) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len).ok()?;
    buf.resize(len, fill);
    Some(buf)
}

pub fn copy_bytes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len()).ok()?;
    copy.extend_from_slice(bytes);
    Some(copy)
}

impl Decoder {
    fn with_backend(backend: Backend, width: i32, height: i32, has_audio: bool) -> Decoder {
        Decoder {
            backend,
            width,
            height,
            has_audio,
            duration: 0.0,
            demuxed_end: 0.0,
            cur_tex: None,
            cur_time: -1.0,
            pending_tex: None,
            pending_time: -1.0,
        }
    }

    pub fn new(bytes: &[u8]) -> Option<Decoder> {
        if bytes.len() < 8 {
            return None;
        }
        if bytes_are_mpeg1(bytes) {
            let mut plm = mpeg1::Mpeg1::open(copy_bytes(bytes)?)?;
            let (w, h) = (plm.width(), plm.height());
            if !dimensions_valid(w, h) {
                return None;
            }
            let has_audio = plm.audio_streams() > 0;
            let duration = plm.duration();
            let mut d = Decoder::with_backend(Backend::Mpeg1(plm), w, h, has_audio);
            d.duration = duration;
            d.demuxed_end = duration;
            return Some(d);
        }
        #[cfg(feature = "libav")]
        if bytes_are_matroska(bytes) || bytes_are_isobmff(bytes) {
            let opened = lav::Demuxer::open(bytes)?;
            if !dimensions_valid(opened.width, opened.height) {
                return None;
            }
            let mut d = Decoder::with_backend(
                Backend::Lav(opened.demuxer),
                opened.width,
                opened.height,
                opened.has_audio,
            );
            d.duration = opened.duration;
            d.demuxed_end = opened.demuxed_end;
            return Some(d);
        }
        None
    }

    pub fn has_audio(&self) -> bool {
        self.has_audio
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn duration(&self) -> f64 {
        self.duration
    }

    pub fn buffered_end(&self) -> f64 {
        self.demuxed_end
    }

    pub fn note_end(&mut self, end: f64) {
        if end <= 0.0 {
            return;
        }
        if end > self.demuxed_end {
            self.demuxed_end = end;
        }
        if end > self.duration {
            self.duration = end;
        }
    }

    pub fn extend(&mut self, bytes: &[u8]) -> bool {
        match &mut self.backend {
            #[cfg(feature = "libav")]
            Backend::Lav(lav) => lav.extend(bytes),
            Backend::Mpeg1(_) => {
                let _ = bytes;
                false
            }
        }
    }

    fn seek(&mut self, seconds: f64) -> bool {
        match &mut self.backend {
            #[cfg(feature = "libav")]
            Backend::Lav(lav) => lav.seek_to(seconds),
            Backend::Mpeg1(_) => {
                let _ = seconds;
                false
            }
        }
    }

    fn rewind(&mut self) {
        match &mut self.backend {
            Backend::Mpeg1(plm) => plm.rewind(),
            #[cfg(feature = "libav")]
            Backend::Lav(lav) => lav.rewind(),
        }
    }

    fn next(&mut self) -> Next {
        let (w, h) = (self.width, self.height);
        match &mut self.backend {
            Backend::Mpeg1(plm) => plm.next(w, h),
            #[cfg(feature = "libav")]
            Backend::Lav(lav) => lav.next(w, h, self.pending_time),
        }
    }

    pub fn frame_at(&mut self, seconds: f64, looping: bool) -> (Option<&Texture>, bool) {
        let seconds = if seconds < 0.0 { 0.0 } else { seconds };
        let backward = seconds + 1e-4 < self.cur_time;
        let far_forward = self.cur_time >= 0.0 && seconds > self.cur_time + 2.0;
        if backward || far_forward {
            if !self.seek(seconds) && backward {
                self.rewind();
            }
            self.cur_time = -1.0;
            self.pending_tex = None;
            self.pending_time = -1.0;
        }

        let mut changed = false;
        let mut ended = false;
        loop {
            if self.pending_time >= 0.0 {
                if self.pending_time <= seconds || self.cur_time < 0.0 {
                    self.cur_tex = self.pending_tex.take();
                    self.cur_time = self.pending_time;
                    self.pending_time = -1.0;
                    changed = true;
                    continue;
                }
                break;
            }
            match self.next() {
                Next::Frame(tex, pts) => {
                    self.pending_tex = tex;
                    self.pending_time = pts.max(0.0);
                }
                Next::Stalled => break,
                Next::Ended => {
                    ended = !looping;
                    break;
                }
            }
        }
        let frame = if changed { self.cur_tex.as_ref() } else { None };
        (frame, ended)
    }
}

impl Player {
    pub fn new(bytes: &[u8]) -> Option<Player> {
        Decoder::new(bytes).map(|d| Player(Box::new(d)))
    }

    pub fn has_audio(&self) -> bool {
        self.0.has_audio()
    }

    pub fn width(&self) -> i32 {
        self.0.width()
    }

    pub fn height(&self) -> i32 {
        self.0.height()
    }

    pub fn duration(&self) -> f64 {
        self.0.duration()
    }

    pub fn buffered_end(&self) -> f64 {
        self.0.buffered_end()
    }

    pub fn note_end(&mut self, end: f64) {
        self.0.note_end(end);
    }

    pub fn extend(&mut self, bytes: &[u8]) -> bool {
        self.0.extend(bytes)
    }

    pub fn frame_at(&mut self, seconds: f64, looping: bool) -> (Option<Texture>, bool) {
        let (frame, ended) = self.0.frame_at(seconds, looping);
        (frame.cloned(), ended)
    }
}

pub fn probe_chunk_range(init: &[u8], chunk: &[u8]) -> Option<(f64, f64)> {
    #[cfg(feature = "libav")]
    {
        lav::probe_chunk_range(init, chunk)
    }
    #[cfg(not(feature = "libav"))]
    {
        let _ = (init, chunk);
        None
    }
}

#[cfg(feature = "libav")]
fn codec_family(codec: &[u8]) -> Option<lav::Codec> {
    use lav::Codec;
    let has = |p: &[u8]| codec.starts_with(p);
    let contains = |n: &[u8]| codec.windows(n.len()).any(|w| w == n);
    let family = if has(b"avc1") || has(b"avc3") {
        Codec::H264
    } else if has(b"hvc1") || has(b"hev1") {
        Codec::Hevc
    } else if has(b"av01") {
        Codec::Av1
    } else if has(b"vp09") || has(b"vp9") {
        Codec::Vp9
    } else if has(b"vp08") || has(b"vp8") {
        Codec::Vp8
    } else if has(b"mp4a.40") || has(b"mp4a.66") || has(b"mp4a.67") || has(b"mp4a.68") {
        Codec::Aac
    } else if has(b"mp4a.69") || has(b"mp4a.6b") || contains(b"mp3") {
        Codec::Mp3
    } else if codec == b"ac-3" || has(b"mp4a.a5") {
        Codec::Ac3
    } else if codec == b"ec-3" || has(b"mp4a.a6") {
        Codec::Eac3
    } else if contains(b"flac") {
        Codec::Flac
    } else if contains(b"alac") {
        Codec::Alac
    } else if contains(b"opus") {
        Codec::Opus
    } else if contains(b"vorbis") {
        Codec::Vorbis
    } else {
        return None;
    };
    Some(family)
}

pub fn codec_available(codec: &[u8]) -> bool {
    #[cfg(feature = "libav")]
    {
        !codec.is_empty() && codec_family(codec).is_some_and(lav::decoder_exists)
    }
    #[cfg(not(feature = "libav"))]
    {
        let _ = codec;
        false
    }
}
