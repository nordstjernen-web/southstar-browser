//! Southstar — the C ABI and Rust-ABI entry points of Web Storage, the realm, event and GLib calls it makes, and the on-disk local area.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GStr};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::store::{Area, Items, Snapshot};
use crate::{Page, binding, class, events};

pub(crate) use southstar_js_runtime::api::NsJs;

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

type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

const LOG_LEVEL_WARNING: c_int = 1 << 4;
const KEY_FILE_NONE: c_int = 0;

unsafe extern "C" {

    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_source_remove(tag: c_uint) -> GBoolean;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);

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

unsafe extern "Rust" {
    unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value;
    unsafe fn nsr_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> Value;
    unsafe fn nsr_js_fire_element_handlers(
        js: *mut NsJs,
        element: *const NsNode,
        kind: &str,
        event: &Value,
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
    Js::from_ptr(scope.context_opaque().cast())
}

pub(crate) fn area_of(value: &Value) -> Option<Area> {
    class::area_of(value)
}

pub(crate) fn new_storage(scope: &mut Scope<'_>, area: i32) -> Value {
    class::new_storage(scope, area).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn halted(js: Js) -> bool {
    api::halted(js.ptr())
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    api::in_frame_load(js.ptr())
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    Some(api::current_url(js.ptr()).to_string_lossy().into_owned())
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    api::current_document(js.ptr())
}

pub(crate) fn in_page_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.ptr()).with(f)
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    unsafe { nsr_make_event(scope, kind, ptr::null()) }
}

pub(crate) fn frame_realm_window(js: Js, frame: Node<'_>) -> Value {
    unsafe { nsr_js_frame_realm_window(js.ptr(), frame.as_ptr()) }
}

pub(crate) fn inline_handlers_allowed(js: Js) -> bool {
    api::inline_handlers_allowed(js.ptr())
}

pub(crate) fn fire_element_handlers(
    _scope: &mut Scope<'_>,
    js: Js,
    element: Node<'_>,
    kind: &str,
    event: &Value,
) {
    unsafe { nsr_js_fire_element_handlers(js.ptr(), element.as_ptr(), kind, event) };
}

pub(crate) fn dispatch_window_event(_scope: &mut Scope<'_>, js: Js, kind: &str, event: &Value) {
    api::dispatch_document_window_event(js.ptr(), kind, event.clone());
}

pub(crate) fn drain_microtasks(js: Js) {
    api::drain_microtasks(js.ptr());
}

pub(crate) fn log_line(js: Js, line: &str) {
    api::log_line(js.ptr(), &c_text(line));
}

pub(crate) fn in_main_realm(js: Js, f: impl FnMut()) {
    api::in_main_realm(js.ptr(), f);
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
    southstar_url::bytes::origin_from(c_text(url).as_bytes())
        .map(|origin| String::from_utf8_lossy(&origin).into_owned())
}

pub(crate) fn storage_path_for_origin(origin: &str) -> Option<String> {
    if origin.is_empty() || southstar_config::private_mode() {
        return None;
    }
    let hash = southstar_digest::sha256_hex(c_text(origin).as_bytes());
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

pub fn free_deferred_events(js: *const NsJs) {
    events::discard(Js::from_ptr(js.cast_mut()));
}

#[unsafe(no_mangle)]
pub fn nsr_storage_install_window(scope: &mut Scope<'_>, global: &Value) {
    binding::install_window(scope, global);
}

pub fn nsr_storage_area_of(value: &Value) -> Option<i32> {
    value
        .is_instance_of_class(&class::STORAGE_CLASS)
        .then(|| class::area_tag(value))
}

pub fn nsr_storage_new(scope: &mut Scope<'_>, area: i32) -> Result<Value, Value> {
    class::new_storage(scope, area)
}

fn init(js: *const NsJs) {
    crate::init(Js::from_ptr(js.cast_mut()));
}

fn teardown(js: *const NsJs) {
    crate::teardown(Js::from_ptr(js.cast_mut()));
}

fn flush_page(js: *const NsJs) {
    if let Some(page) = crate::page(Js::from_ptr(js.cast_mut())) {
        flush(&page);
    }
}

fn schedule_page_flush(js: *const NsJs) {
    let js = Js::from_ptr(js.cast_mut());
    if let Some(page) = crate::page(js) {
        schedule_flush(js, &page);
    }
}

fn drain_deferred_events(js: *const NsJs) {
    events::drain(Js::from_ptr(js.cast_mut()));
}

fn switch_session(js: *const NsJs, old_partition: Option<&CStr>, new_partition: &CStr) {
    let Some(page) = crate::page(Js::from_ptr(js.cast_mut())) else {
        return;
    };
    let old = old_partition.map(|old| old.to_string_lossy().into_owned());
    let new = new_partition.to_string_lossy();
    page.storage
        .borrow_mut()
        .switch_session(old.as_deref(), &new);
}

fn load_page_local(js: *const NsJs, url: Option<&CStr>) {
    if let Some(page) = crate::page(Js::from_ptr(js.cast_mut())) {
        load_local(&page, url.map(|url| url.to_string_lossy().into_owned()));
    }
}

pub fn install_runtime_hooks() {
    api::set_storage_hooks(api::StorageHooks {
        init,
        teardown,
        flush: flush_page,
        schedule_flush: schedule_page_flush,
        drain_deferred_events,
        free_deferred_events,
        switch_session,
        load_local: load_page_local,
    });
}
