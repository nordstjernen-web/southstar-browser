//! Southstar — the Rust-ABI entry points of the binding crates the document factories call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::AttrView;
use southstar_js_engine::{Scope, Value};

pub(crate) use southstar_js_document::nsr_make_realm_document;
pub(crate) use southstar_js_document::nsr_make_synth_xml_document;
pub(crate) use southstar_js_events::nsr_dom_exception;
pub(crate) use southstar_js_events::nsr_event_new;
pub(crate) use southstar_js_reflect::nsr_element_reflect_str_get;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_attr_to_js(
        scope: &mut Scope<'_>,
        owner: &Value,
        attr: AttrView<'_>,
        include_base: bool,
    ) -> Result<Value, Value>;
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
    pub(crate) safe fn nsr_target_dispatchEvent(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}
