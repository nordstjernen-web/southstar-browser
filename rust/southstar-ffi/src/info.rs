//! Southstar — what the Rust side of a build reports about itself on about:southstar, as declared in src/rust_info.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint};
use std::ffi::CString;
use std::sync::OnceLock;

const COMPILER: &str = match option_env!("NS_RUSTC_VERSION") {
    Some(version) => version,
    None => "unknown",
};

const PORTED: &[&str] = &[
    "bookmarks.c",
    "bytecode_cache.c",
    "cache.c",
    "config.c",
    "csp.c",
    "css_media.c",
    "css_prop_syntax.c",
    "css_syntax.c",
    "datetime.c",
    "debuglog.c",
    "glctx.c",
    "history.c",
    "i18n.c",
    "image_ico.c",
    "image_webp.c",
    "js_date.c",
    "mat4.h",
    "netutil.c",
    "safebrowsing.c",
    "security.c",
    "spellcheck.c",
    "threaddump.c",
    "webcrypto.c",
    #[cfg(feature = "woff2")]
    "woff2.c",
];

fn leak(slot: &'static OnceLock<CString>, text: impl FnOnce() -> String) -> *const c_char {
    slot.get_or_init(|| CString::new(text()).unwrap_or_default())
        .as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rust_compiler_version() -> *const c_char {
    static TEXT: OnceLock<CString> = OnceLock::new();
    leak(&TEXT, || COMPILER.to_owned())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rust_minimum_version() -> *const c_char {
    static TEXT: OnceLock<CString> = OnceLock::new();
    leak(&TEXT, || env!("CARGO_PKG_RUST_VERSION").to_owned())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rust_build_profile() -> *const c_char {
    if cfg!(debug_assertions) {
        c"debug".as_ptr()
    } else {
        c"release (LTO)".as_ptr()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rust_module_count() -> c_uint {
    PORTED.len() as c_uint
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_rust_modules() -> *const c_char {
    static TEXT: OnceLock<CString> = OnceLock::new();
    leak(&TEXT, || PORTED.join(", "))
}
