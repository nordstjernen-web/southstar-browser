//! Southstar — writes the Google Safe Browsing API key from NS_SAFE_BROWSING_API_KEY into OUT_DIR, or nothing when it is unset or malformed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

fn looks_like_key(key: &str) -> bool {
    (20..=128).contains(&key.len())
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn main() {
    println!("cargo:rerun-if-env-changed=NS_SAFE_BROWSING_API_KEY");
    let key = std::env::var("NS_SAFE_BROWSING_API_KEY").unwrap_or_default();
    let key = key.trim();
    let key = if key.is_empty() || looks_like_key(key) {
        key
    } else {
        println!(
            "cargo:warning=NS_SAFE_BROWSING_API_KEY does not look like an API key; building without Google Safe Browsing"
        );
        ""
    };
    let out = std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR");
    let path = std::path::Path::new(&out).join("safebrowsing-api-key");
    if std::fs::read(&path).ok().as_deref() != Some(key.as_bytes()) {
        std::fs::write(&path, key).expect("OUT_DIR is writable");
    }
}
