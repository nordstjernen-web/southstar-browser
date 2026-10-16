//! Southstar — the GLib source callback that runs the page's queued message tasks, and the snapshot of the global object's own names.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_glib::{GBoolean, GHashTable};

use super::Js;
use super::glue;
use crate::scheduling::{self, RunTasks};

const SOURCE_REMOVE: GBoolean = 0;
const SOURCE_CONTINUE: GBoolean = 1;

pub(crate) unsafe extern "C" fn run_message_tasks(data: *mut c_void) -> GBoolean {
    let js = Js(data.cast());
    match scheduling::run_message_tasks(js) {
        RunTasks::Done => SOURCE_REMOVE,
        RunTasks::Continue => SOURCE_CONTINUE,
        RunTasks::Retry => {
            let source = glue::attach_timeout(Some(js), 4, run_message_tasks, data);
            js.st().message_task_source.set(source);
            SOURCE_REMOVE
        }
    }
}

pub(crate) fn snapshot_globals(js: Js) -> *mut GHashTable {
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
