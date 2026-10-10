//! Southstar — document.cookie: the script-visible cookie string, its attribute and prefix checks, and seeding it from the cookie jar.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::time::{SystemTime, UNIX_EPOCH};

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{JsResult, until_nul};

const MAX_COOKIE_LEN: usize = 4096;

#[derive(Default)]
struct Attrs {
    expired: bool,
    secure: bool,
    has_domain: bool,
    samesite_none: bool,
    path_is_root: bool,
    has_max_age: bool,
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn max_age_expired(value: &[u8]) -> Option<bool> {
    let start = value.iter().position(|b| !b.is_ascii_whitespace())?;
    let value = &value[start..];
    let (negative, digits) = match value.first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') => (false, &value[1..]),
        _ => (false, value),
    };
    let count = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    if count == 0 {
        return None;
    }
    Some(negative || digits[..count].iter().all(|&b| b == b'0'))
}

fn parse_attrs(attrs: &[u8]) -> Attrs {
    let mut out = Attrs::default();
    for token in attrs.split(|&b| b == b';') {
        let lead = token
            .iter()
            .position(|&b| !is_blank(b) && b != b';')
            .unwrap_or(token.len());
        let mut token = &token[lead..];
        while let Some((&last, rest)) = token.split_last()
            && is_blank(last)
        {
            token = rest;
        }
        if token.is_empty() {
            continue;
        }
        let eq = token.iter().position(|&b| b == b'=');
        let key = &token[..eq.unwrap_or(token.len())];
        let mut value: &[u8] = eq.map_or(&[], |e| &token[e + 1..]);
        while let Some((&first, rest)) = value.split_first()
            && is_blank(first)
        {
            value = rest;
        }
        let has_value = eq.is_some();
        if key.eq_ignore_ascii_case(b"secure") {
            out.secure = true;
        } else if key.eq_ignore_ascii_case(b"max-age") && has_value {
            if let Some(expired) = max_age_expired(value) {
                out.has_max_age = true;
                out.expired = expired;
            }
        } else if key.eq_ignore_ascii_case(b"expires")
            && has_value
            && !value.is_empty()
            && !out.has_max_age
        {
            let expiry = ffi::http_date(value);
            if expiry != -1 && expiry <= now_seconds() {
                out.expired = true;
            }
        } else if key.eq_ignore_ascii_case(b"domain") && has_value && !value.is_empty() {
            out.has_domain = true;
        } else if key.eq_ignore_ascii_case(b"path") && has_value {
            out.path_is_root = value == b"/";
        } else if key.eq_ignore_ascii_case(b"samesite")
            && has_value
            && value.eq_ignore_ascii_case(b"none")
        {
            out.samesite_none = true;
        }
    }
    out
}

fn starts_with_caseless(name: &[u8], prefix: &[u8]) -> bool {
    name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn prefix_ok(name: &[u8], attrs: &Attrs, is_https: bool) -> bool {
    if starts_with_caseless(name, b"__Secure-") && (!attrs.secure || !is_https) {
        return false;
    }
    if starts_with_caseless(name, b"__Host-")
        && (!attrs.secure || !is_https || attrs.has_domain || !attrs.path_is_root)
    {
        return false;
    }
    !(attrs.samesite_none && !attrs.secure)
}

fn remove_named(jar: &mut Vec<u8>, name: &[u8]) {
    if jar.is_empty() || name.is_empty() {
        return;
    }
    let needle = [name, b"="].concat();
    let mut scan = 0;
    while let Some(pos) = find(jar, &needle, scan) {
        if pos == 0 || jar[pos - 1] == b' ' || jar[pos - 1] == b';' {
            let rest = find(jar, b"; ", pos).map(|end| end + 2);
            let tail = rest.map(|r| jar[r..].to_vec()).unwrap_or_default();
            jar.truncate(pos);
            jar.extend_from_slice(&tail);
            scan = pos;
        } else {
            scan = pos + needle.len();
        }
    }
    while jar.last().is_some_and(|&b| b == b';' || b == b' ') {
        jar.pop();
    }
}

fn jar_has_name(jar: &[u8], name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut scan = 0;
    while let Some(pos) = find(jar, name, scan) {
        let at_start = pos == 0 || jar[pos - 1] == b' ' || jar[pos - 1] == b';';
        if at_start && jar.get(pos + name.len()) == Some(&b'=') {
            return true;
        }
        scan = pos + name.len();
    }
    false
}

pub(crate) fn get_cookie(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() || ffi::is_realm_document(scope, this) {
        return Ok(scope.string(""));
    }
    Ok(scope.string_from_bytes(&ffi::cookie_value(js).unwrap_or_default()))
}

pub(crate) fn set_cookie(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() || ffi::is_realm_document(scope, this) {
        return Ok(Value::undefined());
    }
    let bytes = scope.to_bytes(&args[0])?;
    let cookie = until_nul(&bytes);
    if cookie.len() > MAX_COOKIE_LEN {
        return Ok(Value::undefined());
    }
    let Some(eq) = cookie.iter().position(|&b| b == b'=').filter(|&e| e > 0) else {
        return Ok(Value::undefined());
    };
    let semi = cookie.iter().position(|&b| b == b';');
    let name = &cookie[..eq];
    let pair = &cookie[..semi.unwrap_or(cookie.len())];
    let attrs = semi.map_or_else(Attrs::default, |s| parse_attrs(&cookie[s..]));
    let is_https = ffi::partition_key(js).is_some_and(|key| key.starts_with(b"https://"));
    if attrs.secure && !is_https {
        return Ok(Value::undefined());
    }
    if !prefix_ok(name, &attrs, is_https) {
        return Ok(Value::undefined());
    }
    let mut jar = ffi::cookie_value(js).unwrap_or_default();
    remove_named(&mut jar, name);
    if !attrs.expired {
        if !jar.is_empty() {
            jar.extend_from_slice(b"; ");
        }
        jar.extend_from_slice(pair);
    }
    ffi::set_cookie_value(js, &jar);
    if let Some(url) = ffi::current_url(js) {
        ffi::cookie_store_from_js(&url, cookie);
        let visible = ffi::cookies_for_js(&url).unwrap_or_default();
        ffi::set_cookie_value(js, &visible);
    }
    Ok(Value::undefined())
}

pub(crate) fn seed_from_jar(js: Js) {
    let Some(url) = ffi::current_url(js) else {
        return;
    };
    let Some(jar) = ffi::cookies_for_js(&url).filter(|jar| !jar.is_empty()) else {
        return;
    };
    let Some(mut merged) = ffi::cookie_value(js).filter(|existing| !existing.is_empty()) else {
        ffi::set_cookie_value(js, &jar);
        return;
    };
    let mut start = 0;
    loop {
        let end = find(&jar, b"; ", start).unwrap_or(jar.len());
        let pair = &jar[start..end];
        if let Some(eq) = pair.iter().position(|&b| b == b'=')
            && !jar_has_name(&merged, &pair[..eq])
        {
            merged.extend_from_slice(b"; ");
            merged.extend_from_slice(pair);
        }
        if end == jar.len() {
            break;
        }
        start = end + 2;
    }
    ffi::set_cookie_value(js, &merged);
}
