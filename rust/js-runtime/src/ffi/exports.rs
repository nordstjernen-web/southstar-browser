//! Southstar — the GLib source callback that runs the page's queued message tasks, and the snapshot of the global object's own names.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ops::ControlFlow;

use super::Js;
use super::glue;
use crate::scheduling::{self, RunTasks};

pub(crate) fn schedule_message_tasks(js: Js, ms: u32) {
    let source = glue::attach_timeout(Some(js), ms, move || run_message_tasks(js));
    js.st().message_task_source.set(Some(source));
}

fn run_message_tasks(js: Js) -> ControlFlow<()> {
    match scheduling::run_message_tasks(js) {
        RunTasks::Done => ControlFlow::Break(()),
        RunTasks::Continue => ControlFlow::Continue(()),
        RunTasks::Retry => {
            schedule_message_tasks(js, 4);
            ControlFlow::Break(())
        }
    }
}

pub(crate) fn snapshot_globals(js: Js) -> Vec<Vec<u8>> {
    let names = js
        .scope(|scope| {
            let global = scope.global();
            let keys = scope.own_property_keys(&global, false).unwrap_or_default();
            keys.iter()
                .filter_map(|key| scope.to_bytes(key).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    glue::names_table(names)
}
