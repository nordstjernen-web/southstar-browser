//! Southstar — the Rust-ABI entry points of the engine crates the messaging and worker bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_void};

use southstar_js_engine::{ContextHandle, Scope, TaskFn, Value};

type Native = Result<Value, Value>;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_event_new(scope: &mut Scope<'_>) -> Native;
    pub(crate) safe fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value);
    pub(crate) safe fn nsr_target_handler_realm(
        scope: &mut Scope<'_>,
        obj: &Value,
        kind: &str,
        listener_key: &str,
    ) -> ContextHandle;
    pub(crate) safe fn nsr_target_dispatch_with_event(
        scope: &mut Scope<'_>,
        obj: &Value,
        kind: &str,
        ev: &Value,
    );
    pub(crate) safe fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str);
    pub(crate) safe fn nsr_js_dispatch_engine_event(
        scope: &mut Scope<'_>,
        target: &Value,
        ev: &Value,
    );
    pub(crate) safe fn nsr_listener_parse_options(
        scope: &mut Scope<'_>,
        opts: &Value,
        strict_signal: bool,
    ) -> Result<(bool, bool, bool, bool, Option<Value>), Value>;
    pub(crate) safe fn nsr_listeners_compact_dead(scope: &mut Scope<'_>, owner: &Value);
    pub(crate) safe fn nsr_queue_message_task(
        scope: &mut Scope<'_>,
        f: TaskFn,
        args: &[Value],
    ) -> Result<(), Value>;
    pub(crate) unsafe fn nsr_worker_js_eval(
        js: *mut c_void,
        source: &CStr,
        len: usize,
        url: &CStr,
        module: bool,
    ) -> Result<(), (bool, Value)>;
    pub(crate) safe fn nsr_js_exception_message(
        scope: &mut Scope<'_>,
        exception: &Value,
    ) -> Vec<u8>;
    pub(crate) safe fn nsr_bind_event_target_listeners(scope: &mut Scope<'_>, object: &Value);
    pub(crate) safe fn nsr_fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_install_namespace_object(
        scope: &mut Scope<'_>,
        global: &Value,
        name: &str,
        object: Value,
        tag: &str,
    );
    pub(crate) safe fn nsr_event_prevent_default(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_event_stop_propagation(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_event_stop_immediate(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_event_composed_path(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_addEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_removeEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_cache_open(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_illegal_constructor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_window_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_window_response_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
    pub(crate) safe fn nsr_window_request_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
}
