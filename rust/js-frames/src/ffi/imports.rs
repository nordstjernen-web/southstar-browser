//! Southstar — the Rust-ABI entry points of the engine crates the frame bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_void};

use southstar_dom::NsNode;
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::NsJs;

type Native = Result<Value, Value>;

macro_rules! natives {
    ($($name:ident),* $(,)?) => {
        unsafe extern "Rust" {
            $(
                #[allow(non_snake_case)]
                pub(crate) safe fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Native;
            )*
        }
    };
}

natives! {
    nsr_ce_define,
    nsr_ce_get,
    nsr_ce_upgrade,
    nsr_ce_when_defined,
    nsr_ce_get_name,
    nsr_window_addEventListener,
    nsr_window_removeEventListener,
    nsr_window_dispatchEvent,
    nsr_element_addEventListener,
    nsr_element_removeEventListener,
    nsr_element_dispatchEvent,
    nsr_services_set_timeout,
    nsr_services_set_interval,
    nsr_services_clear_timer,
    nsr_services_queue_microtask,
    nsr_window_requestAnimationFrame,
    nsr_window_cancelAnimationFrame,
}

unsafe extern "Rust" {
    pub(crate) safe fn nsr_window_bind_post_message(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_window_make_post_message(scope: &mut Scope<'_>, window: &Value)
    -> Value;
    pub(crate) unsafe fn nsr_window_link_outward(
        js: *const NsJs,
        outward: &Value,
        realm_window: &Value,
    );
    pub(crate) safe fn nsr_window_current_document_for(
        scope: &mut Scope<'_>,
        window: &Value,
    ) -> *mut NsNode;
    pub(crate) safe fn nsr_is_window_named(value: &Value) -> bool;
    pub(crate) safe fn nsr_js_new_window_named(scope: &mut Scope<'_>) -> Native;
    pub(crate) safe fn nsr_storage_area_of(value: &Value) -> Option<i32>;
    pub(crate) safe fn nsr_storage_new(scope: &mut Scope<'_>, area: i32) -> Native;
    pub(crate) unsafe fn nsr_js_navigator_brand(js: *const NsJs) -> Value;
    pub(crate) unsafe fn nsr_js_pristine_promise(js: *const NsJs) -> Value;
    pub(crate) unsafe fn nsr_js_frame_realm_finish(
        js: *mut NsJs,
        frame: ContextHandle,
        frame_global: &Value,
        parent_global: &Value,
    );
    pub(crate) unsafe fn nsr_js_frame_load_enter(
        js: *mut NsJs,
        frame: *mut NsNode,
        doc: *mut NsNode,
        url: *const c_char,
        realm_doc: &Value,
    ) -> *mut c_void;
    pub(crate) safe fn nsr_make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Native;
    pub(crate) safe fn nsr_unwrap_element(value: &Value) -> *const NsNode;
    pub(crate) unsafe fn nsr_make_realm_document(
        scope: &mut Scope<'_>,
        doc_node: *mut NsNode,
        url: Option<&[u8]>,
        charset: Option<&[u8]>,
        content_type: Option<&[u8]>,
        is_xml: bool,
        inert: bool,
    ) -> Value;
    pub(crate) safe fn nsr_realmdoc_deny_cookie(scope: &mut Scope<'_>, doc: &Value);
}
