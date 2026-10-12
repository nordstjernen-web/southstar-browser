//! Southstar — the C ABI of the media bindings as declared in src/js_internal.h and src/js.h, and the js.c, video and microphone calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GStr};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::hooks::{self, Hook};
use crate::{audio, element, eme, mse, support, tracks};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn from_ptr(js: *mut NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

pub(crate) type Element = Node<'static>;

pub(crate) type AudioFn = unsafe extern "C" fn(command: *const c_char, data: *mut c_void);
pub(crate) type SeekFn =
    unsafe extern "C" fn(node: *const c_void, seconds: f64, data: *mut c_void) -> GBoolean;
pub(crate) type ToggleFn =
    unsafe extern "C" fn(node: *const c_void, on: GBoolean, data: *mut c_void);
pub(crate) type VolumeFn =
    unsafe extern "C" fn(node: *const c_void, volume: f64, data: *mut c_void);
pub(crate) type MseFn = unsafe extern "C" fn(
    stream: c_uint,
    kind: c_char,
    bytes: *const u8,
    len: usize,
    eos: GBoolean,
    data: *mut c_void,
) -> GBoolean;
pub(crate) type MseBufferedFn =
    unsafe extern "C" fn(stream: c_uint, kind: c_char, start: *mut f64, data: *mut c_void) -> f64;
pub(crate) type MseRemoveFn = unsafe extern "C" fn(
    stream: c_uint,
    kind: c_char,
    start: f64,
    end: f64,
    data: *mut c_void,
) -> GBoolean;
pub(crate) type MseBytesFn =
    unsafe extern "C" fn(stream: c_uint, kind: c_char, data: *mut c_void) -> usize;

const FORMAT_LIBAV: c_uint = 1;
const FORMAT_VORBIS: c_uint = 2;
const FORMAT_OPUS: c_uint = 4;

unsafe extern "C" {
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_adopt_interface(ctx: *mut JSContext, event: JSValue, iface: *const c_char);
    fn ns_mic_fill_time_domain(out: *mut u8, n: c_int);
    fn ns_mic_fill_frequency(out: *mut u8, n: c_int);
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_video_url_is_inline(url: *const c_char) -> GBoolean;
    fn ns_video_codec_available(codec: *const c_char) -> GBoolean;
    fn ns_perf_realm_now_ms(ctx: *mut JSContext) -> f64;
    fn ns_media_native_formats() -> c_uint;
    fn ns_js_bytes_view(
        ctx: *mut JSContext,
        value: JSValue,
        out_data: *mut *const u8,
        out_len: *mut usize,
        out_holder: *mut JSValue,
    ) -> GBoolean;
}

fn ctx_of(scope: &Scope<'_>) -> *mut JSContext {
    quickjs::raw_context(scope)
}

fn c_text(text: &str) -> CString {
    let bytes = text.as_bytes();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn data_ptr(data: usize) -> *mut c_void {
    data as *mut c_void
}

fn node_ptr(element: Element) -> *const c_void {
    element.as_ptr().cast()
}

pub(crate) fn bind_listeners(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(ctx_of(scope), quickjs::raw(object)) };
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    bind_listeners(scope, object);
    let dispatch = quickjs::c_function(scope, "dispatchEvent", 1, ns_target_dispatchEvent);
    crate::set(scope, object, "dispatchEvent", dispatch);
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(ctx_of(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    let iface = c_text(iface);
    unsafe { ns_event_adopt_interface(ctx_of(scope), quickjs::raw(event), iface.as_ptr()) };
}

fn clamp_len(bytes: &[u8]) -> c_int {
    c_int::try_from(bytes.len()).unwrap_or(c_int::MAX)
}

pub(crate) fn mic_time_domain(bytes: &mut [u8]) {
    let n = clamp_len(bytes);
    unsafe { ns_mic_fill_time_domain(bytes.as_mut_ptr(), n) };
}

pub(crate) fn mic_frequency(bytes: &mut [u8]) {
    let n = clamp_len(bytes);
    unsafe { ns_mic_fill_frequency(bytes.as_mut_ptr(), n) };
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(quickjs::context_opaque(scope).cast())
}

pub(crate) fn element(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap(scope: &mut Scope<'_>, element: Element) -> Value {
    let raw = unsafe { ns_make_element(ctx_of(scope), element.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dispatch(js: Js, element: Element, kind: &str) {
    let kind = c_text(kind);
    unsafe { ns_js_dispatch_event(js.ptr(), element.as_ptr(), kind.as_ptr(), ptr::null_mut()) };
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.ptr()) };
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    let url = unsafe { ns_js_current_url(js.ptr()) };
    if url.is_null() {
        return None;
    }
    let url = unsafe { CStr::from_ptr(url) };
    (!url.is_empty()).then(|| url.to_string_lossy().into_owned())
}

pub(crate) fn url_resolve(base: &str, href: &str) -> Option<String> {
    let (base, href) = (c_text(base), c_text(href));
    let resolved = unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) };
    unsafe { GStr::take(resolved) }.map(|url| url.to_string_lossy().into_owned())
}

pub(crate) fn video_url_is_inline(url: &str) -> bool {
    let url = c_text(url);
    unsafe { ns_video_url_is_inline(url.as_ptr()) != 0 }
}

pub(crate) fn video_codec_available(codec: &str) -> bool {
    let codec = c_text(codec);
    unsafe { ns_video_codec_available(codec.as_ptr()) != 0 }
}

pub(crate) fn perf_now(scope: &Scope<'_>) -> f64 {
    unsafe { ns_perf_realm_now_ms(ctx_of(scope)) }
}

#[derive(Clone, Copy)]
pub(crate) struct NativeFormats {
    pub libav: bool,
    pub vorbis: bool,
    pub opus: bool,
}

pub(crate) fn native_formats() -> NativeFormats {
    let bits = unsafe { ns_media_native_formats() };
    NativeFormats {
        libav: bits & FORMAT_LIBAV != 0,
        vorbis: bits & FORMAT_VORBIS != 0,
        opus: bits & FORMAT_OPUS != 0,
    }
}

pub(crate) fn with_bytes<R>(
    scope: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    let (mut data, mut len) = (ptr::null::<u8>(), 0usize);
    let mut holder = quickjs::UNDEFINED;
    let ok = unsafe {
        ns_js_bytes_view(
            ctx_of(scope),
            quickjs::raw(value),
            &mut data,
            &mut len,
            &mut holder,
        )
    };
    let holder = unsafe { quickjs::take_value(scope, holder) };
    if ok == 0 {
        return None;
    }
    let bytes = if data.is_null() || len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }
    };
    let result = f(bytes);
    drop(holder);
    Some(result)
}

pub(crate) fn call_audio(hook: Hook<AudioFn>, command: &str) {
    let command = c_text(command);
    unsafe { (hook.f)(command.as_ptr(), data_ptr(hook.data)) };
}

pub(crate) fn call_toggle(hook: Hook<ToggleFn>, element: Element, on: bool) {
    unsafe { (hook.f)(node_ptr(element), GBoolean::from(on), data_ptr(hook.data)) };
}

pub(crate) fn call_volume(hook: Hook<VolumeFn>, element: Element, volume: f64) {
    unsafe { (hook.f)(node_ptr(element), volume, data_ptr(hook.data)) };
}

pub(crate) fn call_seek(hook: Hook<SeekFn>, element: Element, seconds: f64) -> bool {
    unsafe { (hook.f)(node_ptr(element), seconds, data_ptr(hook.data)) != 0 }
}

pub(crate) fn call_mse(hook: Hook<MseFn>, stream: u32, kind: u8, bytes: Option<&[u8]>) -> bool {
    let (data, len, eos) = match bytes {
        Some(bytes) => (bytes.as_ptr(), bytes.len(), false),
        None => (ptr::null(), 0, true),
    };
    unsafe {
        (hook.f)(
            stream,
            kind as c_char,
            data,
            len,
            GBoolean::from(eos),
            data_ptr(hook.data),
        ) != 0
    }
}

pub(crate) fn call_mse_buffered(hook: Hook<MseBufferedFn>, stream: u32, kind: u8) -> (f64, f64) {
    let mut start = 0.0;
    let end = unsafe { (hook.f)(stream, kind as c_char, &mut start, data_ptr(hook.data)) };
    (start, end)
}

pub(crate) fn call_mse_remove(
    hook: Hook<MseRemoveFn>,
    stream: u32,
    kind: u8,
    start: f64,
    end: f64,
) -> bool {
    unsafe { (hook.f)(stream, kind as c_char, start, end, data_ptr(hook.data)) != 0 }
}

pub(crate) fn call_mse_bytes(hook: Hook<MseBytesFn>, stream: u32, kind: u8) -> usize {
    unsafe { (hook.f)(stream, kind as c_char, data_ptr(hook.data)) }
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn getter(ctx: *mut JSContext, this_val: JSValue, f: NativeFn) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

unsafe fn setter(ctx: *mut JSContext, this_val: JSValue, value: JSValue, f: NativeFn) -> JSValue {
    let mut args = [value];
    unsafe { quickjs::call_native(ctx, this_val, 1, args.as_mut_ptr(), f) }
}

fn hook<F: Copy>(f: Option<F>, data: *mut c_void) -> Option<Hook<F>> {
    f.map(|f| Hook {
        f,
        data: data as usize,
    })
}

macro_rules! methods {
    ($($name:ident => $f:path;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

macro_rules! getters {
    ($($name:ident => $f:path;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { getter(ctx, this_val, $f) }
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                value: JSValue,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, value, $f) }
            }
        )*
    };
}

macro_rules! hook_setters {
    ($($name:ident: $ty:ty => $field:ident;)*) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(js: *mut NsJs, cb: Option<$ty>, data: *mut c_void) {
                hooks::update(Js::from_ptr(js), |hooks| hooks.$field = hook(cb, data));
            }
        )*
    };
}

methods! {
    ns_eme_request_access => eme::request_access;
    ns_media_set_media_keys => eme::set_media_keys;
    ns_media_canPlayType => support::can_play_type;
    ns_media_source_is_type_supported => support::is_type_supported;
    ns_media_capabilities_info => support::decoding_info;
    ns_media_play => element::play;
    ns_media_pause => element::pause;
    ns_media_load => element::load;
    ns_media_fast_seek => element::fast_seek;
    ns_media_get_video_playback_quality => element::video_playback_quality;
    ns_window_mse_append => mse::append;
    ns_window_mse_eos => mse::end_of_stream;
    ns_window_mse_buffered => mse::buffered;
    ns_window_mse_buffered_start => mse::buffered_start;
    ns_window_mse_remove => mse::remove;
    ns_window_mse_bytes => mse::bytes;
    ns_vtt_cue_ctor => tracks::vtt_cue;
    ns_media_addTextTrack => tracks::add_text_track;
}

getters! {
    ns_media_get_paused => element::paused;
    ns_media_get_ended => element::ended;
    ns_media_get_seeking => element::seeking;
    ns_media_get_readyState => element::ready_state;
    ns_media_get_networkState => element::network_state;
    ns_media_get_current_time => element::current_time;
    ns_media_get_duration => element::duration;
    ns_media_get_error => element::error;
    ns_media_get_seekable_ranges => element::seekable;
    ns_media_get_buffered_ranges => element::buffered;
    ns_media_get_played_ranges => element::played;
    ns_media_get_playbackRate => element::playback_rate;
    ns_media_get_defaultPlaybackRate => element::default_playback_rate;
    ns_media_get_volume => element::volume;
    ns_media_get_muted => element::muted;
    ns_media_get_srcObject => element::src_object;
    ns_media_get_textTracks => tracks::text_tracks;
}

setters! {
    ns_media_set_current_time => element::set_current_time;
    ns_media_set_playbackRate => element::set_playback_rate;
    ns_media_set_defaultPlaybackRate => element::set_default_playback_rate;
    ns_media_set_volume => element::set_volume;
    ns_media_set_muted => element::set_muted;
    ns_media_set_srcObject => element::set_src_object;
}

hook_setters! {
    ns_js_set_audio_cb: AudioFn => audio;
    ns_js_set_media_seek_cb: SeekFn => seek;
    ns_js_set_media_play_cb: ToggleFn => play;
    ns_js_set_media_muted_cb: ToggleFn => muted;
    ns_js_set_media_volume_cb: VolumeFn => volume;
    ns_js_set_mse_cb: MseFn => mse;
    ns_js_set_mse_buffered_cb: MseBufferedFn => mse_buffered;
    ns_js_set_mse_remove_cb: MseRemoveFn => mse_remove;
    ns_js_set_mse_bytes_cb: MseBytesFn => mse_bytes;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_media_init(js: *mut NsJs) {
    hooks::init(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_media_teardown(js: *mut NsJs) {
    hooks::teardown(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_video_event(
    js: *mut NsJs,
    node: *const c_void,
    kind: *const c_char,
    value: f64,
) {
    let js = Js::from_ptr(js);
    if js.is_null() || kind.is_null() {
        return;
    }
    let Some(node) = (unsafe { Node::from_ptr(node.cast()) }) else {
        return;
    };
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return;
    }
    let kind = unsafe { CStr::from_ptr(kind) }.to_string_lossy();
    unsafe {
        quickjs::with_context(ctx, |scope| {
            element::video_event(scope, js, node, &kind, value)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_media_install_audio(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            audio::install(scope, &global);
        })
    }
}
