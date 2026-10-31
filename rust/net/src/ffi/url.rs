//! Southstar — the network helpers that read the configuration: tracking-parameter stripping, HTTP dates, HTTPS-First and the user agent strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::GBoolean;

use crate::url;

fn config_flag(flag: impl Fn(&southstar_config::NsConfig) -> GBoolean) -> bool {
    southstar_config::get().is_some_and(|c| flag(&c) != 0)
}

pub fn http_date(date: &[u8]) -> i64 {
    southstar_netstore::http_date::parse(date).unwrap_or(-1)
}

pub fn strip_tracking_params(url: &[u8]) -> Option<Vec<u8>> {
    if !url::is_http_or_https(url) {
        return None;
    }
    url::strip_tracking_params(url, config_flag(|c| c.strip_tracking_params))
}

pub fn https_first_upgrade(url: &[u8]) -> Option<Vec<u8>> {
    if !config_flag(|c| c.https_first) {
        return None;
    }
    url::https_first_upgrade(url, true)
}

fn without_nul(s: &'static str) -> &'static str {
    s.strip_suffix('\0').unwrap_or(s)
}

pub fn is_mobile_mode() -> bool {
    false
}

pub fn navigator_platform() -> &'static str {
    without_nul(url::NAVIGATOR_PLATFORM)
}

pub fn ua_hint_platform() -> &'static str {
    without_nul(url::HINT_PLATFORM)
}

pub fn user_agent_for_mode(compat_mode: Option<&[u8]>) -> &'static str {
    without_nul(url::user_agent_for_mode(compat_mode))
}
