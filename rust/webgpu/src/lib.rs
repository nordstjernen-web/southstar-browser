//! Southstar — experimental WebGPU (navigator.gpu) over the external wgpu-native library, ported from webgpu.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod context;
mod encode;
mod ffi;
mod gpu;
mod install;
mod objects;
mod parse;
mod pipeline;
mod util;

pub fn install_runtime_hooks() {
    southstar_canvas::links::set_webgpu_links(southstar_canvas::links::WebgpuLinks {
        get_context: crate::context::get_context,
        canvas_surface: crate::ffi::host::canvas_surface,
    });
    southstar_js_runtime::links::set_webgpu_links(southstar_js_runtime::links::WebgpuLinks {
        webgpu_install: crate::install::install,
    });
}
