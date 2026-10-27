//! Southstar — the Rust-ABI entry points of the binding crates the selector queries and live collections call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use crate::links::nsr_dom_exception;
pub(crate) use crate::links::nsr_form_elements_named_lookup;
use southstar_dom::NsNode;
use southstar_js_engine::{Scope, Value};

unsafe extern "Rust" {
    pub(crate) unsafe fn nsr_live_build_labels(
        scope: &mut Scope<'_>,
        n: *const NsNode,
    ) -> Result<Value, Value>;
}
