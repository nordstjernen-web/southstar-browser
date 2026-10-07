//! Southstar — runtime configuration: the flat `key = value` file, its defaults and environment overrides, as the struct of src/config.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{c_char, c_int};
use southstar_glib::{FALSE, GBoolean};

pub const DEFAULT_SEARCH_ENGINE: &str = "https://duckduckgo.com/?q=%s";
const OLD_SEARCH_ENGINE: &[u8] = b"https://lite.duckduckgo.com/lite/?q=%s";
const MAX_REDIRECTS: i32 = 10;

#[repr(C)]
pub struct NsConfig {
    pub home_url: *mut c_char,
    pub user_agent: *mut c_char,
    pub compat_mode: *mut c_char,
    pub accept_language: *mut c_char,
    pub search_engine: *mut c_char,
    pub ai_model_mirror: *mut c_char,
    pub http_proxy: *mut c_char,
    pub https_proxy: *mut c_char,
    pub no_proxy: *mut c_char,
    pub doh_url: *mut c_char,
    pub gsk_renderer: *mut c_char,
    pub referer_policy: c_int,
    pub cookie_policy: c_int,
    pub color_scheme: c_int,
    pub reduced_motion: c_int,
    pub do_not_track: GBoolean,
    pub global_privacy_control: GBoolean,
    pub strip_tracking_params: GBoolean,
    pub https_first: GBoolean,
    pub harden_allocator: GBoolean,
    pub speculative_preload: GBoolean,
    pub async_image_decode: GBoolean,
    pub images_enabled: GBoolean,
    pub javascript_enabled: GBoolean,
    pub webgl_enabled: GBoolean,
    pub camera_enabled: GBoolean,
    pub microphone_enabled: GBoolean,
    pub local_storage_enabled: GBoolean,
    pub cache_enabled: GBoolean,
    pub tls_allow_insecure_override: GBoolean,
    pub watchdog_enabled: GBoolean,
    pub private_mode: GBoolean,
    pub cache_cap_mb: c_int,
    pub js_eval_budget_ms: c_int,
    pub js_memory_cap_mb: c_int,
    pub max_redirects: c_int,
    pub window_width_px: c_int,
    pub window_height_px: c_int,
    pub layout_viewport_px: c_int,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Bool,
    Int,
    Referer,
    Cookie,
    ColorScheme,
    ReducedMotion,
}

struct Field {
    key: &'static str,
    kind: Kind,
    text: &'static str,
    number: i32,
}

const fn text(key: &'static str, default: &'static str) -> Field {
    Field {
        key,
        kind: Kind::Text,
        text: default,
        number: 0,
    }
}

const fn number(key: &'static str, kind: Kind, default: i32) -> Field {
    Field {
        key,
        kind,
        text: "",
        number: default,
    }
}

const fn flag(key: &'static str, default: bool) -> Field {
    number(key, Kind::Bool, default as i32)
}

const REFERER_STRICT_ORIGIN_WHEN_CROSS: i32 = 2;
const COOKIE_FIRST_PARTY: i32 = 1;

const FIELDS: [Field; 38] = [
    text("home_url", "about:start"),
    text("user_agent", ""),
    text("compat_mode", "chrome"),
    text("accept_language", ""),
    text("search_engine", DEFAULT_SEARCH_ENGINE),
    text("ai_model_mirror", ""),
    text("http_proxy", ""),
    text("https_proxy", ""),
    text("no_proxy", ""),
    text("doh_url", ""),
    text("gsk_renderer", "auto"),
    number(
        "referer_policy",
        Kind::Referer,
        REFERER_STRICT_ORIGIN_WHEN_CROSS,
    ),
    number("cookie_policy", Kind::Cookie, COOKIE_FIRST_PARTY),
    number("color_scheme", Kind::ColorScheme, 0),
    number("reduced_motion", Kind::ReducedMotion, 0),
    flag("do_not_track", false),
    flag("global_privacy_control", false),
    flag("strip_tracking_params", true),
    flag("https_first", true),
    flag("harden_allocator", true),
    flag("speculative_preload", true),
    flag("async_image_decode", true),
    flag("images_enabled", true),
    flag("javascript_enabled", true),
    flag("webgl_enabled", true),
    flag("camera_enabled", false),
    flag("microphone_enabled", false),
    flag("local_storage_enabled", true),
    flag("cache_enabled", true),
    flag("tls_allow_insecure_override", false),
    flag("watchdog_enabled", true),
    number("cache_cap_mb", Kind::Int, 256),
    number("js_eval_budget_ms", Kind::Int, 60000),
    number("js_memory_cap_mb", Kind::Int, 2048),
    number("max_redirects", Kind::Int, MAX_REDIRECTS),
    number("window_width_px", Kind::Int, 1280),
    number("window_height_px", Kind::Int, 800),
    number("layout_viewport_px", Kind::Int, 1000),
];

const BOOL_NAMES: [(&str, i32); 8] = [
    ("true", 1),
    ("yes", 1),
    ("on", 1),
    ("1", 1),
    ("false", 0),
    ("no", 0),
    ("off", 0),
    ("0", 0),
];
const REFERER_NAMES: [(&str, i32); 7] = [
    ("none", 0),
    ("no-referrer", 0),
    ("same-origin", 1),
    ("strict-origin-when-cross-origin", 2),
    ("default", 2),
    ("unsafe-url", 3),
    ("full", 3),
];
const COOKIE_NAMES: [(&str, i32); 5] = [
    ("always", 0),
    ("first-party", 1),
    ("first-party-only", 1),
    ("never", 2),
    ("off", 2),
];
const COLOR_SCHEME_NAMES: [(&str, i32); 4] =
    [("auto", 0), ("system", 0), ("light", 1), ("dark", 2)];
const REDUCED_MOTION_NAMES: [(&str, i32); 6] = [
    ("auto", 0),
    ("system", 0),
    ("no-preference", 1),
    ("off", 1),
    ("reduce", 2),
    ("on", 2),
];

const DISABLE_ENV: [(&str, &str); 9] = [
    ("NS_NO_CACHE", "cache_enabled"),
    ("NS_NO_LOCAL_STORAGE", "local_storage_enabled"),
    ("NS_NO_IMAGES", "images_enabled"),
    ("NS_NO_JAVASCRIPT", "javascript_enabled"),
    ("NS_NO_WATCHDOG", "watchdog_enabled"),
    ("NS_NO_HTTPS_FIRST", "https_first"),
    ("NS_NO_HARDEN_ALLOC", "harden_allocator"),
    ("NS_NO_PRELOAD_SCAN", "speculative_preload"),
    ("NS_NO_ASYNC_IMG_DECODE", "async_image_decode"),
];
const VALUE_ENV: [(&str, &str); 8] = [
    ("NS_HOME_URL", "home_url"),
    ("NS_USER_AGENT", "user_agent"),
    ("NS_COMPAT_MODE", "compat_mode"),
    ("NS_HTTP_PROXY", "http_proxy"),
    ("NS_HTTPS_PROXY", "https_proxy"),
    ("NS_NO_PROXY", "no_proxy"),
    ("NS_DOH_URL", "doh_url"),
    ("NS_GSK_RENDERER", "gsk_renderer"),
];

enum Slot<'a> {
    Text(&'a mut *mut c_char),
    Number(&'a mut c_int),
}

impl NsConfig {
    pub const ZERO: NsConfig = NsConfig {
        home_url: core::ptr::null_mut(),
        user_agent: core::ptr::null_mut(),
        compat_mode: core::ptr::null_mut(),
        accept_language: core::ptr::null_mut(),
        search_engine: core::ptr::null_mut(),
        ai_model_mirror: core::ptr::null_mut(),
        http_proxy: core::ptr::null_mut(),
        https_proxy: core::ptr::null_mut(),
        no_proxy: core::ptr::null_mut(),
        doh_url: core::ptr::null_mut(),
        gsk_renderer: core::ptr::null_mut(),
        referer_policy: 0,
        cookie_policy: 0,
        color_scheme: 0,
        reduced_motion: 0,
        do_not_track: 0,
        global_privacy_control: 0,
        strip_tracking_params: 0,
        https_first: 0,
        harden_allocator: 0,
        speculative_preload: 0,
        async_image_decode: 0,
        images_enabled: 0,
        javascript_enabled: 0,
        webgl_enabled: 0,
        camera_enabled: 0,
        microphone_enabled: 0,
        local_storage_enabled: 0,
        cache_enabled: 0,
        tls_allow_insecure_override: 0,
        watchdog_enabled: 0,
        private_mode: 0,
        cache_cap_mb: 0,
        js_eval_budget_ms: 0,
        js_memory_cap_mb: 0,
        max_redirects: 0,
        window_width_px: 0,
        window_height_px: 0,
        layout_viewport_px: 0,
    };

    fn slot(&mut self, key: &str) -> Option<Slot<'_>> {
        Some(match key {
            "home_url" => Slot::Text(&mut self.home_url),
            "user_agent" => Slot::Text(&mut self.user_agent),
            "compat_mode" => Slot::Text(&mut self.compat_mode),
            "accept_language" => Slot::Text(&mut self.accept_language),
            "search_engine" => Slot::Text(&mut self.search_engine),
            "ai_model_mirror" => Slot::Text(&mut self.ai_model_mirror),
            "http_proxy" => Slot::Text(&mut self.http_proxy),
            "https_proxy" => Slot::Text(&mut self.https_proxy),
            "no_proxy" => Slot::Text(&mut self.no_proxy),
            "doh_url" => Slot::Text(&mut self.doh_url),
            "gsk_renderer" => Slot::Text(&mut self.gsk_renderer),
            "referer_policy" => Slot::Number(&mut self.referer_policy),
            "cookie_policy" => Slot::Number(&mut self.cookie_policy),
            "color_scheme" => Slot::Number(&mut self.color_scheme),
            "reduced_motion" => Slot::Number(&mut self.reduced_motion),
            "do_not_track" => Slot::Number(&mut self.do_not_track),
            "global_privacy_control" => Slot::Number(&mut self.global_privacy_control),
            "strip_tracking_params" => Slot::Number(&mut self.strip_tracking_params),
            "https_first" => Slot::Number(&mut self.https_first),
            "harden_allocator" => Slot::Number(&mut self.harden_allocator),
            "speculative_preload" => Slot::Number(&mut self.speculative_preload),
            "async_image_decode" => Slot::Number(&mut self.async_image_decode),
            "images_enabled" => Slot::Number(&mut self.images_enabled),
            "javascript_enabled" => Slot::Number(&mut self.javascript_enabled),
            "webgl_enabled" => Slot::Number(&mut self.webgl_enabled),
            "camera_enabled" => Slot::Number(&mut self.camera_enabled),
            "microphone_enabled" => Slot::Number(&mut self.microphone_enabled),
            "local_storage_enabled" => Slot::Number(&mut self.local_storage_enabled),
            "cache_enabled" => Slot::Number(&mut self.cache_enabled),
            "tls_allow_insecure_override" => Slot::Number(&mut self.tls_allow_insecure_override),
            "watchdog_enabled" => Slot::Number(&mut self.watchdog_enabled),
            "cache_cap_mb" => Slot::Number(&mut self.cache_cap_mb),
            "js_eval_budget_ms" => Slot::Number(&mut self.js_eval_budget_ms),
            "js_memory_cap_mb" => Slot::Number(&mut self.js_memory_cap_mb),
            "max_redirects" => Slot::Number(&mut self.max_redirects),
            "window_width_px" => Slot::Number(&mut self.window_width_px),
            "window_height_px" => Slot::Number(&mut self.window_height_px),
            "layout_viewport_px" => Slot::Number(&mut self.layout_viewport_px),
            _ => return None,
        })
    }

    fn text(&mut self, key: &str) -> Option<Vec<u8>> {
        match self.slot(key)? {
            Slot::Text(p) => ffi::text(*p),
            Slot::Number(_) => None,
        }
    }

    fn number(&mut self, key: &str) -> i32 {
        match self.slot(key) {
            Some(Slot::Number(n)) => *n,
            _ => 0,
        }
    }
}

pub fn get() -> Option<&'static NsConfig> {
    ffi::config()
}

pub fn cache_enabled() -> bool {
    get().is_none_or(|config| config.cache_enabled != FALSE)
}

pub fn private_mode() -> bool {
    get().is_some_and(|config| config.private_mode != FALSE)
}

fn strip(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &bytes[start..end.max(start)]
}

fn parse_choice(value: &[u8], choices: &[(&str, i32)], default: i32) -> i32 {
    choices
        .iter()
        .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(value))
        .map_or(default, |&(_, choice)| choice)
}

fn parse_number(value: &[u8], default: i32) -> i32 {
    if value.is_empty() {
        return default;
    }
    ffi::ascii_strtoll(value).map_or(default, |n| {
        n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    })
}

fn apply_pair(config: &mut NsConfig, key: &[u8], value: &[u8]) {
    let Some(field) = FIELDS.iter().find(|field| field.key.as_bytes() == key) else {
        return;
    };
    match config.slot(field.key) {
        Some(Slot::Text(p)) => ffi::set_text(p, value),
        Some(Slot::Number(n)) => {
            *n = match field.kind {
                Kind::Bool => parse_choice(value, &BOOL_NAMES, *n),
                Kind::Int => parse_number(value, *n),
                Kind::Referer => parse_choice(value, &REFERER_NAMES, *n),
                Kind::Cookie => parse_choice(value, &COOKIE_NAMES, *n),
                Kind::ColorScheme => parse_choice(value, &COLOR_SCHEME_NAMES, *n),
                Kind::ReducedMotion => parse_choice(value, &REDUCED_MOTION_NAMES, *n),
                Kind::Text => *n,
            };
            if field.kind == Kind::Bool && *n != FALSE {
                *n = 1;
            }
        }
        None => {}
    }
}

fn apply_defaults(config: &mut NsConfig) {
    for field in &FIELDS {
        match config.slot(field.key) {
            Some(Slot::Text(p)) => ffi::set_text(p, field.text.as_bytes()),
            Some(Slot::Number(n)) => *n = field.number,
            None => {}
        }
    }
}

fn apply_file(config: &mut NsConfig, contents: &[u8]) {
    let contents = contents.split(|&c| c == 0).next().unwrap_or_default();
    for line in contents.split(|&c| c == b'\n') {
        let line = strip(line);
        if line.first().is_none_or(|&c| c == b'#') {
            continue;
        }
        let Some(eq) = line.iter().position(|&c| c == b'=') else {
            continue;
        };
        let key = strip(&line[..eq]);
        if !key.is_empty() {
            apply_pair(config, key, strip(&line[eq + 1..]));
        }
    }
}

fn apply_env(config: &mut NsConfig, env: impl Fn(&str) -> Option<Vec<u8>>) {
    for (name, key) in DISABLE_ENV {
        if env(name).is_some() {
            apply_pair(config, key.as_bytes(), b"false");
        }
    }
    for (name, key) in VALUE_ENV {
        if let Some(value) = env(name).filter(|value| !value.is_empty()) {
            apply_pair(config, key.as_bytes(), &value);
        }
    }
    if env("NS_PRIVATE").is_some() {
        config.private_mode = 1;
    }
}

fn load(config: &mut NsConfig, contents: Option<&[u8]>, env: impl Fn(&str) -> Option<Vec<u8>>) {
    apply_defaults(config);
    if let Some(contents) = contents {
        apply_file(config, contents);
    }
    if config.text("search_engine").as_deref() == Some(OLD_SEARCH_ENGINE) {
        ffi::set_text(&mut config.search_engine, DEFAULT_SEARCH_ENGINE.as_bytes());
    }
    apply_env(config, env);
}

fn adopt(config: &mut NsConfig, fresh: &mut NsConfig) {
    for field in &FIELDS {
        match (config.slot(field.key), fresh.slot(field.key)) {
            (Some(Slot::Text(to)), Some(Slot::Text(from))) => ffi::adopt_text(to, from),
            (Some(Slot::Number(to)), Some(Slot::Number(from))) => *to = *from,
            _ => {}
        }
    }
}

fn choice_name(
    choices: &[(&'static str, i32)],
    value: i32,
    fallback: &'static str,
) -> &'static str {
    choices
        .iter()
        .find(|&&(_, choice)| choice == value)
        .map_or(fallback, |&(name, _)| name)
}

fn referer_policy_name(value: i32) -> &'static str {
    choice_name(&REFERER_NAMES, value, "strict-origin-when-cross-origin")
}

fn cookie_policy_name(value: i32) -> &'static str {
    choice_name(&COOKIE_NAMES, value, "first-party")
}

fn color_scheme_name(value: i32) -> &'static str {
    choice_name(&COLOR_SCHEME_NAMES, value, "auto")
}

fn reduced_motion_name(value: i32) -> &'static str {
    choice_name(&REDUCED_MOTION_NAMES, value, "auto")
}

fn flag_name(value: i32) -> &'static str {
    if value != FALSE { "true" } else { "false" }
}

fn serialize(config: &mut NsConfig) -> Vec<u8> {
    let mut out = b"# southstar configuration\n".to_vec();
    for field in &FIELDS {
        let value = match field.kind {
            Kind::Text => config.text(field.key).unwrap_or_default(),
            Kind::Bool => flag_name(config.number(field.key)).into(),
            Kind::Int => config.number(field.key).to_string().into_bytes(),
            Kind::Referer => referer_policy_name(config.number(field.key)).into(),
            Kind::Cookie => cookie_policy_name(config.number(field.key)).into(),
            Kind::ColorScheme => color_scheme_name(config.number(field.key)).into(),
            Kind::ReducedMotion => reduced_motion_name(config.number(field.key)).into(),
        };
        out.extend_from_slice(field.key.as_bytes());
        out.extend_from_slice(b" = ");
        out.extend_from_slice(&value);
        out.push(b'\n');
    }
    out
}

fn non_empty(value: Option<Vec<u8>>) -> Option<Vec<u8>> {
    value.filter(|value| !value.is_empty())
}

fn dump(config: &mut NsConfig, path: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut line = |label: &str, value: &[u8]| {
        out.extend_from_slice(label.as_bytes());
        out.extend_from_slice(value);
        out.push(b'\n');
    };
    let text = |config: &mut NsConfig, key: &str| config.text(key).unwrap_or_else(ffi::null_text);
    line("# southstar effective config", b"");
    line("# file: ", path.unwrap_or(b"(none)"));
    line("home_url              = ", &text(config, "home_url"));
    line("user_agent            = ", &text(config, "user_agent"));
    match non_empty(config.text("accept_language")) {
        Some(language) => line("accept_language       = ", &language),
        None => line(
            "accept_language       = ",
            &[b"(auto: ".as_slice(), &ffi::default_accept_language(), b")"].concat(),
        ),
    }
    line("search_engine         = ", &text(config, "search_engine"));
    if let Some(mirror) = non_empty(config.text("ai_model_mirror")) {
        line("ai_model_mirror       = ", &mirror);
    }
    line(
        "gsk_renderer          = ",
        &non_empty(config.text("gsk_renderer")).unwrap_or_else(|| b"auto".to_vec()),
    );
    let none = || b"(none)".to_vec();
    line(
        "http_proxy            = ",
        &non_empty(ffi::proxy_mask(config.http_proxy)).unwrap_or_else(none),
    );
    line(
        "https_proxy           = ",
        &non_empty(ffi::proxy_mask(config.https_proxy)).unwrap_or_else(none),
    );
    line(
        "no_proxy              = ",
        &non_empty(config.text("no_proxy")).unwrap_or_else(none),
    );
    line(
        "doh_url               = ",
        &non_empty(config.text("doh_url")).unwrap_or_else(|| b"(system resolver)".to_vec()),
    );
    line(
        "referer_policy        = ",
        referer_policy_name(config.referer_policy).as_bytes(),
    );
    line(
        "cookie_policy         = ",
        cookie_policy_name(config.cookie_policy).as_bytes(),
    );
    line(
        "color_scheme          = ",
        color_scheme_name(config.color_scheme).as_bytes(),
    );
    line(
        "reduced_motion        = ",
        reduced_motion_name(config.reduced_motion).as_bytes(),
    );
    for (label, value) in [
        ("do_not_track          = ", config.do_not_track),
        ("global_privacy_control = ", config.global_privacy_control),
        ("strip_tracking_params = ", config.strip_tracking_params),
        ("https_first           = ", config.https_first),
        ("harden_allocator      = ", config.harden_allocator),
        ("speculative_preload   = ", config.speculative_preload),
        ("async_image_decode    = ", config.async_image_decode),
        ("images_enabled        = ", config.images_enabled),
        ("javascript_enabled    = ", config.javascript_enabled),
        ("webgl_enabled         = ", config.webgl_enabled),
        ("camera_enabled        = ", config.camera_enabled),
        ("microphone_enabled    = ", config.microphone_enabled),
        ("local_storage_enabled = ", config.local_storage_enabled),
        ("cache_enabled         = ", config.cache_enabled),
        (
            "tls_allow_insecure_override = ",
            config.tls_allow_insecure_override,
        ),
        ("watchdog_enabled      = ", config.watchdog_enabled),
        ("private_mode          = ", config.private_mode),
    ] {
        line(label, flag_name(value).as_bytes());
    }
    for (label, value) in [
        ("cache_cap_mb          = ", config.cache_cap_mb),
        ("js_eval_budget_ms     = ", config.js_eval_budget_ms),
        ("js_memory_cap_mb      = ", config.js_memory_cap_mb),
        ("max_redirects         = ", config.max_redirects),
        ("window_width_px       = ", config.window_width_px),
        ("window_height_px      = ", config.window_height_px),
        ("layout_viewport_px    = ", config.layout_viewport_px),
    ] {
        line(label, value.to_string().as_bytes());
    }
    out
}
