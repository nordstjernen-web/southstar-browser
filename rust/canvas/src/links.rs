//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};
use std::sync::OnceLock;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_layout::BoxRef;

pub type WebglGetContextFn =
    fn(&mut Scope<'_>, usize, &Value, usize, i32, &Value) -> Result<Value, Value>;

pub type ResourceOriginCleanFn = unsafe fn(&Scope<'_>, *const c_char, *const c_char) -> bool;

pub struct JsEventsLinks {
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
}

static JS_EVENTS_LINKS: OnceLock<JsEventsLinks> = OnceLock::new();

pub fn set_js_events_links(links: JsEventsLinks) {
    let _ = JS_EVENTS_LINKS.set(links);
}

fn js_events_links() -> &'static JsEventsLinks {
    JS_EVENTS_LINKS
        .get()
        .expect("southstar-js-events installs its links before the first page")
}

pub struct JsCssomLinks {
    pub computed_text: fn(&Scope<'_>, Node<'_>, &str) -> Option<String>,
}

static JS_CSSOM_LINKS: OnceLock<JsCssomLinks> = OnceLock::new();

pub fn set_js_cssom_links(links: JsCssomLinks) {
    let _ = JS_CSSOM_LINKS.set(links);
}

fn js_cssom_links() -> &'static JsCssomLinks {
    JS_CSSOM_LINKS
        .get()
        .expect("southstar-js-cssom installs its links before the first page")
}

pub struct JsFramesLinks {
    pub realm_for_node: fn(&Scope<'_>, Node<'_>) -> ContextHandle,
}

static JS_FRAMES_LINKS: OnceLock<JsFramesLinks> = OnceLock::new();

pub fn set_js_frames_links(links: JsFramesLinks) {
    let _ = JS_FRAMES_LINKS.set(links);
}

fn js_frames_links() -> &'static JsFramesLinks {
    JS_FRAMES_LINKS
        .get()
        .expect("southstar-js-frames installs its links before the first page")
}

pub struct JsGeometryLinks {
    pub box_for_node: fn(&Scope<'_>, Node<'_>) -> Option<BoxRef<'static>>,
}

static JS_GEOMETRY_LINKS: OnceLock<JsGeometryLinks> = OnceLock::new();

pub fn set_js_geometry_links(links: JsGeometryLinks) {
    let _ = JS_GEOMETRY_LINKS.set(links);
}

fn js_geometry_links() -> &'static JsGeometryLinks {
    JS_GEOMETRY_LINKS
        .get()
        .expect("southstar-js-geometry installs its links before the first page")
}

pub struct JsWindowLinks {
    pub resource_origin_clean: ResourceOriginCleanFn,
}

static JS_WINDOW_LINKS: OnceLock<JsWindowLinks> = OnceLock::new();

pub fn set_js_window_links(links: JsWindowLinks) {
    let _ = JS_WINDOW_LINKS.set(links);
}

fn js_window_links() -> &'static JsWindowLinks {
    JS_WINDOW_LINKS
        .get()
        .expect("southstar-js-window installs its links before the first page")
}

pub struct WebglLinks {
    pub webgl_get_context: WebglGetContextFn,
    pub canvas_surface: fn(usize) -> *mut c_void,
}

static WEBGL_LINKS: OnceLock<WebglLinks> = OnceLock::new();

pub fn set_webgl_links(links: WebglLinks) {
    let _ = WEBGL_LINKS.set(links);
}

fn webgl_links() -> &'static WebglLinks {
    WEBGL_LINKS
        .get()
        .expect("southstar-webgl installs its links before the first page")
}

#[cfg(feature = "webgpu")]
pub struct WebgpuLinks {
    pub get_context: fn(&mut Scope<'_>, &Value, usize) -> Value,
    pub canvas_surface: fn(usize) -> *mut c_void,
}

#[cfg(feature = "webgpu")]
static WEBGPU_LINKS: OnceLock<WebgpuLinks> = OnceLock::new();

#[cfg(feature = "webgpu")]
pub fn set_webgpu_links(links: WebgpuLinks) {
    let _ = WEBGPU_LINKS.set(links);
}

#[cfg(feature = "webgpu")]
fn webgpu_links() -> &'static WebgpuLinks {
    WEBGPU_LINKS
        .get()
        .expect("southstar-webgpu installs its links before the first page")
}

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

pub(crate) fn realm_for_node(scope: &Scope<'_>, node: Node<'_>) -> ContextHandle {
    (js_frames_links().realm_for_node)(scope, node)
}

pub(crate) fn box_for_node(scope: &Scope<'_>, node: Node<'_>) -> Option<BoxRef<'static>> {
    (js_geometry_links().box_for_node)(scope, node)
}

pub(crate) fn resource_origin_clean() -> ResourceOriginCleanFn {
    js_window_links().resource_origin_clean
}

pub(crate) fn nsr_webgl_get_context(
    scope: &mut Scope<'_>,
    js: usize,
    canvas_obj: &Value,
    canvas: usize,
    version: i32,
    attrs: &Value,
) -> Result<Value, Value> {
    (webgl_links().webgl_get_context)(scope, js, canvas_obj, canvas, version, attrs)
}

pub(crate) fn webgl_canvas_surface(canvas: usize) -> *mut c_void {
    (webgl_links().canvas_surface)(canvas)
}

#[cfg(feature = "webgpu")]
pub(crate) fn webgpu_get_context(
    scope: &mut Scope<'_>,
    canvas_obj: &Value,
    canvas: usize,
) -> Value {
    (webgpu_links().get_context)(scope, canvas_obj, canvas)
}

#[cfg(feature = "webgpu")]
pub(crate) fn webgpu_canvas_surface(canvas: usize) -> *mut c_void {
    (webgpu_links().canvas_surface)(canvas)
}

pub(crate) fn computed_text(scope: &Scope<'_>, node: Node<'_>, name: &str) -> Option<String> {
    (js_cssom_links().computed_text)(scope, node, name)
}
