//! Southstar — Media Source Extensions streams: the appended video and audio bytes per stream, their init segments, chunk time ranges and generations.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[derive(Clone, Copy)]
pub struct Chunk {
    offset: u32,
    length: u32,
    start: f64,
    end: f64,
}

#[derive(Default)]
pub struct Track {
    pub bytes: Vec<u8>,
    init: Vec<u8>,
    chunks: Vec<Chunk>,
    pub end: f64,
    pub dirty: bool,
    pub flush_us: i64,
    pub generation: u32,
    pub gen_start: f64,
}

#[derive(Default)]
pub struct MseStream {
    pub video: Track,
    pub audio: Track,
    pub eos: bool,
    pub audio_rebased: bool,
}

pub enum Appended {
    Duplicate,
    TooLarge,
    Stored { new_generation: bool },
}

pub fn is_init_segment(d: &[u8]) -> bool {
    (d.len() >= 8 && &d[4..8] == b"ftyp") || (d.len() >= 4 && d[..4] == [0x1A, 0x45, 0xDF, 0xA3])
}

impl MseStream {
    pub fn track(&mut self, kind: u8) -> &mut Track {
        if kind == b'a' {
            &mut self.audio
        } else {
            &mut self.video
        }
    }

    pub fn track_ref(&self, kind: u8) -> &Track {
        if kind == b'a' {
            &self.audio
        } else {
            &self.video
        }
    }
}

impl Track {
    pub fn append(&mut self, data: &[u8], max_bytes: usize) -> Appended {
        let is_init = is_init_segment(data);
        if is_init && !self.init.is_empty() && !self.bytes.is_empty() && self.init == data {
            return Appended::Duplicate;
        }
        let new_generation = !self.bytes.is_empty() && is_init;
        if new_generation {
            self.bytes.clear();
            self.chunks.clear();
            self.generation = self.generation.wrapping_add(1);
            self.gen_start = -1.0;
            self.end = 0.0;
        }
        if is_init {
            self.init.clear();
            self.init.extend_from_slice(data);
        }
        if self.bytes.len() + data.len() > max_bytes {
            return Appended::TooLarge;
        }
        let offset = self.bytes.len() as u32;
        self.bytes.extend_from_slice(data);
        self.dirty = true;
        if !is_init {
            let range = crate::ffi::probe_chunk_range(&self.init, data);
            if let Some((start, end)) = range {
                if self.gen_start < 0.0 {
                    self.gen_start = start;
                }
                if end > self.end {
                    self.end = end;
                }
            }
            self.chunks.push(Chunk {
                offset,
                length: data.len() as u32,
                start: range.map_or(f64::NAN, |r| r.0),
                end: range.map_or(f64::NAN, |r| r.1),
            });
        }
        Appended::Stored { new_generation }
    }

    pub fn buffered(&self, first: &mut f64) -> f64 {
        if self.end > 0.0 && self.gen_start >= 0.0 && self.gen_start < self.end {
            *first = self.gen_start;
        }
        self.end
    }

    pub fn remove(&mut self, start: f64, end: f64) -> bool {
        let clears_all = start <= 0.001 && end + 0.05 >= self.end;
        if self.bytes.is_empty() {
            return false;
        }
        let mut kept_bytes = Vec::new();
        let mut kept_chunks = Vec::new();
        if !self.init.is_empty() {
            kept_bytes.extend_from_slice(&self.init);
        }
        let mut removed = false;
        let mut retained_unknown = false;
        let mut retained_start = -1.0;
        let mut retained_end: f64 = 0.0;
        for chunk in &self.chunks {
            let known = chunk.start.is_finite() && chunk.end.is_finite();
            if clears_all || (known && chunk.start < end && chunk.end > start) {
                removed = true;
                continue;
            }
            let offset = chunk.offset as usize;
            let length = chunk.length as usize;
            if offset > self.bytes.len() || length > self.bytes.len() - offset {
                removed = true;
                continue;
            }
            let mut kept = *chunk;
            kept.offset = kept_bytes.len() as u32;
            kept_bytes.extend_from_slice(&self.bytes[offset..offset + length]);
            kept_chunks.push(kept);
            if known {
                if retained_start < 0.0 || chunk.start < retained_start {
                    retained_start = chunk.start;
                }
                if chunk.end > retained_end {
                    retained_end = chunk.end;
                }
            } else {
                retained_unknown = true;
            }
        }
        if !removed && !clears_all {
            return false;
        }
        self.bytes = kept_bytes;
        self.chunks = kept_chunks;
        if clears_all || (!retained_unknown && self.chunks.is_empty()) {
            self.end = 0.0;
        } else if !retained_unknown {
            self.end = retained_end;
        }
        self.generation = self.generation.wrapping_add(1);
        self.gen_start = retained_start;
        self.dirty = !self.bytes.is_empty();
        self.flush_us = 0;
        true
    }
}
