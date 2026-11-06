//! Southstar xtask — the configure step: library version floors, required libraries and the optional features the installed libraries allow.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::env;
use std::path::PathBuf;
use std::process::Command;

use crate::options::{Options, Toggle};

const FLOORS: [(&str, &str); 6] = [
    ("glib-2.0", "2.80"),
    ("gobject-2.0", "2.80"),
    ("gio-2.0", "2.80"),
    ("cairo", "1.18"),
    ("harfbuzz", "8.3"),
    ("fontconfig", "2.15"),
];

const REQUIRED: [&str; 14] = [
    "gmodule-2.0",
    "cairo-ft",
    "freetype2",
    "epoxy",
    "libcrypto",
    "uchardet",
    "sqlite3",
    "libwebp",
    "libwebpdemux",
    "glib-2.0",
    "gio-2.0",
    "cairo",
    "harfbuzz",
    "fontconfig",
];

const LIBAV: [(&str, &str); 5] = [
    ("libavformat", "60"),
    ("libavcodec", "60"),
    ("libavutil", "58"),
    ("libswscale", "7"),
    ("libswresample", "4"),
];

fn pkg_config() -> String {
    env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".to_owned())
}

fn has(module: &str) -> bool {
    Command::new(pkg_config())
        .args(["--exists", module])
        .status()
        .is_ok_and(|status| status.success())
}

fn at_least(module: &str, version: &str) -> bool {
    Command::new(pkg_config())
        .args([&format!("--atleast-version={version}"), module])
        .status()
        .is_ok_and(|status| status.success())
}

fn version_of(module: &str) -> Option<String> {
    let output = Command::new(pkg_config())
        .args(["--modversion", module])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn gtk_floor(os: &str) -> &'static str {
    if os == "windows" { "4.22.1" } else { "4.14" }
}

fn check_floors(os: &str) -> Result<(), String> {
    let mut unmet = Vec::new();
    for (module, floor) in FLOORS.iter().copied().chain([("gtk4", gtk_floor(os))]) {
        if !at_least(module, floor) {
            let found = version_of(module).map_or_else(
                || "not found".to_owned(),
                |version| format!("found {version}"),
            );
            unmet.push(format!("  {module} >= {floor} ({found})"));
        }
    }
    let missing: Vec<String> = REQUIRED
        .iter()
        .chain(if os == "linux" {
            &["libseccomp"][..]
        } else {
            &[]
        })
        .filter(|module| !has(module))
        .map(|module| format!("  {module} (not found)"))
        .collect();
    unmet.extend(missing);
    if unmet.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Southstar needs system libraries this machine lacks:\n{}\nInstall their development packages; Ubuntu 24.04, Debian 13 and Fedora 40 are the oldest releases that ship all of them.",
        unmet.join("\n")
    ))
}

fn libav_available() -> bool {
    LIBAV.iter().all(|(module, floor)| at_least(module, floor))
}

fn wgpu_native(options: &Options) -> Option<PathBuf> {
    if has("wgpu_native") {
        return Some(PathBuf::new());
    }
    options
        .wgpu_native_root
        .clone()
        .or_else(|| env::var_os("WGPU_NATIVE_ROOT").map(PathBuf::from))
        .filter(|root| root.join("lib").is_dir())
}

pub struct Configuration {
    pub features: Vec<&'static str>,
    pub wgpu_native_root: Option<PathBuf>,
}

pub fn configure(options: &Options) -> Result<Configuration, String> {
    let os = env::consts::OS;
    check_floors(os)?;
    let mut features = vec!["font-fontconfig"];
    let mut summary = Vec::new();
    let libav = libav_available();
    if !libav && matches!(os, "linux" | "windows") {
        return Err(
            "FFmpeg 6.0 or newer (libavformat, libavcodec, libavutil, libswscale, libswresample) is required on Linux and Windows"
                .to_owned(),
        );
    }
    let mut enable = |name: &'static str, on: bool, label: &str| {
        if on {
            features.push(name);
        }
        summary.push(format!("  {label}: {}", if on { "yes" } else { "no" }));
    };
    enable("libav", libav, "WebM video and Opus/Vorbis audio (libav)");
    let sdl = options.audio != Toggle::Disabled && has("sdl2");
    if options.audio == Toggle::Enabled && !sdl {
        return Err("audio was requested but SDL2 is not installed".to_owned());
    }
    enable("sdl", sdl, "audio helper (SDL2)");
    enable(
        "vorbisfile",
        sdl && has("vorbisfile"),
        "native Vorbis (vorbisfile)",
    );
    enable("opusfile", sdl && has("opusfile"), "native Opus (opusfile)");
    let brotli = has("libbrotlidec");
    enable("http-brotli", brotli, "HTTP brotli (libbrotlidec)");
    enable("woff2", brotli, "WOFF2 fonts (libbrotlidec)");
    enable("http-zstd", has("libzstd"), "HTTP zstd (libzstd)");
    let avif = options.avif != Toggle::Disabled && has("libavif");
    if options.avif == Toggle::Enabled && !avif {
        return Err("AVIF was requested but libavif is not installed".to_owned());
    }
    enable("avif", avif, "AVIF images (libavif)");
    enable("poppler", has("poppler-glib"), "PDF viewing (poppler-glib)");
    enable("enchant", has("enchant-2"), "spell-checking (enchant-2)");
    enable("wasm", options.wasm, "WebAssembly (wasmi)");
    enable("rust-shaper", options.rust_shaper, "pure-Rust fonts");
    let wgpu_native_root = if options.webgpu == Toggle::Disabled {
        None
    } else {
        wgpu_native(options)
    };
    if options.webgpu == Toggle::Enabled && wgpu_native_root.is_none() {
        return Err("WebGPU was requested but wgpu-native is not installed: install its pkg-config file (wgpu_native) or pass --wgpu-native-root".to_owned());
    }
    enable(
        "webgpu",
        wgpu_native_root.is_some(),
        "WebGPU, experimental (wgpu-native)",
    );
    enable("launcher", os == "windows", "Windows bundle launcher");
    eprintln!("southstar: features\n{}", summary.join("\n"));
    Ok(Configuration {
        features,
        wgpu_native_root: wgpu_native_root.filter(|root| !root.as_os_str().is_empty()),
    })
}
