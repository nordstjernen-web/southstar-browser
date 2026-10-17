//! Southstar — the C ABI and Rust-ABI entry points of the shell's input events and drag sessions, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;
use std::borrow::Cow;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use crate::transfer::DragSession;
use crate::{Element, Pointer};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

unsafe extern "C" {
    fn ns_js_node_realm_context(js: *const NsJs, node: *const NsNode) -> ContextHandle;
    fn ns_js_popover_light_dismiss(js: *mut NsJs, target: *const NsNode, up: GBoolean);
    fn ns_js_dialog_light_dismiss(js: *mut NsJs, target: *const NsNode, up: GBoolean);
    fn ns_js_process_close_request(js: *mut NsJs) -> GBoolean;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_box_visual_border_box(
        b: *const NsBox,
        x: *mut f64,
        y: *mut f64,
        w: *mut f64,
        h: *mut f64,
    );
    fn g_file_get_contents(
        filename: *const c_char,
        contents: *mut *mut c_char,
        length: *mut usize,
        error: *mut *mut c_void,
    ) -> GBoolean;
    fn g_content_type_guess(
        filename: *const c_char,
        data: *const u8,
        data_size: usize,
        result_uncertain: *mut GBoolean,
    ) -> *mut c_char;
    fn g_content_type_get_mime_type(content_type: *const c_char) -> *mut c_char;
}

unsafe extern "Rust" {
    unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value;
}

fn live(n: Node<'_>) -> Option<Element> {
    n.downgrade().get()
}

fn opt_text<'a>(text: *const c_char) -> Option<Cow<'a, str>> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_string_lossy())
}

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    pub fn ctx(self) -> ContextHandle {
        api::main_context(self.ptr())
    }

    pub fn has_context(self) -> bool {
        !self.ctx().is_null()
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn realm_scope<R>(self, node: Element, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        unsafe { ns_js_node_realm_context(self.ptr(), node.as_ptr()) }
            .or(api::main_realm(self.ptr()))
            .or(self.ctx())
            .with(f)
    }

    pub fn current_document(self) -> Option<Element> {
        api::current_document(self.ptr())
    }

    pub fn layout_root(self) -> Option<BoxRef<'static>> {
        unsafe { BoxRef::from_ptr(api::layout_root(self.ptr()).cast::<NsBox>()) }
    }

    pub fn halted(self) -> bool {
        api::halted(self.ptr())
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.ptr())
    }

    pub fn blocked(self) -> bool {
        self.halted() || self.in_pump()
    }

    pub fn wpt_hooks_enabled(self) -> bool {
        api::wpt_hooks_enabled(self.ptr())
    }

    pub fn note_user_activation(self) {
        api::note_user_activation(self.ptr());
    }

    pub fn note_pointer_input(self, pointer: bool) {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_focus::note_pointer_input(self.ptr().cast(), pointer) };
    }

    pub fn path_has_active_listener(self, target: Element, kind: &str) -> bool {
        southstar_js_dispatch::path_has_active_listener(self.ptr(), target, kind)
    }

    pub fn dispatch_built(self, target: Element, kind: &str, event: Value) -> (bool, bool) {
        southstar_js_dispatch::dispatch_built_event(self.ptr(), target, kind, event)
    }

    pub fn dispatch_key(
        self,
        target: Element,
        kind: &str,
        key: &str,
        key_code: i32,
        shift: bool,
    ) -> bool {
        let press = southstar_js_dispatch::KeyPress {
            key: Some(key),
            code: Some(""),
            key_code,
            char_code: 0,
            modifiers: southstar_js_dispatch::Modifiers {
                shift,
                ..Default::default()
            },
        };
        southstar_js_dispatch::dispatch_key_event(self.ptr(), target, kind, &press).1
    }

    pub fn light_dismiss(self, target: Element, up: bool) {
        unsafe {
            ns_js_popover_light_dismiss(self.ptr(), target.as_ptr(), glib::boolean(up));
            ns_js_dialog_light_dismiss(self.ptr(), target.as_ptr(), glib::boolean(up));
        }
    }

    pub fn set_focus(self, el: Option<Element>) {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_focus::set_focus(self.ptr().cast(), el) };
    }

    pub fn focused_node(self) -> Option<Element> {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_focus::focused_node(self.ptr().cast()) }
    }

    pub fn sequential_focus_target(self, backward: bool) -> Option<Element> {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_focus::sequential_focus_target(self.ptr().cast(), backward) }
    }

    pub fn keyboard_activate(self, el: Element, key: &str, keyup: bool) {
        let key = key.replace('\0', " ");
        // SAFETY: a Js value names the page's live script engine.
        unsafe {
            southstar_js_focus::keyboard_activate(self.ptr().cast(), el, key.as_bytes(), keyup)
        };
    }

    pub fn process_close_request(self) {
        unsafe { ns_js_process_close_request(self.ptr()) };
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

pub(crate) fn unwrap(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, el: Element) -> Value {
    api::make_element(scope, el.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str, target: Element) -> Value {
    let kind = kind.replace('\0', " ");
    unsafe { nsr_make_event(scope, &kind, target.as_ptr()) }
}

pub(crate) fn realm_now_ms(scope: &Scope<'_>) -> f64 {
    southstar_js_perf::context_now_ms(scope.handle())
}

pub(crate) fn is_focusable(el: Element) -> bool {
    southstar_js_focus::is_focusable(el)
}

pub(crate) fn find_box(root: BoxRef<'_>, node: Element) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node.as_ptr())) }
}

pub(crate) fn visual_border_box(b: BoxRef<'_>) -> (f64, f64) {
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
    unsafe { ns_box_visual_border_box(b.as_ptr(), &mut x, &mut y, &mut w, &mut h) };
    (x, y)
}

pub(crate) struct FileBytes {
    pub contents: Vec<u8>,
    pub mime: Option<String>,
}

pub(crate) fn read_file(path: &CStr) -> Option<FileBytes> {
    let mut contents: *mut c_char = ptr::null_mut();
    let mut len: usize = 0;
    let ok =
        unsafe { g_file_get_contents(path.as_ptr(), &mut contents, &mut len, ptr::null_mut()) };
    if ok == 0 || contents.is_null() {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(contents.cast::<u8>(), len) }.to_vec();
    unsafe { glib::g_free(contents.cast()) };
    let content_type = unsafe {
        g_content_type_guess(
            path.as_ptr(),
            bytes.as_ptr(),
            bytes.len().min(4096),
            ptr::null_mut(),
        )
    };
    let mime = if content_type.is_null() {
        None
    } else {
        let mime = unsafe { g_content_type_get_mime_type(content_type) };
        let text = opt_text(mime).map(Cow::into_owned);
        unsafe {
            glib::g_free(mime.cast());
            glib::g_free(content_type.cast());
        }
        text
    };
    Some(FileBytes {
        contents: bytes,
        mime,
    })
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_mouse_event(
    js: *mut c_void,
    target: Node<'_>,
    kind: &CStr,
    input: &Pointer,
    related: Option<Node<'_>>,
) -> (bool, bool) {
    let js = Js::of(js.cast());
    let Some(target) = live(target) else {
        return (false, false);
    };
    if js.is_null() {
        return (false, false);
    }
    let kind = kind.to_string_lossy();
    crate::mouse::mouse_event(js, target, &kind, input, related.and_then(live))
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_clipboard_event(
    js: *mut c_void,
    target: Node<'_>,
    kind: &CStr,
    text: Option<&CStr>,
) -> (bool, bool) {
    let js = Js::of(js.cast());
    let Some(target) = live(target) else {
        return (false, false);
    };
    if js.is_null() {
        return (false, false);
    }
    let kind = kind.to_string_lossy();
    let text = text.map(CStr::to_string_lossy);
    crate::transfer::clipboard_event(js, target, &kind, text.as_deref())
}

pub struct DragData(Option<DragSession>);

impl DragData {
    /// # Safety
    ///
    /// `js` is null or a live script engine that outlives the session.
    pub unsafe fn new(js: *mut c_void) -> Option<DragData> {
        let js = Js::of(js.cast());
        if js.is_null() {
            return None;
        }
        DragSession::new(js).map(|session| DragData(Some(session)))
    }

    pub fn set_data(&self, kind: &CStr, data: Option<&CStr>) {
        if let Some(session) = &self.0 {
            let data = data.map(CStr::to_string_lossy);
            session.set_data(&kind.to_string_lossy(), data.as_deref());
        }
    }

    pub fn add_file(&self, path: &CStr) {
        if let Some(session) = &self.0 {
            session.add_file(path);
        }
    }
}

impl Drop for DragData {
    fn drop(&mut self) {
        if let Some(session) = self.0.take() {
            session.release();
        }
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_drag_event(
    js: *mut c_void,
    session: Option<&DragData>,
    target: Node<'_>,
    kind: &CStr,
    input: &Pointer,
    related: Option<Node<'_>>,
) -> (bool, bool) {
    let js = Js::of(js.cast());
    let Some(target) = live(target) else {
        return (false, false);
    };
    if js.is_null() {
        return (false, false);
    }
    let kind = kind.to_string_lossy();
    let session = session.and_then(|data| data.0.as_ref());
    crate::transfer::drag_event(js, session, target, &kind, input, related.and_then(live))
}

#[unsafe(no_mangle)]
pub fn nsr_js_input_install_wpt(scope: &mut Scope<'_>, global: &Value) {
    crate::wpt::install(scope, global);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_input_teardown(js: *mut NsJs) {
    crate::teardown(Js::of(js));
}

macro_rules! capture_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[allow(non_snake_case)]
            #[unsafe(no_mangle)]
            pub fn $name(
                scope: &mut Scope<'_>,
                this: &Value,
                args: &[Value],
            ) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

capture_methods! {
    nsr_element_setPointerCapture => crate::capture::set,
    nsr_element_releasePointerCapture => crate::capture::release,
    nsr_element_hasPointerCapture => crate::capture::has,
}
