//! Southstar — the Rust-ABI entry points of the binding crates the node bindings call, resolved at link time because those crates sit beside this one.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) use southstar_js_attrs::nsr_attr_owner;
pub(crate) use southstar_js_collections::nsr_make_live;
pub(crate) use southstar_js_dom_factory::nsr_doc_wrapper_is_xml;
pub(crate) use southstar_js_dom_factory::nsr_validate_attr_ns;
pub(crate) use southstar_js_events::nsr_dom_exception;
pub(crate) use southstar_js_reflect::nsr_element_reflect_str_get;
