//! Southstar — the Rust-ABI entry points of the event, window, frame, console and worker bindings that dispatch calls, resolved at link time because those crates call back into this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

pub(crate) use southstar_js_services::nsr_services_console_emit;
pub(crate) use southstar_js_window::nsr_window_document_for;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value>;
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
    pub(crate) safe fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value);
    pub(crate) safe fn nsr_event_mark_default_prevented(scope: &mut Scope<'_>, event: &Value);
    pub(crate) safe fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str);
    pub(crate) safe fn nsr_event_define_source(scope: &mut Scope<'_>, event: &Value, source: Value);
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_worker_report_exception(js: *mut c_void, exception: &Value) -> bool;
}
