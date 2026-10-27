//! Southstar — the C and Rust-ABI entry points of the media bindings, and the realm, event, video and microphone calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::GBoolean;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::hooks::{self, Hook};
use crate::{audio, element, eme, mse, support, tracks};

pub(crate) use southstar_js_runtime::api::NsJs;

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

use southstar_js_events::nsr_event_adopt_interface;
use southstar_js_events::nsr_event_new;

unsafe extern "Rust" {
    #[allow(non_snake_case)]
    safe fn nsr_target_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn c_text(text: &str) -> CString {
    CString::new(until_nul(text.as_bytes())).unwrap_or_default()
}

fn data_ptr(data: usize) -> *mut c_void {
    data as *mut c_void
}

fn node_ptr(element: Element) -> *const c_void {
    element.as_ptr().cast()
}

pub(crate) fn bind_listeners(scope: &mut Scope<'_>, object: &Value) {
    api::bind_event_target_listeners(scope, object);
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    bind_listeners(scope, object);
    let dispatch = scope.function("dispatchEvent", 1, nsr_target_dispatchEvent);
    crate::set(scope, object, "dispatchEvent", dispatch);
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    nsr_event_new(scope).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    nsr_event_adopt_interface(scope, event, iface);
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(scope.context_opaque().cast())
}

pub(crate) fn element(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, element: Element) -> Value {
    api::make_element(scope, element.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn dispatch(js: Js, element: Element, kind: &str) {
    southstar_js_dispatch::dispatch_event(js.ptr(), element, kind);
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.ptr());
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    let url = api::current_url(js.ptr());
    (!url.is_empty()).then(|| url.to_string_lossy().into_owned())
}

pub(crate) fn url_resolve(base: &str, href: &str) -> Option<String> {
    let resolved = southstar_url::bytes::resolve(
        Some(until_nul(base.as_bytes())),
        until_nul(href.as_bytes()),
    )?;
    Some(String::from_utf8_lossy(until_nul(&resolved)).into_owned())
}

pub(crate) fn video_url_is_inline(url: &str) -> bool {
    southstar_video::url_is_inline_video(until_nul(url.as_bytes()))
}

pub(crate) fn video_codec_available(codec: &str) -> bool {
    southstar_video::codec_available(until_nul(codec.as_bytes()))
}

pub(crate) fn perf_now(scope: &Scope<'_>) -> f64 {
    southstar_js_perf::context_now_ms(scope.handle())
}

#[derive(Clone, Copy)]
pub(crate) struct NativeFormats {
    pub libav: bool,
    pub vorbis: bool,
    pub opus: bool,
}

pub(crate) fn native_formats() -> NativeFormats {
    let bits = api::media_native_formats();
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
    let view = api::bytes_view(scope, value)?;
    let bytes = if view.data.is_null() || view.len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(view.data, view.len) }
    };
    let result = f(bytes);
    drop(view.holder);
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
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

macro_rules! getters {
    ($($name:ident => $f:path;)*) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
                $f(scope, this, &[])
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path;)*) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> Result<(), Value> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
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
    nsr_eme_request_access => eme::request_access;
    nsr_media_set_media_keys => eme::set_media_keys;
    nsr_media_canPlayType => support::can_play_type;
    nsr_media_source_is_type_supported => support::is_type_supported;
    nsr_media_capabilities_info => support::decoding_info;
    nsr_media_play => element::play;
    nsr_media_pause => element::pause;
    nsr_media_load => element::load;
    nsr_media_fast_seek => element::fast_seek;
    nsr_media_get_video_playback_quality => element::video_playback_quality;
    nsr_window_mse_append => mse::append;
    nsr_window_mse_eos => mse::end_of_stream;
    nsr_window_mse_buffered => mse::buffered;
    nsr_window_mse_buffered_start => mse::buffered_start;
    nsr_window_mse_remove => mse::remove;
    nsr_window_mse_bytes => mse::bytes;
    nsr_vtt_cue_ctor => tracks::vtt_cue;
    nsr_media_addTextTrack => tracks::add_text_track;
}

getters! {
    nsr_media_get_paused => element::paused;
    nsr_media_get_ended => element::ended;
    nsr_media_get_seeking => element::seeking;
    nsr_media_get_readyState => element::ready_state;
    nsr_media_get_networkState => element::network_state;
    nsr_media_get_current_time => element::current_time;
    nsr_media_get_duration => element::duration;
    nsr_media_get_error => element::error;
    nsr_media_get_seekable_ranges => element::seekable;
    nsr_media_get_buffered_ranges => element::buffered;
    nsr_media_get_played_ranges => element::played;
    nsr_media_get_playbackRate => element::playback_rate;
    nsr_media_get_defaultPlaybackRate => element::default_playback_rate;
    nsr_media_get_volume => element::volume;
    nsr_media_get_muted => element::muted;
    nsr_media_get_srcObject => element::src_object;
    nsr_media_get_textTracks => tracks::text_tracks;
}

setters! {
    nsr_media_set_current_time => element::set_current_time;
    nsr_media_set_playbackRate => element::set_playback_rate;
    nsr_media_set_defaultPlaybackRate => element::set_default_playback_rate;
    nsr_media_set_volume => element::set_volume;
    nsr_media_set_muted => element::set_muted;
    nsr_media_set_srcObject => element::set_src_object;
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

pub fn video_event(js: *const NsJs, node: Node<'_>, kind: &CStr, value: f64) {
    let js = Js::from_ptr(js.cast_mut());
    let Some(node) = node.downgrade().get() else {
        return;
    };
    if js.is_null() {
        return;
    }
    let handle = api::main_context(js.ptr());
    if handle.is_null() {
        return;
    }
    let kind = kind.to_string_lossy();
    handle.with(|scope| element::video_event(scope, js, node, &kind, value));
}

pub fn nsr_media_install_audio(scope: &mut Scope<'_>, global: &Value) {
    audio::install(scope, global);
}

fn init(js: *const NsJs) {
    hooks::init(Js::from_ptr(js.cast_mut()));
}

fn teardown(js: *const NsJs) {
    hooks::teardown(Js::from_ptr(js.cast_mut()));
}

pub fn install_runtime_hooks() {
    api::set_media_hooks(api::PageStateHooks { init, teardown });
    southstar_js_runtime::links::set_js_media_links(southstar_js_runtime::links::JsMediaLinks {
        media_install_audio: crate::ffi::nsr_media_install_audio,
    });
}
