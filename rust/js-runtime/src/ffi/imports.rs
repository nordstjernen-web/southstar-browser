//! Southstar — the Rust-ABI entry points of the binding crates that realm setup and teardown call back into, resolved at link time because those crates sit above this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_dom::{NsAttr, NsNode};
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::NsJs;

unsafe extern "Rust" {
    pub(crate) unsafe fn nsr_attr_to_js(
        scope: &mut Scope<'_>,
        owner: &Value,
        attr: *const NsAttr,
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
    pub(crate) safe fn nsr_canvas_install(scope: &mut Scope<'_>, global: &Value, window: bool);
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
    pub(crate) safe fn nsr_live_owner_node(obj: &Value) -> *mut NsNode;
    pub(crate) safe fn nsr_live_proto(scope: &mut Scope<'_>, which: c_int) -> Value;
    pub(crate) unsafe fn nsr_make_performance_object(
        scope: &mut Scope<'_>,
        js: *const NsJs,
        include_memory: bool,
    ) -> Value;
    pub(crate) safe fn nsr_perf_supported_entry_types(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_performance_extend_event_target(
        scope: &mut Scope<'_>,
        global: &Value,
        performance: &Value,
    );
    pub(crate) unsafe fn nsr_realm_proto_for(
        js: *mut NsJs,
        realm: ContextHandle,
        proto: &Value,
    ) -> Value;
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
