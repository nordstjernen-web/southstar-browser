//! Southstar — Intl.DateTimeFormat calendars: proleptic Gregorian arithmetic and other calendars converted through Temporal.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

pub(crate) const CALENDARS: [&str; 16] = [
    "buddhist",
    "chinese",
    "coptic",
    "dangi",
    "ethioaa",
    "ethiopic",
    "gregory",
    "hebrew",
    "indian",
    "islamic-civil",
    "islamic-tbla",
    "islamic-umalqura",
    "iso8601",
    "japanese",
    "persian",
    "roc",
];

pub(crate) const MS_PER_DAY: i64 = 86_400_000;

#[derive(Clone, Copy, Default)]
pub(crate) struct Wall {
    pub(crate) year: i64,
    pub(crate) month: u32,
    pub(crate) day: u32,
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) second: u32,
    pub(crate) millisecond: u32,
}

#[derive(Clone, Default)]
pub(crate) struct CalendarDate {
    pub(crate) era: Option<String>,
    pub(crate) era_year: Option<i64>,
    pub(crate) year: i64,
    pub(crate) ordinal: u32,
    pub(crate) month: u32,
    pub(crate) leap_month: bool,
    pub(crate) day: u32,
    pub(crate) months_in_year: u32,
}

pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year =
        (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub(crate) fn wall_from_ms(ms: i64) -> Wall {
    let days = ms.div_euclid(MS_PER_DAY);
    let in_day = ms.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    Wall {
        year,
        month,
        day,
        hour: (in_day / 3_600_000) as u32,
        minute: (in_day / 60_000 % 60) as u32,
        second: (in_day / 1000 % 60) as u32,
        millisecond: (in_day % 1000) as u32,
    }
}

pub(crate) fn weekday(wall: &Wall) -> u32 {
    (days_from_civil(wall.year, wall.month, wall.day) + 4).rem_euclid(7) as u32
}

pub(crate) fn has_eras(calendar: &str) -> bool {
    !matches!(calendar, "chinese" | "dangi")
}

fn gregorian(wall: &Wall) -> CalendarDate {
    let (era, era_year) = if wall.year <= 0 {
        ("bce", 1 - wall.year)
    } else {
        ("ce", wall.year)
    };
    CalendarDate {
        era: Some(era.to_string()),
        era_year: Some(era_year),
        year: wall.year,
        ordinal: wall.month,
        month: wall.month,
        leap_month: false,
        day: wall.day,
        months_in_year: 12,
    }
}

fn number(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<i64> {
    let value = scope.get(object, key).ok()?;
    if value.is_undefined() {
        return None;
    }
    scope.to_number(&value).ok().map(|n| n as i64)
}

fn text(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<String> {
    let value = scope.get(object, key).ok()?;
    if value.is_undefined() {
        return None;
    }
    scope.to_string(&value).ok()
}

fn converted(scope: &mut Scope<'_>, calendar: &str, wall: &Wall) -> Option<CalendarDate> {
    let global = scope.global();
    let temporal = scope.get(&global, "Temporal").ok()?;
    let constructor = scope.get(&temporal, "PlainDate").ok()?;
    let calendar_value = scope.string(calendar);
    let date = scope
        .construct(
            &constructor,
            &[
                Value::number(wall.year as f64),
                Value::number(f64::from(wall.month)),
                Value::number(f64::from(wall.day)),
                calendar_value,
            ],
        )
        .ok()?;
    let code = text(scope, &date, "monthCode")?;
    let month = code
        .get(1..3)
        .and_then(|digits| digits.parse::<u32>().ok())?;
    Some(CalendarDate {
        era: text(scope, &date, "era"),
        era_year: number(scope, &date, "eraYear"),
        year: number(scope, &date, "year")?,
        ordinal: number(scope, &date, "month")? as u32,
        month,
        leap_month: code.ends_with('L'),
        day: number(scope, &date, "day")? as u32,
        months_in_year: number(scope, &date, "monthsInYear").unwrap_or(12) as u32,
    })
}

pub(crate) fn calendar_date(scope: &mut Scope<'_>, calendar: &str, wall: &Wall) -> CalendarDate {
    match calendar {
        "gregory" | "iso8601" => gregorian(wall),
        _ => converted(scope, calendar, wall).unwrap_or_else(|| gregorian(wall)),
    }
}
