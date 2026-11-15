//! Southstar — the technical details on about:southstar: the build, the system, the library versions, the optional features compiled in and Google Safe Browsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::about::templates::VERSION;
use crate::ffi::host as sys;

pub const BUILD_DATE: &str = match option_env!("NS_BUILD_DATE") {
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

const FEATURES: [(&str, bool); 5] = [
    ("WebM video (VP8 / VP9 / Opus)", cfg!(feature = "libav")),
    ("AVIF images", cfg!(feature = "avif")),
    ("SVG images", true),
    ("Inline PDF viewer", cfg!(feature = "poppler")),
    ("Spell checking", cfg!(feature = "enchant")),
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
    let (class, label) = if on {
        ("on", "Included")
    } else {
        ("off", "Not included")
    };
    out.extend_from_slice(
        format!(
            "<div class=\"drow\"><span class=\"dk\">{key}</span><span class=\"dv {class}\">{label}</span></div>"
        )
        .as_bytes(),
    );
}

fn user_agent() -> Option<Vec<u8>> {
    let config = sys::unlocked_config();
    let config = config.as_deref();
    config
        .and_then(|c| c.user_agent.non_empty())
        .map(<[u8]>::to_vec)
        .or_else(|| sys::user_agent_for_mode(config.and_then(|c| c.compat_mode.bytes())))
}

fn build(s: &mut Vec<u8>) {
    s.extend_from_slice(b"<h3>Southstar</h3>");
    kv(s, "Version", Some(VERSION.as_bytes()));
    kv(s, "Build date", Some(BUILD_DATE.as_bytes()));
    let rust = sys::rust_info();
    kv(s, "Build profile", rust.profile.as_deref());
    let compiler = [&b"rustc "[..], rust.compiler.as_deref().unwrap_or_default()].concat();
    kv(s, "Compiler", Some(&compiler));
    kv(s, "Minimum Rust version", rust.minimum.as_deref());
    kv(s, "User agent", user_agent().as_deref());
}

fn system(s: &mut Vec<u8>) {
    s.extend_from_slice(b"<h3>System</h3>");
    let os = sys::os_info(c"PRETTY_NAME").or_else(|| sys::os_info(c"NAME"));
    kv(
        s,
        "Operating system",
        Some(os.as_deref().unwrap_or(PLATFORM.as_bytes())),
    );
    kv(s, "Architecture", Some(ARCH.as_bytes()));
    kv(
        s,
        "Logical CPUs",
        Some(sys::processors().to_string().as_bytes()),
    );
}

fn libraries(s: &mut Vec<u8>) {
    let v = sys::versions();
    s.extend_from_slice(b"<h3>Libraries</h3>");
    kv(s, "JavaScript engine", v.js_engine.as_deref());
    kv(s, "Networking", Some(&v.networking));
    kv(s, "TLS", Some(southstar_http::tls_description()));
    kv(s, "Web Cryptography", v.openssl.as_deref());
    if cfg!(feature = "libav") {
        kv(s, "Video (FFmpeg libav*)", v.libav.as_deref());
    }
    let [major, minor, micro] = v.glib;
    kv(
        s,
        "GLib",
        Some(format!("{major}.{minor}.{micro}").as_bytes()),
    );
    kv(s, "Cairo", v.cairo.as_deref());
    kv(s, "SQLite", v.sqlite.as_deref());
}

fn features(s: &mut Vec<u8>) {
    s.extend_from_slice(b"<h3>Features</h3>");
    feature(s, "WebGL (3D canvas)", true);
    if cfg!(feature = "webgpu") {
        let state = if sys::env_set("NS_WEBGPU_ALLOW") {
            "Enabled (--enable-webgpu)"
        } else {
            "Included \u{2014} start with --enable-webgpu"
        };
        kv(s, "WebGPU (experimental)", Some(state.as_bytes()));
    } else {
        feature(s, "WebGPU (experimental)", false);
    }
    for (key, on) in FEATURES {
        feature(s, key, on);
    }
    if cfg!(target_os = "linux") {
        feature(s, "Seccomp sandbox", cfg!(feature = "seccomp"));
    }
}

fn safe_browsing(s: &mut Vec<u8>) {
    let Some(rows) = crate::safety::status_rows() else {
        return;
    };
    s.extend_from_slice(b"<h3>Google Safe Browsing</h3>");
    for (key, value) in rows {
        let key = String::from_utf8_lossy(&sys::markup_escape(key.as_bytes())).into_owned();
        kv(s, &key, Some(value.as_bytes()));
    }
}

pub fn html() -> Vec<u8> {
    let mut s = b"<div class=\"diag\" id=\"diag\">".to_vec();
    build(&mut s);
    system(&mut s);
    libraries(&mut s);
    features(&mut s);
    safe_browsing(&mut s);
    s.extend_from_slice(b"</div>");
    s
}
