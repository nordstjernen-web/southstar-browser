//! Southstar — Intl.DateTimeFormat and Temporal: recognising Temporal objects and reading their ISO fields.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ObjectKind, Scope, Value};

use super::calendar::Wall;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    PlainDate,
    PlainDateTime,
    PlainYearMonth,
    PlainMonthDay,
    PlainTime,
    Instant,
    ZonedDateTime,
    Duration,
}

const KINDS: [(Kind, &str, &str); 8] = [
    (Kind::PlainDate, "PlainDate", "calendarId"),
    (Kind::PlainDateTime, "PlainDateTime", "calendarId"),
    (Kind::PlainYearMonth, "PlainYearMonth", "calendarId"),
    (Kind::PlainMonthDay, "PlainMonthDay", "calendarId"),
    (Kind::PlainTime, "PlainTime", "hour"),
    (Kind::Instant, "Instant", "epochMilliseconds"),
    (Kind::ZonedDateTime, "ZonedDateTime", "calendarId"),
    (Kind::Duration, "Duration", "sign"),
];

pub(crate) struct Temporal {
    pub(crate) kind: Kind,
    pub(crate) calendar: String,
    pub(crate) wall: Wall,
    pub(crate) epoch_ms: f64,
    pub(crate) zone: String,
}

fn prototype(scope: &mut Scope<'_>, name: &str) -> Option<Value> {
    let global = scope.global();
    let temporal = scope
        .get(&global, "Temporal")
        .ok()
        .filter(Value::is_object)?;
    let constructor = scope.get(&temporal, name).ok().filter(Value::is_object)?;
    scope
        .get(&constructor, "prototype")
        .ok()
        .filter(Value::is_object)
}

fn getter(scope: &mut Scope<'_>, name: &str, key: &str) -> Option<Value> {
    let prototype = prototype(scope, name)?;
    let key = scope.string(key);
    let descriptor = scope.own_property(&prototype, &key).ok()??;
    (descriptor.accessor && scope.is_function(&descriptor.getter)).then_some(descriptor.getter)
}

fn branded(scope: &mut Scope<'_>, value: &Value, name: &str, key: &str) -> Option<Value> {
    let getter = getter(scope, name, key)?;
    scope.call(&getter, value, &[]).ok()
}

pub(crate) fn kind(scope: &mut Scope<'_>, value: &Value) -> Option<Kind> {
    if !value.is_object() || scope.object_kind(value) != Some(ObjectKind::Other) {
        return None;
    }
    KINDS
        .iter()
        .find(|(_, name, key)| branded(scope, value, name, key).is_some())
        .map(|(kind, ..)| *kind)
}

pub(crate) fn is(scope: &mut Scope<'_>, value: &Value, wanted: Kind) -> bool {
    KINDS
        .iter()
        .find(|(kind, ..)| *kind == wanted)
        .is_some_and(|(_, name, key)| branded(scope, value, name, key).is_some())
}

fn digits(text: &[u8]) -> Option<i64> {
    if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(
        text.iter()
            .fold(0, |sum, &d| sum * 10 + i64::from(d - b'0')),
    )
}

fn parse_date(text: &[u8]) -> Option<(i64, u32, u32, &[u8])> {
    let (sign, rest) = match text.first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => (1, text),
    };
    let dash = rest.iter().position(|&c| c == b'-')?;
    let year = sign * digits(&rest[..dash])?;
    let rest = &rest[dash + 1..];
    let month = digits(rest.get(..2)?)? as u32;
    let day = digits(rest.get(3..5)?)? as u32;
    Some((year, month, day, &rest[5..]))
}

fn parse_time(text: &[u8], wall: &mut Wall) -> Option<()> {
    wall.hour = digits(text.get(..2)?)? as u32;
    wall.minute = digits(text.get(3..5)?)? as u32;
    wall.second = text.get(6..8).and_then(digits).unwrap_or(0) as u32;
    if text.get(8) == Some(&b'.') {
        let fraction: Vec<u8> = text[9..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .copied()
            .chain(std::iter::repeat(b'0'))
            .take(3)
            .collect();
        wall.millisecond = digits(&fraction)? as u32;
    }
    Some(())
}

fn string_of(
    scope: &mut Scope<'_>,
    value: &Value,
    name: &str,
    calendar_name: Option<&str>,
) -> Option<String> {
    let prototype = prototype(scope, name)?;
    let to_string = scope.get(&prototype, "toString").ok()?;
    let args = match calendar_name {
        Some(display) => {
            let options = scope.new_object();
            let display = scope.string(display);
            scope.set(&options, "calendarName", display).ok()?;
            vec![options]
        }
        None => Vec::new(),
    };
    let text = scope.call(&to_string, value, &args).ok()?;
    scope.to_string(&text).ok()
}

fn text_getter(scope: &mut Scope<'_>, value: &Value, name: &str, key: &str) -> Option<String> {
    let result = branded(scope, value, name, key)?;
    scope.to_string(&result).ok()
}

pub(crate) fn read(scope: &mut Scope<'_>, value: &Value, kind: Kind) -> Option<Temporal> {
    let name = KINDS.iter().find(|(known, ..)| *known == kind)?.1;
    let mut temporal = Temporal {
        kind,
        calendar: String::new(),
        wall: Wall {
            year: 1970,
            month: 1,
            day: 1,
            hour: 12,
            ..Wall::default()
        },
        epoch_ms: 0.0,
        zone: String::new(),
    };
    match kind {
        Kind::Instant | Kind::ZonedDateTime => {
            let ms = branded(scope, value, name, "epochMilliseconds")?;
            temporal.epoch_ms = scope.to_number(&ms).ok()?;
            if kind == Kind::ZonedDateTime {
                temporal.calendar = text_getter(scope, value, name, "calendarId")?;
                temporal.zone = text_getter(scope, value, name, "timeZoneId")?;
            }
        }
        Kind::PlainTime => {
            let text = string_of(scope, value, name, None)?;
            temporal.wall.hour = 0;
            parse_time(text.as_bytes(), &mut temporal.wall)?;
        }
        Kind::Duration => {}
        _ => {
            temporal.calendar = text_getter(scope, value, name, "calendarId")?;
            let shown = if matches!(kind, Kind::PlainYearMonth | Kind::PlainMonthDay) {
                "always"
            } else {
                "never"
            };
            let text = string_of(scope, value, name, Some(shown))?;
            let (year, month, day, rest) = parse_date(text.as_bytes())?;
            temporal.wall.year = year;
            temporal.wall.month = month;
            temporal.wall.day = day;
            if kind == Kind::PlainDateTime {
                parse_time(rest.get(1..)?, &mut temporal.wall)?;
            }
        }
    }
    Some(temporal)
}
