//! Southstar — the Rust-ABI entry points of the event, dispatch, form, worker and URL crates that the networking bindings call, resolved at link time.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_event_adopt_interface;
pub(crate) use crate::links::nsr_event_define_cancel_bubble;
pub(crate) use crate::links::nsr_event_new;
pub(crate) use crate::links::nsr_js_form_data_serialize;
pub(crate) use crate::links::nsr_make_abort_error;
pub(crate) use crate::links::nsr_net_install_form_data;
pub(crate) use crate::links::nsr_net_install_ports;
pub(crate) use crate::links::nsr_target_fire_event;
pub(crate) use crate::links::nsr_xhr_fire_progress_event;
pub(crate) use southstar_js_url::nsr_blob_bytes;
pub(crate) use southstar_js_url::nsr_net_install_file_reader;

unsafe extern "Rust" {
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
}
