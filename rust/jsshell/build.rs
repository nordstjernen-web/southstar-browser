//! Southstar — links GLib, and the meson-built QuickJS-ng archive when the quickjs engine is selected, into southstar-jsshell.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=NS_QUICKJS_LIB_DIR");
    println!("cargo:rustc-link-lib=glib-2.0");
    if env::var_os("CARGO_FEATURE_QUICKJS").is_none() {
        return;
    }
    let dir = env::var_os("NS_QUICKJS_LIB_DIR").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("builddir")
                .join("src")
                .join("quickjs")
        },
        PathBuf::from,
    );
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static=qjs");
    if env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("unix") {
        println!("cargo:rustc-link-lib=m");
    }
}
