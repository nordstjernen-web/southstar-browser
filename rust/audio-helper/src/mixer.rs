//! Southstar — the players the audio callback mixes into the 44.1 kHz stereo device, the clock they publish, and the silent fallback output.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::ffi::clock::{Clock, Sample};

pub const MAX_PLAYERS: usize = 16;
pub const DEVICE_RATE: usize = 44100;
pub const MAX_SECONDS: usize = 1800;
pub const MAX_FLOATS: usize = 200 * 1024 * 1024;
const TOKEN_MAX: usize = 63;
const NULL_FRAMES: usize = 1024;

pub struct Player {
    pub token: Vec<u8>,
    pub used: bool,
    pub playing: bool,
    pub looping: bool,
    pub pcm: Option<Vec<f32>>,
    pub cursor: usize,
    pub volume: f32,
    pub timeline_offset: f64,
    pub seek_target: f64,
    pub seek_pending: bool,
    pub reached_end: bool,
}

impl Player {
    const EMPTY: Player = Player {
        token: Vec::new(),
        used: false,
        playing: false,
        looping: false,
        pcm: None,
        cursor: 0,
        volume: 0.0,
        timeline_offset: 0.0,
        seek_target: 0.0,
        seek_pending: false,
        reached_end: false,
    };

    pub fn frames(&self) -> usize {
        self.pcm.as_ref().map_or(0, |pcm| pcm.len() / 2)
    }

    pub fn position(&self) -> f64 {
        self.timeline_offset
            + if self.seek_pending {
                self.seek_target
            } else {
                self.cursor as f64 / DEVICE_RATE as f64
            }
    }

    pub fn apply_pending_seek(&mut self) {
        if !self.seek_pending {
            return;
        }
        let target = (self.seek_target * DEVICE_RATE as f64) as usize;
        let frames = self.frames();
        self.cursor = target.min(frames);
        if target <= frames {
            self.seek_pending = false;
            self.reached_end = false;
        }
    }

    pub fn resume_if_behind(&mut self) {
        if self.reached_end && self.cursor < self.frames() {
            self.reached_end = false;
            self.playing = true;
        }
    }
}

pub struct Mixer {
    pub players: [Player; MAX_PLAYERS],
    pub clock: Option<Clock>,
}

static MIXER: Mutex<Mixer> = Mutex::new(Mixer {
    players: [Player::EMPTY; MAX_PLAYERS],
    clock: None,
});

pub fn lock() -> MutexGuard<'static, Mixer> {
    MIXER.lock().unwrap_or_else(|e| e.into_inner())
}

impl Mixer {
    pub fn find(&self, token: &[u8]) -> Option<usize> {
        self.players.iter().position(|p| p.used && p.token == token)
    }

    pub fn alloc(&mut self, token: &[u8]) -> Option<usize> {
        if let Some(i) = self.find(token) {
            return Some(i);
        }
        let i = self.players.iter().position(|p| !p.used)?;
        self.players[i] = Player {
            token: token[..token.len().min(TOKEN_MAX)].to_vec(),
            used: true,
            volume: 1.0,
            ..Player::EMPTY
        };
        self.publish(i);
        Some(i)
    }

    pub fn release(&mut self, i: usize) {
        if !self.players[i].used {
            return;
        }
        if let Some(clock) = &self.clock {
            clock.store(i, None);
        }
        self.players[i] = Player::EMPTY;
    }

    pub fn publish(&self, i: usize) {
        let Some(clock) = &self.clock else {
            return;
        };
        let p = &self.players[i];
        clock.store(
            i,
            Some(Sample {
                playing: p.playing,
                position: p.position(),
                token: &p.token,
            }),
        );
    }

    pub fn mix(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        for i in 0..MAX_PLAYERS {
            let p = &self.players[i];
            if !p.used || !p.playing || p.pcm.is_none() {
                continue;
            }
            self.publish(i);
            let p = &mut self.players[i];
            let Some(pcm) = p.pcm.as_ref() else {
                continue;
            };
            let total = pcm.len() / 2;
            for frame in out.as_chunks_mut::<2>().0 {
                if p.cursor >= total {
                    if p.looping && total > 0 {
                        p.cursor = 0;
                    } else {
                        p.playing = false;
                        p.reached_end = true;
                        break;
                    }
                }
                frame[0] += pcm[p.cursor * 2] * p.volume;
                frame[1] += pcm[p.cursor * 2 + 1] * p.volume;
                p.cursor += 1;
            }
            if !p.playing {
                self.publish(i);
            }
        }
        for sample in &mut out[..frames * 2] {
            *sample = sample.clamp(-1.0, 1.0);
        }
    }
}

pub struct NullOutput {
    quit: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NullOutput {
    pub fn start() -> Option<NullOutput> {
        let quit = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&quit);
        let thread = std::thread::Builder::new()
            .name("ns-null-audio".into())
            .spawn(move || run_null(&flag))
            .ok()?;
        Some(NullOutput {
            quit,
            thread: Some(thread),
        })
    }
}

impl Drop for NullOutput {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run_null(quit: &AtomicBool) {
    let mut buf = vec![0f32; NULL_FRAMES * 2];
    let mut start = Instant::now();
    let mut total_frames: u64 = 0;
    while !quit.load(Ordering::SeqCst) {
        buf.fill(0.0);
        lock().mix(&mut buf);
        total_frames += NULL_FRAMES as u64;
        let target = total_frames * 1000 / DEVICE_RATE as u64;
        let now = start.elapsed().as_millis() as u64;
        if target > now {
            std::thread::sleep(Duration::from_millis(target - now));
        } else if now - target > 1000 {
            start = Instant::now();
            total_frames = 0;
        }
    }
}
