//! Southstar — entering and leaving a realm, a frame's URL, a dispatch, a frame load, a timer and an animation frame, with the top-level URL slot they share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use core::ptr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Kind, Node, NsNode};
use southstar_js_engine::{ContextHandle, Value};

use super::Js;
use super::state::UrlCell;
use crate::budget;

const FRAME_URL_ATTR: &CStr = c"data-nd-frame-url";

pub(crate) struct FrameUrl {
    saved: Option<UrlCell>,
    owns_slot: bool,
}

impl FrameUrl {
    pub fn new() -> FrameUrl {
        FrameUrl {
            saved: None,
            owns_slot: false,
        }
    }
}

pub(crate) struct RealmScope {
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    url: Option<UrlCell>,
    entered_url: Option<CString>,
    prev_slot: Option<UrlCell>,
    active: bool,
    is_base: bool,
}

impl RealmScope {
    pub fn inactive() -> RealmScope {
        RealmScope {
            ctx: ContextHandle::NULL,
            doc: ptr::null_mut(),
            frame: ptr::null_mut(),
            url: None,
            entered_url: None,
            prev_slot: None,
            active: false,
            is_base: false,
        }
    }
}

struct DispatchScope {
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    entered: bool,
    url: FrameUrl,
}

impl DispatchScope {
    fn inactive() -> DispatchScope {
        DispatchScope {
            ctx: ContextHandle::NULL,
            doc: ptr::null_mut(),
            frame: ptr::null_mut(),
            entered: false,
            url: FrameUrl::new(),
        }
    }
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

pub(crate) struct FrameLoadScope {
    url: FrameUrl,
    doc: *mut NsNode,
    script: *mut NsNode,
    iframe_doc: Option<Value>,
    frame: *mut NsNode,
}

fn with_top_slot<R>(js: Js, f: impl FnOnce(&mut Option<CString>) -> R) -> R {
    let slot = js.st().top_url_slot.borrow().clone();
    match slot {
        Some(slot) => f(&mut slot.borrow_mut()),
        None => f(&mut js.st().current_url.borrow_mut()),
    }
}

pub(crate) fn top_url(js: Js) -> CString {
    with_top_slot(js, |url| url.clone().unwrap_or_default())
}

pub(crate) fn set_top_url(js: Js, url: Option<&CStr>) {
    let copy = non_null_text(url);
    with_top_slot(js, |slot| *slot = Some(copy));
}

fn non_null_text(text: Option<&CStr>) -> CString {
    text.map_or_else(CString::default, CStr::to_owned)
}

fn frame_url_attr<'a>(frame: *const NsNode) -> Option<&'a CStr> {
    super::record::node(frame).and_then(|frame| frame.attr(FRAME_URL_ATTR))
}

fn save_current_url(js: Js) -> UrlCell {
    Rc::new(RefCell::new(js.current_url()))
}

fn restore_current_url(js: Js, saved: Option<&UrlCell>) {
    js.replace_current_url(saved.and_then(|saved| saved.borrow_mut().take()));
}

fn frame_url_enter(js: Js, fu: &mut FrameUrl, url: Option<&CStr>) {
    let url = non_null_text(url);
    let saved = save_current_url(js);
    fu.owns_slot = js.st().top_url_slot.borrow().is_none();
    if fu.owns_slot {
        js.st().top_url_slot.replace(Some(saved.clone()));
    }
    fu.saved = Some(saved);
    js.replace_current_url(Some(url));
}

fn frame_url_leave(js: Js, fu: &mut FrameUrl) {
    if fu.owns_slot {
        js.st().top_url_slot.replace(None);
    }
    restore_current_url(js, fu.saved.as_ref());
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
    Node::ptr_or_null(super::record::node(node).map(southstar_dom::tree::root))
}

pub(crate) fn frame_of_realm(js: Js, realm: ContextHandle) -> *mut NsNode {
    (crate::api::frames_hooks().frame_of_realm)(js.ptr(), realm)
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn node_realm_context(js: Js, node: *const NsNode) -> ContextHandle {
    super::record::node(node).map_or(ContextHandle::NULL, |node| {
        (crate::api::frames_hooks().node_realm_context)(js.ptr(), node)
    })
}

fn iframe_document_node(frame: *const NsNode) -> *mut NsNode {
    super::record::node(frame)
        .and_then(crate::api::frames_hooks().document_node)
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn save(js: Js, scope: &mut RealmScope) {
    scope.ctx = js.ctx();
    scope.doc = js.current_doc();
    scope.frame = js.frame();
    scope.url = Some(save_current_url(js));
    scope.entered_url = None;
    scope.prev_slot = js.st().top_url_slot.borrow().clone();
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
    if state.top_url_slot.borrow().is_none() {
        state.top_url_slot.replace(scope.url.clone());
    }
    js.set_ctx(realm);
    let frame_doc = iframe_document_node(frame);
    if !frame_doc.is_null() {
        js.set_current_doc(frame_doc);
    }
    js.set_frame(frame);
    js.replace_current_url(Some(non_null_text(frame_url_attr(frame))));
}

pub(crate) fn realm_url(js: Js, realm: ContextHandle) -> Option<CString> {
    if realm.is_null() {
        return None;
    }
    let main = js.main_or_current();
    if realm == main {
        return Some(top_url(js));
    }
    let frame = frame_of_realm(js, realm);
    if frame.is_null() {
        return None;
    }
    frame_url_attr(frame)
        .filter(|url| !url.is_empty())
        .map(CStr::to_owned)
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
        let top = top_url(js);
        scope.entered_url = Some(top.clone());
        js.replace_current_url(Some(top));
        js.st().top_url_slot.replace(None);
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
    let current = js.current_url();
    let entered_url = scope.entered_url.take();
    if let (Some(entered_url), Some(prev_slot)) = (entered_url, &scope.prev_slot)
        && current.as_ref() != Some(&entered_url)
    {
        *prev_slot.borrow_mut() = Some(non_null_text(current.as_deref()));
    }
    if scope.is_base {
        js.st().realm_scope_base.set(ptr::null_mut());
    }
    js.st().top_url_slot.replace(scope.prev_slot.take());
    restore_current_url(js, scope.url.as_ref());
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

pub(crate) fn frame_url_begin(js: Js, url: Option<&CStr>) -> Box<FrameUrl> {
    let mut fu = Box::new(FrameUrl::new());
    frame_url_enter(js, &mut fu, url);
    fu
}

pub(crate) fn frame_url_end(js: Js, fu: &mut FrameUrl) {
    frame_url_leave(js, fu);
}

fn dispatch_enter(
    js: Js,
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    scope: &mut DispatchScope,
) {
    *scope = DispatchScope {
        ctx: js.ctx(),
        doc: js.current_doc(),
        frame: js.frame(),
        entered: !frame.is_null(),
        url: FrameUrl::new(),
    };
    js.set_ctx(ctx);
    if !frame.is_null() {
        js.set_current_doc(doc);
        js.set_frame(frame);
        frame_url_enter(js, &mut scope.url, frame_url_attr(frame));
    }
}

fn dispatch_leave(js: Js, scope: &mut DispatchScope) {
    if scope.entered {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(scope.frame);
    js.set_current_doc(scope.doc);
    js.set_ctx(scope.ctx);
}

pub(crate) fn in_dispatch<R>(
    js: Js,
    ctx: ContextHandle,
    doc: *mut NsNode,
    frame: *mut NsNode,
    f: impl FnOnce() -> R,
) -> R {
    let mut scope = DispatchScope::inactive();
    dispatch_enter(js, ctx, doc, frame, &mut scope);
    let result = f();
    dispatch_leave(js, &mut scope);
    result
}

pub(crate) fn frame_load_begin(
    js: Js,
    frame: *mut NsNode,
    doc: *mut NsNode,
    url: Option<&CStr>,
    realm_doc: Option<Value>,
) -> Box<FrameLoadScope> {
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
    if let Some(doc) = super::record::node(doc) {
        southstar_dom::index::id_build(doc);
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
    scope
}

pub(crate) fn frame_load_end(js: Js, scope: &mut FrameLoadScope) {
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

pub(crate) fn realm_url_begin(js: Js, realm: ContextHandle) -> Option<Box<FrameUrl>> {
    let url = realm_url(js, realm).filter(|url| !url.is_empty())?;
    Some(frame_url_begin(js, Some(&url)))
}

pub(crate) fn timer_scope_begin(js: Js, ctx: ContextHandle, frame: *mut NsNode) -> Box<TimerScope> {
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
    scope
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

pub(crate) fn raf_scope_begin(
    js: Js,
    ctx: ContextHandle,
    frame: *mut NsNode,
) -> Option<Box<RafScope>> {
    if !frame.is_null() && node_root(frame) != node_root(js.current_doc()) {
        return None;
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
        let frame_doc = iframe_document_node(frame);
        if !frame_doc.is_null() {
            js.set_current_doc(frame_doc);
        }
        frame_url_enter(js, &mut scope.url, frame_url_attr(frame));
    }
    js.st()
        .eval_deadline_us
        .set(super::monotonic_us() + budget::eval_budget_us());
    Some(scope)
}

impl RafScope {
    pub fn js(&self) -> Js {
        self.js
    }
}

pub(crate) fn raf_scope_end(scope: &mut RafScope) {
    let js = scope.js;
    if !scope.frame.is_null() {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(scope.previous_frame);
    js.set_current_doc(scope.previous_doc);
    js.set_ctx(scope.previous_ctx);
}
