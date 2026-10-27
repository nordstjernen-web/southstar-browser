//! Southstar — the Rust-ABI entry points of the engine crates the messaging and worker bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

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

pub(crate) use southstar_js_dispatch::nsr_target_addEventListener;
pub(crate) use southstar_js_dispatch::nsr_target_dispatchEvent;
pub(crate) use southstar_js_dispatch::nsr_target_removeEventListener;
pub(crate) use southstar_js_events::nsr_event_composed_path;
pub(crate) use southstar_js_events::nsr_event_prevent_default;
pub(crate) use southstar_js_events::nsr_event_stop_immediate;
pub(crate) use southstar_js_events::nsr_event_stop_propagation;
pub(crate) use southstar_js_events::nsr_window_event_ctor;
pub(crate) use southstar_js_services::nsr_cache_open;
