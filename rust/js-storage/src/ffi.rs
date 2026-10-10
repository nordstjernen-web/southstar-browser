//! Southstar — the C ABI of Web Storage as declared in src/js_internal.h, the js.c and GLib calls it makes, and the on-disk local area.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GStr};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::store::{Area, Items, Snapshot};
use crate::{Page, binding, events};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
struct GKeyFile {
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

type MainRealmFn = unsafe extern "C" fn(data: *mut c_void);
type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

const LOG_LEVEL_WARNING: c_int = 1 << 4;
const KEY_FILE_NONE: c_int = 0;

unsafe extern "C" {
    fn ns_storage_area_of(obj: JSValue) -> c_int;
    fn ns_storage_new(ctx: *mut JSContext, area: c_int) -> JSValue;
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_frame_load(js: *const NsJs) -> GBoolean;
    fn ns_js_in_main_realm(js: *mut NsJs, f: MainRealmFn, data: *mut c_void);
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> JSValue;
    fn ns_js_inline_handlers_allowed(js: *const NsJs) -> GBoolean;
    fn ns_js_fire_element_handlers(
        js: *mut NsJs,
        element: *const NsNode,
        kind: *const c_char,
        event: JSValue,
    );
    fn ns_js_dispatch_document_window_event(js: *mut NsJs, kind: *const c_char, event: JSValue);
    fn ns_drain_microtasks(js: *mut NsJs);
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;

    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_source_remove(tag: c_uint) -> GBoolean;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_compute_checksum_for_string(
        checksum_type: glib::GChecksumType,
        text: *const c_char,
        length: isize,
    ) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_key_file_new() -> *mut GKeyFile;
    fn g_key_file_free(key_file: *mut GKeyFile);
    fn g_key_file_load_from_file(
        key_file: *mut GKeyFile,
        file: *const c_char,
        flags: c_int,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_key_file_to_data(
        key_file: *mut GKeyFile,
        length: *mut usize,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_key_file_get_keys(
        key_file: *mut GKeyFile,
        group: *const c_char,
        length: *mut usize,
        error: *mut *mut GError,
    ) -> *mut *mut c_char;
    fn g_key_file_get_string(
        key_file: *mut GKeyFile,
        group: *const c_char,
        key: *const c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_key_file_set_string(
        key_file: *mut GKeyFile,
        group: *const c_char,
        key: *const c_char,
        value: *const c_char,
    );
}

fn c_text(text: &str) -> CString {
    let bytes = text.as_bytes();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn owned(text: *const c_char) -> Option<String> {
    (!text.is_null()).then(|| {
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    })
}

fn taken(text: *mut c_char) -> Option<String> {
    unsafe { GStr::take(text) }.map(|text| text.to_string_lossy().into_owned())
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(quickjs::context_opaque(scope).cast())
}

pub(crate) fn area_of(value: &Value) -> Option<Area> {
    if !value.is_object() {
        return None;
    }
    Area::from_tag(unsafe { ns_storage_area_of(quickjs::raw(value)) })
}

pub(crate) fn new_storage(scope: &mut Scope<'_>, area: i32) -> Value {
    let raw = unsafe { ns_storage_new(quickjs::raw_context(scope), area) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn halted(js: Js) -> bool {
    unsafe { ns_js_halted(js.ptr()) != 0 }
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    unsafe { ns_js_in_frame_load(js.ptr()) != 0 }
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    owned(unsafe { ns_js_current_url(js.ptr()) })
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_current_document(js.ptr())) }
}

pub(crate) fn in_page_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    unsafe { quickjs::with_context(ctx, f) }
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    let kind = c_text(kind);
    let raw = unsafe { ns_make_event(quickjs::raw_context(scope), kind.as_ptr(), ptr::null()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn frame_realm_window(scope: &mut Scope<'_>, js: Js, frame: Node<'_>) -> Value {
    let raw = unsafe { ns_js_frame_realm_window(js.ptr(), frame.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn inline_handlers_allowed(js: Js) -> bool {
    unsafe { ns_js_inline_handlers_allowed(js.ptr()) != 0 }
}

pub(crate) fn fire_element_handlers(
    _scope: &mut Scope<'_>,
    js: Js,
    element: Node<'_>,
    kind: &str,
    event: &Value,
) {
    let kind = c_text(kind);
    unsafe {
        ns_js_fire_element_handlers(
            js.ptr(),
            element.as_ptr(),
            kind.as_ptr(),
            quickjs::raw(event),
        )
    };
}

pub(crate) fn dispatch_window_event(_scope: &mut Scope<'_>, js: Js, kind: &str, event: &Value) {
    let kind = c_text(kind);
    let event = quickjs::into_raw(event.clone());
    unsafe { ns_js_dispatch_document_window_event(js.ptr(), kind.as_ptr(), event) };
}

pub(crate) fn drain_microtasks(js: Js) {
    unsafe { ns_drain_microtasks(js.ptr()) };
}

pub(crate) fn log_line(js: Js, line: &str) {
    let line = c_text(line);
    unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) };
}

struct MainRealmJob<'a> {
    run: &'a mut dyn FnMut(),
}

unsafe extern "C" fn run_main_realm_job(data: *mut c_void) {
    let job = unsafe { &mut *data.cast::<MainRealmJob<'_>>() };
    (job.run)();
}

pub(crate) fn in_main_realm(js: Js, mut f: impl FnMut()) {
    let mut job = MainRealmJob { run: &mut f };
    unsafe {
        ns_js_in_main_realm(
            js.ptr(),
            run_main_realm_job,
            (&raw mut job).cast::<c_void>(),
        )
    };
}

fn warn(message: &str) {
    let message = c_text(message);
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
}

fn url_origin(url: &str) -> Option<String> {
    let url = c_text(url);
    taken(unsafe { ns_url_origin_from(url.as_ptr()) })
}

pub(crate) fn storage_path_for_origin(origin: &str) -> Option<String> {
    if origin.is_empty() || southstar_config::private_mode() {
        return None;
    }
    let origin = c_text(origin);
    let hash = taken(unsafe {
        g_compute_checksum_for_string(glib::G_CHECKSUM_SHA256, origin.as_ptr(), -1)
    })?;
    let data_dir = owned(unsafe { glib::g_get_user_data_dir() })?;
    let separator = std::path::MAIN_SEPARATOR;
    let dir = format!("{data_dir}{separator}southstar{separator}localstorage");
    let dir_c = c_text(&dir);
    unsafe {
        g_mkdir_with_parents(dir_c.as_ptr(), 0o700);
        g_chmod(dir_c.as_ptr(), 0o700);
    }
    Some(format!("{dir}{separator}{hash}.ini"))
}

struct KeyFile(*mut GKeyFile);

impl KeyFile {
    fn new() -> KeyFile {
        KeyFile(unsafe { g_key_file_new() })
    }
}

impl Drop for KeyFile {
    fn drop(&mut self) {
        unsafe { g_key_file_free(self.0) };
    }
}

pub(crate) fn read_items(path: &str) -> Items {
    let mut items = Items::new();
    let key_file = KeyFile::new();
    let path = c_text(path);
    let group = c"storage";
    let loaded = unsafe {
        g_key_file_load_from_file(key_file.0, path.as_ptr(), KEY_FILE_NONE, ptr::null_mut())
    };
    if loaded == 0 {
        return items;
    }
    let mut n = 0usize;
    let keys = unsafe { g_key_file_get_keys(key_file.0, group.as_ptr(), &mut n, ptr::null_mut()) };
    if keys.is_null() {
        return items;
    }
    for i in 0..n {
        let key = unsafe { *keys.add(i) };
        let value = taken(unsafe {
            g_key_file_get_string(key_file.0, group.as_ptr(), key, ptr::null_mut())
        });
        if let (Some(key), Some(value)) = (owned(key), value) {
            items.insert(key, value);
        }
    }
    unsafe { glib::g_strfreev(keys) };
    items
}

fn write_snapshot(snapshot: &Snapshot) {
    let key_file = KeyFile::new();
    if let Some(origin) = &snapshot.origin {
        let origin = c_text(origin);
        unsafe {
            g_key_file_set_string(
                key_file.0,
                c"meta".as_ptr(),
                c"origin".as_ptr(),
                origin.as_ptr(),
            )
        };
    }
    for (key, value) in &snapshot.items {
        let (key, value) = (c_text(key), c_text(value));
        unsafe {
            g_key_file_set_string(
                key_file.0,
                c"storage".as_ptr(),
                key.as_ptr(),
                value.as_ptr(),
            )
        };
    }
    let mut len = 0usize;
    let data = unsafe { g_key_file_to_data(key_file.0, &mut len, ptr::null_mut()) };
    let Some(data) = (unsafe { GStr::take(data) }) else {
        return;
    };
    let path = c_text(&snapshot.path);
    let mut error: *mut GError = ptr::null_mut();
    let written = unsafe {
        g_file_set_contents(
            path.as_ptr(),
            data.as_ptr(),
            isize::try_from(len).unwrap_or(isize::MAX),
            &mut error,
        )
    };
    if written == 0 {
        let reason = if error.is_null() {
            String::new()
        } else {
            let reason = owned(unsafe { (*error).message }).unwrap_or_default();
            unsafe { glib::g_error_free(error) };
            reason
        };
        warn(&format!(
            "local storage: failed to write {}: {reason}",
            snapshot.path
        ));
    }
    unsafe { g_chmod(path.as_ptr(), 0o600) };
}

pub(crate) fn cancel_flush(page: &Page) {
    let source = page.flush_source.replace(0);
    if source != 0 {
        unsafe { g_source_remove(source) };
    }
}

fn flush(page: &Page) {
    cancel_flush(page);
    let snapshot = page.storage.borrow_mut().take_snapshot();
    if let Some(snapshot) = snapshot {
        write_snapshot(&snapshot);
    }
}

unsafe extern "C" fn flush_timer(data: *mut c_void) -> GBoolean {
    if let Some(page) = crate::page(Js(data as usize)) {
        page.flush_source.set(0);
        flush(&page);
    }
    glib::FALSE
}

fn schedule_flush(js: Js, page: &Page) {
    if !page.storage.borrow().wants_flush() || page.flush_source.get() != 0 {
        return;
    }
    let source = unsafe { g_timeout_add(1000, flush_timer, js.ptr().cast()) };
    page.flush_source.set(source);
}

fn load_local(page: &Rc<Page>, url: Option<String>) {
    if page.storage.borrow().local_disabled() {
        page.storage.borrow_mut().clear_local();
        return;
    }
    let origin = url.as_deref().and_then(url_origin);
    if page.storage.borrow().same_origin(origin.as_deref()) {
        return;
    }
    flush(page);
    page.storage.borrow_mut().adopt_origin(origin);
}

unsafe fn with_value<R>(
    ctx: *mut JSContext,
    value: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> R,
) -> R {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_init(js: *mut NsJs) {
    crate::init(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_teardown(js: *mut NsJs) {
    crate::teardown(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_flush(js: *mut NsJs) {
    if let Some(page) = crate::page(Js::from_ptr(js)) {
        flush(&page);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_schedule_flush(js: *mut NsJs) {
    let js = Js::from_ptr(js);
    if let Some(page) = crate::page(js) {
        schedule_flush(js, &page);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_drain_deferred_events(js: *mut NsJs) {
    events::drain(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_free_deferred_events(js: *mut NsJs) {
    events::discard(Js::from_ptr(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_switch_session(
    js: *mut NsJs,
    old_partition: *const c_char,
    new_partition: *const c_char,
) {
    let Some(page) = crate::page(Js::from_ptr(js)) else {
        return;
    };
    let old = owned(old_partition);
    let new = owned(new_partition).unwrap_or_default();
    page.storage
        .borrow_mut()
        .switch_session(old.as_deref(), &new);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_load_local(js: *mut NsJs, url: *const c_char) {
    if let Some(page) = crate::page(Js::from_ptr(js)) {
        load_local(&page, owned(url));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_install_proto(ctx: *mut JSContext, proto: JSValue) {
    unsafe { with_value(ctx, proto, binding::install_proto) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_install_window(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_value(ctx, global, binding::install_window) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_named_value(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
) -> *mut c_char {
    let Some(name) = owned(name) else {
        return ptr::null_mut();
    };
    let value = unsafe {
        with_value(ctx, obj, |scope, obj| {
            binding::named_value(scope, obj, &name)
        })
    };
    value.map_or(ptr::null_mut(), |value| {
        glib::strdup(c_text(&value).as_bytes())
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_named_set(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
    value: JSValue,
) -> c_int {
    let Some(name) = owned(name) else {
        return 0;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            let value = quickjs::borrow_value(scope, value);
            match binding::named_set(scope, &obj, &name, &value) {
                Ok(()) => 1,
                Err(error) => {
                    quickjs::result_raw(scope, Err(error));
                    -1
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_named_delete(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
) {
    if let Some(name) = owned(name) {
        unsafe {
            with_value(ctx, obj, |scope, obj| {
                binding::named_delete(scope, obj, &name)
            })
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_names(ctx: *mut JSContext, obj: JSValue) -> *mut *mut c_char {
    let names = unsafe { with_value(ctx, obj, |scope, obj| binding::names(scope, obj)) };
    let strv = unsafe { glib::g_malloc0((names.len() + 1) * size_of::<*mut c_char>()) }
        .cast::<*mut c_char>();
    for (i, name) in names.iter().enumerate() {
        unsafe { *strv.add(i) = glib::strdup(c_text(name).as_bytes()) };
    }
    strv
}
