//! Southstar — links libsouthstar against the system libraries its enabled features need.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;

use southstar_build::{engine_link, hardening_link_args};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=include/libsouthstar.h");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let link = engine_link(&target_os, false);
    let args = link
        .engine
        .into_iter()
        .chain(link.system)
        .chain(hardening_link_args(&target_os));
    for arg in args {
        println!("cargo:rustc-cdylib-link-arg={arg}");
    }
}
