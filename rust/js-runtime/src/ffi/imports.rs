//! Southstar — the Rust-ABI entry points of the binding crates that realm setup and teardown call back into, resolved at link time because those crates sit above this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::NsJs;

pub(crate) use crate::links::nsr_attr_to_js;
pub(crate) use crate::links::nsr_blob_bytes;
pub(crate) use crate::links::nsr_canvas_install;
pub(crate) use crate::links::nsr_cssom_install_css;
pub(crate) use crate::links::nsr_cssom_install_window;
pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_element_isSameNode;
pub(crate) use crate::links::nsr_events_install_window;
pub(crate) use crate::links::nsr_events_install_window_base;
pub(crate) use crate::links::nsr_events_install_worker;
pub(crate) use crate::links::nsr_fetch_install_interfaces;
pub(crate) use crate::links::nsr_form_indexed_control;
pub(crate) use crate::links::nsr_form_named_control;
pub(crate) use crate::links::nsr_idb_install;
pub(crate) use crate::links::nsr_install_abort_signal_interface;
pub(crate) use crate::links::nsr_install_body_reflected_handlers;
pub(crate) use crate::links::nsr_install_event_attribute_getters;
pub(crate) use crate::links::nsr_install_idle_deadline;
pub(crate) use crate::links::nsr_install_window_chrome;
pub(crate) use crate::links::nsr_install_window_compat;
pub(crate) use crate::links::nsr_js_compile_module_cached;
pub(crate) use crate::links::nsr_js_fetch;
pub(crate) use crate::links::nsr_js_input_install_wpt;
pub(crate) use crate::links::nsr_js_install_event_target;
pub(crate) use crate::links::nsr_js_install_proto_table;
pub(crate) use crate::links::nsr_js_module_load;
pub(crate) use crate::links::nsr_js_module_normalize;
pub(crate) use crate::links::nsr_js_module_set_import_meta;
pub(crate) use crate::links::nsr_js_net_install_sockets;
pub(crate) use crate::links::nsr_js_new_window_named;
pub(crate) use crate::links::nsr_js_promise_rejection_tracker;
pub(crate) use crate::links::nsr_js_register_class;
pub(crate) use crate::links::nsr_live_proto;
pub(crate) use crate::links::nsr_make_abort_error;
pub(crate) use crate::links::nsr_media_install_audio;
pub(crate) use crate::links::nsr_net_install_interfaces;
pub(crate) use crate::links::nsr_net_install_text_codecs;
pub(crate) use crate::links::nsr_services_install_clipboard_item;
pub(crate) use crate::links::nsr_services_install_console;
pub(crate) use crate::links::nsr_services_install_rtc;
pub(crate) use crate::links::nsr_services_install_screen;
pub(crate) use crate::links::nsr_services_window_navigator;
pub(crate) use crate::links::nsr_services_worker_navigator;
pub(crate) use crate::links::nsr_static_range_ctor;
pub(crate) use crate::links::nsr_storage_install_window;
pub(crate) use crate::links::nsr_sw_install_container;
pub(crate) use crate::links::nsr_sw_install_scope;
pub(crate) use crate::links::nsr_tag_owner_document;
pub(crate) use crate::links::nsr_target_handler_realm;
pub(crate) use crate::links::nsr_url_install_interface;
pub(crate) use crate::links::nsr_usp_install_interface;
pub(crate) use crate::links::nsr_webgl_install;
pub(crate) use crate::links::nsr_window_abort_controller_ctor;
pub(crate) use crate::links::nsr_window_bind_post_message;
pub(crate) use crate::links::nsr_window_cancelAnimationFrame;
pub(crate) use crate::links::nsr_window_find;
pub(crate) use crate::links::nsr_window_get_selection;
pub(crate) use crate::links::nsr_window_install_actions;
pub(crate) use crate::links::nsr_window_install_browsing_context;
pub(crate) use crate::links::nsr_window_install_history;
pub(crate) use crate::links::nsr_window_open_method;
pub(crate) use crate::links::nsr_window_request_ctor;
pub(crate) use crate::links::nsr_window_requestAnimationFrame;
pub(crate) use crate::links::nsr_window_response_ctor;
pub(crate) use crate::links::nsr_window_structured_clone;
pub(crate) use crate::links::nsr_window_xhr_ctor;
pub(crate) use crate::links::nsr_worker_install_console;
pub(crate) use crate::links::nsr_worker_install_constructor;
pub(crate) use crate::links::nsr_worker_install_location;
pub(crate) use crate::links::nsr_worker_promise_rejection_tracker;
pub(crate) use crate::links::nsr_xhr_install_interface;
pub(crate) use southstar_js_crypto::nsr_crypto_install_window;
pub(crate) use southstar_js_crypto::nsr_crypto_install_window_subtle;
pub(crate) use southstar_js_crypto::nsr_crypto_install_worker;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_window_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_scroll_to(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_scroll_by(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_scroll_by_lines(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_scroll_by_pages(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_append(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_eos(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_buffered(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_buffered_start(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_remove(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_mse_bytes(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_media_source_is_type_supported(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_vtt_cue_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_js_reset_form(scope: &mut Scope<'_>, form: *mut NsNode);
    pub(crate) unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
    pub(crate) unsafe fn nsr_js_report_uncaught(
        js: *mut NsJs,
        exception: &Value,
        origin: Option<&str>,
    );
    pub(crate) unsafe fn nsr_target_report_exception(
        js: *mut NsJs,
        scope: &mut Scope<'_>,
        kind: Option<&str>,
        exception: &Value,
    );
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_addEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_removeEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_attr_cloneNode(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_window_addEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_window_removeEventListener(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_window_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[allow(non_snake_case)]
    pub(crate) safe fn nsr_target_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_report_error(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_window_install_state(
        js: *const NsJs,
        scope: &mut Scope<'_>,
        global: &Value,
    );
    pub(crate) safe fn nsr_mutation_observer_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_mutation_observer_observe(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_mutation_observer_disconnect(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_mutation_observer_take_records(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_intersection_observer_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_intersection_observer_observe(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_intersection_observer_unobserve(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_intersection_observer_disconnect(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_intersection_observer_take_records(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_resize_observer_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_resize_observer_observe(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_resize_observer_unobserve(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_resize_observer_disconnect(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_realm_proto_for(
        js: *mut NsJs,
        realm: ContextHandle,
        proto: &Value,
    ) -> Value;
    pub(crate) safe fn nsr_services_alert(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_set_timeout(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_set_interval(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_clear_timer(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_camera_request(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_camera_release(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_mic_release(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_camera_label(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_camera_enumerate(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_confirm(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_prompt(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_match_media(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_request_idle_callback(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_queue_microtask(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_services_notification_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_cache_open(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[cfg(feature = "webgpu")]
    pub(crate) safe fn nsr_webgpu_install(scope: &mut Scope<'_>, navigator: &Value);
    #[cfg(feature = "wasm")]
    pub(crate) safe fn nsr_wasm_install(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_btoa(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_atob(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_can_parse(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_parse_static(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_create_object(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_update_object(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_url_revoke_object(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_usp_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_text_encoder_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_text_decoder_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_filereader_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

unsafe extern "Rust" {
    pub(crate) safe fn nsr_window_form_data_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_custom_state_set_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_radio_node_list_get_value(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_radio_node_list_set_value(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

unsafe extern "Rust" {
    pub(crate) safe fn nsr_window_message_channel(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_broadcast_channel(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_global_post_message(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_global_close(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_import_scripts(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_report_error(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_performance_now(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_performance_entries(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_worker_performance_clear(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

macro_rules! rust_natives {
    ($($name:ident),* $(,)?) => {
        unsafe extern "Rust" {
            $(
                #[allow(non_snake_case)]
                pub(crate) safe fn $name(
                    scope: &mut Scope<'_>,
                    this: &Value,
                    args: &[Value],
                ) -> Result<Value, Value>;
            )*
        }
    };
}

rust_natives! {
    nsr_window_image_ctor,
    nsr_window_audio_ctor,
    nsr_window_document_fragment_ctor,
    nsr_window_text_ctor,
    nsr_window_comment_ctor,
    nsr_window_option_ctor,
    nsr_window_dom_parser_ctor,
    nsr_dom_parser_parseFromString,
    nsr_window_close_watcher_ctor,
    nsr_document_ctor,
}
