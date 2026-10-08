//! Southstar — form controls over the DOM: input types and their numeric values, stepping, validity, email syntax and editable values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};

use southstar_datetime::{MAX_YEAR, civil_from_days, floormod};

use crate::ffi::{self, Node};
use crate::{MAX_DEPTH, ancestors, ancestors_and_self, children, index, tree};

const DAY_MS: f64 = 86_400_000.0;
const WEEK_MS: f64 = 604_800_000.0;
const WEEK_BASE_MS: f64 = -259_200_000.0;

#[derive(Clone, Copy, PartialEq)]
pub enum InputKind {
    Other,
    Number,
    Range,
    Date,
    Month,
    Week,
    Time,
    DateTime,
}

pub enum Step {
    Applied(Vec<u8>),
    NotApplicable,
    NoStep,
    Unchanged,
}

fn ieq(value: &CStr, word: &str) -> bool {
    value.to_bytes().eq_ignore_ascii_case(word.as_bytes())
}

fn type_in(ty: Option<&CStr>, words: &[&str]) -> bool {
    ty.is_some_and(|t| words.iter().any(|w| ieq(t, w)))
}

fn empty_or_missing(ty: Option<&CStr>) -> bool {
    ty.is_none_or(CStr::is_empty)
}

fn named(node: Node, name: &[u8]) -> bool {
    node.element_name() == Some(name)
}

fn raw_name_is(node: Node, name: &[u8]) -> bool {
    node.name().is_some_and(|n| n.to_bytes() == name)
}

fn c_trunc_long(d: f64) -> c_long {
    let (low, high) = if cfg!(windows) {
        (-2_147_483_649.0, 2_147_483_648.0)
    } else {
        (-9_223_372_036_854_775_809.0, 9_223_372_036_854_775_808.0)
    };
    if cfg!(any(target_arch = "x86", target_arch = "x86_64")) && !(d > low && d < high) {
        return c_long::MIN;
    }
    d as c_long
}

fn ascii_space(c: u8) -> bool {
    matches!(c, b' ' | b'\x0c' | b'\n' | b'\r' | b'\t' | b'\x0b')
}

fn ascii_strtoll(s: &[u8]) -> Option<(i64, bool)> {
    let mut i = s.iter().take_while(|&&c| ascii_space(c)).count();
    let negative = s.get(i) == Some(&b'-');
    if negative || s.get(i) == Some(&b'+') {
        i += 1;
    }
    let digits = s[i.min(s.len())..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let mut value: u64 = 0;
    let mut overflow = false;
    for &c in &s[i..i + digits] {
        match value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(c - b'0')))
        {
            Some(v) => value = v,
            None => overflow = true,
        }
    }
    if overflow {
        return Some((if negative { i64::MIN } else { i64::MAX }, true));
    }
    if negative {
        if value > 1 << 63 {
            return Some((i64::MIN, true));
        }
        return Some(((value as i64).wrapping_neg(), false));
    }
    if value > i64::MAX as u64 {
        return Some((i64::MAX, true));
    }
    Some((value as i64, false))
}

pub fn parse_int(s: Option<&CStr>, default: c_int, min: c_int, max: c_int) -> c_int {
    let Some(s) = s.map(CStr::to_bytes).filter(|s| !s.is_empty()) else {
        return default;
    };
    let s = &s[s.iter().take_while(|&&c| c == b' ' || c == b'\t').count()..];
    if s.is_empty() {
        return default;
    }
    let Some((mut v, range_error)) = ascii_strtoll(s) else {
        return default;
    };
    if range_error {
        v = if v < 0 {
            i64::from(min)
        } else {
            i64::from(max)
        };
    }
    if v < i64::from(min) {
        v = i64::from(min);
    }
    if v > i64::from(max) {
        v = i64::from(max);
    }
    v as c_int
}

pub fn input_kind(ty: Option<&CStr>) -> InputKind {
    let Some(ty) = ty else {
        return InputKind::Other;
    };
    [
        ("number", InputKind::Number),
        ("range", InputKind::Range),
        ("date", InputKind::Date),
        ("month", InputKind::Month),
        ("week", InputKind::Week),
        ("time", InputKind::Time),
        ("datetime-local", InputKind::DateTime),
    ]
    .into_iter()
    .find(|(name, _)| ieq(ty, name))
    .map_or(InputKind::Other, |(_, kind)| kind)
}

pub fn type_has_number_value(ty: Option<&CStr>) -> bool {
    input_kind(ty) != InputKind::Other
}

pub fn type_supports_readonly(ty: Option<&CStr>) -> bool {
    empty_or_missing(ty)
        || type_in(
            ty,
            &[
                "text",
                "search",
                "url",
                "tel",
                "email",
                "password",
                "date",
                "month",
                "week",
                "time",
                "datetime-local",
                "number",
            ],
        )
}

pub fn type_supports_text_constraints(ty: Option<&CStr>) -> bool {
    empty_or_missing(ty) || type_in(ty, &["text", "search", "url", "tel", "email", "password"])
}

pub fn readonly_bars_validation(control: Node) -> bool {
    let Some(name) = control.element_name() else {
        return false;
    };
    if control.attr(c"readonly").is_none() {
        return false;
    }
    match name {
        b"textarea" => true,
        b"input" => type_supports_readonly(control.attr(c"type")),
        _ => false,
    }
}

pub fn length_limits_apply(control: Node) -> bool {
    match control.element_name() {
        Some(b"textarea") => true,
        Some(b"input") => type_supports_text_constraints(control.attr(c"type")),
        _ => false,
    }
}

pub fn supports_required(control: Node) -> bool {
    match control.element_name() {
        Some(b"textarea" | b"select") => true,
        Some(b"input") => {
            let ty = control.attr(c"type");
            empty_or_missing(ty)
                || !type_in(
                    ty,
                    &[
                        "hidden", "range", "color", "submit", "image", "reset", "button",
                    ],
                )
        }
        _ => false,
    }
}

fn parse_finite_double(v: Option<&CStr>) -> Option<f64> {
    let v = v.map(CStr::to_bytes).filter(|v| !v.is_empty())?;
    let (d, consumed) = southstar_glib::ascii_strtod_prefix(v);
    if consumed == 0 {
        return None;
    }
    let rest = &v[consumed..];
    let trailing = rest
        .iter()
        .take_while(|&&c| c == b' ' || c == b'\t')
        .count();
    (trailing == rest.len() && d.is_finite()).then_some(d)
}

fn read_digits(s: &[u8], min: c_int, max: c_int) -> Option<(usize, c_int)> {
    southstar_datetime::read_digits(s, min, max)
}

pub fn value_to_number(ty: Option<&CStr>, value: Option<&CStr>) -> Option<f64> {
    let value = value.filter(|v| !v.is_empty())?;
    let s = value.to_bytes();
    match input_kind(ty) {
        InputKind::Number | InputKind::Range => parse_finite_double(Some(value)),
        InputKind::Date => {
            let (n, y, m, d) = southstar_datetime::read_date(s)?;
            (n == s.len()).then(|| southstar_datetime::days_from_civil(y, m, d) as f64 * DAY_MS)
        }
        InputKind::Month => {
            let (n, y) = read_digits(s, 4, 9)?;
            if s.get(n) != Some(&b'-') {
                return None;
            }
            let (k, m) = read_digits(&s[n + 1..], 2, 2)?;
            let valid =
                n + 1 + k == s.len() && (1..=MAX_YEAR).contains(&y) && (1..=12).contains(&m);
            valid.then(|| f64::from((y - 1970) * 12 + (m - 1)))
        }
        InputKind::Week => {
            let (n, y) = read_digits(s, 4, 9)?;
            if s.get(n) != Some(&b'-') || s.get(n + 1) != Some(&b'W') {
                return None;
            }
            let (k, w) = read_digits(&s[n + 2..], 2, 2)?;
            let valid = n + 2 + k == s.len()
                && (1..=MAX_YEAR).contains(&y)
                && w >= 1
                && w <= southstar_datetime::iso_weeks_in_year(y);
            if !valid {
                return None;
            }
            let monday = southstar_datetime::iso_week1_monday(y) + c_long::from(w - 1) * 7;
            Some(monday as f64 * DAY_MS)
        }
        InputKind::Time => {
            let (n, ms) = southstar_datetime::read_time(s)?;
            (n == s.len()).then_some(f64::from(ms))
        }
        InputKind::DateTime => {
            let (n, y, m, d) = southstar_datetime::read_date(s)?;
            if s.get(n) != Some(&b'T') && s.get(n) != Some(&b' ') {
                return None;
            }
            let (k, ms) = southstar_datetime::read_time(&s[n + 1..])?;
            (n + 1 + k == s.len()).then(|| {
                southstar_datetime::days_from_civil(y, m, d) as f64 * DAY_MS + f64::from(ms)
            })
        }
        InputKind::Other => None,
    }
}

pub fn value_range_state(input: Node, value: Option<&CStr>) -> Option<(bool, bool)> {
    if !named(input, b"input") {
        return None;
    }
    let ty = input.attr(c"type");
    let v = value_to_number(ty, value)?;
    let under = value_to_number(ty, input.attr(c"min")).is_some_and(|min| v < min);
    let over = value_to_number(ty, input.attr(c"max")).is_some_and(|max| v > max);
    Some((under, over))
}

fn step_scale(kind: InputKind) -> f64 {
    match kind {
        InputKind::Date => DAY_MS,
        InputKind::Week => WEEK_MS,
        InputKind::Time | InputKind::DateTime => 1000.0,
        _ => 1.0,
    }
}

fn default_step(kind: InputKind) -> f64 {
    match kind {
        InputKind::Time | InputKind::DateTime => 60.0,
        _ => 1.0,
    }
}

fn serialize_time(out: &mut String, ms: c_int) {
    let h = ms / 3_600_000;
    let mi = (ms / 60_000) % 60;
    let s = (ms / 1000) % 60;
    let frac = ms % 1000;
    out.push_str(&format!("{h:02}:{mi:02}"));
    if s != 0 || frac != 0 {
        out.push_str(&format!(":{s:02}"));
    }
    if frac != 0 {
        out.push_str(&format!(".{frac:03}"));
    }
}

fn year_ok(y: c_int) -> bool {
    (1..=MAX_YEAR).contains(&y)
}

fn number_to_value(kind: InputKind, v: f64) -> Option<Vec<u8>> {
    if !v.is_finite() {
        return None;
    }
    let mut out = String::new();
    match kind {
        InputKind::Number | InputKind::Range => {
            return Some(southstar_glib::ascii_dtostr(if v == 0.0 { 0.0 } else { v }));
        }
        InputKind::Date => {
            let days = (v / DAY_MS + 0.5).floor();
            let (y, m, d) = civil_from_days(c_trunc_long(days));
            if !year_ok(y) {
                return None;
            }
            out.push_str(&format!("{y:04}-{m:02}-{d:02}"));
        }
        InputKind::Month => {
            let months = c_trunc_long((v + 0.5).floor());
            let years = if months >= 0 {
                months / 12
            } else {
                -(months.wrapping_neg().wrapping_add(11) / 12)
            };
            let y = 1970i32.wrapping_add(years as c_int);
            let m = floormod(months, 12) as c_int + 1;
            if !year_ok(y) {
                return None;
            }
            out.push_str(&format!("{y:04}-{m:02}"));
        }
        InputKind::Week => {
            let monday = c_trunc_long((v / WEEK_MS + 0.5).floor())
                .wrapping_mul(7)
                .wrapping_sub(3);
            let (y, _, _) = civil_from_days(monday.wrapping_add(3));
            if !year_ok(y) {
                return None;
            }
            let week1 = southstar_datetime::iso_week1_monday(y);
            let w = (monday.wrapping_sub(week1) / 7) as c_int + 1;
            if w < 1 || w > southstar_datetime::iso_weeks_in_year(y) {
                return None;
            }
            out.push_str(&format!("{y:04}-W{w:02}"));
        }
        InputKind::Time => {
            let ms = floormod(c_trunc_long((v + 0.5).floor()), 86_400_000);
            serialize_time(&mut out, ms as c_int);
        }
        InputKind::DateTime => {
            let total = (v + 0.5).floor();
            let days = c_trunc_long((total / DAY_MS).floor());
            let ms = c_trunc_long(total - days as f64 * DAY_MS);
            let (y, m, d) = civil_from_days(days);
            if !year_ok(y) {
                return None;
            }
            out.push_str(&format!("{y:04}-{m:02}-{d:02}T"));
            serialize_time(&mut out, ms as c_int);
        }
        InputKind::Other => return None,
    }
    Some(out.into_bytes())
}

fn step_base(kind: InputKind, ty: Option<&CStr>, input: Node) -> f64 {
    let fallback = if kind == InputKind::Week {
        WEEK_BASE_MS
    } else {
        0.0
    };
    value_to_number(ty, input.attr(c"min")).unwrap_or(fallback)
}

fn step_size(kind: InputKind, step_attr: Option<&CStr>) -> f64 {
    let value = parse_finite_double(step_attr)
        .filter(|&p| p > 0.0)
        .unwrap_or(default_step(kind));
    value * step_scale(kind)
}

pub fn step_apply(input: Node, sign: c_int, n: f64, buffer_len: usize) -> Step {
    if !named(input, b"input") {
        return Step::NotApplicable;
    }
    let ty = input.attr(c"type");
    let kind = input_kind(ty);
    if kind == InputKind::Other {
        return Step::NotApplicable;
    }
    let step_attr = input.attr(c"step");
    if step_attr.is_some_and(|s| ieq(s, "any")) {
        return Step::NoStep;
    }
    let step = step_size(kind, step_attr);
    if step <= 0.0 || !step.is_finite() {
        return Step::NoStep;
    }
    let min = value_to_number(ty, input.attr(c"min"));
    let max = value_to_number(ty, input.attr(c"max"));
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Step::Unchanged;
        }
    }
    let mut value = value_to_number(ty, used_value(input)).unwrap_or(0.0);
    let before = value;
    let base = step_base(kind, ty, input);
    let q = (value - base) / step;
    let nearest = q.round();
    let tolerance = 1e-7 * if q.abs() > 1.0 { q.abs() } else { 1.0 };
    if (q - nearest).abs() > tolerance {
        value = base + if sign > 0 { q.ceil() } else { q.floor() } * step;
    } else {
        value += f64::from(sign) * n * step;
    }
    if let Some(bound) = value_to_number(ty, input.attr(c"min")) {
        if value < bound {
            value = base + ((bound - base) / step).ceil() * step;
        }
    }
    if let Some(bound) = value_to_number(ty, input.attr(c"max")) {
        if value > bound {
            value = base + ((bound - base) / step).floor() * step;
        }
    }
    if (sign > 0 && value < before) || (sign < 0 && value > before) {
        return Step::Unchanged;
    }
    match number_to_value(kind, value).filter(|text| text.len() < buffer_len) {
        Some(text) => Step::Applied(text),
        None => Step::Unchanged,
    }
}

pub fn value_step_mismatch(input: Node, value: Option<&CStr>) -> bool {
    if !named(input, b"input") {
        return false;
    }
    let ty = input.attr(c"type");
    let kind = input_kind(ty);
    if kind == InputKind::Other {
        return false;
    }
    let step_attr = input.attr(c"step");
    if step_attr.is_some_and(|s| ieq(s, "any")) {
        return false;
    }
    let Some(v) = value_to_number(ty, value) else {
        return false;
    };
    let step = step_size(kind, step_attr);
    if step <= 0.0 || !step.is_finite() {
        return false;
    }
    let base = step_base(kind, ty, input);
    let q = (v - base) / step;
    let nearest = q.round();
    let scale = if q.abs() > 1.0 { q.abs() } else { 1.0 };
    (q - nearest).abs() > 1e-7 * scale
}

fn input_type_is(node: Node, want: &str) -> bool {
    named(node, b"input") && node.attr(c"type").is_some_and(|t| ieq(t, want))
}

fn radio_group_has_checked(scan: Node, owner: Option<Node>, name: &CStr, depth: i32) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if input_type_is(scan, "radio") {
        let scan_name = scan.attr(c"name").unwrap_or(c"");
        if scan_name == name && form_owner(scan) == owner && is_checked(scan) {
            return true;
        }
    }
    children(scan).any(|child| radio_group_has_checked(child, owner, name, depth + 1))
}

pub fn value_missing(control: Node, value: Option<&CStr>, doc: Option<Node>) -> bool {
    if control.element_name().is_none() {
        return false;
    }
    if input_type_is(control, "checkbox") {
        return !is_checked(control);
    }
    if input_type_is(control, "radio") {
        let name = control.attr(c"name").unwrap_or(c"");
        let root = doc.unwrap_or_else(|| tree::root(control));
        return !radio_group_has_checked(root, form_owner(control), name, 0);
    }
    value.is_none_or(CStr::is_empty)
}

fn email_local_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&c)
}

fn email_domain_valid(domain: &[u8]) -> bool {
    let mut label = domain;
    while !label.is_empty() {
        let dot = label.iter().position(|&b| b == b'.');
        let part = &label[..dot.unwrap_or(label.len())];
        let len = part.len();
        if len == 0
            || len > 63
            || !part[0].is_ascii_alphanumeric()
            || !part[len - 1].is_ascii_alphanumeric()
        {
            return false;
        }
        if part[1..len.max(1)]
            .iter()
            .take(len.saturating_sub(2))
            .any(|&c| !c.is_ascii_alphanumeric() && c != b'-')
        {
            return false;
        }
        match dot {
            None => return true,
            Some(dot) => label = &label[dot + 1..],
        }
    }
    false
}

fn email_token_valid(token: &[u8]) -> bool {
    let trimmed = token.trim_ascii();
    let Some(at) = trimmed.iter().position(|&b| b == b'@') else {
        return false;
    };
    let domain = &trimmed[at + 1..];
    at != 0
        && !domain.is_empty()
        && !domain.contains(&b'@')
        && trimmed[..at].iter().all(|&c| email_local_char(c))
        && email_domain_valid(domain)
}

pub fn email_value_valid(input: Option<Node>, value: Option<&CStr>) -> bool {
    let Some(value) = value.map(CStr::to_bytes).filter(|v| !v.is_empty()) else {
        return true;
    };
    if input.is_none_or(|i| i.attr(c"multiple").is_none()) {
        return email_token_valid(value);
    }
    value.split(|&b| b == b',').all(email_token_valid)
}

pub fn ce_attr_enables(ce: Option<&CStr>) -> bool {
    ce.is_some_and(|ce| ce.is_empty() || ieq(ce, "true") || ieq(ce, "plaintext-only"))
}

pub fn is_text_input(node: Node) -> bool {
    match node.element_name() {
        Some(b"textarea") => true,
        Some(b"input") => {
            let ty = node.attr(c"type");
            empty_or_missing(ty)
                || type_in(
                    ty,
                    &[
                        "text", "search", "email", "url", "tel", "number", "password",
                    ],
                )
        }
        _ => false,
    }
}

pub fn is_contenteditable_host(node: Node) -> bool {
    match node.element_name() {
        None | Some(b"input" | b"textarea") => false,
        Some(_) => ce_attr_enables(node.attr(c"contenteditable")),
    }
}

pub fn is_editable(node: Node) -> bool {
    is_text_input(node) || is_contenteditable_host(node)
}

pub fn spellcheck_used(node: Node) -> bool {
    for cur in ancestors_and_self(node) {
        if !cur.is_element() {
            continue;
        }
        let Some(v) = cur.attr(c"spellcheck") else {
            continue;
        };
        if ieq(v, "false") {
            return false;
        }
        if v.is_empty() || ieq(v, "true") {
            return true;
        }
    }
    true
}

pub fn spellcheck_host(node: Node) -> Option<Node> {
    let host = ancestors_and_self(node).find(|cur| is_editable(*cur))?;
    spellcheck_used(host).then_some(host)
}

pub fn value_is_dirty_mode(node: Node) -> bool {
    if !raw_name_is(node, b"input") {
        return false;
    }
    let ty = node.attr(c"type");
    ty.is_none()
        || !type_in(
            ty,
            &[
                "checkbox", "radio", "submit", "reset", "button", "image", "file", "hidden",
            ],
        )
}

fn first_text_child(node: Node<'_>) -> Option<&CStr> {
    children(node)
        .filter(|c| c.is_text())
        .find_map(|c| c.text())
}

pub fn used_value(node: Node<'_>) -> Option<&CStr> {
    if raw_name_is(node, b"textarea") {
        if node.attr(c"data-nd-vdirty").is_some() {
            return Some(node.attr(c"data-nd-value").unwrap_or(c""));
        }
        return Some(first_text_child(node).unwrap_or(c""));
    }
    if value_is_dirty_mode(node) {
        if let Some(dirty) = node.attr(c"data-nd-value") {
            return Some(dirty);
        }
    }
    node.attr(c"value")
}

pub fn textarea_default_value(node: Option<Node>) -> Vec<u8> {
    let mut value = Vec::new();
    if let Some(node) = node.filter(|n| raw_name_is(*n, b"textarea")) {
        for child in children(node).filter(|c| c.is_text()) {
            if let Some(text) = child.text() {
                value.extend_from_slice(text.to_bytes());
            }
        }
    }
    value
}

pub fn textarea_normalized(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut i = 0;
    while i < value.len() {
        if value[i] == b'\r' {
            if value.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            out.push(b'\n');
        } else {
            out.push(value[i]);
        }
        i += 1;
    }
    out
}

pub fn textarea_value(node: Option<Node>) -> Vec<u8> {
    if let Some(n) = node {
        if n.attr(c"data-nd-vdirty").is_some() {
            return textarea_normalized(n.attr(c"data-nd-value").map_or(&[][..], CStr::to_bytes));
        }
    }
    textarea_normalized(&textarea_default_value(node))
}

pub fn is_checked(node: Node) -> bool {
    match node.attr(c"data-nd-checked") {
        Some(dirty) => dirty.to_bytes() == b"1",
        None => node.attr(c"checked").is_some(),
    }
}

pub fn editable_value(node: Node<'_>) -> &CStr {
    if raw_name_is(node, b"textarea") {
        return used_value(node).unwrap_or(c"");
    }
    if is_contenteditable_host(node) {
        return first_text_child(node).unwrap_or(c"");
    }
    used_value(node).unwrap_or(c"")
}

pub fn set_editable_value(node: Node, value: &CStr) {
    if raw_name_is(node, b"textarea") {
        let normalized =
            std::ffi::CString::new(textarea_normalized(value.to_bytes())).unwrap_or_default();
        ffi::set_attr(node, c"data-nd-value", &normalized);
        ffi::set_attr(node, c"data-nd-vdirty", c"1");
        ffi::set_attr(node, c"data-nd-user-edited", c"1");
    } else if is_contenteditable_host(node) {
        let doc = ancestors_and_self(node).last().unwrap_or(node);
        let mut child = node.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            ffi::detach(c);
            if doc != c {
                index::id_subtree_removed(doc, c);
                index::class_subtree_removed(doc, c);
                index::tag_subtree_removed(doc, c);
            }
            ffi::free(c);
        }
        ffi::append_text(node, value);
    } else if value_is_dirty_mode(node) {
        ffi::set_attr(node, c"data-nd-value", value);
        ffi::set_attr(node, c"data-nd-user-edited", c"1");
    } else {
        ffi::set_attr(node, c"value", value);
    }
}

pub fn flatten_editable(node: Node) {
    if !is_contenteditable_host(node) {
        return;
    }
    let text = std::ffi::CString::new(node.collect_text()).unwrap_or_default();
    set_editable_value(node, &text);
}

pub fn is_numeric_input(node: Node) -> bool {
    named(node, b"input") && node.attr(c"type").is_some_and(|t| ieq(t, "number"))
}

pub fn numeric_filter(insert: &[u8]) -> Vec<u8> {
    insert
        .iter()
        .copied()
        .filter(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+' | b'e' | b'E'))
        .collect()
}

pub fn is_one_line_text(node: Node) -> bool {
    if !named(node, b"input") {
        return false;
    }
    let ty = node.attr(c"type");
    ty.is_none()
        || !type_in(
            ty,
            &[
                "button",
                "checkbox",
                "color",
                "date",
                "datetime-local",
                "file",
                "hidden",
                "image",
                "month",
                "number",
                "radio",
                "range",
                "reset",
                "submit",
                "time",
                "week",
            ],
        )
}

fn is_shadow_root(node: Node) -> bool {
    node.is_element() && node.attr(c"data-nd-shadow-root").is_some()
}

pub fn form_owner(control: Node<'_>) -> Option<Node<'_>> {
    if !control.is_element() {
        return None;
    }
    if let Some(form_id) = control.attr(c"form") {
        if form_id.is_empty() {
            return None;
        }
        let tree_root = ancestors_and_self(control)
            .find(|n| n.parent().is_none() || is_shadow_root(*n))
            .unwrap_or(control);
        return index::find_by_id(tree_root, form_id).filter(|owner| named(*owner, b"form"));
    }
    ancestors(control)
        .take(MAX_DEPTH as usize)
        .take_while(|p| !is_shadow_root(*p))
        .find(|p| named(*p, b"form"))
}

fn reset_control(node: Node) {
    match node.element_name() {
        Some(b"input" | b"textarea") => {
            if type_in(node.attr(c"type"), &["checkbox", "radio"]) {
                ffi::remove_attr(node, c"data-nd-checked");
            }
            ffi::remove_attr(node, c"data-nd-value");
            ffi::remove_attr(node, c"data-nd-vdirty");
            ffi::remove_attr(node, c"data-nd-user-edited");
        }
        Some(b"select") => {
            ffi::remove_attr(node, c"data-nd-noselect");
            for option in children(node).filter(|o| named(*o, b"option")) {
                ffi::remove_attr(option, c"selected");
            }
        }
        _ => {}
    }
}

fn reset_walk(form: Node, scan: Node, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if scan.element_name().is_some() && form_owner(scan) == Some(form) {
        reset_control(scan);
    }
    for child in children(scan) {
        reset_walk(form, child, depth + 1);
    }
}

pub fn reset_owned_controls(form: Node, root: Option<Node>) {
    reset_walk(form, root.unwrap_or(form), 0);
}

pub fn supports_disabled(el: Node) -> bool {
    matches!(
        el.element_name(),
        Some(
            b"button" | b"fieldset" | b"input" | b"optgroup" | b"option" | b"select" | b"textarea"
        )
    )
}

pub fn effectively_disabled(el: Node) -> bool {
    if !supports_disabled(el) {
        return false;
    }
    if el.attr(c"disabled").is_some() {
        return true;
    }
    let is_option = named(el, b"option");
    for p in ancestors(el) {
        if is_option && named(p, b"optgroup") && p.attr(c"disabled").is_some() {
            return true;
        }
        if !named(p, b"fieldset") || p.attr(c"disabled").is_none() {
            continue;
        }
        let legend = children(p).find(|c| named(*c, b"legend"));
        if legend.is_some_and(|legend| tree::contains(legend.as_ptr(), el)) {
            continue;
        }
        return true;
    }
    false
}
