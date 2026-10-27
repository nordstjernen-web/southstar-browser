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

pub use ffi::{dump_stats, run_scripts_in_doc};

pub fn install_runtime_hooks() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        southstar_canvas::install_runtime_hooks();
        southstar_idb::install_runtime_hooks();
        southstar_js_attrs::install_runtime_hooks();
        southstar_js_clone::install_runtime_hooks();
        southstar_js_collections::install_runtime_hooks();
        southstar_js_cssom::install_runtime_hooks();
        southstar_js_custom_elements::install_runtime_hooks();
        southstar_js_dispatch::install_runtime_hooks();
        southstar_js_document::install_runtime_hooks();
        southstar_js_dom_factory::install_runtime_hooks();
        southstar_js_elements::install_runtime_hooks();
        southstar_js_events::install_runtime_hooks();
        southstar_js_focus::install_runtime_hooks();
        southstar_js_fonts::install_runtime_hooks();
        southstar_js_forms::install_runtime_hooks();
        southstar_js_frames::install_runtime_hooks();
        southstar_js_geometry::install_runtime_hooks();
        southstar_js_images::install_runtime_hooks();
        southstar_js_input::install_runtime_hooks();
        southstar_js_interfaces::install_runtime_hooks();
        southstar_js_lifecycle::install_runtime_hooks();
        southstar_js_loader::install_runtime_hooks();
        southstar_js_media::install_runtime_hooks();
        southstar_js_mutation::install_runtime_hooks();
        southstar_js_net::install_runtime_hooks();
        southstar_js_node::install_runtime_hooks();
        southstar_js_observers::install_runtime_hooks();
        southstar_js_perf::install_runtime_hooks();
        southstar_js_reflect::install_runtime_hooks();
        southstar_js_services::install_runtime_hooks();
        southstar_js_shadow::install_runtime_hooks();
        southstar_js_storage::install_runtime_hooks();
        southstar_js_top_layer::install_runtime_hooks();
        southstar_js_traversal::install_runtime_hooks();
        southstar_js_url::install_runtime_hooks();
        southstar_js_window::install_runtime_hooks();
        southstar_js_workers::install_runtime_hooks();
        southstar_webgl::install_runtime_hooks();
        #[cfg(feature = "webgpu")]
        southstar_webgpu::install_runtime_hooks();
    });
}
