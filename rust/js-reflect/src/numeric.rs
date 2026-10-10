//! Southstar — integer and boolean reflection: the long/unsigned/clamped attribute rules, width and height, tabIndex and the boolean attributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::controls;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi;
use crate::tables::{self, IntAttr, IntKind};
use crate::{Element, JsResult, is_named, name_is_any_of, raw_name, raw_name_is};

const HTML_UNSIGNED_CAP: i64 = 4_294_967_295;
const WIDTH_MAGIC: i32 = 8;
const ASYNC_PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

pub(crate) fn parse_int(s: &[u8]) -> Option<i64> {
    let mut p = s
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0c | b'\r'))
        .map_or(&[][..], |start| &s[start..]);
    let mut sign = 1;
    match p.first() {
        Some(b'-') => {
            sign = -1;
            p = &p[1..];
        }
        Some(b'+') => p = &p[1..],
        _ => {}
    }
    if !p.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let v = p
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i64, |v, &d| {
            (v * 10 + i64::from(d - b'0')).min(HTML_UNSIGNED_CAP)
        });
    Some(sign * v)
}

pub(crate) fn int_attr_index(attr: &[u8]) -> c_int {
    tables::INT_ATTRS
        .iter()
        .position(|d| d.attr.to_bytes() == attr)
        .map_or(-1, |i| i as c_int)
}

fn int_attr(magic: i32) -> Option<&'static IntAttr> {
    usize::try_from(magic)
        .ok()
        .and_then(|i| tables::INT_ATTRS.get(i))
}

fn reflects_as_string(n: Element, attr: &[u8]) -> bool {
    match attr {
        b"size" => raw_name_is(n, b"hr") || raw_name_is(n, b"font"),
        b"cols" | b"rows" => raw_name_is(n, b"frameset"),
        _ => false,
    }
}

fn adjusted(n: Element, def: &IntAttr, getter: bool) -> (IntKind, i32) {
    let attr = def.attr.to_bytes();
    let mut kind = def.kind;
    let mut default = def.default;
    if raw_name_is(n, b"pre") && attr == b"width" {
        kind = IntKind::Long;
    }
    if attr == b"size" {
        if raw_name_is(n, b"input") {
            default = 20;
        } else {
            kind = IntKind::ULong;
            if getter {
                default = 0;
            }
        }
    }
    if raw_name_is(n, b"canvas") {
        match attr {
            b"width" => default = 300,
            b"height" => default = 150,
            _ => {}
        }
    }
    (kind, default)
}

fn missing_dimension(scope: &Scope<'_>, n: Element, attr: &[u8]) -> Option<i32> {
    let is_width = attr == b"width";
    if !is_width && attr != b"height" {
        return None;
    }
    if raw_name_is(n, b"img") {
        let (w, h) = ffi::image_natural_size(ffi::js_of(scope), n)?;
        return Some(if is_width { w } else { h });
    }
    raw_name_is(n, b"canvas").then_some(if is_width { 300 } else { 150 })
}

pub(crate) fn int_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let Some(def) = int_attr(magic) else {
        return Ok(Value::int(0));
    };
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(Value::int(def.default));
    };
    let attr = def.attr.to_bytes();
    if reflects_as_string(n, attr) {
        return Ok(scope.string_from_bytes(ffi::attr(n, def.attr).unwrap_or_default()));
    }
    let (kind, default) = adjusted(n, def, true);
    let Some(v) = ffi::attr_c(n, def.attr) else {
        return Ok(Value::int(
            missing_dimension(scope, n, attr).unwrap_or(default),
        ));
    };
    let Some(parsed) = parse_int(v.to_bytes()) else {
        return Ok(Value::int(default));
    };
    if kind != IntKind::Long && parsed < 0 {
        return Ok(Value::int(default));
    }
    if kind == IntKind::Clamped {
        return Ok(Value::int(
            parsed.clamp(i64::from(def.lo), i64::from(def.hi)) as i32,
        ));
    }
    let floor = if kind == IntKind::LimitedULong {
        1
    } else {
        i64::from(i32::MIN)
    };
    if parsed < floor || parsed > i64::from(i32::MAX) {
        return Ok(Value::int(default));
    }
    Ok(Value::int(parsed as i32))
}

struct Decimal([u8; 24]);

impl Decimal {
    fn new(value: i64) -> Decimal {
        let mut buf = [0u8; 24];
        let mut digits = [0u8; 20];
        let mut n = value.unsigned_abs();
        let mut count = 0;
        loop {
            digits[count] = b'0' + (n % 10) as u8;
            count += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        let mut at = 0;
        if value < 0 {
            buf[0] = b'-';
            at = 1;
        }
        for i in (0..count).rev() {
            buf[at] = digits[i];
            at += 1;
        }
        Decimal(buf)
    }

    fn as_c_str(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.0).unwrap_or(c"0")
    }
}

fn index_size_error(scope: &mut Scope<'_>, message: &CStr) -> Value {
    ffi::dom_exception(scope, c"IndexSizeError", 1, message)
}

pub(crate) fn int_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    let Some(def) = int_attr(magic) else {
        return Ok(());
    };
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let js = ffi::js_of(scope);
    let attr = def.attr.to_bytes();
    if reflects_as_string(n, attr) {
        let s = ffi::to_text(scope, val)?;
        ffi::set_attr_len(js, n, def.attr, s.bytes());
        return Ok(());
    }
    let is_size = attr == b"size";
    let (kind, default) = adjusted(n, def, false);
    let store = if matches!(kind, IntKind::Long | IntKind::LimitedLong) {
        let iv = scope.to_int32(val)?;
        if kind == IntKind::LimitedLong && iv < 0 {
            return Err(index_size_error(scope, c"value must be non-negative"));
        }
        i64::from(iv)
    } else {
        let uv = scope.to_int32(val)? as u32;
        if kind == IntKind::LimitedULong && uv == 0 && is_size {
            return Err(index_size_error(scope, c"value must be greater than zero"));
        }
        let out_of_range = uv > i32::MAX as u32 || (kind == IntKind::LimitedULong && uv == 0);
        if out_of_range {
            i64::from(default)
        } else {
            i64::from(uv)
        }
    };
    ffi::set_attr(js, n, def.attr, Decimal::new(store).as_c_str());
    Ok(())
}

pub(crate) fn dimension_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    if let Some(n) = ffi::unwrap_node(this)
        .filter(|&n| raw_name(n).is_some_and(|name| tables::STRING_DIMENSION_TAGS.contains(&name)))
    {
        let s = ffi::to_text(scope, val)?;
        let attr = if magic == WIDTH_MAGIC {
            c"width"
        } else {
            c"height"
        };
        ffi::set_attr_len(ffi::js_of(scope), n, attr, s.bytes());
        return Ok(());
    }
    int_set(scope, this, val, magic)
}

pub(crate) fn tab_index_get(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(Value::int(-1));
    };
    let Some(v) = ffi::attr_c(n, c"tabindex") else {
        let focusable = name_is_any_of(n, tables::FOCUSABLE_TAGS)
            || (is_named(n, b"summary") && n.parent().is_some_and(|p| is_named(p, b"details")));
        return Ok(Value::int(if focusable { 0 } else { -1 }));
    };
    Ok(Value::int(controls::parse_int(
        Some(v),
        0,
        i32::MIN,
        i32::MAX,
    )))
}

pub(crate) fn tab_index_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let iv = scope.to_int32(val)?;
    ffi::set_attr(
        ffi::js_of(scope),
        n,
        c"tabindex",
        Decimal::new(i64::from(iv)).as_c_str(),
    );
    Ok(())
}

fn bool_name(names: &'static [&'static CStr], magic: i32) -> Option<&'static CStr> {
    usize::try_from(magic)
        .ok()
        .and_then(|i| names.get(i).copied())
}

pub(crate) fn plain_bool_get(_scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let present = ffi::unwrap_node(this)
        .zip(bool_name(tables::PLAIN_BOOLEANS, magic))
        .is_some_and(|(n, name)| ffi::attr(n, name).is_some());
    Ok(Value::boolean(present))
}

fn toggle(scope: &mut Scope<'_>, n: Element, name: &CStr, on: bool) {
    let js = ffi::js_of(scope);
    if on {
        ffi::set_attr(js, n, name, c"");
    } else {
        ffi::remove_attr(js, n, name);
    }
}

pub(crate) fn plain_bool_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    if let Some((n, name)) = ffi::unwrap_node(this).zip(bool_name(tables::PLAIN_BOOLEANS, magic)) {
        let on = scope.to_bool(val);
        toggle(scope, n, name, on);
    }
    Ok(())
}

fn is_async_method(n: Element, name: &CStr) -> bool {
    name.to_bytes() == b"async" && !is_named(n, b"script")
}

pub(crate) fn bool_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let Some((n, name)) = ffi::unwrap_node(this).zip(bool_name(tables::BOOLEANS, magic)) else {
        return Ok(Value::boolean(false));
    };
    if is_async_method(n, name) {
        return Ok(ffi::async_method(scope));
    }
    Ok(Value::boolean(ffi::attr(n, name).is_some()))
}

pub(crate) fn bool_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    let Some((n, name)) = ffi::unwrap_node(this).zip(bool_name(tables::BOOLEANS, magic)) else {
        return Ok(());
    };
    if is_async_method(n, name) {
        ffi::define_own(scope, this, name, val, ASYNC_PROPERTY);
        return Ok(());
    }
    let on = scope.to_bool(val);
    let was_on = ffi::attr(n, name).is_some();
    toggle(scope, n, name, on);
    if name.to_bytes() == b"open" && was_on != on && is_named(n, b"details") {
        ffi::details_toggle_open(ffi::js_of(scope), n, on);
    }
    Ok(())
}
