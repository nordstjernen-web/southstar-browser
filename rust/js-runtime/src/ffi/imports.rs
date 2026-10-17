//! Southstar — the Rust-ABI entry points of the binding crates that realm setup and teardown call back into, resolved at link time because those crates sit above this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_dom::{AttrView, NsNode};
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::NsJs;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_attr_to_js(
        scope: &mut Scope<'_>,
        owner: &Value,
        attr: AttrView<'_>,
        include_base: bool,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8>;
    pub(crate) safe fn nsr_events_install_window_base(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_events_install_window(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_events_install_worker(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_install_event_attribute_getters(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_structured_clone(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_canvas_install(scope: &mut Scope<'_>, global: &Value, window: bool);
    pub(crate) safe fn nsr_cssom_install_window(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_cssom_install_css(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_crypto_install_window(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_crypto_install_window_subtle(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_crypto_install_worker(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_static_range_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_get_selection(
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
    pub(crate) safe fn nsr_media_install_audio(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_make_abort_error(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_form_indexed_control(
        scope: &mut Scope<'_>,
        form: &Value,
        index: u32,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_form_named_control(
        scope: &mut Scope<'_>,
        form: &Value,
        name: &[u8],
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_js_reset_form(scope: &mut Scope<'_>, form: *mut NsNode);
    pub(crate) safe fn nsr_install_body_reflected_handlers(scope: &mut Scope<'_>, wrapper: &Value);
    pub(crate) unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
    pub(crate) safe fn nsr_js_promise_rejection_tracker(
        scope: &mut Scope<'_>,
        promise: &Value,
        reason: &Value,
        handled: bool,
    );
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
    pub(crate) safe fn nsr_target_handler_realm(
        scope: &mut Scope<'_>,
        obj: &Value,
        kind: &str,
        listener_key: &str,
    ) -> ContextHandle;
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
    pub(crate) safe fn nsr_element_isSameNode(
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
    pub(crate) safe fn nsr_js_install_event_target(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_register_class(scope: &mut Scope<'_>, which: c_int) -> Value;
    pub(crate) safe fn nsr_install_window_chrome(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_install_window_compat(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_install_proto_table(
        scope: &mut Scope<'_>,
        proto: &Value,
        table: c_int,
    );
    pub(crate) safe fn nsr_js_compile_module_cached(
        scope: &mut Scope<'_>,
        src: &[u8],
        name: &[u8],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_js_module_set_import_meta(
        scope: &mut Scope<'_>,
        module: &Value,
        is_main: bool,
    ) -> Result<(), Value>;
    pub(crate) safe fn nsr_js_module_normalize(
        scope: &mut Scope<'_>,
        base: &str,
        name: &str,
    ) -> Result<String, Value>;
    pub(crate) safe fn nsr_idb_install(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_module_load(
        scope: &mut Scope<'_>,
        name: &str,
        attributes: &Value,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_js_new_window_named(scope: &mut Scope<'_>) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_window_install_state(
        js: *const NsJs,
        scope: &mut Scope<'_>,
        global: &Value,
    );
    pub(crate) safe fn nsr_live_proto(scope: &mut Scope<'_>, which: c_int) -> Value;
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
    pub(crate) safe fn nsr_services_install_console(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_services_install_clipboard_item(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_services_install_screen(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_services_install_rtc(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_install_idle_deadline(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_services_window_navigator(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_services_worker_navigator(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_tag_owner_document(
        scope: &mut Scope<'_>,
        document: &Value,
        node: &Value,
    );
    pub(crate) safe fn nsr_webgl_install(scope: &mut Scope<'_>, global: &Value);
    #[cfg(feature = "webgpu")]
    pub(crate) safe fn nsr_webgpu_install(scope: &mut Scope<'_>, navigator: &Value);
    pub(crate) safe fn nsr_window_bind_post_message(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_find(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_install_actions(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_install_browsing_context(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_install_history(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_open_method(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    #[cfg(feature = "wasm")]
    pub(crate) safe fn nsr_wasm_install(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_worker_install_location(
        scope: &mut Scope<'_>,
        global: &Value,
        url: &str,
    );
    pub(crate) safe fn nsr_worker_promise_rejection_tracker(
        scope: &mut Scope<'_>,
        promise: &Value,
        reason: &Value,
        handled: bool,
    );
    pub(crate) safe fn nsr_js_fetch(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_xhr_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_abort_controller_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_response_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_request_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_xhr_install_interface(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_install_abort_signal_interface(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_fetch_install_interfaces(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_net_install_sockets(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_net_install_interfaces(scope: &mut Scope<'_>, global: &Value);
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
    pub(crate) safe fn nsr_url_install_interface(scope: &mut Scope<'_>);
    pub(crate) safe fn nsr_usp_install_interface(scope: &mut Scope<'_>);
    pub(crate) safe fn nsr_net_install_text_codecs(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_requestAnimationFrame(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_cancelAnimationFrame(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_storage_install_window(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_input_install_wpt(scope: &mut Scope<'_>, global: &Value);
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
    pub(crate) safe fn nsr_sw_install_container(scope: &mut Scope<'_>, target: &Value);
    pub(crate) safe fn nsr_worker_install_constructor(scope: &mut Scope<'_>, target: &Value);
    pub(crate) safe fn nsr_worker_install_console(scope: &mut Scope<'_>, target: &Value);
    pub(crate) safe fn nsr_sw_install_scope(scope: &mut Scope<'_>, target: &Value);
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
