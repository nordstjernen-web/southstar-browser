//! Southstar — the per-video state, the page's video caches and the generation-stamped slots boxes, paint, canvas and script reach videos through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, Ref, RefCell, RefMut};
use core::ffi::{CStr, c_int, c_uint};
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::{Rc, Weak};

use southstar_dom::{Node, WeakNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_layout::{BoxRef, MediaHandle};
use southstar_webvtt::Cue;

use crate::ffi::{self, Player, Response, Texture, decode_image, until_nul};
use crate::{Cache, FetchKind, Sink};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Flag(GBoolean);

impl Flag {
    pub fn get(self) -> bool {
        self.0 != 0
    }

    pub fn set(&mut self, on: bool) {
        self.0 = glib::boolean(on);
    }

    pub fn raw(self) -> GBoolean {
        self.0
    }
}

pub struct OwnedStr(CString);

impl OwnedStr {
    pub fn new(bytes: &[u8]) -> Option<OwnedStr> {
        CString::new(until_nul(bytes)).ok().map(OwnedStr)
    }

    pub fn c_str(&self) -> &CStr {
        &self.0
    }

    pub fn bytes(&self) -> &[u8] {
        self.0.to_bytes()
    }
}

pub fn str_of(s: &Option<OwnedStr>) -> Option<&[u8]> {
    s.as_ref().map(OwnedStr::bytes)
}

pub struct Cues(pub Vec<Cue>);

#[derive(Default)]
pub struct NsVideo {
    pub url: Option<OwnedStr>,
    pub natural_width: c_int,
    pub natural_height: c_int,
    pub poster_texture: Option<Texture>,
    pub poster_url: Option<OwnedStr>,
    pub frame_texture: Option<Texture>,
    pub loaded: Flag,
    pub load_events_fired: Flag,
    pub failed: Flag,
    pub player: Option<Player>,
    pub dom: Option<WeakNode>,
    pub is_camera: Flag,
    pub autoplay: Flag,
    pub looping: Flag,
    pub controls: Flag,
    pub muted: Flag,
    pub has_audio: Flag,
    pub audio_opened: Flag,
    pub audio_url: Option<OwnedStr>,
    pub audio_file: Option<OwnedStr>,
    pub audio_file_len: usize,
    pub audio_file_gen: c_uint,
    pub audio_timeline_start: f64,
    pub pending_audio_file: Option<OwnedStr>,
    pub pending_audio_file_len: usize,
    pub pending_audio_file_gen: c_uint,
    pub pending_audio_start: f64,
    pub video_file: Option<OwnedStr>,
    pub video_file_len: usize,
    pub video_file_gen: c_uint,
    pub pending_video_file: Option<OwnedStr>,
    pub pending_video_file_len: usize,
    pub pending_video_file_gen: c_uint,
    pub pending_video_start: f64,
    pub video_opened: Flag,
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    pub clip_x: f64,
    pub clip_y: f64,
    pub clip_w: f64,
    pub clip_h: f64,
    pub rect_fit: c_int,
    pub last_paint_us: i64,
    pub rect_dirty: Flag,
    pub rect_sent_us: i64,
    pub sent_rect_x: f64,
    pub sent_rect_y: f64,
    pub sent_rect_w: f64,
    pub sent_rect_h: f64,
    pub sent_clip_x: f64,
    pub sent_clip_y: f64,
    pub sent_clip_w: f64,
    pub sent_clip_h: f64,
    pub sent_rect_fit: c_int,
    pub sent_rect_page: Flag,
    pub token: Option<OwnedStr>,
    pub playing: Flag,
    pub ended: Flag,
    pub volume: f64,
    pub meta_sent: Flag,
    pub buf_sent: Flag,
    pub sent_buffered_end: f64,
    pub seq: c_uint,
    pub base_us: i64,
    pub last_refresh_us: i64,
    pub stalled: Flag,
    pub stall_since_us: i64,
    pub mse_id: c_uint,
    pub cur_time: f64,
    pub prev_tick_time: f64,
    pub last_emit_time: f64,
    pub duration: f64,
    pub cues: Option<Box<Cues>>,
    pub track_requested: Flag,
    handle: Option<MediaHandle>,
}

impl NsVideo {
    pub fn dom(&self) -> Option<Node<'static>> {
        self.dom.and_then(WeakNode::get)
    }
}

struct VideoSlot {
    stamp: Cell<u64>,
    video: RefCell<Option<Video>>,
}

thread_local! {
    static VACANT_SLOTS: RefCell<Vec<&'static VideoSlot>> = const { RefCell::new(Vec::new()) };
    static NEXT_STAMP: Cell<u64> = const { Cell::new(1) };
}

fn open_slot(video: &Video) -> MediaHandle {
    let slot = VACANT_SLOTS
        .try_with(|v| v.borrow_mut().pop())
        .ok()
        .flatten()
        .unwrap_or_else(|| {
            Box::leak(Box::new(VideoSlot {
                stamp: Cell::new(0),
                video: RefCell::new(None),
            }))
        });
    let stamp = NEXT_STAMP.with(|next| next.replace(next.get() + 1));
    slot.stamp.set(stamp);
    drop(slot.video.replace(Some(video.clone())));
    MediaHandle::new(slot, stamp)
}

fn slot_of(handle: MediaHandle) -> Option<&'static VideoSlot> {
    handle
        .target()
        .downcast_ref::<VideoSlot>()
        .filter(|slot| slot.stamp.get() == handle.stamp())
}

fn video_at(handle: MediaHandle) -> Option<Video> {
    slot_of(handle)?.video.try_borrow().ok()?.clone()
}

fn close_slot(handle: MediaHandle) {
    if let Some(slot) = slot_of(handle) {
        slot.stamp.set(0);
        let released = slot.video.replace(None);
        let _ = VACANT_SLOTS.try_with(|v| v.borrow_mut().push(slot));
        drop(released);
    }
}

#[derive(Clone)]
pub struct Video(Rc<RefCell<NsVideo>>);

impl PartialEq for Video {
    fn eq(&self, other: &Video) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Video {
    pub fn adopt(v: NsVideo) -> Video {
        let video = Video(Rc::new(RefCell::new(v)));
        let handle = open_slot(&video);
        video.get().handle = Some(handle);
        video
    }

    pub fn of_box(b: BoxRef<'_>) -> Option<Video> {
        video_at(b.media()?.video()?)
    }

    pub fn media_handle(&self) -> Option<MediaHandle> {
        self.read().handle
    }

    pub fn get(&self) -> RefMut<'_, NsVideo> {
        self.0.borrow_mut()
    }

    pub fn read(&self) -> Ref<'_, NsVideo> {
        self.0.borrow()
    }

    pub fn try_get(&self) -> Option<RefMut<'_, NsVideo>> {
        self.0.try_borrow_mut().ok()
    }

    fn try_read(&self) -> Option<Ref<'_, NsVideo>> {
        self.0.try_borrow().ok()
    }

    fn downgrade(&self) -> Weak<RefCell<NsVideo>> {
        Rc::downgrade(&self.0)
    }

    pub fn close(&self) {
        let handle = self.try_get().and_then(|mut v| v.handle.take());
        if let Some(handle) = handle {
            close_slot(handle);
        }
    }
}

pub type AudioSink = Rc<dyn Fn(&CStr)>;
pub type JsSink = Rc<dyn Fn(Node<'static>, &CStr, f64)>;

#[derive(Clone)]
pub struct AudioOut(AudioSink);

impl AudioOut {
    pub fn send(&self, command: &[u8]) {
        if let Ok(command) = CString::new(command) {
            (self.0)(&command);
        }
    }
}

#[derive(Clone)]
pub struct JsOut(JsSink);

impl JsOut {
    fn deliver(&self, node: Node<'static>, kind: &CStr, value: f64) {
        (self.0)(node, kind, value);
    }
}

pub struct Fetch {
    url: Vec<u8>,
    top_url: Option<Vec<u8>>,
    video: Weak<RefCell<NsVideo>>,
    kind: FetchKind,
}

pub fn fetch(cache: &mut Cache, url: &[u8], video: &Video, kind: FetchKind) {
    cache.pending += 1;
    cache.outbox.push(Fetch {
        url: until_nul(url).to_vec(),
        top_url: cache.base_url.as_deref().map(|u| until_nul(u).to_vec()),
        video: video.downgrade(),
        kind,
    });
}

fn on_fetched(cache: VideoCache, video: &Weak<RefCell<NsVideo>>, kind: FetchKind, resp: &Response) {
    cache.with(|c| {
        c.pending = c.pending.saturating_sub(1);
        if let Some(video) = video.upgrade().map(Video)
            && let Some(mut v) = video.try_get()
        {
            crate::fetched(c, &mut v, kind, resp);
        }
    });
}

thread_local! {
    static CACHES: RefCell<HashMap<u64, Rc<RefCell<Cache>>>> = RefCell::new(HashMap::new());
    static NEXT_CACHE: Cell<u64> = const { Cell::new(1) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VideoCache(u64);

impl VideoCache {
    pub fn create() -> VideoCache {
        let id = NEXT_CACHE.with(|next| next.replace(next.get() + 1));
        let cache = Rc::new(RefCell::new(Cache::new()));
        CACHES.with(|caches| caches.borrow_mut().insert(id, cache));
        VideoCache(id)
    }

    pub fn free(self) {
        let released = CACHES
            .try_with(|caches| caches.borrow_mut().remove(&self.0))
            .ok()
            .flatten();
        drop(released);
    }

    fn cell(self) -> Option<Rc<RefCell<Cache>>> {
        CACHES
            .try_with(|caches| caches.borrow().get(&self.0).cloned())
            .ok()
            .flatten()
    }

    fn with<R>(self, f: impl FnOnce(&mut Cache) -> R) -> Option<R> {
        let cell = self.cell()?;
        let (result, outbox) = {
            let mut cache = cell.try_borrow_mut().ok()?;
            let result = f(&mut cache);
            (result, core::mem::take(&mut cache.outbox))
        };
        for Fetch {
            url,
            top_url,
            video,
            kind,
        } in outbox
        {
            ffi::request(&url, top_url.as_deref(), move |resp| {
                on_fetched(self, &video, kind, &resp);
                drop(resp);
                self.flush_js();
            });
        }
        Some(result)
    }

    fn flush_js(self) {
        let Some((out, events)) =
            self.with(|c| (c.sink.js.clone(), core::mem::take(&mut c.sink.queue)))
        else {
            return;
        };
        if let Some(out) = out {
            for e in events {
                if let Some(node) = e.node.get() {
                    out.deliver(node, e.kind, e.value);
                }
            }
        }
    }

    pub fn set_base(self, base_url: Option<&[u8]>) {
        self.with(|c| c.base_url = base_url.map(<[u8]>::to_vec));
    }

    pub fn set_js_sink(self, sink: Option<JsSink>) {
        self.with(|c| c.sink.js = sink.map(JsOut));
    }

    pub fn set_audio_sink(self, sink: Option<AudioSink>) {
        self.with(|c| c.sink.audio = sink.map(AudioOut));
    }

    pub fn discover(self, root: BoxRef<'_>, doc: Option<Node<'_>>) {
        self.with(|c| c.discover(root, doc));
    }

    pub fn set_page_coords(self, on: bool) {
        self.with(|c| c.page_coords = on);
    }

    pub fn note_layout(self, root: BoxRef<'_>, scroll_x: f64, scroll_y: f64, scale: f64) {
        self.with(|c| c.note_layout(root, scroll_x, scroll_y, scale));
    }

    pub fn tick(self, now_us: i64) -> bool {
        let changed = self.with(|c| c.tick(now_us)).unwrap_or(false);
        self.flush_js();
        changed
    }

    pub fn flush_composites(self, now_us: i64) {
        self.with(|c| c.flush_composites(now_us));
    }

    pub fn animating(self) -> bool {
        self.with(|c| c.animating()).unwrap_or(false)
    }

    pub fn waiting_growth(self) -> bool {
        self.with(|c| c.waiting_growth()).unwrap_or(false)
    }

    pub fn has_pending(self) -> bool {
        self.with(|c| c.pending != 0).unwrap_or(false)
    }

    pub fn mse_append(self, stream_id: u32, kind: u8, data: &[u8]) -> bool {
        stream_id != 0
            && !data.is_empty()
            && self
                .with(|c| c.mse_append(stream_id, kind, data))
                .unwrap_or(false)
    }

    pub fn mse_eos(self, stream_id: u32) {
        if stream_id != 0 {
            self.with(|c| c.mse_eos(stream_id));
        }
    }

    pub fn mse_buffered(self, stream_id: u32, kind: u8) -> (f64, f64) {
        let mut first = 0.0;
        let end = if stream_id == 0 {
            0.0
        } else {
            self.with(|c| c.mse_buffered(stream_id, kind, &mut first))
                .unwrap_or(0.0)
        };
        (first, end)
    }

    pub fn mse_remove(self, stream_id: u32, kind: u8, start: f64, end: f64) -> bool {
        stream_id != 0
            && start >= 0.0
            && end > start
            && self
                .with(|c| c.mse_remove(stream_id, kind, start, end))
                .unwrap_or(false)
    }

    pub fn mse_bytes(self, stream_id: u32, kind: u8) -> usize {
        if stream_id == 0 {
            return 0;
        }
        self.with(|c| c.mse_bytes(stream_id, kind)).unwrap_or(0)
    }

    pub fn helper_event(self, token: &[u8], kind: &[u8]) -> bool {
        if token.is_empty() || kind.is_empty() {
            return false;
        }
        let handled = self.with(|c| c.helper_event(token, kind)).unwrap_or(false);
        self.flush_js();
        handled
    }

    pub fn seek_node(self, dom_node: Node<'_>, seconds: f64, now_us: i64) -> bool {
        let handled = self
            .with(|c| c.seek_node(dom_node, seconds, now_us))
            .unwrap_or(false);
        self.flush_js();
        handled
    }

    pub fn set_node_playing(self, dom_node: Node<'_>, play: bool, now_us: i64) -> bool {
        self.with(|c| c.set_node_playing(dom_node, play, now_us))
            .unwrap_or(false)
    }

    pub fn set_node_muted(self, dom_node: Node<'_>, muted: bool) -> bool {
        self.with(|c| c.set_node_muted(dom_node, muted))
            .unwrap_or(false)
    }

    pub fn set_node_volume(self, dom_node: Node<'_>, volume: f64) -> bool {
        self.with(|c| c.set_node_volume(dom_node, volume))
            .unwrap_or(false)
    }
}

#[derive(Clone, Copy)]
pub struct BoxVideo(MediaHandle);

impl BoxVideo {
    pub fn of(b: BoxRef<'_>) -> Option<BoxVideo> {
        let handle = b.media()?.video()?;
        video_at(handle).map(|_| BoxVideo(handle))
    }

    fn read<R: Default>(self, f: impl FnOnce(&NsVideo) -> R) -> R {
        let Some(video) = video_at(self.0) else {
            return R::default();
        };
        let Some(v) = video.try_read() else {
            return R::default();
        };
        f(&v)
    }

    fn write<R: Default>(self, f: impl FnOnce(&mut NsVideo) -> R) -> R {
        let Some(video) = video_at(self.0) else {
            return R::default();
        };
        let Some(mut v) = video.try_get() else {
            return R::default();
        };
        f(&mut v)
    }

    pub fn playing(self) -> bool {
        self.read(|v| v.playing.get())
    }

    pub fn video_opened(self) -> bool {
        self.read(|v| v.video_opened.get())
    }

    pub fn frame_texture(self) -> Option<Texture> {
        self.read(|v| v.frame_texture.clone())
    }

    pub fn poster_texture(self) -> Option<Texture> {
        self.read(|v| v.poster_texture.clone())
    }

    pub fn is_camera(self) -> bool {
        self.read(|v| v.is_camera.get())
    }

    pub fn note_paint_rect(self, x: f64, y: f64, w: f64, h: f64, fit: i32) {
        self.write(|v| v.note_paint_rect(x, y, w, h, fit));
    }

    pub fn note_paint_clip(self, x: f64, y: f64, w: f64, h: f64) {
        self.write(|v| v.note_paint_clip(x, y, w, h));
    }

    pub fn helper_composited(self) -> bool {
        self.read(crate::helper_composited)
    }

    pub fn with_poster<R>(self, f: impl FnOnce(Option<&Texture>, Option<&CStr>) -> R) -> R {
        let video = video_at(self.0);
        match video.as_ref().and_then(Video::try_read) {
            Some(v) => f(
                v.poster_texture.as_ref(),
                v.poster_url.as_ref().map(OwnedStr::c_str),
            ),
            None => f(None, None),
        }
    }

    pub fn take_load_events(self) -> bool {
        self.write(|v| {
            let take = v.loaded.get() && !v.load_events_fired.get();
            if take {
                v.load_events_fired.set(true);
            }
            take
        })
    }

    pub fn active_cue_text(self) -> Option<Vec<u8>> {
        self.read(|v| {
            let cues = v.cues.as_ref()?;
            let cue = southstar_webvtt::active(&cues.0, v.cur_time)?;
            Some(cue.text.to_bytes().to_vec())
        })
    }

    pub fn has_player(self) -> bool {
        self.read(|v| v.player.is_some())
    }

    pub fn toggle(self, cache: Option<VideoCache>, now_us: i64) -> bool {
        let Some(video) = video_at(self.0) else {
            return false;
        };
        let toggle = |sink: &mut Sink| {
            video
                .try_get()
                .is_some_and(|mut v| crate::toggle(sink, &mut v, now_us))
        };
        match cache {
            Some(cache) => {
                let toggled = cache.with(|c| toggle(&mut c.sink)).unwrap_or(false);
                cache.flush_js();
                toggled
            }
            None => toggle(&mut Sink::default()),
        }
    }
}

pub struct StillVideo(Box<NsVideo>);

impl StillVideo {
    fn new(url: &[u8]) -> StillVideo {
        let mut v = Box::<NsVideo>::default();
        v.url = OwnedStr::new(url);
        StillVideo(v)
    }

    pub fn from_player(url: &[u8], body: &[u8]) -> Option<StillVideo> {
        let mut player = Player::new(body)?;
        let (frame, _) = player.frame_at(0.0, false);
        let mut still = StillVideo::new(url);
        let v = &mut still.0;
        v.natural_width = player.width();
        v.natural_height = player.height();
        v.duration = player.duration();
        v.frame_texture = frame;
        v.player = Some(player);
        Some(still)
    }

    pub fn from_poster(url: &[u8], body: &[u8]) -> Option<StillVideo> {
        let (texture, w, h) = decode_image(body)?;
        let mut still = StillVideo::new(url);
        let v = &mut still.0;
        v.poster_texture = Some(texture);
        v.natural_width = w;
        v.natural_height = h;
        Some(still)
    }

    pub fn attach(self, b: BoxRef<'_>) -> Result<(), StillVideo> {
        match b.media().filter(|m| m.video().is_none()) {
            Some(m) => {
                m.set_video(Video::adopt(*self.0).media_handle());
                Ok(())
            }
            None => Err(self),
        }
    }
}
