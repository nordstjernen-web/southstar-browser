//! Southstar — the C ABI of the interface brand checks, as declared in src/js_brand.h, and their Rust-ABI twins.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ContextHandle, Scope};

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_brand_node_interfaces(ctx: ContextHandle, element_cid: u32, attr_cid: u32) {
    ctx.with(|scope| crate::brand_node_interfaces(scope, element_cid, attr_cid));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_name_engine_members(ctx: ContextHandle) {
    ctx.with(crate::names::name_engine_members);
}

#[unsafe(no_mangle)]
pub fn nsr_name_engine_members(scope: &mut Scope<'_>) {
    crate::names::name_engine_members(scope);
}
