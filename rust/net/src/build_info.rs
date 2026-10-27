//! Southstar — what the Rust side of a build reports about itself on about:southstar.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const COMPILER: &str = match option_env!("NS_RUSTC_VERSION") {
    Some(version) => version,
    None => "unknown",
};

const PORTED: &[&str] = &[
    "anim.c",
    "appmain.c",
    "audio/main.c",
    "audio/minimp3_impl.c",
    "bookmarks.c",
    "bytecode_cache.c",
    "cache.c",
    "camera.c",
    "config.c",
    "csp.c",
    "css.c",
    "css_media.c",
    "css_prop_syntax.c",
    "css_syntax.c",
    "datetime.c",
    "debuglog.c",
    "dom.c",
    "embed_shim.c",
    "engine.c",
    "eventsource.c",
    "ext.c",
    "font.c",
    "forms.c",
    "glctx.c",
    "headless.c",
    "history.c",
    "html.c",
    "i18n.c",
    "icons.c",
    "idb.c",
    "image.c",
    "image_avif.c",
    "image_ico.c",
    "image_webp.c",
    "ipc_http.c",
    "js.c",
    "js_brand.c",
    "js_canvas.c",
    "js_canvas_api.c",
    "js_date.c",
    "js_intl.c",
    "js_perf.c",
    "js_realm.c",
    "layout.c",
    "libsouthstar.c",
    "macos_dock.m",
    "mat4.h",
    "mathml.c",
    "mic.c",
    "net.c",
    "netutil.c",
    "pagelayers.c",
    "paint.c",
    "pdf.c",
    "print.c",
    "procview.c",
    "procwindow.c",
    "render.c",
    "renderer_http.c",
    "renderer_serve.c",
    "renderer_tiles.c",
    "rproc_http.c",
    "rproc_inproc.c",
    "safebrowsing.c",
    "security.c",
    "selection.c",
    "spellcheck.c",
    "svg.c",
    "texture.c",
    "threaddump.c",
    "titlerow.c",
    "video.c",
    "video_decode.c",
    "videoproc/main.c",
    #[cfg(feature = "wasm")]
    "wasm.c",
    "watchdog.c",
    "webaudio.c",
    "win_launcher.c",
    "ws.c",
    "webcrypto.c",
    "webgl.c",
    #[cfg(feature = "webgpu")]
    "webgpu.c",
    "xml.c",
    #[cfg(feature = "woff2")]
    "woff2.c",
];

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

pub fn ported_modules() -> &'static [&'static str] {
    PORTED
}
