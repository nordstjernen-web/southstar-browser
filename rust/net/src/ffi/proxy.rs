//! Southstar — proxy settings: the command-line override and the configured HTTP, HTTPS and no-proxy values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_config::NsConfig;

use crate::storage::Slot;

static OVERRIDE: Slot = Slot::new();

fn config() -> Option<&'static NsConfig> {
    southstar_config::get()
}

pub fn set_proxy_override(proxy_url: Option<&[u8]>) {
    OVERRIDE.set(proxy_url.filter(|p| !p.is_empty()).map(<[u8]>::to_vec));
}

pub fn proxy_override() -> Option<Vec<u8>> {
    OVERRIDE.get()
}

pub fn pick_configured_proxy(url: &[u8]) -> Option<Vec<u8>> {
    if let Some(forced) = OVERRIDE.get().filter(|f| !f.is_empty()) {
        return Some(forced);
    }
    let cfg = config()?;
    let https = url.starts_with(b"https://") || url.starts_with(b"wss://");
    let secure = if https {
        cfg.https_proxy.non_empty()
    } else {
        None
    };
    secure
        .or_else(|| cfg.http_proxy.non_empty())
        .map(<[u8]>::to_vec)
}

pub fn configured_no_proxy() -> Option<Vec<u8>> {
    config()?.no_proxy.non_empty().map(<[u8]>::to_vec)
}

pub fn http_proxy() -> Option<Vec<u8>> {
    config()?.http_proxy.to_vec()
}

pub fn https_proxy() -> Option<Vec<u8>> {
    config()?.https_proxy.to_vec()
}

pub fn no_proxy() -> Option<Vec<u8>> {
    config()?.no_proxy.to_vec()
}

pub fn shutdown() {
    OVERRIDE.set(None);
}
