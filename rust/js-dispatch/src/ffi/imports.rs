//! Southstar — the Rust-ABI entry points of the event, window, frame, console and worker bindings that dispatch calls, resolved at link time because those crates call back into this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_event_adopt_interface;
pub(crate) use crate::links::nsr_event_define_cancel_bubble;
pub(crate) use crate::links::nsr_event_define_source;
pub(crate) use crate::links::nsr_event_mark_default_prevented;
pub(crate) use crate::links::nsr_event_new;
pub(crate) use crate::links::nsr_worker_report_exception;
pub(crate) use southstar_js_services::nsr_services_console_emit;
pub(crate) use southstar_js_window::nsr_window_document_for;

unsafe extern "Rust" {
    pub(crate) unsafe fn nsr_make_event(
        scope: &mut Scope<'_>,
        kind: &str,
        target: *const NsNode,
    ) -> Value;
    pub(crate) safe fn nsr_event_prevent_default(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_event_stop_propagation(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_event_stop_immediate(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}
