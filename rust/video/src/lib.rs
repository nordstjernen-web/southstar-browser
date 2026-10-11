//! Southstar — inline <video> playback: the per-page video cache that finds <video> elements, fetches their streams, posters and subtitle tracks, advances frames off the animation tick and drives the audio and video helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod media;
mod mse;
mod vtt;

use core::ffi::{CStr, c_void};
use std::collections::{HashMap, HashSet};

use southstar_dom::Node;
use southstar_layout::BoxRef;

use ffi::{Cues, NsVideo, OwnedStr, PendingPtr, Response, VideoPtr, str_of};
use media::{
    audio_pause, audio_resync, audio_seek, audio_seek_to_current, audio_start, audio_stop,
    audio_volume, build_player, extend_timed, f3, helper_flush_rect, helper_playpause, helper_seek,
    helper_stop, materialize_audio, materialize_video, mse_sync, us,
};
use mse::{Appended, MseStream};

pub use media::helper_composited;

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MEDIA_SRC_ATTR: &CStr = c"data-nd-media-src";
const MEDIA_STREAM_ATTR: &CStr = c"data-nd-media-stream";

#[derive(Clone, Copy)]
pub enum FetchKind {
    Video,
    Extend,
    Audio,
    Poster,
    Track,
}

struct JsEvent {
    node: *const c_void,
    kind: &'static CStr,
    value: f64,
}

#[derive(Default)]
pub struct Sink {
    audio: Option<ffi::AudioOut>,
    js: Option<ffi::JsOut>,
    queue: Vec<JsEvent>,
    next_token: u32,
}

impl Sink {
    fn audio_on(&self) -> bool {
        self.audio.is_some()
    }

    fn send(&self, parts: &[&[u8]]) {
        if let Some(out) = &self.audio {
            out.send(&parts.join(&b' '));
        }
    }

    fn event(&mut self, v: &NsVideo, kind: &'static CStr, value: f64) {
        if self.js.is_some() && !v.dom_node.is_null() {
            self.queue.push(JsEvent {
                node: v.dom_node,
                kind,
                value,
            });
        }
    }

    fn ensure_token(&mut self, v: &mut NsVideo) {
        if v.token.is_none() {
            self.next_token = self.next_token.wrapping_add(1);
            v.token = OwnedStr::new(format!("nv{}", self.next_token).as_bytes());
        }
    }
}

#[derive(Default)]
struct NodeMediaState {
    playing: Option<bool>,
    muted: Option<bool>,
    volume: Option<f64>,
    seek: Option<f64>,
}

pub struct Cache {
    videos: Vec<(Vec<u8>, VideoPtr)>,
    requested: HashSet<Vec<u8>>,
    streams: HashMap<u32, MseStream>,
    node_state: HashMap<usize, NodeMediaState>,
    pending: Vec<PendingPtr>,
    base_url: Option<Vec<u8>>,
    sink: Sink,
    next_seq: u32,
    page_coords: bool,
}

fn helper_enabled() -> bool {
    ffi::env_set(c"NS_VIDEO_HELPER")
}

fn dbg_audio() -> bool {
    ffi::env_set(c"NS_DBG_AUDIO")
}

fn debug(message: &str) {
    southstar_glib::stderr_write(message.as_bytes());
}

fn non_negative(seconds: f64) -> f64 {
    if seconds < 0.0 { 0.0 } else { seconds }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn lookup(videos: &[(Vec<u8>, VideoPtr)], key: &[u8]) -> Option<VideoPtr> {
    videos.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
}

fn mse_key(id: u32) -> Vec<u8> {
    format!("ndmse:{id}").into_bytes()
}

fn parse_u64(s: &[u8]) -> u64 {
    let mut i = 0;
    while s.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut value: u64 = 0;
    let mut overflow = false;
    while let Some(&d) = s.get(i).filter(|c| c.is_ascii_digit()) {
        match value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(d - b'0')))
        {
            Some(v) => value = v,
            None => overflow = true,
        }
        i += 1;
    }
    if overflow {
        u64::MAX
    } else if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

fn mse_url_id(url: &[u8]) -> u32 {
    url.strip_prefix(b"blob:nd-mse/")
        .map_or(0, |rest| parse_u64(rest) as u32)
}

fn stream_key(abs_url: &[u8]) -> Vec<u8> {
    let mse_id = mse_url_id(abs_url);
    if mse_id != 0 {
        return mse_key(mse_id);
    }
    match find(abs_url, b"#ndms=") {
        Some(marker) => abs_url[..marker].to_vec(),
        None => abs_url.to_vec(),
    }
}

fn url_is_growing_stream(url: Option<&[u8]>) -> bool {
    url.and_then(|u| find(u, b"#ndms=").map(|m| &u[m..]))
        .is_some_and(|marker| find(marker, b"&eos").is_none())
}

fn stream_growing(streams: &HashMap<u32, MseStream>, v: &NsVideo) -> bool {
    if v.mse_id != 0 {
        return streams.get(&v.mse_id).is_none_or(|s| !s.eos);
    }
    url_is_growing_stream(str_of(&v.url))
}

fn buffered_end(streams: &HashMap<u32, MseStream>, v: &NsVideo) -> f64 {
    if v.mse_id != 0
        && let Some(s) = streams.get(&v.mse_id)
    {
        if s.video.end > 0.0 && s.audio.end > 0.0 {
            return if s.video.end < s.audio.end {
                s.video.end
            } else {
                s.audio.end
            };
        }
        if s.video.end > 0.0 {
            return s.video.end;
        }
        if s.audio.end > 0.0 {
            return s.audio.end;
        }
    }
    v.player.as_ref().map_or(0.0, ffi::Player::buffered_end)
}

#[cfg(feature = "libav")]
const INLINE_EXTENSIONS: &[&[u8]] = &[
    b".mpg", b".mpeg", b".m1v", b".mpeg1", b".mpg1", b".mp4", b".m4v", b".webm",
];
#[cfg(not(feature = "libav"))]
const INLINE_EXTENSIONS: &[&[u8]] = &[b".mpg", b".mpeg", b".m1v", b".mpeg1", b".mpg1"];

pub fn url_is_inline_video(url: &[u8]) -> bool {
    if url.starts_with(b"blob:") {
        return true;
    }
    let n = url
        .iter()
        .position(|&b| b == b'?' || b == b'#')
        .unwrap_or(url.len());
    let path = &url[..n];
    INLINE_EXTENSIONS
        .iter()
        .any(|ext| n >= ext.len() && path[n - ext.len()..].eq_ignore_ascii_case(ext))
}

fn fetchable_scheme(url: &[u8]) -> bool {
    [&b"http://"[..], b"https://", b"file://", b"blob:"]
        .iter()
        .any(|p| url.starts_with(p))
}

fn attr<'a>(n: Option<Node<'a>>, name: &CStr) -> Option<&'a [u8]> {
    n.and_then(|n| n.attr(name)).map(CStr::to_bytes)
}

fn non_empty_attr<'a>(n: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    attr(Some(n), name).filter(|s| !s.is_empty())
}

fn node_ptr(n: Node<'_>) -> *const c_void {
    n.as_ptr().cast()
}

fn pick_track(dom: Node<'_>) -> Option<Node<'_>> {
    let mut child = dom.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if !c.is_element() || c.name().is_none_or(|n| n.to_bytes() != b"track") {
            continue;
        }
        if c.attr(c"default").is_none() {
            continue;
        }
        if let Some(kind) = non_empty_attr(c, c"kind")
            && !kind.eq_ignore_ascii_case(b"subtitles")
            && !kind.eq_ignore_ascii_case(b"captions")
        {
            continue;
        }
        if non_empty_attr(c, c"src").is_some() {
            return Some(c);
        }
    }
    None
}

fn inherit_node_playback(v: &mut NsVideo, previous: &NsVideo) {
    v.playing = previous.playing;
    v.muted = previous.muted;
    v.volume = previous.volume;
    v.cur_time = previous.cur_time;
    v.prev_tick_time = previous.cur_time;
    v.last_emit_time = previous.cur_time;
    if v.playing.get() {
        v.base_us = ffi::monotonic_us() - us(v.cur_time);
    }
}

fn release_node(sink: &Sink, v: &mut NsVideo) {
    audio_stop(sink, v);
    helper_stop(sink, v);
    v.playing.set(false);
    v.dom_node = core::ptr::null();
}

pub fn toggle(sink: &mut Sink, v: &mut NsVideo, now_us: i64) -> bool {
    if v.player.is_none() && !v.video_opened.get() {
        return false;
    }
    if v.playing.get() && v.muted.get() {
        v.muted.set(false);
        audio_start(sink, v);
        audio_seek_to_current(sink, v);
        sink.event(v, c"unmuted", v.cur_time);
        return true;
    }
    let was_playing = v.playing.get();
    let restart = !was_playing && v.ended.get();
    if v.player.is_some() {
        v.toggle(now_us);
    } else if was_playing {
        if v.base_us > 0 {
            v.cur_time = (now_us - v.base_us) as f64 / 1e6;
        }
        v.playing.set(false);
    } else {
        if restart {
            v.cur_time = 0.0;
            v.ended.set(false);
        }
        v.base_us = now_us - us(v.cur_time);
        v.playing.set(true);
    }
    sink.event(v, if was_playing { c"pause" } else { c"play" }, v.cur_time);
    if was_playing {
        audio_pause(sink, v);
    } else {
        audio_start(sink, v);
    }
    if restart {
        if v.audio_opened.get() {
            audio_seek(sink, v, "0");
        }
        helper_seek(sink, v, 0.0);
    }
    helper_playpause(sink, v, !was_playing);
    true
}

pub fn fetched(cache: &mut Cache, v: &mut NsVideo, kind: FetchKind, resp: &Response) {
    let body = resp.body();
    let bounded = body.filter(|b| b.len() <= MAX_BYTES);
    let sink = &mut cache.sink;
    match kind {
        FetchKind::Extend => {
            let Some(body) = bounded else {
                return;
            };
            if v.player.is_none() {
                build_player(sink, v, Some(body));
                v.loaded.set(true);
            } else if v.player.as_mut().is_some_and(|p| extend_timed(p, body)) {
                v.buf_sent.set(false);
                if v.has_audio.get() && v.audio_url.is_none() {
                    materialize_audio(sink, v, body, 0, 0.0);
                }
            } else if let Some(fresh) = ffi::Player::new(body) {
                v.duration = fresh.duration();
                v.has_audio.set(fresh.has_audio());
                v.player = Some(fresh);
                v.buf_sent.set(false);
                if v.playing.get() {
                    v.base_us = ffi::monotonic_us() - us(v.cur_time);
                }
            }
        }
        FetchKind::Video => {
            build_player(sink, v, body);
            if let Some(body) = bounded
                && helper_enabled()
            {
                if v.autoplay.get() && !v.playing.get() {
                    v.playing.set(true);
                    v.base_us = ffi::monotonic_us();
                }
                materialize_video(sink, v, body, 0, 0.0);
            }
            v.loaded.set(true);
        }
        FetchKind::Audio => {
            if let Some(body) = bounded {
                materialize_audio(sink, v, body, 0, 0.0);
            }
        }
        FetchKind::Poster => {
            if let Some((texture, w, h)) = body.and_then(ffi::decode_image) {
                v.poster_texture = Some(texture);
                v.poster_url = resp.final_url().and_then(OwnedStr::new);
                if v.natural_width <= 0 {
                    v.natural_width = w;
                }
                if v.natural_height <= 0 {
                    v.natural_height = h;
                }
            }
        }
        FetchKind::Track => {
            if let Some(body) = bounded {
                v.cues = Some(Box::new(Cues(vtt::parse(body))));
            }
        }
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        if self.sink.audio_on() {
            for (_, p) in &self.videos {
                let v = p.get();
                audio_stop(&self.sink, v);
                helper_stop(&self.sink, v);
            }
        }
        ffi::mark_dead(&self.pending);
        for (_, p) in self.videos.drain(..) {
            p.free();
        }
    }
}

impl Cache {
    fn new() -> Cache {
        Cache {
            videos: Vec::new(),
            requested: HashSet::new(),
            streams: HashMap::new(),
            node_state: HashMap::new(),
            pending: Vec::new(),
            base_url: None,
            sink: Sink::default(),
            next_seq: 0,
            page_coords: false,
        }
    }

    fn node_state(&mut self, dom_node: *const c_void) -> &mut NodeMediaState {
        self.node_state.entry(dom_node as usize).or_default()
    }

    fn apply_node_state(&mut self, v: &mut NsVideo, dom_node: *const c_void) {
        let Some(st) = self.node_state.remove(&(dom_node as usize)) else {
            return;
        };
        if let Some(muted) = st.muted {
            v.muted.set(muted);
        }
        if let Some(volume) = st.volume {
            v.volume = volume;
        }
        if let Some(t) = st.seek {
            v.cur_time = t;
            v.prev_tick_time = t;
            v.last_emit_time = t;
            if v.playing.get() {
                v.base_us = ffi::monotonic_us() - us(t);
            }
        }
        if let Some(playing) = st.playing {
            if v.player.is_some() && playing != v.playing.get() {
                let now = ffi::monotonic_us();
                if playing {
                    v.play(now);
                    audio_start(&mut self.sink, v);
                } else {
                    v.pause(now);
                    audio_pause(&self.sink, v);
                }
                helper_playpause(&self.sink, v, playing);
            } else {
                v.playing.set(playing);
            }
        }
    }

    fn newest(&self, pick: impl Fn(&NsVideo) -> bool) -> Option<VideoPtr> {
        let mut best: Option<VideoPtr> = None;
        for (_, p) in &self.videos {
            let cand = p.get();
            if !pick(cand) {
                continue;
            }
            if best.is_none_or(|b| cand.seq > b.get().seq) {
                best = Some(*p);
            }
        }
        best
    }

    fn find_by_node(&self, dom_node: *const c_void) -> Option<VideoPtr> {
        self.newest(|v| v.dom_node == dom_node && !v.is_camera.get())
    }

    fn find_mse_rollover(&self) -> Option<VideoPtr> {
        self.newest(|v| v.mse_id != 0 && v.playing.get() && !v.is_camera.get())
    }

    fn helper_event(&mut self, token: &[u8], kind: &[u8]) -> bool {
        let Some(p) = self
            .videos
            .iter()
            .map(|(_, p)| *p)
            .find(|p| str_of(&p.get().token) == Some(token))
        else {
            return false;
        };
        let v = p.get();
        let stalled = kind == b"stalled";
        if !stalled && (kind != b"ended" || v.ended.get()) {
            return false;
        }
        if stalled || stream_growing(&self.streams, v) {
            v.stalled.set(true);
            v.stall_since_us = ffi::monotonic_us();
            if v.playing.get() {
                v.base_us = v.stall_since_us - us(v.cur_time);
            }
            self.sink.event(v, c"waiting", v.cur_time);
            return true;
        }
        let ended_at = v.duration;
        v.playing.set(false);
        v.ended.set(true);
        v.duration = ended_at;
        if ended_at > 0.0 {
            v.cur_time = ended_at;
        }
        v.prev_tick_time = v.cur_time;
        v.last_emit_time = v.cur_time;
        audio_stop(&self.sink, v);
        helper_stop(&self.sink, v);
        self.sink.event(v, c"ended", v.cur_time);
        true
    }

    fn seek_node(&mut self, dom_node: *const c_void, seconds: f64, now_us: i64) -> bool {
        let found = self.find_by_node(dom_node);
        if found.is_some_and(|p| p.get().is_camera.get()) {
            return false;
        }
        let Some(v) = found.map(VideoPtr::get) else {
            self.node_state(dom_node).seek = Some(non_negative(seconds));
            return true;
        };
        if v.player.is_none() {
            v.cur_time = non_negative(seconds);
            v.prev_tick_time = v.cur_time;
            v.last_emit_time = v.cur_time;
            return true;
        }
        let mut t = non_negative(seconds);
        if v.duration > 0.0 && t > v.duration {
            t = v.duration;
        }
        v.cur_time = t;
        v.prev_tick_time = t;
        v.last_emit_time = t;
        v.ended
            .set(v.duration > 0.0 && t >= v.duration && !v.looping.get());
        if v.playing.get() {
            v.base_us = now_us - us(t);
        }
        let looping = v.looping.get();
        let frame = v.player.as_mut().and_then(|p| p.frame_at(t, looping).0);
        if frame.is_some() {
            v.frame_texture = frame;
        }
        if v.audio_opened.get() {
            audio_seek(&self.sink, v, &f3(t));
        }
        helper_seek(&self.sink, v, t);
        self.sink.event(v, c"pos", t);
        true
    }

    fn set_node_playing(&mut self, dom_node: *const c_void, play: bool, now_us: i64) -> bool {
        let found = self.find_by_node(dom_node);
        if found.is_some_and(|p| p.get().is_camera.get()) {
            return false;
        }
        let Some(v) = found.map(VideoPtr::get) else {
            self.node_state(dom_node).playing = Some(play);
            return true;
        };
        if v.player.is_none() && !v.video_opened.get() {
            v.playing.set(play);
            return true;
        }
        if play == v.playing.get() {
            return false;
        }
        let restart = play && v.ended.get();
        let sink = &mut self.sink;
        if play {
            if v.player.is_some() {
                v.play(now_us);
            } else {
                if restart {
                    v.cur_time = 0.0;
                    v.ended.set(false);
                }
                v.base_us = now_us - us(v.cur_time);
                v.playing.set(true);
            }
            audio_start(sink, v);
            if restart {
                if v.audio_opened.get() {
                    audio_seek(sink, v, "0");
                }
                helper_seek(sink, v, 0.0);
            }
        } else {
            if v.player.is_some() {
                v.pause(now_us);
            } else {
                if v.base_us > 0 {
                    v.cur_time = (now_us - v.base_us) as f64 / 1e6;
                }
                v.playing.set(false);
            }
            audio_pause(sink, v);
        }
        helper_playpause(sink, v, play);
        true
    }

    fn set_node_volume(&mut self, dom_node: *const c_void, volume: f64) -> bool {
        let Some(v) = self.find_by_node(dom_node).map(VideoPtr::get) else {
            self.node_state(dom_node).volume = Some(volume);
            return true;
        };
        v.volume = volume;
        if v.token.is_some() && v.audio_opened.get() {
            audio_volume(&self.sink, v);
        }
        if volume > 0.0 && !v.muted.get() && v.playing.get() && !v.audio_opened.get() {
            audio_start(&mut self.sink, v);
            audio_seek_to_current(&self.sink, v);
        }
        true
    }

    fn set_node_muted(&mut self, dom_node: *const c_void, muted: bool) -> bool {
        let found = self.find_by_node(dom_node).map(VideoPtr::get);
        if dbg_audio() {
            debug(&format!(
                "[set-muted] muted={} video={} was={}\n",
                i32::from(muted),
                if found.is_some() { "yes" } else { "NULL" },
                found.as_ref().map_or(-1, |v| v.muted.raw())
            ));
        }
        let Some(v) = found else {
            self.node_state(dom_node).muted = Some(muted);
            return true;
        };
        if v.muted.get() == muted {
            return true;
        }
        v.muted.set(muted);
        if muted {
            audio_pause(&self.sink, v);
        } else if v.playing.get() {
            audio_start(&mut self.sink, v);
            audio_seek_to_current(&self.sink, v);
        }
        true
    }

    fn check_audio_attr(&mut self, p: VideoPtr, dom: Option<Node<'_>>) {
        let Some(asrc) = attr(dom, c"data-audio-src") else {
            return;
        };
        if asrc.is_empty() || !asrc.starts_with(b"blob:") {
            return;
        }
        let v = p.get();
        if str_of(&v.audio_url) == Some(asrc) {
            return;
        }
        v.audio_url = OwnedStr::new(asrc);
        ffi::fetch(self, asrc, p, FetchKind::Audio);
    }

    fn refresh_growing_stream(&mut self, p: VideoPtr, v: &mut NsVideo, now_us: i64) {
        if v.mse_id != 0 || v.player.is_none() {
            return;
        }
        let Some(url) = str_of(&v.url) else {
            return;
        };
        if now_us - v.last_refresh_us < 500_000 {
            return;
        }
        v.last_refresh_us = now_us;
        let base = stream_key(url);
        ffi::fetch(self, &base, p, FetchKind::Extend);
        self.check_audio_attr(p, v.dom());
    }

    fn note_stream_version(&mut self, p: VideoPtr, dom: Option<Node<'_>>, abs_url: &[u8]) {
        let v = p.get();
        if v.is_camera.get() || str_of(&v.url) == Some(abs_url) {
            return;
        }
        if self.requested.contains(abs_url) {
            return;
        }
        self.requested.insert(abs_url.to_vec());
        v.url = OwnedStr::new(abs_url);
        ffi::fetch(self, abs_url, p, FetchKind::Extend);
        self.check_audio_attr(p, dom);
    }

    fn discover_track(&mut self, p: VideoPtr, dom: Node<'_>) {
        if p.get().track_requested.get() {
            return;
        }
        let Some(track) = pick_track(dom) else {
            return;
        };
        let Some(tsrc) = attr(Some(track), c"src") else {
            return;
        };
        let Some(abs) = ffi::url_resolve(self.base_url.as_deref(), tsrc) else {
            return;
        };
        p.get().track_requested.set(true);
        ffi::fetch(self, &abs, p, FetchKind::Track);
    }

    fn adopt_existing(&mut self, p: VideoPtr, dom: Node<'_>, abs: &[u8]) {
        p.get().dom_node = node_ptr(dom);
        self.apply_node_state(p.get(), node_ptr(dom));
        self.discover_track(p, dom);
        self.note_stream_version(p, Some(dom), abs);
    }

    fn start(
        &mut self,
        dom: Option<Node<'_>>,
        bx: Option<BoxRef<'_>>,
        abs: &[u8],
        poster: Option<&[u8]>,
    ) {
        let mse_id = mse_url_id(abs);
        let dom_ptr = dom.map_or(core::ptr::null(), node_ptr);
        let mut previous = dom.and_then(|_| self.find_by_node(dom_ptr));
        if mse_id != 0 && previous.is_none() {
            previous = self.find_mse_rollover();
        }
        let key = stream_key(abs);
        if let Some(p) = lookup(&self.videos, &key) {
            if let Some(media) = bx.and_then(BoxRef::media) {
                media.set_video(p.as_raw());
            }
            if mse_id == 0 {
                self.note_stream_version(p, dom, abs);
            }
            return;
        }

        let mut v = NsVideo::zeroed();
        v.url = OwnedStr::new(abs);
        v.dom_node = dom_ptr;
        v.autoplay.set(attr(dom, c"autoplay").is_some());
        v.looping.set(attr(dom, c"loop").is_some());
        v.controls.set(attr(dom, c"controls").is_some());
        v.muted.set(attr(dom, c"muted").is_some());
        v.volume = 1.0;
        v.cur_time = 0.0;
        self.next_seq = self.next_seq.wrapping_add(1);
        v.seq = self.next_seq;
        v.mse_id = mse_id;
        let p = VideoPtr::adopt(v);
        if let Some(prev) = previous {
            if mse_id != 0 && prev.get().mse_id != mse_id {
                inherit_node_playback(p.get(), prev.get());
            }
            if dom.is_some() && prev.get().dom_node == dom_ptr {
                release_node(&self.sink, prev.get());
            }
        }
        self.videos.push((key, p));
        if let Some(media) = bx.and_then(BoxRef::media) {
            media.set_video(p.as_raw());
        }
        if let Some(dom) = dom {
            self.apply_node_state(p.get(), dom_ptr);
            self.discover_track(p, dom);
        }

        if mse_id != 0 {
            if let Some(s) = self.streams.get_mut(&mse_id) {
                mse_sync(&mut self.sink, s, p.get());
            }
            return;
        }

        ffi::fetch(self, abs, p, FetchKind::Video);

        if let Some(asrc) = attr(dom, c"data-audio-src").filter(|a| {
            [&b"blob:"[..], b"https://", b"http://", b"file://"]
                .iter()
                .any(|s| a.starts_with(s))
        }) {
            p.get().audio_url = OwnedStr::new(asrc);
            ffi::fetch(self, asrc, p, FetchKind::Audio);
        }

        if let Some(poster) = poster.filter(|s| !s.is_empty()) {
            ffi::fetch(self, poster, p, FetchKind::Poster);
        }
    }

    fn absolute(&self, src: &[u8]) -> Option<Vec<u8>> {
        if src.starts_with(b"blob:") {
            Some(src.to_vec())
        } else {
            ffi::url_resolve(self.base_url.as_deref(), src)
        }
    }

    fn discover_element(&mut self, node: Node<'_>) {
        if !node.is_element() || node.name().is_none_or(|n| n.to_bytes() != b"video") {
            return;
        }
        let Some(src) =
            non_empty_attr(node, c"src").or_else(|| non_empty_attr(node, MEDIA_SRC_ATTR))
        else {
            return;
        };
        let Some(abs) = self.absolute(src) else {
            return;
        };
        if !url_is_inline_video(&abs) || !fetchable_scheme(&abs) {
            return;
        }
        if let Some(p) = lookup(&self.videos, &stream_key(&abs)) {
            self.adopt_existing(p, node, &abs);
        } else if !self.requested.contains(&abs) {
            self.requested.insert(abs.clone());
            let poster = non_empty_attr(node, c"poster")
                .and_then(|raw| ffi::url_resolve(self.base_url.as_deref(), raw));
            self.start(Some(node), None, &abs, poster.as_deref());
        }
    }

    fn discover(&mut self, root: BoxRef<'_>, doc: Option<Node<'_>>) {
        for b in ffi::collect_videos(root) {
            let (Some(media), Some(dom)) = (b.media(), ffi::node(b.dom_ptr())) else {
                continue;
            };
            if attr(Some(dom), MEDIA_STREAM_ATTR) == Some(&b"camera"[..]) {
                let key = format!("camera:{:p}", b.dom_ptr()).into_bytes();
                let p = match lookup(&self.videos, &key) {
                    Some(p) => p,
                    None => {
                        let mut cv = NsVideo::zeroed();
                        cv.is_camera.set(true);
                        cv.dom_node = b.dom_ptr();
                        cv.playing.set(true);
                        let p = VideoPtr::adopt(cv);
                        self.videos.push((key, p));
                        p
                    }
                };
                media.set_video(p.as_raw());
                continue;
            }
            let Some(src) = non_empty_attr(dom, c"src")
                .or_else(|| non_empty_attr(dom, MEDIA_SRC_ATTR))
                .or_else(|| {
                    media
                        .video_src()
                        .map(CStr::to_bytes)
                        .filter(|s| !s.is_empty())
                })
            else {
                continue;
            };
            let Some(abs) = self.absolute(src) else {
                continue;
            };
            if !fetchable_scheme(&abs) || !url_is_inline_video(&abs) {
                continue;
            }
            let poster = media
                .video_poster()
                .and_then(|p| ffi::url_resolve(self.base_url.as_deref(), p.to_bytes()));
            if let Some(p) = lookup(&self.videos, &stream_key(&abs)) {
                media.set_video(p.as_raw());
                self.adopt_existing(p, dom, &abs);
            } else if !self.requested.contains(&abs) {
                self.requested.insert(abs.clone());
                self.start(Some(dom), Some(b), &abs, poster.as_deref());
            }
        }
        if let Some(doc) = doc {
            let mut node = Some(doc);
            while let Some(n) = node {
                self.discover_element(n);
                node = ffi::next_in_subtree(n, doc);
            }
        }
    }

    fn note_layout(&mut self, root: BoxRef<'_>, scroll_x: f64, scroll_y: f64, scale: f64) {
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let scroll_y = if self.page_coords { 0.0 } else { scroll_y };
        for b in ffi::collect_videos(root) {
            let Some(v) = ffi::video_of_box(b) else {
                continue;
            };
            v.note_paint_rect(
                (b.x() - scroll_x) * scale,
                (b.y() - scroll_y) * scale,
                b.content_width() * scale,
                b.content_height() * scale,
                ffi::box_fit(b),
            );
            if self.page_coords {
                v.note_paint_clip(v.rect_x, v.rect_y, v.rect_w, v.rect_h);
            }
        }
    }

    fn flush_composites(&mut self, now_us: i64) {
        let mut decoder_owner: Option<VideoPtr> = None;
        let mut layout_owner: Option<VideoPtr> = None;
        for (_, p) in &self.videos {
            let v = p.get();
            if v.mse_id != 0
                && v.video_opened.get()
                && decoder_owner.is_none_or(|d| v.seq > d.get().seq)
            {
                decoder_owner = Some(*p);
            }
            if v.mse_id != 0
                && v.playing.get()
                && v.last_paint_us > 0
                && layout_owner.is_none_or(|l| {
                    let l = l.get();
                    v.last_paint_us > l.last_paint_us
                        || (v.last_paint_us == l.last_paint_us && v.seq > l.seq)
                })
            {
                layout_owner = Some(*p);
            }
        }
        if let (Some(d), Some(l)) = (decoder_owner, layout_owner)
            && d != l
            && now_us - l.get().last_paint_us < 2_000_000
        {
            let l = l.get();
            let (rect, fit, clip) = (
                [l.rect_x, l.rect_y, l.rect_w, l.rect_h],
                l.rect_fit,
                [l.clip_x, l.clip_y, l.clip_w, l.clip_h],
            );
            let d = d.get();
            d.note_paint_rect(rect[0], rect[1], rect[2], rect[3], fit);
            d.note_paint_clip(clip[0], clip[1], clip[2], clip[3]);
        }
        for (_, p) in &self.videos {
            helper_flush_rect(self.page_coords, &self.sink, p.get(), now_us);
        }
    }

    fn mse_append(&mut self, stream_id: u32, kind: u8, data: &[u8]) -> bool {
        let s = self.streams.entry(stream_id).or_default();
        s.eos = false;
        let sv = lookup(&self.videos, &mse_key(stream_id));
        let new_generation = match s.track(kind).append(data, MAX_BYTES) {
            Appended::Duplicate => return true,
            Appended::TooLarge => return false,
            Appended::Stored { new_generation } => new_generation,
        };
        if dbg_audio() {
            debug(&format!(
                "[mse-append] id={stream_id} kind={} len={} total={} vend={:.1} aend={:.1}{}\n",
                char::from(kind),
                data.len(),
                s.track_ref(kind).bytes.len(),
                s.video.end,
                s.audio.end,
                if new_generation { " NEW-GEN" } else { "" }
            ));
        }
        if let Some(v) = sv {
            mse_sync(&mut self.sink, s, v.get());
        }
        true
    }

    fn mse_buffered(&self, stream_id: u32, kind: u8, first: &mut f64) -> f64 {
        self.streams
            .get(&stream_id)
            .map_or(0.0, |s| s.track_ref(kind).buffered(first))
    }

    fn mse_bytes(&self, stream_id: u32, kind: u8) -> usize {
        self.streams
            .get(&stream_id)
            .map_or(0, |s| s.track_ref(kind).bytes.len())
    }

    fn mse_remove(&mut self, stream_id: u32, kind: u8, start: f64, end: f64) -> bool {
        let Some(s) = self.streams.get_mut(&stream_id) else {
            return false;
        };
        if !s.track(kind).remove(start, end) {
            return true;
        }
        let v = lookup(&self.videos, &mse_key(stream_id)).map(VideoPtr::get);
        if kind == b'a' {
            s.audio_rebased = true;
            if let Some(v) = v {
                audio_stop(&self.sink, v);
            }
            return true;
        }
        let Some(v) = v else {
            return true;
        };
        let now = ffi::monotonic_us();
        if v.playing.get() && v.base_us > 0 {
            v.cur_time = (now - v.base_us) as f64 / 1e6;
        }
        helper_stop(&self.sink, v);
        v.player = None;
        v.frame_texture = None;
        v.prev_tick_time = v.cur_time;
        v.last_emit_time = v.cur_time;
        v.duration = 0.0;
        v.base_us = if v.playing.get() {
            now - us(v.cur_time)
        } else {
            0
        };
        v.loaded.set(false);
        v.ended.set(false);
        v.stalled.set(false);
        v.meta_sent.set(false);
        v.buf_sent.set(false);
        true
    }

    fn mse_eos(&mut self, stream_id: u32) {
        let Some(s) = self.streams.get_mut(&stream_id) else {
            return;
        };
        s.eos = true;
        if let Some(v) = lookup(&self.videos, &mse_key(stream_id)) {
            mse_sync(&mut self.sink, s, v.get());
        }
    }

    fn tick(&mut self, now_us: i64) -> bool {
        let mut changed = false;
        let mut emits: Vec<(VideoPtr, &'static CStr, f64)> = Vec::new();
        let mut opened_count: u32 = 0;
        let mut newest_audible: Option<VideoPtr> = None;
        for (_, p) in &self.videos {
            let c = p.get();
            if c.video_opened.get() || c.audio_opened.get() {
                opened_count = opened_count.wrapping_add(1);
            }
            if c.mse_id != 0
                && c.playing.get()
                && !c.muted.get()
                && c.audio_opened.get()
                && newest_audible.is_none_or(|n| c.seq > n.get().seq)
            {
                newest_audible = Some(*p);
            }
        }
        let videos: Vec<VideoPtr> = self.videos.iter().map(|(_, p)| *p).collect();
        for p in videos {
            let v = p.get();
            let superseded = opened_count >= 2
                && newest_audible.is_some_and(|n| n != p && v.mse_id != 0 && v.seq < n.get().seq);
            let stale = v.last_paint_us > 0 && now_us - v.last_paint_us > 3_000_000;
            if opened_count >= 2
                && (v.video_opened.get() || v.audio_opened.get())
                && (superseded || stale)
            {
                audio_stop(&self.sink, v);
                helper_stop(&self.sink, v);
                v.playing.set(false);
                opened_count = opened_count.wrapping_sub(1);
                continue;
            }
            if v.is_camera.get() {
                if let Some(Some(frame)) = ffi::camera_next_frame() {
                    v.natural_width = frame.width();
                    v.natural_height = frame.height();
                    v.frame_texture = Some(frame);
                    changed = true;
                }
                continue;
            }
            if v.mse_id != 0
                && let Some(s) = self.streams.get_mut(&v.mse_id)
            {
                mse_sync(&mut self.sink, s, v);
            }
            media::activate_pending(&self.sink, v);
            if v.player.is_none() {
                continue;
            }

            if !v.meta_sent.get() && v.duration > 0.0 {
                v.meta_sent.set(true);
                emits.push((p, c"meta", v.duration));
                if v.natural_width > 0 {
                    emits.push((p, c"vwidth", f64::from(v.natural_width)));
                }
                if v.natural_height > 0 {
                    emits.push((p, c"vheight", f64::from(v.natural_height)));
                }
            }
            if !v.buf_sent.get() {
                v.buf_sent.set(true);
                let mut end = buffered_end(&self.streams, v);
                if end <= 0.0 {
                    end = v.duration;
                }
                if end > 0.0 && end != v.sent_buffered_end {
                    v.sent_buffered_end = end;
                    emits.push((p, c"buf", end));
                }
            }
            let helper_owns = helper_composited(v);
            let helper = helper_enabled() && v.video_opened.get();
            if helper {
                helper_flush_rect(self.page_coords, &self.sink, v, now_us);
            }
            if !v.playing.get() {
                continue;
            }

            if !v.audio_opened.get()
                && !v.muted.get()
                && (v.has_audio.get() || v.audio_file.is_some())
            {
                audio_start(&mut self.sink, v);
                audio_seek_to_current(&self.sink, v);
            }

            if v.base_us == 0 {
                v.base_us = now_us - us(v.cur_time);
            }
            let mut elapsed = (now_us - v.base_us) as f64 / 1e6;
            if ffi::env_set(c"NS_DBG_CLOCK") && v.mse_id != 0 {
                debug(&format!(
                    "[clock] elapsed={elapsed:.2} prev={:.2} cur={:.2} edge={:.2}\n",
                    v.prev_tick_time,
                    v.cur_time,
                    buffered_end(&self.streams, v)
                ));
            }
            if !helper_owns && elapsed - v.prev_tick_time > 1.0 {
                elapsed = v.prev_tick_time + 1.0;
                v.base_us = now_us - us(elapsed);
                if v.audio_opened.get() && !helper {
                    audio_seek(&self.sink, v, &f3(elapsed));
                }
            }
            let mut t = elapsed;
            if v.looping.get() && v.duration > 0.0 {
                t = elapsed % v.duration;
            }

            if stream_growing(&self.streams, v) {
                let edge = non_negative(buffered_end(&self.streams, v) - 0.35);
                if t > edge {
                    self.refresh_growing_stream(p, v, now_us);
                    v.base_us = now_us - us(edge);
                    t = edge;
                    if v.stall_since_us == 0 {
                        v.stall_since_us = now_us;
                    }
                    if !v.stalled.get() && now_us - v.stall_since_us > 1_500_000 {
                        v.stalled.set(true);
                        emits.push((p, c"waiting", edge));
                    }
                } else {
                    v.stall_since_us = 0;
                }
            }

            if v.looping.get() && t + 1e-3 < v.prev_tick_time {
                audio_resync(&self.sink, v);
            }
            v.prev_tick_time = t;

            let looping = v.looping.get();
            let (frame, mut ended) = match v.player.as_mut() {
                Some(player) if !helper_owns => player.frame_at(t, looping),
                _ => (None, false),
            };
            if helper_owns && !looping && v.duration > 0.0 && t >= v.duration {
                ended = true;
            }
            if helper && v.stalled.get() && t + 0.5 < buffered_end(&self.streams, v) {
                v.stalled.set(false);
                emits.push((p, c"resumed", t));
            }
            if let Some(frame) = frame {
                v.frame_texture = Some(frame);
                changed = true;
                if v.stalled.get() {
                    v.stalled.set(false);
                    emits.push((p, c"resumed", t));
                }
            }
            v.cur_time = t;
            if t - v.last_emit_time >= 0.20 || ended {
                v.last_emit_time = t;
                emits.push((p, c"pos", v.cur_time));
            }
            if ended {
                if stream_growing(&self.streams, v) {
                    v.prev_tick_time = v.cur_time;
                    v.base_us = now_us - us(v.cur_time);
                    if !v.stalled.get() {
                        v.stalled.set(true);
                        emits.push((p, c"waiting", v.cur_time));
                    }
                    self.refresh_growing_stream(p, v, now_us);
                    continue;
                }
                v.playing.set(false);
                v.ended.set(true);
                v.cur_time = v.duration;
                audio_stop(&self.sink, v);
                helper_playpause(&self.sink, v, false);
                emits.push((p, c"ended", v.duration));
            }
        }
        for (p, kind, value) in emits {
            self.sink.event(p.get(), kind, value);
        }
        changed
    }

    fn animating(&self) -> bool {
        if !self.pending.is_empty() {
            return true;
        }
        self.videos.iter().any(|(_, p)| {
            let v = p.get();
            (v.is_camera.get() && ffi::camera_active()) || (v.player.is_some() && v.playing.get())
        })
    }

    fn waiting_growth(&self) -> bool {
        self.videos.iter().any(|(_, p)| {
            let v = p.get();
            v.playing.get() && v.stalled.get()
        })
    }
}
