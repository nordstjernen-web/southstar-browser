//! Southstar — the Rust API other crates call to collect, schedule and run a page's scripts and to evaluate source in it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_js_runtime::api::Page;

use crate::ffi::{Js, PerfInfo};
use crate::{Schedule, Task, compile, eval, fetch, hold, import_map, page, scan, schedule};

#[derive(Clone, Default)]
pub struct ScriptTasks(Vec<Task<'static>>);

impl ScriptTasks {
    pub fn nodes(&self) -> Vec<Node<'static>> {
        self.0.iter().map(|task| task.node).collect()
    }
}

fn live(js: Option<Page>) -> Option<Js> {
    let js = Js::of(js);
    (!js.is_null()).then_some(js)
}

pub fn collect_script_tasks(root: Node<'_>) -> ScriptTasks {
    ScriptTasks(
        scan::collect_script_tasks(root)
            .into_iter()
            .filter_map(|task| {
                Some(Task {
                    node: task.node.downgrade().get()?,
                    schedule: task.schedule,
                })
            })
            .collect(),
    )
}

pub fn run_parser_blocking_scripts(js: Option<Page>, tasks: &ScriptTasks, origin: Option<&CStr>) {
    if let Some(js) = live(js) {
        hold::run_parser_blocking_scripts(js, &tasks.0, origin);
    }
}

pub fn run_script_schedule(
    js: Option<Page>,
    tasks: &ScriptTasks,
    which: Schedule,
    origin: Option<&CStr>,
) {
    if let Some(js) = live(js) {
        schedule::run_schedule(js, &tasks.0, which, origin);
    }
}

pub fn set_lifecycle_tasks(js: Option<Page>, tasks: Option<ScriptTasks>) {
    if let Some(js) = live(js) {
        page::with_page(js, |page| page.lifecycle_tasks = tasks.map(|tasks| tasks.0));
    }
}

pub fn has_lifecycle_tasks(js: Option<Page>) -> bool {
    live(js).is_some_and(|js| {
        page::peek_page(js, |page| page.lifecycle_tasks.is_some()).unwrap_or(false)
    })
}

pub fn run_next_lifecycle_schedule(
    js: Option<Page>,
    which: Schedule,
    origin: Option<&CStr>,
) -> bool {
    let Some(js) = live(js) else {
        return false;
    };
    let tasks = page::peek_page(js, |page| page.lifecycle_tasks.clone())
        .flatten()
        .unwrap_or_default();
    schedule::run_next(js, &tasks, which, origin)
}

pub fn register_import_maps(js: Option<Page>, root: Node<'_>) {
    if let Some(js) = live(js) {
        import_map::register_in(js, root);
    }
}

pub fn run_script_element(js: Option<Page>, script: Node<'_>, origin: Option<&CStr>) {
    if let Some(js) = live(js) {
        fetch::run_script_element(js, script, origin);
    }
}

pub fn mark_scripts_already_started(root: Node<'_>) {
    scan::mark_scripts_already_started(root);
}

pub fn script_type_is_module(script: Node<'_>) -> bool {
    crate::type_is_module(script)
}

pub fn script_type_supported(script: Node<'_>) -> bool {
    crate::type_supported(script)
}

pub fn script_skipped_by_nomodule(script: Node<'_>) -> bool {
    crate::skipped_by_nomodule(script)
}

pub fn schedule_pending_script_drain(js: Option<Page>) {
    if let Some(js) = live(js) {
        schedule::schedule_pending_drain(js);
    }
}

pub fn drain_load_event_scripts(js: Option<Page>) {
    if let Some(js) = live(js) {
        schedule::drain_load_event_scripts(js);
    }
}

pub fn has_pending_script_roots(js: Option<Page>) -> bool {
    live(js).is_some_and(page::has_pending_roots)
}

pub fn async_scripts_pending(js: Option<Page>) -> bool {
    live(js).is_some_and(page::async_pending)
}

pub fn cancel_async_scripts(js: Option<Page>) {
    if let Some(js) = live(js) {
        page::cancel_async(js);
    }
}

pub fn script_needs_prepare(js: Option<Page>, script: Node<'_>) {
    if let Some(js) = live(js) {
        schedule::script_needs_prepare(js, script);
    }
}

pub fn run_inserted_scripts(js: Option<Page>, root: Node<'_>) {
    if let Some(js) = live(js) {
        schedule::run_inserted_scripts(js, root);
    }
}

pub fn profile_enabled() -> bool {
    compile::profile_enabled()
}

pub fn run_content_scripts(js: Option<Page>, url: Option<&CStr>, document_start: bool) {
    eval::run_content_scripts(Js::of(js), url, document_start);
}

pub fn eval_source(js: Option<Page>, src: &CStr, origin: Option<&CStr>) -> Option<Vec<u8>> {
    eval::eval_source(live(js)?, src, origin)
}

pub fn decode_data_url(url: &CStr) -> Option<Vec<u8>> {
    crate::ffi::decode_data_url(url)
}

pub fn element_perf_info(js: Option<Page>, element: Option<Node<'_>>) -> PerfInfo {
    let mut info = PerfInfo::default();
    crate::ffi::fill_element_perf_info(js, element, &mut info);
    info
}
