//! Southstar — the C ABI and Rust-ABI entry points of the shell's input events and drag sessions, and the realm, event and dispatch calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::borrow::Cow;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_layout::{BoxRef, NsBox};

use crate::transfer::DragSession;
use crate::{Element, Modifiers, Pointer};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

unsafe extern "C" {
    fn ns_js_path_has_active_listener(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
    ) -> GBoolean;
    fn ns_js_dispatch_key_event_full(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        key: *const c_char,
        code: *const c_char,
        key_code: c_int,
        char_code: c_int,
        shift: GBoolean,
        ctrl: GBoolean,
        alt: GBoolean,
        meta: GBoolean,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
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
    fn ns_perf_realm_now_ms(ctx: ContextHandle) -> f64;
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
    unsafe fn nsr_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: &str,
        event: Value,
    ) -> (bool, bool);
}

pub(crate) fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
}

fn opt_text<'a>(text: *const c_char) -> Option<Cow<'a, str>> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_string_lossy())
}

fn with_c<R>(text: &str, f: impl FnOnce(*const c_char) -> R) -> R {
    let owned = CString::new(text.replace('\0', " ")).unwrap_or_default();
    f(owned.as_ptr())
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
        with_c(kind, |kind| unsafe {
            ns_js_path_has_active_listener(self.ptr(), target.as_ptr(), kind) != 0
        })
    }

    pub fn dispatch_built(self, target: Element, kind: &str, event: Value) -> (bool, bool) {
        let kind = kind.replace('\0', " ");
        unsafe { nsr_js_dispatch_built_event(self.ptr(), target.as_ptr(), &kind, event) }
    }

    pub fn dispatch_key(
        self,
        target: Element,
        kind: &str,
        key: &str,
        key_code: i32,
        shift: bool,
    ) -> bool {
        let mut prevented: GBoolean = 0;
        with_c(kind, |kind| {
            with_c(key, |key| unsafe {
                ns_js_dispatch_key_event_full(
                    self.ptr(),
                    target.as_ptr(),
                    kind,
                    key,
                    c"".as_ptr(),
                    key_code,
                    0,
                    glib::boolean(shift),
                    glib::FALSE,
                    glib::FALSE,
                    glib::FALSE,
                    &mut prevented,
                )
            })
        });
        prevented != 0
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
    unsafe { ns_perf_realm_now_ms(scope.handle()) }
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

fn pointer(
    client: (f64, f64),
    page: (f64, f64),
    button: c_int,
    buttons: c_int,
    modifiers: [GBoolean; 4],
) -> Pointer {
    Pointer {
        client,
        page,
        button,
        buttons,
        modifiers: Modifiers {
            shift: modifiers[0] != 0,
            ctrl: modifiers[1] != 0,
            alt: modifiers[2] != 0,
            meta: modifiers[3] != 0,
        },
    }
}

fn report(default_prevented: *mut GBoolean, prevented: bool) {
    if !default_prevented.is_null() {
        unsafe { *default_prevented = glib::boolean(prevented) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_mouse_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    client_x: f64,
    client_y: f64,
    page_x: f64,
    page_y: f64,
    button: c_int,
    buttons: c_int,
    shift: GBoolean,
    ctrl: GBoolean,
    alt: GBoolean,
    meta: GBoolean,
    related: *const NsNode,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    report(default_prevented, false);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    let js = Js::of(js);
    if js.is_null() {
        return glib::FALSE;
    }
    let input = pointer(
        (client_x, client_y),
        (page_x, page_y),
        button,
        buttons,
        [shift, ctrl, alt, meta],
    );
    let (fired, prevented) = crate::mouse::mouse_event(js, target, &kind, &input, node(related));
    report(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_clipboard_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    text: *const c_char,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    report(default_prevented, false);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    let js = Js::of(js);
    if js.is_null() {
        return glib::FALSE;
    }
    let (fired, prevented) =
        crate::transfer::clipboard_event(js, target, &kind, opt_text(text).as_deref());
    report(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drag_session_new(js: *mut NsJs) -> *mut DragSession {
    let js = Js::of(js);
    if js.is_null() {
        return ptr::null_mut();
    }
    crate::transfer::DragSession::new(js)
        .map_or(ptr::null_mut(), |session| Box::into_raw(Box::new(session)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drag_session_free(session: *mut DragSession) {
    if session.is_null() {
        return;
    }
    let session = unsafe { Box::from_raw(session) };
    session.release();
}

fn session_ref<'a>(session: *const DragSession) -> Option<&'a DragSession> {
    unsafe { session.as_ref() }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drag_session_set_data(
    session: *mut DragSession,
    kind: *const c_char,
    data: *const c_char,
) {
    let (Some(session), Some(kind)) = (session_ref(session), opt_text(kind)) else {
        return;
    };
    session.set_data(&kind, opt_text(data).as_deref());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drag_session_add_file(
    session: *mut DragSession,
    path: *const c_char,
) {
    let Some(session) = session_ref(session) else {
        return;
    };
    if path.is_null() {
        return;
    }
    session.add_file(unsafe { CStr::from_ptr(path) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_drag_event(
    js: *mut NsJs,
    session: *mut DragSession,
    target: *const NsNode,
    kind: *const c_char,
    client_x: f64,
    client_y: f64,
    page_x: f64,
    page_y: f64,
    button: c_int,
    buttons: c_int,
    shift: GBoolean,
    ctrl: GBoolean,
    alt: GBoolean,
    meta: GBoolean,
    related: *const NsNode,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    report(default_prevented, false);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    let js = Js::of(js);
    if js.is_null() {
        return glib::FALSE;
    }
    let input = pointer(
        (client_x, client_y),
        (page_x, page_y),
        button,
        buttons,
        [shift, ctrl, alt, meta],
    );
    let (fired, prevented) = crate::transfer::drag_event(
        js,
        session_ref(session),
        target,
        &kind,
        &input,
        node(related),
    );
    report(default_prevented, prevented);
    glib::boolean(fired)
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
