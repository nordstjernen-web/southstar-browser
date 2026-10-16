//! Southstar — the C ABI and Rust-ABI entry points of the URL, codec, object-URL, blob: URL registry and FileReader bindings, and the realm and net calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_bind::host::{self, HostKind};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_url::bytes::Parts;

use crate::JsResult;

pub(crate) const HO_FILE_READER: HostKind = HostKind::FileReader;
pub(crate) const HO_TEXT_ENCODER: HostKind = HostKind::TextEncoder;
pub(crate) const HO_TEXT_DECODER: HostKind = HostKind::TextDecoder;

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    pub(crate) fn of(scope: &Scope<'_>) -> Js {
        Js(scope.context_opaque() as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub(crate) fn is_null(self) -> bool {
        self.0 == 0
    }
}

#[repr(C)]
pub(crate) struct GBytes {
    _private: [u8; 0],
}

pub(crate) struct SharedBytes(*mut GBytes);

unsafe impl Send for SharedBytes {}

impl SharedBytes {
    pub(crate) fn new(bytes: &[u8]) -> SharedBytes {
        SharedBytes(unsafe { g_bytes_new(bytes.as_ptr().cast(), bytes.len()) })
    }
}

impl Drop for SharedBytes {
    fn drop(&mut self) {
        unsafe { g_bytes_unref(self.0) };
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Source(c_uint);

type BlobResolver = Option<
    unsafe extern "C" fn(
        url: *const c_char,
        out_type: *mut *mut c_char,
        user_data: *mut c_void,
    ) -> *mut GBytes,
>;

unsafe extern "C" {
    fn ns_net_set_blob_resolver(resolver: BlobResolver, user_data: *mut c_void);
    fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    fn g_bytes_ref(bytes: *mut GBytes) -> *mut GBytes;
    fn g_bytes_unref(bytes: *mut GBytes);
    fn g_uuid_string_random() -> *mut c_char;
}

unsafe extern "Rust" {
    safe fn nsr_xhr_fire_progress_event(
        scope: &mut Scope<'_>,
        target: &Value,
        kind: &str,
        loaded: f64,
        total: f64,
        length_computable: bool,
    );
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    if p.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }
}

unsafe fn take_gstr(p: *mut c_char) -> Option<Vec<u8>> {
    if p.is_null() {
        return None;
    }
    let bytes = unsafe { borrowed(p) };
    unsafe { glib::g_free(p.cast()) };
    Some(bytes)
}

pub(crate) fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(base.map(until_nul), href)
}

pub(crate) fn url_set_component(href: &[u8], component: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::set_component(until_nul(href), until_nul(component), value)
}

pub(crate) fn url_parts(url: &[u8]) -> Option<Parts> {
    southstar_url::bytes::parts(until_nul(url))
}

pub(crate) fn page_origin(js: Js) -> Option<Vec<u8>> {
    if js.is_null() {
        return None;
    }
    southstar_url::bytes::origin_from(api::current_url(js.ptr()).to_bytes())
}

pub(crate) fn random_uuid() -> Vec<u8> {
    unsafe { take_gstr(g_uuid_string_random()) }.unwrap_or_default()
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.ptr());
}

unsafe extern "C" fn read_ready(data: *mut c_void) -> GBoolean {
    crate::file_reader::complete(data as usize as u64);
    glib::FALSE
}

pub(crate) fn attach_read_idle(js: Js, id: u64) -> Source {
    Source(api::attach_idle(
        js.ptr(),
        read_ready,
        id as usize as *mut c_void,
    ))
}

pub(crate) fn attach_read_retry(js: Js, id: u64, ms: u32) -> Source {
    Source(api::attach_timeout(
        js.ptr(),
        ms,
        read_ready,
        id as usize as *mut c_void,
    ))
}

pub(crate) fn source_remove(js: Js, source: Source) {
    api::source_remove(js.ptr(), source.0);
}

pub(crate) fn in_pump(js: Js) -> bool {
    api::in_pump(js.ptr())
}

pub(crate) fn in_main_context(js: Js, f: impl FnOnce(&mut Scope<'_>)) {
    api::main_context(js.ptr()).with(f);
}

pub(crate) fn fire_progress(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &CStr,
    loaded: f64,
    total: f64,
) {
    let kind = kind.to_string_lossy();
    nsr_xhr_fire_progress_event(scope, target, &kind, loaded, total, true);
}

pub(crate) fn abort_error(scope: &mut Scope<'_>) -> Value {
    api::abort_error(scope)
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &CStr,
) -> Value {
    api::dom_exception(
        scope,
        &name.to_string_lossy(),
        code,
        &message.to_string_lossy(),
    )
}

pub(crate) fn host_construct(
    scope: &mut Scope<'_>,
    new_target: &Value,
    kind: HostKind,
) -> JsResult {
    host::construct(scope, new_target, kind)
}

pub(crate) fn host_is(value: &Value, kind: HostKind) -> bool {
    host::is(value, kind)
}

pub(crate) fn bytes_view(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    let view = api::bytes_view(scope, value)?;
    let bytes = if view.data.is_null() || view.len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(view.data, view.len) }.to_vec()
    };
    drop(view);
    Some(bytes)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

natives! {
    nsr_window_btoa => crate::base64::btoa,
    nsr_window_atob => crate::base64::atob,
    nsr_window_url_ctor => crate::url::url_ctor,
    nsr_window_url_can_parse => crate::url::can_parse,
    nsr_window_url_parse_static => crate::url::parse_static,
    nsr_window_url_create_object => crate::blob::create_object_url,
    nsr_window_url_update_object => crate::blob::update_object_url,
    nsr_window_url_revoke_object => crate::blob::revoke_object_url,
    nsr_window_usp_ctor => crate::search_params::usp_ctor,
    nsr_window_text_encoder_ctor => crate::text_codec::encoder_ctor,
    nsr_window_text_decoder_ctor => crate::text_codec::decoder_ctor,
    nsr_window_filereader_ctor => crate::file_reader::ctor,
}

#[unsafe(no_mangle)]
pub fn nsr_url_install_interface(scope: &mut Scope<'_>) {
    crate::url::install_interface(scope);
}

#[unsafe(no_mangle)]
pub fn nsr_usp_install_interface(scope: &mut Scope<'_>) {
    crate::search_params::install_interface(scope);
}

#[unsafe(no_mangle)]
pub fn nsr_net_install_text_codecs(scope: &mut Scope<'_>, global: &Value) {
    crate::text_codec::install(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_net_install_file_reader(scope: &mut Scope<'_>, global: &Value) {
    crate::file_reader::install(scope, global);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_filereader_pending(js: *const NsJs) -> GBoolean {
    glib::boolean(crate::file_reader::pending(Js(js as usize)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_filereader_teardown(js: *mut NsJs) {
    crate::file_reader::teardown(Js(js as usize));
}

unsafe extern "C" fn resolve_blob(
    url: *const c_char,
    out_type: *mut *mut c_char,
    _user_data: *mut c_void,
) -> *mut GBytes {
    if !out_type.is_null() {
        unsafe { *out_type = ptr::null_mut() };
    }
    if url.is_null() {
        return ptr::null_mut();
    }
    let url = unsafe { CStr::from_ptr(url) }.to_bytes();
    crate::registry::resolve(url, |entry| {
        if !out_type.is_null() {
            let kind = entry
                .kind
                .as_ref()
                .map_or(ptr::null_mut(), |k| glib::strdup(k.as_bytes()));
            unsafe { *out_type = kind };
        }
        unsafe { g_bytes_ref(entry.bytes.0) }
    })
    .unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_blob_registry_add(js: *mut NsJs) {
    if !js.is_null() && crate::registry::add_page(Js(js as usize)) {
        unsafe { ns_net_set_blob_resolver(Some(resolve_blob), ptr::null_mut()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_blob_registry_remove(js: *mut NsJs) {
    if !js.is_null() {
        crate::registry::remove_page(Js(js as usize));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_blob_urls_free(js: *mut NsJs) {
    crate::registry::free_page(Js(js as usize));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_blob_url_lookup(
    js: *mut NsJs,
    url: *const c_char,
    out_type: *mut *mut c_char,
) -> *mut GBytes {
    if !out_type.is_null() {
        unsafe { *out_type = ptr::null_mut() };
    }
    if js.is_null() || url.is_null() {
        return ptr::null_mut();
    }
    let url = unsafe { CStr::from_ptr(url) }.to_bytes();
    crate::registry::with_entry(Js(js as usize), url, |entry| {
        if !out_type.is_null() {
            let kind = entry
                .kind
                .as_ref()
                .map_or(ptr::null_mut(), |k| k.as_ptr().cast_mut());
            unsafe { *out_type = kind };
        }
        entry.bytes.0
    })
    .unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_teardown(js: *const c_void) {
    crate::teardown(Js(js as usize));
}

#[unsafe(no_mangle)]
pub fn nsr_blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8> {
    crate::blob::blob_bytes(scope, blob)
}
