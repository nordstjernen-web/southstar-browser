//! Southstar — when scripts run: the parser-blocking, deferred and async schedules of a task list, scripts inserted after the parse, and the deferred and async roots drained off a timer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{Kind, Node};

use crate::ffi::{self, Js, is_named};
use crate::{
    ALREADY_STARTED, EMPTY_SOURCE, FLAG_NOT_PARSER_INSERTED, Schedule, Task, fetch, hold,
    import_map, page, scan, type_is_module,
};

fn node_at<'a>(addr: usize) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(addr as *const southstar_dom::NsNode) }
}

fn origin_of(js: Js, root: Node) -> CString {
    js.base_url(root).unwrap_or_else(|| c"inline".to_owned())
}

pub(crate) fn run_schedule(js: Js, tasks: &[Task], which: Schedule, origin: Option<&CStr>) {
    for task in tasks.iter().filter(|task| task.schedule == which) {
        fetch::run_script_element(js, task.node, origin);
    }
    js.upgrade_all();
}

pub(crate) fn run_next(js: Js, tasks: &[Task], which: Schedule, origin: Option<&CStr>) -> bool {
    let Some(task) = tasks
        .iter()
        .find(|task| task.schedule == which && task.node.attr(ALREADY_STARTED).is_none())
    else {
        return false;
    };
    fetch::run_script_element(js, task.node, origin);
    js.upgrade_all();
    true
}

pub(crate) fn schedule_pending_drain(js: Js) {
    let wanted = page::peek_page(js, |page| {
        page.async_source == 0 && (!page.deferred_roots.is_empty() || !page.async_roots.is_empty())
    })
    .unwrap_or(false);
    if wanted {
        let source = js.attach_drain_timer();
        page::with_page(js, |page| page.async_source = source);
    }
}

pub(crate) fn drain_timer_fired(js: Js) {
    page::with_existing(js, |page| page.async_source = 0);
    if js.eval_depth() > 0 || js.in_pump() {
        schedule_pending_drain(js);
        return;
    }
    drain_deferred(js);
    drain_async_roots(js);
    schedule_pending_drain(js);
}

fn schedule_deferred_root(js: Js, root: Node) {
    let addr = root.as_ptr() as usize;
    page::with_page(js, |page| {
        if !page.deferred_roots.contains(&addr) {
            page.deferred_roots.push(addr);
        }
    });
    schedule_pending_drain(js);
}

fn schedule_async_root(js: Js, root: Node) {
    let addr = root.as_ptr() as usize;
    let added = page::with_page(js, |page| {
        if page.async_roots.contains(&addr) {
            return false;
        }
        page.async_roots.push(addr);
        true
    });
    if added {
        schedule_pending_drain(js);
    }
}

fn take_first(js: Js, deferred: bool) -> Option<usize> {
    page::with_existing(js, |page| {
        let roots = if deferred {
            &mut page.deferred_roots
        } else {
            &mut page.async_roots
        };
        (!roots.is_empty()).then(|| roots.remove(0))
    })
    .flatten()
}

fn root_count(js: Js, deferred: bool) -> usize {
    page::peek_page(js, |page| {
        if deferred {
            page.deferred_roots.len()
        } else {
            page.async_roots.len()
        }
    })
    .unwrap_or(0)
}

fn drain_async_roots(js: Js) {
    if js.halted() {
        return;
    }
    let mut scanned = root_count(js, false);
    while scanned > 0 && !js.halted() {
        scanned -= 1;
        let Some(root) = take_first(js, false) else {
            break;
        };
        let Some(root) = node_at(root).filter(|root| js.in_page(*root)) else {
            continue;
        };
        let origin = origin_of(js, root);
        import_map::register_in(js, root);
        let tasks = scan::collect_script_tasks(root);
        for which in [Schedule::Blocking, Schedule::Deferred, Schedule::Async] {
            run_schedule(js, &tasks, which, Some(&origin));
        }
        break;
    }
}

fn drain_deferred(js: Js) {
    let mut scanned = root_count(js, true);
    while scanned > 0 && !js.halted() {
        scanned -= 1;
        let Some(root) = take_first(js, true) else {
            break;
        };
        let Some(root) = node_at(root).filter(|root| js.in_page(*root)) else {
            continue;
        };
        let sheets = scan::pending_stylesheets(root);
        if !scan::has_pending_script(root) && sheets.is_empty() {
            continue;
        }
        let origin = origin_of(js, root);
        import_map::register_in(js, root);
        let tasks = scan::collect_script_tasks(root);
        run_schedule(js, &tasks, Schedule::Blocking, Some(&origin));
        run_schedule(js, &tasks, Schedule::Deferred, Some(&origin));
        if tasks.iter().any(|task| task.schedule == Schedule::Async) {
            schedule_async_root(js, root);
        }
        for sheet in sheets {
            fetch::load_stylesheet_element(js, sheet, Some(&origin));
        }
        break;
    }
}

pub(crate) fn drain_load_event_scripts(js: Js) {
    if !js.halted() && page::has_pending_roots(js) {
        drain_deferred(js);
        drain_async_roots(js);
        js.drain_microtasks();
    }
    if !page::has_pending_roots(js) {
        let source =
            page::with_existing(js, |page| core::mem::take(&mut page.async_source)).unwrap_or(0);
        if source != 0 {
            ffi::source_remove(source);
        }
    }
}

pub(crate) fn script_needs_prepare(js: Js, script: Node) {
    if !is_named(script, c"script")
        || script.flags() & FLAG_NOT_PARSER_INSERTED == 0
        || script.attr(EMPTY_SOURCE).is_none()
        || !js.in_page(script)
    {
        return;
    }
    crate::unmark(script, EMPTY_SOURCE);
    crate::unmark(script, ALREADY_STARTED);
    run_inserted_scripts(js, script);
}

pub(crate) fn run_inserted_scripts(js: Js, root: Node) {
    if js.current_doc().is_none() || js.halted() {
        return;
    }
    if let Some(parent) = root.parent().filter(|parent| is_named(*parent, c"script")) {
        script_needs_prepare(js, parent);
        if root.kind() != Kind::Element {
            return;
        }
    }
    js.rescan_images(root);
    if js.in_pump() || js.ce_upgrading() || !js.in_page(root) {
        return;
    }
    js.schedule_static_iframes(root);
    let sheets = scan::pending_stylesheets(root);
    if !scan::has_pending_script(root) && sheets.is_empty() {
        return;
    }
    let origin = origin_of(js, root);
    import_map::register_in(js, root);
    let tasks = scan::collect_script_tasks(root);
    let mut have_external = false;
    for task in &tasks {
        let parser_paused = hold::holding(js) && js.eval_depth() == 0 && js.callback_depth() == 0;
        if task.schedule == Schedule::Blocking
            && (task.node.attr(c"src").is_none() || parser_paused)
            && !type_is_module(task.node)
        {
            fetch::run_script_element(js, task.node, Some(&origin));
        } else {
            have_external = true;
        }
    }
    js.upgrade_all();
    if js.eval_depth() > 0 || js.callback_depth() > 0 || js.dispatch_depth() > 0 {
        if have_external || !sheets.is_empty() {
            schedule_deferred_root(js, root);
        }
        return;
    }
    if have_external {
        schedule_async_root(js, root);
    }
    for sheet in sheets {
        fetch::load_stylesheet_element(js, sheet, Some(&origin));
    }
}
