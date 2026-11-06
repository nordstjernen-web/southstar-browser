//! Southstar — FFmpeg's struct offsets and constants, probed from the installed headers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn main() {
    println!("cargo:rerun-if-changed=layout.txt");
    let Some(library) = southstar_build::pkg_config(&[
        "libavformat",
        "libavcodec",
        "libavutil",
        "libswscale",
        "libswresample",
    ]) else {
        return;
    };
    let list = std::fs::read_to_string("layout.txt").expect("layout.txt is part of the crate");
    if let Some(layout) = southstar_build::layout(&list, &library) {
        println!("cargo:rustc-env=NS_AV_LAYOUT={layout}");
    }
}
