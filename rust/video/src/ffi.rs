//! Southstar — the ns_video struct, the video cache, box video and still video handles, and the C ABI paint, canvas and script still read videos through, over the network, textures, the camera, the DOM and the box tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;
use southstar_glib::{self as glib, GBoolean, GError};
use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId, StyleRef};

use crate::{Cache, FetchKind, Sink};
use southstar_webvtt::Cue;

#[repr(transparent)]
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

#[repr(transparent)]
pub struct OwnedStr(NonNull<c_char>);

impl OwnedStr {
    pub fn new(bytes: &[u8]) -> Option<OwnedStr> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let owned = CString::new(&bytes[..end]).ok()?;
        NonNull::new(owned.into_raw()).map(OwnedStr)
    }

    pub fn c_str(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.0.as_ptr()) }
    }

    pub fn bytes(&self) -> &[u8] {
        self.c_str().to_bytes()
    }
}

impl Drop for OwnedStr {
    fn drop(&mut self) {
        drop(unsafe { CString::from_raw(self.0.as_ptr()) });
    }
}

pub fn str_of(s: &Option<OwnedStr>) -> Option<&[u8]> {
    s.as_ref().map(OwnedStr::bytes)
}

pub use southstar_texture::TextureRef as Texture;

pub use crate::decode::{Player, probe_chunk_range};

pub struct Cues(pub Vec<Cue>);

#[repr(C)]
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
    pub dom_node: *const c_void,
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
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<NsVideo>() == 544
        && core::mem::offset_of!(NsVideo, duration) == 520
        && core::mem::offset_of!(NsVideo, player) == 56
        && core::mem::offset_of!(NsVideo, is_camera) == 72
        && core::mem::offset_of!(NsVideo, video_opened) == 232
        && core::mem::offset_of!(NsVideo, playing) == 416
);

impl NsVideo {
    pub fn zeroed() -> Box<NsVideo> {
        Box::new(unsafe { core::mem::zeroed() })
    }

    pub fn dom(&self) -> Option<Node<'static>> {
        node(self.dom_node)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VideoPtr(NonNull<NsVideo>);

impl VideoPtr {
    pub fn adopt(v: Box<NsVideo>) -> VideoPtr {
        VideoPtr(NonNull::from(Box::leak(v)))
    }

    pub fn as_raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    pub fn get<'a>(self) -> &'a mut NsVideo {
        unsafe { &mut *self.0.as_ptr() }
    }

    pub fn free(self) {
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

pub type AudioSink = Rc<dyn Fn(&CStr)>;
pub type JsSink = Rc<dyn Fn(*const c_void, &CStr, f64)>;

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
    fn deliver(&self, node: *const c_void, kind: &CStr, value: f64) {
        (self.0)(node, kind, value);
    }
}

pub struct Pending {
    cache: *mut Cache,
    video: VideoPtr,
    kind: FetchKind,
    dead: bool,
}

pub type PendingPtr = NonNull<Pending>;

pub fn mark_dead(pending: &[PendingPtr]) {
    for p in pending {
        unsafe { (*p.as_ptr()).dead = true };
    }
}

pub struct Response(Option<southstar_net::Response>);

impl Response {
    pub fn body(&self) -> Option<&[u8]> {
        let resp = self.0.as_ref().filter(|resp| resp.error().is_none())?;
        resp.body().filter(|body| !body.is_empty())
    }

    pub fn final_url(&self) -> Option<&[u8]> {
        self.0.as_ref()?.final_url().map(CStr::to_bytes)
    }
}

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn env_set(name: &CStr) -> bool {
    !unsafe { glib::g_getenv(name.as_ptr()) }.is_null()
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn cstring(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

fn take_gstr(p: *mut c_char) -> Option<Vec<u8>> {
    let s = unsafe { glib::GStr::take(p) }?;
    Some(s.to_bytes().to_vec())
}

pub fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(base.map(until_nul), until_nul(href))
}

pub fn texture_from_bgra(width: i32, height: i32, pixels: &[u8], stride: usize) -> Option<Texture> {
    Texture::new(width, height, pixels, stride)
}

pub fn decode_image(bytes: &[u8]) -> Option<(Texture, i32, i32)> {
    southstar_image::decode_bytes(bytes)
}

pub fn write_stream_file(subdir: &CStr, prefix: u8, token: u32, data: &[u8]) -> Option<Vec<u8>> {
    let cache_dir = unsafe { CStr::from_ptr(glib::g_get_user_cache_dir()) };
    let mut parts = [
        cache_dir.as_ptr().cast_mut(),
        c"southstar".as_ptr().cast_mut(),
        subdir.as_ptr().cast_mut(),
        ptr::null_mut(),
    ];
    let dir = take_gstr(unsafe { glib::g_build_filenamev(parts.as_mut_ptr()) })?;
    let dir_c = cstring(&dir);
    unsafe { g_mkdir_with_parents(dir_c.as_ptr(), 0o700) };
    let mut path = dir;
    path.extend_from_slice(
        format!(
            "/{}{}-{}.dat",
            char::from(prefix),
            std::process::id() as i32,
            token
        )
        .as_bytes(),
    );
    let path_c = cstring(&path);
    let ok = unsafe {
        g_file_set_contents(
            path_c.as_ptr(),
            data.as_ptr().cast(),
            data.len() as isize,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let mut url = b"file://".to_vec();
    url.extend_from_slice(&path);
    Some(url)
}

pub fn node<'a>(p: *const c_void) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p.cast()) }
}

pub fn next_in_subtree<'a>(n: Node<'a>, root: Node<'a>) -> Option<Node<'a>> {
    southstar_dom::index::next_in_subtree(n, Some(root), true)
}

pub fn collect_videos(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    southstar_layout_query::collect_videos(root)
}

pub fn video_of_box<'a>(b: BoxRef<'_>) -> Option<&'a mut NsVideo> {
    b.media()
        .and_then(|m| unsafe { m.video().cast::<NsVideo>().as_mut() })
}

pub fn box_fit(b: BoxRef<'_>) -> c_int {
    let style = unsafe { StyleRef::from_ptr(b.style()) };
    let fit = match style.and_then(|s| s.get(PropId::ObjectFit)) {
        Some(v) if v.kind() == Kind::Keyword => v.keyword_text().map(CStr::to_bytes),
        _ => Some(&b"contain"[..]),
    };
    match fit {
        Some(b"fill") => 0,
        Some(b"cover") => 2,
        Some(b"none") => 3,
        Some(b"scale-down") => 4,
        _ => 1,
    }
}

pub fn fetch(cache: &mut Cache, url: &[u8], video: VideoPtr, kind: FetchKind) {
    let pending = NonNull::from(Box::leak(Box::new(Pending {
        cache: ptr::from_mut(cache),
        video,
        kind,
        dead: false,
    })));
    cache.pending.push(pending);
    let request = southstar_net::FetchRequest {
        url: until_nul(url),
        top_url: cache.base_url.as_deref().map(until_nul),
        method: Some(b"GET"),
        ..southstar_net::FetchRequest::default()
    };
    southstar_net::request_with(&request, None, move |outcome| {
        // SAFETY: the pending fetch was leaked for this request and is reclaimed once, here.
        unsafe { on_fetched(pending.as_ptr(), Response(outcome.ok())) }
    });
}

unsafe fn on_fetched(raw: *mut Pending, resp: Response) {
    let pending = unsafe { Box::from_raw(raw) };
    if pending.dead {
        return;
    }
    let cache_ptr = pending.cache;
    {
        let cache = unsafe { &mut *cache_ptr };
        crate::fetched(cache, pending.video.get(), pending.kind, &resp);
        if let Some(i) = cache.pending.iter().position(|p| p.as_ptr() == raw) {
            cache.pending.swap_remove(i);
        }
    }
    drop(resp);
    flush_js(cache_ptr);
}

fn flush_js(cache: *mut Cache) {
    let Some(c) = (unsafe { cache.as_mut() }) else {
        return;
    };
    let out = c.sink.js.clone();
    let events = core::mem::take(&mut c.sink.queue);
    if let Some(out) = out {
        for e in events {
            out.deliver(e.node, e.kind, e.value);
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VideoCache(NonNull<Cache>);

impl VideoCache {
    pub fn create() -> VideoCache {
        VideoCache(NonNull::from(Box::leak(Box::new(Cache::new()))))
    }

    /// # Safety
    ///
    /// `p` is null or a pointer `raw()` returned for a cache that has not been freed.
    pub unsafe fn from_raw(p: *mut c_void) -> Option<VideoCache> {
        NonNull::new(p.cast()).map(VideoCache)
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// # Safety
    ///
    /// No copy of this handle, and no pointer `raw()` returned, may be used afterwards.
    pub unsafe fn free(self) {
        // SAFETY: the cache was leaked from a Box by create(), and the caller guarantees nothing uses it again.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }

    fn with<R>(self, f: impl FnOnce(&mut Cache) -> R) -> R {
        // SAFETY: the cache stays allocated until free(), whose contract rules out later use, and the borrow ends before any script callback can call back in.
        f(unsafe { &mut *self.0.as_ptr() })
    }

    fn flush_js(self) {
        flush_js(self.0.as_ptr());
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
        let changed = self.with(|c| c.tick(now_us));
        self.flush_js();
        changed
    }

    pub fn flush_composites(self, now_us: i64) {
        self.with(|c| c.flush_composites(now_us));
    }

    pub fn animating(self) -> bool {
        self.with(|c| c.animating())
    }

    pub fn waiting_growth(self) -> bool {
        self.with(|c| c.waiting_growth())
    }

    pub fn has_pending(self) -> bool {
        self.with(|c| !c.pending.is_empty())
    }

    pub fn mse_append(self, stream_id: u32, kind: u8, data: &[u8]) -> bool {
        stream_id != 0 && !data.is_empty() && self.with(|c| c.mse_append(stream_id, kind, data))
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
        };
        (first, end)
    }

    pub fn mse_remove(self, stream_id: u32, kind: u8, start: f64, end: f64) -> bool {
        stream_id != 0
            && start >= 0.0
            && end > start
            && self.with(|c| c.mse_remove(stream_id, kind, start, end))
    }

    pub fn mse_bytes(self, stream_id: u32, kind: u8) -> usize {
        if stream_id == 0 {
            return 0;
        }
        self.with(|c| c.mse_bytes(stream_id, kind))
    }

    pub fn helper_event(self, token: &[u8], kind: &[u8]) -> bool {
        if token.is_empty() || kind.is_empty() {
            return false;
        }
        let handled = self.with(|c| c.helper_event(token, kind));
        self.flush_js();
        handled
    }

    pub fn seek_node(self, dom_node: *const c_void, seconds: f64, now_us: i64) -> bool {
        if dom_node.is_null() {
            return false;
        }
        let handled = self.with(|c| c.seek_node(dom_node, seconds, now_us));
        self.flush_js();
        handled
    }

    pub fn set_node_playing(self, dom_node: *const c_void, play: bool, now_us: i64) -> bool {
        !dom_node.is_null() && self.with(|c| c.set_node_playing(dom_node, play, now_us))
    }

    pub fn set_node_muted(self, dom_node: *const c_void, muted: bool) -> bool {
        !dom_node.is_null() && self.with(|c| c.set_node_muted(dom_node, muted))
    }

    pub fn set_node_volume(self, dom_node: *const c_void, volume: f64) -> bool {
        !dom_node.is_null() && self.with(|c| c.set_node_volume(dom_node, volume))
    }
}

#[derive(Clone, Copy)]
pub struct BoxVideo(NonNull<NsVideo>);

impl BoxVideo {
    pub fn of(b: BoxRef<'_>) -> Option<BoxVideo> {
        NonNull::new(b.media()?.video().cast::<NsVideo>()).map(BoxVideo)
    }

    fn with<R>(self, f: impl FnOnce(&mut NsVideo) -> R) -> R {
        // SAFETY: a box's video slot holds a video the page's cache or the headless driver keeps alive while the box tree exists, and nothing else borrows it during the call.
        f(unsafe { &mut *self.0.as_ptr() })
    }

    pub fn is_camera(self) -> bool {
        self.with(|v| v.is_camera.get())
    }

    pub fn note_paint_rect(self, x: f64, y: f64, w: f64, h: f64, fit: i32) {
        self.with(|v| v.note_paint_rect(x, y, w, h, fit));
    }

    pub fn note_paint_clip(self, x: f64, y: f64, w: f64, h: f64) {
        self.with(|v| v.note_paint_clip(x, y, w, h));
    }

    pub fn helper_composited(self) -> bool {
        self.with(|v| crate::helper_composited(v))
    }

    pub fn with_poster<R>(self, f: impl FnOnce(Option<&Texture>, Option<&CStr>) -> R) -> R {
        self.with(|v| {
            f(
                v.poster_texture.as_ref(),
                v.poster_url.as_ref().map(OwnedStr::c_str),
            )
        })
    }

    pub fn take_load_events(self) -> bool {
        self.with(|v| {
            let take = v.loaded.get() && !v.load_events_fired.get();
            if take {
                v.load_events_fired.set(true);
            }
            take
        })
    }

    pub fn active_cue_text(self) -> Option<Vec<u8>> {
        self.with(|v| {
            let cues = v.cues.as_ref()?;
            let cue = southstar_webvtt::active(&cues.0, v.cur_time)?;
            Some(cue.text.to_bytes().to_vec())
        })
    }

    pub fn has_player(self) -> bool {
        self.with(|v| v.player.is_some())
    }

    pub fn toggle(self, cache: Option<VideoCache>, now_us: i64) -> bool {
        let toggled = match cache {
            Some(cache) => cache.with(|c| self.with(|v| crate::toggle(&mut c.sink, v, now_us))),
            None => self.with(|v| crate::toggle(&mut Sink::default(), v, now_us)),
        };
        if let Some(cache) = cache {
            cache.flush_js();
        }
        toggled
    }
}

pub struct StillVideo(Box<NsVideo>);

impl StillVideo {
    fn new(url: &[u8]) -> StillVideo {
        let mut v = NsVideo::zeroed();
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
        match b.media().filter(|m| m.video().is_null()) {
            Some(m) => {
                m.set_video(Box::into_raw(self.0).cast());
                Ok(())
            }
            None => Err(self),
        }
    }
}
