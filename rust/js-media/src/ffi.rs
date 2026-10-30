//! Southstar — the Rust-ABI entry points and embedder hook setters of the media bindings, and the realm, event, video and microphone calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_uint};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::hooks;
use crate::{audio, element, eme, mse, support, tracks};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<Page>);

impl Js {
    fn from_ptr(js: Option<Page>) -> Js {
        Js(js)
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }
}

pub type Element = Node<'static>;

pub type AudioFn = dyn Fn(&CStr);
pub type SeekFn = dyn Fn(Element, f64) -> bool;
pub type ToggleFn = dyn Fn(Element, bool);
pub type VolumeFn = dyn Fn(Element, f64);
pub type MseFn = dyn Fn(u32, u8, Option<&[u8]>) -> bool;
pub type MseBufferedFn = dyn Fn(u32, u8) -> (f64, f64);
pub type MseRemoveFn = dyn Fn(u32, u8, f64, f64) -> bool;
pub type MseBytesFn = dyn Fn(u32, u8) -> usize;

const FORMAT_LIBAV: c_uint = 1;
const FORMAT_VORBIS: c_uint = 2;
const FORMAT_OPUS: c_uint = 4;

use southstar_js_events::nsr_event_adopt_interface;
use southstar_js_events::nsr_event_new;

use southstar_js_dispatch::nsr_target_dispatchEvent;

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

pub(crate) fn c_text(text: &str) -> CString {
    CString::new(until_nul(text.as_bytes())).unwrap_or_default()
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
    Js(Page::current(scope))
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

pub(crate) fn helper_may_fetch(js: Js, element: Element, url: &str) -> bool {
    let page = api::node_frame_url(element).or_else(|| current_url(js).map(|page| c_text(&page)));
    let mixed = page
        .as_deref()
        .is_some_and(|page| page.to_bytes().starts_with(b"https://"))
        && url.starts_with("http://");
    !mixed && api::csp_allows_media(js.ptr(), Some(element), &c_text(url), page.as_deref())
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
        // SAFETY: bytes_view points at view.len bytes of a buffer that view.holder keeps alive until it is dropped after f returns.
        unsafe { core::slice::from_raw_parts(view.data, view.len) }
    };
    let result = f(bytes);
    drop(view.holder);
    Some(result)
}

macro_rules! methods {
    ($($name:ident => $f:path;)*) => {
        $(
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
            pub fn $name(js: Option<Page>, hook: Option<Rc<$ty>>) {
                hooks::update(Js::from_ptr(js), |hooks| hooks.$field = hook);
            }
        )*
    };
}

methods! {
    nsr_eme_request_access => eme::request_access;
    nsr_media_source_is_type_supported => support::is_type_supported;
    nsr_media_capabilities_info => support::decoding_info;
    nsr_window_mse_append => mse::append;
    nsr_window_mse_eos => mse::end_of_stream;
    nsr_window_mse_buffered => mse::buffered;
    nsr_window_mse_buffered_start => mse::buffered_start;
    nsr_window_mse_remove => mse::remove;
    nsr_window_mse_bytes => mse::bytes;
    nsr_vtt_cue_ctor => tracks::vtt_cue;
}

getters! {}

setters! {}

hook_setters! {
    set_audio_hook: AudioFn => audio;
    set_seek_hook: SeekFn => seek;
    set_play_hook: ToggleFn => play;
    set_muted_hook: ToggleFn => muted;
    set_volume_hook: VolumeFn => volume;
    set_mse_hook: MseFn => mse;
    set_mse_buffered_hook: MseBufferedFn => mse_buffered;
    set_mse_remove_hook: MseRemoveFn => mse_remove;
    set_mse_bytes_hook: MseBytesFn => mse_bytes;
}

pub fn video_event(js: Option<Page>, node: Node<'_>, kind: &CStr, value: f64) {
    let js = Js::from_ptr(js);
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

fn init(js: Page) {
    hooks::init(Js::from_ptr(Some(js)));
}

fn teardown(js: Page) {
    hooks::teardown(Js::from_ptr(Some(js)));
}

pub fn install_runtime_hooks() {
    api::set_media_hooks(api::PageStateHooks { init, teardown });
    southstar_js_runtime::links::set_js_media_links(southstar_js_runtime::links::JsMediaLinks {
        vtt_cue_ctor: crate::ffi::nsr_vtt_cue_ctor,
        media_source_is_type_supported: crate::ffi::nsr_media_source_is_type_supported,
        window_mse_bytes: crate::ffi::nsr_window_mse_bytes,
        window_mse_remove: crate::ffi::nsr_window_mse_remove,
        window_mse_buffered_start: crate::ffi::nsr_window_mse_buffered_start,
        window_mse_buffered: crate::ffi::nsr_window_mse_buffered,
        window_mse_eos: crate::ffi::nsr_window_mse_eos,
        window_mse_append: crate::ffi::nsr_window_mse_append,
        media_install_audio: crate::ffi::nsr_media_install_audio,
    });
    southstar_js_services::links::set_js_media_links(southstar_js_services::links::JsMediaLinks {
        media_capabilities_info: crate::ffi::nsr_media_capabilities_info,
        eme_request_access: crate::ffi::nsr_eme_request_access,
    });
}
