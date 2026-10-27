//! Southstar — minimalist WebGL 1 and 2: canvas.getContext mapped onto OpenGL ES, ported from webgl.c.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod args;
mod consts;
mod context;
pub(crate) mod ffi;
mod install;
mod limits;
mod methods;
mod methods2;
mod permission;
mod tables;

pub use permission::{set_decision, take_pending_origin};

pub fn install_runtime_hooks() {
    southstar_canvas::links::set_webgl_links(southstar_canvas::links::WebglLinks {
        webgl_get_context: crate::ffi::host::nsr_webgl_get_context,
    });
    southstar_js_runtime::links::set_webgl_links(southstar_js_runtime::links::WebglLinks {
        webgl_install: crate::ffi::host::nsr_webgl_install,
    });
}
