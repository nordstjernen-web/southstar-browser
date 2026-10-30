//! Southstar — the helper's line protocol: open, reload, play, pause, seek, volume, loop and stop commands in, meta, pos and ended events out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

#[cfg(feature = "libav")]
use crate::decode::Sink;
use crate::decode::{self, MAX_BYTES};
#[cfg(feature = "libav")]
use crate::ffi::av;
use crate::ffi::clock::Clock;
use crate::ffi::sdl::{self, Device};
use crate::mixer::{self, DEVICE_RATE, MAX_PLAYERS, MAX_SECONDS, NullOutput};

const LINE_MAX: usize = 4095;

struct Output {
    device: Option<Device>,
    null: Option<NullOutput>,
}

#[derive(Default)]
struct Local {
    tmp_path: Option<Vec<u8>>,
    reload_size: i64,
    reload_ticks: u32,
    #[cfg(feature = "libav")]
    input: Option<av::Input>,
}

struct Helper {
    output: Output,
    locals: [Local; MAX_PLAYERS],
}

#[cfg(feature = "libav")]
struct LiveSink(usize);

#[cfg(feature = "libav")]
impl Sink for LiveSink {
    fn frames(&self) -> usize {
        mixer::lock().players[self.0].frames()
    }

    fn append(&mut self, stereo: &[f32]) -> bool {
        decode::append_stereo(&mut mixer::lock().players[self.0].pcm, stereo)
    }
}

#[cfg(feature = "libav")]
struct FreshSink(Option<Vec<f32>>);

#[cfg(feature = "libav")]
impl Sink for FreshSink {
    fn frames(&self) -> usize {
        self.0.as_ref().map_or(0, |pcm| pcm.len() / 2)
    }

    fn append(&mut self, stereo: &[f32]) -> bool {
        decode::append_stereo(&mut self.0, stereo)
    }
}

fn ticks() -> u32 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u32
}

fn emit(parts: &[&[u8]]) {
    let mut line = parts.concat();
    line.push(b'\n');
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&line);
    let _ = out.flush();
}

fn seconds(frames: usize) -> Vec<u8> {
    format!("{:.3}", frames as f64 / DEVICE_RATE as f64).into_bytes()
}

fn emit_meta(token: &[u8], frames: usize) {
    emit(&[b"meta ", token, b" ", &seconds(frames)]);
}

fn emit_error(token: &[u8], what: &[u8]) {
    emit(&[b"error ", token, b" ", what]);
}

fn tmp_dir() -> Vec<u8> {
    match std::env::var_os("TMPDIR") {
        Some(dir) if !dir.is_empty() => dir.to_string_lossy().into_owned().into_bytes(),
        _ => b"/tmp".to_vec(),
    }
}

fn remove_file(path: &[u8]) {
    let _ = std::fs::remove_file(decode::path(path));
}

fn create_temp() -> Option<String> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let dir = String::from_utf8(tmp_dir()).ok()?;
    for _ in 0..100 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let mut seed = u64::from(nanos)
            ^ (u64::from(std::process::id()) << 32)
            ^ u64::from(COUNTER.fetch_add(1, Ordering::Relaxed))
                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let suffix: String = (0..6)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ALPHABET[(seed >> 33) as usize % ALPHABET.len()] as char
            })
            .collect();
        let path = format!("{dir}/nsaudio-{suffix}");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(&path) {
            Ok(_) => return Some(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

fn write_temp_from_url(url: &[u8]) -> Option<Vec<u8>> {
    let path = create_temp()?;
    if !southstar_helper_ffi::download::to_file(url, &path, MAX_BYTES) {
        remove_file(path.as_bytes());
        return None;
    }
    Some(path.into_bytes())
}

fn local_path_for(url: &[u8]) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
    if let Some(rest) = url.strip_prefix(b"file://") {
        let rest = rest.strip_prefix(b"localhost").unwrap_or(rest);
        return Some((rest.to_vec(), None));
    }
    if url.starts_with(b"http://") || url.starts_with(b"https://") || url.starts_with(b"data:") {
        let tmp = write_temp_from_url(url)?;
        return Some((tmp.clone(), Some(tmp)));
    }
    None
}

fn discard_tmp(tmp: Option<Vec<u8>>) {
    if let Some(tmp) = tmp {
        remove_file(&tmp);
    }
}

#[cfg(feature = "libav")]
fn frames_of(i: usize) -> usize {
    mixer::lock().players[i].frames()
}

impl Helper {
    fn available(&self) -> bool {
        self.output.device.is_some() || self.output.null.is_some()
    }

    fn release(&mut self, i: usize) {
        mixer::lock().release(i);
        let local = std::mem::take(&mut self.locals[i]);
        discard_tmp(local.tmp_path);
    }

    #[cfg(feature = "libav")]
    fn pump(&mut self, i: usize) {
        av::pump(&mut self.locals[i].input, &mut LiveSink(i));
    }

    fn open(&mut self, token: &[u8], url: &[u8]) {
        let Some(i) = mixer::lock().alloc(token) else {
            emit_error(token, b"too-many-players");
            return;
        };
        self.release(i);
        let Some(i) = mixer::lock().alloc(token) else {
            return;
        };
        if !self.available() {
            emit_error(token, b"no-audio-device");
            return;
        }
        let Some((path, tmp)) = local_path_for(url) else {
            emit_error(token, b"fetch-failed");
            return;
        };
        self.locals[i].tmp_path = tmp;
        #[cfg(feature = "libav")]
        if decode::file_is_container(&path) {
            self.locals[i].input = None;
            if let Some(input) = av::Input::open(&path) {
                self.locals[i].input = Some(input);
                self.pump(i);
                emit_meta(token, frames_of(i));
                return;
            }
        }
        let Some(pcm) = decode::load(&path) else {
            emit_error(token, b"decode-failed");
            self.release(i);
            return;
        };
        let frames = {
            let mut m = mixer::lock();
            let p = &mut m.players[i];
            p.pcm = Some(pcm);
            p.cursor = 0;
            p.reached_end = false;
            p.playing = false;
            p.frames()
        };
        emit_meta(token, frames);
    }

    fn swap_pcm(&mut self, i: usize, pcm: Option<Vec<f32>>, tmp: Option<Vec<u8>>) -> usize {
        let (old, frames) = {
            let mut m = mixer::lock();
            let p = &mut m.players[i];
            let old = std::mem::replace(&mut p.pcm, pcm);
            if p.seek_pending {
                p.apply_pending_seek();
            } else {
                p.cursor = p.cursor.min(p.frames());
            }
            p.resume_if_behind();
            (old, p.frames())
        };
        drop(old);
        let old_tmp = std::mem::replace(&mut self.locals[i].tmp_path, tmp);
        discard_tmp(old_tmp);
        frames
    }

    fn reload(&mut self, token: &[u8], url: &[u8]) {
        let found = mixer::lock().find(token);
        let Some(i) = found else {
            self.open(token, url);
            return;
        };
        let Some((path, tmp)) = local_path_for(url) else {
            emit_error(token, b"fetch-failed");
            return;
        };
        #[cfg(feature = "libav")]
        {
            if self.locals[i]
                .input
                .as_ref()
                .is_some_and(|input| input.path() == path.as_slice())
            {
                discard_tmp(tmp);
                let before = frames_of(i);
                self.pump(i);
                let frames = {
                    let mut m = mixer::lock();
                    let p = &mut m.players[i];
                    if p.frames() != before {
                        p.apply_pending_seek();
                        p.resume_if_behind();
                    }
                    p.frames()
                };
                if frames != before {
                    emit_meta(token, frames);
                }
                return;
            }
            if decode::file_is_container(&path) {
                let mut fresh = av::Input::open(&path);
                if fresh.is_none() {
                    emit_error(token, b"decode-failed");
                    discard_tmp(tmp);
                    return;
                }
                let mut sink = FreshSink(None);
                av::pump(&mut fresh, &mut sink);
                self.locals[i].input = fresh;
                let frames = self.swap_pcm(i, sink.0, tmp);
                emit_meta(token, frames);
                return;
            }
        }
        let size_now = decode::file_size(&path);
        let now = ticks();
        let behind = {
            let m = mixer::lock();
            let p = &m.players[i];
            p.reached_end || p.cursor + DEVICE_RATE >= p.frames()
        };
        let min_gap = if behind { 1500 } else { 6000 };
        let local = &self.locals[i];
        let since = now.wrapping_sub(local.reload_ticks);
        if size_now >= 0
            && size_now == local.reload_size
            && local.reload_ticks != 0
            && since < 30000
        {
            discard_tmp(tmp);
            return;
        }
        if local.reload_ticks != 0 && since < min_gap {
            discard_tmp(tmp);
            return;
        }
        let Some(pcm) = decode::load(&path) else {
            emit_error(token, b"decode-failed");
            discard_tmp(tmp);
            return;
        };
        let frames = self.swap_pcm(i, Some(pcm), tmp);
        self.locals[i].reload_size = size_now;
        self.locals[i].reload_ticks = now;
        emit_meta(token, frames);
    }

    fn play(&mut self, token: &[u8]) {
        {
            let mut m = mixer::lock();
            let Some(i) = m.find(token).filter(|&i| m.players[i].pcm.is_some()) else {
                return;
            };
            let p = &mut m.players[i];
            if p.cursor >= p.frames() {
                p.cursor = 0;
            }
            p.reached_end = false;
            p.playing = true;
            m.publish(i);
        }
        emit(&[b"playing ", token]);
    }

    fn resume(&mut self, token: &[u8]) {
        let resumed = {
            let mut m = mixer::lock();
            let Some(i) = m.find(token).filter(|&i| m.players[i].pcm.is_some()) else {
                return;
            };
            let p = &mut m.players[i];
            let can = p.cursor < p.frames();
            if can {
                p.reached_end = false;
                p.playing = true;
                m.publish(i);
            }
            can
        };
        if resumed {
            emit(&[b"playing ", token]);
        }
    }

    fn pause(&mut self, token: &[u8]) {
        {
            let mut m = mixer::lock();
            let Some(i) = m.find(token).filter(|&i| m.players[i].pcm.is_some()) else {
                return;
            };
            m.players[i].playing = false;
            m.publish(i);
        }
        emit(&[b"paused ", token]);
    }

    fn seek(&mut self, token: &[u8], to: f64) {
        let mut m = mixer::lock();
        let Some(i) = m.find(token).filter(|&i| m.players[i].pcm.is_some()) else {
            return;
        };
        let p = &mut m.players[i];
        let mut seconds = to - p.timeline_offset;
        if seconds.is_nan() || seconds <= 0.0 {
            seconds = 0.0;
        }
        seconds = seconds.min(MAX_SECONDS as f64);
        let frame = (seconds * DEVICE_RATE as f64) as usize;
        let frames = p.frames();
        p.seek_target = seconds;
        p.seek_pending = frame > frames;
        p.cursor = frame.min(frames);
        p.reached_end = false;
        m.publish(i);
    }

    fn offset(&mut self, token: &[u8], seconds: f64) {
        let mut m = mixer::lock();
        let Some(i) = m.find(token) else {
            return;
        };
        m.players[i].timeline_offset = if seconds > 0.0 { seconds } else { 0.0 };
        m.publish(i);
    }

    fn volume(&mut self, token: &[u8], vol: f64) {
        let mut m = mixer::lock();
        let Some(i) = m.find(token).filter(|&i| m.players[i].pcm.is_some()) else {
            return;
        };
        let vol = if vol > 0.0 { vol.min(1.0) } else { 0.0 };
        m.players[i].volume = vol as f32;
    }

    fn set_loop(&mut self, token: &[u8], on: bool) {
        let mut m = mixer::lock();
        if let Some(i) = m.find(token) {
            m.players[i].looping = on;
        }
    }

    fn stop(&mut self, token: &[u8]) {
        let found = mixer::lock().find(token);
        if let Some(i) = found {
            self.release(i);
        }
    }

    fn poll(&mut self) {
        let mut events = Vec::new();
        {
            let mut m = mixer::lock();
            for p in m.players.iter_mut() {
                if !p.used || p.pcm.is_none() || !(p.playing || p.reached_end) {
                    continue;
                }
                events.push((p.token.clone(), p.position(), p.reached_end, p.playing));
                p.reached_end = false;
            }
        }
        for (token, position, ended, active) in events {
            if active {
                emit(&[b"pos ", &token, b" ", format!("{position:.3}").as_bytes()]);
            }
            if ended {
                emit(&[b"ended ", &token]);
            }
        }
    }
}

struct Tokens<'a>(&'a [u8]);

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Option<&'a [u8]> {
        let s = self.0;
        let start = s.iter().position(|&c| c != b' ' && c != b'\t')?;
        let s = &s[start..];
        let end = s.iter().position(|&c| c == b' ' || c == b'\t');
        let (token, rest) = match end {
            Some(end) => (&s[..end], &s[end + 1..]),
            None => (s, &s[s.len()..]),
        };
        self.0 = rest;
        Some(token)
    }

    fn rest(&mut self) -> &'a [u8] {
        let start = self
            .0
            .iter()
            .position(|&c| c != b' ')
            .unwrap_or(self.0.len());
        &self.0[start..]
    }

    fn number(&mut self, default: f64) -> f64 {
        self.next().map_or(default, atof)
    }
}

fn skip_space(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
        .unwrap_or(s.len());
    &s[start..]
}

fn atof(s: &[u8]) -> f64 {
    let s = skip_space(s);
    let end = s
        .iter()
        .position(|c| !(c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.')))
        .unwrap_or(s.len());
    let text = String::from_utf8_lossy(&s[..end]);
    (1..=text.len())
        .rev()
        .filter(|&n| text.is_char_boundary(n))
        .find_map(|n| text[..n].parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn atoi(s: &[u8]) -> i64 {
    let s = skip_space(s);
    let (negative, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let value = digits
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0i64, |acc, &c| {
            acc.saturating_mul(10).saturating_add(i64::from(c - b'0'))
        });
    if negative { -value } else { value }
}

fn start_output() -> Output {
    let device = if sdl::init_audio() {
        Device::open(DEVICE_RATE as i32)
    } else {
        None
    };
    let null = if device.is_none() {
        NullOutput::start()
    } else {
        None
    };
    Output { device, null }
}

fn command(helper: &mut Helper, line: &[u8]) -> bool {
    let mut tokens = Tokens(line);
    let Some(op) = tokens.next() else {
        helper.poll();
        return true;
    };
    match op {
        b"quit" => return false,
        b"poll" => {
            helper.poll();
            return true;
        }
        _ => {}
    }
    let Some(token) = tokens.next() else {
        return true;
    };
    match op {
        b"open" => helper.open(token, tokens.rest()),
        b"play" => helper.play(token),
        b"resume" => helper.resume(token),
        b"pause" => helper.pause(token),
        b"seek" => helper.seek(token, tokens.number(0.0)),
        b"offset" => helper.offset(token, tokens.number(0.0)),
        b"volume" => helper.volume(token, tokens.number(1.0)),
        b"loop" => helper.set_loop(token, tokens.next().is_some_and(|v| atoi(v) as i32 != 0)),
        b"stop" => helper.stop(token),
        b"reload" => helper.reload(token, tokens.rest()),
        _ => {}
    }
    true
}

pub fn run() {
    #[cfg(feature = "libav")]
    av::quiet();
    southstar_http::init();
    ticks();
    let output = start_output();
    if let Some(clock) = Clock::create() {
        let name = clock.name().to_vec();
        mixer::lock().clock = Some(clock);
        emit(&[b"clock ", &name]);
    }
    let mut helper = Helper {
        output,
        locals: Default::default(),
    };
    let ready: &[u8] = if helper.available() { b"1" } else { b"0" };
    emit(&[b"ready ", ready]);
    #[cfg(target_os = "linux")]
    crate::ffi::os::sandbox_self(&tmp_dir());

    let mut input = std::io::stdin().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.len() > LINE_MAX {
            continue;
        }
        if let Some(nul) = line.iter().position(|&c| c == 0) {
            line.truncate(nul);
        }
        while line.last().is_some_and(|&c| c == b'\n' || c == b'\r') {
            line.pop();
        }
        if !command(&mut helper, &line) {
            break;
        }
    }

    helper.output.null = None;
    helper.output.device = None;
    for i in 0..MAX_PLAYERS {
        if mixer::lock().players[i].used {
            helper.release(i);
        }
    }
    mixer::lock().clock = None;
    sdl::quit();
}
