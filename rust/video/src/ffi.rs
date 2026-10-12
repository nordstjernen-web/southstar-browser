//! Southstar — the C ABI of src/video.h: the ns_video struct and the video cache, over the decoder of src/video_decode.h, the network, textures, the camera, the DOM and the box tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GPtrArray};
use southstar_layout::{BoxRef, NsBox};
use southstar_style::{Kind, PropId, StyleRef};

use crate::vtt::Cue;
use crate::{Cache, FetchKind, Sink};

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

    pub fn bytes(&self) -> &[u8] {
        unsafe { CStr::from_ptr(self.0.as_ptr()) }.to_bytes()
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

#[repr(transparent)]
pub struct Texture(NonNull<c_void>);

impl Texture {
    fn adopt(p: *mut c_void) -> Option<Texture> {
        NonNull::new(p).map(Texture)
    }

    fn retain(p: *mut c_void) -> Option<Texture> {
        let t = NonNull::new(p)?;
        unsafe { ns_texture_ref(t.as_ptr()) };
        Some(Texture(t))
    }

    pub fn width(&self) -> i32 {
        unsafe { ns_texture_get_width(self.0.as_ptr()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { ns_texture_get_height(self.0.as_ptr()) }
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { ns_texture_unref(self.0.as_ptr()) };
    }
}

#[repr(transparent)]
pub struct Player(NonNull<c_void>);

impl Player {
    pub fn new(bytes: &[u8]) -> Option<Player> {
        NonNull::new(unsafe { ns_video_player_new(bytes.as_ptr(), bytes.len()) }).map(Player)
    }

    pub fn has_audio(&self) -> bool {
        unsafe { ns_video_player_has_audio(self.0.as_ptr()) != 0 }
    }

    pub fn width(&self) -> i32 {
        unsafe { ns_video_player_width(self.0.as_ptr()) }
    }

    pub fn height(&self) -> i32 {
        unsafe { ns_video_player_height(self.0.as_ptr()) }
    }

    pub fn duration(&self) -> f64 {
        unsafe { ns_video_player_duration(self.0.as_ptr()) }
    }

    pub fn buffered_end(&self) -> f64 {
        unsafe { ns_video_player_buffered_end(self.0.as_ptr()) }
    }

    pub fn note_end(&mut self, end: f64) {
        unsafe { ns_video_player_note_end(self.0.as_ptr(), end) };
    }

    pub fn extend(&mut self, bytes: &[u8]) -> bool {
        unsafe { ns_video_player_extend(self.0.as_ptr(), bytes.as_ptr(), bytes.len()) != 0 }
    }

    pub fn frame_at(&mut self, seconds: f64, looping: bool) -> (Option<Texture>, bool) {
        let mut ended: GBoolean = 0;
        let frame = unsafe {
            ns_video_player_frame_at(self.0.as_ptr(), seconds, glib::boolean(looping), &mut ended)
        };
        (Texture::retain(frame), ended != 0)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe { ns_video_player_free(self.0.as_ptr()) };
    }
}

pub fn probe_chunk_range(init: &[u8], chunk: &[u8]) -> Option<(f64, f64)> {
    let (init_ptr, init_len) = if init.is_empty() {
        (ptr::null(), 0)
    } else {
        (init.as_ptr(), init.len())
    };
    let mut start = 0.0;
    let mut end = 0.0;
    let ranged = unsafe {
        ns_video_probe_chunk_range(
            init_ptr,
            init_len,
            chunk.as_ptr(),
            chunk.len(),
            &mut start,
            &mut end,
        )
    };
    (ranged != 0).then_some((start, end))
}

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

#[derive(Clone, Copy)]
pub struct AudioOut {
    cb: AudioCb,
    user: *mut c_void,
}

impl AudioOut {
    pub fn send(&self, command: &[u8]) {
        if let Ok(command) = CString::new(command) {
            unsafe { (self.cb)(command.as_ptr(), self.user) };
        }
    }
}

#[derive(Clone, Copy)]
pub struct JsOut {
    cb: JsCb,
    user: *mut c_void,
}

impl JsOut {
    fn deliver(&self, node: *const c_void, kind: &CStr, value: f64) {
        unsafe { (self.cb)(node, kind.as_ptr(), value, self.user) };
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

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    _headers: [*mut c_char; 9],
    body: *mut GByteArray,
    error: *mut c_char,
}

pub struct Response(*mut NsResponse);

impl Response {
    pub fn body(&self) -> Option<&[u8]> {
        let r = unsafe { self.0.as_ref() }?;
        if !r.error.is_null() || r.body.is_null() {
            return None;
        }
        let body = unsafe { &*r.body };
        if body.len == 0 || body.data.is_null() {
            return None;
        }
        Some(unsafe { glib::slice(body.data, body.len as usize) })
    }

    pub fn final_url(&self) -> Option<&[u8]> {
        let r = unsafe { self.0.as_ref() }?;
        unsafe { glib::bytes(r.final_url) }
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { ns_response_free(self.0) };
    }
}

type AudioCb = unsafe extern "C" fn(command: *const c_char, user_data: *mut c_void);
type JsCb = unsafe extern "C" fn(
    dom_node: *const c_void,
    kind: *const c_char,
    value: f64,
    user: *mut c_void,
);
type ReadyCallback =
    unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void);

unsafe extern "C" {
    fn ns_texture_ref(texture: *mut c_void) -> *mut c_void;
    fn ns_texture_unref(texture: *mut c_void);
    fn ns_texture_get_width(texture: *mut c_void) -> c_int;
    fn ns_texture_get_height(texture: *mut c_void) -> c_int;
    fn ns_video_player_new(bytes: *const u8, len: usize) -> *mut c_void;
    fn ns_video_player_free(player: *mut c_void);
    fn ns_video_player_has_audio(player: *const c_void) -> GBoolean;
    fn ns_video_player_width(player: *const c_void) -> c_int;
    fn ns_video_player_height(player: *const c_void) -> c_int;
    fn ns_video_player_duration(player: *const c_void) -> f64;
    fn ns_video_player_buffered_end(player: *const c_void) -> f64;
    fn ns_video_player_note_end(player: *mut c_void, end: f64);
    fn ns_video_probe_chunk_range(
        init: *const u8,
        init_len: usize,
        chunk: *const u8,
        chunk_len: usize,
        out_start: *mut f64,
        out_end: *mut f64,
    ) -> GBoolean;
    fn ns_video_player_extend(player: *mut c_void, bytes: *const u8, len: usize) -> GBoolean;
    fn ns_video_player_frame_at(
        player: *mut c_void,
        seconds: f64,
        looping: GBoolean,
        out_ended: *mut GBoolean,
    ) -> *mut c_void;
    fn ns_camera_active() -> *mut c_void;
    fn ns_camera_next_frame(camera: *mut c_void) -> *mut c_void;
    fn ns_image_decode_bytes(
        data: *const u8,
        len: usize,
        out_w: *mut c_int,
        out_h: *mut c_int,
    ) -> *mut c_void;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_net_fetch_async(
        url: *const c_char,
        top_url: *const c_char,
        cancellable: *mut c_void,
        callback: Option<ReadyCallback>,
        user_data: *mut c_void,
    );
    fn ns_net_fetch_finish(result: *mut c_void, error: *mut *mut GError) -> *mut NsResponse;
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_node_next_in_subtree(
        n: *const NsNode,
        root: *const NsNode,
        descend: GBoolean,
    ) -> *mut NsNode;
    fn ns_layout_collect_videos(root: *const NsBox, out: *mut GPtrArray);
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

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take_gstr(p: *mut c_char) -> Option<Vec<u8>> {
    let s = unsafe { glib::GStr::take(p) }?;
    Some(s.to_bytes().to_vec())
}

pub fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    let base = base.map(cstring);
    let href = cstring(href);
    take_gstr(unsafe {
        ns_url_resolve(
            base.as_ref().map_or(ptr::null(), |b| b.as_ptr()),
            href.as_ptr(),
        )
    })
}

pub fn decode_image(bytes: &[u8]) -> Option<(Texture, i32, i32)> {
    let mut w = 0;
    let mut h = 0;
    let tex = unsafe { ns_image_decode_bytes(bytes.as_ptr(), bytes.len(), &mut w, &mut h) };
    Texture::adopt(tex).map(|t| (t, w, h))
}

pub fn camera_active() -> bool {
    !unsafe { ns_camera_active() }.is_null()
}

pub fn camera_next_frame() -> Option<Option<Texture>> {
    let cam = unsafe { ns_camera_active() };
    if cam.is_null() {
        return None;
    }
    Some(Texture::adopt(unsafe { ns_camera_next_frame(cam) }))
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
    unsafe {
        Node::from_ptr(ns_node_next_in_subtree(
            n.as_ptr(),
            root.as_ptr(),
            glib::TRUE,
        ))
    }
}

pub fn collect_videos(root: BoxRef<'_>) -> Vec<BoxRef<'_>> {
    let array = unsafe { glib::g_ptr_array_new() };
    unsafe { ns_layout_collect_videos(root.as_ptr(), array) };
    let a = unsafe { &*array };
    let out = (0..a.len as usize)
        .filter_map(|i| unsafe { BoxRef::from_ptr((*a.pdata.add(i)).cast::<NsBox>()) })
        .collect();
    unsafe { glib::g_ptr_array_free(array, glib::TRUE) };
    out
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
    let url = cstring(url);
    let base = cache.base_url.as_deref().map(cstring);
    unsafe {
        ns_net_fetch_async(
            url.as_ptr(),
            base.as_ref().map_or(ptr::null(), |b| b.as_ptr()),
            ptr::null_mut(),
            Some(on_fetched),
            pending.as_ptr().cast(),
        )
    };
}

unsafe extern "C" fn on_fetched(_source: *mut c_void, result: *mut c_void, user_data: *mut c_void) {
    let raw = user_data.cast::<Pending>();
    let pending = unsafe { Box::from_raw(raw) };
    let mut error = ptr::null_mut();
    let resp = Response(unsafe { ns_net_fetch_finish(result, &mut error) });
    if !error.is_null() {
        unsafe { glib::g_error_free(error) };
    }
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
    let out = c.sink.js;
    let events = core::mem::take(&mut c.sink.queue);
    if let Some(out) = out {
        for e in events {
            out.deliver(e.node, e.kind, e.value);
        }
    }
}

fn cache_mut<'a>(cache: *mut Cache) -> Option<&'a mut Cache> {
    unsafe { cache.as_mut() }
}

fn cache_ref<'a>(cache: *const Cache) -> Option<&'a Cache> {
    unsafe { cache.as_ref() }
}

fn video_mut<'a>(v: *mut NsVideo) -> Option<&'a mut NsVideo> {
    unsafe { v.as_mut() }
}

fn video_ref<'a>(v: *const NsVideo) -> Option<&'a NsVideo> {
    unsafe { v.as_ref() }
}

fn c_bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_video_cache_new() -> *mut Cache {
    Box::into_raw(Box::new(Cache::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_free(cache: *mut Cache) {
    if !cache.is_null() {
        drop(unsafe { Box::from_raw(cache) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_poster_texture(
    v: *const NsVideo,
    url: *mut *const c_char,
) -> *mut c_void {
    let v = video_ref(v);
    if let Some(url) = unsafe { url.as_mut() } {
        *url = v
            .and_then(|v| v.poster_url.as_ref())
            .map_or(ptr::null(), |s| s.0.as_ptr().cast_const());
    }
    v.and_then(|v| v.poster_texture.as_ref())
        .map_or(ptr::null_mut(), |t| t.0.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_base(cache: *mut Cache, base_url: *const c_char) {
    if let Some(c) = cache_mut(cache) {
        c.base_url = c_bytes(base_url).map(<[u8]>::to_vec);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_js_cb(
    cache: *mut Cache,
    cb: Option<JsCb>,
    user_data: *mut c_void,
) {
    if let Some(c) = cache_mut(cache) {
        c.sink.js = cb.map(|cb| JsOut {
            cb,
            user: user_data,
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_audio_cb(
    cache: *mut Cache,
    cb: Option<AudioCb>,
    user_data: *mut c_void,
) {
    if let Some(c) = cache_mut(cache) {
        c.sink.audio = cb.map(|cb| AudioOut {
            cb,
            user: user_data,
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_discover(
    cache: *mut Cache,
    root: *const NsBox,
    doc: *const NsNode,
    _now_us: i64,
) {
    let (Some(c), Some(root)) = (cache_mut(cache), unsafe { BoxRef::from_ptr(root) }) else {
        return;
    };
    c.discover(root, unsafe { Node::from_ptr(doc) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_page_coords(cache: *mut Cache, on: GBoolean) {
    if let Some(c) = cache_mut(cache) {
        c.page_coords = on != 0;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_note_layout(
    cache: *mut Cache,
    root: *const NsBox,
    scroll_x: f64,
    scroll_y: f64,
    scale: f64,
) {
    let (Some(c), Some(root)) = (cache_mut(cache), unsafe { BoxRef::from_ptr(root) }) else {
        return;
    };
    c.note_layout(root, scroll_x, scroll_y, scale);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_tick(cache: *mut Cache, now_us: i64) -> GBoolean {
    let Some(c) = cache_mut(cache) else {
        return glib::FALSE;
    };
    let changed = c.tick(now_us);
    flush_js(cache);
    glib::boolean(changed)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_flush_composites(cache: *mut Cache, now_us: i64) {
    if let Some(c) = cache_mut(cache) {
        c.flush_composites(now_us);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_animating(cache: *const Cache) -> GBoolean {
    glib::boolean(cache_ref(cache).is_some_and(Cache::animating))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_waiting_growth(cache: *const Cache) -> GBoolean {
    glib::boolean(cache_ref(cache).is_some_and(Cache::waiting_growth))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_has_pending(cache: *const Cache) -> GBoolean {
    glib::boolean(cache_ref(cache).is_some_and(|c| !c.pending.is_empty()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_mse_append(
    cache: *mut Cache,
    stream_id: c_uint,
    kind: c_char,
    data: *const u8,
    len: usize,
) -> GBoolean {
    let Some(c) = cache_mut(cache) else {
        return glib::FALSE;
    };
    if stream_id == 0 || data.is_null() || len == 0 {
        return glib::FALSE;
    }
    let data = unsafe { glib::slice(data, len) };
    glib::boolean(c.mse_append(stream_id, kind as u8, data))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_mse_eos(cache: *mut Cache, stream_id: c_uint) {
    if let Some(c) = cache_mut(cache)
        && stream_id != 0
    {
        c.mse_eos(stream_id);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_mse_buffered(
    cache: *mut Cache,
    stream_id: c_uint,
    kind: c_char,
    start: *mut f64,
) -> f64 {
    let start = unsafe { start.as_mut() };
    let mut first = 0.0;
    let end = match cache_mut(cache) {
        Some(c) if stream_id != 0 => c.mse_buffered(stream_id, kind as u8, &mut first),
        _ => 0.0,
    };
    if let Some(start) = start {
        *start = first;
    }
    end
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_mse_remove(
    cache: *mut Cache,
    stream_id: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
) -> GBoolean {
    let Some(c) = cache_mut(cache) else {
        return glib::FALSE;
    };
    if stream_id == 0 || start < 0.0 || end <= start {
        return glib::FALSE;
    }
    glib::boolean(c.mse_remove(stream_id, kind as u8, start, end))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_mse_bytes(
    cache: *mut Cache,
    stream_id: c_uint,
    kind: c_char,
) -> usize {
    match cache_mut(cache) {
        Some(c) if stream_id != 0 => c.mse_bytes(stream_id, kind as u8),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_helper_event(
    cache: *mut Cache,
    token: *const c_char,
    kind: *const c_char,
) -> GBoolean {
    let (Some(c), Some(token), Some(kind)) = (cache_mut(cache), c_bytes(token), c_bytes(kind))
    else {
        return glib::FALSE;
    };
    if token.is_empty() || kind.is_empty() {
        return glib::FALSE;
    }
    let handled = c.helper_event(token, kind);
    flush_js(cache);
    glib::boolean(handled)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_url_is_inline(url: *const c_char) -> GBoolean {
    glib::boolean(c_bytes(url).is_some_and(crate::url_is_inline_video))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_seek_node(
    cache: *mut Cache,
    dom_node: *const c_void,
    seconds: f64,
    now_us: i64,
) -> GBoolean {
    let Some(c) = cache_mut(cache) else {
        return glib::FALSE;
    };
    if dom_node.is_null() {
        return glib::FALSE;
    }
    let handled = c.seek_node(dom_node, seconds, now_us);
    flush_js(cache);
    glib::boolean(handled)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_node_playing(
    cache: *mut Cache,
    dom_node: *const c_void,
    play: GBoolean,
    now_us: i64,
) -> GBoolean {
    match cache_mut(cache) {
        Some(c) if !dom_node.is_null() => {
            glib::boolean(c.set_node_playing(dom_node, play != 0, now_us))
        }
        _ => glib::FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_node_muted(
    cache: *mut Cache,
    dom_node: *const c_void,
    muted: GBoolean,
) -> GBoolean {
    match cache_mut(cache) {
        Some(c) if !dom_node.is_null() => glib::boolean(c.set_node_muted(dom_node, muted != 0)),
        _ => glib::FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_set_node_volume(
    cache: *mut Cache,
    dom_node: *const c_void,
    volume: f64,
) -> GBoolean {
    match cache_mut(cache) {
        Some(c) if !dom_node.is_null() => glib::boolean(c.set_node_volume(dom_node, volume)),
        _ => glib::FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_cache_toggle(
    cache: *mut Cache,
    v: *mut NsVideo,
    now_us: i64,
) -> GBoolean {
    let Some(v) = video_mut(v) else {
        return glib::FALSE;
    };
    let mut detached = Sink::default();
    let sink = match cache_mut(cache) {
        Some(c) => &mut c.sink,
        None => &mut detached,
    };
    let toggled = crate::toggle(sink, v, now_us);
    flush_js(cache);
    glib::boolean(toggled)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_toggle(v: *mut NsVideo, now_us: i64) -> GBoolean {
    glib::boolean(video_mut(v).is_some_and(|v| v.toggle(now_us)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_play(v: *mut NsVideo, now_us: i64) {
    if let Some(v) = video_mut(v) {
        v.play(now_us);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_pause(v: *mut NsVideo, now_us: i64) {
    if let Some(v) = video_mut(v) {
        v.pause(now_us);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_note_paint_rect(
    v: *mut NsVideo,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fit: c_int,
) {
    if let Some(v) = video_mut(v) {
        v.note_paint_rect(x, y, w, h, fit);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_note_paint_clip(v: *mut NsVideo, x: f64, y: f64, w: f64, h: f64) {
    if let Some(v) = video_mut(v) {
        v.note_paint_clip(x, y, w, h);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_helper_composited(v: *const NsVideo) -> GBoolean {
    glib::boolean(video_ref(v).is_some_and(crate::helper_composited))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_video_active_cue_text(v: *const NsVideo) -> *const c_char {
    let Some(v) = video_ref(v) else {
        return ptr::null();
    };
    v.cues
        .as_ref()
        .and_then(|cues| crate::vtt::active(&cues.0, v.cur_time))
        .map_or(ptr::null(), |c| c.text.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_video_take_load_events(video: *mut NsVideo) -> GBoolean {
    let Some(video) = (unsafe { video.as_mut() }) else {
        return glib::FALSE;
    };
    if !video.loaded.get() || video.load_events_fired.get() {
        return glib::FALSE;
    }
    video.load_events_fired.set(true);
    glib::TRUE
}
