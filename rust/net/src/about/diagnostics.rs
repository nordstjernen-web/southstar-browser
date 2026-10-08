//! Southstar — the diagnostics card on about:southstar: the system, the versions of Southstar and its libraries, the Rust build and the optional features compiled in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::about::templates::VERSION;
use crate::ffi::host as sys;

const BUILD_DATE: &str = match option_env!("NS_BUILD_DATE") {
    Some(date) => date,
    None => "",
};

const PLATFORM: &str = if cfg!(windows) {
    "Windows"
} else if cfg!(target_os = "macos") {
    "macOS"
} else if cfg!(target_os = "linux") {
    "Linux"
} else {
    "Unknown"
};

const ARCH: &str = if cfg!(target_arch = "x86_64") {
    "x86-64"
} else if cfg!(target_arch = "aarch64") {
    "arm64"
} else if cfg!(target_arch = "x86") {
    "x86"
} else if cfg!(target_arch = "arm") {
    "arm"
} else {
    "unknown"
};

const FEATURES: [(&str, bool); 6] = [
    ("WebM video (VP8 / VP9 / Opus)", cfg!(feature = "libav")),
    ("AVIF images", cfg!(feature = "avif")),
    ("SVG images", true),
    ("Inline PDF viewer", cfg!(feature = "poppler")),
    ("Spell checking", cfg!(feature = "enchant")),
    ("Seccomp sandbox", cfg!(feature = "seccomp")),
];

fn kv(out: &mut Vec<u8>, key: &str, value: Option<&[u8]>) {
    let value = value
        .filter(|v| !v.is_empty())
        .unwrap_or("\u{2014}".as_bytes());
    out.extend_from_slice(b"<div class=\"drow\"><span class=\"dk\">");
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(b"</span><span class=\"dv\">");
    out.extend_from_slice(&sys::markup_escape(value));
    out.extend_from_slice(b"</span></div>");
}

fn feature(out: &mut Vec<u8>, key: &str, on: bool) {
    out.extend_from_slice(
        format!(
            "<div class=\"drow\"><span class=\"dk\">{key}</span><span class=\"dv {}\">{}</span></div>",
            if on { "on" } else { "off" },
            if on { "Enabled" } else { "Not built" }
        )
        .as_bytes(),
    );
}

fn user_agent() -> Option<Vec<u8>> {
    let config = sys::unlocked_config();
    config
        .and_then(|c| sys::config_text(c.user_agent))
        .filter(|ua| !ua.is_empty())
        .or_else(|| sys::user_agent_for_mode(config.map_or(core::ptr::null(), |c| c.compat_mode)))
}

pub fn html() -> Vec<u8> {
    let mut s = b"<div class=\"diag\">".to_vec();
    s.extend_from_slice(b"<h3>System</h3>");
    let os = sys::os_info(c"PRETTY_NAME").or_else(|| sys::os_info(c"NAME"));
    kv(
        &mut s,
        "Operating system",
        Some(os.as_deref().unwrap_or(PLATFORM.as_bytes())),
    );
    kv(&mut s, "Platform", Some(PLATFORM.as_bytes()));
    kv(&mut s, "Architecture", Some(ARCH.as_bytes()));
    kv(
        &mut s,
        "Logical CPUs",
        Some(sys::processors().to_string().as_bytes()),
    );

    let v = sys::versions();
    s.extend_from_slice(b"<h3>Version &amp; libraries</h3>");
    kv(
        &mut s,
        "Southstar",
        Some(format!("{VERSION} (built {BUILD_DATE})").as_bytes()),
    );
    kv(&mut s, "User agent", user_agent().as_deref());
    kv(&mut s, "JavaScript engine", v.js_engine.as_deref());
    if let Some(lexbor) = option_env!("NS_LEXBOR_VERSION") {
        kv(&mut s, "HTML / CSS (lexbor)", Some(lexbor.as_bytes()));
    }
    let [major, minor, micro] = v.glib;
    kv(
        &mut s,
        "GLib",
        Some(format!("{major}.{minor}.{micro}").as_bytes()),
    );
    kv(&mut s, "Pango", v.pango.as_deref());
    kv(&mut s, "Cairo", v.cairo.as_deref());
    kv(&mut s, "SQLite", v.sqlite.as_deref());
    let webp = format!(
        "{}.{}.{}",
        (v.webp >> 16) & 0xff,
        (v.webp >> 8) & 0xff,
        v.webp & 0xff
    );
    kv(&mut s, "libwebp", Some(webp.as_bytes()));
    if cfg!(feature = "libav") {
        kv(&mut s, "Video (FFmpeg libav*)", v.libav.as_deref());
    }
    kv(&mut s, "TLS / crypto", v.openssl.as_deref());
    kv(&mut s, "Networking", v.curl.as_deref());

    let rust = sys::rust_info();
    s.extend_from_slice(b"<h3>Rust</h3>");
    let compiler = [&b"rustc "[..], rust.compiler.as_deref().unwrap_or_default()].concat();
    kv(&mut s, "Compiler", Some(&compiler));
    kv(&mut s, "Minimum Rust version", rust.minimum.as_deref());
    kv(&mut s, "Build profile", rust.profile.as_deref());
    kv(
        &mut s,
        "Ported to Rust",
        Some(format!("{} C modules", rust.module_count).as_bytes()),
    );
    kv(&mut s, "Modules", rust.modules.as_deref());
    kv(
        &mut s,
        "JavaScript bindings",
        Some(b"Temporal, over the js-engine layer"),
    );

    s.extend_from_slice(b"<h3>Features</h3>");
    feature(&mut s, "WebGL (3D canvas)", true);
    if cfg!(feature = "webgpu") {
        let state = if sys::env_set(c"NS_WEBGPU_ALLOW") {
            "Enabled (--enable-webgpu)"
        } else {
            "Built \u{2014} start with --enable-webgpu"
        };
        kv(&mut s, "WebGPU (experimental)", Some(state.as_bytes()));
    } else {
        feature(&mut s, "WebGPU (experimental)", false);
    }
    for (key, on) in FEATURES {
        feature(&mut s, key, on);
    }
    s.extend_from_slice(b"</div>");
    s
}
