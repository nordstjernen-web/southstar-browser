//! Southstar — the C ABI of the URL, codec, object-URL, blob: URL registry and FileReader bindings, and the js.c and net.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_bind::host::{self, HostKind};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::JsResult;

pub(crate) const HO_FILE_READER: HostKind = HostKind::FileReader;
pub(crate) const HO_TEXT_ENCODER: HostKind = HostKind::TextEncoder;
pub(crate) const HO_TEXT_DECODER: HostKind = HostKind::TextDecoder;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    pub(crate) fn of(scope: &Scope<'_>) -> Js {
        Js(quickjs::context_opaque(scope) as usize)
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

type SourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

type BlobResolver = Option<
    unsafe extern "C" fn(
        url: *const c_char,
        out_type: *mut *mut c_char,
        user_data: *mut c_void,
    ) -> *mut GBytes,
>;

#[repr(C)]
struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

pub(crate) struct UrlParts {
    pub href: Vec<u8>,
    pub protocol: Vec<u8>,
    pub origin: Vec<u8>,
    pub host: Vec<u8>,
    pub hostname: Vec<u8>,
    pub port: Vec<u8>,
    pub pathname: Vec<u8>,
    pub search: Vec<u8>,
    pub hash: Vec<u8>,
    pub username: Vec<u8>,
    pub password: Vec<u8>,
}

unsafe extern "C" {
    fn ns_url_resolve_len(base: *const c_char, href: *const c_char, href_len: usize)
    -> *mut c_char;
    fn ns_url_set_component_len(
        href: *const c_char,
        component: *const c_char,
        value: *const c_char,
        value_len: usize,
    ) -> *mut c_char;
    fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts;
    fn ns_url_parts_free(parts: *mut NsUrlParts);
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_attach_idle(js: *mut NsJs, func: SourceFunc, data: *mut c_void) -> c_uint;
    fn ns_js_attach_timeout(
        js: *mut NsJs,
        ms: c_uint,
        func: SourceFunc,
        data: *mut c_void,
    ) -> c_uint;
    fn ns_js_source_remove(js: *mut NsJs, id: c_uint);
    fn ns_net_set_blob_resolver(resolver: BlobResolver, user_data: *mut c_void);
    fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    fn g_bytes_ref(bytes: *mut GBytes) -> *mut GBytes;
    fn g_bytes_unref(bytes: *mut GBytes);
    fn ns_xhr_fire_progress_event(
        ctx: *mut JSContext,
        target: JSValue,
        kind: *const c_char,
        loaded: f64,
        total: f64,
        computable: GBoolean,
    );
    fn ns_make_abort_error(ctx: *mut JSContext) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_bytes_view(
        ctx: *mut JSContext,
        value: JSValue,
        out_data: *mut *const u8,
        out_len: *mut usize,
        out_holder: *mut JSValue,
    ) -> GBoolean;
    fn g_uuid_string_random() -> *mut c_char;
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
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
    let base = base.map(cstring);
    let base_ptr = base.as_ref().map_or(ptr::null(), |b| b.as_ptr());
    unsafe {
        take_gstr(ns_url_resolve_len(
            base_ptr,
            href.as_ptr().cast(),
            href.len(),
        ))
    }
}

pub(crate) fn url_set_component(href: &[u8], component: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    let href = cstring(href);
    let component = cstring(component);
    unsafe {
        take_gstr(ns_url_set_component_len(
            href.as_ptr(),
            component.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
        ))
    }
}

pub(crate) fn url_parts(url: &[u8]) -> Option<UrlParts> {
    let url = cstring(url);
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    if parts.is_null() {
        return None;
    }
    let copy = unsafe {
        let p = &*parts;
        UrlParts {
            href: borrowed(p.href),
            protocol: borrowed(p.protocol),
            origin: borrowed(p.origin),
            host: borrowed(p.host),
            hostname: borrowed(p.hostname),
            port: borrowed(p.port),
            pathname: borrowed(p.pathname),
            search: borrowed(p.search),
            hash: borrowed(p.hash),
            username: borrowed(p.username),
            password: borrowed(p.password),
        }
    };
    unsafe { ns_url_parts_free(parts) };
    Some(copy)
}

pub(crate) fn page_origin(js: Js) -> Option<Vec<u8>> {
    if js.is_null() {
        return None;
    }
    let url = unsafe { ns_js_current_url(js.ptr()) };
    if url.is_null() {
        return None;
    }
    unsafe { take_gstr(ns_url_origin_from(url)) }
}

pub(crate) fn random_uuid() -> Vec<u8> {
    unsafe { take_gstr(g_uuid_string_random()) }.unwrap_or_default()
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.ptr()) };
}

unsafe extern "C" fn read_ready(data: *mut c_void) -> GBoolean {
    crate::file_reader::complete(data as usize as u64);
    glib::FALSE
}

pub(crate) fn attach_read_idle(js: Js, id: u64) -> Source {
    Source(unsafe { ns_js_attach_idle(js.ptr(), read_ready, id as usize as *mut c_void) })
}

pub(crate) fn attach_read_retry(js: Js, id: u64, ms: u32) -> Source {
    Source(unsafe { ns_js_attach_timeout(js.ptr(), ms, read_ready, id as usize as *mut c_void) })
}

pub(crate) fn source_remove(js: Js, source: Source) {
    unsafe { ns_js_source_remove(js.ptr(), source.0) };
}

pub(crate) fn in_pump(js: Js) -> bool {
    unsafe { ns_js_in_pump(js.ptr()) != 0 }
}

pub(crate) fn in_main_context(js: Js, f: impl FnOnce(&mut Scope<'_>)) {
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    unsafe { quickjs::with_context(ctx, f) }
}

pub(crate) fn fire_progress(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &CStr,
    loaded: f64,
    total: f64,
) {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        ns_xhr_fire_progress_event(
            ctx,
            quickjs::raw(target),
            kind.as_ptr(),
            loaded,
            total,
            glib::TRUE,
        )
    };
}

pub(crate) fn abort_error(scope: &mut Scope<'_>) -> Value {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_make_abort_error(ctx) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: c_int,
    message: &CStr,
) -> Value {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_throw_dom_exception(ctx, name.as_ptr(), code, message.as_ptr()) };
    quickjs::take_exception(scope)
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
    let ctx = quickjs::raw_context(scope);
    let mut data: *const u8 = ptr::null();
    let mut len = 0usize;
    let mut holder = quickjs::raw(&Value::undefined());
    let ok =
        unsafe { ns_js_bytes_view(ctx, quickjs::raw(value), &mut data, &mut len, &mut holder) };
    if ok == 0 {
        return None;
    }
    let holder = unsafe { quickjs::take_value(scope, holder) };
    let bytes = if data.is_null() || len == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }.to_vec()
    };
    drop(holder);
    Some(bytes)
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

unsafe fn with_value(ctx: *mut JSContext, value: JSValue, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value);
        })
    }
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
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

export_native! {
    ns_window_btoa => crate::base64::btoa,
    ns_window_atob => crate::base64::atob,
    ns_window_url_ctor => crate::url::url_ctor,
    ns_window_url_can_parse => crate::url::can_parse,
    ns_window_url_parse_static => crate::url::parse_static,
    ns_window_url_create_object => crate::blob::create_object_url,
    ns_window_url_update_object => crate::blob::update_object_url,
    ns_window_url_revoke_object => crate::blob::revoke_object_url,
    ns_window_usp_ctor => crate::search_params::usp_ctor,
    ns_window_text_encoder_ctor => crate::text_codec::encoder_ctor,
    ns_window_text_decoder_ctor => crate::text_codec::decoder_ctor,
    ns_window_filereader_ctor => crate::file_reader::ctor,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_install_interface(ctx: *mut JSContext) {
    unsafe { quickjs::with_context(ctx, crate::url::install_interface) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_usp_install_interface(ctx: *mut JSContext) {
    unsafe { quickjs::with_context(ctx, crate::search_params::install_interface) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_text_codecs(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::text_codec::install) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_file_reader(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, crate::file_reader::install) }
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
pub unsafe extern "C" fn ns_blob_bytes_as_string(
    ctx: *mut JSContext,
    blob: JSValue,
    out_len: *mut usize,
) -> *mut c_char {
    let bytes = unsafe {
        quickjs::with_context(ctx, |scope| {
            let blob = quickjs::borrow_value(scope, blob);
            crate::blob::blob_bytes(scope, &blob)
        })
    };
    if !out_len.is_null() {
        unsafe { *out_len = bytes.len() };
    }
    let out = unsafe { glib::g_malloc(bytes.len() + 1) }.cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
    }
    out.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_url_teardown(js: *const c_void) {
    crate::teardown(Js(js as usize));
}

#[unsafe(no_mangle)]
pub fn nsr_blob_bytes(
    scope: &mut southstar_js_engine::Scope<'_>,
    blob: &southstar_js_engine::Value,
) -> Vec<u8> {
    crate::blob::blob_bytes(scope, blob)
}
