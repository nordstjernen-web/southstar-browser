//! Southstar — libavif's struct offsets and constants, probed from the installed headers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn main() {
    println!("cargo:rerun-if-changed=avif-layout.txt");
    if std::env::var_os("CARGO_FEATURE_AVIF").is_none() {
        return;
    }
    let Some(library) = southstar_build::pkg_config(&["libavif"]) else {
        return;
    };
    let list =
        std::fs::read_to_string("avif-layout.txt").expect("avif-layout.txt is part of the crate");
    if let Some(layout) = southstar_build::layout(&list, &library) {
        println!("cargo:rustc-env=NS_AVIF_LAYOUT={layout}");
    }
}
