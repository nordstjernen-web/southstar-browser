//! Southstar — the Rust-ABI entry points of the engine crates the frame bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

use super::NsJs;

pub(crate) use crate::links::nsr_element_addEventListener;
pub(crate) use crate::links::nsr_element_dispatchEvent;
pub(crate) use crate::links::nsr_element_removeEventListener;
pub(crate) use crate::links::nsr_is_window_named;
pub(crate) use crate::links::nsr_js_new_window_named;
pub(crate) use crate::links::nsr_realmdoc_deny_cookie;
pub(crate) use crate::links::nsr_window_addEventListener;
pub(crate) use crate::links::nsr_window_bind_post_message;
pub(crate) use crate::links::nsr_window_cancelAnimationFrame;
pub(crate) use crate::links::nsr_window_current_document_for;
pub(crate) use crate::links::nsr_window_dispatchEvent;
pub(crate) use crate::links::nsr_window_make_post_message;
pub(crate) use crate::links::nsr_window_removeEventListener;
pub(crate) use crate::links::nsr_window_requestAnimationFrame;
pub(crate) use southstar_js_services::nsr_services_clear_timer;
pub(crate) use southstar_js_services::nsr_services_queue_microtask;
pub(crate) use southstar_js_services::nsr_services_set_interval;
pub(crate) use southstar_js_services::nsr_services_set_timeout;
pub(crate) use southstar_js_storage::nsr_storage_area_of;
pub(crate) use southstar_js_storage::nsr_storage_new;

unsafe extern "Rust" {
    pub(crate) unsafe fn nsr_window_link_outward(
        js: *const NsJs,
        outward: &Value,
        realm_window: &Value,
    );
    pub(crate) unsafe fn nsr_make_realm_document(
        scope: &mut Scope<'_>,
        doc_node: *mut NsNode,
        url: Option<&[u8]>,
        charset: Option<&[u8]>,
        content_type: Option<&[u8]>,
        is_xml: bool,
        inert: bool,
    ) -> Value;
}
