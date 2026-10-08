//! Southstar — the window's side of the renderer protocol: renderer processes, their framebuffer and the requests that drive them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod client;
mod ffi;
mod os;

pub use ffi::PrintFn;
pub use os::AttachFn;

pub fn set_inproc(attach: Option<AttachFn>) {
    os::set_attach(attach);
}

pub fn set_inproc_print(print: Option<PrintFn>) {
    ffi::ns_rproc_http_set_inproc_print(print);
}
