//! Southstar — unit tests for the MPEG-1 decoders on generated, partly corrupt program streams: every stream must decode to its end without a panic or an endless loop.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: MIT

use crate::Plm;

const STEP_LIMIT: usize = 100_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn pes(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![0, 0, 1, kind];
    let len = payload.len() + 1;
    v.push((len >> 8) as u8);
    v.push(len as u8);
    v.push(0x0F);
    v.extend_from_slice(payload);
    v
}

fn mpeg(r: &mut Rng) -> Vec<u8> {
    let mut b: Vec<u8> = vec![0, 0, 1, 0xBA, 0x21, 0, 1, 0, 1, 0x80, 0, 1];
    b.extend_from_slice(&[0, 0, 1, 0xBB, 0, 6, 0x80, 0, 1, 0x04, 0x04, 0xFF]);
    let mut video = vec![0, 0, 1, 0xB3];
    let w = 1 + r.below(64) as u32;
    let h = 1 + r.below(64) as u32;
    video.push((w >> 4) as u8);
    video.push(((w & 15) << 4) as u8 | (h >> 8) as u8);
    video.push(h as u8);
    video.push(0x10 | r.below(16) as u8);
    for _ in 0..4 {
        video.push(r.byte());
    }
    video.push(r.byte() & 0xFC);
    for _ in 0..r.below(6) {
        video.extend_from_slice(&[0, 0, 1, 0x00]);
        let pt = 1 + r.below(3) as u8;
        video.push(r.byte());
        video.push((r.byte() & 0xC7) | (pt << 3));
        for _ in 0..4 {
            video.push(r.byte());
        }
        for s in 0..1 + r.below(5) {
            video.extend_from_slice(&[0, 0, 1, 1 + s as u8]);
            for _ in 0..r.below(400) {
                let x = r.byte();
                video.push(if r.below(4) == 0 { 0xFF } else { x });
            }
        }
    }
    let mut audio = Vec::new();
    for _ in 0..r.below(4) {
        audio.extend_from_slice(&[0xFF, 0xFC | (r.byte() & 1)]);
        audio.push(r.byte());
        audio.push(r.byte());
        for _ in 0..r.below(800) {
            audio.push(r.byte());
        }
    }
    if r.below(5) == 0 {
        audio = vec![0; r.below(20)];
    }
    for chunk in video.chunks(1 + r.below(2000)) {
        b.extend(pes(0xE0, chunk));
        if !audio.is_empty() {
            let n = audio.len().min(1 + r.below(700));
            let a: Vec<u8> = audio.drain(..n).collect();
            b.extend(pes(0xC0, &a));
        }
    }
    while !audio.is_empty() {
        let n = audio.len().min(700);
        let a: Vec<u8> = audio.drain(..n).collect();
        b.extend(pes(0xC0, &a));
    }
    for _ in 0..r.below(3) {
        let i = r.below(b.len());
        b[i] = r.byte();
    }
    b.extend_from_slice(&[0xFF; 16]);
    b
}

fn decodes_to_the_end(bytes: Vec<u8>, audio: bool) {
    let mut plm = Plm::new(bytes);
    let mut steps = 0;
    if audio {
        plm.set_video_enabled(false);
        plm.set_audio_enabled(true);
        while plm.decode_audio().is_some() {
            steps += 1;
            assert!(steps < STEP_LIMIT, "audio never ends");
        }
    } else {
        plm.set_audio_enabled(false);
        while plm.decode_video().is_some() {
            steps += 1;
            assert!(steps < STEP_LIMIT, "video never ends");
        }
    }
}

#[test]
fn audio_survives_corrupt_streams() {
    let mut seeds = Rng(0x1234567);
    for _ in 0..400 {
        let seed = seeds.next() | 1;
        decodes_to_the_end(mpeg(&mut Rng(seed)), true);
    }
}

#[test]
fn video_reaches_the_end_after_a_bad_picture_header() {
    decodes_to_the_end(mpeg(&mut Rng(12477951132730869137)), false);
    let mut seeds = Rng(0x7654321);
    for _ in 0..200 {
        let seed = seeds.next() | 1;
        decodes_to_the_end(mpeg(&mut Rng(seed)), false);
    }
}
