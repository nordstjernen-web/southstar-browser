//! Southstar — the Rust-ABI entry points of the binding crates the window bindings call, resolved at link time because those crates sit beside this one in a cyclic graph.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

use super::NsJs;

pub(crate) use crate::links::nsr_port_transfer_commit;
pub(crate) use crate::links::nsr_port_transfer_prepare;
pub(crate) use crate::links::nsr_target_dispatch_with_event;
pub(crate) use crate::links::nsr_target_make_event;
pub(crate) use southstar_js_clone::nsr_structured_clone_transfer;
pub(crate) use southstar_js_clone::nsr_window_structured_clone;

unsafe extern "Rust" {
    pub(crate) unsafe fn nsr_make_event(
        scope: &mut Scope<'_>,
        kind: &str,
        target: *const NsNode,
    ) -> Value;
    pub(crate) unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
}
