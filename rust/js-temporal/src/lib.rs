//! Southstar — the native, ICU-free Temporal date/time API, installed through the engine-neutral JavaScript layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "quickjs")]
mod ffi;

use core::ffi::{c_int, c_long};
use std::time::{SystemTime, UNIX_EPOCH};

use southstar_datetime as dt;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

const NS_PER_DAY: i64 = 86_400_000_000_000;
const NS_PER_HOUR: i64 = 3_600_000_000_000;
const NS_PER_MINUTE: i64 = 60_000_000_000;
const NS_PER_SECOND: i64 = 1_000_000_000;
const DURATION_FIELDS: [&str; 10] = [
    "years",
    "months",
    "weeks",
    "days",
    "hours",
    "minutes",
    "seconds",
    "milliseconds",
    "microseconds",
    "nanoseconds",
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Instant,
    PlainDate,
    PlainTime,
    PlainDateTime,
    PlainYearMonth,
    PlainMonthDay,
    ZonedDateTime,
    Duration,
}

#[derive(Clone)]
struct Temporal {
    kind: Kind,
    epoch_sec: i64,
    nanos: i32,
    year: c_int,
    month: c_int,
    day: c_int,
    hour: c_int,
    minute: c_int,
    second: c_int,
    ms: c_int,
    us: c_int,
    ns: c_int,
    dur: [i64; 10],
    tz: Option<String>,
}

impl Temporal {
    fn new(kind: Kind) -> Temporal {
        Temporal {
            kind,
            epoch_sec: 0,
            nanos: 0,
            year: 0,
            month: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            ms: 0,
            us: 0,
            ns: 0,
            dur: [0; 10],
            tz: None,
        }
    }

    fn set_epoch_ns(&mut self, ns: i64) {
        self.epoch_sec = if ns >= 0 { ns } else { ns - 999_999_999 } / NS_PER_SECOND;
        self.nanos = (ns - self.epoch_sec * NS_PER_SECOND) as i32;
    }

    fn set_date_fields(&mut self, y: c_int, mo: c_int, d: c_int) {
        let mo = mo.clamp(1, 12);
        let dim = dt::days_in_month(y, mo);
        let d = if d < 1 { 1 } else { d.min(dim) };
        self.year = y;
        self.month = mo;
        self.day = d;
    }

    fn set_time_from(&mut self, src: &Temporal) {
        self.hour = src.hour;
        self.minute = src.minute;
        self.second = src.second;
        self.ms = src.ms;
        self.us = src.us;
        self.ns = src.ns;
    }

    fn time_ns(&self) -> i64 {
        i64::from(self.hour) * NS_PER_HOUR
            + i64::from(self.minute) * NS_PER_MINUTE
            + i64::from(self.second) * NS_PER_SECOND
            + i64::from(self.ms) * 1_000_000
            + i64::from(self.us) * 1000
            + i64::from(self.ns)
    }

    fn split_time_ns(&mut self, mut ns: i64) {
        self.hour = (ns / NS_PER_HOUR) as c_int;
        ns %= NS_PER_HOUR;
        self.minute = (ns / NS_PER_MINUTE) as c_int;
        ns %= NS_PER_MINUTE;
        self.second = (ns / NS_PER_SECOND) as c_int;
        ns %= NS_PER_SECOND;
        self.ms = (ns / 1_000_000) as c_int;
        ns %= 1_000_000;
        self.us = (ns / 1000) as c_int;
        self.ns = (ns % 1000) as c_int;
    }

    fn normalize_time(&mut self) {
        let ns = ((self.time_ns() % NS_PER_DAY) + NS_PER_DAY) % NS_PER_DAY;
        self.split_time_ns(ns);
    }

    fn add_date(&mut self, y: i64, mo: i64, w: i64, d: i64, sign: i64) {
        let mut ny = self.year.wrapping_add((sign * y) as c_int);
        let total_mo = long(i64::from(self.month) - 1 + sign * mo);
        ny = ny.wrapping_add(
            (if total_mo >= 0 {
                total_mo / 12
            } else {
                (total_mo - 11) / 12
            }) as c_int,
        );
        let nmo = dt::floormod(total_mo, 12) as c_int + 1;
        let dim = dt::days_in_month(ny, nmo);
        let nd = self.day.min(dim);
        let days = long(wide(dt::days_from_civil(ny, nmo, nd)) + sign * (w * 7 + d));
        (self.year, self.month, self.day) = dt::civil_from_days(days);
    }

    fn duration_time_ns(&self) -> i64 {
        self.dur[3] * NS_PER_DAY
            + self.dur[4] * NS_PER_HOUR
            + self.dur[5] * NS_PER_MINUTE
            + self.dur[6] * NS_PER_SECOND
            + self.dur[7] * 1_000_000
            + self.dur[8] * 1000
            + self.dur[9]
    }

    fn duration_sign(&self) -> i32 {
        self.dur
            .iter()
            .find(|&&field| field != 0)
            .map_or(0, |&field| if field > 0 { 1 } else { -1 })
    }
}

#[allow(clippy::unnecessary_cast)]
fn long(value: i64) -> c_long {
    value as c_long
}

#[allow(clippy::useless_conversion)]
fn wide(value: c_long) -> i64 {
    i64::from(value)
}

fn iso_day_of_week(y: c_int, m: c_int, d: c_int) -> c_int {
    let days = dt::days_from_civil(y, m, d);
    match dt::floormod(days + 4, 7) as c_int {
        0 => 7,
        dow => dow,
    }
}

fn day_of_year(y: c_int, m: c_int, d: c_int) -> c_int {
    (dt::days_from_civil(y, m, d) - dt::days_from_civil(y, 1, 1)) as c_int + 1
}

fn days_in_year(y: c_int) -> c_int {
    if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
        366
    } else {
        365
    }
}

fn week_of_year(mut y: c_int, m: c_int, d: c_int) -> c_int {
    let ord = dt::days_from_civil(y, m, d);
    let mut week1 = dt::iso_week1_monday(y);
    if ord < week1 {
        y -= 1;
        week1 = dt::iso_week1_monday(y);
    }
    if ord >= dt::iso_week1_monday(y + 1) {
        return 1;
    }
    ((ord - week1) / 7) as c_int + 1
}

fn breakdown(epoch_sec: i64) -> (c_int, c_int, c_int, c_int, c_int, c_int) {
    let days = long(
        if epoch_sec >= 0 {
            epoch_sec
        } else {
            epoch_sec - 86_399
        } / 86_400,
    );
    let rem = long(epoch_sec - wide(days) * 86_400);
    let (y, mo, d) = dt::civil_from_days(days);
    (
        y,
        mo,
        d,
        (rem / 3600) as c_int,
        ((rem % 3600) / 60) as c_int,
        (rem % 60) as c_int,
    )
}

fn epoch_of(y: c_int, mo: c_int, d: c_int, h: c_int, mi: c_int, s: c_int) -> i64 {
    wide(dt::days_from_civil(y, mo, d)) * 86_400
        + i64::from(h.wrapping_mul(3600))
        + i64::from(mi.wrapping_mul(60))
        + i64::from(s)
}

fn fraction(out: &mut String, frac: i64) {
    if frac == 0 {
        return;
    }
    let digits = format!("{frac:09}");
    let digits = digits.get(..9).unwrap_or(&digits);
    out.push('.');
    out.push_str(digits.trim_end_matches('0'));
}

fn fraction_of(out: &mut String, ms: c_int, us: c_int, ns: c_int) {
    fraction(
        out,
        i64::from(ms) * 1_000_000 + i64::from(us) * 1000 + i64::from(ns),
    );
}

fn byte(s: &[u8], at: usize) -> u8 {
    s.get(at).copied().unwrap_or(0)
}

#[derive(Default)]
struct Parsed {
    y: c_int,
    mo: c_int,
    d: c_int,
    h: c_int,
    mi: c_int,
    sec: c_int,
    ms: c_int,
    us: c_int,
    ns: c_int,
    off: c_int,
}

fn parse_datetime(s: &[u8]) -> Option<Parsed> {
    let (mut p, y, mo, d) = dt::read_date(s)?;
    let mut out = Parsed {
        y,
        mo,
        d,
        ..Parsed::default()
    };
    if matches!(byte(s, p), b'T' | b't' | b' ') {
        p += 1;
        let Some((n, hh)) = dt::read_digits(&s[p.min(s.len())..], 2, 2) else {
            return Some(out);
        };
        let q = p + n;
        if byte(s, q) != b':' {
            return Some(out);
        }
        let (n, mm) = dt::read_digits(&s[q + 1..], 2, 2)?;
        out.h = hh;
        out.mi = mm;
        p = q + 1 + n;
        if byte(s, p) == b':' {
            p += 1;
            let (n, ss) = dt::read_digits(&s[p..], 2, 2)?;
            p += n;
            out.sec = ss;
            if matches!(byte(s, p), b'.' | b',') {
                p += 1;
                let mut value: c_int = 0;
                let mut count = 0;
                while byte(s, p).is_ascii_digit() && count < 9 {
                    value = value * 10 + c_int::from(byte(s, p) - b'0');
                    p += 1;
                    count += 1;
                }
                while count < 9 {
                    value *= 10;
                    count += 1;
                }
                while byte(s, p).is_ascii_digit() {
                    p += 1;
                }
                out.ms = value / 1_000_000;
                out.us = (value / 1000) % 1000;
                out.ns = value % 1000;
            }
        }
    }
    match byte(s, p) {
        b'Z' | b'z' => out.off = 0,
        sign @ (b'+' | b'-') => {
            let sign = if sign == b'-' { -1 } else { 1 };
            p += 1;
            if let Some((n, oh)) = dt::read_digits(&s[p.min(s.len())..], 2, 2) {
                let mut q = p + n;
                if byte(s, q) == b':' {
                    q += 1;
                }
                let om = dt::read_digits(&s[q.min(s.len())..], 2, 2).map_or(0, |(_, om)| om);
                out.off = sign * (oh * 60 + om);
            }
        }
        _ => {}
    }
    Some(out)
}

fn scan_int(s: &[u8], mut at: usize) -> Option<(c_int, usize)> {
    while matches!(byte(s, at), b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        at += 1;
    }
    let negative = byte(s, at) == b'-';
    if matches!(byte(s, at), b'+' | b'-') {
        at += 1;
    }
    let start = at;
    let mut value: i64 = 0;
    while byte(s, at).is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte(s, at) - b'0'));
        at += 1;
    }
    if at == start {
        return None;
    }
    let value = if negative { -value } else { value };
    Some((value as c_int, at))
}

fn scan_pair(s: &[u8]) -> Option<(c_int, c_int)> {
    let (first, at) = scan_int(s, 0)?;
    if byte(s, at) != b'-' {
        return None;
    }
    let (second, _) = scan_int(s, at + 1)?;
    Some((first, second))
}

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn text_of(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    let text = scope.to_string(value).ok()?;
    let bytes = text.into_bytes();
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    Some(bytes[..end].to_vec())
}

fn temporal_of(scope: &mut Scope<'_>, value: &Value) -> Option<Temporal> {
    scope.host_data::<Temporal>(value)
}

fn this_temporal(scope: &mut Scope<'_>, this: &Value, kind: Kind) -> Result<Temporal, Value> {
    match temporal_of(scope, this) {
        Some(t) if t.kind == kind => Ok(t),
        _ => Err(scope.type_error("invalid Temporal receiver")),
    }
}

fn object_or_none(value: Result<Value, Value>) -> Option<Value> {
    value.ok().filter(Value::is_object)
}

fn own_prototype(scope: &mut Scope<'_>, constructor: &Value) -> Option<Value> {
    object_or_none(scope.get(constructor, "prototype"))
}

fn constructor_prototype(scope: &mut Scope<'_>, this: &Value) -> Option<Value> {
    let constructor = scope.get(this, "constructor").ok()?;
    object_or_none(scope.get(&constructor, "prototype"))
}

fn global_prototype(scope: &mut Scope<'_>, name: &str) -> Option<Value> {
    let global = scope.global();
    let temporal = scope.get(&global, "Temporal").ok()?;
    let constructor = scope.get(&temporal, name).ok()?;
    object_or_none(scope.get(&constructor, "prototype"))
}

fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str, default: c_int) -> c_int {
    match scope.get(object, key) {
        Ok(value) if !value.is_undefined() && !value.is_null() => {
            scope.to_int32(&value).unwrap_or(default)
        }
        _ => default,
    }
}

fn has_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    if !object.is_object() {
        return false;
    }
    scope
        .get(object, key)
        .map_or(true, |value| !value.is_undefined())
}

fn read_duration(scope: &mut Scope<'_>, value: &Value) -> [i64; 10] {
    if let Some(dur) = temporal_of(scope, value).filter(|t| t.kind == Kind::Duration) {
        return dur.dur;
    }
    let mut out = [0i64; 10];
    if value.is_object() {
        for (slot, name) in out.iter_mut().zip(DURATION_FIELDS) {
            *slot = i64::from(int_prop(scope, value, name, 0));
        }
    }
    out
}

fn int_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> c_int {
    scope.to_int32(&arg(args, index)).unwrap_or(0)
}

fn define(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.define(object, key, value, Attributes::ENUMERABLE);
}

fn month_code(month: c_int) -> String {
    format!("M{month:02}")
}

fn fill_date(scope: &mut Scope<'_>, object: &Value, t: &Temporal) {
    let (y, m, d) = (t.year, t.month, t.day);
    define(scope, object, "year", Value::int(y));
    define(scope, object, "month", Value::int(m));
    let code = scope.string(&month_code(m));
    define(scope, object, "monthCode", code);
    define(scope, object, "day", Value::int(d));
    let calendar = scope.string("iso8601");
    define(scope, object, "calendarId", calendar);
    define(
        scope,
        object,
        "dayOfWeek",
        Value::int(iso_day_of_week(y, m, d)),
    );
    define(scope, object, "dayOfYear", Value::int(day_of_year(y, m, d)));
    define(
        scope,
        object,
        "weekOfYear",
        Value::int(week_of_year(y, m, d)),
    );
    define(scope, object, "daysInWeek", Value::int(7));
    define(
        scope,
        object,
        "daysInMonth",
        Value::int(dt::days_in_month(y, m)),
    );
    define(scope, object, "daysInYear", Value::int(days_in_year(y)));
    define(scope, object, "monthsInYear", Value::int(12));
    define(
        scope,
        object,
        "inLeapYear",
        Value::boolean(days_in_year(y) == 366),
    );
}

fn fill_time(scope: &mut Scope<'_>, object: &Value, t: &Temporal) {
    define(scope, object, "hour", Value::int(t.hour));
    define(scope, object, "minute", Value::int(t.minute));
    define(scope, object, "second", Value::int(t.second));
    define(scope, object, "millisecond", Value::int(t.ms));
    define(scope, object, "microsecond", Value::int(t.us));
    define(scope, object, "nanosecond", Value::int(t.ns));
}

fn fill_epoch(scope: &mut Scope<'_>, object: &Value, t: &Temporal) {
    let millis = t.epoch_sec * 1000 + i64::from(t.nanos / 1_000_000);
    define(scope, object, "epochMilliseconds", Value::int64(millis));
    let nanos = scope.bigint64(t.epoch_sec.wrapping_mul(NS_PER_SECOND) + i64::from(t.nanos));
    define(scope, object, "epochNanoseconds", nanos);
}

fn fill(scope: &mut Scope<'_>, object: &Value, t: &Temporal) {
    match t.kind {
        Kind::PlainDate => fill_date(scope, object, t),
        Kind::PlainTime => fill_time(scope, object, t),
        Kind::PlainDateTime => {
            fill_date(scope, object, t);
            fill_time(scope, object, t);
        }
        Kind::PlainYearMonth => {
            define(scope, object, "year", Value::int(t.year));
            define(scope, object, "month", Value::int(t.month));
            let code = scope.string(&month_code(t.month));
            define(scope, object, "monthCode", code);
            let calendar = scope.string("iso8601");
            define(scope, object, "calendarId", calendar);
            let dim = dt::days_in_month(t.year, t.month);
            define(scope, object, "daysInMonth", Value::int(dim));
            define(
                scope,
                object,
                "daysInYear",
                Value::int(days_in_year(t.year)),
            );
            define(scope, object, "monthsInYear", Value::int(12));
            let leap = Value::boolean(days_in_year(t.year) == 366);
            define(scope, object, "inLeapYear", leap);
        }
        Kind::PlainMonthDay => {
            let code = scope.string(&month_code(t.month));
            define(scope, object, "monthCode", code);
            define(scope, object, "day", Value::int(t.day));
            let calendar = scope.string("iso8601");
            define(scope, object, "calendarId", calendar);
        }
        Kind::Instant => fill_epoch(scope, object, t),
        Kind::ZonedDateTime => {
            let tz = scope.string(t.tz.as_deref().unwrap_or("UTC"));
            define(scope, object, "timeZoneId", tz);
            let calendar = scope.string("iso8601");
            define(scope, object, "calendarId", calendar);
            fill_epoch(scope, object, t);
        }
        Kind::Duration => {
            for (name, &field) in DURATION_FIELDS.iter().zip(&t.dur) {
                define(scope, object, name, Value::int64(field));
            }
            let sign = t.duration_sign();
            define(scope, object, "sign", Value::int(sign));
            define(scope, object, "blank", Value::boolean(sign == 0));
        }
    }
}

fn make(scope: &mut Scope<'_>, prototype: Option<Value>, mut t: Temporal) -> Value {
    t.year = t.year.clamp(-dt::MAX_YEAR, dt::MAX_YEAR);
    let object = scope.new_host_object(prototype.as_ref(), t.clone());
    fill(scope, &object, &t);
    object
}

fn instant_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.is_empty() {
        return Err(scope.type_error("Instant.from requires an argument"));
    }
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::Instant);
    let source = arg(args, 0);
    if source.is_string() {
        let Some(p) = text_of(scope, &source).and_then(|s| parse_datetime(&s)) else {
            return Err(scope.range_error("invalid Instant string"));
        };
        t.epoch_sec =
            epoch_of(p.y, p.mo, p.d, p.h, p.mi, p.sec) - i64::from(p.off.wrapping_mul(60));
        t.nanos = p.ms * 1_000_000 + p.us * 1000 + p.ns;
    } else if let Some(src) = temporal_of(scope, &source)
        .filter(|s| matches!(s.kind, Kind::Instant | Kind::ZonedDateTime))
    {
        t.epoch_sec = src.epoch_sec;
        t.nanos = src.nanos;
    }
    Ok(make(scope, prototype, t))
}

fn instant_from_epoch_milliseconds(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let ms = if args.is_empty() {
        0
    } else {
        scope.to_int64(&args[0]).unwrap_or(0)
    };
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::Instant);
    t.epoch_sec = if ms >= 0 { ms } else { ms - 999 } / 1000;
    t.nanos = ((ms - t.epoch_sec * 1000) * 1_000_000) as i32;
    Ok(make(scope, prototype, t))
}

fn epoch_nanoseconds_arg(scope: &mut Scope<'_>, args: &[Value]) -> i64 {
    if args.is_empty() {
        0
    } else {
        scope.to_bigint64(&args[0]).unwrap_or(0)
    }
}

fn instant_from_epoch_nanoseconds(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let ns = epoch_nanoseconds_arg(scope, args);
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::Instant);
    t.set_epoch_ns(ns);
    Ok(make(scope, prototype, t))
}

fn instant_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    instant_from_epoch_nanoseconds(scope, this, args)
}

fn instant_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::Instant)?;
    let (y, mo, d, h, mi, s) = breakdown(t.epoch_sec);
    let mut out = format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}");
    fraction_of(
        &mut out,
        t.nanos / 1_000_000,
        (t.nanos / 1000) % 1000,
        t.nanos % 1000,
    );
    out.push('Z');
    Ok(scope.string(&out))
}

fn instant_add_impl(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::Instant)?;
    let duration = arg(args, 0);
    let add_ns = match temporal_of(scope, &duration).filter(|d| d.kind == Kind::Duration) {
        Some(dur) => dur.duration_time_ns(),
        None if duration.is_object() => {
            i64::from(int_prop(scope, &duration, "hours", 0)) * NS_PER_HOUR
                + i64::from(int_prop(scope, &duration, "minutes", 0)) * NS_PER_MINUTE
                + i64::from(int_prop(scope, &duration, "seconds", 0)) * NS_PER_SECOND
                + i64::from(int_prop(scope, &duration, "milliseconds", 0)) * 1_000_000
                + i64::from(int_prop(scope, &duration, "microseconds", 0)) * 1000
                + i64::from(int_prop(scope, &duration, "nanoseconds", 0))
        }
        None => 0,
    };
    let total = t.epoch_sec * NS_PER_SECOND + i64::from(t.nanos) + sign * add_ns;
    let prototype = constructor_prototype(scope, this);
    let mut nt = Temporal::new(Kind::Instant);
    nt.set_epoch_ns(total);
    Ok(make(scope, prototype, nt))
}

fn instant_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    instant_add_impl(scope, this, args, 1)
}

fn instant_subtract(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    instant_add_impl(scope, this, args, -1)
}

fn date_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainDate);
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        let date = text_of(scope, &source).and_then(|s| dt::read_date(&s));
        let Some((_, y, mo, d)) = date else {
            return Err(scope.range_error("invalid PlainDate string"));
        };
        t.set_date_fields(y, mo, d);
    } else if !args.is_empty() && source.is_object() {
        match temporal_of(scope, &source)
            .filter(|s| matches!(s.kind, Kind::PlainDate | Kind::PlainDateTime))
        {
            Some(src) => t.set_date_fields(src.year, src.month, src.day),
            None => {
                let y = int_prop(scope, &source, "year", 1970);
                let mo = int_prop(scope, &source, "month", 1);
                let d = int_prop(scope, &source, "day", 1);
                t.set_date_fields(y, mo, d);
            }
        }
    }
    Ok(make(scope, prototype, t))
}

fn date_constructor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainDate);
    let y = if args.is_empty() {
        1970
    } else {
        int_arg(scope, args, 0)
    };
    let mo = if args.len() >= 2 {
        int_arg(scope, args, 1)
    } else {
        1
    };
    let d = if args.len() >= 3 {
        int_arg(scope, args, 2)
    } else {
        1
    };
    t.set_date_fields(y, mo, d);
    Ok(make(scope, prototype, t))
}

fn date_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    Ok(scope.string(&format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)))
}

fn date_add_impl(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    let dur = read_duration(scope, &arg(args, 0));
    let prototype = constructor_prototype(scope, this);
    let mut nt = t.clone();
    nt.tz = None;
    nt.add_date(dur[0], dur[1], dur[2], dur[3], sign);
    Ok(make(scope, prototype, nt))
}

fn date_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    date_add_impl(scope, this, args, 1)
}

fn date_subtract(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    date_add_impl(scope, this, args, -1)
}

fn date_with(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    let fields = arg(args, 0);
    let prototype = constructor_prototype(scope, this);
    let mut nt = Temporal::new(Kind::PlainDate);
    let field = |scope: &mut Scope<'_>, key: &str, current: c_int| {
        if has_prop(scope, &fields, key) {
            int_prop(scope, &fields, key, current)
        } else {
            current
        }
    };
    let y = field(scope, "year", t.year);
    let mo = field(scope, "month", t.month);
    let d = field(scope, "day", t.day);
    nt.set_date_fields(y, mo, d);
    Ok(make(scope, prototype, nt))
}

fn compare_dates(a: &Temporal, b: &Temporal) -> c_int {
    let da = dt::days_from_civil(a.year, a.month, a.day);
    let db = dt::days_from_civil(b.year, b.month, b.day);
    (da > db) as c_int - (da < db) as c_int
}

fn date_until_impl(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    let Some(o) = temporal_of(scope, &arg(args, 0)).filter(|o| o.kind == Kind::PlainDate) else {
        return Err(scope.type_error("expected a PlainDate"));
    };
    let days =
        dt::days_from_civil(o.year, o.month, o.day) - dt::days_from_civil(t.year, t.month, t.day);
    let prototype = global_prototype(scope, "Duration");
    let mut nt = Temporal::new(Kind::Duration);
    nt.dur[3] = sign * wide(days);
    Ok(make(scope, prototype, nt))
}

fn date_until(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    date_until_impl(scope, this, args, 1)
}

fn date_since(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    date_until_impl(scope, this, args, -1)
}

fn date_equals(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    let other = if args.is_empty() {
        None
    } else {
        temporal_of(scope, &args[0])
    };
    let equal = other.is_some_and(|o| o.kind == Kind::PlainDate && compare_dates(&t, &o) == 0);
    Ok(Value::boolean(equal))
}

fn date_compare(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let a = args.first().and_then(|a| temporal_of(scope, a));
    let b = args.get(1).and_then(|b| temporal_of(scope, b));
    match (a, b) {
        (Some(a), Some(b)) => Ok(Value::int(compare_dates(&a, &b))),
        _ => Err(scope.type_error("Temporal.PlainDate.compare needs two dates")),
    }
}

fn date_to_plain_date_time(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDate)?;
    let time = if args.is_empty() {
        None
    } else {
        temporal_of(scope, &args[0])
    };
    let prototype = global_prototype(scope, "PlainDateTime");
    let mut nt = Temporal::new(Kind::PlainDateTime);
    nt.year = t.year;
    nt.month = t.month;
    nt.day = t.day;
    if let Some(time) = time.filter(|time| time.kind == Kind::PlainTime) {
        nt.set_time_from(&time);
    }
    Ok(make(scope, prototype, nt))
}

fn time_constructor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut vals = [0 as c_int; 6];
    for (i, slot) in vals.iter_mut().enumerate().take(args.len()) {
        *slot = int_arg(scope, args, i);
    }
    let mut t = Temporal::new(Kind::PlainTime);
    [t.hour, t.minute, t.second, t.ms, t.us, t.ns] = vals;
    t.normalize_time();
    Ok(make(scope, prototype, t))
}

fn time_fields_from(scope: &mut Scope<'_>, t: &mut Temporal, object: &Value) {
    t.hour = int_prop(scope, object, "hour", 0);
    t.minute = int_prop(scope, object, "minute", 0);
    t.second = int_prop(scope, object, "second", 0);
    t.ms = int_prop(scope, object, "millisecond", 0);
    t.us = int_prop(scope, object, "microsecond", 0);
    t.ns = int_prop(scope, object, "nanosecond", 0);
}

fn time_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainTime);
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        let time = text_of(scope, &source).and_then(|s| dt::read_time(&s));
        let Some((_, ms)) = time else {
            return Err(scope.range_error("invalid PlainTime string"));
        };
        t.hour = ms / 3_600_000;
        t.minute = (ms / 60_000) % 60;
        t.second = (ms / 1000) % 60;
        t.ms = ms % 1000;
    } else if !args.is_empty() && source.is_object() {
        match temporal_of(scope, &source)
            .filter(|s| matches!(s.kind, Kind::PlainTime | Kind::PlainDateTime))
        {
            Some(src) => t.set_time_from(&src),
            None => time_fields_from(scope, &mut t, &source),
        }
    }
    t.normalize_time();
    Ok(make(scope, prototype, t))
}

fn time_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainTime)?;
    let mut out = format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
    fraction_of(&mut out, t.ms, t.us, t.ns);
    Ok(scope.string(&out))
}

fn time_add_impl(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainTime)?;
    let dur = read_duration(scope, &arg(args, 0));
    let prototype = constructor_prototype(scope, this);
    let mut nt = t.clone();
    nt.tz = None;
    nt.hour = nt.hour.wrapping_add((sign * dur[4]) as c_int);
    nt.minute = nt.minute.wrapping_add((sign * dur[5]) as c_int);
    nt.second = nt.second.wrapping_add((sign * dur[6]) as c_int);
    nt.ms = nt.ms.wrapping_add((sign * dur[7]) as c_int);
    nt.us = nt.us.wrapping_add((sign * dur[8]) as c_int);
    nt.ns = nt.ns.wrapping_add((sign * dur[9]) as c_int);
    nt.normalize_time();
    Ok(make(scope, prototype, nt))
}

fn time_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    time_add_impl(scope, this, args, 1)
}

fn time_subtract(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    time_add_impl(scope, this, args, -1)
}

fn datetime_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainDateTime);
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        let Some(p) = text_of(scope, &source).and_then(|s| parse_datetime(&s)) else {
            return Err(scope.range_error("invalid PlainDateTime string"));
        };
        t.set_date_fields(p.y, p.mo, p.d);
        t.hour = p.h;
        t.minute = p.mi;
        t.second = p.sec;
        t.ms = p.ms;
        t.us = p.us;
        t.ns = p.ns;
    } else if !args.is_empty() && source.is_object() {
        let y = int_prop(scope, &source, "year", 1970);
        let mo = int_prop(scope, &source, "month", 1);
        let d = int_prop(scope, &source, "day", 1);
        t.set_date_fields(y, mo, d);
        time_fields_from(scope, &mut t, &source);
    }
    Ok(make(scope, prototype, t))
}

fn datetime_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut vals: [c_int; 9] = [1970, 1, 1, 0, 0, 0, 0, 0, 0];
    for (i, slot) in vals.iter_mut().enumerate().take(args.len()) {
        *slot = int_arg(scope, args, i);
    }
    let mut t = Temporal::new(Kind::PlainDateTime);
    t.set_date_fields(vals[0], vals[1], vals[2]);
    [t.hour, t.minute, t.second, t.ms, t.us, t.ns] =
        [vals[3], vals[4], vals[5], vals[6], vals[7], vals[8]];
    t.normalize_time();
    Ok(make(scope, prototype, t))
}

fn datetime_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDateTime)?;
    let mut out = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.minute, t.second
    );
    fraction_of(&mut out, t.ms, t.us, t.ns);
    Ok(scope.string(&out))
}

fn datetime_add_impl(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDateTime)?;
    let dur = read_duration(scope, &arg(args, 0));
    let prototype = constructor_prototype(scope, this);
    let mut nt = t.clone();
    nt.tz = None;
    let mut tns = nt.time_ns();
    tns += sign
        * (dur[4] * NS_PER_HOUR
            + dur[5] * NS_PER_MINUTE
            + dur[6] * NS_PER_SECOND
            + dur[7] * 1_000_000
            + dur[8] * 1000
            + dur[9]);
    let carry = if tns >= 0 {
        tns / NS_PER_DAY
    } else {
        (tns - (NS_PER_DAY - 1)) / NS_PER_DAY
    };
    tns -= carry * NS_PER_DAY;
    nt.split_time_ns(tns);
    let mut ny = nt.year.wrapping_add((sign * dur[0]) as c_int);
    let total_mo = long(i64::from(nt.month) - 1 + sign * dur[1]);
    ny = ny.wrapping_add(
        (if total_mo >= 0 {
            total_mo / 12
        } else {
            (total_mo - 11) / 12
        }) as c_int,
    );
    let nmo = dt::floormod(total_mo, 12) as c_int + 1;
    let dim = dt::days_in_month(ny, nmo);
    let nd = nt.day.min(dim);
    let days = long(wide(dt::days_from_civil(ny, nmo, nd)) + sign * (dur[2] * 7 + dur[3]) + carry);
    (nt.year, nt.month, nt.day) = dt::civil_from_days(days);
    Ok(make(scope, prototype, nt))
}

fn datetime_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    datetime_add_impl(scope, this, args, 1)
}

fn datetime_subtract(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    datetime_add_impl(scope, this, args, -1)
}

fn datetime_to_plain_date(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDateTime)?;
    let prototype = global_prototype(scope, "PlainDate");
    let mut nt = Temporal::new(Kind::PlainDate);
    nt.year = t.year;
    nt.month = t.month;
    nt.day = t.day;
    Ok(make(scope, prototype, nt))
}

fn datetime_to_plain_time(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainDateTime)?;
    let prototype = global_prototype(scope, "PlainTime");
    let mut nt = Temporal::new(Kind::PlainTime);
    nt.set_time_from(&t);
    Ok(make(scope, prototype, nt))
}

fn yearmonth_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainYearMonth);
    t.year = 1970;
    t.month = 1;
    t.day = 1;
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        if let Some((y, m)) = text_of(scope, &source).and_then(|s| scan_pair(&s)) {
            t.year = y;
            t.month = m;
        }
    } else if !args.is_empty() && source.is_object() {
        t.year = int_prop(scope, &source, "year", 1970);
        t.month = int_prop(scope, &source, "month", 1);
    }
    t.month = t.month.clamp(1, 12);
    Ok(make(scope, prototype, t))
}

fn yearmonth_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let y = if args.is_empty() {
        1970
    } else {
        int_arg(scope, args, 0)
    };
    let m = if args.len() >= 2 {
        int_arg(scope, args, 1)
    } else {
        1
    };
    let mut t = Temporal::new(Kind::PlainYearMonth);
    t.year = y;
    t.month = m.clamp(1, 12);
    t.day = 1;
    Ok(make(scope, prototype, t))
}

fn yearmonth_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainYearMonth)?;
    Ok(scope.string(&format!("{:04}-{:02}", t.year, t.month)))
}

fn monthday_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::PlainMonthDay);
    t.year = 1972;
    t.month = 1;
    t.day = 1;
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        if let Some(text) = text_of(scope, &source) {
            let rest = text.strip_prefix(b"--").unwrap_or(&text);
            if let Some((m, d)) = scan_pair(rest) {
                t.month = m;
                t.day = d;
            }
        }
    } else if !args.is_empty() && source.is_object() {
        t.month = int_prop(scope, &source, "month", 1);
        t.day = int_prop(scope, &source, "day", 1);
    }
    t.month = t.month.clamp(1, 12);
    Ok(make(scope, prototype, t))
}

fn monthday_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let m = if args.is_empty() {
        1
    } else {
        int_arg(scope, args, 0)
    };
    let d = if args.len() >= 2 {
        int_arg(scope, args, 1)
    } else {
        1
    };
    let mut t = Temporal::new(Kind::PlainMonthDay);
    t.year = 1972;
    t.month = m.clamp(1, 12);
    t.day = d;
    Ok(make(scope, prototype, t))
}

fn monthday_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::PlainMonthDay)?;
    Ok(scope.string(&format!("{:02}-{:02}", t.month, t.day)))
}

fn zoned_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::ZonedDateTime);
    t.tz = Some("UTC".to_owned());
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        if let Some(text) = text_of(scope, &source) {
            if let Some(p) = parse_datetime(&text) {
                t.epoch_sec =
                    epoch_of(p.y, p.mo, p.d, p.h, p.mi, p.sec) - i64::from(p.off.wrapping_mul(60));
                t.nanos = p.ms * 1_000_000 + p.us * 1000 + p.ns;
                if let Some(open) = text.iter().position(|&c| c == b'[') {
                    if let Some(close) = text[open..].iter().position(|&c| c == b']') {
                        if close > 1 {
                            let zone = &text[open + 1..open + close];
                            t.tz = Some(String::from_utf8_lossy(zone).into_owned());
                        }
                    }
                }
            }
        }
    }
    Ok(make(scope, prototype, t))
}

fn zoned_constructor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let ns = epoch_nanoseconds_arg(scope, args);
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::ZonedDateTime);
    t.set_epoch_ns(ns);
    let zone = arg(args, 1);
    t.tz = Some(if args.len() >= 2 && zone.is_string() {
        text_of(scope, &zone)
            .map(|z| String::from_utf8_lossy(&z).into_owned())
            .unwrap_or_else(|| "UTC".to_owned())
    } else {
        "UTC".to_owned()
    });
    Ok(make(scope, prototype, t))
}

fn zoned_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::ZonedDateTime)?;
    let (y, mo, d, h, mi, s) = breakdown(t.epoch_sec);
    let mut out = format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}");
    fraction_of(
        &mut out,
        t.nanos / 1_000_000,
        (t.nanos / 1000) % 1000,
        t.nanos % 1000,
    );
    out.push_str(&format!("+00:00[{}]", t.tz.as_deref().unwrap_or("UTC")));
    Ok(scope.string(&out))
}

fn zoned_to_instant(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::ZonedDateTime)?;
    let prototype = global_prototype(scope, "Instant");
    let mut nt = Temporal::new(Kind::Instant);
    nt.epoch_sec = t.epoch_sec;
    nt.nanos = t.nanos;
    Ok(make(scope, prototype, nt))
}

fn zoned_to_plain_date_time(
    scope: &mut Scope<'_>,
    this: &Value,
    _: &[Value],
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::ZonedDateTime)?;
    let (y, mo, d, h, mi, s) = breakdown(t.epoch_sec);
    let prototype = global_prototype(scope, "PlainDateTime");
    let mut nt = Temporal::new(Kind::PlainDateTime);
    (nt.year, nt.month, nt.day, nt.hour, nt.minute, nt.second) = (y, mo, d, h, mi, s);
    nt.ms = t.nanos / 1_000_000;
    nt.us = (t.nanos / 1000) % 1000;
    nt.ns = t.nanos % 1000;
    Ok(make(scope, prototype, nt))
}

fn duration_constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::Duration);
    for (i, slot) in t.dur.iter_mut().enumerate().take(args.len()) {
        *slot = scope.to_int64(&args[i]).unwrap_or(0);
    }
    Ok(make(scope, prototype, t))
}

fn c_truncate(value: f64) -> i64 {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if value.is_nan() || !(-LIMIT..LIMIT).contains(&value) {
        i64::MIN
    } else {
        value as i64
    }
}

fn parse_duration(text: &[u8], dur: &mut [i64; 10]) {
    let mut p = 0;
    let mut sign = 1.0;
    if matches!(byte(text, p), b'-' | b'+') {
        if byte(text, p) == b'-' {
            sign = -1.0;
        }
        p += 1;
    }
    if !matches!(byte(text, p), b'P' | b'p') {
        return;
    }
    p += 1;
    let mut in_time = false;
    while byte(text, p) != 0 {
        if matches!(byte(text, p), b'T' | b't') {
            in_time = true;
            p += 1;
            continue;
        }
        let (value, consumed) = southstar_glib::ascii_strtod_prefix(&text[p..]);
        if consumed == 0 {
            break;
        }
        let end = p + consumed;
        let unit = byte(text, end);
        if unit == 0 {
            break;
        }
        p = end + 1;
        let iv = c_truncate(sign * value);
        match (in_time, unit.to_ascii_uppercase()) {
            (false, b'Y') => dur[0] = iv,
            (false, b'M') => dur[1] = iv,
            (false, b'W') => dur[2] = iv,
            (false, b'D') => dur[3] = iv,
            (true, b'H') => dur[4] = iv,
            (true, b'M') => dur[5] = iv,
            (true, b'S') => {
                dur[6] = iv;
                let frac = sign * (value - c_truncate(value) as f64);
                dur[7] = c_truncate(frac * 1000.0);
                dur[8] = c_truncate(frac * 1_000_000.0).wrapping_rem(1000);
                dur[9] = c_truncate(frac * 1_000_000_000.0).wrapping_rem(1000);
            }
            _ => {}
        }
    }
}

fn duration_from(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let prototype = own_prototype(scope, this);
    let mut t = Temporal::new(Kind::Duration);
    let source = arg(args, 0);
    if !args.is_empty() && source.is_string() {
        if let Some(text) = text_of(scope, &source) {
            parse_duration(&text, &mut t.dur);
        }
    } else if !args.is_empty() && source.is_object() {
        for (slot, name) in t.dur.iter_mut().zip(DURATION_FIELDS) {
            *slot = i64::from(int_prop(scope, &source, name, 0));
        }
    }
    Ok(make(scope, prototype, t))
}

fn duration_to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::Duration)?;
    let mut out = String::new();
    if t.duration_sign() < 0 {
        out.push('-');
    }
    out.push('P');
    let a: Vec<i64> = t.dur.iter().map(|&field| field.wrapping_abs()).collect();
    for (value, unit) in a[..4].iter().zip(['Y', 'M', 'W', 'D']) {
        if *value != 0 {
            out.push_str(&format!("{value}{unit}"));
        }
    }
    let frac_ns = a[7] * 1_000_000 + a[8] * 1000 + a[9];
    if a[4] != 0 || a[5] != 0 || a[6] != 0 || frac_ns != 0 {
        out.push('T');
        if a[4] != 0 {
            out.push_str(&format!("{}H", a[4]));
        }
        if a[5] != 0 {
            out.push_str(&format!("{}M", a[5]));
        }
        if a[6] != 0 || frac_ns != 0 {
            out.push_str(&a[6].to_string());
            fraction(&mut out, frac_ns);
            out.push('S');
        }
    }
    if out.len() == 1 {
        out.push_str("T0S");
    }
    Ok(scope.string(&out))
}

fn duration_arith(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    sign: i64,
) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::Duration)?;
    let other = read_duration(scope, &arg(args, 0));
    let prototype = constructor_prototype(scope, this);
    let mut nt = Temporal::new(Kind::Duration);
    for ((slot, mine), theirs) in nt.dur.iter_mut().zip(t.dur).zip(other) {
        *slot = mine + sign * theirs;
    }
    Ok(make(scope, prototype, nt))
}

fn duration_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    duration_arith(scope, this, args, 1)
}

fn duration_subtract(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    duration_arith(scope, this, args, -1)
}

fn duration_unary(scope: &mut Scope<'_>, this: &Value, abs: bool) -> Result<Value, Value> {
    let t = this_temporal(scope, this, Kind::Duration)?;
    let prototype = constructor_prototype(scope, this);
    let mut nt = Temporal::new(Kind::Duration);
    for (slot, field) in nt.dur.iter_mut().zip(t.dur) {
        *slot = if abs {
            field.wrapping_abs()
        } else {
            field.wrapping_neg()
        };
    }
    Ok(make(scope, prototype, nt))
}

fn duration_negated(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    duration_unary(scope, this, false)
}

fn duration_abs(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    duration_unary(scope, this, true)
}

fn now(scope: &mut Scope<'_>, constructor: &str, kind: Kind) -> Result<Value, Value> {
    let us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_micros() as i64);
    let epoch_sec = us / 1_000_000;
    let nanos = ((us % 1_000_000) * 1000) as i32;
    let prototype = global_prototype(scope, constructor);
    let mut t = Temporal::new(kind);
    match kind {
        Kind::Instant => {
            t.epoch_sec = epoch_sec;
            t.nanos = nanos;
        }
        Kind::ZonedDateTime => {
            t.epoch_sec = epoch_sec;
            t.nanos = nanos;
            t.tz = Some("UTC".to_owned());
        }
        _ => {
            let (y, mo, d, h, mi, s) = breakdown(epoch_sec);
            (t.year, t.month, t.day, t.hour, t.minute, t.second) = (y, mo, d, h, mi, s);
            t.ms = nanos / 1_000_000;
            t.us = (nanos / 1000) % 1000;
            t.ns = nanos % 1000;
        }
    }
    Ok(make(scope, prototype, t))
}

fn now_instant(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    now(scope, "Instant", Kind::Instant)
}

fn now_zoned_date_time(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    now(scope, "ZonedDateTime", Kind::ZonedDateTime)
}

fn now_plain_date_time(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    now(scope, "PlainDateTime", Kind::PlainDateTime)
}

fn now_plain_date(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    now(scope, "PlainDate", Kind::PlainDate)
}

fn now_plain_time(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    now(scope, "PlainTime", Kind::PlainTime)
}

fn now_time_zone_id(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> Result<Value, Value> {
    Ok(scope.string("UTC"))
}

fn to_json(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let to_string = scope
        .get(this, "toString")
        .unwrap_or_else(|_| Value::undefined());
    scope.call(&to_string, this, &[])
}

type Method = (&'static str, NativeFn, u32);

fn bind(scope: &mut Scope<'_>, object: &Value, (name, f, arity): Method) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    scope.define(object, name, function, Attributes::METHOD)
}

fn register(
    scope: &mut Scope<'_>,
    temporal: &Value,
    (name, constructor, arity): Method,
    methods: &[Method],
    statics: &[Method],
) -> Result<Value, Value> {
    let function = scope.constructor(name, arity, constructor);
    let prototype = scope.new_object();
    for &method in methods {
        bind(scope, &prototype, method)?;
    }
    bind(scope, &prototype, ("toJSON", to_json, 0))?;
    scope.define_to_string_tag(&prototype, &format!("Temporal.{name}"))?;
    scope.set_constructor(&function, &prototype)?;
    for &method in statics {
        bind(scope, &function, method)?;
    }
    scope.define(temporal, name, function.clone(), Attributes::METHOD)?;
    Ok(function)
}

fn already_installed(scope: &mut Scope<'_>, global: &Value) -> bool {
    if !scope.has_property(global, "Temporal").unwrap_or(false) {
        return false;
    }
    match scope.get(global, "Temporal") {
        Ok(existing) if existing.is_object() => {
            scope.has_property(&existing, "Now").unwrap_or(false)
        }
        _ => false,
    }
}

pub fn install(scope: &mut Scope<'_>, global: &Value) -> Result<(), Value> {
    if already_installed(scope, global) {
        return Ok(());
    }
    let temporal = scope.new_object();
    register(
        scope,
        &temporal,
        ("Instant", instant_constructor, 1),
        &[
            ("toString", instant_to_string, 0),
            ("add", instant_add, 1),
            ("subtract", instant_subtract, 1),
        ],
        &[
            ("from", instant_from, 1),
            ("fromEpochMilliseconds", instant_from_epoch_milliseconds, 1),
            ("fromEpochNanoseconds", instant_from_epoch_nanoseconds, 1),
        ],
    )?;
    register(
        scope,
        &temporal,
        ("PlainDate", date_constructor, 3),
        &[
            ("toString", date_to_string, 0),
            ("add", date_add, 1),
            ("subtract", date_subtract, 1),
            ("with", date_with, 1),
            ("until", date_until, 1),
            ("since", date_since, 1),
            ("equals", date_equals, 1),
            ("toPlainDateTime", date_to_plain_date_time, 1),
        ],
        &[("from", date_from, 1), ("compare", date_compare, 2)],
    )?;
    register(
        scope,
        &temporal,
        ("PlainTime", time_constructor, 0),
        &[
            ("toString", time_to_string, 0),
            ("add", time_add, 1),
            ("subtract", time_subtract, 1),
        ],
        &[("from", time_from, 1)],
    )?;
    register(
        scope,
        &temporal,
        ("PlainDateTime", datetime_constructor, 3),
        &[
            ("toString", datetime_to_string, 0),
            ("add", datetime_add, 1),
            ("subtract", datetime_subtract, 1),
            ("toPlainDate", datetime_to_plain_date, 0),
            ("toPlainTime", datetime_to_plain_time, 0),
        ],
        &[("from", datetime_from, 1)],
    )?;
    register(
        scope,
        &temporal,
        ("PlainYearMonth", yearmonth_constructor, 2),
        &[("toString", yearmonth_to_string, 0)],
        &[("from", yearmonth_from, 1)],
    )?;
    register(
        scope,
        &temporal,
        ("PlainMonthDay", monthday_constructor, 2),
        &[("toString", monthday_to_string, 0)],
        &[("from", monthday_from, 1)],
    )?;
    register(
        scope,
        &temporal,
        ("ZonedDateTime", zoned_constructor, 2),
        &[
            ("toString", zoned_to_string, 0),
            ("toInstant", zoned_to_instant, 0),
            ("toPlainDateTime", zoned_to_plain_date_time, 0),
        ],
        &[("from", zoned_from, 1)],
    )?;
    let duration = register(
        scope,
        &temporal,
        ("Duration", duration_constructor, 10),
        &[
            ("toString", duration_to_string, 0),
            ("add", duration_add, 1),
            ("subtract", duration_subtract, 1),
            ("negated", duration_negated, 0),
            ("abs", duration_abs, 0),
        ],
        &[("from", duration_from, 1)],
    )?;
    scope.define(&duration, "length", Value::int(0), Attributes::CONFIGURABLE)?;
    let now = scope.new_object();
    for method in [
        ("instant", now_instant as NativeFn, 0),
        ("zonedDateTimeISO", now_zoned_date_time, 0),
        ("plainDateTimeISO", now_plain_date_time, 0),
        ("plainDateISO", now_plain_date, 0),
        ("plainTimeISO", now_plain_time, 0),
        ("timeZoneId", now_time_zone_id, 0),
    ] {
        bind(scope, &now, method)?;
    }
    scope.define_to_string_tag(&now, "Temporal.Now")?;
    scope.define(&temporal, "Now", now, Attributes::METHOD)?;
    scope.define_to_string_tag(&temporal, "Temporal")?;
    scope.define(global, "Temporal", temporal, Attributes::METHOD)
}
