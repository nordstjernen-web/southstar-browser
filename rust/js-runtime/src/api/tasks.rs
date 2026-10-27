//! Southstar — the page's scheduling for binding crates: draining microtasks and mutations, the timer gate, GLib sources on the page's context, eval budgets, pumped fetches and the orphan sweep.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_long, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::GBoolean;

use crate::ffi::glue;
use crate::ffi::{self, Js, NsJs, record};
use crate::{budget, orphans, scheduling, teardown};

pub use crate::ffi::glue::SourceFn;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResourceInfo {
    pub timeline: *const c_void,
    pub document_url: *const c_char,
    pub render_blocking: GBoolean,
    pub cors_mode: GBoolean,
    pub next_hop_protocol: *const c_char,
    pub timing_allow_origin: *const c_char,
    pub status: c_long,
    pub body_size: i64,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ResourceInfo>() == 56);

impl Default for ResourceInfo {
    fn default() -> ResourceInfo {
        ResourceInfo {
            timeline: ptr::null(),
            document_url: ptr::null(),
            render_blocking: 0,
            cors_mode: 0,
            next_hop_protocol: ptr::null(),
            timing_allow_origin: ptr::null(),
            status: 0,
            body_size: 0,
        }
    }
}

pub struct Upload<'a> {
    pub method: &'a CStr,
    pub body: &'a [u8],
    pub content_type: Option<&'a CStr>,
}

pub struct Fetched {
    pub response: Option<southstar_net::Response>,
    pub error: Option<CString>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TimerGate {
    Run,
    Wait,
    Drop,
}

fn each(js: *const NsJs, f: impl FnOnce(Js)) {
    if let Some(js) = Js::of(js) {
        f(js);
    }
}

pub fn drain_microtasks(js: *const NsJs) {
    each(js, scheduling::drain_microtasks);
}

pub fn drain_mutations(js: *const NsJs) {
    each(js, scheduling::drain_mutations);
}

pub fn microtask_checkpoint(js: *const NsJs) {
    each(js, scheduling::microtask_checkpoint);
}

pub fn due_timers_allowed(js: *const NsJs) -> bool {
    Js::of(js).is_some_and(scheduling::due_timers_allowed)
}

pub fn timer_gate(js: *const NsJs, frame: Option<Node<'_>>, idle_expired: bool) -> TimerGate {
    let Some(js) = Js::of(js) else {
        return TimerGate::Drop;
    };
    match scheduling::timer_gate(js, Node::ptr_or_null(frame).cast_mut(), idle_expired) {
        scheduling::TIMER_RUN => TimerGate::Run,
        scheduling::TIMER_WAIT => TimerGate::Wait,
        _ => TimerGate::Drop,
    }
}

pub fn dispatch_finish(js: *const NsJs) {
    each(js, scheduling::dispatch_finish);
}

pub fn drop_message_tasks(js: *const NsJs) {
    each(js, scheduling::drop_message_tasks);
}

unsafe extern "C" fn observer_tick(data: *mut c_void) -> GBoolean {
    let Some(js) = Js::of(data.cast()) else {
        return 0;
    };
    GBoolean::from(scheduling::observer_tick(js))
}

pub fn observer_schedule_tick(js: *const NsJs) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let state = js.st();
    if js.ctx().is_null()
        || !state.worker_host.get().is_null()
        || state.observer_tick_source.get() != 0
    {
        return;
    }
    state
        .observer_tick_source
        .set(glue::timeout_add(4, observer_tick, js));
}

pub fn process_pending_iframes(js: *const NsJs) {
    each(js, orphans::process_pending_iframes);
}

pub fn sweep_orphans(js: *const NsJs) {
    each(js, orphans::sweep);
}

pub fn unpin_subtree(js: *const NsJs, root: Node<'_>) {
    let root = record::node(root.as_ptr());
    if let (Some(js), Some(root)) = (Js::of(js), root) {
        orphans::unpin_subtree(js, root);
    }
}

pub fn purge_subtree_rafs(js: *const NsJs, root: Node<'_>) {
    record::purge_subtree_rafs(js, root.as_mut_ptr());
}

pub fn source_remove(js: *const NsJs, id: u32) {
    glue::source_remove(Js::of(js), id);
}

pub fn attach_timeout(js: *const NsJs, ms: u32, func: SourceFn, data: *mut c_void) -> u32 {
    glue::attach_timeout(Js::of(js), ms, func, data)
}

pub fn attach_idle(js: *const NsJs, func: SourceFn, data: *mut c_void) -> u32 {
    glue::attach_idle(Js::of(js), func, data)
}

pub fn budget_enter(js: *const NsJs) -> i64 {
    Js::of(js).map_or(0, budget::push)
}

pub fn budget_leave(js: *const NsJs, saved: i64) {
    each(js, |js| budget::pop(js, saved));
}

pub fn credit_pumped_time(js: *const NsJs, pump_start_us: i64) {
    each(js, |js| budget::credit_pumped_time(js, pump_start_us));
}

pub fn pump_iteration(js: *const NsJs) -> bool {
    let js = Js::of(js);
    if js.is_some_and(Js::halted) {
        return false;
    }
    glue::pump_iteration(js);
    true
}

pub fn frame_eval<R>(
    js: *const NsJs,
    script: Option<Node<'_>>,
    deadline_us: i64,
    f: impl FnOnce() -> R,
) -> R {
    let Some(js) = Js::of(js) else {
        return f();
    };
    let state = js.st();
    let script = match script {
        Some(script) => script.as_mut_ptr(),
        None => state.current_script.get(),
    };
    let previous = state.current_script.replace(script);
    state.eval_deadline_us.set(deadline_us);
    scheduling::add_depth(&state.eval_depth, 1);
    let result = f();
    let depth = scheduling::add_depth(&state.eval_depth, -1);
    state.eval_deadline_us.set(0);
    state.current_script.set(previous);
    if depth == 0 {
        scheduling::microtask_checkpoint(js);
        glue::finish_eval_depth(js);
    }
    result
}

fn fetch(
    js: Option<Js>,
    url: &CStr,
    top_url: Option<&CStr>,
    headers: &[&CStr],
    upload: Option<Upload<'_>>,
) -> Fetched {
    let headers: Vec<Vec<u8>> = headers
        .iter()
        .map(|header| header.to_bytes().to_vec())
        .collect();
    let req = southstar_net::FetchRequest {
        url: url.to_bytes(),
        top_url: top_url.map(CStr::to_bytes),
        method: Some(
            upload
                .as_ref()
                .map_or(&b"GET"[..], |upload| upload.method.to_bytes()),
        ),
        body: upload.as_ref().map(|upload| upload.body),
        content_type: upload
            .as_ref()
            .and_then(|upload| upload.content_type)
            .map(CStr::to_bytes),
        headers: &headers,
    };
    match glue::fetch_resource(js, &req) {
        Ok(response) => Fetched {
            response: Some(response),
            error: None,
        },
        Err(message) => Fetched {
            response: None,
            error: (!message.is_empty()).then(|| glue::c_line(&message)),
        },
    }
}

pub fn fetch_resource(
    js: *const NsJs,
    url: &CStr,
    top_url: Option<&CStr>,
    headers: &[&CStr],
    upload: Option<Upload<'_>>,
) -> Fetched {
    fetch(Js::of(js), url, top_url, headers, upload)
}

pub fn fetch_subresource(
    js: *const NsJs,
    url: &CStr,
    top_url: Option<&CStr>,
    headers: &[&CStr],
    initiator: Option<&CStr>,
    info: &ResourceInfo,
) -> Fetched {
    let start = ffi::monotonic_us();
    let fetched = fetch(Js::of(js), url, top_url, headers, None);
    if let Some(js) = Js::of(js)
        && js.st().worker_host.get().is_null()
        && let Some(initiator) = initiator
    {
        glue::add_resource_timing(js, info, url, initiator, start, fetched.response.as_ref());
    }
    fetched
}

pub fn eval_polyfills(js: *const NsJs) {
    record::eval_polyfills(js);
}

pub fn release_pinned_wrappers(js: *const NsJs) {
    each(js, teardown::release_pinned_wrappers);
}

pub fn storage_load_for(js: *const NsJs, url: Option<&CStr>) {
    record::storage_load_for(js, url);
}
