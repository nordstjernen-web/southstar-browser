//! Southstar — the shared-memory audio clock the shell reads each player's position from, laid out as rust/media-shm declares.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_media_shm::{
    AUDIO_CLOCK_SLOTS, AudioClockHdr, MEDIA_CLOCK_PLAYING, MEDIA_CLOCK_USED, SharedMemory,
    TokenSample,
};

const BYTES: usize = core::mem::size_of::<AudioClockHdr>();

pub struct Sample<'a> {
    pub playing: bool,
    pub position: f64,
    pub token: &'a [u8],
}

pub struct Clock {
    memory: SharedMemory,
    name: Vec<u8>,
}

impl Clock {
    pub fn create() -> Option<Clock> {
        let name = format!("/nsaud-{}", std::process::id()).into_bytes();
        let memory = SharedMemory::create(&name, BYTES)?;
        memory.audio_clock()?.initialize();
        Some(Clock { memory, name })
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn store(&self, index: usize, sample: Option<Sample>) {
        let Ok(index) = u32::try_from(index) else {
            return;
        };
        let Some(clock) = self.memory.audio_clock() else {
            return;
        };
        if index >= AUDIO_CLOCK_SLOTS {
            return;
        }
        let monotonic_us = southstar_glib::monotonic_us();
        match sample {
            None => clock.clear(index, monotonic_us),
            Some(sample) => {
                let mut token = [0u8; 64];
                let n = sample.token.len().min(63);
                token[..n].copy_from_slice(&sample.token[..n]);
                clock.store(
                    index,
                    TokenSample {
                        flags: MEDIA_CLOCK_USED
                            | if sample.playing {
                                MEDIA_CLOCK_PLAYING
                            } else {
                                0
                            },
                        monotonic_us,
                        position: sample.position,
                        token,
                    },
                );
            }
        }
    }
}
