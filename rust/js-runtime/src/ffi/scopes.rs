//! Southstar — entering and leaving a realm, a frame's URL, a dispatch, a frame load, a timer and an animation frame, with the top-level URL slot they share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Kind, Node, WeakNode};
use southstar_js_engine::{ContextHandle, Value};

use super::Js;
use super::record::{live, weak};
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

#[derive(Clone, Copy)]
pub(crate) struct BaseFrame {
    doc: Option<WeakNode>,
    frame: Option<WeakNode>,
}

pub(crate) struct RealmScope {
    ctx: ContextHandle,
    doc: Option<WeakNode>,
    frame: Option<WeakNode>,
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
            doc: None,
            frame: None,
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
    doc: Option<WeakNode>,
    frame: Option<WeakNode>,
    entered: bool,
    url: FrameUrl,
}

impl DispatchScope {
    fn inactive() -> DispatchScope {
        DispatchScope {
            ctx: ContextHandle::NULL,
            doc: None,
            frame: None,
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
    previous_doc: Option<WeakNode>,
    previous_frame: Option<WeakNode>,
    frame: Option<WeakNode>,
    url: FrameUrl,
}

pub(crate) struct FrameLoadScope {
    url: FrameUrl,
    doc: Option<WeakNode>,
    script: Option<WeakNode>,
    iframe_doc: Option<Value>,
    frame: Option<WeakNode>,
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

fn frame_url_attr<'a>(frame: Option<Node<'a>>) -> Option<&'a CStr> {
    frame.and_then(|frame| frame.attr(FRAME_URL_ATTR))
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

pub(crate) fn top_document(doc: Option<Node<'_>>) -> Option<Node<'_>> {
    let mut current = doc;
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
                None => return None,
            }
        }
    }
    current
}

pub(crate) fn node_root(node: Option<Node<'_>>) -> Option<Node<'_>> {
    node.map(southstar_dom::tree::root)
}

pub(crate) fn frame_of_realm(js: Js, realm: ContextHandle) -> Option<Node<'static>> {
    (crate::api::frames_hooks().frame_of_realm)(js.page(), realm)
}

pub(crate) fn node_realm_context(js: Js, node: Option<Node<'_>>) -> ContextHandle {
    node.map_or(ContextHandle::NULL, |node| {
        (crate::api::frames_hooks().node_realm_context)(js.page(), node)
    })
}

fn iframe_document_node(frame: Option<Node<'_>>) -> Option<Node<'_>> {
    frame.and_then(crate::api::frames_hooks().document_node)
}

fn save(js: Js, scope: &mut RealmScope) {
    scope.ctx = js.ctx();
    scope.doc = weak(js.current_doc());
    scope.frame = weak(js.frame());
    scope.url = Some(save_current_url(js));
    scope.entered_url = None;
    scope.prev_slot = js.st().top_url_slot.borrow().clone();
    scope.is_base = false;
    scope.active = true;
}

pub(crate) fn frame_scope_enter(
    js: Js,
    realm: ContextHandle,
    frame: Option<Node<'_>>,
    scope: &mut RealmScope,
) {
    save(js, scope);
    let state = js.st();
    if state.realm_scope_base.get().is_none() && js.ctx() == js.main_realm() {
        state.realm_scope_base.set(Some(BaseFrame {
            doc: scope.doc,
            frame: scope.frame,
        }));
        scope.is_base = true;
    }
    if state.top_url_slot.borrow().is_none() {
        state.top_url_slot.replace(scope.url.clone());
    }
    js.set_ctx(realm);
    if let Some(frame_doc) = iframe_document_node(frame) {
        js.set_current_doc(Some(frame_doc));
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
    let frame = frame_of_realm(js, realm)?;
    frame_url_attr(Some(frame))
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
        js.set_frame(base.and_then(|base| live(base.frame)));
        js.set_current_doc(
            base.map_or_else(|| top_document(js.current_doc()), |base| live(base.doc)),
        );
        let top = top_url(js);
        scope.entered_url = Some(top.clone());
        js.replace_current_url(Some(top));
        js.st().top_url_slot.replace(None);
        return;
    }
    let Some(frame) = frame_of_realm(js, realm) else {
        return;
    };
    frame_scope_enter(js, realm, Some(frame), scope);
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
        js.st().realm_scope_base.set(None);
    }
    js.st().top_url_slot.replace(scope.prev_slot.take());
    restore_current_url(js, scope.url.as_ref());
    js.set_frame(live(scope.frame));
    js.set_current_doc(live(scope.doc));
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
    doc: Option<Node<'_>>,
    frame: Option<Node<'_>>,
    scope: &mut DispatchScope,
) {
    *scope = DispatchScope {
        ctx: js.ctx(),
        doc: weak(js.current_doc()),
        frame: weak(js.frame()),
        entered: frame.is_some(),
        url: FrameUrl::new(),
    };
    js.set_ctx(ctx);
    if frame.is_some() {
        js.set_current_doc(doc);
        js.set_frame(frame);
        frame_url_enter(js, &mut scope.url, frame_url_attr(frame));
    }
}

fn dispatch_leave(js: Js, scope: &mut DispatchScope) {
    if scope.entered {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(live(scope.frame));
    js.set_current_doc(live(scope.doc));
    js.set_ctx(scope.ctx);
}

pub(crate) fn in_dispatch<R>(
    js: Js,
    ctx: ContextHandle,
    doc: Option<Node<'_>>,
    frame: Option<Node<'_>>,
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
    frame: Node<'_>,
    doc: Node<'_>,
    url: Option<&CStr>,
    realm_doc: Option<Value>,
) -> Box<FrameLoadScope> {
    let mut scope = Box::new(FrameLoadScope {
        url: FrameUrl::new(),
        doc: None,
        script: None,
        iframe_doc: None,
        frame: None,
    });
    frame_url_enter(js, &mut scope.url, url);
    scope.doc = weak(js.current_doc());
    js.set_current_doc(Some(doc));
    southstar_dom::index::id_build(doc);
    let state = js.st();
    scope.script = state.current_script.get();
    scope.iframe_doc = if realm_doc.is_some() {
        state.iframe_doc.replace(realm_doc)
    } else {
        state.iframe_doc.borrow().clone()
    };
    scope.frame = weak(js.frame());
    js.set_frame(Some(frame));
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
    js.set_frame(live(scope.frame));
    let previous_doc = scope.iframe_doc.take();
    drop(state.iframe_doc.replace(previous_doc));
    state.current_script.set(scope.script);
    js.set_current_doc(live(scope.doc));
    frame_url_leave(js, &mut scope.url);
}

pub(crate) fn realm_url_begin(js: Js, realm: ContextHandle) -> Option<Box<FrameUrl>> {
    let url = realm_url(js, realm).filter(|url| !url.is_empty())?;
    Some(frame_url_begin(js, Some(&url)))
}

pub(crate) fn timer_scope_begin(
    js: Js,
    ctx: ContextHandle,
    frame: Option<Node<'_>>,
) -> Box<TimerScope> {
    let previous_ctx = js.ctx();
    let mut ctx = if ctx.is_null() { previous_ctx } else { ctx };
    if frame.is_some() {
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
    if frame.is_none() {
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
    frame: Option<Node<'_>>,
) -> Option<Box<RafScope>> {
    if frame.is_some() && node_root(frame) != node_root(js.current_doc()) {
        return None;
    }
    let current_realm = node_realm_context(js, frame);
    let mut scope = Box::new(RafScope {
        js,
        previous_ctx: js.ctx(),
        previous_doc: weak(js.current_doc()),
        previous_frame: weak(js.frame()),
        frame: weak(frame),
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
    if frame.is_some() {
        if let Some(frame_doc) = iframe_document_node(frame) {
            js.set_current_doc(Some(frame_doc));
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
    if scope.frame.is_some() {
        frame_url_leave(js, &mut scope.url);
    }
    js.set_frame(live(scope.previous_frame));
    js.set_current_doc(live(scope.previous_doc));
    js.set_ctx(scope.previous_ctx);
}
