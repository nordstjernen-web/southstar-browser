//! Southstar — the Rust-ABI entry points of the event, dispatch, form, worker and URL crates that the networking bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

pub(crate) use southstar_js_url::nsr_blob_bytes;
pub(crate) use southstar_js_url::nsr_net_install_file_reader;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    pub(crate) safe fn nsr_make_abort_error(scope: &mut Scope<'_>) -> Value;
    pub(crate) safe fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value>;
    pub(crate) safe fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value);
    pub(crate) safe fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str);
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
    pub(crate) safe fn nsr_event_composed_path(
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
    pub(crate) safe fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str);
    pub(crate) safe fn nsr_xhr_fire_progress_event(
        scope: &mut Scope<'_>,
        target: &Value,
        kind: &str,
        loaded: f64,
        total: f64,
        length_computable: bool,
    );
    pub(crate) safe fn nsr_net_install_form_data(scope: &mut Scope<'_>, global: &Value);
    pub(crate) safe fn nsr_js_form_data_serialize(
        scope: &mut Scope<'_>,
        fd: &Value,
    ) -> (Vec<u8>, Vec<u8>);
    pub(crate) safe fn nsr_net_install_ports(scope: &mut Scope<'_>, global: &Value);
}
