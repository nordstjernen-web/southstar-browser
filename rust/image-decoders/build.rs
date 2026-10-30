//! Southstar — libavif's struct offsets and constants, probed from the installed headers for a plain cargo build; meson passes them in NS_AVIF_LAYOUT instead.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn main() {
    println!("cargo:rerun-if-changed=avif-layout.txt");
    println!("cargo:rerun-if-env-changed=NS_AVIF_LAYOUT");
    if std::env::var_os("NS_AVIF_LAYOUT").is_some() || southstar_build::meson_build() {
        return;
    }
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
