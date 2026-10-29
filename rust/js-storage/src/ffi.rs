//! Southstar — the C ABI and Rust-ABI entry points of Web Storage, the realm, event and GLib calls it makes, and the on-disk local area.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use southstar_dom::Node;
use southstar_glib as glib;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_keyfile as keyfile;

use crate::store::{Area, Items, Snapshot};
use crate::{Page, binding, class, events};

use southstar_js_runtime::api::Page as JsPage;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<JsPage>);

impl Js {
    fn page(self) -> Option<JsPage> {
        self.0
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }
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

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(JsPage::current(scope))
}

pub(crate) fn area_of(value: &Value) -> Option<Area> {
    class::area_of(value)
}

pub(crate) fn new_storage(scope: &mut Scope<'_>, area: i32) -> Value {
    class::new_storage(scope, area).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn halted(js: Js) -> bool {
    api::halted(js.page())
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    api::in_frame_load(js.page())
}

pub(crate) fn current_url(js: Js) -> Option<String> {
    Some(api::current_url(js.page()).to_string_lossy().into_owned())
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    api::current_document(js.page())
}

pub(crate) fn in_page_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.page()).with(f)
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    southstar_js_runtime::events::make_event(scope, kind, None)
}

pub(crate) fn frame_realm_window(js: Js, frame: Node<'_>) -> Value {
    (api::frames_hooks().frame_realm_window)(js.page(), frame)
}

pub(crate) fn inline_handlers_allowed(js: Js) -> bool {
    api::inline_handlers_allowed(js.page())
}

pub(crate) fn fire_element_handlers(
    _scope: &mut Scope<'_>,
    js: Js,
    element: Node<'_>,
    kind: &str,
    event: &Value,
) {
    southstar_js_runtime::events::fire_element_handlers(js.page(), element, kind, event);
}

pub(crate) fn dispatch_window_event(_scope: &mut Scope<'_>, js: Js, kind: &str, event: &Value) {
    api::dispatch_document_window_event(js.page(), kind, event.clone());
}

pub(crate) fn drain_microtasks(js: Js) {
    api::drain_microtasks(js.page());
}

pub(crate) fn log_line(js: Js, line: &str) {
    api::log_line(js.page(), &c_text(line));
}

pub(crate) fn in_main_realm(js: Js, f: impl FnMut()) {
    api::in_main_realm(js.page(), f);
}

fn warn(message: &str) {
    southstar_glib::warning(message);
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
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(&dir);
    restrict_mode(Path::new(&dir), 0o700);
    Some(format!("{dir}{separator}{hash}.ini"))
}

pub(crate) fn read_items(path: &str) -> Items {
    std::fs::read(path)
        .ok()
        .and_then(|data| keyfile::read_group(&data, "storage"))
        .map(|entries| entries.into_iter().collect())
        .unwrap_or_default()
}

fn write_snapshot(snapshot: &Snapshot) {
    let meta = snapshot
        .origin
        .iter()
        .map(|origin| ("origin", origin.as_str()))
        .collect();
    let storage = snapshot
        .items
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let text = keyfile::write_groups([("meta", meta), ("storage", storage)]);
    let path = Path::new(&snapshot.path);
    let contents = text.as_bytes();
    if let Err(reason) = replace_file(path, contents) {
        warn(&format!(
            "local storage: failed to write {}: {reason}",
            snapshot.path
        ));
    }
    restrict_mode(path, 0o600);
}

fn replace_file(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut temp_name = path.as_os_str().to_owned();
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp_name);
    let written = write_synced(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

fn write_synced(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, contents)?;
    file.sync_all()
}

#[cfg(unix)]
fn restrict_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict_mode(_path: &Path, _mode: u32) {}

pub(crate) fn cancel_flush(page: &Page) {
    if let Some(source) = page.flush_source.take() {
        source.remove();
    }
}

fn flush(page: &Page) {
    cancel_flush(page);
    let snapshot = page.storage.borrow_mut().take_snapshot();
    if let Some(snapshot) = snapshot {
        write_snapshot(&snapshot);
    }
}

fn flush_timer(js: Js) {
    if let Some(page) = crate::page(js) {
        page.flush_source.set(None);
        flush(&page);
    }
}

fn schedule_flush(js: Js, page: &Page) {
    if !page.storage.borrow().wants_flush() || page.flush_source.get().is_some() {
        return;
    }
    let source = glib::timeout_add_local_once(1000, move || flush_timer(js));
    page.flush_source.set(Some(source));
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

pub fn free_deferred_events(js: Option<JsPage>) {
    events::discard(Js(js));
}

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

fn init(js: JsPage) {
    crate::init(Js(Some(js)));
}

fn teardown(js: JsPage) {
    crate::teardown(Js(Some(js)));
}

fn flush_page(js: JsPage) {
    if let Some(page) = crate::page(Js(Some(js))) {
        flush(&page);
    }
}

fn schedule_page_flush(js: JsPage) {
    let js = Js(Some(js));
    if let Some(page) = crate::page(js) {
        schedule_flush(js, &page);
    }
}

fn drain_deferred_events(js: JsPage) {
    events::drain(Js(Some(js)));
}

fn switch_session(js: Option<JsPage>, _old_partition: Option<&CStr>, new_partition: &CStr) {
    let Some(page) = crate::page(Js(js)) else {
        return;
    };
    page.storage
        .borrow_mut()
        .switch_session(&new_partition.to_string_lossy());
}

fn load_page_local(js: Option<JsPage>, url: Option<&CStr>) {
    if let Some(page) = crate::page(Js(js)) {
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
        free_deferred_events: |js| free_deferred_events(Some(js)),
        switch_session,
        load_local: load_page_local,
    });
    southstar_js_runtime::links::set_js_storage_links(
        southstar_js_runtime::links::JsStorageLinks {
            storage_install_window: crate::ffi::nsr_storage_install_window,
        },
    );
}
