//! Southstar — the C ABI of image readiness as declared in src/js.h and src/js_internal.h, and the js.c, image cache, layout and GLib calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, NsNode};
use southstar_image::ImageRef;
use southstar_layout::{BoxRef, NsBox};

use crate::{Element, Load};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Realm(*mut c_void);

#[repr(C, align(16))]
struct ScopeBuffer([u8; 128]);

type GBoolean = c_int;
type ImageReadyFn = unsafe extern "C" fn(img: *mut c_void, user_data: *mut c_void);
type SourceFn = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_engine_in_blocking_fetch() -> GBoolean;
    fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char;
    fn ns_js_image_cache(js: *const NsJs) -> *mut c_void;
    fn ns_js_image_load_realm(js: *mut NsJs) -> *mut c_void;
    fn ns_js_realm_scope_push(js: *mut NsJs, realm: *mut c_void, buf: *mut c_void);
    fn ns_js_realm_scope_pop(js: *mut NsJs, buf: *mut c_void);
    fn ns_js_log_enabled(js: *const NsJs) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_request_repaint(js: *mut NsJs);
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
    fn ns_js_page_time_origin_us(js: *const NsJs) -> i64;
    fn g_get_monotonic_time() -> i64;
    fn ns_js_dispatch_resource_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
    ) -> GBoolean;
    fn ns_js_layout_root(js: *const NsJs) -> *const NsBox;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
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
    fn g_free(mem: *mut c_void);
}

pub(crate) fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub(crate) fn halted(js: Js) -> bool {
    unsafe { ns_js_halted(js.0) != 0 }
}

pub(crate) fn in_pump(js: Js) -> bool {
    unsafe { ns_js_in_pump(js.0) != 0 }
}

pub(crate) fn in_blocking_fetch() -> bool {
    unsafe { ns_engine_in_blocking_fetch() != 0 }
}

pub(crate) fn doc_base(js: Js, el: Element) -> Option<&'static CStr> {
    c_str(unsafe { ns_js_node_doc_base(js.0, el.as_ptr()) })
}

pub(crate) fn url_resolve(base: &CStr, href: &CStr) -> Option<CString> {
    let raw = unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) };
    let out = c_str(raw).map(CStr::to_owned);
    if !raw.is_null() {
        unsafe { g_free(raw.cast()) };
    }
    out
}

fn image_cache(js: Js) -> *mut c_void {
    if js.is_null() {
        return ptr::null_mut();
    }
    unsafe { ns_js_image_cache(js.0) }
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
    Realm(unsafe { ns_js_image_load_realm(js.0) })
}

pub(crate) fn in_realm_scope<R>(js: Js, realm: Realm, f: impl FnOnce() -> R) -> R {
    let mut buffer = ScopeBuffer([0; 128]);
    let buf: *mut c_void = buffer.0.as_mut_ptr().cast();
    unsafe { ns_js_realm_scope_push(js.0, realm.0, buf) };
    let result = f();
    unsafe { ns_js_realm_scope_pop(js.0, buf) };
    result
}

pub(crate) fn log_enabled(js: Js) -> bool {
    unsafe { ns_js_log_enabled(js.0) != 0 }
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    let line = CString::new(line).unwrap_or_default();
    unsafe { ns_js_log_line(js.0, line.as_ptr()) };
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.0) };
}

pub(crate) fn request_repaint(js: Js) {
    unsafe { ns_js_request_repaint(js.0) };
}

pub(crate) fn record_image_timing(js: Js, node: Element, img: ImageRef<'_>) {
    unsafe { ns_js_record_image_timing(js.0, node.as_ptr(), img.url(), img.as_ptr()) };
}

pub(crate) fn dispatch_resource_event(js: Js, node: Element, kind: &CStr) {
    unsafe { ns_js_dispatch_resource_event(js.0, node.as_ptr(), kind.as_ptr()) };
}

pub(crate) fn layout_image(js: Js, el: Element) -> Option<ImageRef<'static>> {
    let root = unsafe { BoxRef::from_ptr(ns_js_layout_root(js.0)) }?;
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

#[repr(C)]
struct PerfInfo {
    timeline: *const c_void,
    document_url: *const c_char,
    render_blocking: GBoolean,
    cors_mode: GBoolean,
    next_hop_protocol: *const c_char,
    timing_allow_origin: *const c_char,
    status: core::ffi::c_long,
    body_size: i64,
}

const _: () = assert!(core::mem::size_of::<PerfInfo>() == 56);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_record_image_timing(
    js: *mut NsJs,
    node: *const NsNode,
    url: *const c_char,
    img: *const c_void,
) {
    let Some(image) = (unsafe { ImageRef::from_ptr(img) }) else {
        return;
    };
    if js.is_null() || url.is_null() {
        return;
    }
    let mut info = PerfInfo {
        timeline: ptr::null(),
        document_url: ptr::null(),
        render_blocking: 0,
        cors_mode: 0,
        next_hop_protocol: ptr::null(),
        timing_allow_origin: ptr::null(),
        status: 0,
        body_size: 0,
    };
    unsafe {
        ns_js_element_perf_info(js, node, &mut info);
        if ns_perf_has_resource(js, info.timeline, url, c"img".as_ptr()) != 0 {
            return;
        }
    }
    info.next_hop_protocol = image.next_hop_protocol();
    info.timing_allow_origin = image.timing_allow_origin();
    info.status = image.http_status();
    info.body_size = image.body_size();
    let now = unsafe { g_get_monotonic_time() };
    let origin = unsafe { ns_js_page_time_origin_us(js) };
    let start = if image.request_us() >= origin {
        image.request_us()
    } else {
        now
    };
    let end = if image.response_us() >= start {
        image.response_us()
    } else {
        now
    };
    unsafe {
        ns_perf_add_resource_timed(js, &info, url, c"img".as_ptr(), start, end, ptr::null());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_settled(
    img: *const c_void,
    failed: *mut GBoolean,
) -> GBoolean {
    let Some(image) = (unsafe { ImageRef::from_ptr(img) }) else {
        return 0;
    };
    if let Some(failed) = unsafe { failed.as_mut() } {
        *failed = GBoolean::from(image.failed());
    }
    GBoolean::from(image.loaded() || image.failed())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_natural_size(
    js: *mut NsJs,
    el: *const NsNode,
    width: *mut c_int,
    height: *mut c_int,
) -> GBoolean {
    let image = unsafe { ns_js_image_for_node(js, el) };
    let Some(image) = (unsafe { ImageRef::from_ptr(image) }) else {
        return 0;
    };
    let (w, h) = image.natural_size();
    unsafe {
        if let Some(width) = width.as_mut() {
            *width = w;
        }
        if let Some(height) = height.as_mut() {
            *height = h;
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_image_for_node(js: *mut NsJs, el: *const NsNode) -> *const c_void {
    node_arg(el)
        .and_then(|el| crate::image_for_node(js_arg(js), el))
        .map_or(ptr::null(), ImageRef::as_ptr)
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
