//! Southstar — the settings page's endpoints: the current settings as JSON and saving a submitted form into the configuration.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_config::NsConfig;
use southstar_glib::GBoolean;

use crate::ffi::host as sys;

fn json_escape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for &b in text {
        if b == b'"' || b == b'\\' {
            out.push(b'\\');
        }
        if b < 0x20 {
            out.extend_from_slice(format!("\\u{b:04x}").as_bytes());
            continue;
        }
        out.push(b);
    }
    out
}

fn flag(on: bool) -> &'static str {
    if on { "true" } else { "false" }
}

fn text_field(
    c: Option<&NsConfig>,
    field: impl Fn(&NsConfig) -> *mut core::ffi::c_char,
) -> Vec<u8> {
    json_escape(
        &c.and_then(|c| sys::config_text(field(c)))
            .unwrap_or_default(),
    )
}

pub fn json() -> Vec<u8> {
    let config = sys::ConfigGuard::lock();
    config.reload();
    let c = config.get();
    let on = |field: fn(&NsConfig) -> GBoolean| flag(c.is_some_and(|c| field(c) != 0));
    let mut out = b"{\"home_url\":\"".to_vec();
    out.extend_from_slice(&text_field(c, |c| c.home_url));
    out.extend_from_slice(b"\",\"search_engine\":\"");
    out.extend_from_slice(&text_field(c, |c| c.search_engine));
    out.extend_from_slice(
        format!(
            concat!(
                "\",\"cookie_policy\":{},",
                "\"do_not_track\":{},\"global_privacy_control\":{},",
                "\"strip_tracking_params\":{},\"https_first\":{},",
                "\"images_enabled\":{},\"javascript_enabled\":{},",
                "\"webgl_enabled\":{},",
                "\"local_storage_enabled\":{},\"cache_enabled\":{}}}"
            ),
            c.map_or(1, |c| c.cookie_policy),
            on(|c| c.do_not_track),
            on(|c| c.global_privacy_control),
            on(|c| c.strip_tracking_params),
            on(|c| c.https_first),
            on(|c| c.images_enabled),
            on(|c| c.javascript_enabled),
            on(|c| c.webgl_enabled),
            on(|c| c.local_storage_enabled),
            on(|c| c.cache_enabled),
        )
        .as_bytes(),
    );
    out
}

type Switch = fn(&mut NsConfig) -> &mut GBoolean;

const SWITCHES: [(&str, Switch); 9] = [
    ("do_not_track", |c| &mut c.do_not_track),
    ("global_privacy_control", |c| &mut c.global_privacy_control),
    ("strip_tracking_params", |c| &mut c.strip_tracking_params),
    ("https_first", |c| &mut c.https_first),
    ("images_enabled", |c| &mut c.images_enabled),
    ("javascript_enabled", |c| &mut c.javascript_enabled),
    ("webgl_enabled", |c| &mut c.webgl_enabled),
    ("local_storage_enabled", |c| &mut c.local_storage_enabled),
    ("cache_enabled", |c| &mut c.cache_enabled),
];

pub fn save(form: &[u8]) {
    if form.is_empty() {
        return;
    }
    let Some(fields) = sys::parse_form(form) else {
        return;
    };
    let mut config = sys::ConfigGuard::lock();
    config.reload();
    if let Some(c) = config.get_mut() {
        let value = |key: &str| fields.get(key.as_bytes());
        if let Some(v) = value("home_url") {
            sys::set_config_text(&mut c.home_url, v);
        }
        if let Some(v) = value("search_engine") {
            sys::set_config_text(&mut c.search_engine, v);
        }
        if let Some(v) = value("cookie_policy") {
            c.cookie_policy = sys::atoi_of(v);
        }
        for (key, field) in SWITCHES {
            if let Some(v) = value(key) {
                *field(c) = GBoolean::from(sys::atoi_of(v) != 0);
            }
        }
    }
    config.save();
}
