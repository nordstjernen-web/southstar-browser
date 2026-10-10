//! Southstar — the C ABI of the URL, codec, object-URL and FileReader bindings, and the js.c and net.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::JsResult;

pub(crate) const HO_FILE_READER: c_int = 4;
pub(crate) const HO_TEXT_ENCODER: c_int = 8;
pub(crate) const HO_TEXT_DECODER: c_int = 9;

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
    fn ns_js_blob_urls_put(
        js: *mut NsJs,
        url: *const c_char,
        bytes: *const u8,
        len: usize,
        kind: *const c_char,
    );
    fn ns_js_blob_urls_remove(js: *mut NsJs, url: *const c_char);
    fn ns_js_filereader_schedule(ctx: *mut JSContext, reader: JSValue, generation: i64);
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
    fn ns_ho_construct(ctx: *mut JSContext, new_target: JSValue, kind: c_int) -> JSValue;
    fn ns_js_net_host_is(v: JSValue, kind: c_int) -> GBoolean;
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

pub(crate) fn blob_urls_put(js: Js, url: &[u8], bytes: &[u8], kind: Option<&[u8]>) {
    let url = cstring(url);
    let kind = kind.map(cstring);
    let kind_ptr = kind.as_ref().map_or(ptr::null(), |k| k.as_ptr());
    unsafe {
        ns_js_blob_urls_put(
            js.ptr(),
            url.as_ptr(),
            bytes.as_ptr(),
            bytes.len(),
            kind_ptr,
        )
    };
}

pub(crate) fn blob_urls_remove(js: Js, url: &[u8]) {
    let url = cstring(url);
    unsafe { ns_js_blob_urls_remove(js.ptr(), url.as_ptr()) };
}

pub(crate) fn schedule_file_reader(scope: &mut Scope<'_>, reader: &Value, generation: i64) {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_js_filereader_schedule(ctx, quickjs::raw(reader), generation) };
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

pub(crate) fn host_construct(scope: &mut Scope<'_>, new_target: &Value, kind: c_int) -> JsResult {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_ho_construct(ctx, quickjs::raw(new_target), kind) };
    quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })
}

pub(crate) fn host_is(value: &Value, kind: c_int) -> bool {
    unsafe { ns_js_net_host_is(quickjs::raw(value), kind) != 0 }
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
pub unsafe extern "C" fn ns_filereader_run(ctx: *mut JSContext, reader: JSValue, generation: i64) {
    unsafe {
        with_value(ctx, reader, |scope, reader| {
            crate::file_reader::run(scope, reader, generation)
        })
    }
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
