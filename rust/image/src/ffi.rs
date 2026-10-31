//! Southstar — the image cache and decode chain, over GTask, the network layer and the decoders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::cell::{Cell, Ref, RefCell, RefMut};
use core::ffi::{CStr, c_char, c_int, c_long, c_void};
use core::marker::PhantomData;
use core::ptr;
use std::collections::HashMap;
use std::ffi::CString;
use std::rc::Rc;

use southstar_image_decoders::{self as decoders, Pixels};
use southstar_layout::MediaHandle;
use southstar_net::{Response, monotonic_us};
use southstar_texture::TextureRef;

use crate::RetryState;

const CACHE_BUDGET_BYTES: i64 = 256 * 1024 * 1024;

#[derive(Default)]
struct State {
    url: Option<CString>,
    final_url: Option<CString>,
    cors_allow_origin: Option<CString>,
    texture: Option<TextureRef>,
    render_cache: Option<Rc<dyn Any>>,
    natural_width: c_int,
    natural_height: c_int,
    http_status: c_long,
    error: Option<CString>,
    failed_at_us: i64,
    request_us: i64,
    response_us: i64,
    next_hop_protocol: Option<CString>,
    timing_allow_origin: Option<CString>,
    body_size: i64,
    attempts: c_int,
    loaded: bool,
    failed: bool,
    purged: bool,
    bytes: i64,
    generation: u64,
    anim_frames: Option<Vec<AnimFrame>>,
    anim_start_us: i64,
    anim_current: usize,
    anim_total_ms: c_int,
}

pub struct Image {
    stamp: Cell<u64>,
    state: RefCell<State>,
}

thread_local! {
    static VACANT_IMAGES: RefCell<Vec<&'static Image>> = const { RefCell::new(Vec::new()) };
    static CACHES: RefCell<Vec<&'static Cache>> = const { RefCell::new(Vec::new()) };
    static NEXT_STAMP: Cell<u64> = const { Cell::new(1) };
}

fn next_stamp() -> u64 {
    NEXT_STAMP.with(|next| {
        let stamp = next.get();
        next.set(stamp + 1);
        stamp
    })
}

fn text_ptr(text: &Option<CString>) -> *const c_char {
    text.as_ref().map_or(ptr::null(), |t| t.as_ptr())
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

#[derive(Clone, Copy)]
pub struct ImageRef<'a> {
    image: &'static Image,
    stamp: u64,
    life: PhantomData<&'a Image>,
}

impl<'a> ImageRef<'a> {
    pub fn from_media(handle: MediaHandle) -> Option<ImageRef<'a>> {
        let image = ImageRef {
            image: handle.target().downcast_ref::<Image>()?,
            stamp: handle.stamp(),
            life: PhantomData,
        };
        image.alive().then_some(image)
    }

    pub fn media_handle(self) -> MediaHandle {
        MediaHandle::new(self.image, self.stamp)
    }

    pub fn same(self, other: ImageRef<'_>) -> bool {
        ptr::eq(self.image, other.image) && self.stamp == other.stamp
    }

    fn alive(self) -> bool {
        self.image.stamp.get() == self.stamp
    }

    fn state(self) -> Option<Ref<'static, State>> {
        self.alive().then(|| self.image.state.borrow())
    }

    fn state_mut(self) -> Option<RefMut<'static, State>> {
        self.alive().then(|| self.image.state.borrow_mut())
    }

    fn read<R: Default>(self, f: impl FnOnce(&State) -> R) -> R {
        self.state().map_or_else(R::default, |s| f(&s))
    }

    fn write(self, f: impl FnOnce(&mut State)) {
        if let Some(mut s) = self.state_mut() {
            f(&mut s);
        }
    }

    pub fn texture(self) -> Option<TextureRef> {
        self.read(|s| s.texture.clone())
    }

    pub fn has_texture(self) -> bool {
        self.read(|s| s.texture.is_some())
    }

    pub fn loaded(self) -> bool {
        self.read(|s| s.loaded)
    }

    pub fn failed(self) -> bool {
        self.read(|s| s.failed)
    }

    pub fn url(self) -> *const c_char {
        self.state().map_or(ptr::null(), |s| text_ptr(&s.url))
    }

    pub fn error(self) -> *const c_char {
        self.state().map_or(ptr::null(), |s| text_ptr(&s.error))
    }

    pub fn url_text(self) -> Option<CString> {
        self.read(|s| s.url.clone())
    }

    pub fn error_text(self) -> Option<CString> {
        self.read(|s| s.error.clone())
    }

    pub fn http_status(self) -> c_long {
        self.read(|s| s.http_status)
    }

    pub fn natural_size(self) -> (i32, i32) {
        self.read(|s| (s.natural_width, s.natural_height))
    }

    pub fn source_url(self) -> Option<CString> {
        self.read(|s| s.final_url.as_ref().or(s.url.as_ref()).cloned())
    }

    pub fn cors_allow_origin(self) -> Option<CString> {
        self.read(|s| s.cors_allow_origin.clone())
    }

    pub fn is_animated(self) -> bool {
        self.read(|s| s.anim_frames.is_some())
    }

    pub fn render_cache(self) -> Option<Rc<dyn Any>> {
        self.read(|s| s.render_cache.clone())
    }

    pub fn set_render_cache(self, cache: Rc<dyn Any>) {
        let old = self
            .state_mut()
            .and_then(|mut s| s.render_cache.replace(cache));
        drop(old);
    }

    pub fn adopt_origin(self, final_url: Option<&CStr>, cors_allow_origin: Option<&CStr>) {
        self.write(|s| {
            if s.final_url.is_none() {
                s.final_url = final_url.map(CStr::to_owned);
                s.cors_allow_origin = cors_allow_origin.map(CStr::to_owned);
            }
        });
    }

    pub fn fetch_times_us(self) -> (i64, i64) {
        self.read(|s| (s.request_us, s.response_us))
    }

    pub fn next_hop_protocol(self) -> *const c_char {
        self.state()
            .map_or(ptr::null(), |s| text_ptr(&s.next_hop_protocol))
    }

    pub fn timing_allow_origin(self) -> *const c_char {
        self.state()
            .map_or(ptr::null(), |s| text_ptr(&s.timing_allow_origin))
    }

    pub fn next_hop_protocol_text(self) -> Option<CString> {
        self.read(|s| s.next_hop_protocol.clone())
    }

    pub fn timing_allow_origin_text(self) -> Option<CString> {
        self.read(|s| s.timing_allow_origin.clone())
    }

    pub fn body_size(self) -> i64 {
        self.read(|s| s.body_size)
    }
}

fn new_image(url: &CStr) -> ImageRef<'static> {
    let image = VACANT_IMAGES
        .with(|v| v.borrow_mut().pop())
        .unwrap_or_else(|| {
            Box::leak(Box::new(Image {
                stamp: Cell::new(0),
                state: RefCell::new(State::default()),
            }))
        });
    let stamp = next_stamp();
    image.stamp.set(stamp);
    *image.state.borrow_mut() = State {
        url: Some(url.to_owned()),
        ..State::default()
    };
    ImageRef {
        image,
        stamp,
        life: PhantomData,
    }
}

fn release_image(image: ImageRef<'static>) {
    if !image.alive() {
        return;
    }
    image.image.stamp.set(0);
    let old = image.image.state.replace(State::default());
    VACANT_IMAGES.with(|v| v.borrow_mut().push(image.image));
    drop(old);
}

struct AnimFrame {
    texture: TextureRef,
    delay_ms: c_int,
}

pub type ImageReady = fn(image: ImageRef<'static>, data: usize);

struct Pending {
    img: ImageRef<'static>,
    cache: ImageCache,
    cb: Cell<Option<ImageReady>>,
    data: Cell<usize>,
    dead: Cell<bool>,
}

#[derive(Default)]
struct CacheState {
    by_url: HashMap<CString, ImageRef<'static>>,
    pending: Vec<Rc<Pending>>,
    generation: u64,
    total_bytes: i64,
}

pub struct Cache {
    stamp: Cell<u64>,
    state: RefCell<CacheState>,
}

#[derive(Clone, Copy)]
pub struct ImageCache {
    cache: &'static Cache,
    stamp: u64,
}

impl PartialEq for ImageCache {
    fn eq(&self, other: &ImageCache) -> bool {
        ptr::eq(self.cache, other.cache) && self.stamp == other.stamp
    }
}

impl Eq for ImageCache {}

struct Decoded {
    tex: Option<TextureRef>,
    frames: Option<Vec<AnimFrame>>,
    w: c_int,
    h: c_int,
}

struct DecodeJob {
    pending: Rc<Pending>,
    body: Vec<u8>,
    content_type: Vec<u8>,
}

fn texture_bytes(texture: &TextureRef) -> i64 {
    let (w, h) = (texture.width(), texture.height());
    if w > 0 && h > 0 {
        i64::from(w) * i64::from(h) * 4
    } else {
        0
    }
}

fn decoded_bytes(img: &State) -> i64 {
    match img.anim_frames.as_deref() {
        Some(anim) if !anim.is_empty() => anim.iter().map(|f| texture_bytes(&f.texture)).sum(),
        _ => img.texture.as_ref().map_or(0, texture_bytes),
    }
}

fn purge(cache: &mut CacheState, img: &mut State) {
    cache.total_bytes -= img.bytes;
    img.bytes = 0;
    img.anim_frames = None;
    img.texture = None;
    img.render_cache = None;
    img.anim_current = 0;
    img.anim_total_ms = 0;
    img.anim_start_us = 0;
    img.loaded = false;
    img.purged = true;
}

impl ImageCache {
    pub fn create() -> ImageCache {
        let vacant = CACHES.with(|c| c.borrow().iter().copied().find(|c| c.stamp.get() == 0));
        let cache = vacant.unwrap_or_else(|| {
            let cache: &'static Cache = Box::leak(Box::new(Cache {
                stamp: Cell::new(0),
                state: RefCell::new(CacheState::default()),
            }));
            CACHES.with(|c| c.borrow_mut().push(cache));
            cache
        });
        let stamp = next_stamp();
        cache.stamp.set(stamp);
        ImageCache { cache, stamp }
    }

    pub fn raw(self) -> *mut c_void {
        ptr::from_ref(self.cache).cast_mut().cast()
    }

    pub fn from_raw(raw: *mut c_void) -> Option<ImageCache> {
        CACHES.with(|c| {
            c.borrow()
                .iter()
                .copied()
                .find(|&cache| ptr::eq(ptr::from_ref(cache).cast(), raw.cast_const()))
                .filter(|cache| cache.stamp.get() != 0)
                .map(|cache| ImageCache {
                    cache,
                    stamp: cache.stamp.get(),
                })
        })
    }

    pub fn free(self) {
        if !self.alive() {
            return;
        }
        self.cache.stamp.set(0);
        let state = self.cache.state.take();
        for p in &state.pending {
            p.dead.set(true);
        }
        for (_, img) in state.by_url {
            release_image(img);
        }
    }

    fn alive(self) -> bool {
        self.cache.stamp.get() == self.stamp
    }

    fn state(self) -> Option<RefMut<'static, CacheState>> {
        self.alive().then(|| self.cache.state.borrow_mut())
    }

    fn images(self) -> Vec<ImageRef<'static>> {
        self.state()
            .map(|c| c.by_url.values().copied().collect())
            .unwrap_or_default()
    }

    fn lookup(self, url: &CStr) -> Option<ImageRef<'static>> {
        self.state()?.by_url.get(url).copied()
    }

    fn insert(self, url: &CStr, img: ImageRef<'static>) {
        let replaced = self
            .state()
            .and_then(|mut c| c.by_url.insert(url.to_owned(), img));
        if let Some(old) = replaced {
            release_image(old);
        }
    }

    fn has_pending_for(self, img: ImageRef<'_>) -> bool {
        self.state()
            .is_some_and(|c| c.pending.iter().any(|p| !p.dead.get() && p.img.same(img)))
    }

    fn generation(self) -> u64 {
        self.state().map_or(0, |c| c.generation)
    }

    fn account(self, img: ImageRef<'_>) {
        let (Some(mut cache), Some(mut img)) = (self.state(), img.state_mut()) else {
            return;
        };
        if !img.loaded || img.bytes != 0 {
            return;
        }
        img.bytes = decoded_bytes(&img);
        img.generation = cache.generation;
        cache.total_bytes += img.bytes;
    }

    fn remove_pending(self, pending: &Rc<Pending>) {
        if let Some(mut cache) = self.state()
            && let Some(index) = cache.pending.iter().position(|p| Rc::ptr_eq(p, pending))
        {
            cache.pending.swap_remove(index);
        }
    }

    fn take_pending_for(self, img: ImageRef<'_>) -> Vec<Rc<Pending>> {
        let Some(mut cache) = self.state() else {
            return Vec::new();
        };
        let mut fire = Vec::new();
        let mut i = 0;
        while i < cache.pending.len() {
            if cache.pending[i].img.same(img) {
                fire.push(cache.pending.swap_remove(i));
            } else {
                i += 1;
            }
        }
        fire
    }

    pub fn tick(self, now_us: i64) -> bool {
        let mut any = false;
        for image in self.images() {
            image.write(|img| {
                let anim = img.anim_frames.as_deref().unwrap_or_default();
                if anim.len() < 2 || img.anim_total_ms <= 0 {
                    return;
                }
                let delays: Vec<i32> = anim.iter().map(|f| f.delay_ms).collect();
                let index =
                    crate::animation_frame(&delays, img.anim_total_ms, img.anim_start_us, now_us);
                if index != img.anim_current {
                    img.texture = Some(anim[index].texture.clone());
                    img.anim_current = index;
                    any = true;
                }
            });
        }
        any
    }

    pub fn animating(self) -> bool {
        self.images().into_iter().any(|image| {
            image.read(|img| img.loaded && img.anim_frames.as_ref().is_some_and(|a| a.len() > 1))
        })
    }

    pub fn has_pending(self) -> bool {
        self.state().is_some_and(|c| !c.pending.is_empty())
    }

    pub fn get(
        self,
        url: &CStr,
        top_url: Option<&CStr>,
        ready: Option<ImageReady>,
        data: usize,
    ) -> Option<ImageRef<'static>> {
        if !self.alive() {
            return None;
        }
        Some(cache_get(self, url, top_url, ready, data))
    }

    pub fn cancel_ready(self, data: usize) {
        if let Some(cache) = self.state() {
            for p in cache.pending.iter().filter(|p| p.data.get() == data) {
                p.cb.set(None);
                p.data.set(0);
            }
        }
    }

    pub fn peek(self, url: &CStr) -> Option<ImageRef<'static>> {
        let image = self.lookup(url)?;
        let generation = self.generation();
        image.write(|img| img.generation = generation);
        Some(image)
    }

    pub fn insert_encoded(self, url: &CStr, bytes: &[u8]) -> Option<ImageRef<'static>> {
        if !self.alive() || bytes.is_empty() {
            return None;
        }
        insert_encoded(self, url, bytes)
    }

    pub fn begin_generation(self) {
        if let Some(mut cache) = self.state() {
            cache.generation = cache.generation.wrapping_add(1);
        }
    }

    pub fn collect(self) {
        if let Some(mut cache) = self.state() {
            collect(&mut cache);
        }
    }
}

fn fire_pending(pending: &Pending) {
    for p in pending.cache.take_pending_for(pending.img) {
        if let Some(cb) = p.cb.get() {
            cb(p.img, p.data.get());
        }
    }
}

pub fn decode_bytes(bytes: &[u8]) -> Option<(TextureRef, i32, i32)> {
    if bytes.is_empty() {
        return None;
    }
    if let Some(decoded) = decode_builtin(bytes) {
        return Some((decoded.texture, decoded.width, decoded.height));
    }
    if crate::bytes_blocked_on_platform(bytes) || !southstar_svg::looks_like_svg(bytes) {
        return None;
    }
    southstar_svg::decode_bytes(bytes)
}

fn decode_builtin(bytes: &[u8]) -> Option<decoders::Decoded> {
    if bytes.len() >= 4
        && bytes[0] == 0
        && bytes[1] == 0
        && (bytes[2] == 1 || bytes[2] == 2)
        && bytes[3] == 0
        && let Some(decoded) = decoders::decode_ico(bytes)
    {
        return Some(decoded);
    }
    if decoders::raster::supports(bytes)
        && let Some(decoded) = decoders::decode_raster(bytes).and_then(Pixels::into_texture)
    {
        return Some(decoded);
    }
    if decoders::webp_supports(bytes)
        && let Some(decoded) = decoders::decode_webp(bytes).and_then(Pixels::into_texture)
    {
        return Some(decoded);
    }
    #[cfg(feature = "avif")]
    if decoders::avif_supports(bytes)
        && let Some(decoded) = decoders::decode_avif(bytes).and_then(Pixels::into_texture)
    {
        return Some(decoded);
    }
    None
}

pub struct BgraPixels {
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
}

struct Bgra {
    pixels: decoders::Buffer,
    width: c_int,
    height: c_int,
    stride: usize,
    buf_len: usize,
}

fn texture_to_pixels(texture: TextureRef, w: c_int, h: c_int) -> Option<Bgra> {
    let size = if w <= 0 || h <= 0 {
        None
    } else {
        let stride = w as usize * 4;
        stride.checked_mul(h as usize).map(|len| (stride, len))
    };
    size.and_then(|(stride, buf_len)| {
        let mut buffer = decoders::Buffer::try_new(buf_len)?;
        texture.download(&mut buffer, stride);
        Some(Bgra {
            pixels: buffer,
            width: w,
            height: h,
            stride,
            buf_len,
        })
    })
}

fn decode_to_bgra(bytes: &[u8]) -> Option<Bgra> {
    let still = if decoders::raster::supports(bytes) {
        decoders::decode_raster(bytes)
    } else {
        None
    };
    let still = still.or_else(|| {
        if decoders::webp_supports(bytes) {
            decoders::decode_webp(bytes)
        } else {
            None
        }
    });
    if let Some(still) = still {
        return Some(Bgra {
            width: still.width,
            height: still.height,
            stride: still.stride,
            buf_len: still.stride * still.height as usize,
            pixels: still.buffer,
        });
    }
    #[cfg(feature = "avif")]
    if decoders::avif_supports(bytes)
        && let Some(decoded) = decoders::decode_avif(bytes).and_then(Pixels::into_texture)
    {
        let bgra = texture_to_pixels(decoded.texture, decoded.width, decoded.height);
        if bgra.is_some() {
            return bgra;
        }
    }
    if crate::bytes_blocked_on_platform(bytes) {
        return None;
    }
    let (texture, w, h) = decode_bytes(bytes)?;
    texture_to_pixels(texture, w, h)
}

pub fn decode_bytes_to_pixels(bytes: &[u8]) -> Option<BgraPixels> {
    if bytes.is_empty() {
        return None;
    }
    let bgra = decode_to_bgra(bytes)?;
    Some(BgraPixels {
        pixels: bgra.pixels.get(..bgra.buf_len)?.to_vec(),
        width: bgra.width,
        height: bgra.height,
        stride: bgra.stride,
    })
}

struct Anim {
    frames: Vec<AnimFrame>,
    w: c_int,
    h: c_int,
    total_ms: c_int,
}

fn anim_frames_from_pixels(source: Vec<decoders::Frame>) -> Option<Anim> {
    if source.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let mut ok = true;
    let mut total: i64 = 0;
    let (mut w, mut h) = (0, 0);
    for (i, decoders::Frame { pixels, delay_ms }) in source.into_iter().enumerate() {
        let (width, height) = (pixels.width, pixels.height);
        if pixels.buffer.is_empty() || width <= 0 || height <= 0 {
            ok = false;
            break;
        }
        let Some(decoded) = pixels.into_texture() else {
            ok = false;
            break;
        };
        let delay = if delay_ms > 0 { delay_ms } else { 100 };
        out.push(AnimFrame {
            texture: decoded.texture,
            delay_ms: delay,
        });
        total += i64::from(delay);
        if i == 0 {
            w = width;
            h = height;
        }
    }
    if !ok || out.is_empty() {
        return None;
    }
    Some(Anim {
        frames: out,
        w,
        h,
        total_ms: crate::total_delay_ms(core::iter::once(total)),
    })
}

fn pixel_frames_for(bytes: &[u8]) -> Option<Vec<decoders::Frame>> {
    if (bytes.len() >= 6 && bytes.starts_with(b"GIF")) || decoders::raster::png_is_animated(bytes) {
        return decoders::decode_raster_animation(bytes).map(|(frames, _, _)| frames);
    }
    if decoders::webp_supports(bytes) {
        return decoders::decode_webp_animation(bytes).map(|(frames, _, _)| frames);
    }
    None
}

fn decode_body(bytes: &[u8]) -> Decoded {
    if let Some(anim) = pixel_frames_for(bytes).and_then(anim_frames_from_pixels)
        && anim.frames.len() > 1
    {
        return Decoded {
            tex: None,
            frames: Some(anim.frames),
            w: anim.w,
            h: anim.h,
        };
    }
    let (tex, w, h) =
        decode_bytes(bytes).map_or((None, 0, 0), |(texture, w, h)| (Some(texture), w, h));
    Decoded {
        tex,
        frames: None,
        w,
        h,
    }
}

fn set_error(img: &mut State, message: &[u8]) {
    img.failed = true;
    img.failed_at_us = monotonic_us();
    img.error = Some(cstring(message));
}

fn apply_decoded_state(img: &mut State, d: Decoded, content_type: Option<&[u8]>, body_len: usize) {
    if let Some(anim) = d.frames {
        img.texture = anim.first().map(|f| f.texture.clone());
        img.natural_width = d.w;
        img.natural_height = d.h;
        img.anim_total_ms = crate::total_delay_ms(anim.iter().map(|f| i64::from(f.delay_ms)));
        img.anim_frames = Some(anim);
        img.anim_current = 0;
        img.anim_start_us = monotonic_us();
        img.loaded = true;
    } else if let Some(tex) = d.tex {
        img.texture = Some(tex);
        img.natural_width = d.w;
        img.natural_height = d.h;
        img.loaded = true;
    } else {
        set_error(img, &crate::decode_error(content_type, body_len));
    }
}

fn finish_decode(pending: &Rc<Pending>, decoded: Decoded, content_type: &[u8], body_len: usize) {
    pending
        .img
        .write(|img| apply_decoded_state(img, decoded, Some(content_type), body_len));
    pending.cache.account(pending.img);
    fire_pending(pending);
}

mod task {
    use core::ffi::c_void;
    use core::ptr;

    use southstar_glib::{GDestroyNotify, GError};

    use super::{DecodeJob, Decoded, decode_body, finish_decode};

    type AsyncCallback =
        unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void);
    type TaskThreadFunc = unsafe extern "C" fn(
        task: *mut c_void,
        source: *mut c_void,
        task_data: *mut c_void,
        cancellable: *mut c_void,
    );

    unsafe extern "C" {
        fn g_task_new(
            source: *mut c_void,
            cancellable: *mut c_void,
            callback: Option<AsyncCallback>,
            user_data: *mut c_void,
        ) -> *mut c_void;
        fn g_task_set_task_data(task: *mut c_void, data: *mut c_void, destroy: GDestroyNotify);
        fn g_task_run_in_thread(task: *mut c_void, func: Option<TaskThreadFunc>);
        fn g_task_return_pointer(task: *mut c_void, result: *mut c_void, destroy: GDestroyNotify);
        fn g_task_propagate_pointer(task: *mut c_void, error: *mut *mut GError) -> *mut c_void;
        fn g_object_unref(object: *mut c_void);
    }

    unsafe extern "C" fn decoded_free(data: *mut c_void) {
        // SAFETY: GTask hands back the Decoded box decode_worker returned, once, when nobody propagated it.
        drop(unsafe { Box::from_raw(data.cast::<Decoded>()) });
    }

    unsafe extern "C" fn decode_worker(
        task: *mut c_void,
        _source: *mut c_void,
        task_data: *mut c_void,
        _cancel: *mut c_void,
    ) {
        let job = task_data.cast_const().cast::<DecodeJob>();
        // SAFETY: the task data is the job, which on_image_decoded frees only after this worker returned; only its body is read here.
        let body = unsafe { &(*job).body };
        let decoded = Box::into_raw(Box::new(decode_body(body)));
        // SAFETY: task is the running GTask, and decoded_free reclaims the box if nobody takes it.
        unsafe { g_task_return_pointer(task, decoded.cast(), Some(decoded_free)) };
    }

    unsafe extern "C" fn on_image_decoded(
        _source: *mut c_void,
        result: *mut c_void,
        user_data: *mut c_void,
    ) {
        // SAFETY: user_data is the DecodeJob start() leaked for this task, reclaimed once here.
        let job = unsafe { Box::from_raw(user_data.cast::<DecodeJob>()) };
        // SAFETY: result is this task, finished, and its pointer has not been taken before.
        let decoded =
            unsafe { g_task_propagate_pointer(result, ptr::null_mut()) }.cast::<Decoded>();
        let decoded = (!decoded.is_null()).then(|| {
            // SAFETY: the propagated pointer is the Decoded box decode_worker leaked, now owned here.
            *unsafe { Box::from_raw(decoded) }
        });
        if job.pending.dead.get() {
            return;
        }
        let decoded = decoded.unwrap_or(Decoded {
            tex: None,
            frames: None,
            w: 0,
            h: 0,
        });
        finish_decode(&job.pending, decoded, &job.content_type, job.body.len());
    }

    pub(super) fn start(job: Box<DecodeJob>) {
        let job = Box::into_raw(job).cast::<c_void>();
        // SAFETY: on_image_decoded takes ownership of job when the task completes.
        let task = unsafe {
            g_task_new(
                ptr::null_mut(),
                ptr::null_mut(),
                Some(on_image_decoded),
                job,
            )
        };
        // SAFETY: the job outlives the worker, and GTask frees nothing.
        unsafe { g_task_set_task_data(task, job, None) };
        // SAFETY: task is the task created above, run once.
        unsafe { g_task_run_in_thread(task, Some(decode_worker)) };
        // SAFETY: drops this function's reference; the running task holds its own.
        unsafe { g_object_unref(task) };
    }
}

fn record_response(img: &mut State, resp: &Response) {
    img.http_status = resp.status();
    img.final_url = resp.final_url().map(CStr::to_owned);
    img.cors_allow_origin = resp.cors_allow_origin().map(CStr::to_owned);
    img.next_hop_protocol = resp.next_hop_protocol().map(CStr::to_owned);
    img.timing_allow_origin = resp
        .raw_headers()
        .and_then(|raw| southstar_net::raw_header_values(raw.to_bytes(), b"timing-allow-origin"))
        .map(|values| cstring(&values));
    img.body_size = resp.body().map_or(0, |b| b.len() as i64);
}

fn finish_failed(pending: &Rc<Pending>, message: &[u8]) {
    let message = if message.is_empty() {
        &b"fetch failed"[..]
    } else {
        message
    };
    pending.img.write(|img| set_error(img, message));
    if let Some(cb) = pending.cb.get() {
        cb(pending.img, pending.data.get());
    }
    pending.cache.remove_pending(pending);
}

enum Fetched {
    Done,
    Decode(Vec<u8>, Vec<u8>),
}

fn image_fetched(pending: Rc<Pending>, outcome: southstar_net::Outcome) {
    if pending.dead.get() {
        return;
    }
    pending.img.write(|img| img.response_us = monotonic_us());
    let response = match outcome {
        Ok(response) => response,
        Err(message) => return finish_failed(&pending, &message),
    };
    let mut next = Fetched::Done;
    let img = pending.img;
    img.write(|img| record_response(img, &response));
    let body = response.body().filter(|b| !b.is_empty());
    if let Some(error) = response.error() {
        img.write(|img| set_error(img, error.to_bytes()));
    } else if response.status() >= 400 {
        img.write(|img| set_error(img, format!("HTTP {}", response.status()).as_bytes()));
    } else if let Some(body) = body {
        let content_type = response.content_type().map(CStr::to_bytes);
        if southstar_config::get().is_some_and(|c| c.async_image_decode != 0) {
            next = Fetched::Decode(body.to_vec(), content_type.unwrap_or_default().to_vec());
        } else {
            let decoded = decode_body(body);
            img.write(|img| apply_decoded_state(img, decoded, content_type, body.len()));
        }
    } else {
        img.write(|img| set_error(img, b"empty response"));
    }
    drop(response);
    match next {
        Fetched::Decode(body, content_type) => task::start(Box::new(DecodeJob {
            pending,
            body,
            content_type,
        })),
        Fetched::Done => {
            pending.cache.account(pending.img);
            fire_pending(&pending);
        }
    }
}

fn retry_state(img: &State) -> RetryState {
    RetryState {
        failed: img.failed,
        attempts: img.attempts,
        http_status: img.http_status,
        failed_at_us: img.failed_at_us,
    }
}

fn start_request(
    cache: ImageCache,
    img: ImageRef<'static>,
    url: &CStr,
    top_url: Option<&CStr>,
    cb: Option<ImageReady>,
    data: usize,
) {
    let pending = Rc::new(Pending {
        img,
        cache,
        cb: Cell::new(cb),
        data: Cell::new(data),
        dead: Cell::new(false),
    });
    if let Some(mut c) = cache.state() {
        c.pending.push(Rc::clone(&pending));
    }
    img.write(|img| {
        img.attempts += 1;
        img.request_us = monotonic_us();
        img.response_us = 0;
    });
    let headers = southstar_net::FetchDest::Image.header_lines();
    let request = southstar_net::FetchRequest {
        url: url.to_bytes(),
        top_url: top_url.map(CStr::to_bytes),
        method: Some(b"GET"),
        headers: &headers,
        ..southstar_net::FetchRequest::default()
    };
    southstar_net::request_with(&request, None, move |outcome| {
        image_fetched(pending, outcome);
    });
}

fn images_disabled() -> bool {
    southstar_config::get().is_some_and(|c| c.images_enabled == 0)
}

fn settled(img: ImageRef<'_>) -> bool {
    img.read(|s| s.loaded || s.failed)
}

fn cache_get(
    cache: ImageCache,
    url: &CStr,
    top_url: Option<&CStr>,
    cb: Option<ImageReady>,
    data: usize,
) -> ImageRef<'static> {
    let Some(cached) = cache.lookup(url) else {
        let img = new_image(url);
        cache.insert(url, img);
        if images_disabled() {
            img.write(|s| s.failed = true);
            if let Some(cb) = cb {
                cb(img, data);
            }
        } else {
            start_request(cache, img, url, top_url, cb, data);
        }
        return img;
    };
    let generation = cache.generation();
    cached.write(|s| s.generation = generation);
    if images_disabled() {
        if let Some(cb) = cb
            && settled(cached)
        {
            cb(cached, data);
        }
        return cached;
    }
    if cached.read(|s| s.purged) && !cache.has_pending_for(cached) {
        cached.write(|s| s.purged = false);
        start_request(cache, cached, url, top_url, cb, data);
        return cached;
    }
    if cached.read(|s| crate::should_retry(&retry_state(s), monotonic_us())) {
        cached.write(|s| {
            s.failed = false;
            s.http_status = 0;
            s.failed_at_us = 0;
            s.error = None;
        });
        start_request(cache, cached, url, top_url, cb, data);
        return cached;
    }
    if settled(cached) {
        if let Some(cb) = cb {
            cb(cached, data);
        }
    } else if let Some(cb) = cb
        && let Some(mut c) = cache.state()
    {
        let duplicate = c.pending.iter().any(|p| {
            !p.dead.get()
                && p.img.same(cached)
                && p.cb.get().is_some_and(|f| ptr::fn_addr_eq(f, cb))
                && p.data.get() == data
        });
        if !duplicate {
            c.pending.push(Rc::new(Pending {
                img: cached,
                cache,
                cb: Cell::new(Some(cb)),
                data: Cell::new(data),
                dead: Cell::new(false),
            }));
        }
    }
    cached
}

fn insert_loaded(
    cache: ImageCache,
    url: &CStr,
    fill: impl FnOnce(&mut State),
) -> ImageRef<'static> {
    let img = new_image(url);
    img.write(|s| {
        fill(s);
        s.loaded = true;
    });
    cache.insert(url, img);
    cache.account(img);
    img
}

fn insert_encoded(cache: ImageCache, url: &CStr, bytes: &[u8]) -> Option<ImageRef<'static>> {
    if let Some(existing) = cache.lookup(url) {
        return Some(existing);
    }
    let anim = pixel_frames_for(bytes)
        .filter(|frames| frames.len() > 1)
        .and_then(anim_frames_from_pixels);
    if let Some(anim) = anim
        && anim.frames.len() > 1
    {
        return Some(insert_loaded(cache, url, |s| {
            s.texture = anim.frames.first().map(|f| f.texture.clone());
            s.anim_frames = Some(anim.frames);
            s.natural_width = anim.w;
            s.natural_height = anim.h;
            s.anim_total_ms = anim.total_ms;
            s.anim_start_us = monotonic_us();
        }));
    }
    let (texture, w, h) = decode_bytes(bytes)?;
    Some(insert_loaded(cache, url, |s| {
        s.texture = Some(texture);
        s.natural_width = w;
        s.natural_height = h;
    }))
}

fn collect(cache: &mut CacheState) {
    while cache.total_bytes > CACHE_BUDGET_BYTES {
        let mut victim: Option<ImageRef<'static>> = None;
        let mut oldest = u64::MAX;
        for &img in cache.by_url.values() {
            let pending = cache
                .pending
                .iter()
                .any(|p| !p.dead.get() && p.img.same(img));
            let generation = img.read(|s| {
                (s.loaded && s.bytes != 0 && s.generation != cache.generation)
                    .then_some(s.generation)
            });
            if let Some(generation) = generation
                && !pending
                && generation < oldest
            {
                oldest = generation;
                victim = Some(img);
            }
        }
        let Some(mut victim) = victim.and_then(ImageRef::state_mut) else {
            break;
        };
        purge(cache, &mut victim);
    }
}
