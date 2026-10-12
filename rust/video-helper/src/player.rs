//! Southstar — one video player: its playback clock shared with the shell and the decode thread that keeps its frame ring filled.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::ffi::decoder::{Decoder, Next};
use crate::ffi::os::now_us;
use crate::ffi::ring::{Ring, RingView};

#[derive(Default)]
pub struct State {
    pub path: Vec<u8>,
    pub playing: bool,
    pub quit: bool,
    pub want_reopen: bool,
    pub reset_queue: bool,
    pub seek_to: f64,
    pub want_seek: bool,
    pub cur: f64,
    pub base_us: i64,
    duration: f64,
    ring: Option<RingView>,
}

impl State {
    pub fn position(&self) -> f64 {
        if self.playing {
            (now_us() - self.base_us) as f64 / 1e6
        } else {
            self.cur
        }
    }

    pub fn store_clock(&self, position: f64) {
        if let Some(ring) = self.ring {
            ring.store_clock(position, self.playing);
        }
    }

    fn interrupted(&self) -> bool {
        self.quit || self.want_seek || self.want_reopen
    }
}

pub struct Shared {
    pub token: String,
    state: Mutex<State>,
}

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct Player {
    pub shared: Arc<Shared>,
    pub open_seq: u64,
    thread: JoinHandle<Option<Ring>>,
}

impl Player {
    pub fn spawn(token: &str, path: &[u8], open_seq: u64) -> Option<Player> {
        let shared = Arc::new(Shared {
            token: token.to_owned(),
            state: Mutex::new(State {
                path: path.to_vec(),
                ..State::default()
            }),
        });
        let name: String = token.chars().filter(|&c| c != '\0').take(8).collect();
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name(format!("decode-{name}"))
            .spawn(move || decode(&worker))
            .ok()?;
        Some(Player {
            shared,
            open_seq,
            thread,
        })
    }

    pub fn playing(&self) -> bool {
        self.shared.lock().playing
    }

    pub fn release(self, emit_closed: bool) {
        self.shared.lock().quit = true;
        let ring = self.thread.join().ok().flatten();
        if emit_closed {
            emit!(
                "closed {} {}",
                self.shared.token,
                ring.as_ref().map_or("", Ring::name)
            );
        }
        self.shared.lock().ring = None;
        drop(ring);
    }
}

fn sleep_ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

fn local_path_for(url: &[u8]) -> &[u8] {
    match url.strip_prefix(b"file://") {
        Some(path) => path.strip_prefix(b"localhost").unwrap_or(path),
        None => url,
    }
}

fn wait_for_space(shared: &Shared, ring: Option<RingView>) -> bool {
    let Some(ring) = ring else {
        return false;
    };
    while !ring.has_space() {
        let interrupted = {
            let mut state = shared.lock();
            let position = state.position();
            state.cur = position;
            state.store_clock(position);
            state.interrupted() || !state.playing
        };
        if interrupted {
            return false;
        }
        sleep_ms(2);
    }
    true
}

fn decode(shared: &Shared) -> Option<Ring> {
    let mut decoder: Option<Decoder> = None;
    let mut ring: Option<Ring> = None;
    let mut eof_since = 0i64;
    let mut stalled_reported = false;
    let mut decoded_until = 0.0f64;

    loop {
        let (playing, want_reopen, growth_reopen, keep, path);
        let (mut want_seek, mut seek_to);
        {
            let mut state = shared.lock();
            if state.quit {
                break;
            }
            playing = state.playing;
            want_reopen = state.want_reopen;
            want_seek = state.want_seek;
            let reset_queue = state.reset_queue;
            growth_reopen = decoder.is_some() && want_reopen && !reset_queue;
            seek_to = state.seek_to;
            keep = state.position();
            state.cur = keep;
            state.want_reopen = false;
            state.want_seek = false;
            state.reset_queue = false;
            if reset_queue {
                if let Some(view) = state.ring {
                    view.reset();
                }
                decoded_until = 0.0;
            }
            state.store_clock(keep);
            path = state.path.clone();
        }
        let local = local_path_for(&path);

        if decoder.is_none() || want_reopen {
            decoder = None;
            let Some((opened, duration)) = Decoder::open(local) else {
                if !want_reopen {
                    emit!("error {} decode-failed", shared.token);
                }
                sleep_ms(200);
                continue;
            };
            {
                let mut state = shared.lock();
                if duration > state.duration {
                    state.duration = duration;
                }
                if keep <= 0.01 && state.playing {
                    state.base_us = now_us();
                }
            }
            if ring.is_none() {
                let Some(created) = Ring::create(opened.width, opened.height) else {
                    emit!("error {} shm-failed", shared.token);
                    sleep_ms(500);
                    continue;
                };
                let view = created.view();
                let duration = {
                    let mut state = shared.lock();
                    state.ring = Some(view);
                    state.store_clock(keep);
                    state.duration
                };
                emit!(
                    "shm {} {} {} {} {} {:.3}",
                    shared.token,
                    created.name(),
                    view.width(),
                    view.height(),
                    view.stride(),
                    duration
                );
                ring = Some(created);
            }
            decoder = Some(opened);
            if keep > 0.01 {
                want_seek = true;
                seek_to = if growth_reopen && decoded_until > 0.25 {
                    decoded_until - 0.25
                } else {
                    keep
                };
            }
        }
        let Some(active) = decoder.as_mut() else {
            continue;
        };

        if want_seek {
            active.seek(seek_to);
            let clock_position = if growth_reopen { keep } else { seek_to };
            let mut state = shared.lock();
            state.cur = clock_position;
            state.base_us = now_us() - (clock_position * 1e6) as i64;
            state.store_clock(clock_position);
        }

        if !playing {
            sleep_ms(20);
            continue;
        }

        let view = ring.as_ref().map(Ring::view);
        if !wait_for_space(shared, view) {
            continue;
        }
        let Some(view) = view else {
            continue;
        };

        let clock = {
            let mut state = shared.lock();
            let clock = state.position();
            state.cur = clock;
            state.store_clock(clock);
            clock
        };

        match active.next_frame(local) {
            Next::Grew => {
                shared.lock().want_reopen = true;
                eof_since = 0;
                stalled_reported = false;
                continue;
            }
            Next::Idle => {
                if eof_since == 0 {
                    eof_since = now_us();
                }
                let published = view.published();
                let released = view.released();
                if !stalled_reported
                    && published <= released.wrapping_add(1)
                    && now_us() - eof_since > 3_000_000
                {
                    emit!("stalled {}", shared.token);
                    stalled_reported = true;
                }
                sleep_ms(if published.wrapping_sub(released) > 1 {
                    5
                } else {
                    50
                });
                continue;
            }
            Next::Frame => {}
        }

        let pts = active.frame_pts(clock);
        let frame_duration = active.default_duration;
        decoded_until = decoded_until.max(pts + frame_duration);

        if !shared.lock().interrupted() {
            view.publish(active, pts, frame_duration);
            eof_since = 0;
            stalled_reported = false;
        }
        active.unref_frame();
    }

    drop(decoder);
    ring
}
