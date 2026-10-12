//! Southstar — the helper's line protocol: commands from the shell on stdin, events back on stdout, and the table of up to four players.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::{BufRead, Write};

use crate::ffi::decoder;
use crate::ffi::os::{self, now_us};
use crate::player::Player;

const MAX_PLAYERS: usize = 4;

pub fn emit(line: std::fmt::Arguments<'_>) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_fmt(line);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

struct Players {
    slots: Vec<Player>,
    open_seq: u64,
}

impl Players {
    fn find(&self, token: &str) -> Option<usize> {
        self.slots
            .iter()
            .position(|player| player.shared.token == token)
    }

    fn get(&self, token: &str) -> Option<&Player> {
        self.find(token).map(|index| &self.slots[index])
    }

    fn open(&mut self, token: &str, url: &[u8]) {
        if let Some(index) = self.find(token) {
            self.slots.swap_remove(index).release(false);
        }
        if self.slots.len() >= MAX_PLAYERS {
            let mut victim = 0;
            for (index, candidate) in self.slots.iter().enumerate() {
                let current = &self.slots[victim];
                let (candidate_playing, current_playing) = (candidate.playing(), current.playing());
                if (!candidate_playing && current_playing)
                    || (candidate_playing == current_playing
                        && candidate.open_seq < current.open_seq)
                {
                    victim = index;
                }
            }
            self.slots.swap_remove(victim).release(true);
        }
        self.open_seq += 1;
        match Player::spawn(token, url, self.open_seq) {
            Some(player) => self.slots.push(player),
            None => emit!("error {token} thread-failed"),
        }
    }

    fn reload(&mut self, token: &str, url: &[u8]) {
        let Some(player) = self.get(token) else {
            self.open(token, url);
            return;
        };
        let mut state = player.shared.lock();
        state.path = url.to_vec();
        state.want_reopen = true;
        state.reset_queue = true;
    }

    fn play(&self, token: &str) {
        let Some(player) = self.get(token) else {
            return;
        };
        {
            let mut state = player.shared.lock();
            if !state.playing {
                state.base_us = now_us() - (state.cur * 1e6) as i64;
                state.playing = true;
            }
            state.store_clock(state.cur);
        }
        emit!("playing {token}");
    }

    fn pause(&self, token: &str) {
        let Some(player) = self.get(token) else {
            return;
        };
        {
            let mut state = player.shared.lock();
            state.cur = state.position();
            state.playing = false;
            state.store_clock(state.cur);
        }
        emit!("paused {token}");
    }

    fn seek(&self, token: &str, seconds: f64) {
        let Some(player) = self.get(token) else {
            return;
        };
        let mut state = player.shared.lock();
        state.seek_to = if seconds < 0.0 { 0.0 } else { seconds };
        state.want_seek = true;
        state.reset_queue = true;
    }

    fn resync(&self, token: &str, seconds: f64) {
        if seconds < 0.0 {
            return;
        }
        let Some(player) = self.get(token) else {
            return;
        };
        let mut state = player.shared.lock();
        if state.playing {
            let clock = (now_us() - state.base_us) as f64 / 1e6;
            if (seconds - clock).abs() > 0.04 {
                state.base_us = now_us() - (seconds * 1e6) as i64;
            }
            state.cur = seconds;
            state.store_clock(seconds);
        }
    }

    fn stop(&mut self, token: &str) {
        if let Some(index) = self.find(token) {
            self.slots.swap_remove(index).release(true);
        }
    }
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn next_token<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let start = rest.iter().position(|&b| !is_blank(b))?;
    let tail = &rest[start..];
    let end = tail.iter().position(|&b| is_blank(b)).unwrap_or(tail.len());
    *rest = tail.get(end + 1..).unwrap_or_default();
    Some(&tail[..end])
}

fn trim_spaces(rest: &[u8]) -> &[u8] {
    let start = rest.iter().position(|&b| b != b' ').unwrap_or(rest.len());
    &rest[start..]
}

fn atof(text: Option<&[u8]>) -> f64 {
    let text = String::from_utf8_lossy(text.unwrap_or_default());
    let text = text.trim_start();
    (1..=text.len())
        .rev()
        .filter(|&end| text.is_char_boundary(end))
        .find_map(|end| {
            let prefix = &text[..end];
            let numeric = prefix
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'));
            if numeric { prefix.parse().ok() } else { None }
        })
        .unwrap_or(0.0)
}

pub fn run() {
    decoder::quiet_logging();
    emit!("ready 1");
    os::sandbox_self();

    let mut players = Players {
        slots: Vec::with_capacity(MAX_PLAYERS),
        open_seq: 0,
    };
    let mut input = std::io::stdin().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        while line.last().is_some_and(|&b| b == b'\n' || b == b'\r') {
            line.pop();
        }
        let mut rest = line.as_slice();
        let Some(op) = next_token(&mut rest) else {
            continue;
        };
        if op == b"quit" {
            break;
        }
        let Some(token) = next_token(&mut rest) else {
            continue;
        };
        let token = String::from_utf8_lossy(token);
        match op {
            b"open" => players.open(&token, trim_spaces(rest)),
            b"reload" => players.reload(&token, trim_spaces(rest)),
            b"play" => players.play(&token),
            b"pause" => players.pause(&token),
            b"seek" => players.seek(&token, atof(next_token(&mut rest))),
            b"resync" => players.resync(&token, atof(next_token(&mut rest))),
            b"stop" => players.stop(&token),
            _ => {}
        }
    }

    for player in players.slots.drain(..) {
        player.release(false);
    }
}
