//! Southstar — one video's playback clock, the audio and video helper commands it emits, and the stream files and decoder it builds from fetched or appended bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::ffi::{self, Player};
use crate::mse::MseStream;
use crate::store::{NsVideo, OwnedStr, str_of};
use crate::{MAX_BYTES, Sink, dbg_audio, debug, helper_enabled};

pub fn f3(x: f64) -> String {
    format!("{x:.3}")
}

pub fn us(seconds: f64) -> i64 {
    (seconds * 1e6) as i64
}

fn token(v: &NsVideo) -> &[u8] {
    str_of(&v.token).unwrap_or(b"(null)")
}

fn lossy(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

fn path_of(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(bytes).into()
    }
    #[cfg(not(unix))]
    {
        String::from_utf8_lossy(bytes).into_owned().into()
    }
}

pub fn unlink_file_url(url: &OwnedStr) {
    if let Some(path) = url.bytes().strip_prefix(b"file://") {
        let _ = std::fs::remove_file(path_of(path));
    }
}

impl Drop for NsVideo {
    fn drop(&mut self) {
        for file in [
            &self.audio_file,
            &self.video_file,
            &self.pending_audio_file,
            &self.pending_video_file,
        ]
        .into_iter()
        .flatten()
        {
            unlink_file_url(file);
        }
    }
}

impl NsVideo {
    pub fn play(&mut self, now_us: i64) {
        if self.player.is_none() || self.playing.get() {
            return;
        }
        if self.ended.get() {
            self.cur_time = 0.0;
            self.ended.set(false);
        }
        self.base_us = now_us - us(self.cur_time);
        self.playing.set(true);
    }

    pub fn pause(&mut self, now_us: i64) {
        if self.player.is_none() || !self.playing.get() {
            return;
        }
        self.cur_time = (now_us - self.base_us) as f64 / 1e6;
        self.playing.set(false);
    }

    pub fn toggle(&mut self, now_us: i64) -> bool {
        if self.player.is_none() {
            return false;
        }
        if self.playing.get() {
            self.pause(now_us);
        } else {
            self.play(now_us);
        }
        true
    }

    pub fn note_paint_rect(&mut self, x: f64, y: f64, w: f64, h: f64, fit: i32) {
        if dbg_audio() {
            debug(&format!(
                "[note-rect] {x:.0},{y:.0} {w:.0}x{h:.0} opened={}\n",
                self.video_opened.raw()
            ));
        }
        self.last_paint_us = ffi::monotonic_us();
        self.rect_x = x;
        self.rect_y = y;
        self.rect_w = w;
        self.rect_h = h;
        self.rect_fit = fit;
        if self.clip_w <= 0.5 || self.clip_h <= 0.5 {
            self.clip_x = x;
            self.clip_y = y;
            self.clip_w = w;
            self.clip_h = h;
        }
        if (x - self.sent_rect_x).abs() > 0.5
            || (y - self.sent_rect_y).abs() > 0.5
            || (w - self.sent_rect_w).abs() > 0.5
            || (h - self.sent_rect_h).abs() > 0.5
            || fit != self.sent_rect_fit
        {
            self.rect_dirty.set(true);
        }
    }

    pub fn note_paint_clip(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.clip_x = x;
        self.clip_y = y;
        self.clip_w = w;
        self.clip_h = h;
        if (x - self.sent_clip_x).abs() > 0.5
            || (y - self.sent_clip_y).abs() > 0.5
            || (w - self.sent_clip_w).abs() > 0.5
            || (h - self.sent_clip_h).abs() > 0.5
        {
            self.rect_dirty.set(true);
        }
    }

    fn set_frame_at(&mut self, seconds: f64) {
        let looping = self.looping.get();
        let frame = self
            .player
            .as_mut()
            .and_then(|p| p.frame_at(seconds, looping).0);
        if frame.is_some() {
            self.frame_texture = frame;
        }
    }

    fn adopt_player(&mut self, player: Player) {
        self.duration = player.duration();
        self.has_audio.set(player.has_audio());
        if self.natural_width <= 0 {
            self.natural_width = player.width();
        }
        if self.natural_height <= 0 {
            self.natural_height = player.height();
        }
        self.player = Some(player);
    }
}

fn audio_safe(url: &[u8]) -> bool {
    !url.is_empty() && url.iter().all(|&b| b >= 0x20 && b != 0x7f)
}

pub fn audio_start(sink: &mut Sink, v: &mut NsVideo) {
    if dbg_audio() {
        debug(&format!(
            "[audio-start] muted={} has_audio={} file={} playing={}\n",
            v.muted.raw(),
            v.has_audio.raw(),
            lossy(str_of(&v.audio_file).unwrap_or(b"-")),
            v.playing.raw()
        ));
    }
    if !sink.audio_on() || v.muted.get() {
        return;
    }
    if !v.has_audio.get() && v.audio_file.is_none() {
        return;
    }
    let Some(src) = str_of(&v.audio_file)
        .or(str_of(&v.url))
        .filter(|s| audio_safe(s))
        .map(<[u8]>::to_vec)
    else {
        return;
    };
    sink.ensure_token(v);
    if !v.audio_opened.get() {
        sink.send(&[b"open", token(v), &src]);
        if v.audio_timeline_start > 0.0 {
            sink.send(&[b"offset", token(v), f3(v.audio_timeline_start).as_bytes()]);
        }
        if v.looping.get() {
            sink.send(&[b"loop", token(v), b"1"]);
        }
        if v.volume >= 0.0 && v.volume < 1.0 {
            sink.send(&[b"volume", token(v), f3(v.volume).as_bytes()]);
        }
        v.audio_opened.set(true);
    }
    sink.send(&[b"play", token(v)]);
}

pub fn audio_pause(sink: &Sink, v: &NsVideo) {
    if sink.audio_on() && v.audio_opened.get() {
        sink.send(&[b"pause", token(v)]);
    }
}

pub fn audio_stop(sink: &Sink, v: &mut NsVideo) {
    if sink.audio_on() && v.audio_opened.get() {
        sink.send(&[b"stop", token(v)]);
        v.audio_opened.set(false);
    }
}

pub fn audio_resync(sink: &Sink, v: &NsVideo) {
    if sink.audio_on() && v.audio_opened.get() {
        sink.send(&[b"seek", token(v), b"0"]);
    }
}

pub fn audio_seek_to_current(sink: &Sink, v: &NsVideo) {
    if v.audio_opened.get() && v.cur_time > 0.0 {
        sink.send(&[b"seek", token(v), f3(v.cur_time).as_bytes()]);
    }
}

pub fn audio_seek(sink: &Sink, v: &NsVideo, seconds: &str) {
    sink.send(&[b"seek", token(v), seconds.as_bytes()]);
}

pub fn audio_volume(sink: &Sink, v: &NsVideo) {
    sink.send(&[b"volume", token(v), f3(v.volume).as_bytes()]);
}

pub fn helper_playpause(sink: &Sink, v: &NsVideo, play: bool) {
    if sink.audio_on() && v.video_opened.get() {
        let action: &[u8] = if play { b"play" } else { b"pause" };
        sink.send(&[b"video", action, token(v)]);
    }
}

pub fn helper_seek(sink: &Sink, v: &NsVideo, seconds: f64) {
    if sink.audio_on() && v.video_opened.get() {
        sink.send(&[b"video seek", token(v), f3(seconds).as_bytes()]);
    }
}

pub fn helper_stop(sink: &Sink, v: &mut NsVideo) {
    if sink.audio_on() && v.video_opened.get() {
        sink.send(&[b"video stop", token(v)]);
        v.video_opened.set(false);
    }
}

fn round(x: f64) -> String {
    (x.round() as i64 as i32).to_string()
}

pub fn helper_flush_rect(page_coords: bool, sink: &Sink, v: &mut NsVideo, now_us: i64) {
    if !v.video_opened.get() || !sink.audio_on() {
        return;
    }
    if !v.rect_dirty.get()
        && v.sent_rect_page.get() == page_coords
        && now_us - v.rect_sent_us < 1_000_000
    {
        return;
    }
    if v.rect_w <= 0.5 || v.rect_h <= 0.5 {
        return;
    }
    v.rect_dirty.set(false);
    v.rect_sent_us = now_us;
    v.sent_rect_x = v.rect_x;
    v.sent_rect_y = v.rect_y;
    v.sent_rect_w = v.rect_w;
    v.sent_rect_h = v.rect_h;
    v.sent_rect_fit = v.rect_fit;
    v.sent_rect_page.set(page_coords);
    v.sent_clip_x = v.clip_x;
    v.sent_clip_y = v.clip_y;
    v.sent_clip_w = v.clip_w;
    v.sent_clip_h = v.clip_h;
    let fields = [
        round(v.rect_x),
        round(v.rect_y),
        round(v.rect_w),
        round(v.rect_h),
        v.rect_fit.to_string(),
        round(v.clip_x),
        round(v.clip_y),
        round(v.clip_w),
        round(v.clip_h),
        if page_coords { "1" } else { "0" }.to_string(),
    ];
    let mut parts: Vec<&[u8]> = vec![b"video rect", token(v)];
    parts.extend(fields.iter().map(String::as_bytes));
    sink.send(&parts);
}

pub fn helper_composited(v: &NsVideo) -> bool {
    helper_enabled() && (v.video_opened.get() || v.mse_id != 0)
}

fn stream_file_append(file_url: Option<&[u8]>, data: &[u8], written: &mut usize) -> bool {
    let Some(path) = file_url.and_then(|u| u.strip_prefix(b"file://")) else {
        return false;
    };
    if *written > data.len() {
        return false;
    }
    if *written == data.len() {
        return true;
    }
    let Ok(mut f) = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path_of(path))
    else {
        return false;
    };
    if f.write_all(&data[*written..]).is_err() {
        return false;
    }
    *written = data.len();
    true
}

fn stream_file_create(
    sink: &mut Sink,
    subdir: &core::ffi::CStr,
    prefix: u8,
    data: &[u8],
) -> Option<OwnedStr> {
    sink.next_token = sink.next_token.wrapping_add(1);
    ffi::write_stream_file(subdir, prefix, sink.next_token, data).and_then(|u| OwnedStr::new(&u))
}

pub fn materialize_audio(
    sink: &mut Sink,
    v: &mut NsVideo,
    data: &[u8],
    generation: u32,
    start: f64,
) {
    if v.audio_file.is_some() && generation != v.audio_file_gen {
        if v.pending_audio_file.is_some() && generation == v.pending_audio_file_gen {
            stream_file_append(
                str_of(&v.pending_audio_file),
                data,
                &mut v.pending_audio_file_len,
            );
            if start >= 0.0 {
                v.pending_audio_start = start;
            }
            return;
        }
        if let Some(old) = v.pending_audio_file.take() {
            unlink_file_url(&old);
        }
        v.pending_audio_file = stream_file_create(sink, c"msaudio", b'a', data);
        if v.pending_audio_file.is_none() {
            return;
        }
        v.pending_audio_file_len = data.len();
        v.pending_audio_file_gen = generation;
        v.pending_audio_start = start;
        return;
    }
    if v.audio_file.is_some() {
        stream_file_append(str_of(&v.audio_file), data, &mut v.audio_file_len);
    }
    if v.audio_file.is_none() {
        v.audio_file = stream_file_create(sink, c"msaudio", b'a', data);
        if v.audio_file.is_none() {
            return;
        }
        v.audio_file_len = data.len();
        v.audio_file_gen = generation;
        v.audio_timeline_start = if start > 0.0 { start } else { 0.0 };
    }
    if v.audio_opened.get() {
        sink.send(&[
            b"reload",
            token(v),
            str_of(&v.audio_file).unwrap_or_default(),
        ]);
        if v.playing.get() && !v.muted.get() {
            sink.send(&[b"resume", token(v)]);
        }
    } else if v.playing.get() && !v.muted.get() {
        audio_start(sink, v);
        audio_seek_to_current(sink, v);
    }
}

pub fn materialize_video(
    sink: &mut Sink,
    v: &mut NsVideo,
    data: &[u8],
    generation: u32,
    start: f64,
) {
    if !helper_enabled() || !sink.audio_on() {
        return;
    }
    if v.video_file.is_some() && generation != v.video_file_gen {
        if v.pending_video_file.is_some() && generation == v.pending_video_file_gen {
            stream_file_append(
                str_of(&v.pending_video_file),
                data,
                &mut v.pending_video_file_len,
            );
            if start >= 0.0 {
                v.pending_video_start = start;
            }
            return;
        }
        if let Some(old) = v.pending_video_file.take() {
            unlink_file_url(&old);
        }
        v.pending_video_file = stream_file_create(sink, c"msvideo", b'v', data);
        if v.pending_video_file.is_none() {
            return;
        }
        v.pending_video_file_len = data.len();
        v.pending_video_file_gen = generation;
        v.pending_video_start = start;
        return;
    }
    if v.video_file.is_some() {
        stream_file_append(str_of(&v.video_file), data, &mut v.video_file_len);
    }
    if v.video_file.is_none() {
        v.video_file = stream_file_create(sink, c"msvideo", b'v', data);
        if v.video_file.is_none() {
            return;
        }
        v.video_file_len = data.len();
        v.video_file_gen = generation;
    }
    sink.ensure_token(v);
    if !v.video_opened.get() {
        sink.send(&[
            b"video open",
            token(v),
            str_of(&v.video_file).unwrap_or_default(),
        ]);
        v.video_opened.set(true);
        if v.cur_time > 0.0 {
            helper_seek(sink, v, v.cur_time);
        }
        if v.playing.get() {
            helper_playpause(sink, v, true);
        }
    }
}

pub fn activate_pending(sink: &Sink, v: &mut NsVideo) {
    if v.pending_video_file.is_some()
        && v.pending_video_file_len >= 4096
        && v.pending_video_start >= 0.0
        && v.cur_time + 0.35 >= v.pending_video_start
    {
        let old = v.video_file.take();
        v.video_file = v.pending_video_file.take();
        v.video_file_len = v.pending_video_file_len;
        v.video_file_gen = v.pending_video_file_gen;
        v.pending_video_file_len = 0;
        sink.send(&[
            b"video reload",
            token(v),
            str_of(&v.video_file).unwrap_or_default(),
        ]);
        helper_seek(sink, v, v.cur_time);
        if v.playing.get() {
            helper_playpause(sink, v, true);
        }
        if let Some(old) = old {
            unlink_file_url(&old);
        }
    }
    if v.pending_audio_file.is_some()
        && v.pending_audio_file_len >= 4096
        && v.pending_audio_start >= 0.0
        && v.cur_time + 0.35 >= v.pending_audio_start
    {
        let old = v.audio_file.take();
        v.audio_file = v.pending_audio_file.take();
        v.audio_file_len = v.pending_audio_file_len;
        v.audio_file_gen = v.pending_audio_file_gen;
        v.audio_timeline_start = v.pending_audio_start;
        v.pending_audio_file_len = 0;
        sink.send(&[
            b"reload",
            token(v),
            str_of(&v.audio_file).unwrap_or_default(),
        ]);
        sink.send(&[b"offset", token(v), f3(v.audio_timeline_start).as_bytes()]);
        sink.send(&[b"seek", token(v), f3(v.cur_time).as_bytes()]);
        if v.playing.get() && !v.muted.get() {
            sink.send(&[b"resume", token(v)]);
        }
        if let Some(old) = old {
            unlink_file_url(&old);
        }
    }
}

pub fn build_player(sink: &mut Sink, v: &mut NsVideo, body: Option<&[u8]>) {
    let Some(body) = body.filter(|b| b.len() <= MAX_BYTES) else {
        return;
    };
    let Some(player) = Player::new(body) else {
        return;
    };
    v.adopt_player(player);
    v.set_frame_at(v.cur_time);
    if v.has_audio.get()
        && v.audio_file.is_none()
        && str_of(&v.url).is_some_and(|u| u.starts_with(b"blob:"))
    {
        materialize_audio(sink, v, body, 0, 0.0);
    }
    if v.playing.get() {
        if v.base_us == 0 {
            v.base_us = ffi::monotonic_us() - us(v.cur_time);
        }
        audio_start(sink, v);
        audio_seek_to_current(sink, v);
    } else if v.autoplay.get() {
        v.play(ffi::monotonic_us());
        audio_start(sink, v);
        sink.event(v, c"play", v.cur_time);
    }
}

pub fn extend_timed(player: &mut Player, data: &[u8]) -> bool {
    let t0 = ffi::monotonic_us();
    let ok = player.extend(data);
    if ffi::env_set(c"NS_PROFILE") {
        debug(&format!(
            "[profile] mse extend {:6.1}ms  {}b ok={}\n",
            (ffi::monotonic_us() - t0) as f64 / 1000.0,
            data.len(),
            i32::from(ok)
        ));
    }
    ok
}

fn mse_attach_player(v: &mut NsVideo, s: &MseStream) {
    if v.player.is_some() || s.video.bytes.len() < 4096 {
        return;
    }
    let Some(player) = Player::new(&s.video.bytes) else {
        return;
    };
    v.adopt_player(player);
    v.loaded.set(true);
    if !helper_enabled() {
        v.set_frame_at(v.cur_time);
    }
    if v.playing.get() && v.base_us == 0 {
        v.base_us = ffi::monotonic_us() - us(v.cur_time);
    }
}

pub fn mse_sync(sink: &mut Sink, s: &mut MseStream, v: &mut NsVideo) {
    let helper_owns = helper_composited(v);
    match v.player.as_mut() {
        None => {
            mse_attach_player(v, s);
            if v.player.is_some() {
                v.buf_sent.set(false);
            }
        }
        Some(player) if !helper_owns => {
            if !s.video.bytes.is_empty() && player.extend(&s.video.bytes) {
                v.buf_sent.set(false);
            }
        }
        Some(_) => {
            if !s.video.bytes.is_empty() {
                v.buf_sent.set(false);
            }
        }
    }
    if s.video.end > 0.0
        && let Some(player) = v.player.as_mut()
    {
        player.note_end(s.video.end);
        let d = player.duration();
        if d > v.duration {
            v.duration = d;
        }
    }
    let now = ffi::monotonic_us();
    if s.video.bytes.len() >= 4096 && s.video.dirty && (s.eos || now - s.video.flush_us > 250_000) {
        s.video.dirty = false;
        s.video.flush_us = now;
        materialize_video(
            sink,
            v,
            &s.video.bytes,
            s.video.generation,
            s.video.gen_start,
        );
    }
    let has_side_audio = !s.audio.bytes.is_empty();
    if has_side_audio
        && s.audio.bytes.len() >= 4096
        && s.audio.dirty
        && (s.eos || now - s.audio.flush_us > 250_000)
    {
        s.audio.dirty = false;
        s.audio.flush_us = now;
        materialize_audio(
            sink,
            v,
            &s.audio.bytes,
            s.audio.generation,
            s.audio.gen_start,
        );
        if s.audio_rebased {
            s.audio_rebased = false;
            if v.audio_opened.get() {
                sink.send(&[b"seek", token(v), b"0"]);
            }
        }
    }
    if !has_side_audio
        && v.player.is_some()
        && v.has_audio.get()
        && v.audio_file.is_none()
        && !s.video.bytes.is_empty()
    {
        materialize_audio(
            sink,
            v,
            &s.video.bytes,
            s.video.generation,
            s.video.gen_start,
        );
    }
}
