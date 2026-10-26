//! Southstar — microphone capture for getUserMedia audio and Web Audio: a mono ring buffer and its time-domain and frequency views.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
#[cfg(test)]
mod tests;

pub use ffi::{acquire, fill_frequency, fill_time_domain, release};

use core::f64::consts::PI;
use std::sync::Mutex;

static GRANTS: Mutex<Vec<(usize, u32)>> = Mutex::new(Vec::new());

fn grants() -> std::sync::MutexGuard<'static, Vec<(usize, u32)>> {
    GRANTS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn grant(page: usize) {
    let mut grants = grants();
    match grants.iter_mut().find(|(key, _)| *key == page) {
        Some((_, count)) => *count = count.saturating_add(1),
        None => grants.push((page, 1)),
    }
}

pub fn revoke(page: usize) -> bool {
    let mut grants = grants();
    let Some(at) = grants.iter().position(|(key, _)| *key == page) else {
        return false;
    };
    grants[at].1 -= 1;
    if grants[at].1 == 0 {
        grants.swap_remove(at);
    }
    true
}

pub fn revoke_all(page: usize) -> u32 {
    let mut grants = grants();
    let Some(at) = grants.iter().position(|(key, _)| *key == page) else {
        return 0;
    };
    grants.swap_remove(at).1
}

pub fn granted(page: usize) -> bool {
    grants().iter().any(|(key, _)| *key == page)
}

pub const RING: usize = 16384;

pub struct Ring {
    pub samples: Box<[f32; RING]>,
    pub head: u32,
}

impl Default for Ring {
    fn default() -> Ring {
        Ring {
            samples: Box::new([0.0; RING]),
            head: 0,
        }
    }
}

impl Ring {
    pub fn clear(&mut self) {
        self.samples.fill(0.0);
        self.head = 0;
    }

    pub fn push_interleaved(&mut self, stream: &[u8], channels: i32) {
        let channels = if channels > 0 { channels as usize } else { 1 };
        let frames = stream.len() / 4 / channels;
        let mut head = self.head;
        for i in 0..frames {
            let mut acc = 0.0f32;
            for c in 0..channels {
                let at = (i * channels + c) * 4;
                acc += f32::from_ne_bytes([
                    stream[at],
                    stream[at + 1],
                    stream[at + 2],
                    stream[at + 3],
                ]);
            }
            self.samples[head as usize % RING] = acc / channels as f32;
            head = head.wrapping_add(1);
        }
        self.head = head;
    }

    fn sample_before(&self, back_from_head: i64) -> f32 {
        let idx = (self.head as i32).wrapping_sub(back_from_head as i32);
        if idx >= 0 {
            self.samples[(idx as u32) as usize % RING]
        } else {
            0.0
        }
    }

    pub fn time_domain(&self, out: &mut [u8]) {
        let n = out.len() as i64;
        for (i, slot) in out.iter_mut().enumerate() {
            let s = self.sample_before(n - i as i64);
            let v = 128 + (s * 128.0) as i32;
            *slot = v.clamp(0, 255) as u8;
        }
    }

    pub fn window(&self, n: usize) -> Vec<f32> {
        let win = if n > RING / 2 { RING } else { n * 2 };
        (0..win)
            .map(|t| self.sample_before((win - t) as i64))
            .collect()
    }
}

pub fn frequency(snap: &[f32], out: &mut [u8]) {
    let n = out.len();
    let win = snap.len();
    for (k, slot) in out.iter_mut().enumerate() {
        let freq = (k + 1) as f64 * PI / n as f64;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (t, &s) in snap.iter().enumerate() {
            re += f64::from(s) * (freq * t as f64).cos();
            im -= f64::from(s) * (freq * t as f64).sin();
        }
        let mag = (re * re + im * im).sqrt() / win as f64;
        let db = (mag * 4.0 * 255.0) as i32;
        *slot = db.clamp(0, 255) as u8;
    }
}
