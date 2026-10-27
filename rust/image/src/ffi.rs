//! Southstar — the image cache and decode chain, over GLib, GTask, the network layer and the decoders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::mem::size_of;
use core::ptr;

use southstar_glib::{self as glib, FALSE, GBoolean, GError, GHashTable, TRUE};

use southstar_image_decoders::{self as decoders, Pixels};
use southstar_net::Response;
use southstar_texture::{Texture, TextureRef};

use crate::RetryState;

const CACHE_BUDGET_BYTES: i64 = 256 * 1024 * 1024;

#[repr(C)]
pub struct GArray {
    data: *mut c_char,
    len: c_uint,
}

#[repr(C)]
pub struct NsImage {
    url: *mut c_char,
    final_url: *mut c_char,
    cors_allow_origin: *mut c_char,
    texture: *mut Texture,
    render_surface: *mut c_void,
    natural_width: c_int,
    natural_height: c_int,
    http_status: c_long,
    error: *mut c_char,
    failed_at_us: i64,
    request_us: i64,
    response_us: i64,
    next_hop_protocol: *mut c_char,
    timing_allow_origin: *mut c_char,
    body_size: i64,
    attempts: c_int,
    loaded: GBoolean,
    failed: GBoolean,
    purged: GBoolean,
    bytes: i64,
    generation: u64,
    anim_frames: *mut GArray,
    anim_start_us: i64,
    anim_current: c_int,
    anim_total_ms: c_int,
}

#[derive(Clone, Copy)]
pub struct ImageRef<'a>(
    ptr::NonNull<NsImage>,
    core::marker::PhantomData<&'a NsImage>,
);

impl<'a> ImageRef<'a> {
    pub unsafe fn from_ptr(image: *const c_void) -> Option<ImageRef<'a>> {
        ptr::NonNull::new(image.cast_mut().cast()).map(|p| ImageRef(p, core::marker::PhantomData))
    }

    fn raw(self) -> &'a NsImage {
        unsafe { &*self.0.as_ptr() }
    }

    pub fn texture(self) -> *mut c_void {
        self.raw().texture.cast()
    }

    pub fn loaded(self) -> bool {
        self.raw().loaded != 0
    }

    pub fn failed(self) -> bool {
        self.raw().failed != 0
    }

    pub fn as_ptr(self) -> *const c_void {
        self.0.as_ptr().cast()
    }

    pub fn url(self) -> *const c_char {
        self.raw().url
    }

    pub fn error(self) -> *const c_char {
        self.raw().error
    }

    pub fn http_status(self) -> c_long {
        self.raw().http_status
    }

    pub fn natural_size(self) -> (i32, i32) {
        let image = self.raw();
        (image.natural_width, image.natural_height)
    }

    pub fn source_url(self) -> *const c_char {
        let image = self.raw();
        if image.final_url.is_null() {
            image.url
        } else {
            image.final_url
        }
    }

    pub fn cors_allow_origin(self) -> *const c_char {
        self.raw().cors_allow_origin
    }

    pub fn is_animated(self) -> bool {
        !self.raw().anim_frames.is_null()
    }

    pub fn render_surface(self) -> *mut c_void {
        self.raw().render_surface
    }

    pub fn adopt_origin(self, final_url: Option<&CStr>, cors_allow_origin: Option<&CStr>) {
        let image = self.0.as_ptr();
        // SAFETY: the image is live while its cache holds it, and only the main thread touches it.
        if !unsafe { (*image).final_url }.is_null() {
            return;
        }
        let dup =
            |value: Option<&CStr>| value.map_or(ptr::null_mut(), |v| glib::strdup(v.to_bytes()));
        let final_url = dup(final_url);
        let cors_allow_origin = dup(cors_allow_origin);
        // SAFETY: as above; the field was null and now owns a fresh copy.
        unsafe { (*image).final_url = final_url };
        // SAFETY: as above; the field was null and now owns a fresh copy.
        unsafe { (*image).cors_allow_origin = cors_allow_origin };
    }

    pub fn set_render_surface(self, surface: *mut c_void) {
        unsafe { (*self.0.as_ptr()).render_surface = surface };
    }

    pub fn fetch_times_us(self) -> (i64, i64) {
        let image = self.raw();
        (image.request_us, image.response_us)
    }

    pub fn next_hop_protocol(self) -> *const c_char {
        self.raw().next_hop_protocol
    }

    pub fn timing_allow_origin(self) -> *const c_char {
        self.raw().timing_allow_origin
    }

    pub fn body_size(self) -> i64 {
        self.raw().body_size
    }
}

#[repr(C)]
struct AnimFrame {
    texture: *mut Texture,
    delay_ms: c_int,
}

pub type ImageReady = fn(image: ImageRef<'static>, data: usize);
type AsyncCallback =
    unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void);
type TaskThreadFunc = unsafe extern "C" fn(
    task: *mut c_void,
    source: *mut c_void,
    task_data: *mut c_void,
    cancellable: *mut c_void,
);
type HashForeach =
    unsafe extern "C" fn(key: *mut c_void, value: *mut c_void, user_data: *mut c_void);

unsafe extern "C" {
    fn cairo_surface_destroy(surface: *mut c_void);
    fn g_get_monotonic_time() -> i64;
    fn g_array_new(zero_terminated: GBoolean, clear: GBoolean, element_size: c_uint)
    -> *mut GArray;
    fn g_array_append_vals(array: *mut GArray, data: *const c_void, len: c_uint) -> *mut GArray;
    fn g_array_set_clear_func(array: *mut GArray, clear_func: glib::GDestroyNotify);
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn g_bytes_new(data: *const c_void, size: usize) -> *mut c_void;
    fn g_bytes_get_data(bytes: *mut c_void, size: *mut usize) -> *const u8;
    fn g_bytes_unref(bytes: *mut c_void);
    fn g_hash_table_foreach(
        table: *mut GHashTable,
        func: Option<HashForeach>,
        user_data: *mut c_void,
    );
    fn g_hash_table_destroy(table: *mut GHashTable);
    fn g_task_new(
        source: *mut c_void,
        cancellable: *mut c_void,
        callback: Option<AsyncCallback>,
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn g_task_set_task_data(task: *mut c_void, data: *mut c_void, destroy: glib::GDestroyNotify);
    fn g_task_run_in_thread(task: *mut c_void, func: Option<TaskThreadFunc>);
    fn g_task_return_pointer(task: *mut c_void, result: *mut c_void, destroy: glib::GDestroyNotify);
    fn g_task_propagate_pointer(task: *mut c_void, error: *mut *mut GError) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
}

pub struct Cache {
    by_url: *mut GHashTable,
    pending: Vec<*mut Pending>,
    generation: u64,
    total_bytes: i64,
}

struct Pending {
    img: *mut NsImage,
    cache: *mut Cache,
    cb: Option<ImageReady>,
    data: usize,
    dead: bool,
}

struct Decoded {
    tex: *mut Texture,
    frames: *mut GArray,
    w: c_int,
    h: c_int,
}

struct DecodeJob {
    pending: *mut Pending,
    body: *mut c_void,
    content_type: Vec<u8>,
    body_len: usize,
}

fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

unsafe fn data_slice<'a>(data: *const u8, len: usize) -> &'a [u8] {
    unsafe { glib::slice(data, len) }
}

unsafe fn frames<'a, T>(array: *mut GArray) -> &'a mut [T] {
    match unsafe { array.as_ref() } {
        Some(a) if a.len > 0 && !a.data.is_null() => unsafe {
            core::slice::from_raw_parts_mut(a.data.cast::<T>(), a.len as usize)
        },
        _ => &mut [],
    }
}

fn gfree<T>(p: *mut T) {
    unsafe { glib::g_free(p.cast()) };
}

unsafe extern "C" fn anim_frame_clear(data: *mut c_void) {
    if let Some(frame) = unsafe { data.cast::<AnimFrame>().as_ref() } {
        // SAFETY: each frame owns one reference to its texture, given up as the frame is cleared.
        unsafe { TextureRef::release(frame.texture) };
    }
}

unsafe extern "C" fn image_free(p: *mut c_void) {
    let Some(img) = (unsafe { p.cast::<NsImage>().as_mut() }) else {
        return;
    };
    for s in [
        img.url,
        img.final_url,
        img.cors_allow_origin,
        img.next_hop_protocol,
        img.timing_allow_origin,
        img.error,
    ] {
        gfree(s);
    }
    if !img.render_surface.is_null() {
        // SAFETY: the image owns one reference to its render surface, released once here.
        unsafe { cairo_surface_destroy(img.render_surface) };
    }
    if img.anim_frames.is_null() {
        // SAFETY: the image owns its texture reference, released once here.
        unsafe { TextureRef::release(img.texture) };
    } else {
        // SAFETY: the image owns its frame array, freed once here with its elements.
        unsafe { g_array_free(img.anim_frames, TRUE) };
    }
    gfree(p);
}

fn new_image(url: *const c_char) -> *mut NsImage {
    let img = unsafe { glib::g_malloc0(size_of::<NsImage>()) }.cast::<NsImage>();
    unsafe { (*img).url = glib::g_strdup(url) };
    img
}

fn texture_bytes(texture: *mut Texture) -> i64 {
    // SAFETY: image and frame textures are null or held by the image that is being measured.
    let Some(texture) = (unsafe { Texture::borrow(texture) }) else {
        return 0;
    };
    let (w, h) = (texture.width(), texture.height());
    if w > 0 && h > 0 {
        i64::from(w) * i64::from(h) * 4
    } else {
        0
    }
}

fn decoded_bytes(img: &NsImage) -> i64 {
    let anim: &mut [AnimFrame] = unsafe { frames(img.anim_frames) };
    if !anim.is_empty() {
        return anim.iter().map(|f| texture_bytes(f.texture)).sum();
    }
    texture_bytes(img.texture)
}

unsafe fn cache_account(cache: *mut Cache, img: *mut NsImage) {
    let (Some(cache), Some(img)) = (unsafe { cache.as_mut() }, unsafe { img.as_mut() }) else {
        return;
    };
    if img.loaded == FALSE || img.bytes != 0 {
        return;
    }
    img.bytes = decoded_bytes(img);
    img.generation = cache.generation;
    cache.total_bytes += img.bytes;
}

fn has_pending(cache: &Cache, img: *mut NsImage) -> bool {
    cache
        .pending
        .iter()
        .any(|&p| unsafe { !(*p).dead && (*p).img == img })
}

fn purge(cache: &mut Cache, img: &mut NsImage) {
    cache.total_bytes -= img.bytes;
    img.bytes = 0;
    unsafe {
        if img.anim_frames.is_null() {
            TextureRef::release(img.texture);
        } else {
            g_array_free(img.anim_frames, TRUE);
            img.anim_frames = ptr::null_mut();
        }
        img.texture = ptr::null_mut();
        if !img.render_surface.is_null() {
            cairo_surface_destroy(img.render_surface);
            img.render_surface = ptr::null_mut();
        }
    }
    img.anim_current = 0;
    img.anim_total_ms = 0;
    img.anim_start_us = 0;
    img.loaded = FALSE;
    img.purged = TRUE;
}

fn for_each_image<F: FnMut(*mut NsImage)>(table: *mut GHashTable, mut f: F) {
    unsafe extern "C" fn trampoline<F: FnMut(*mut NsImage)>(
        _key: *mut c_void,
        value: *mut c_void,
        user_data: *mut c_void,
    ) {
        let f = unsafe { &mut *user_data.cast::<F>() };
        f(value.cast());
    }
    unsafe { g_hash_table_foreach(table, Some(trampoline::<F>), (&raw mut f).cast()) };
}

fn image_ref(img: *mut NsImage) -> Option<ImageRef<'static>> {
    ptr::NonNull::new(img).map(|p| ImageRef(p, core::marker::PhantomData))
}

fn lookup(cache: &Cache, url: *const c_char) -> *mut NsImage {
    unsafe { glib::g_hash_table_lookup(cache.by_url, url.cast()) }.cast()
}

fn insert(cache: &Cache, url: *const c_char, img: *mut NsImage) {
    unsafe { glib::g_hash_table_insert(cache.by_url, glib::g_strdup(url).cast(), img.cast()) };
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ImageCache(ptr::NonNull<Cache>);

impl ImageCache {
    pub fn create() -> ImageCache {
        // SAFETY: the hash and equality functions match the g_strdup'd keys and the destroy notifiers own the keys and the boxed images.
        let by_url = unsafe {
            glib::g_hash_table_new_full(
                Some(glib::g_str_hash),
                Some(glib::g_str_equal),
                Some(glib::g_free),
                Some(image_free),
            )
        };
        ImageCache(ptr::NonNull::from(Box::leak(Box::new(Cache {
            by_url,
            pending: Vec::new(),
            generation: 0,
            total_bytes: 0,
        }))))
    }

    pub fn raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    /// # Safety
    ///
    /// No copy of this handle, and no pointer `raw()` returned, may be used afterwards.
    pub unsafe fn free(self) {
        // SAFETY: the cache was leaked from a Box by create(), and the caller guarantees nothing uses it again.
        let cache = unsafe { Box::from_raw(self.0.as_ptr()) };
        for &p in &cache.pending {
            // SAFETY: a pending request stays allocated until its callback fires, which marking it dead turns into a no-op.
            unsafe { (*p).dead = true };
        }
        // SAFETY: the table belongs to this cache alone and is destroyed once, here.
        unsafe { g_hash_table_destroy(cache.by_url) };
    }

    fn cache<'a>(self) -> &'a Cache {
        // SAFETY: the cache stays allocated until free(), whose contract rules out later use, and the page only reads its fields here; images are changed through the raw pointers the table holds.
        unsafe { self.0.as_ref() }
    }

    pub fn tick(self, now_us: i64) -> bool {
        let mut any = false;
        for_each_image(self.cache().by_url, |img| {
            // SAFETY: the table owns each image until it is removed, which nothing does during the walk.
            let img = unsafe { &mut *img };
            // SAFETY: a non-null frame array holds anim_frames' AnimFrame values for as long as the image keeps it.
            let anim: &mut [AnimFrame] = unsafe { frames(img.anim_frames) };
            if anim.len() < 2 || img.anim_total_ms <= 0 {
                return;
            }
            let delays: Vec<i32> = anim.iter().map(|f| f.delay_ms).collect();
            let index =
                crate::animation_frame(&delays, img.anim_total_ms, img.anim_start_us, now_us);
            if index as c_int != img.anim_current {
                img.anim_current = index as c_int;
                img.texture = anim[index].texture;
                any = true;
            }
        });
        any
    }

    pub fn animating(self) -> bool {
        let mut any = false;
        for_each_image(self.cache().by_url, |img| {
            // SAFETY: the table owns each image until it is removed, which nothing does during the walk.
            let img = unsafe { &*img };
            // SAFETY: anim_frames is null or the image's live frame array.
            let frames = unsafe { img.anim_frames.as_ref() };
            any |= img.loaded != 0 && frames.is_some_and(|a| a.len > 1);
        });
        any
    }

    pub fn has_pending(self) -> bool {
        !self.cache().pending.is_empty()
    }
}

unsafe fn fire_pending(pending: *mut Pending) {
    let (img, cache) = unsafe { ((*pending).img, (*pending).cache) };
    let mut fire = Vec::new();
    let mut i = 0;
    while i < unsafe { (*cache).pending.len() } {
        let p = unsafe { (&(*cache).pending)[i] };
        if unsafe { (*p).img } == img {
            unsafe { (*cache).pending.swap_remove(i) };
            fire.push(p);
        } else {
            i += 1;
        }
    }
    for p in fire {
        let pending = unsafe { Box::from_raw(p) };
        if let (Some(cb), Some(image)) = (pending.cb, image_ref(img)) {
            cb(image, pending.data);
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
    frames: *mut GArray,
    len: usize,
    w: c_int,
    h: c_int,
    total_ms: c_int,
}

fn anim_frames_from_pixels(source: Vec<decoders::Frame>) -> Option<Anim> {
    if source.is_empty() {
        return None;
    }
    let out = unsafe { g_array_new(FALSE, FALSE, size_of::<AnimFrame>() as c_uint) };
    unsafe { g_array_set_clear_func(out, Some(anim_frame_clear)) };
    let mut ok = true;
    let mut len = 0;
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
        let frame = AnimFrame {
            texture: decoded.texture.into_raw(),
            delay_ms: delay,
        };
        unsafe { g_array_append_vals(out, (&raw const frame).cast(), 1) };
        len += 1;
        total += i64::from(delay);
        if i == 0 {
            w = width;
            h = height;
        }
    }
    if !ok || len == 0 {
        unsafe { g_array_free(out, TRUE) };
        return None;
    }
    Some(Anim {
        frames: out,
        len,
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

fn decode_body(data: *const u8, len: usize) -> Decoded {
    // SAFETY: data is the response body, null or len readable bytes.
    let bytes = unsafe { data_slice(data, len) };
    if let Some(anim) = pixel_frames_for(bytes).and_then(anim_frames_from_pixels) {
        if anim.len > 1 {
            return Decoded {
                tex: ptr::null_mut(),
                frames: anim.frames,
                w: anim.w,
                h: anim.h,
            };
        }
        // SAFETY: anim.frames is owned here and not used after it is freed.
        unsafe { g_array_free(anim.frames, TRUE) };
    }
    let (tex, w, h) = decode_bytes(bytes).map_or((ptr::null_mut(), 0, 0), |(texture, w, h)| {
        (texture.into_raw(), w, h)
    });
    Decoded {
        tex,
        frames: ptr::null_mut(),
        w,
        h,
    }
}

fn set_error(img: &mut NsImage, message: *mut c_char) {
    img.failed = TRUE;
    img.failed_at_us = monotonic_us();
    img.error = message;
}

fn apply_decoded_state(
    img: &mut NsImage,
    d: &Decoded,
    content_type: Option<&[u8]>,
    body_len: usize,
) {
    if !d.frames.is_null() {
        unsafe { g_array_set_clear_func(d.frames, Some(anim_frame_clear)) };
        img.anim_frames = d.frames;
        let anim: &mut [AnimFrame] = unsafe { frames(d.frames) };
        img.texture = anim[0].texture;
        img.natural_width = d.w;
        img.natural_height = d.h;
        img.anim_total_ms = crate::total_delay_ms(anim.iter().map(|f| i64::from(f.delay_ms)));
        img.anim_start_us = monotonic_us();
        img.loaded = TRUE;
    } else if !d.tex.is_null() {
        img.texture = d.tex;
        img.natural_width = d.w;
        img.natural_height = d.h;
        img.loaded = TRUE;
    } else {
        set_error(
            img,
            glib::strdup(&crate::decode_error(content_type, body_len)),
        );
    }
}

unsafe extern "C" fn decoded_free(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<Decoded>()) });
}

unsafe extern "C" fn decode_worker(
    task: *mut c_void,
    _source: *mut c_void,
    task_data: *mut c_void,
    _cancel: *mut c_void,
) {
    let job = unsafe { &*task_data.cast::<DecodeJob>() };
    let mut len = 0usize;
    let data = unsafe { g_bytes_get_data(job.body, &mut len) };
    let decoded = Box::new(decode_body(data, len));
    unsafe { g_task_return_pointer(task, Box::into_raw(decoded).cast(), Some(decoded_free)) };
}

fn free_job(job: DecodeJob) {
    if !job.body.is_null() {
        unsafe { g_bytes_unref(job.body) };
    }
}

unsafe extern "C" fn on_image_decoded(
    _source: *mut c_void,
    result: *mut c_void,
    user_data: *mut c_void,
) {
    let job = unsafe { Box::from_raw(user_data.cast::<DecodeJob>()) };
    let decoded = unsafe { g_task_propagate_pointer(result, ptr::null_mut()) }.cast::<Decoded>();
    let decoded = (!decoded.is_null()).then(|| unsafe { Box::from_raw(decoded) });
    let pending = job.pending;
    if unsafe { (*pending).dead } {
        if let Some(d) = decoded {
            unsafe {
                if !d.frames.is_null() {
                    g_array_set_clear_func(d.frames, Some(anim_frame_clear));
                    g_array_free(d.frames, TRUE);
                }
                TextureRef::release(d.tex);
            }
        }
        drop(unsafe { Box::from_raw(pending) });
        free_job(*job);
        return;
    }
    let result = decoded.map_or(
        Decoded {
            tex: ptr::null_mut(),
            frames: ptr::null_mut(),
            w: 0,
            h: 0,
        },
        |d| *d,
    );
    unsafe {
        let img = &mut *(*pending).img;
        apply_decoded_state(img, &result, Some(&job.content_type), job.body_len);
        cache_account((*pending).cache, (*pending).img);
        fire_pending(pending);
    }
    free_job(*job);
}

fn record_response(img: &mut NsImage, resp: &Response) {
    let dup = |value: Option<&CStr>| value.map_or(ptr::null_mut(), |v| glib::strdup(v.to_bytes()));
    img.http_status = resp.status();
    gfree(img.final_url);
    img.final_url = dup(resp.final_url());
    gfree(img.cors_allow_origin);
    img.cors_allow_origin = dup(resp.cors_allow_origin());
    gfree(img.next_hop_protocol);
    img.next_hop_protocol = dup(resp.next_hop_protocol());
    gfree(img.timing_allow_origin);
    img.timing_allow_origin = resp
        .raw_headers()
        .and_then(|raw| southstar_net::raw_header_values(raw.to_bytes(), b"timing-allow-origin"))
        .map_or(ptr::null_mut(), |values| glib::strdup(&values));
    img.body_size = resp.body().map_or(0, |b| b.len() as i64);
}

unsafe fn finish_failed(pending: *mut Pending, message: &[u8]) {
    unsafe {
        let img = &mut *(*pending).img;
        let message = if message.is_empty() {
            &b"fetch failed"[..]
        } else {
            message
        };
        set_error(img, glib::strdup(message));
        if let (Some(cb), Some(image)) = ((*pending).cb, image_ref((*pending).img)) {
            cb(image, (*pending).data);
        }
        let list = &mut (*(*pending).cache).pending;
        if let Some(index) = list.iter().position(|&p| p == pending) {
            list.swap_remove(index);
        }
        drop(Box::from_raw(pending));
    }
}

unsafe fn decode_in_thread(pending: *mut Pending, body: &[u8], content_type: &[u8]) {
    let job = Box::new(DecodeJob {
        pending,
        // SAFETY: body is a readable slice that GLib copies.
        body: unsafe { g_bytes_new(body.as_ptr().cast(), body.len()) },
        content_type: content_type.to_vec(),
        body_len: body.len(),
    });
    let job = Box::into_raw(job).cast::<c_void>();
    unsafe {
        let task = g_task_new(
            ptr::null_mut(),
            ptr::null_mut(),
            Some(on_image_decoded),
            job,
        );
        g_task_set_task_data(task, job, None);
        g_task_run_in_thread(task, Some(decode_worker));
        g_object_unref(task);
    }
}

unsafe fn image_fetched(pending: *mut Pending, outcome: southstar_net::Outcome) {
    unsafe {
        if (*pending).dead {
            drop(Box::from_raw(pending));
            return;
        }
        (*(*pending).img).response_us = monotonic_us();
        let response = match outcome {
            Ok(response) => response,
            Err(message) => return finish_failed(pending, &message),
        };
        let img = &mut *(*pending).img;
        record_response(img, &response);
        let body = response.body().filter(|b| !b.is_empty());
        if let Some(error) = response.error() {
            set_error(img, glib::strdup(error.to_bytes()));
        } else if response.status() >= 400 {
            set_error(
                img,
                glib::strdup(format!("HTTP {}", response.status()).as_bytes()),
            );
        } else if let Some(body) = body {
            let content_type = response.content_type().map(CStr::to_bytes);
            if southstar_config::get().is_some_and(|c| c.async_image_decode != 0) {
                return decode_in_thread(pending, body, content_type.unwrap_or_default());
            }
            let decoded = decode_body(body.as_ptr(), body.len());
            apply_decoded_state(img, &decoded, content_type, body.len());
        } else {
            set_error(img, glib::strdup(b"empty response"));
        }
        drop(response);
        cache_account((*pending).cache, (*pending).img);
        fire_pending(pending);
    }
}

fn retry_state(img: &NsImage) -> RetryState {
    RetryState {
        failed: img.failed != 0,
        attempts: img.attempts,
        http_status: img.http_status,
        failed_at_us: img.failed_at_us,
    }
}

unsafe fn start_request(
    cache: *mut Cache,
    img: *mut NsImage,
    url: &CStr,
    top_url: Option<&CStr>,
    cb: Option<ImageReady>,
    data: usize,
) {
    let pending = Box::into_raw(Box::new(Pending {
        img,
        cache,
        cb,
        data,
        dead: false,
    }));
    unsafe {
        (*cache).pending.push(pending);
        (*img).attempts += 1;
        (*img).request_us = monotonic_us();
        (*img).response_us = 0;
    }
    let headers = southstar_net::FetchDest::Image.header_lines();
    let request = southstar_net::FetchRequest {
        url: url.to_bytes(),
        top_url: top_url.map(CStr::to_bytes),
        method: Some(b"GET"),
        headers: &headers,
        ..southstar_net::FetchRequest::default()
    };
    southstar_net::request_with(&request, None, move |outcome| {
        // SAFETY: the pending request stays allocated until this completion, which reclaims it.
        unsafe { image_fetched(pending, outcome) }
    });
}

fn images_disabled() -> bool {
    southstar_config::get().is_some_and(|c| c.images_enabled == 0)
}

unsafe fn cache_get(
    cache: *mut Cache,
    url: &CStr,
    top_url: Option<&CStr>,
    cb: Option<ImageReady>,
    data: usize,
) -> *mut NsImage {
    unsafe {
        let cached = lookup(&*cache, url.as_ptr());
        if let Some(img) = cached.as_mut() {
            img.generation = (*cache).generation;
            if images_disabled() {
                if let Some(cb) = cb
                    && (img.loaded != 0 || img.failed != 0)
                    && let Some(image) = image_ref(cached)
                {
                    cb(image, data);
                }
                return cached;
            }
            if img.purged != 0 && !has_pending(&*cache, cached) {
                img.purged = FALSE;
                start_request(cache, cached, url, top_url, cb, data);
                return cached;
            }
            if crate::should_retry(&retry_state(img), monotonic_us()) {
                img.failed = FALSE;
                img.http_status = 0;
                img.failed_at_us = 0;
                gfree(img.error);
                img.error = ptr::null_mut();
                start_request(cache, cached, url, top_url, cb, data);
                return cached;
            }
            if img.loaded != 0 || img.failed != 0 {
                if let (Some(cb), Some(image)) = (cb, image_ref(cached)) {
                    cb(image, data);
                }
            } else if let Some(cb) = cb {
                let duplicate = (*cache).pending.iter().any(|&p| {
                    let p = &*p;
                    !p.dead
                        && p.img == cached
                        && p.cb.is_some_and(|f| ptr::fn_addr_eq(f, cb))
                        && p.data == data
                });
                if !duplicate {
                    let pending = Box::into_raw(Box::new(Pending {
                        img: cached,
                        cache,
                        cb: Some(cb),
                        data,
                        dead: false,
                    }));
                    (*cache).pending.push(pending);
                }
            }
            return cached;
        }
        let img = new_image(url.as_ptr());
        insert(&*cache, url.as_ptr(), img);
        if images_disabled() {
            (*img).failed = TRUE;
            if let (Some(cb), Some(image)) = (cb, image_ref(img)) {
                cb(image, data);
            }
            return img;
        }
        start_request(cache, img, url, top_url, cb, data);
        img
    }
}

unsafe fn insert_loaded(
    cache: *mut Cache,
    url: *const c_char,
    texture: TextureRef,
    width: c_int,
    height: c_int,
) -> *mut NsImage {
    unsafe {
        let existing = lookup(&*cache, url);
        if !existing.is_null() {
            return existing;
        }
        let img = new_image(url);
        (*img).texture = texture.into_raw();
        (*img).natural_width = width;
        (*img).natural_height = height;
        (*img).loaded = TRUE;
        insert(&*cache, url, img);
        cache_account(cache, img);
        img
    }
}

unsafe fn insert_encoded(cache: *mut Cache, url: *const c_char, bytes: &[u8]) -> *mut NsImage {
    if bytes.is_empty() {
        return ptr::null_mut();
    }
    unsafe {
        let existing = lookup(&*cache, url);
        if !existing.is_null() {
            return existing;
        }
        let anim = pixel_frames_for(bytes)
            .filter(|frames| frames.len() > 1)
            .and_then(anim_frames_from_pixels);
        if let Some(anim) = anim {
            if anim.len > 1 {
                let img = new_image(url);
                (*img).anim_frames = anim.frames;
                (*img).texture = frames::<AnimFrame>(anim.frames)[0].texture;
                (*img).natural_width = anim.w;
                (*img).natural_height = anim.h;
                (*img).anim_total_ms = anim.total_ms;
                (*img).anim_start_us = monotonic_us();
                (*img).loaded = TRUE;
                insert(&*cache, url, img);
                cache_account(cache, img);
                return img;
            }
            g_array_free(anim.frames, TRUE);
        }
        match decode_bytes(bytes) {
            Some((texture, w, h)) => insert_loaded(cache, url, texture, w, h),
            None => ptr::null_mut(),
        }
    }
}

impl ImageCache {
    /// # Safety
    ///
    /// `raw` is null or a pointer `raw()` returned for a cache that has not been freed.
    pub unsafe fn from_raw(raw: *mut c_void) -> Option<ImageCache> {
        ptr::NonNull::new(raw.cast()).map(ImageCache)
    }

    pub fn get(
        self,
        url: &CStr,
        top_url: Option<&CStr>,
        ready: Option<ImageReady>,
        data: usize,
    ) -> Option<ImageRef<'static>> {
        // SAFETY: the cache is live for as long as its handle may be used.
        image_ref(unsafe { cache_get(self.0.as_ptr(), url, top_url, ready, data) })
    }

    pub fn cancel_ready(self, data: usize) {
        for &p in &self.cache().pending {
            // SAFETY: a pending request stays allocated while it is listed in the cache.
            let p = unsafe { &mut *p };
            if p.data == data {
                p.cb = None;
                p.data = 0;
            }
        }
    }

    pub fn peek(self, url: &CStr) -> Option<ImageRef<'static>> {
        let cache = self.cache();
        let image = image_ref(lookup(cache, url.as_ptr()))?;
        // SAFETY: the table owns the image until it is removed, which nothing does during the call.
        unsafe { (*image.0.as_ptr()).generation = cache.generation };
        Some(image)
    }

    pub fn insert_encoded(self, url: &CStr, bytes: &[u8]) -> Option<ImageRef<'static>> {
        // SAFETY: the cache is live for as long as its handle may be used, and url is NUL-terminated.
        image_ref(unsafe { insert_encoded(self.0.as_ptr(), url.as_ptr(), bytes) })
    }

    pub fn begin_generation(self) {
        // SAFETY: the cache is live for as long as its handle may be used, and nothing else borrows it during the call.
        let cache = unsafe { &mut *self.0.as_ptr() };
        cache.generation = cache.generation.wrapping_add(1);
    }

    pub fn collect(self) {
        // SAFETY: the cache is live for as long as its handle may be used, and nothing else borrows it during the call.
        let cache = unsafe { &mut *self.0.as_ptr() };
        collect(cache);
    }
}

fn collect(cache: &mut Cache) {
    while cache.total_bytes > CACHE_BUDGET_BYTES {
        let mut victim: *mut NsImage = ptr::null_mut();
        let mut oldest = u64::MAX;
        for_each_image(cache.by_url, |img| {
            let image = unsafe { &*img };
            if image.loaded == 0
                || image.bytes == 0
                || image.generation == cache.generation
                || has_pending(cache, img)
            {
                return;
            }
            if image.generation < oldest {
                oldest = image.generation;
                victim = img;
            }
        });
        let Some(victim) = (unsafe { victim.as_mut() }) else {
            break;
        };
        purge(cache, victim);
    }
}
