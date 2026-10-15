//! Southstar — Intl.DurationFormat: durations as unit lists or digital clock time, formatted through NumberFormat and ListFormat.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use super::list;
use super::service::{self, Options, Part};
use crate::{Method, arg};

pub(crate) const METHODS: [Method; 3] = [
    ("format", format, 1),
    ("formatToParts", format_to_parts, 1),
    ("resolvedOptions", resolved_options, 0),
];

const BASE_STYLES: [&str; 4] = ["long", "short", "narrow", "digital"];
const DATE_STYLES: [&str; 3] = ["long", "short", "narrow"];
const TIME_STYLES: [&str; 5] = ["long", "short", "narrow", "numeric", "2-digit"];
const SUBSECOND_STYLES: [&str; 4] = ["long", "short", "narrow", "numeric"];
const DISPLAYS: [&str; 2] = ["auto", "always"];
const TIME_SEPARATOR: &str = ":";

const HOURS: usize = 4;
const MINUTES: usize = 5;
const SECONDS: usize = 6;
const NANOSECONDS: usize = 9;

struct UnitRow {
    field: &'static str,
    display: &'static str,
    unit: &'static str,
    styles: &'static [&'static str],
    digital: &'static str,
}

const fn row(
    field: &'static str,
    display: &'static str,
    unit: &'static str,
    styles: &'static [&'static str],
    digital: &'static str,
) -> UnitRow {
    UnitRow {
        field,
        display,
        unit,
        styles,
        digital,
    }
}

const UNITS: [UnitRow; 10] = [
    row("years", "yearsDisplay", "year", &DATE_STYLES, "short"),
    row("months", "monthsDisplay", "month", &DATE_STYLES, "short"),
    row("weeks", "weeksDisplay", "week", &DATE_STYLES, "short"),
    row("days", "daysDisplay", "day", &DATE_STYLES, "short"),
    row("hours", "hoursDisplay", "hour", &TIME_STYLES, "numeric"),
    row(
        "minutes",
        "minutesDisplay",
        "minute",
        &TIME_STYLES,
        "numeric",
    ),
    row(
        "seconds",
        "secondsDisplay",
        "second",
        &TIME_STYLES,
        "numeric",
    ),
    row(
        "milliseconds",
        "millisecondsDisplay",
        "millisecond",
        &SUBSECOND_STYLES,
        "numeric",
    ),
    row(
        "microseconds",
        "microsecondsDisplay",
        "microsecond",
        &SUBSECOND_STYLES,
        "numeric",
    ),
    row(
        "nanoseconds",
        "nanosecondsDisplay",
        "nanosecond",
        &SUBSECOND_STYLES,
        "numeric",
    ),
];

const RECORD_READ_ORDER: [usize; 10] = [3, 4, 8, 7, 5, 1, 9, 6, 2, 0];
const NANOSECONDS_PER_UNIT: [i128; 7] = [
    86_400_000_000_000,
    3_600_000_000_000,
    60_000_000_000,
    1_000_000_000,
    1_000_000,
    1_000,
    1,
];
const MAX_NANOSECONDS: i128 = (1 << 53) * 1_000_000_000;
const MAX_CALENDAR_UNITS: f64 = 4_294_967_296.0;

#[derive(Clone, Copy)]
struct UnitOptions {
    style: &'static str,
    display: &'static str,
}

#[derive(Clone)]
struct DurationFormat {
    locale: String,
    numbering_system: String,
    style: &'static str,
    units: [UnitOptions; 10],
    fractional_digits: Option<u32>,
}

type Record = [f64; 10];

enum Amount {
    Number(f64),
    Decimal(String),
}

impl Amount {
    fn is_zero(&self) -> bool {
        matches!(self, Amount::Number(value) if *value == 0.0)
    }

    fn to_value(&self, scope: &mut Scope<'_>) -> Value {
        match self {
            Amount::Number(value) => Value::number(*value),
            Amount::Decimal(text) => scope.string(text),
        }
    }
}

fn is_numeric_like(style: &str) -> bool {
    matches!(style, "numeric" | "2-digit")
}

fn unit_options(
    scope: &mut Scope<'_>,
    options: &Options,
    row: &UnitRow,
    base_style: &'static str,
    previous: &'static str,
) -> Result<UnitOptions, Value> {
    let requested = options.choice(scope, row.field, row.styles)?;
    let clock_unit = matches!(row.field, "hours" | "minutes" | "seconds");
    let (mut style, mut display_default) = match requested {
        Some(style) => (style, "always"),
        None if base_style == "digital" => {
            (row.digital, if clock_unit { "always" } else { "auto" })
        }
        None if matches!(previous, "fractional" | "numeric" | "2-digit") => {
            let shown = matches!(row.field, "minutes" | "seconds");
            ("numeric", if shown { "always" } else { "auto" })
        }
        None => (base_style, "auto"),
    };
    let subsecond = matches!(row.field, "milliseconds" | "microseconds" | "nanoseconds");
    if style == "numeric" && subsecond {
        style = "fractional";
        display_default = "auto";
    }
    let display = options.choice_or(scope, row.display, &DISPLAYS, display_default)?;
    if display == "always" && style == "fractional" {
        return Err(scope.range_error(&format!(
            "{} cannot always be displayed as a fraction",
            row.field
        )));
    }
    let breaks_fraction = previous == "fractional" && style != "fractional";
    let breaks_clock =
        is_numeric_like(previous) && !matches!(style, "fractional" | "numeric" | "2-digit");
    if breaks_fraction || breaks_clock {
        return Err(scope.range_error(&format!(
            "Style {style} for {} cannot follow style {previous}",
            row.field
        )));
    }
    if matches!(row.field, "minutes" | "seconds") && is_numeric_like(previous) {
        style = "2-digit";
    }
    Ok(UnitOptions { style, display })
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::require_new_target(scope, this, "DurationFormat")?;
    let prototype = service::prototype_from(scope, this, "DurationFormat")?;
    let requested = service::requested_locales(scope, &arg(args, 0))?;
    let options = Options::object(scope, &arg(args, 1))?;
    options.locale_matcher(scope)?;
    let numbering = options.numbering_system(scope)?;
    let (locale, numbering_system) = service::resolve_numbering(
        scope,
        &requested,
        service::COMMON_LANGUAGES,
        numbering.as_deref(),
    );
    let style = options.choice_or(scope, "style", &BASE_STYLES, "short")?;
    let mut units = [UnitOptions {
        style: "short",
        display: "auto",
    }; 10];
    let mut previous = "";
    for (index, row) in UNITS.iter().enumerate() {
        units[index] = unit_options(scope, &options, row, style, previous)?;
        if (HOURS..NANOSECONDS).contains(&index) {
            previous = units[index].style;
        }
    }
    let fractional_digits = options.integer(scope, "fractionalDigits", 0.0..=9.0)?;
    let format = DurationFormat {
        locale,
        numbering_system,
        style,
        units,
        fractional_digits,
    };
    Ok(scope.new_host_object(Some(&prototype), format))
}

pub(crate) fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::supported_locales(scope, args, service::COMMON_LANGUAGES)
}

fn duration_sign(record: &Record) -> i32 {
    record
        .iter()
        .find(|&&value| value != 0.0)
        .map_or(0, |&value| if value < 0.0 { -1 } else { 1 })
}

fn is_valid(record: &Record) -> bool {
    let sign = duration_sign(record);
    let consistent = record.iter().all(|&value| {
        value.is_finite() && !(value < 0.0 && sign > 0) && !(value > 0.0 && sign < 0)
    });
    if !consistent
        || record[..3]
            .iter()
            .any(|value| value.abs() >= MAX_CALENDAR_UNITS)
    {
        return false;
    }
    let mut total: i128 = 0;
    for (&value, scale) in record[3..].iter().zip(NANOSECONDS_PER_UNIT) {
        if value.abs() >= 2.0 * MAX_NANOSECONDS as f64 / scale as f64 {
            return false;
        }
        total += value as i128 * scale;
    }
    total.abs() < MAX_NANOSECONDS
}

struct Number<'a> {
    digits: &'a str,
    fraction: &'a str,
}

fn split_number(text: &str) -> Option<(Number<'_>, char, &str)> {
    let digits_end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    if digits_end == 0 {
        return None;
    }
    let digits = &text[..digits_end];
    let mut rest = &text[digits_end..];
    let mut fraction = "";
    if let Some(after) = rest.strip_prefix(['.', ',']) {
        let fraction_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if !(1..=9).contains(&fraction_end) {
            return None;
        }
        fraction = &after[..fraction_end];
        rest = &after[fraction_end..];
    }
    let designator = rest.chars().next()?;
    let rest = &rest[designator.len_utf8()..];
    Some((
        Number { digits, fraction },
        designator.to_ascii_uppercase(),
        rest,
    ))
}

fn fraction_nanoseconds(fraction: &str, unit_nanoseconds: i128) -> i128 {
    if fraction.is_empty() {
        return 0;
    }
    let scale = 10i128.pow(fraction.len() as u32);
    let digits: i128 = fraction.parse().unwrap_or(0);
    digits * unit_nanoseconds / scale
}

fn parse_date_part(mut text: &str, record: &mut Record) -> Option<()> {
    let mut next_slot = 0;
    while !text.is_empty() {
        let (number, designator, rest) = split_number(text)?;
        let slot = ['Y', 'M', 'W', 'D'].iter().position(|&d| d == designator)?;
        if slot < next_slot || !number.fraction.is_empty() {
            return None;
        }
        record[slot] = number.digits.parse().ok()?;
        next_slot = slot + 1;
        text = rest;
    }
    Some(())
}

fn parse_time_part(mut text: &str, record: &mut Record) -> Option<()> {
    let mut next_slot = 0;
    let mut fraction_seen = false;
    if text.is_empty() {
        return None;
    }
    while !text.is_empty() {
        let (number, designator, rest) = split_number(text)?;
        let slot = ['H', 'M', 'S'].iter().position(|&d| d == designator)?;
        if slot < next_slot || fraction_seen {
            return None;
        }
        record[HOURS + slot] = number.digits.parse().ok()?;
        if !number.fraction.is_empty() {
            fraction_seen = true;
            let mut remaining =
                fraction_nanoseconds(number.fraction, NANOSECONDS_PER_UNIT[1 + slot]);
            for (index, &scale) in NANOSECONDS_PER_UNIT.iter().enumerate().skip(2 + slot) {
                record[3 + index] = (remaining / scale) as f64;
                remaining %= scale;
            }
        }
        next_slot = slot + 1;
        text = rest;
    }
    Some(())
}

fn parse_duration(text: &str) -> Option<Record> {
    let (negative, rest) = if let Some(rest) = text.strip_prefix(['-', '\u{2212}']) {
        (true, rest)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let rest = rest.strip_prefix(['P', 'p'])?;
    let (date, time) = match rest.find(['T', 't']) {
        Some(at) => (&rest[..at], Some(&rest[at + 1..])),
        None => (rest, None),
    };
    if date.is_empty() && time.is_none() {
        return None;
    }
    let mut record = [0.0; 10];
    parse_date_part(date, &mut record)?;
    if let Some(time) = time {
        parse_time_part(time, &mut record)?;
    }
    if negative {
        for value in &mut record {
            *value = -*value + 0.0;
        }
    }
    Some(record)
}

fn temporal_duration_text(scope: &mut Scope<'_>, input: &Value) -> Option<String> {
    let global = scope.global();
    let temporal = scope.get(&global, "Temporal").ok()?;
    if !temporal.is_object() {
        return None;
    }
    let duration = scope.get(&temporal, "Duration").ok()?;
    if !duration.is_object() {
        return None;
    }
    let prototype = scope.get(&duration, "prototype").ok()?;
    if !prototype.is_object() {
        return None;
    }
    let to_string = scope.get(&prototype, "toString").ok()?;
    let text = scope.call(&to_string, input, &[]).ok()?;
    service::text_of(scope, &text).ok()
}

fn record_from_text(scope: &mut Scope<'_>, text: &str) -> Result<Record, Value> {
    match parse_duration(text).filter(is_valid) {
        Some(record) => Ok(record),
        None => Err(scope.range_error(&format!("Invalid duration string {text}"))),
    }
}

fn duration_record(scope: &mut Scope<'_>, input: &Value) -> Result<Record, Value> {
    if input.is_string() {
        let text = service::text_of(scope, input)?;
        return record_from_text(scope, &text);
    }
    if !input.is_object() {
        return Err(scope.type_error("Duration must be an object or a string"));
    }
    if let Some(text) = temporal_duration_text(scope, input) {
        return record_from_text(scope, &text);
    }
    let mut record = [0.0; 10];
    let mut present = false;
    for index in RECORD_READ_ORDER {
        let value = scope.get(input, UNITS[index].field)?;
        if value.is_undefined() {
            continue;
        }
        present = true;
        let number = scope.to_number(&value)?;
        if !number.is_finite() || number.fract() != 0.0 {
            return Err(scope.range_error(&format!(
                "Duration field {} must be an integer",
                UNITS[index].field
            )));
        }
        record[index] = number + 0.0;
    }
    if !present {
        return Err(scope.type_error("Duration object has no duration fields"));
    }
    if !is_valid(&record) {
        return Err(scope.range_error("Duration is out of range"));
    }
    Ok(record)
}

fn fractional_amount(record: &Record, index: usize) -> Amount {
    if record[index + 1..].iter().all(|&value| value == 0.0) {
        return Amount::Number(record[index]);
    }
    let exponent = 9 - 3 * (index - SECONDS);
    let mut nanoseconds = record[NANOSECONDS] as i128;
    for (slot, &value) in record.iter().enumerate().take(NANOSECONDS).skip(index) {
        nanoseconds += value as i128 * NANOSECONDS_PER_UNIT[slot - 3];
    }
    let scale = 10i128.pow(exponent as u32);
    let whole = nanoseconds / scale;
    let fraction = (nanoseconds % scale).abs();
    let sign = if nanoseconds < 0 && whole == 0 {
        "-"
    } else {
        ""
    };
    Amount::Decimal(format!("{sign}{whole}.{fraction:0exponent$}"))
}

struct NumberOptions {
    entries: Vec<(&'static str, Value)>,
}

impl NumberOptions {
    fn new(scope: &mut Scope<'_>, format: &DurationFormat) -> NumberOptions {
        let numbering = scope.string(&format.numbering_system);
        NumberOptions {
            entries: vec![("numberingSystem", numbering)],
        }
    }

    fn push(&mut self, key: &'static str, value: Value) {
        self.entries.push((key, value));
    }

    fn push_text(&mut self, scope: &mut Scope<'_>, key: &'static str, value: &str) {
        let value = scope.string(value);
        self.push(key, value);
    }

    fn fraction_digits(&mut self, scope: &mut Scope<'_>, format: &DurationFormat) {
        let (maximum, minimum) = match format.fractional_digits {
            Some(digits) => (digits as i32, digits as i32),
            None => (9, 0),
        };
        self.push("maximumFractionDigits", Value::int(maximum));
        self.push("minimumFractionDigits", Value::int(minimum));
        self.push_text(scope, "roundingMode", "trunc");
    }

    fn parts(
        self,
        scope: &mut Scope<'_>,
        format: &DurationFormat,
        amount: &Amount,
        unit: &'static str,
    ) -> Result<Vec<Part>, Value> {
        let options = service::plain_object(scope, &self.entries);
        let locale = scope.string(&format.locale);
        let number_format = service::construct_intl(scope, "NumberFormat", &[locale, options])?;
        let value = amount.to_value(scope);
        let parts = service::number_parts(scope, &number_format, &value)?;
        Ok(parts.into_iter().map(|part| part.with_unit(unit)).collect())
    }
}

fn signed(amount: Amount, sign_displayed: bool, negative: bool) -> Amount {
    if sign_displayed && negative && amount.is_zero() {
        Amount::Number(-0.0)
    } else {
        amount
    }
}

fn numeric_field(
    scope: &mut Scope<'_>,
    format: &DurationFormat,
    index: usize,
    amount: &Amount,
    sign_displayed: bool,
) -> Result<Vec<Part>, Value> {
    let mut options = NumberOptions::new(scope, format);
    if format.units[index].style == "2-digit" {
        options.push("minimumIntegerDigits", Value::int(2));
    }
    if !sign_displayed {
        options.push_text(scope, "signDisplay", "never");
    }
    options.push("useGrouping", Value::boolean(false));
    if index == SECONDS {
        options.fraction_digits(scope, format);
    }
    options.parts(scope, format, amount, UNITS[index].unit)
}

fn numeric_units(
    scope: &mut Scope<'_>,
    format: &DurationFormat,
    record: &Record,
    first: usize,
    mut sign_displayed: bool,
) -> Result<Vec<Part>, Value> {
    let negative = duration_sign(record) < 0;
    let hours = record[HOURS];
    let minutes = record[MINUTES];
    let seconds = fractional_amount(record, SECONDS);
    let shows = |index: usize, zero: bool| !zero || format.units[index].display == "always";
    let hours_formatted = first == HOURS && shows(HOURS, hours == 0.0);
    let seconds_formatted = shows(SECONDS, seconds.is_zero());
    let minutes_formatted = first <= MINUTES
        && ((hours_formatted && seconds_formatted) || shows(MINUTES, minutes == 0.0));
    let fields = [
        (HOURS, Amount::Number(hours), hours_formatted),
        (MINUTES, Amount::Number(minutes), minutes_formatted),
        (SECONDS, seconds, seconds_formatted),
    ];
    let mut parts = Vec::new();
    let mut previous_formatted = false;
    for (index, amount, formatted) in fields {
        if formatted {
            if previous_formatted {
                parts.push(Part::literal(TIME_SEPARATOR));
            }
            let amount = signed(amount, sign_displayed, negative);
            parts.extend(numeric_field(
                scope,
                format,
                index,
                &amount,
                sign_displayed,
            )?);
            sign_displayed = false;
        }
        previous_formatted = formatted;
    }
    Ok(parts)
}

fn partition(
    scope: &mut Scope<'_>,
    format: &DurationFormat,
    record: &Record,
) -> Result<Vec<Part>, Value> {
    let negative = duration_sign(record) < 0;
    let mut groups: Vec<Vec<Part>> = Vec::new();
    let mut sign_displayed = true;
    for (index, row) in UNITS.iter().enumerate() {
        let options = format.units[index];
        if is_numeric_like(options.style) {
            let parts = numeric_units(scope, format, record, index, sign_displayed)?;
            if !parts.is_empty() {
                groups.push(parts);
            }
            break;
        }
        let fractional = (SECONDS..NANOSECONDS).contains(&index)
            && format.units[index + 1].style == "fractional";
        let amount = if fractional {
            fractional_amount(record, index)
        } else {
            Amount::Number(record[index])
        };
        if !amount.is_zero() || options.display != "auto" {
            let mut number_options = NumberOptions::new(scope, format);
            if fractional {
                number_options.fraction_digits(scope, format);
            }
            let amount = signed(amount, sign_displayed, negative);
            if !sign_displayed {
                number_options.push_text(scope, "signDisplay", "never");
            }
            sign_displayed = false;
            number_options.push_text(scope, "style", "unit");
            number_options.push_text(scope, "unit", row.unit);
            number_options.push_text(scope, "unitDisplay", options.style);
            groups.push(number_options.parts(scope, format, &amount, row.unit)?);
        }
        if fractional {
            break;
        }
    }
    let list_style = if format.style == "digital" {
        "short"
    } else {
        format.style
    };
    let strings: Vec<String> = groups
        .iter()
        .map(|group| group.iter().map(|part| part.value.as_str()).collect())
        .collect();
    let mut groups = groups.into_iter();
    let mut parts = Vec::new();
    for part in list::unit_list_parts(&format.locale, list_style, &strings) {
        if part.kind == "element" {
            parts.extend(groups.next().unwrap_or_default());
        } else {
            parts.push(part);
        }
    }
    Ok(parts)
}

fn formatted_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> Result<Vec<Part>, Value> {
    let format: DurationFormat = service::receiver(scope, this, method)?;
    let record = duration_record(scope, &arg(args, 0))?;
    partition(scope, &format, &record)
}

fn format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = formatted_parts(scope, this, args, "Intl.DurationFormat.prototype.format")?;
    Ok(service::joined(scope, &parts))
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = formatted_parts(
        scope,
        this,
        args,
        "Intl.DurationFormat.prototype.formatToParts",
    )?;
    Ok(service::parts_array(scope, &parts))
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let format: DurationFormat =
        service::receiver(scope, this, "Intl.DurationFormat.prototype.resolvedOptions")?;
    let options = scope.new_object();
    service::text_property(scope, &options, "locale", &format.locale);
    service::text_property(scope, &options, "numberingSystem", &format.numbering_system);
    service::text_property(scope, &options, "style", format.style);
    for (row, unit) in UNITS.iter().zip(format.units) {
        let style = if unit.style == "fractional" {
            "numeric"
        } else {
            unit.style
        };
        service::text_property(scope, &options, row.field, style);
        service::text_property(scope, &options, row.display, unit.display);
    }
    if let Some(digits) = format.fractional_digits {
        service::data_property(
            scope,
            &options,
            "fractionalDigits",
            Value::int(digits as i32),
        );
    }
    Ok(options)
}
