//! Southstar — the C ABI of image readiness, and the js.c, image cache, layout and GLib calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;
use southstar_image::{ImageCache, ImageRef};
use southstar_js_engine::ContextHandle;
use southstar_js_runtime::api::Page;
use southstar_js_runtime::api::{self};

use crate::{Element, Load};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_none()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Realm(ContextHandle);

type GBoolean = c_int;
type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_idle_add(function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_source_remove(id: c_uint) -> GBoolean;
}

pub(crate) fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub(crate) fn halted(js: Js) -> bool {
    api::halted(js.0)
}

pub(crate) fn in_pump(js: Js) -> bool {
    api::in_pump(js.0)
}

pub(crate) fn in_blocking_fetch() -> bool {
    southstar_net::in_blocking_fetch()
}

pub(crate) fn doc_base(js: Js, el: Element) -> Option<CString> {
    api::node_doc_base(js.0, Some(el))
}

pub(crate) fn base_url(js: Js, el: Element) -> Option<CString> {
    let document_url = doc_base(js, el);
    southstar_dom::base_url::owning_document(el)
        .and_then(|doc| {
            southstar_dom::base_url::document_base(doc, document_url.as_deref().map(CStr::to_bytes))
        })
        .and_then(|base| CString::new(base).ok())
        .or(document_url)
}

pub(crate) fn url_resolve(base: &CStr, href: &CStr) -> Option<CString> {
    let mut resolved = southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())?;
    if let Some(nul) = resolved.iter().position(|&b| b == 0) {
        resolved.truncate(nul);
    }
    Some(CString::new(resolved).unwrap_or_default())
}

fn image_cache(js: Js) -> Option<ImageCache> {
    ImageCache::from_raw(api::image_cache(js.0))
}

pub(crate) fn has_image_cache(js: Js) -> bool {
    image_cache(js).is_some()
}

fn load_ptr(load: &Load) -> *mut c_void {
    ptr::from_ref(load).cast_mut().cast()
}

fn load_addr(load: &Load) -> usize {
    load_ptr(load) as usize
}

pub(crate) fn cache_get(
    js: Js,
    url: &CStr,
    top_url: &CStr,
    load: &Rc<Load>,
) -> Option<ImageRef<'static>> {
    image_cache(js)?.get(url, Some(top_url), Some(on_image_ready), load_addr(load))
}

pub(crate) fn cache_peek(js: Js, url: &CStr) -> Option<ImageRef<'static>> {
    image_cache(js)?.peek(url)
}

pub(crate) fn cancel_image_cb(js: Js, load: &Load) {
    if let Some(cache) = image_cache(js) {
        cache.cancel_ready(load_addr(load));
    }
}

pub(crate) fn source_remove(id: c_uint) {
    unsafe { g_source_remove(id) };
}

pub(crate) fn idle_add_ready(load: &Rc<Load>) -> c_uint {
    unsafe { g_idle_add(ready_idle, load_ptr(load)) }
}

pub(crate) fn timeout_add_ready(ms: c_uint, load: &Rc<Load>) -> c_uint {
    unsafe { g_timeout_add(ms, ready_idle, load_ptr(load)) }
}

pub(crate) fn image_load_realm(js: Js) -> Realm {
    Realm(api::image_load_realm(js.0))
}

pub(crate) fn in_realm_scope<R>(js: Js, realm: Realm, f: impl FnOnce() -> R) -> R {
    api::in_realm(js.0, realm.0, f)
}

pub(crate) fn log_enabled(js: Js) -> bool {
    api::log_enabled(js.0)
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    api::log_line(js.0, &CString::new(line).unwrap_or_default());
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn request_repaint(js: Js) {
    api::request_repaint(js.0);
}

pub(crate) fn record_image_timing(js: Js, node: Element, img: ImageRef<'_>) {
    let Some(url) = c_str(img.url()) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let mut info = (api::loader_hooks().element_perf_info)(js.0, Some(node));
    if southstar_js_perf::has_resource_timing(js.0, info.timeline, url, Some(c"img")) {
        return;
    }
    info.next_hop_protocol = c_str(img.next_hop_protocol()).map(CStr::to_owned);
    info.timing_allow_origin = c_str(img.timing_allow_origin()).map(CStr::to_owned);
    info.status = img.http_status();
    info.body_size = img.body_size();
    let now = unsafe { g_get_monotonic_time() };
    let (request_us, response_us) = img.fetch_times_us();
    let origin_us = api::time_origin_us(js.0);
    let start = if request_us >= origin_us {
        request_us
    } else {
        now
    };
    let end = if response_us >= start {
        response_us
    } else {
        now
    };
    southstar_js_perf::add_resource_timed(js.0, Some(&info), url, Some(c"img"), (start, end), None);
}

pub(crate) fn dispatch_resource_event(js: Js, node: Element, kind: &CStr) {
    southstar_js_runtime::events::dispatch_resource_event(js.0, node, &kind.to_string_lossy());
}

pub(crate) fn layout_image(js: Js, el: Element) -> Option<ImageRef<'static>> {
    let root = api::layout_root(js.0)?;
    let b = (api::geometry_hooks().find_box_by_dom)(root, el)?;
    ImageRef::from_media(b.media()?.image()?)
}

unsafe fn retain(data: *mut c_void) -> Option<Rc<Load>> {
    let load = data.cast_const().cast::<Load>();
    if load.is_null() {
        return None;
    }
    unsafe {
        Rc::increment_strong_count(load);
        Some(Rc::from_raw(load))
    }
}

fn on_image_ready(img: ImageRef<'static>, load: usize) {
    // SAFETY: load is the address of a Load whose Rc keeps it alive until its callback is cancelled.
    if let Some(load) = unsafe { retain(load as *mut c_void) } {
        crate::on_image_ready(load, Some(img));
    }
}

unsafe extern "C" fn ready_idle(data: *mut c_void) -> GBoolean {
    if let Some(load) = unsafe { retain(data) } {
        crate::ready_idle(load);
    }
    0
}

fn js_arg(js: Option<Page>) -> Js {
    Js(js)
}

fn element(node: Node<'_>) -> Option<Element> {
    node.downgrade().get()
}

pub fn start_image_load(js: Option<Page>, el: Node<'_>, src: Option<&CStr>) {
    if let Some(el) = element(el) {
        crate::start_load(js_arg(js), el, src);
    }
}

pub fn image_for_node(js: Option<Page>, el: Node<'_>) -> Option<ImageRef<'static>> {
    element(el).and_then(|el| crate::image_of(js_arg(js), el))
}

pub fn image_natural_size(js: Option<Page>, el: Node<'_>) -> Option<(c_int, c_int)> {
    image_for_node(js, el).map(|image| image.natural_size())
}

pub fn fire_img_load_once(js: Option<Page>, node: Node<'_>, failed: bool) {
    if let Some(node) = element(node) {
        crate::fire_load_once(js_arg(js), node, failed);
    }
}

pub fn image_loads_pending(js: Option<Page>) -> bool {
    js.is_some() && crate::loads_pending(js_arg(js))
}

pub fn flush_ready_images(js: Option<Page>) {
    crate::flush_ready(js_arg(js));
}

pub fn rescan_pending_images(js: Option<Page>, root: Option<Node<'_>>) {
    crate::rescan_pending(js_arg(js), root.and_then(element));
}

pub fn image_load_tracked(js: Option<Page>, el: Node<'_>) -> bool {
    js.is_some() && element(el).is_some_and(|el| crate::is_tracked(js_arg(js), el))
}

pub fn forget_node(js: Option<Page>, node: Node<'_>) {
    if js.is_some()
        && let Some(node) = element(node)
    {
        crate::forget(js_arg(js), node);
    }
}

fn teardown(js: Option<Page>) {
    if js.is_some() {
        crate::teardown(js_arg(js));
    }
}

pub fn install_runtime_hooks() {
    api::set_images_hooks(api::ImagesHooks {
        start_image_load,
        forget_node: |js, a1| forget_node(Some(js), a1),
        teardown: |js| teardown(Some(js)),
    });
}
