//! Southstar — the Rust-ABI entry points of the binding crates the attribute bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use southstar_js_collections::nsr_live_owner_node;
pub(crate) use southstar_js_collections::nsr_make_live;
pub(crate) use southstar_js_events::nsr_dom_exception;
pub(crate) use southstar_js_mutation::nsr_attribute_map_lookup;
pub(crate) use southstar_js_mutation::nsr_attribute_map_remember;
pub(crate) use southstar_js_reflect::nsr_element_reflect_str_get;
