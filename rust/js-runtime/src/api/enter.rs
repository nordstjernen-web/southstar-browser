//! Southstar — entering the page's realms, frames, dispatches and frame loads for binding crates, and the scopes a frame URL, a timer, an animation frame and an event handler's realm hold until they are left.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};

use crate::ffi::{Js, NsJs, glue, scopes};
use crate::{budget, scheduling};

#[must_use]
pub struct UrlScope {
    js: Js,
    url: Box<scopes::FrameUrl>,
}

#[must_use]
pub struct TimerScope(Box<scopes::TimerScope>);

#[must_use]
pub struct RafScope(Box<scopes::RafScope>);

#[must_use]
pub struct HandlerRealm(Box<scopes::RealmScope>);

pub fn in_realm<R>(js: *const NsJs, realm: ContextHandle, f: impl FnOnce() -> R) -> R {
    scopes::in_realm(Js::of(js), realm, f)
}

pub fn in_main_realm<R>(js: *const NsJs, f: impl FnOnce() -> R) -> R {
    let js = Js::of(js);
    let realm = js.map_or(ContextHandle::NULL, Js::main_realm);
    scopes::in_realm(js, realm, f)
}

pub fn in_frame_scope<R>(
    js: *const NsJs,
    realm: ContextHandle,
    frame: Node<'_>,
    f: impl FnOnce() -> R,
) -> R {
    let Some(js) = Js::of(js) else {
        return f();
    };
    let mut scope = scopes::RealmScope::inactive();
    scopes::frame_scope_enter(js, realm, frame.as_mut_ptr(), &mut scope);
    let result = f();
    scopes::realm_scope_leave(Some(js), &mut scope);
    result
}

pub fn in_dispatch_scope<R>(
    js: *const NsJs,
    ctx: ContextHandle,
    frame: Option<(Node<'_>, Node<'_>)>,
    f: impl FnOnce() -> R,
) -> R {
    let Some(js) = Js::of(js) else {
        return f();
    };
    let (doc, frame) = frame.map_or((ptr::null_mut(), ptr::null_mut()), |(doc, frame)| {
        (doc.as_mut_ptr(), frame.as_mut_ptr())
    });
    scopes::in_dispatch(js, ctx, doc, frame, f)
}

pub fn during_frame_load<R>(
    js: *const NsJs,
    frame: Node<'_>,
    doc: Node<'_>,
    url: &CStr,
    realm_doc: &Value,
    f: impl FnOnce() -> R,
) -> R {
    let Some(js) = Js::of(js) else {
        return f();
    };
    let realm_doc = realm_doc.is_object().then(|| realm_doc.clone());
    let mut scope = scopes::frame_load_begin(
        js,
        frame.as_mut_ptr(),
        doc.as_mut_ptr(),
        Some(url),
        realm_doc,
    );
    let result = f();
    scopes::frame_load_end(js, &mut scope);
    result
}

pub fn enter_frame_url(js: *const NsJs, url: Option<&CStr>) -> Option<UrlScope> {
    let js = Js::of(js)?;
    let url = scopes::frame_url_begin(js, url);
    Some(UrlScope { js, url })
}

pub fn enter_realm_url(js: *const NsJs, realm: ContextHandle) -> Option<UrlScope> {
    let js = Js::of(js)?;
    let url = scopes::realm_url_begin(js, realm)?;
    Some(UrlScope { js, url })
}

impl UrlScope {
    pub fn leave(mut self) {
        scopes::frame_url_end(self.js, &mut self.url);
    }
}

pub fn enter_timer_scope(
    js: *const NsJs,
    ctx: ContextHandle,
    frame: Option<Node<'_>>,
) -> Option<TimerScope> {
    let frame = Node::ptr_or_null(frame).cast_mut();
    Some(TimerScope(scopes::timer_scope_begin(
        Js::of(js)?,
        ctx,
        frame,
    )))
}

impl TimerScope {
    pub fn context(&self) -> ContextHandle {
        self.0.ctx()
    }

    pub fn leave(mut self, exception: Option<Value>) {
        finish_timer(&mut self.0, exception);
    }
}

fn finish_timer(scope: &mut scopes::TimerScope, exception: Option<Value>) {
    let js = scope.js();
    let ctx = scope.ctx();
    scheduling::add_depth(&js.st().callback_depth, -1);
    budget::pop(js, scope.budget());
    if let Some(exception) = exception {
        let (message, stack) = glue::exception_text_in(ctx, &exception);
        if let Some(message) = message
            && js.st().log.is_set()
        {
            let mut line = b"JS error in timer: ".to_vec();
            line.extend_from_slice(&message);
            if let Some(stack) = stack {
                line.push(b'\n');
                line.extend_from_slice(&stack);
            }
            let line = glue::c_line(&line);
            glue::log_line(js, line.as_ptr());
        }
        glue::report_uncaught(js, &exception, js.current_url().as_deref());
    }
    scheduling::drain_mutations(js);
    scope.leave();
}

pub fn enter_raf_scope(
    js: *const NsJs,
    ctx: ContextHandle,
    frame: Option<Node<'_>>,
) -> Option<RafScope> {
    let frame = Node::ptr_or_null(frame).cast_mut();
    scopes::raf_scope_begin(Js::of(js)?, ctx, frame).map(RafScope)
}

impl RafScope {
    pub fn context(&self) -> ContextHandle {
        self.0.js().ctx()
    }

    pub fn leave(mut self) {
        scheduling::drain_microtasks(self.0.js());
        scopes::raf_scope_end(&mut self.0);
    }
}

pub fn enter_handler_realm(scope: &mut Scope<'_>, obj: &Value, kind: &str) -> HandlerRealm {
    let mut entered = Box::new(scopes::RealmScope::inactive());
    let realm = glue::handler_realm(scope, obj, kind);
    scopes::realm_scope_enter(Js::of_scope(scope), realm, &mut entered);
    HandlerRealm(entered)
}

impl HandlerRealm {
    pub fn leave(mut self, scope: &Scope<'_>) {
        scopes::realm_scope_leave(Js::of_scope(scope), &mut self.0);
    }
}
