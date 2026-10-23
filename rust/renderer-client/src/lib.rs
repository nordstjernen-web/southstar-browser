//! Southstar — the window's side of the renderer protocol: renderer processes, their framebuffer and the requests that drive them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod client;
mod ffi;
mod os;

pub use client::{Favicon, Frame, Page, Renderer, Text, Tick, TilesRequest, View, Wheel};
pub use ffi::{PrintFn, PrintSetup, PrintedPages, exchange_dir};
pub use os::{AttachFn, ProcInfo, Remote, proc_cpu, proc_info, proc_threads, self_pid};

pub fn set_inproc(attach: Option<AttachFn>) {
    os::set_attach(attach);
}

pub fn set_inproc_print(print: Option<PrintFn>) {
    ffi::set_print(print);
}
