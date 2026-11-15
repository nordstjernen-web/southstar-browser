//! Southstar — what the Rust side of a build reports about itself on about:southstar.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const COMPILER: &str = match option_env!("NS_RUSTC_VERSION") {
    Some(version) => version,
    None => "unknown",
};

pub fn compiler_version() -> &'static str {
    COMPILER
}

pub fn minimum_version() -> &'static str {
    env!("CARGO_PKG_RUST_VERSION")
}

pub fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release (LTO)"
    }
}
