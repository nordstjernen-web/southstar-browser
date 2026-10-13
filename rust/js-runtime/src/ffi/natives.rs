//! Southstar — the raw-QuickJS natives and installers the window and worker realms still bind; each moves to an nsr_ Rust declaration when its crate is converted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{JSContext, JSValue};

macro_rules! natives {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(
                pub(crate) fn $name(
                    ctx: *mut JSContext,
                    this_val: JSValue,
                    argc: c_int,
                    argv: *mut JSValue,
                ) -> JSValue;
            )*
        }
    };
}

macro_rules! installers {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(pub(crate) fn $name(ctx: *mut JSContext, target: JSValue);)*
        }
    };
}

pub(crate) use super::legacy::ns_illegal_constructor;

macro_rules! context_installers {
    ($($name:ident),* $(,)?) => {
        unsafe extern "C" {
            $(pub(crate) fn $name(ctx: *mut JSContext);)*
        }
    };
}

natives! {
    ns_services_alert,
    ns_services_set_timeout,
    ns_services_set_interval,
    ns_services_clear_timer,
    ns_services_camera_request,
    ns_services_camera_release,
    ns_services_mic_release,
    ns_services_camera_label,
    ns_services_camera_enumerate,
    ns_services_confirm,
    ns_services_prompt,
    ns_services_match_media,
    ns_services_request_idle_callback,
    ns_services_queue_microtask,
    ns_services_notification_ctor,
    ns_js_fetch,
    ns_mutation_observer_ctor,
    ns_mutation_observer_observe,
    ns_mutation_observer_disconnect,
    ns_mutation_observer_takeRecords,
    ns_intersection_observer_ctor,
    ns_intersection_observer_observe,
    ns_intersection_observer_unobserve,
    ns_intersection_observer_disconnect,
    ns_intersection_observer_takeRecords,
    ns_resize_observer_ctor,
    ns_resize_observer_observe,
    ns_resize_observer_unobserve,
    ns_resize_observer_disconnect,
    ns_perf_observer_ctor,
    ns_perf_observer_observe,
    ns_perf_observer_disconnect,
    ns_perf_observer_takeRecords,
    ns_window_addEventListener,
    ns_window_removeEventListener,
    ns_window_dispatchEvent,
    ns_window_find,
    ns_window_scroll_to,
    ns_window_scroll_by,
    ns_window_scroll_by_lines,
    ns_window_scroll_by_pages,
    ns_window_open_method,
    ns_window_requestAnimationFrame,
    ns_window_cancelAnimationFrame,
    ns_window_url_update_object,
    ns_window_mse_append,
    ns_window_mse_eos,
    ns_window_mse_buffered,
    ns_window_mse_buffered_start,
    ns_window_mse_remove,
    ns_window_mse_bytes,
    ns_media_source_is_type_supported,
    ns_window_btoa,
    ns_window_atob,
    ns_window_url_ctor,
    ns_window_url_can_parse,
    ns_window_url_parse_static,
    ns_window_url_create_object,
    ns_window_url_revoke_object,
    ns_ce_define,
    ns_ce_get,
    ns_ce_upgrade,
    ns_ce_when_defined,
    ns_ce_get_name,
    ns_window_image_ctor,
    ns_static_range_ctor,
    ns_vtt_cue_ctor,
    ns_window_audio_ctor,
    ns_window_document_fragment_ctor,
    ns_window_text_ctor,
    ns_window_comment_ctor,
    ns_window_option_ctor,
    ns_window_usp_ctor,
    ns_window_xhr_ctor,
    ns_window_dom_parser_ctor,
    ns_dom_parser_parseFromString,
    ns_window_abort_controller_ctor,
    ns_window_close_watcher_ctor,
    ns_cache_open,
    ns_returns_resolved_false,
    ns_returns_resolved_empty_array,
    ns_returns_resolved_undefined,
    ns_window_text_encoder_ctor,
    ns_window_text_decoder_ctor,
    ns_window_response_ctor,
    ns_window_request_ctor,
    ns_window_filereader_ctor,
    ns_window_event_ctor,
    ns_document_ctor,
    ns_window_get_selection,
    ns_window_structured_clone,
    ns_window_report_error,
    ns_target_dispatchEvent,
}

installers! {
    ns_services_install_console,
    ns_services_install_clipboard_item,
    ns_services_install_screen,
    ns_services_install_rtc,
    ns_window_bind_post_message,
    ns_perf_install_entry_list,
    ns_window_install_actions,
    ns_cssom_install_window,
    ns_js_input_install_wpt,
    ns_events_install_window_base,
    ns_window_install_history,
    ns_crypto_install_window,
    ns_webgl_install,
    ns_media_install_audio,
    ns_xhr_install_interface,
    ns_install_abort_signal_interface,
    ns_net_install_text_codecs,
    ns_fetch_install_interfaces,
    ns_events_install_window,
    ns_js_install_event_target,
    ns_window_install_browsing_context,
    ns_install_idle_deadline,
    ns_js_net_install_sockets,
    ns_cssom_install_css,
    ns_crypto_install_window_subtle,
    ns_storage_install_window,
    ns_events_install_worker,
    ns_net_install_interfaces,
    ns_idb_install,
    ns_install_event_attribute_getters,
    ns_crypto_install_worker,
}

#[cfg(feature = "wasm")]
installers! {
    ns_wasm_install,
}

context_installers! {
    ns_usp_install_interface,
    ns_url_install_interface,
    ns_js_name_engine_members,
}
