//! Southstar — the Location object and window.open: reading the top-level URL, navigating it, fragment changes and the scheme allow-list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js, UrlField};
use crate::text_of;

fn location_url(js: Js) -> Vec<u8> {
    if js.is_null() {
        Vec::new()
    } else {
        ffi::top_url(js)
    }
}

fn part(scope: &mut Scope<'_>, field: UrlField, fallback: &str) -> Result<Value, Value> {
    let url = location_url(ffi::js_of(scope));
    Ok(match ffi::url_field(&url, field) {
        Some(value) => scope.string_from_bytes(&value),
        None => scope.string(fallback),
    })
}

fn get_href(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let url = location_url(ffi::js_of(scope));
    Ok(scope.string_from_bytes(&url))
}

fn get_protocol(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Protocol, "")
}

fn get_host(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Host, "")
}

fn get_hostname(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Hostname, "")
}

fn get_port(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Port, "")
}

fn get_pathname(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let url = location_url(ffi::js_of(scope));
    Ok(match ffi::url_field(&url, UrlField::Pathname) {
        Some(path) if !path.is_empty() => scope.string_from_bytes(&path),
        _ => scope.string("/"),
    })
}

fn get_search(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Search, "")
}

fn get_hash(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Hash, "")
}

fn get_origin(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    part(scope, UrlField::Origin, "")
}

fn target_allowed(target: &[u8]) -> bool {
    match target.first() {
        None => return false,
        Some(b'/' | b'?' | b'#') => return true,
        Some(_) => {}
    }
    let colon = target.iter().position(|&c| c == b':');
    let slash = target.iter().position(|&c| c == b'/');
    match (colon, slash) {
        (None, _) => return true,
        (Some(colon), Some(slash)) if slash < colon => return true,
        _ => {}
    }
    ["http:", "https:", "about:", "data:", "mailto:"]
        .iter()
        .any(|scheme| {
            target.len() >= scheme.len()
                && target[..scheme.len()].eq_ignore_ascii_case(scheme.as_bytes())
        })
}

fn log_blocked(js: Js, target: &[u8]) {
    let shown = &target[..target.len().min(64)];
    let mut line = b"blocked navigation: scheme not allowed (".to_vec();
    line.extend_from_slice(shown);
    line.push(b')');
    ffi::log_line(js, &line);
}

fn navigate_unless_fragment(js: Js, target: &[u8]) {
    if ffi::in_frame_load(js) {
        return;
    }
    let absolute = ffi::url_resolve(Some(&ffi::top_url(js)), target);
    if !absolute.is_some_and(|absolute| ffi::anchor_fragment_navigate(js, &absolute)) {
        ffi::navigate(js, target, false);
    }
}

fn set_href(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() || !ffi::can_navigate(js) {
        return Ok(Value::undefined());
    }
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    let Some(target) = text_of(scope, &value) else {
        return Ok(Value::undefined());
    };
    if !ffi::url_parses(js, &target) {
        return Err(scope.dom_exception("SyntaxError", "location.href: invalid URL"));
    }
    if !target_allowed(&target) {
        log_blocked(js, &target);
        return Ok(Value::undefined());
    }
    navigate_unless_fragment(js, &target);
    Ok(Value::undefined())
}

fn assign(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(value) = args.first() else {
        return Ok(Value::undefined());
    };
    if js.is_null() || !ffi::can_navigate(js) {
        return Ok(Value::undefined());
    }
    let Some(target) = text_of(scope, value) else {
        return Ok(Value::undefined());
    };
    if !target_allowed(&target) {
        log_blocked(js, &target);
        return Ok(Value::undefined());
    }
    navigate_unless_fragment(js, &target);
    Ok(Value::undefined())
}

fn reload(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if !js.is_null() && !ffi::in_frame_load(js) {
        ffi::navigate(js, &ffi::top_url(js), true);
    }
    Ok(Value::undefined())
}

fn set_hash(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    let Some(hash) = text_of(scope, &value) else {
        return Ok(Value::undefined());
    };
    let fragment = hash.strip_prefix(b"#").unwrap_or(&hash);
    let old_url = ffi::top_url(js);
    let base = match old_url.iter().position(|&c| c == b'#') {
        Some(cut) => &old_url[..cut],
        None => &old_url[..],
    };
    let mut new_url = base.to_vec();
    if !fragment.is_empty() {
        new_url.push(b'#');
        new_url.extend_from_slice(fragment);
    }
    if old_url == new_url {
        return Ok(Value::undefined());
    }
    ffi::set_top_url(js, &new_url);
    ffi::soft_navigate(js, &new_url, false);
    ffi::fragment_navigated(js, &new_url);
    ffi::dispatch_hashchange(js, &old_url, &new_url);
    Ok(Value::undefined())
}

fn set_component(scope: &mut Scope<'_>, args: &[Value], component: &str) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let top = ffi::top_url(js);
    if top.is_empty() {
        return Ok(Value::undefined());
    }
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    let value = scope.to_bytes(&value)?;
    let Some(next) = ffi::url_set_component(&top, component, &value) else {
        return Ok(Value::undefined());
    };
    if ffi::can_navigate(js) && !ffi::in_frame_load(js) && next != top {
        ffi::navigate(js, &next, false);
    }
    Ok(Value::undefined())
}

fn set_protocol(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "protocol")
}

fn set_host(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "host")
}

fn set_hostname(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "hostname")
}

fn set_port(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "port")
}

fn set_pathname(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "pathname")
}

fn set_search(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    set_component(scope, args, "search")
}

const ACCESSORS: [(&str, NativeFn, Option<NativeFn>); 9] = [
    ("href", get_href, Some(set_href)),
    ("protocol", get_protocol, Some(set_protocol)),
    ("host", get_host, Some(set_host)),
    ("hostname", get_hostname, Some(set_hostname)),
    ("port", get_port, Some(set_port)),
    ("pathname", get_pathname, Some(set_pathname)),
    ("search", get_search, Some(set_search)),
    ("hash", get_hash, Some(set_hash)),
    ("origin", get_origin, None),
];

const METHODS: [(&str, u32, NativeFn); 4] = [
    ("assign", 1, assign),
    ("reload", 0, reload),
    ("replace", 1, assign),
    ("toString", 0, get_href),
];

pub(crate) fn make_location(scope: &mut Scope<'_>) -> Value {
    let location = scope.new_object();
    for (name, getter, setter) in ACCESSORS {
        let getter = scope.function(&format!("get {name}"), 0, getter);
        let setter = setter.map(|setter| scope.function(&format!("set {name}"), 1, setter));
        let _ = scope.define_accessor(
            &location,
            name,
            Some(&getter),
            setter.as_ref(),
            Attributes {
                writable: false,
                enumerable: true,
                configurable: true,
            },
        );
    }
    for (name, arity, method) in METHODS {
        let function = scope.function(name, arity, method);
        let _ = scope.define(
            &location,
            name,
            function,
            Attributes {
                writable: true,
                enumerable: true,
                configurable: true,
            },
        );
    }
    let _ = scope.define_to_string_tag(&location, "Location");
    location
}

pub(crate) fn open(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(url) = args.first() else {
        return Ok(Value::null());
    };
    if js.is_null() || !ffi::can_navigate(js) {
        return Ok(Value::null());
    }
    let Some(url) = text_of(scope, url) else {
        return Ok(this.clone());
    };
    if !url.is_empty() && !ffi::url_parses(js, &url) {
        return Err(scope.dom_exception("SyntaxError", "window.open: invalid URL"));
    }
    if !ffi::has_transient_activation(js) {
        let shown: &[u8] = if url.is_empty() {
            b"about:blank"
        } else {
            &url[..url.len().min(256)]
        };
        let mut line = b"Blocked a popup: window.open(".to_vec();
        line.extend_from_slice(shown);
        line.extend_from_slice(b") was called without user interaction");
        ffi::log_line(js, &line);
        return Ok(Value::null());
    }
    ffi::consume_user_activation(js);
    ffi::navigate(js, &url, false);
    Ok(this.clone())
}
