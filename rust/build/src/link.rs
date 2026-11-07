//! Southstar build — the system libraries the engine, the GTK shell and the media helpers link, by enabled cargo feature and target.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::path::PathBuf;

use crate::{link_args, pkg_config, require};

const ENGINE_LIBRARIES: [&str; 15] = [
    "glib-2.0",
    "gobject-2.0",
    "gio-2.0",
    "gmodule-2.0",
    "cairo",
    "cairo-ft",
    "harfbuzz",
    "fontconfig",
    "freetype2",
    "epoxy",
    "libcrypto",
    "uchardet",
    "sqlite3",
    "libwebp",
    "libwebpdemux",
];

const FEATURE_LIBRARIES: [(&str, &[&str]); 7] = [
    (
        "LIBAV",
        &[
            "libavformat",
            "libavcodec",
            "libavutil",
            "libswscale",
            "libswresample",
        ],
    ),
    ("SDL", &["sdl2"]),
    ("AVIF", &["libavif"]),
    ("POPPLER", &["poppler-glib"]),
    ("ENCHANT", &["enchant-2"]),
    ("VORBISFILE", &["vorbisfile"]),
    ("OPUSFILE", &["opusfile"]),
];

pub struct EngineLink {
    pub engine: Vec<String>,
    pub shell: Vec<String>,
    pub system: Vec<String>,
}

pub fn feature_enabled(name: &str) -> bool {
    env::var_os(format!("CARGO_FEATURE_{name}")).is_some()
}

fn wgpu_native_args() -> Vec<String> {
    println!("cargo:rerun-if-env-changed=WGPU_NATIVE_ROOT");
    if let Some(library) = pkg_config(&["wgpu_native"]) {
        return link_args(&library);
    }
    let root = env::var_os("WGPU_NATIVE_ROOT").map(PathBuf::from).unwrap_or_else(|| {
        panic!("southstar: the webgpu feature needs wgpu-native: install its pkg-config file (wgpu_native) or set WGPU_NATIVE_ROOT to an extracted release")
    });
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib").display()
    );
    ["-lwgpu_native", "-ldl", "-lm", "-lpthread"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect()
}

pub fn engine_link(target_os: &str, gtk: bool) -> EngineLink {
    let mut modules = ENGINE_LIBRARIES.to_vec();
    if target_os == "linux" {
        modules.push("libseccomp");
    }
    for (name, extra) in FEATURE_LIBRARIES {
        if feature_enabled(name) {
            modules.extend(extra);
        }
    }
    let mut engine: Vec<String> = link_args(&require(&modules))
        .into_iter()
        .filter(|arg| arg != "-lSDL2main" && arg != "-lmingw32")
        .collect();
    if feature_enabled("WEBGPU") {
        engine.extend(wgpu_native_args());
    }
    let shell = if gtk {
        link_args(&require(&["gtk4"]))
            .into_iter()
            .filter(|arg| !engine.contains(arg))
            .collect()
    } else {
        Vec::new()
    };
    let system: &[&str] = match target_os {
        "windows" => &[
            "-Wl,--stack,8388608",
            "-lcrypt32",
            "-lbcrypt",
            "-ladvapi32",
            "-lshell32",
            "-luser32",
        ],
        "macos" => &["-framework", "OpenGL", "-framework", "AppKit", "-lobjc"],
        _ => &["-lrt", "-lm"],
    };
    EngineLink {
        engine,
        shell,
        system: system.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

pub fn hardening_link_args(target_os: &str) -> Vec<String> {
    match target_os {
        "linux" | "freebsd" | "netbsd" | "openbsd" | "dragonfly" => [
            "-Wl,-z,relro",
            "-Wl,-z,now",
            "-Wl,-z,noexecstack",
            "-Wl,-z,separate-code",
            "-Wl,--gc-sections",
        ]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect(),
        _ => Vec::new(),
    }
}
