//! Southstar — creating and freeing a page's or a worker's JavaScript realm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod interrupt;
mod page;
mod teardown;
mod worker;

use southstar_dom::Node;
use southstar_js_engine::Scope;
use southstar_js_engine::quickjs;

pub(crate) type Element = Node<'static>;

const ENGINE_PRIVATE_NAMES: [&str; 24] = [
    "_listeners",
    "__ndAdoptWindowEventOps",
    "__ndEventTargetMethods",
    "__ndIsEngineFunction",
    "__ndDispatchPath",
    "__ndRealmProto",
    "__ndAdoptCss",
    "__ndBlobBytes",
    "__ndBlobType",
    "__ndFileName",
    "__ndFileMtime",
    "__ndHeaderMap",
    "__ndSetCookies",
    "__ndPairs",
    "__ndOwner",
    "__ndNotify",
    "__ndReady",
    "__ndSync",
    "__ndSetSearchRaw",
    "__nd",
    "__ndSP",
    "__ndMediaSourceObjectURL",
    "__ns_port_bridge",
    "__ns_broadcast_channels",
];

pub(crate) fn add_engine_private_names(scope: &mut Scope<'_>) {
    for name in ENGINE_PRIVATE_NAMES {
        quickjs::add_engine_private_name(scope, name);
    }
}

pub(crate) fn memory_cap_bytes(default_mb: i32, max_mb: Option<i32>) -> usize {
    let configured = southstar_config::get().map_or(default_mb, |config| config.js_memory_cap_mb);
    let mut mb = if configured <= 0 {
        default_mb
    } else {
        configured
    };
    if let Some(max_mb) = max_mb {
        mb = mb.min(max_mb);
    }
    mb as usize * 1024 * 1024
}
