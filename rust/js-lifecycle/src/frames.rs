//! Southstar — requestAnimationFrame, requestVideoFrameCallback and the animation-frame tick that runs their callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_void};

use southstar_dom::NsNode;
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::JSContext;
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, CallbackScope, Js, SOURCE_CONTINUE, SOURCE_REMOVE};
use crate::{Page, page, page_or_new, scrollend};

const FIRST_ID: i32 = 0x4000_0000;
const TICK_MS: c_uint = 16;
const FRAME_US: i64 = 16667;

struct Request {
    id: i32,
    realm: *mut JSContext,
    callback: Value,
    video_frame: bool,
    frame: *mut NsNode,
    media: *mut NsNode,
}

#[derive(Default)]
pub(crate) struct Frames {
    pending: Vec<Request>,
    requested: bool,
    next_id: i32,
    last_us: i64,
    tick_source: c_uint,
    host_driven: bool,
}

pub(crate) fn clear(page: &Page) {
    let pending = core::mem::take(&mut page.frames.borrow_mut().pending);
    drop(pending);
}

pub(crate) fn teardown(page: &Page) {
    clear(page);
    let source = core::mem::take(&mut page.frames.borrow_mut().tick_source);
    if source != 0 {
        ffi::source_remove_default(source);
    }
}

pub(crate) fn has_pending(js: Js) -> bool {
    page(js).is_some_and(|page| !page.frames.borrow().pending.is_empty())
}

pub(crate) fn count(js: Js) -> usize {
    page(js).map_or(0, |page| page.frames.borrow().pending.len())
}

pub(crate) fn set_host_driven(js: Js) {
    if !js.is_null() {
        page_or_new(js).frames.borrow_mut().host_driven = true;
    }
}

pub(crate) fn idle_frame_end(js: Js, now: i64, end: i64) -> i64 {
    let Some(page) = page(js) else {
        return end;
    };
    let frames = page.frames.borrow();
    if frames.pending.is_empty() {
        return end;
    }
    let base = if frames.last_us > 0 {
        frames.last_us
    } else {
        now
    };
    let frame = base + FRAME_US;
    if frame > now && frame < end {
        frame
    } else {
        end
    }
}

fn remove_where(page: &Page, mut doomed: impl FnMut(&Request) -> bool) {
    let removed: Vec<Request> = {
        let mut frames = page.frames.borrow_mut();
        let (removed, kept) = core::mem::take(&mut frames.pending)
            .into_iter()
            .partition(|request| doomed(request));
        frames.pending = kept;
        removed
    };
    drop(removed);
}

pub(crate) fn purge_frame(js: Js, frame: *const NsNode) {
    if frame.is_null() {
        return;
    }
    let Some(page) = page(js) else {
        return;
    };
    if !page.frames.borrow().requested {
        return;
    }
    remove_where(&page, |request| request.frame.cast_const() == frame);
    js.forget_frame_context(frame);
}

fn in_tree(node: *const NsNode, root: *const NsNode) -> bool {
    let mut current = ffi::node(node);
    while let Some(node) = current {
        if node.as_ptr() == root {
            return true;
        }
        current = node.parent();
    }
    false
}

pub(crate) fn purge_media(js: Js, root: *const NsNode) {
    if root.is_null() {
        return;
    }
    let Some(page) = page(js) else {
        return;
    };
    remove_where(&page, |request| {
        !request.media.is_null() && in_tree(request.media, root)
    });
}

unsafe extern "C" fn tick(data: *mut c_void) -> GBoolean {
    let js = Js::from_data(data);
    let Some(page) = page(js) else {
        return SOURCE_REMOVE;
    };
    let stop = || {
        page.frames.borrow_mut().tick_source = 0;
        SOURCE_REMOVE
    };
    if !js.has_ctx() || js.halted() {
        return stop();
    }
    if js.in_pump() || js.dispatch_depth() > 0 || ffi::in_blocking_fetch() {
        return SOURCE_CONTINUE;
    }
    {
        let frames = page.frames.borrow();
        if frames.pending.is_empty() || frames.host_driven {
            drop(frames);
            return stop();
        }
    }
    run(js);
    if !page.frames.borrow().pending.is_empty() {
        return SOURCE_CONTINUE;
    }
    stop()
}

fn schedule_tick(js: Js, page: &Page) {
    if !js.has_ctx() || js.is_worker() {
        return;
    }
    let mut frames = page.frames.borrow_mut();
    if frames.tick_source != 0 || frames.host_driven {
        return;
    }
    frames.tick_source = js.timeout_add(TICK_MS, tick);
}

fn request(
    scope: &mut Scope<'_>,
    js: Js,
    callback: &Value,
    video_frame: bool,
    media: *mut NsNode,
) -> i32 {
    let page = page_or_new(js);
    let frame = js.context_frame(scope);
    let realm = southstar_js_engine::quickjs::raw_context(scope);
    let id = {
        let mut frames = page.frames.borrow_mut();
        frames.next_id = frames.next_id.wrapping_add(1);
        let id = FIRST_ID.wrapping_add(frames.next_id);
        frames.requested = true;
        frames.pending.push(Request {
            id,
            realm,
            callback: callback.clone(),
            video_frame,
            frame,
            media,
        });
        id
    };
    schedule_tick(js, &page);
    id
}

fn callback_arg(scope: &mut Scope<'_>, js: Js, args: &[Value]) -> Option<Value> {
    if js.is_null() {
        return None;
    }
    let callback = args.first()?;
    scope.is_function(callback).then(|| callback.clone())
}

pub(crate) fn request_animation_frame(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(callback) = callback_arg(scope, js, args) else {
        return Ok(Value::int(0));
    };
    Ok(Value::int(request(
        scope,
        js,
        &callback,
        false,
        core::ptr::null_mut(),
    )))
}

pub(crate) fn request_video_frame_callback(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(callback) = callback_arg(scope, js, args) else {
        return Ok(Value::int(0));
    };
    let media = ffi::unwrap_element(this);
    Ok(Value::int(request(scope, js, &callback, true, media)))
}

pub(crate) fn cancel_animation_frame(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(arg) = args.first().filter(|_| !js.is_null()) else {
        return Ok(Value::undefined());
    };
    let id = scope.to_int32(arg).unwrap_or(0);
    let Some(page) = page(js) else {
        return Ok(Value::undefined());
    };
    let removed = {
        let mut frames = page.frames.borrow_mut();
        frames
            .pending
            .iter()
            .position(|request| request.id == id)
            .map(|index| frames.pending.remove(index))
    };
    drop(removed);
    Ok(Value::undefined())
}

fn number_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Value> {
    scope.get(object, key).ok().filter(Value::is_number)
}

fn video_frame_metadata(scope: &mut Scope<'_>, media: *mut NsNode, ts_ms: f64) -> Value {
    let mut media_time = 0.0;
    let mut width = 0;
    let mut height = 0;
    let mut presented_frames = 0;
    let element = if media.is_null() {
        Value::undefined()
    } else {
        ffi::make_element(scope, media)
    };
    if element.is_object() {
        if let Some(value) = number_prop(scope, &element, "_nd_pos") {
            media_time = scope.to_number(&value).unwrap_or(0.0);
        }
        if let Some(value) = number_prop(scope, &element, "videoWidth") {
            width = scope.to_int32(&value).unwrap_or(0);
        }
        if let Some(value) = number_prop(scope, &element, "videoHeight") {
            height = scope.to_int32(&value).unwrap_or(0);
        }
        if let Some(value) = number_prop(scope, &element, "_nd_presented_frames") {
            presented_frames = scope.to_int32(&value).unwrap_or(0);
        }
        presented_frames = presented_frames.wrapping_add(1);
        let _ = scope.set(
            &element,
            "_nd_presented_frames",
            Value::int(presented_frames),
        );
    }
    let meta = scope.new_object();
    let _ = scope.set(&meta, "presentationTime", Value::number(ts_ms));
    let _ = scope.set(&meta, "expectedDisplayTime", Value::number(ts_ms));
    let _ = scope.set(&meta, "width", Value::int(width));
    let _ = scope.set(&meta, "height", Value::int(height));
    let _ = scope.set(&meta, "mediaTime", Value::number(media_time));
    let _ = scope.set(&meta, "presentedFrames", Value::int(presented_frames));
    meta
}

fn run_one(js: Js, request: Request, now_us: i64) {
    let Some(callback_scope) = CallbackScope::enter(js, request.realm, request.frame) else {
        return;
    };
    let ts_ms = js.time_origin_ms(callback_scope.context(), now_us);
    let message = callback_scope.run(|scope| {
        let arg = Value::number(ts_ms);
        let result = if request.video_frame {
            let meta = video_frame_metadata(scope, request.media, ts_ms);
            scope.call(&request.callback, &Value::undefined(), &[arg, meta])
        } else {
            scope.call(&request.callback, &Value::undefined(), &[arg])
        };
        match result {
            Ok(_) => None,
            Err(exception) => scope.to_string(&exception).ok(),
        }
    });
    if let Some(message) = message {
        js.log(&format!("JS error in requestAnimationFrame: {message}"));
    }
    drop(request);
    callback_scope.leave();
}

pub(crate) fn run(js: Js) -> bool {
    if js.halted() || js.in_pump() {
        return false;
    }
    scrollend::flush(js);
    js.flush_ready_images();
    js.flush_autofocus();
    js.drain_microtasks();
    js.promote_deferred_iframes();
    js.process_pending_iframes();
    js.drain_microtasks();
    let Some(page) = page(js).filter(|page| !page.frames.borrow().pending.is_empty()) else {
        return js.mutated();
    };
    let now_us = ffi::monotonic_us();
    let fired = {
        let mut frames = page.frames.borrow_mut();
        frames.last_us = now_us;
        core::mem::take(&mut frames.pending)
    };
    let saved = js.budget_enter();
    js.callback_depth_add(1);
    for request in fired {
        run_one(js, request, now_us);
    }
    js.callback_depth_add(-1);
    js.drain_mutations();
    js.budget_leave(saved);
    js.mutated()
}
