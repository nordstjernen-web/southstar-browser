//! Southstar — entering and leaving a realm, a frame's URL, a dispatch, a frame load, a timer and an animation frame, with the top-level URL slot they share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Kind, Node, NsNode};
use southstar_glib as glib;
use southstar_js_engine::{ContextHandle, Value};

use super::Js;
use crate::budget;

const FRAME_URL_ATTR: &CStr = c"data-nd-frame-url";

unsafe extern "C" {
    fn ns_iframe_document_node(iframe: *const NsNode) -> *mut NsNode;
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_js_frame_of_realm(js: *const c_void, realm: ContextHandle) -> *mut NsNode;
    fn ns_js_node_realm_context(js: *const c_void, node: *const NsNode) -> ContextHandle;
    fn ns_node_root(n: *const NsNode) -> *const NsNode;
    fn ns_doc_id_index_build(doc: *mut NsNode);
    fn g_strcmp0(a: *const c_char, b: *const c_char) -> c_int;
}

#[repr(C)]
pub(crate) struct FrameUrl {
    saved: *mut c_char,
    owns_slot: bool,
}

impl FrameUrl {
    pub const fn new() -> FrameUrl {
        FrameUrl {
            saved: ptr::null_mut(),
            owns_slot: false,
        }
    }
}

#[repr(C)]
pub(crate) struct RealmScope {
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    url: *mut c_char,
    entered_url: *mut c_char,
    prev_slot: *mut *mut c_char,
    active: bool,
    is_base: bool,
}

impl RealmScope {
    pub const fn inactive() -> RealmScope {
        RealmScope {
            ctx: ContextHandle::NULL,
            doc: ptr::null_mut(),
            frame: ptr::null_mut(),
            url: ptr::null_mut(),
            entered_url: ptr::null_mut(),
            prev_slot: ptr::null_mut(),
            active: false,
            is_base: false,
        }
    }
}

#[repr(C)]
struct DispatchScope {
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    entered: bool,
    url: FrameUrl,
}

pub(crate) struct TimerScope {
    js: Js,
    ctx: ContextHandle,
    previous_ctx: ContextHandle,
    frame_scope: RealmScope,
    budget: i64,
}

pub(crate) struct RafScope {
    js: Js,
    previous_ctx: ContextHandle,
    previous_doc: *mut NsNode,
    previous_frame: *mut NsNode,
    frame: *mut NsNode,
    url: FrameUrl,
}

struct FrameLoadScope {
    url: FrameUrl,
    doc: *mut NsNode,
    script: *mut NsNode,
    iframe_doc: Option<Value>,
    frame: *mut NsNode,
}

pub(crate) const SCOPE_BYTES: usize = 128;

const _: () = assert!(core::mem::size_of::<RealmScope>() <= SCOPE_BYTES);
const _: () = assert!(core::mem::size_of::<DispatchScope>() <= SCOPE_BYTES);

pub(crate) fn top_slot(js: Js) -> *mut *mut c_char {
    let slot = js.st().top_url_slot.get();
    if slot.is_null() {
        js.st().slots.current_url
    } else {
        slot
    }
}

pub(crate) fn top_url(js: Js) -> *const c_char {
    let url = unsafe { *top_slot(js) };
    if url.is_null() { c"".as_ptr() } else { url }
}

pub(crate) fn set_top_url(js: Js, url: *const c_char) {
    let slot = top_slot(js);
    let copy = unsafe { glib::g_strdup(non_null_text(url)) };
    unsafe {
        glib::g_free((*slot).cast());
        *slot = copy;
    }
}

fn non_null_text(text: *const c_char) -> *const c_char {
    if text.is_null() { c"".as_ptr() } else { text }
}

fn frame_url_attr(frame: *const NsNode) -> *const c_char {
    unsafe { ns_element_get_attr(frame, FRAME_URL_ATTR.as_ptr()) }
}

fn frame_url_enter(js: Js, fu: &mut FrameUrl, url: *const c_char) {
    fu.saved = js.current_url_raw();
    fu.owns_slot = js.st().top_url_slot.get().is_null();
    if fu.owns_slot {
        js.st().top_url_slot.set(&mut fu.saved);
    }
    js.set_current_url_raw(unsafe { glib::g_strdup(non_null_text(url)) });
}

fn frame_url_leave(js: Js, fu: &mut FrameUrl) {
    if fu.owns_slot {
        js.st().top_url_slot.set(ptr::null_mut());
    }
    unsafe { glib::g_free(js.current_url_raw().cast()) };
    js.set_current_url_raw(fu.saved);
}

pub(crate) fn top_document(doc: *mut NsNode) -> *mut NsNode {
    let mut current = unsafe { Node::from_ptr(doc) };
    while let Some(node) = current {
        let Some(mut parent) = node.parent() else {
            break;
        };
        loop {
            if parent.kind() == Kind::Document {
                current = Some(parent);
                break;
            }
            match parent.parent() {
                Some(next) => parent = next,
                None => return ptr::null_mut(),
            }
        }
    }
    Node::ptr_or_null(current).cast_mut()
}

pub(crate) fn node_root(node: *const NsNode) -> *const NsNode {
    unsafe { ns_node_root(node) }
}

pub(crate) fn frame_of_realm(js: Js, realm: ContextHandle) -> *mut NsNode {
    unsafe { ns_js_frame_of_realm(js.ptr().cast(), realm) }
}

pub(crate) fn node_realm_context(js: Js, node: *const NsNode) -> ContextHandle {
    unsafe { ns_js_node_realm_context(js.ptr().cast(), node) }
}

fn save(js: Js, scope: &mut RealmScope) {
    scope.ctx = js.ctx();
    scope.doc = js.current_doc();
    scope.frame = js.frame();
    scope.url = js.current_url_raw();
    scope.entered_url = ptr::null_mut();
    scope.prev_slot = js.st().top_url_slot.get();
    scope.is_base = false;
    scope.active = true;
}

pub(crate) fn frame_scope_enter(
    js: Js,
    realm: ContextHandle,
    frame: *mut NsNode,
    scope: &mut RealmScope,
) {
    save(js, scope);
    let state = js.st();
    if state.realm_scope_base.get().is_null() && js.ctx() == js.main_realm() {
        state.realm_scope_base.set(scope);
        scope.is_base = true;
    }
    if state.top_url_slot.get().is_null() {
        state.top_url_slot.set(&mut scope.url);
    }
    js.set_ctx(realm);
    let frame_doc = unsafe { ns_iframe_document_node(frame) };
    if !frame_doc.is_null() {
        js.set_current_doc(frame_doc);
    }
    js.set_frame(frame);
    js.set_current_url_raw(unsafe { glib::g_strdup(non_null_text(frame_url_attr(frame))) });
}

pub(crate) fn realm_url(js: Js, realm: ContextHandle) -> *const c_char {
    if realm.is_null() {
        return ptr::null();
    }
    let main = js.main_or_current();
    if realm == main {
        return top_url(js);
    }
    let frame = frame_of_realm(js, realm);
    let url = if frame.is_null() {
        ptr::null()
    } else {
        frame_url_attr(frame)
    };
    if url.is_null() || unsafe { *url } == 0 {
        ptr::null()
    } else {
        url
    }
}

pub(crate) fn realm_scope_enter(js: Option<Js>, realm: ContextHandle, scope: &mut RealmScope) {
    *scope = RealmScope::inactive();
    let Some(js) = js else {
        return;
    };
    if realm.is_null() || realm == js.ctx() {
        return;
    }
    if realm == js.main_realm() {
        let base = js.st().realm_scope_base.get();
        save(js, scope);
        js.set_ctx(realm);
        let base = unsafe { base.as_ref() };
        js.set_frame(base.map_or(ptr::null_mut(), |base| base.frame));
        js.set_current_doc(base.map_or_else(|| top_document(js.current_doc()), |base| base.doc));
        js.set_current_url_raw(unsafe { glib::g_strdup(top_url(js)) });
        scope.entered_url = unsafe { glib::g_strdup(js.current_url_raw()) };
        js.st().top_url_slot.set(ptr::null_mut());
        return;
    }
    let frame = frame_of_realm(js, realm);
    if frame.is_null() {
        return;
    }
    frame_scope_enter(js, realm, frame, scope);
}

pub(crate) fn realm_scope_leave(js: Option<Js>, scope: &mut RealmScope) {
    if !scope.active {
        return;
    }
    let Some(js) = js else {
        return;
    };
    let current = js.current_url_raw();
    if !scope.entered_url.is_null()
        && !scope.prev_slot.is_null()
        && unsafe { g_strcmp0(current, scope.entered_url) } != 0
    {
        unsafe {
            glib::g_free((*scope.prev_slot).cast());
            *scope.prev_slot = glib::g_strdup(non_null_text(current));
        }
    }
    unsafe { glib::g_free(scope.entered_url.cast()) };
    if scope.is_base {
        js.st().realm_scope_base.set(ptr::null_mut());
    }
    js.st().top_url_slot.set(scope.prev_slot);
    unsafe { glib::g_free(js.current_url_raw().cast()) };
    js.set_current_url_raw(scope.url);
    js.set_frame(scope.frame);
    js.set_current_doc(scope.doc);
    js.set_ctx(scope.ctx);
}

pub(crate) fn in_realm<R>(js: Option<Js>, realm: ContextHandle, f: impl FnOnce() -> R) -> R {
    let mut scope = RealmScope::inactive();
    realm_scope_enter(js, realm, &mut scope);
    let result = f();
    realm_scope_leave(js, &mut scope);
    result
}

pub(crate) unsafe fn realm_scope_push(js: Option<Js>, realm: ContextHandle, buf: *mut c_void) {
    let scope = buf.cast::<RealmScope>();
    unsafe {
        scope.write(RealmScope::inactive());
        realm_scope_enter(js, realm, &mut *scope);
    }
}

pub(crate) unsafe fn realm_scope_pop(js: Option<Js>, buf: *mut c_void) {
    unsafe { realm_scope_leave(js, &mut *buf.cast::<RealmScope>()) };
}

pub(crate) unsafe fn frame_scope_push(
    js: Js,
    realm: ContextHandle,
    frame: *mut NsNode,
    buf: *mut c_void,
) {
    let scope = buf.cast::<RealmScope>();
    unsafe {
        scope.write(RealmScope::inactive());
        frame_scope_enter(js, realm, frame, &mut *scope);
    }
}

pub(crate) fn realm_scope_new() -> *mut c_void {
    Box::into_raw(Box::new(RealmScope::inactive())).cast()
}

pub(crate) unsafe fn realm_scope_free(scope: *mut c_void) {
    drop(unsafe { Box::from_raw(scope.cast::<RealmScope>()) });
}

pub(crate) fn frame_url_push(js: Js, url: *const c_char) -> *mut c_void {
    let mut fu = Box::new(FrameUrl::new());
    frame_url_enter(js, &mut fu, url);
    Box::into_raw(fu).cast()
}

pub(crate) unsafe fn frame_url_pop(js: Js, token: *mut c_void) {
    if token.is_null() {
        return;
    }
    let mut fu = unsafe { Box::from_raw(token.cast::<FrameUrl>()) };
    frame_url_leave(js, &mut fu);
}

pub(crate) unsafe fn dispatch_scope_enter(
    js: Js,
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    buf: *mut c_void,
) {
    let scope = buf.cast::<DispatchScope>();
    unsafe {
        scope.write(DispatchScope {
            ctx: js.ctx(),
            doc: js.current_doc(),
            frame: js.frame(),
            entered: !frame.is_null(),
            url: FrameUrl::new(),
        });
    }
    js.set_ctx(ctx);
    if !frame.is_null() {
        js.set_current_doc(doc);
        js.set_frame(frame);
        frame_url_enter(js, unsafe { &mut (*scope).url }, frame_url_attr(frame));
    }
}

pub(crate) unsafe fn dispatch_scope_leave(js: Js, buf: *mut c_void) {
    let scope = unsafe { &mut *buf.cast::<DispatchScope>() };
    if scope.entered {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(scope.frame);
    js.set_current_doc(scope.doc);
    js.set_ctx(scope.ctx);
}

pub(crate) fn frame_load_enter(
    js: Js,
    frame: *mut NsNode,
    doc: *mut NsNode,
    url: *const c_char,
    realm_doc: Option<Value>,
) -> *mut c_void {
    let mut scope = Box::new(FrameLoadScope {
        url: FrameUrl::new(),
        doc: ptr::null_mut(),
        script: ptr::null_mut(),
        iframe_doc: None,
        frame: ptr::null_mut(),
    });
    frame_url_enter(js, &mut scope.url, url);
    scope.doc = js.current_doc();
    js.set_current_doc(doc);
    if !doc.is_null() {
        unsafe { ns_doc_id_index_build(doc) };
    }
    let state = js.st();
    scope.script = state.current_script.get();
    scope.iframe_doc = if realm_doc.is_some() {
        state.iframe_doc.replace(realm_doc)
    } else {
        state.iframe_doc.borrow().clone()
    };
    scope.frame = js.frame();
    js.set_frame(frame);
    state
        .iframe_load_depth
        .set(state.iframe_load_depth.get() + 1);
    Box::into_raw(scope).cast()
}

pub(crate) unsafe fn frame_load_leave(js: Js, token: *mut c_void) {
    let mut scope = unsafe { Box::from_raw(token.cast::<FrameLoadScope>()) };
    let state = js.st();
    state
        .iframe_load_depth
        .set(state.iframe_load_depth.get() - 1);
    js.set_frame(scope.frame);
    let previous_doc = scope.iframe_doc.take();
    drop(state.iframe_doc.replace(previous_doc));
    state.current_script.set(scope.script);
    js.set_current_doc(scope.doc);
    frame_url_leave(js, &mut scope.url);
}

pub(crate) fn realm_url_enter(js: Js, realm: ContextHandle) -> *mut c_void {
    let url = realm_url(js, realm);
    if url.is_null() || unsafe { *url } == 0 {
        return ptr::null_mut();
    }
    let copy = unsafe { glib::g_strdup(url) };
    let token = frame_url_push(js, copy);
    unsafe { glib::g_free(copy.cast()) };
    token
}

pub(crate) fn timer_scope_enter(js: Js, ctx: ContextHandle, frame: *mut NsNode) -> *mut c_void {
    let previous_ctx = js.ctx();
    let mut ctx = if ctx.is_null() { previous_ctx } else { ctx };
    if !frame.is_null() {
        let current_realm = node_realm_context(js, frame);
        if !current_realm.is_null() {
            ctx = current_realm;
        }
    }
    let mut scope = Box::new(TimerScope {
        js,
        ctx,
        previous_ctx,
        frame_scope: RealmScope::inactive(),
        budget: 0,
    });
    if frame.is_null() {
        js.set_ctx(ctx);
    } else {
        frame_scope_enter(js, ctx, frame, &mut scope.frame_scope);
    }
    scope.budget = budget::push(js);
    let state = js.st();
    state.callback_depth.set(state.callback_depth.get() + 1);
    Box::into_raw(scope).cast()
}

pub(crate) unsafe fn timer_scope_context(scope: *const c_void) -> ContextHandle {
    unsafe { (*scope.cast::<TimerScope>()).ctx }
}

pub(crate) unsafe fn timer_scope_take(scope: *mut c_void) -> Box<TimerScope> {
    unsafe { Box::from_raw(scope.cast::<TimerScope>()) }
}

impl TimerScope {
    pub fn js(&self) -> Js {
        self.js
    }

    pub fn ctx(&self) -> ContextHandle {
        self.ctx
    }

    pub fn budget(&self) -> i64 {
        self.budget
    }

    pub fn leave(&mut self) {
        let js = self.js;
        if self.frame_scope.active {
            realm_scope_leave(Some(js), &mut self.frame_scope);
        } else {
            js.set_ctx(self.previous_ctx);
        }
    }
}

pub(crate) fn raf_scope_enter(js: Js, ctx: ContextHandle, frame: *mut NsNode) -> *mut c_void {
    if !frame.is_null() && node_root(frame) != node_root(js.current_doc()) {
        return ptr::null_mut();
    }
    let current_realm = if frame.is_null() {
        ContextHandle::NULL
    } else {
        node_realm_context(js, frame)
    };
    let mut scope = Box::new(RafScope {
        js,
        previous_ctx: js.ctx(),
        previous_doc: js.current_doc(),
        previous_frame: js.frame(),
        frame,
        url: FrameUrl::new(),
    });
    let next_ctx = if !current_realm.is_null() {
        current_realm
    } else if !ctx.is_null() {
        ctx
    } else {
        js.ctx()
    };
    js.set_ctx(next_ctx);
    js.set_frame(frame);
    if !frame.is_null() {
        let frame_doc = unsafe { ns_iframe_document_node(frame) };
        if !frame_doc.is_null() {
            js.set_current_doc(frame_doc);
        }
        frame_url_enter(js, &mut scope.url, frame_url_attr(frame));
    }
    js.st()
        .eval_deadline_us
        .set(super::monotonic_us() + budget::eval_budget_us());
    Box::into_raw(scope).cast()
}

pub(crate) unsafe fn raf_scope_js(scope: *const c_void) -> Js {
    unsafe { (*scope.cast::<RafScope>()).js }
}

pub(crate) unsafe fn raf_scope_finish(scope: *mut c_void) {
    let mut scope = unsafe { Box::from_raw(scope.cast::<RafScope>()) };
    let js = scope.js;
    if !scope.frame.is_null() {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(scope.previous_frame);
    js.set_current_doc(scope.previous_doc);
    js.set_ctx(scope.previous_ctx);
}
