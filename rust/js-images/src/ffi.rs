//! Southstar — the C ABI of image readiness, and the js.c, image cache, layout and GLib calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};
use southstar_image::ImageRef;
use southstar_js_engine::ContextHandle;
use southstar_js_runtime::api::{self, ResourceInfo as PerfInfo};
use southstar_layout::{BoxRef, NsBox};

use crate::{Element, Load};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Realm(ContextHandle);

type GBoolean = c_int;
type ImageReadyFn = unsafe extern "C" fn(img: *mut c_void, user_data: *mut c_void);
type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_js_element_perf_info(js: *mut NsJs, el: *const NsNode, info: *mut PerfInfo);
    fn ns_perf_has_resource(
        js: *const NsJs,
        timeline: *const c_void,
        url: *const c_char,
        initiator: *const c_char,
    ) -> c_int;
    fn ns_perf_add_resource_timed(
        js: *const NsJs,
        info: *const PerfInfo,
        url: *const c_char,
        initiator: *const c_char,
        start_us: i64,
        end_us: i64,
        resp: *const c_void,
    );
    fn g_get_monotonic_time() -> i64;
    fn ns_js_dispatch_resource_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
    ) -> GBoolean;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_image_cache_get(
        cache: *mut c_void,
        url: *const c_char,
        top_url: *const c_char,
        cb: ImageReadyFn,
        user_data: *mut c_void,
    ) -> *mut c_void;
    fn ns_image_cache_peek(cache: *mut c_void, url: *const c_char) -> *mut c_void;
    fn ns_image_cache_cancel_cb(cache: *mut c_void, user_data: *mut c_void);
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
    unsafe { ns_engine_in_blocking_fetch() != 0 }
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

fn image_cache(js: Js) -> *mut c_void {
    api::image_cache(js.0)
}

pub(crate) fn has_image_cache(js: Js) -> bool {
    !image_cache(js).is_null()
}

fn load_ptr(load: &Load) -> *mut c_void {
    ptr::from_ref(load).cast_mut().cast()
}

pub(crate) fn cache_get(
    js: Js,
    url: &CStr,
    top_url: &CStr,
    load: &Rc<Load>,
) -> Option<ImageRef<'static>> {
    let raw = unsafe {
        ns_image_cache_get(
            image_cache(js),
            url.as_ptr(),
            top_url.as_ptr(),
            on_image_ready,
            load_ptr(load),
        )
    };
    unsafe { ImageRef::from_ptr(raw) }
}

pub(crate) fn cache_peek(js: Js, url: &CStr) -> Option<ImageRef<'static>> {
    unsafe { ImageRef::from_ptr(ns_image_cache_peek(image_cache(js), url.as_ptr())) }
}

pub(crate) fn cancel_image_cb(js: Js, load: &Load) {
    let cache = image_cache(js);
    if !cache.is_null() {
        unsafe { ns_image_cache_cancel_cb(cache, load_ptr(load)) };
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
    let url = img.url();
    if js.is_null() || url.is_null() {
        return;
    }
    let mut info = PerfInfo::default();
    unsafe { ns_js_element_perf_info(js.0, node.as_ptr(), &mut info) };
    if unsafe { ns_perf_has_resource(js.0, info.timeline, url, c"img".as_ptr()) } != 0 {
        return;
    }
    info.next_hop_protocol = img.next_hop_protocol();
    info.timing_allow_origin = img.timing_allow_origin();
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
    unsafe {
        ns_perf_add_resource_timed(js.0, &info, url, c"img".as_ptr(), start, end, ptr::null())
    };
}

pub(crate) fn dispatch_resource_event(js: Js, node: Element, kind: &CStr) {
    unsafe { ns_js_dispatch_resource_event(js.0, node.as_ptr(), kind.as_ptr()) };
}

pub(crate) fn layout_image(js: Js, el: Element) -> Option<ImageRef<'static>> {
    let root = unsafe { BoxRef::from_ptr(api::layout_root(js.0).cast::<NsBox>()) }?;
    let b = unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), el.as_ptr())) }?;
    unsafe { ImageRef::from_ptr(b.media()?.image()) }
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

unsafe extern "C" fn on_image_ready(img: *mut c_void, user_data: *mut c_void) {
    if let Some(load) = unsafe { retain(user_data) } {
        crate::on_image_ready(load, unsafe { ImageRef::from_ptr(img) });
    }
}

unsafe extern "C" fn ready_idle(data: *mut c_void) -> GBoolean {
    if let Some(load) = unsafe { retain(data) } {
        crate::ready_idle(load);
    }
    0
}

fn js_arg(js: *mut NsJs) -> Js {
    Js(js)
}

fn node_arg(node: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(node) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_start_image_load(
    js: *mut NsJs,
    el: *mut NsNode,
    src: *const c_char,
) {
    if let Some(el) = node_arg(el) {
        crate::start_image_load(js_arg(js), el, c_str(src));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_for_node(js: *mut NsJs, el: *const NsNode) -> *const c_void {
    node_arg(el)
        .and_then(|el| crate::image_for_node(js_arg(js), el))
        .map_or(ptr::null(), ImageRef::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_natural_size(
    js: *mut NsJs,
    el: *const NsNode,
    width: *mut c_int,
    height: *mut c_int,
) -> GBoolean {
    let Some(image) = node_arg(el).and_then(|el| crate::image_for_node(js_arg(js), el)) else {
        return 0;
    };
    let (w, h) = image.natural_size();
    unsafe {
        *width = w;
        *height = h;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fire_img_load_once(
    js: *mut NsJs,
    node: *mut NsNode,
    failed: GBoolean,
) {
    if let Some(node) = node_arg(node) {
        crate::fire_img_load_once(js_arg(js), node, failed != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_loads_pending(js: *const NsJs) -> GBoolean {
    GBoolean::from(!js.is_null() && crate::loads_pending(js_arg(js.cast_mut())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_ready_images(js: *mut NsJs) {
    crate::flush_ready_images(js_arg(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_rescan_pending_images(js: *mut NsJs, root: *mut NsNode) {
    crate::rescan_pending_images(js_arg(js), node_arg(root));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_load_tracked(js: *mut NsJs, el: *const NsNode) -> GBoolean {
    let tracked = !js.is_null() && node_arg(el).is_some_and(|el| crate::is_tracked(js_arg(js), el));
    GBoolean::from(tracked)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_images_forget_node(js: *mut NsJs, node: *const NsNode) {
    if !js.is_null()
        && let Some(node) = node_arg(node)
    {
        crate::forget_node(js_arg(js), node);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_images_teardown(js: *mut NsJs) {
    if !js.is_null() {
        crate::teardown(js_arg(js));
    }
}
