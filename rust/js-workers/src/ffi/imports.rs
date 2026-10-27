//! Southstar — the Rust-ABI entry points of the engine crates the messaging and worker bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

type Native = Result<Value, Value>;

pub(crate) use southstar_js_dispatch::nsr_js_dispatch_engine_event;
pub(crate) use southstar_js_dispatch::nsr_listener_parse_options;
pub(crate) use southstar_js_dispatch::nsr_listeners_compact_dead;
pub(crate) use southstar_js_dispatch::nsr_target_dispatch_with_event;
pub(crate) use southstar_js_dispatch::nsr_target_fire_event;
pub(crate) use southstar_js_dispatch::nsr_target_handler_realm;
pub(crate) use southstar_js_events::nsr_dom_exception;
pub(crate) use southstar_js_events::nsr_event_define_cancel_bubble;
pub(crate) use southstar_js_events::nsr_event_new;
pub(crate) use southstar_js_interfaces::nsr_install_namespace_object;
pub(crate) use southstar_js_net::nsr_fetch_install_interfaces;
pub(crate) use southstar_js_net::nsr_window_request_ctor;
pub(crate) use southstar_js_net::nsr_window_response_ctor;

unsafe extern "Rust" {
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
    pub(crate) safe fn nsr_window_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Native;
}
