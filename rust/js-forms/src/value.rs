//! Southstar — form-control values: value and defaultValue, sanitization, valueAsNumber/valueAsDate, progress and meter.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};

use southstar_datetime as dt;
use southstar_dom::{controls, select as dom_select};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Page, cstring};
use crate::{Element, JsResult, attr_bytes, c_bytes, name_is, named, selection, type_is, type_of};

const DAY_MS: f64 = 86_400_000.0;
const HTML_MAXINT: i64 = 2_147_483_647;
const HTML_MININT: i64 = -2_147_483_648;
const RANGE_ATTRS: [&CStr; 5] = [c"min", c"max", c"low", c"high", c"optimum"];
const RANGE_MIN: c_int = 0;
const RANGE_MAX: c_int = 1;
const RANGE_LOW: c_int = 2;
const RANGE_HIGH: c_int = 3;
const RANGE_OPTIMUM: c_int = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputKind {
    Other,
    Number,
    Range,
    Date,
    Month,
    Week,
    Time,
    DateTime,
}

pub(crate) fn kind_of(node: Element) -> InputKind {
    if !named(node, "input") {
        return InputKind::Other;
    }
    let Some(ty) = type_of(node) else {
        return InputKind::Other;
    };
    match ty.to_bytes().to_ascii_lowercase().as_slice() {
        b"number" => InputKind::Number,
        b"range" => InputKind::Range,
        b"date" => InputKind::Date,
        b"month" => InputKind::Month,
        b"week" => InputKind::Week,
        b"time" => InputKind::Time,
        b"datetime-local" => InputKind::DateTime,
        _ => InputKind::Other,
    }
}

fn floordiv(a: c_long, b: c_long) -> c_long {
    let (q, r) = (a / b, a % b);
    if r != 0 && ((r < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

pub(crate) fn value_to_ms(kind: InputKind, v: Option<&[u8]>, as_date: bool) -> Option<f64> {
    let v = v.filter(|v| !v.is_empty())?;
    let whole = |n: usize| n == v.len();
    match kind {
        InputKind::Number | InputKind::Range => {
            if as_date {
                return None;
            }
            let (d, n) = southstar_glib::ascii_strtod_prefix(v);
            let rest_blank = v[n..].iter().all(|&c| c == b' ' || c == b'\t');
            (n > 0 && rest_blank && d.is_finite()).then_some(d)
        }
        InputKind::Date => {
            let (n, y, m, d) = dt::read_date(v)?;
            whole(n).then(|| dt::days_from_civil(y, m, d) as f64 * DAY_MS)
        }
        InputKind::Month => {
            let (n, y) = dt::read_digits(v, 4, 9)?;
            if v.get(n) != Some(&b'-') {
                return None;
            }
            let (k, m) = dt::read_digits(&v[n + 1..], 2, 2)?;
            if !whole(n + 1 + k) || !(1..=dt::MAX_YEAR).contains(&y) || !(1..=12).contains(&m) {
                return None;
            }
            Some(if as_date {
                dt::days_from_civil(y, m, 1) as f64 * DAY_MS
            } else {
                f64::from((y - 1970) * 12 + (m - 1))
            })
        }
        InputKind::Week => {
            let (n, y) = dt::read_digits(v, 4, 9)?;
            if v.get(n) != Some(&b'-') || v.get(n + 1) != Some(&b'W') {
                return None;
            }
            let (k, w) = dt::read_digits(&v[n + 2..], 2, 2)?;
            if !whole(n + 2 + k)
                || !(1..=dt::MAX_YEAR).contains(&y)
                || w < 1
                || w > dt::iso_weeks_in_year(y)
            {
                return None;
            }
            let monday = dt::iso_week1_monday(y) + c_long::from(w - 1) * 7;
            Some(monday as f64 * DAY_MS)
        }
        InputKind::Time => {
            let (n, ms) = dt::read_time(v)?;
            whole(n).then_some(f64::from(ms))
        }
        InputKind::DateTime => {
            if as_date {
                return None;
            }
            let (n, y, m, d) = dt::read_date(v)?;
            if v.get(n) != Some(&b'T') && v.get(n) != Some(&b' ') {
                return None;
            }
            let (k, ms) = dt::read_time(&v[n + 1..])?;
            whole(n + 1 + k).then(|| dt::days_from_civil(y, m, d) as f64 * DAY_MS + f64::from(ms))
        }
        InputKind::Other => None,
    }
}

fn number_to_string(d: f64) -> Vec<u8> {
    let mut text = Vec::new();
    for precision in 1..=17 {
        let format = cstring(format!("%.{precision}g").as_bytes());
        text = ffi::format_double(&format, d);
        if southstar_glib::ascii_strtod(&text) == d {
            break;
        }
    }
    text
}

fn append_time(out: &mut String, ms_of_day: c_long) {
    let mut ms = dt::floormod(ms_of_day, 86_400_000);
    let h = ms / 3_600_000;
    ms %= 3_600_000;
    let mi = ms / 60_000;
    ms %= 60_000;
    let se = ms / 1000;
    let fr = ms % 1000;
    if fr != 0 {
        out.push_str(&format!("{h:02}:{mi:02}:{se:02}.{fr:03}"));
    } else if se != 0 {
        out.push_str(&format!("{h:02}:{mi:02}:{se:02}"));
    } else {
        out.push_str(&format!("{h:02}:{mi:02}"));
    }
}

pub(crate) fn ms_to_value(kind: InputKind, v: f64) -> Option<Vec<u8>> {
    if !v.is_finite() {
        return None;
    }
    let limit = match kind {
        InputKind::Number | InputKind::Range => f64::INFINITY,
        InputKind::Month => 1.0e8,
        InputKind::Time => 1.0e9,
        _ => 8.64e15,
    };
    if !(-limit..=limit).contains(&v) {
        return None;
    }
    let mut out = String::new();
    match kind {
        InputKind::Number | InputKind::Range => return Some(number_to_string(v)),
        InputKind::Date => {
            let (y, m, d) = dt::civil_from_days((v / DAY_MS).floor() as c_long);
            out.push_str(&format!("{y:04}-{m:02}-{d:02}"));
        }
        InputKind::Month => {
            let months = v.floor() as c_long;
            let y = 1970 + floordiv(months, 12) as c_int;
            let m = dt::floormod(months, 12) as c_int + 1;
            out.push_str(&format!("{y:04}-{m:02}"));
        }
        InputKind::Week => {
            let monday = (v / DAY_MS).floor() as c_long;
            let (ty, _, _) = dt::civil_from_days(monday + 3);
            let week = ((monday - dt::iso_week1_monday(ty)) / 7) as c_int + 1;
            out.push_str(&format!("{ty:04}-W{week:02}"));
        }
        InputKind::Time => append_time(&mut out, v.floor() as c_long),
        InputKind::DateTime => {
            let days = (v / DAY_MS).floor() as c_long;
            let (y, m, d) = dt::civil_from_days(days);
            out.push_str(&format!("{y:04}-{m:02}-{d:02}T"));
            append_time(&mut out, (v - days as f64 * DAY_MS) as c_long);
        }
        InputKind::Other => return None,
    }
    Some(out.into_bytes())
}

fn used_value(node: Element) -> Option<&'static [u8]> {
    controls::used_value(node).map(CStr::to_bytes)
}

pub(crate) fn set_used_value(page: Option<Page>, node: Element, value: &[u8]) {
    let attr = if controls::value_is_dirty_mode(node) {
        c"data-nd-value"
    } else {
        c"value"
    };
    ffi::set_attr(node, attr, value);
    ffi::remove_attr(node, c"data-nd-user-edited");
    Page::mark_mutated(page);
}

fn strip_newlines(value: &[u8]) -> Vec<u8> {
    value
        .iter()
        .copied()
        .filter(|&c| c != b'\r' && c != b'\n')
        .collect()
}

fn strip(value: &[u8]) -> &[u8] {
    let space = |c: &u8| matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r');
    let start = value.iter().take_while(|c| space(c)).count();
    let end = value.len() - value[start..].iter().rev().take_while(|c| space(c)).count();
    &value[start..end]
}

fn sanitize_email(el: Element, value: &[u8]) -> Vec<u8> {
    let joined = strip_newlines(value);
    if el.attr(c"multiple").is_none() {
        return strip(&joined).to_vec();
    }
    let tokens: Vec<&[u8]> = joined.split(|&c| c == b',').map(strip).collect();
    tokens.join(&b',')
}

fn is_simple_color(s: &[u8]) -> bool {
    s.len() == 7 && s[0] == b'#' && s[1..].iter().all(u8::is_ascii_hexdigit)
}

fn sanitize_color(value: &[u8]) -> Vec<u8> {
    let trimmed = strip(value);
    if is_simple_color(trimmed) {
        return trimmed.to_ascii_lowercase();
    }
    if !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case(b"currentcolor")
        && let Some((r, g, b, 255)) = ffi::parse_color(trimmed)
    {
        return format!("#{r:02x}{g:02x}{b:02x}").into_bytes();
    }
    b"#000000".to_vec()
}

fn number_of(ty: &CStr, value: &[u8]) -> Option<f64> {
    let value = cstring(value);
    controls::value_to_number(Some(ty), Some(&value))
}

fn sanitize_range(el: Element, value: &[u8]) -> Vec<u8> {
    let lo = attr_bytes(el, c"min")
        .and_then(|m| number_of(c"range", m))
        .unwrap_or(0.0);
    let hi = attr_bytes(el, c"max")
        .and_then(|m| number_of(c"range", m))
        .unwrap_or(100.0)
        .max(lo);
    let v = match number_of(c"range", value) {
        None => lo + (hi - lo) / 2.0,
        Some(v) if v < lo => lo,
        Some(v) if v > hi => hi,
        Some(v) => v,
    };
    ffi::format_double(c"%g", v)
}

pub(crate) fn sanitize(el: Element, value: &[u8]) -> Vec<u8> {
    if !named(el, "input") {
        return value.to_vec();
    }
    let ty = type_of(el).map_or_else(|| b"text".to_vec(), |t| t.to_bytes().to_ascii_lowercase());
    match ty.as_slice() {
        b"email" => sanitize_email(el, value),
        b"url" => strip(&strip_newlines(value)).to_vec(),
        b"number" => {
            if number_of(c"number", value).is_some() {
                value.to_vec()
            } else {
                Vec::new()
            }
        }
        b"range" => sanitize_range(el, value),
        b"color" => sanitize_color(value),
        b"date" | b"month" | b"week" | b"time" | b"datetime-local" => {
            if value_to_ms(kind_of(el), Some(value), false).is_some() {
                value.to_vec()
            } else {
                Vec::new()
            }
        }
        b"file" => Vec::new(),
        b"hidden" | b"submit" | b"reset" | b"button" | b"image" | b"checkbox" | b"radio" => {
            value.to_vec()
        }
        _ => strip_newlines(value),
    }
}

pub(crate) fn resanitize(el: Element) {
    if !named(el, "input") || el.attr(c"data-nd-vdirty").is_none() {
        return;
    }
    let Some(current) = attr_bytes(el, c"data-nd-value").or_else(|| attr_bytes(el, c"value"))
    else {
        return;
    };
    let sanitized = sanitize(el, current);
    if controls::value_is_dirty_mode(el) {
        ffi::set_attr(el, c"data-nd-value", &sanitized);
    } else {
        ffi::set_attr(el, c"value", &sanitized);
        ffi::remove_attr(el, c"data-nd-value");
    }
}

fn attr_float(node: Element, attr: &CStr) -> Option<f64> {
    attr_bytes(node, attr).and_then(southstar_html_util::parse_float)
}

struct Progress {
    max: f64,
    value: f64,
    position: f64,
}

fn progress_state(n: Element) -> Progress {
    let max = attr_float(n, c"max").filter(|&p| p > 0.0).unwrap_or(1.0);
    if n.attr(c"value").is_none() {
        return Progress {
            max,
            value: 0.0,
            position: -1.0,
        };
    }
    let value = attr_float(n, c"value")
        .filter(|&p| p > 0.0)
        .unwrap_or(0.0)
        .min(max);
    Progress {
        max,
        value,
        position: if max > 0.0 { value / max } else { 0.0 },
    }
}

struct Meter {
    min: f64,
    max: f64,
    value: f64,
    low: f64,
    high: f64,
    optimum: f64,
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    let v = if v < lo { lo } else { v };
    if v > hi { hi } else { v }
}

fn meter_state(n: Element) -> Meter {
    let min = attr_float(n, c"min").unwrap_or(0.0);
    let candidate_max = attr_float(n, c"max").unwrap_or(1.0);
    let max = if candidate_max >= min {
        candidate_max
    } else {
        min
    };
    let value = clamp(attr_float(n, c"value").unwrap_or(0.0), min, max);
    let low = clamp(attr_float(n, c"low").unwrap_or(min), min, max);
    let high = clamp(attr_float(n, c"high").unwrap_or(max), low, max);
    let midpoint = min + (max - min) / 2.0;
    let optimum = clamp(attr_float(n, c"optimum").unwrap_or(midpoint), min, max);
    Meter {
        min,
        max,
        value,
        low,
        high,
        optimum,
    }
}

fn set_double_attr(
    scope: &mut Scope<'_>,
    el: Element,
    attr: &CStr,
    value: &Value,
    positive_only: bool,
) -> JsResult {
    let d = scope.to_number(value)?;
    if !d.is_finite() {
        return Err(scope.type_error("The value provided is non-finite."));
    }
    if positive_only && d <= 0.0 {
        return Ok(Value::undefined());
    }
    if let Ok(text) = scope.to_bytes(&Value::number(d)) {
        Page::set_attr_recorded(Page::of(scope), el, attr, &text);
    }
    Ok(Value::undefined())
}

fn reflect_get(scope: &mut Scope<'_>, el: Option<Element>, attr: &CStr) -> Value {
    let value = el
        .and_then(|e| e.attr(attr))
        .map_or(&[][..], CStr::to_bytes);
    scope.string_from_bytes(value)
}

fn reflect_set(scope: &mut Scope<'_>, el: Element, value: &Value, attr: &CStr) -> JsResult {
    let text = scope.to_bytes(value)?;
    Page::set_attr_recorded_len(Page::of(scope), el, attr, &text);
    Ok(Value::undefined())
}

fn arg(args: &[Value]) -> Value {
    args.first().cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn range_number(scope: &mut Scope<'_>, this: &Value, magic: c_int) -> JsResult {
    let Some(el) = ffi::element(this).filter(|e| e.name().is_some()) else {
        return Ok(scope.string(""));
    };
    if name_is(el, "meter") {
        let st = meter_state(el);
        let number = match magic {
            RANGE_MIN => Some(st.min),
            RANGE_MAX => Some(st.max),
            RANGE_LOW => Some(st.low),
            RANGE_HIGH => Some(st.high),
            RANGE_OPTIMUM => Some(st.optimum),
            _ => None,
        };
        if let Some(number) = number {
            return Ok(Value::number(number));
        }
    }
    if name_is(el, "progress") && magic == RANGE_MAX {
        return Ok(Value::number(progress_state(el).max));
    }
    match usize::try_from(magic).ok().and_then(|m| RANGE_ATTRS.get(m)) {
        Some(attr) => Ok(reflect_get(scope, Some(el), attr)),
        None => Ok(scope.string("")),
    }
}

pub(crate) fn set_range_number(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: c_int,
) -> JsResult {
    let Some(el) = ffi::element(this).filter(|e| e.name().is_some()) else {
        return Ok(Value::undefined());
    };
    let Some(attr) = usize::try_from(magic).ok().and_then(|m| RANGE_ATTRS.get(m)) else {
        return Ok(Value::undefined());
    };
    if name_is(el, "meter") {
        return set_double_attr(scope, el, attr, value, false);
    }
    if name_is(el, "progress") && magic == RANGE_MAX {
        return set_double_attr(scope, el, attr, value, true);
    }
    reflect_set(scope, el, value, attr)
}

pub(crate) fn progress_position(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    match ffi::element(this).filter(|e| name_is(*e, "progress")) {
        Some(el) => Ok(Value::number(progress_state(el).position)),
        None => Ok(Value::undefined()),
    }
}

fn parse_html_int(s: &[u8]) -> Option<i64> {
    let start = s
        .iter()
        .take_while(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r'))
        .count();
    let mut rest = &s[start..];
    let mut sign = 1;
    match rest.first() {
        Some(b'-') => {
            sign = -1;
            rest = &rest[1..];
        }
        Some(b'+') => rest = &rest[1..],
        _ => {}
    }
    if !rest.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let value = rest
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0i64, |v, &d| {
            (v * 10 + i64::from(d - b'0')).min(4_294_967_295)
        });
    Some(sign * value)
}

pub(crate) fn get_value(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(el) = ffi::element(this) else {
        return Ok(scope.string(""));
    };
    let text = match el.element_name() {
        Some(b"progress") => return Ok(Value::number(progress_state(el).value)),
        Some(b"meter") => return Ok(Value::number(meter_state(el).value)),
        Some(b"li") => {
            let n = attr_bytes(el, c"value")
                .and_then(parse_html_int)
                .filter(|n| (HTML_MININT..=HTML_MAXINT).contains(n))
                .unwrap_or(0);
            return Ok(Value::int(n as i32));
        }
        Some(b"data") => return Ok(reflect_get(scope, Some(el), c"value")),
        Some(b"textarea") => controls::textarea_value(Some(el)),
        Some(b"output") => el.collect_text(),
        Some(b"select") => crate::select::value(el),
        Some(b"option") => dom_select::option_value(el),
        Some(b"input") => {
            let mut v = used_value(el).unwrap_or_default();
            if (type_is(el, "checkbox") || type_is(el, "radio")) && el.attr(c"value").is_none() {
                v = b"on";
            }
            sanitize(el, v)
        }
        _ => used_value(el).unwrap_or_default().to_vec(),
    };
    Ok(scope.string_from_bytes(&text))
}

fn is_custom_element(el: Element) -> bool {
    el.element_name().is_some_and(|n| n.contains(&b'-'))
}

fn set_output(page: Option<Page>, el: Element, text: &[u8]) {
    if el.attr(c"data-nd-output-dirty").is_none() {
        let current = el.collect_text();
        ffi::set_attr(el, c"data-nd-output-default", &current);
        ffi::set_attr(el, c"data-nd-output-dirty", b"");
    }
    Page::clear_children(page, el);
    if !text.is_empty() {
        ffi::append_text(el, text);
    }
    Page::mark_mutated(page);
}

pub(crate) fn set_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = arg(args);
    let Some(el) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    if is_custom_element(el) {
        let all = Attributes {
            writable: true,
            enumerable: true,
            configurable: true,
        };
        scope.define(this, "value", value, all)?;
        return Ok(Value::undefined());
    }
    match el.element_name() {
        Some(b"progress" | b"meter") => return set_double_attr(scope, el, c"value", &value, false),
        Some(b"li") => {
            let n = scope.to_int32(&value)?;
            Page::set_attr_recorded(Page::of(scope), el, c"value", n.to_string().as_bytes());
            return Ok(Value::undefined());
        }
        Some(b"data") => return reflect_set(scope, el, &value, c"value"),
        _ => {}
    }
    let null_to_empty = value.is_null() && (named(el, "input") || named(el, "textarea"));
    let selection_applies = selection::applies(el);
    let old_value = if selection_applies {
        get_value(scope, this, &[])?
    } else {
        Value::undefined()
    };
    let text = if null_to_empty {
        Vec::new()
    } else {
        crate::until_nul(scope.to_bytes(&value)?)
    };
    let page = Page::of(scope);
    match el.element_name() {
        Some(b"select") => {
            crate::select::select_value(page, el, &text);
            return Ok(Value::undefined());
        }
        Some(b"output") => {
            set_output(page, el, &text);
            return Ok(Value::undefined());
        }
        Some(b"textarea") => {
            controls::set_editable_value(el, &cstring(&text));
            ffi::remove_attr(el, c"data-nd-user-edited");
            selection::value_changed(scope, this, &old_value);
            Page::mark_mutated(page);
            return Ok(Value::undefined());
        }
        Some(b"input") if type_is(el, "file") && !text.is_empty() => {
            return Err(ffi::dom_exception(
                scope,
                c"InvalidStateError",
                11,
                "This input element accepts a filename, which may only be programmatically set to the empty string.",
            ));
        }
        _ => {}
    }
    let sanitized = sanitize(el, &text);
    let attr = if controls::value_is_dirty_mode(el) {
        c"data-nd-value"
    } else {
        c"value"
    };
    ffi::set_attr(el, attr, &sanitized);
    ffi::remove_attr(el, c"data-nd-user-edited");
    if named(el, "input") {
        ffi::set_attr(el, c"data-nd-vdirty", b"1");
    }
    if selection_applies {
        selection::value_changed(scope, this, &old_value);
    }
    Page::mark_mutated(page);
    Ok(Value::undefined())
}

pub(crate) fn default_value(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let el = ffi::element(this);
    let text = match el {
        Some(n) if named(n, "output") => {
            if n.attr(c"data-nd-output-dirty").is_some() {
                attr_bytes(n, c"data-nd-output-default")
                    .unwrap_or_default()
                    .to_vec()
            } else {
                n.collect_text()
            }
        }
        Some(n) if named(n, "textarea") => controls::textarea_default_value(Some(n)),
        _ => return Ok(reflect_get(scope, el, c"value")),
    };
    Ok(scope.string_from_bytes(&text))
}

pub(crate) fn set_default_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = arg(args);
    let Some(el) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let output = named(el, "output");
    if !output && !named(el, "textarea") {
        return reflect_set(scope, el, &value, c"value");
    }
    let text = c_bytes(scope, &value).unwrap_or_default();
    let page = Page::of(scope);
    if output {
        ffi::set_attr(el, c"data-nd-output-default", &text);
        if el.attr(c"data-nd-output-dirty").is_none() {
            Page::clear_children(page, el);
            if !text.is_empty() {
                ffi::append_text(el, &text);
            }
        }
    } else {
        Page::clear_children(page, el);
        if !text.is_empty() {
            ffi::append_text(el, &text);
        }
    }
    Page::mark_mutated(page);
    Ok(Value::undefined())
}

pub(crate) fn value_as_number(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let number = ffi::element(this)
        .and_then(|n| value_to_ms(kind_of(n), used_value(n), false))
        .unwrap_or(f64::NAN);
    Ok(Value::number(number))
}

pub(crate) fn set_value_as_number(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(n) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let kind = kind_of(n);
    if kind == InputKind::Other {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            "valueAsNumber is not applicable to this input type.",
        ));
    }
    let d = scope.to_number(&arg(args))?;
    if d.is_infinite() {
        return Err(scope.type_error("The value provided is non-finite."));
    }
    let page = Page::of(scope);
    if d.is_nan() {
        set_used_value(page, n, b"");
    } else if let Some(text) = ms_to_value(kind, d) {
        set_used_value(page, n, &text);
    }
    Ok(Value::undefined())
}

fn date_kind(kind: InputKind) -> bool {
    matches!(
        kind,
        InputKind::Date | InputKind::Month | InputKind::Week | InputKind::Time
    )
}

pub(crate) fn value_as_date(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(n) = ffi::element(this) else {
        return Ok(Value::null());
    };
    let kind = kind_of(n);
    if !date_kind(kind) {
        return Ok(Value::null());
    }
    let Some(ms) = value_to_ms(kind, used_value(n), true) else {
        return Ok(Value::null());
    };
    let global = scope.global();
    let date = scope.get(&global, "Date")?;
    scope.construct(&date, &[Value::number(ms)])
}

pub(crate) fn set_value_as_date(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(n) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let kind = kind_of(n);
    if !date_kind(kind) {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            11,
            "valueAsDate is not applicable to this input type.",
        ));
    }
    let value = arg(args);
    let page = Page::of(scope);
    if value.is_null() {
        set_used_value(page, n, b"");
        return Ok(Value::undefined());
    }
    let get_time = scope
        .get(&value, "getTime")
        .unwrap_or_else(|_| Value::undefined());
    if !scope.is_function(&get_time) {
        return Err(scope.type_error("The value provided is not a Date."));
    }
    let ms = scope
        .call(&get_time, &value, &[])
        .and_then(|r| scope.to_number(&r))
        .ok()
        .filter(|ms| !ms.is_nan());
    let Some(ms) = ms else {
        set_used_value(page, n, b"");
        return Ok(Value::undefined());
    };
    let serialize_kind = if kind == InputKind::Month {
        InputKind::Date
    } else {
        kind
    };
    if let Some(mut text) = ms_to_value(serialize_kind, ms) {
        if kind == InputKind::Month
            && let Some(cut) = text.iter().rposition(|&c| c == b'-')
        {
            text.truncate(cut);
        }
        set_used_value(page, n, &text);
    }
    Ok(Value::undefined())
}

pub(crate) fn label(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(el) = ffi::element(this).filter(|e| e.element_name().is_some()) else {
        return Ok(Value::undefined());
    };
    if name_is(el, "option") {
        let text = dom_select::option_label(el);
        return Ok(scope.string_from_bytes(&text));
    }
    if name_is(el, "optgroup") || name_is(el, "track") {
        return Ok(reflect_get(scope, Some(el), c"label"));
    }
    Ok(Value::undefined())
}

pub(crate) fn set_label(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(el) = ffi::element(this).filter(|e| e.element_name().is_some()) else {
        return Ok(Value::undefined());
    };
    if !(name_is(el, "option") || name_is(el, "optgroup") || name_is(el, "track")) {
        return Ok(Value::undefined());
    }
    reflect_set(scope, el, &arg(args), c"label")
}
