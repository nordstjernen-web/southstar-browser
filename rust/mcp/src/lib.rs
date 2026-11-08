//! Southstar — remote control over MCP: the control file both ends share, the JSON they speak and the --mcp stdio server.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub mod json;
mod server;

use std::path::PathBuf;

pub use server::run;

pub const CONTROL_FILE: &str = "remote-control.json";

pub fn state_dir() -> PathBuf {
    let base =
        southstar_glib::path_from_bytes(&southstar_glib::user_state_dir()).unwrap_or_default();
    base.join("southstar")
}

pub fn control_file_path() -> PathBuf {
    state_dir().join(CONTROL_FILE)
}

pub fn process_alive(pid: i32) -> bool {
    pid > 0 && southstar_renderer_client::proc_info(pid).alive
}
