//! Southstar — installing a parsed document into the page's realm and running its scripts, and the page's JavaScript statistics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod install;
mod interfaces;
mod run;
mod stats;

use std::sync::Once;

use southstar_dom::Node;

pub(crate) type Element = Node<'static>;

pub fn install_runtime_hooks() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        southstar_js_custom_elements::install_runtime_hooks();
        southstar_js_images::install_runtime_hooks();
        southstar_js_perf::install_runtime_hooks();
    });
}
