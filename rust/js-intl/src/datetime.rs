//! Southstar — Intl.DateTimeFormat: construction, option resolution, formatting to strings, parts and ranges, and the toLocaleString methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod calendar;
mod data;
mod locales;
mod pattern;
mod temporal;
mod zone;

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use southstar_js_engine::{Attributes, NativeFn, ObjectKind, Scope, Trace, Value};

use self::calendar::{CalendarDate, Wall};
use self::data::{Cycle, LocaleData};
use self::locales::{Keyword, Requested, Tag};
use self::pattern::{FIELDS, Field, Format, Moment, Renderer, Request, Style};
use self::temporal::{Kind, Temporal};
use crate::{Method, arg, ffi};

pub(crate) const METHODS: [Method; 4] = [
    ("formatToParts", format_to_parts, 1),
    ("formatRange", format_range, 2),
    ("formatRangeToParts", format_range_to_parts, 2),
    ("resolvedOptions", resolved_options, 0),
];

const STYLE_NAMES: [&str; 4] = ["full", "long", "medium", "short"];
const DATE_FIELDS: [Field; 5] = [
    Field::Weekday,
    Field::Era,
    Field::Year,
    Field::Month,
    Field::Day,
];
const TIME_FIELDS: [Field; 5] = [
    Field::DayPeriod,
    Field::Hour,
    Field::Minute,
    Field::Second,
    Field::Fraction,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Required {
    Date,
    Time,
    YearMonth,
    MonthDay,
    Any,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Defaults {
    Date,
    Time,
    YearMonth,
    MonthDay,
    ZonedDateTime,
    All,
}

struct DateTimeFormat {
    locale: String,
    data: &'static LocaleData,
    calendar: String,
    numbering: String,
    zone: String,
    cycle: Cycle,
    date_style: Option<usize>,
    time_style: Option<usize>,
    date_time: Format,
    request: Request,
    zoned: bool,
    temporal: OnceCell<TemporalFormats>,
}

struct TemporalFormats {
    plain_date: Option<Format>,
    plain_year_month: Option<Format>,
    plain_month_day: Option<Format>,
    plain_time: Option<Format>,
    plain_date_time: Format,
    instant: Format,
}

impl DateTimeFormat {
    fn styled(&self) -> bool {
        self.date_style.is_some() || self.time_style.is_some()
    }

    fn builder(&self) -> Builder<'_> {
        Builder {
            data: self.data,
            calendar: &self.calendar,
            cycle: self.cycle,
            decimal: pattern::decimal_for(self.data, &self.numbering),
        }
    }

    fn temporal_formats(&self) -> &TemporalFormats {
        self.temporal.get_or_init(|| {
            let builder = self.builder();
            if self.styled() {
                builder.styled_formats(
                    &self.date_time,
                    self.date_style.is_some(),
                    self.time_style.is_some(),
                )
            } else {
                builder.component_formats(&self.request, self.zoned)
            }
        })
    }
}

#[derive(Clone)]
struct Instance {
    format: Rc<DateTimeFormat>,
    bound: Rc<RefCell<Option<Value>>>,
}

impl Trace for Instance {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(bound) = self.bound.borrow().as_ref() {
            visit(bound);
        }
    }
}

struct Options(Option<Value>);

impl Options {
    fn coerce(scope: &mut Scope<'_>, value: &Value) -> Result<Options, Value> {
        if value.is_undefined() {
            Ok(Options(None))
        } else {
            Ok(Options(Some(scope.to_object(value)?)))
        }
    }

    fn get(&self, scope: &mut Scope<'_>, key: &str) -> Result<Value, Value> {
        match &self.0 {
            Some(object) => scope.get(object, key),
            None => Ok(Value::undefined()),
        }
    }

    fn string(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        allowed: &[&str],
    ) -> Result<Option<String>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        let text = scope.to_string(&value)?;
        if !allowed.is_empty() && !allowed.contains(&text.as_str()) {
            return Err(scope.range_error(&format!("{text} is not a valid {key}")));
        }
        Ok(Some(text))
    }

    fn boolean(&self, scope: &mut Scope<'_>, key: &str) -> Result<Option<bool>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        Ok(Some(scope.to_bool(&value)))
    }

    fn number(
        &self,
        scope: &mut Scope<'_>,
        key: &str,
        minimum: f64,
        maximum: f64,
    ) -> Result<Option<u8>, Value> {
        let value = self.get(scope, key)?;
        if value.is_undefined() {
            return Ok(None);
        }
        let number = scope.to_number(&value)?;
        if number.is_nan() || number < minimum || number > maximum {
            return Err(scope.range_error(&format!("{key} is out of range")));
        }
        Ok(Some(number.floor() as u8))
    }

    fn identifier(&self, scope: &mut Scope<'_>, key: &str) -> Result<Option<String>, Value> {
        let value = self.string(scope, key, &[])?;
        if let Some(text) = &value
            && !locales::is_type_sequence(text)
        {
            return Err(scope.range_error(&format!("{text} is not a valid {key}")));
        }
        Ok(value)
    }
}

fn available(tag: &Tag) -> bool {
    tag.variants.is_empty() && data::has_language(&tag.language)
}

fn default_tag() -> Tag {
    static DEFAULT: OnceLock<Tag> = OnceLock::new();
    DEFAULT.get_or_init(system_tag).clone()
}

fn system_tag() -> Tag {
    let fallback = || locales::canonical_tag("en-US").unwrap_or_default();
    let system = crate::negotiate::default_locale().to_owned();
    match locales::canonical_tag(&system) {
        Some(tag) if available(&tag.without_unicode()) => tag.without_unicode(),
        Some(tag) => {
            let base = Tag {
                language: tag.language.clone(),
                script: tag.script.clone(),
                region: tag.region.clone(),
                ..Tag::default()
            };
            if available(&base) { base } else { fallback() }
        }
        None => fallback(),
    }
}

fn requested(value: Option<String>) -> Requested {
    value.map_or(Requested::Absent, Requested::Text)
}

fn keyword_default(tag: &Tag, key: &str) -> Option<String> {
    match key {
        "ca" => Some("gregory".to_string()),
        "nu" => Some(data::default_numbering(&tag.language, tag.region.as_deref()).to_string()),
        _ => None,
    }
}

fn style_level(name: Option<String>) -> Option<usize> {
    name.and_then(|name| STYLE_NAMES.iter().position(|known| *known == name))
}

struct Builder<'a> {
    data: &'static LocaleData,
    calendar: &'a str,
    cycle: Cycle,
    decimal: &'static str,
}

impl Builder<'_> {
    fn components(&self, request: &Request) -> Format {
        pattern::components(self.data, self.calendar, request, self.cycle, self.decimal)
    }

    fn best(
        &self,
        options: &Request,
        required: Required,
        defaults: Defaults,
        inherit_all: bool,
    ) -> Option<Format> {
        let required_fields: &[Field] = match required {
            Required::Date => &[Field::Weekday, Field::Year, Field::Month, Field::Day],
            Required::Time => &TIME_FIELDS,
            Required::YearMonth => &[Field::Year, Field::Month],
            Required::MonthDay => &[Field::Month, Field::Day],
            Required::Any => &[
                Field::Weekday,
                Field::Year,
                Field::Month,
                Field::Day,
                Field::DayPeriod,
                Field::Hour,
                Field::Minute,
                Field::Second,
                Field::Fraction,
            ],
        };
        let default_fields: &[Field] = match defaults {
            Defaults::Date => &[Field::Year, Field::Month, Field::Day],
            Defaults::Time => &[Field::Hour, Field::Minute, Field::Second],
            Defaults::YearMonth => &[Field::Year, Field::Month],
            Defaults::MonthDay => &[Field::Month, Field::Day],
            Defaults::ZonedDateTime | Defaults::All => &[
                Field::Year,
                Field::Month,
                Field::Day,
                Field::Hour,
                Field::Minute,
                Field::Second,
            ],
        };
        let mut request: Request = if inherit_all { *options } else { [None; 11] };
        if !inherit_all
            && matches!(
                required,
                Required::Date | Required::YearMonth | Required::Any
            )
        {
            request[Field::Era.index()] = options[Field::Era.index()];
        }
        let any_present = FIELDS
            .iter()
            .filter(|field| !matches!(field, Field::Era | Field::ZoneName))
            .any(|field| options[field.index()].is_some());
        let mut need_defaults = true;
        for field in required_fields {
            if let Some(style) = options[field.index()] {
                request[field.index()] = Some(style);
                need_defaults = false;
            }
        }
        if need_defaults {
            if any_present && !inherit_all {
                return None;
            }
            for field in default_fields {
                request[field.index()] = Some(Style::Numeric);
            }
            if defaults == Defaults::ZonedDateTime && request[Field::ZoneName.index()].is_none() {
                request[Field::ZoneName.index()] = Some(Style::Short);
            }
        }
        Some(self.components(&request))
    }

    fn styled_formats(&self, base: &Format, date: bool, time: bool) -> TemporalFormats {
        let all_fields: Vec<Field> = DATE_FIELDS.iter().chain(&TIME_FIELDS).copied().collect();
        TemporalFormats {
            plain_date: date.then(|| self.adjust(base, &DATE_FIELDS)),
            plain_year_month: date
                .then(|| self.adjust(base, &[Field::Era, Field::Year, Field::Month])),
            plain_month_day: date.then(|| self.adjust(base, &[Field::Month, Field::Day])),
            plain_time: time.then(|| self.adjust(base, &TIME_FIELDS)),
            plain_date_time: self.adjust(base, &all_fields),
            instant: base.clone(),
        }
    }

    fn component_formats(&self, request: &Request, zoned: bool) -> TemporalFormats {
        let instant_defaults = if zoned {
            Defaults::ZonedDateTime
        } else {
            Defaults::All
        };
        let fallback = || self.components(&[None; 11]);
        TemporalFormats {
            plain_date: self.best(request, Required::Date, Defaults::Date, false),
            plain_year_month: self.best(request, Required::YearMonth, Defaults::YearMonth, false),
            plain_month_day: self.best(request, Required::MonthDay, Defaults::MonthDay, false),
            plain_time: self.best(request, Required::Time, Defaults::Time, false),
            plain_date_time: self
                .best(request, Required::Any, Defaults::All, false)
                .unwrap_or_else(fallback),
            instant: self
                .best(request, Required::Any, instant_defaults, true)
                .unwrap_or_else(fallback),
        }
    }

    fn adjust(&self, base: &Format, allowed: &[Field]) -> Format {
        let conflicting = FIELDS
            .iter()
            .any(|field| base.has(*field) && !allowed.contains(field));
        if !conflicting {
            return base.clone();
        }
        let mut request: Request = [None; 11];
        for field in allowed {
            request[field.index()] = base.fields[field.index()];
        }
        self.components(&request)
    }
}

fn create(
    scope: &mut Scope<'_>,
    locales: &Value,
    options: &Value,
    required: Required,
    defaults: Defaults,
    zone_override: Option<&str>,
) -> Result<DateTimeFormat, Value> {
    let requested_locales = locales::canonicalize_locale_list(scope, locales)?;
    let options = Options::coerce(scope, options)?;
    options.string(scope, "localeMatcher", &["lookup", "best fit"])?;
    let calendar = options.identifier(scope, "calendar")?;
    let numbering = options.identifier(scope, "numberingSystem")?;
    let hour12 = options.boolean(scope, "hour12")?;
    let hour_cycle = options.string(scope, "hourCycle", &["h11", "h12", "h23", "h24"])?;
    let hc = if hour12.is_some() {
        Requested::Null
    } else {
        requested(hour_cycle)
    };
    let is_calendar = |value: &str| calendar::CALENDARS.contains(&value);
    let is_cycle = |value: &str| Cycle::parse(value).is_some();
    let resolution = locales::resolve(
        &requested_locales,
        &available,
        &default_tag(),
        &[
            Keyword {
                key: "ca",
                requested: requested(calendar),
                supported: &is_calendar,
            },
            Keyword {
                key: "hc",
                requested: hc,
                supported: &is_cycle,
            },
            Keyword {
                key: "nu",
                requested: requested(numbering),
                supported: &pattern::numbering_supported,
            },
        ],
        &keyword_default,
    );
    let tag = &resolution.data_tag;
    let data = data::locale_data(&tag.language, tag.region.as_deref());
    let mut values = resolution.values.into_iter();
    let calendar = values
        .next()
        .flatten()
        .unwrap_or_else(|| "gregory".to_string());
    let resolved_cycle = values.next().flatten();
    let numbering = values
        .next()
        .flatten()
        .unwrap_or_else(|| "latn".to_string());
    let cycle = match hour12 {
        Some(true) => data.cycle12,
        Some(false) => Cycle::H23,
        None => resolved_cycle
            .as_deref()
            .and_then(Cycle::parse)
            .unwrap_or(data.cycle),
    };
    let zone_value = options.get(scope, "timeZone")?;
    let zone = if zone_value.is_undefined() {
        match zone_override {
            Some(zone) => zone.to_string(),
            None => zone::system_zone(scope),
        }
    } else {
        if zone_override.is_some() {
            return Err(scope.type_error("the timeZone option cannot be used here"));
        }
        scope.to_string(&zone_value)?
    };
    let zone = match zone::parse_offset(&zone) {
        Some(minutes) => zone::offset_id(minutes),
        None => match zone::named(scope, &zone) {
            Some(named) => named,
            None => return Err(scope.range_error(&format!("invalid time zone: {zone}"))),
        },
    };
    let mut request: Request = [None; 11];
    let mut explicit = false;
    for field in FIELDS {
        let style = if field == Field::Fraction {
            options
                .number(scope, field.option(), 1.0, 3.0)?
                .map(Style::Digits)
        } else {
            options
                .string(scope, field.option(), field.values())?
                .and_then(|text| Style::parse(&text))
        };
        explicit |= style.is_some();
        request[field.index()] = style;
    }
    options.string(scope, "formatMatcher", &["basic", "best fit"])?;
    let date_style = style_level(options.string(scope, "dateStyle", &STYLE_NAMES)?);
    let time_style = style_level(options.string(scope, "timeStyle", &STYLE_NAMES)?);
    let builder = Builder {
        data,
        calendar: &calendar,
        cycle,
        decimal: pattern::decimal_for(data, &numbering),
    };
    let styled = date_style.is_some() || time_style.is_some();
    let date_time = if styled {
        if explicit {
            return Err(scope.type_error(
                "dateStyle and timeStyle cannot be combined with other date-time options",
            ));
        }
        if required == Required::Date && time_style.is_some() {
            return Err(scope.type_error("timeStyle cannot be used to format a date"));
        }
        if required == Required::Time && date_style.is_some() {
            return Err(scope.type_error("dateStyle cannot be used to format a time"));
        }
        pattern::styles(data, &calendar, date_style, time_style, cycle)
    } else {
        builder
            .best(&request, required, defaults, true)
            .unwrap_or_else(|| builder.components(&[None; 11]))
    };
    Ok(DateTimeFormat {
        locale: resolution.locale,
        data,
        calendar,
        numbering,
        zone,
        cycle,
        date_style,
        time_style,
        date_time,
        request,
        zoned: zone_override.is_some(),
        temporal: OnceCell::new(),
    })
}

fn intrinsic_prototype(scope: &mut Scope<'_>) -> Result<Value, Value> {
    let constructor = crate::global_constructor(scope, "DateTimeFormat")?;
    scope.get(&constructor, "prototype")
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let prototype = if this.is_undefined() {
        None
    } else {
        scope.get(this, "prototype").ok().filter(Value::is_object)
    };
    let prototype = match prototype {
        Some(prototype) => prototype,
        None => intrinsic_prototype(scope)?,
    };
    let format = create(
        scope,
        &arg(args, 0),
        &arg(args, 1),
        Required::Any,
        Defaults::Date,
        None,
    )?;
    Ok(scope.new_traced_host_object(
        Some(&prototype),
        Instance {
            format: Rc::new(format),
            bound: Rc::default(),
        },
    ))
}

fn instance(scope: &mut Scope<'_>, this: &Value, method: &str) -> Result<Instance, Value> {
    match this.with_host(|instance: &Instance| instance.clone()) {
        Some(instance) => Ok(instance),
        None => Err(scope.type_error(&format!(
            "Intl.DateTimeFormat.prototype.{method} called on an incompatible receiver"
        ))),
    }
}

enum Input {
    Time(f64),
    Temporal(Temporal),
}

enum Source {
    Epoch(f64),
    Plain(Wall),
}

fn formattable(scope: &mut Scope<'_>, value: &Value) -> Result<Input, Value> {
    if let Some(kind) = temporal::kind(scope, value)
        && kind != Kind::Duration
    {
        return match temporal::read(scope, value, kind) {
            Some(read) => Ok(Input::Temporal(read)),
            None => Err(scope.type_error("cannot read the Temporal object")),
        };
    }
    Ok(Input::Time(scope.to_number(value)?))
}

fn time_clip(time: f64) -> Option<f64> {
    if !time.is_finite() || time.abs() > 8.64e15 {
        return None;
    }
    Some(time.trunc() + 0.0)
}

fn handle<'a>(
    scope: &mut Scope<'_>,
    format: &'a DateTimeFormat,
    input: &Input,
) -> Result<(&'a Format, Source), Value> {
    let temporal = match input {
        Input::Time(time) => {
            return match time_clip(*time) {
                Some(time) => Ok((&format.date_time, Source::Epoch(time))),
                None => Err(scope.range_error("Invalid time value")),
            };
        }
        Input::Temporal(temporal) => temporal,
    };
    if matches!(temporal.kind, Kind::ZonedDateTime | Kind::Duration) {
        return Err(
            scope.type_error("Temporal.ZonedDateTime cannot be formatted by Intl.DateTimeFormat")
        );
    }
    let iso_allowed = matches!(temporal.kind, Kind::PlainDate | Kind::PlainDateTime);
    let calendar_ok = temporal.calendar == format.calendar
        || (iso_allowed && temporal.calendar == "iso8601")
        || matches!(temporal.kind, Kind::PlainTime | Kind::Instant);
    if !calendar_ok {
        return Err(scope.range_error(&format!(
            "calendar {} does not match the formatter's calendar {}",
            temporal.calendar, format.calendar
        )));
    }
    let formats = format.temporal_formats();
    let chosen = match temporal.kind {
        Kind::PlainDate => formats.plain_date.as_ref(),
        Kind::PlainYearMonth => formats.plain_year_month.as_ref(),
        Kind::PlainMonthDay => formats.plain_month_day.as_ref(),
        Kind::PlainTime => formats.plain_time.as_ref(),
        Kind::PlainDateTime => Some(&formats.plain_date_time),
        Kind::Instant => {
            return Ok((&formats.instant, Source::Epoch(temporal.epoch_ms)));
        }
        Kind::ZonedDateTime | Kind::Duration => None,
    };
    match chosen {
        Some(chosen) => Ok((chosen, Source::Plain(temporal.wall))),
        None => {
            Err(scope.type_error("the formatter has no fields that apply to this Temporal object"))
        }
    }
}

fn needs_date(format: &Format) -> bool {
    format.tokens.iter().any(|token| {
        matches!(
            token,
            pattern::Token::Field(b'G' | b'y' | b'r' | b'U' | b'M' | b'L' | b'd', _)
        )
    })
}

fn moment(
    scope: &mut Scope<'_>,
    format: &DateTimeFormat,
    chosen: &Format,
    source: &Source,
) -> Moment {
    let (wall, offset_ms, daylight) = match source {
        Source::Epoch(time) => {
            let offset = zone::offset_ms(scope, &format.zone, *time);
            let wall = calendar::wall_from_ms(*time as i64 + offset);
            let named_zone = matches!(
                chosen.fields[Field::ZoneName.index()],
                Some(Style::Short | Style::Long)
            );
            let daylight =
                named_zone && zone::in_daylight_time(scope, &format.zone, wall.year, offset);
            (wall, offset, daylight)
        }
        Source::Plain(wall) => (*wall, 0, false),
    };
    let date = if needs_date(chosen) {
        calendar::calendar_date(scope, &format.calendar, &wall)
    } else {
        CalendarDate::default()
    };
    Moment {
        wall,
        weekday: calendar::weekday(&wall),
        date,
        offset_ms,
        daylight,
    }
}

fn renderer<'a>(format: &'a DateTimeFormat, source: &Source) -> Renderer<'a> {
    let zone = match source {
        Source::Plain(_) => "+00:00",
        Source::Epoch(_) => &format.zone,
    };
    Renderer::new(format.data, &format.calendar, zone, &format.numbering)
}

fn format_parts(
    scope: &mut Scope<'_>,
    format: &DateTimeFormat,
    input: &Input,
) -> Result<Vec<pattern::Part>, Value> {
    let (chosen, source) = handle(scope, format, input)?;
    let moment = moment(scope, format, chosen, &source);
    Ok(renderer(format, &source).parts(&chosen.tokens, &moment))
}

fn joined(parts: &[pattern::Part]) -> String {
    parts.iter().map(|(_, value)| value.as_str()).collect()
}

fn same_kind(x: &Input, y: &Input) -> bool {
    match (x, y) {
        (Input::Time(_), Input::Time(_)) => true,
        (Input::Temporal(a), Input::Temporal(b)) => a.kind == b.kind,
        _ => false,
    }
}

fn range_parts(
    scope: &mut Scope<'_>,
    format: &DateTimeFormat,
    x: &Input,
    y: &Input,
) -> Result<Vec<(&'static str, String, &'static str)>, Value> {
    if !same_kind(x, y) {
        return Err(scope.type_error("formatRange needs two values of the same kind"));
    }
    let (chosen, start_source) = handle(scope, format, x)?;
    let (_, end_source) = handle(scope, format, y)?;
    let start = moment(scope, format, chosen, &start_source);
    let end = moment(scope, format, chosen, &end_source);
    Ok(renderer(format, &start_source).range(&chosen.tokens, &start, &end))
}

fn data_object(scope: &mut Scope<'_>, entries: &[(&str, Value)]) -> Result<Value, Value> {
    let object = scope.new_object();
    for (key, value) in entries {
        let key = scope.string(key);
        scope.define_entry(&object, &key, value.clone())?;
    }
    Ok(object)
}

fn parts_array(
    scope: &mut Scope<'_>,
    parts: &[(&'static str, String, Option<&'static str>)],
) -> Result<Value, Value> {
    let array = scope.new_array();
    for (index, (kind, value, source)) in parts.iter().enumerate() {
        let mut entries = vec![("type", scope.string(kind)), ("value", scope.string(value))];
        if let Some(source) = source {
            entries.push(("source", scope.string(source)));
        }
        let part = data_object(scope, &entries)?;
        scope.define_entry(&array, &Value::int(index as i32), part)?;
    }
    Ok(array)
}

fn now_or(scope: &mut Scope<'_>, value: Option<&Value>) -> Result<Input, Value> {
    match value {
        Some(value) if !value.is_undefined() => formattable(scope, value),
        _ => Ok(Input::Time(ffi::real_time_ms())),
    }
}

fn format_bound(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let instance = instance(scope, &arg(data, 0), "format")?;
    let input = now_or(scope, args.first())?;
    let parts = format_parts(scope, &instance.format, &input)?;
    Ok(scope.string(&joined(&parts)))
}

fn format_getter(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let instance = instance(scope, this, "format")?;
    if let Some(bound) = instance.bound.borrow().as_ref() {
        return Ok(bound.clone());
    }
    let bound = scope.bound_function("", 1, format_bound, std::slice::from_ref(this));
    *instance.bound.borrow_mut() = Some(bound.clone());
    Ok(bound)
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let instance = instance(scope, this, "formatToParts")?;
    let input = now_or(scope, args.first())?;
    let parts = format_parts(scope, &instance.format, &input)?;
    let parts: Vec<_> = parts
        .into_iter()
        .map(|(kind, value)| (kind, value, None))
        .collect();
    parts_array(scope, &parts)
}

fn range_inputs(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> Result<(Instance, Input, Input), Value> {
    let instance = instance(scope, this, method)?;
    let start = arg(args, 0);
    let end = arg(args, 1);
    if start.is_undefined() || end.is_undefined() {
        return Err(scope.type_error(&format!("{method} needs a start and an end date")));
    }
    let x = formattable(scope, &start)?;
    let y = formattable(scope, &end)?;
    Ok((instance, x, y))
}

fn format_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (instance, x, y) = range_inputs(scope, this, args, "formatRange")?;
    let parts = range_parts(scope, &instance.format, &x, &y)?;
    let text: String = parts.iter().map(|(_, value, _)| value.as_str()).collect();
    Ok(scope.string(&text))
}

fn format_range_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (instance, x, y) = range_inputs(scope, this, args, "formatRangeToParts")?;
    let parts = range_parts(scope, &instance.format, &x, &y)?;
    let parts: Vec<_> = parts
        .into_iter()
        .map(|(kind, value, source)| (kind, value, Some(source)))
        .collect();
    parts_array(scope, &parts)
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let instance = instance(scope, this, "resolvedOptions")?;
    let format = &instance.format;
    let mut entries: Vec<(&str, Value)> = vec![
        ("locale", scope.string(&format.locale)),
        ("calendar", scope.string(&format.calendar)),
        ("numberingSystem", scope.string(&format.numbering)),
        ("timeZone", scope.string(&format.zone)),
    ];
    if format.date_time.has(Field::Hour) {
        entries.push(("hourCycle", scope.string(format.cycle.name())));
        entries.push(("hour12", Value::boolean(format.cycle.twelve_hour())));
    }
    if !format.styled() {
        for field in FIELDS {
            match format.date_time.fields[field.index()] {
                Some(Style::Digits(digits)) => {
                    entries.push((field.option(), Value::int(i32::from(digits))));
                }
                Some(style) => entries.push((field.option(), scope.string(style.name()))),
                None => {}
            }
        }
    }
    if let Some(level) = format.date_style {
        entries.push(("dateStyle", scope.string(STYLE_NAMES[level])));
    }
    if let Some(level) = format.time_style {
        entries.push(("timeStyle", scope.string(STYLE_NAMES[level])));
    }
    data_object(scope, &entries)
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let requested = locales::canonicalize_locale_list(scope, &arg(args, 0))?;
    let options = Options::coerce(scope, &arg(args, 1))?;
    options.string(scope, "localeMatcher", &["lookup", "best fit"])?;
    let array = scope.new_array();
    let mut count = 0;
    for locale in requested {
        if locales::lookup(std::slice::from_ref(&locale), &available).is_some() {
            let value = scope.string(&locale);
            scope.define_entry(&array, &Value::int(count), value)?;
            count += 1;
        }
    }
    Ok(array)
}

fn format_value(
    scope: &mut Scope<'_>,
    args: &[Value],
    input: &Input,
    required: Required,
    defaults: Defaults,
    zone_override: Option<&str>,
) -> Result<Value, Value> {
    let format = create(
        scope,
        &arg(args, 0),
        &arg(args, 1),
        required,
        defaults,
        zone_override,
    )?;
    let parts = format_parts(scope, &format, input)?;
    Ok(scope.string(&joined(&parts)))
}

fn date_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
    required: Required,
    defaults: Defaults,
) -> Result<Value, Value> {
    if scope.object_kind(this) != Some(ObjectKind::Date) {
        return Err(scope.type_error(&format!(
            "Date.prototype.{method} requires that 'this' be a Date"
        )));
    }
    let time = scope.to_number(this)?;
    if time.is_nan() {
        return Ok(scope.string("Invalid Date"));
    }
    format_value(scope, args, &Input::Time(time), required, defaults, None)
}

fn date_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    date_method(
        scope,
        this,
        args,
        "toLocaleString",
        Required::Any,
        Defaults::All,
    )
}

fn date_to_locale_date_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    date_method(
        scope,
        this,
        args,
        "toLocaleDateString",
        Required::Date,
        Defaults::Date,
    )
}

fn date_to_locale_time_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    date_method(
        scope,
        this,
        args,
        "toLocaleTimeString",
        Required::Time,
        Defaults::Time,
    )
}

fn temporal_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    kind: Kind,
    required: Required,
    defaults: Defaults,
) -> Result<Value, Value> {
    let read = if temporal::is(scope, this, kind) {
        temporal::read(scope, this, kind)
    } else {
        None
    };
    let Some(read) = read else {
        return Err(scope.type_error("toLocaleString called on an incompatible receiver"));
    };
    if kind != Kind::ZonedDateTime {
        return format_value(
            scope,
            args,
            &Input::Temporal(read),
            required,
            defaults,
            None,
        );
    }
    let format = create(
        scope,
        &arg(args, 0),
        &arg(args, 1),
        required,
        defaults,
        Some(&read.zone),
    )?;
    if read.calendar != "iso8601" && read.calendar != format.calendar {
        return Err(scope.range_error("calendar does not match the formatter's calendar"));
    }
    let instant = Temporal {
        kind: Kind::Instant,
        ..read
    };
    let parts = format_parts(scope, &format, &Input::Temporal(instant))?;
    Ok(scope.string(&joined(&parts)))
}

fn instant_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::Instant,
        Required::Any,
        Defaults::All,
    )
}

fn plain_date_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::PlainDate,
        Required::Date,
        Defaults::Date,
    )
}

fn plain_date_time_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::PlainDateTime,
        Required::Any,
        Defaults::All,
    )
}

fn plain_month_day_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::PlainMonthDay,
        Required::Date,
        Defaults::Date,
    )
}

fn plain_time_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::PlainTime,
        Required::Time,
        Defaults::Time,
    )
}

fn plain_year_month_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::PlainYearMonth,
        Required::Date,
        Defaults::Date,
    )
}

fn zoned_date_time_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    temporal_method(
        scope,
        this,
        args,
        Kind::ZonedDateTime,
        Required::Any,
        Defaults::All,
    )
}

const DURATION_UNITS: [&str; 10] = [
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

fn duration_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    getters: &[Value],
) -> Result<Value, Value> {
    let Some((sign, units)) = getters.split_first() else {
        return Err(scope.type_error("Temporal.Duration is unavailable"));
    };
    if scope.call(sign, this, &[]).is_err() {
        return Err(scope.type_error("toLocaleString called on an incompatible receiver"));
    }
    let record = scope.new_object();
    for (unit, getter) in DURATION_UNITS.iter().zip(units) {
        let value = scope.call(getter, this, &[])?;
        let key = scope.string(unit);
        scope.define_entry(&record, &key, value)?;
    }
    let constructor = crate::global_constructor(scope, "DurationFormat")?;
    let formatter = scope.construct(&constructor, &[arg(args, 0), arg(args, 1)])?;
    let format = scope.get(&formatter, "format")?;
    scope.call(&format, &formatter, &[record])
}

fn install_duration(scope: &mut Scope<'_>, temporal: &Value) {
    let Some(prototype) = prototype_of(scope, temporal, "Duration") else {
        return;
    };
    let mut getters = Vec::new();
    for name in std::iter::once("sign").chain(DURATION_UNITS) {
        let key = scope.string(name);
        match scope.own_property(&prototype, &key) {
            Ok(Some(descriptor)) if descriptor.accessor => getters.push(descriptor.getter),
            _ => return,
        }
    }
    let function = scope.bound_function("toLocaleString", 0, duration_to_locale_string, &getters);
    let _ = scope.define(&prototype, "toLocaleString", function, Attributes::METHOD);
}

const TEMPORAL_METHODS: [(&str, NativeFn); 7] = [
    ("Instant", instant_to_locale_string),
    ("PlainDate", plain_date_to_locale_string),
    ("PlainDateTime", plain_date_time_to_locale_string),
    ("PlainMonthDay", plain_month_day_to_locale_string),
    ("PlainTime", plain_time_to_locale_string),
    ("PlainYearMonth", plain_year_month_to_locale_string),
    ("ZonedDateTime", zoned_date_time_to_locale_string),
];

fn prototype_of(scope: &mut Scope<'_>, holder: &Value, name: &str) -> Option<Value> {
    let constructor = scope.get(holder, name).ok().filter(Value::is_object)?;
    scope
        .get(&constructor, "prototype")
        .ok()
        .filter(Value::is_object)
}

fn replace_method(scope: &mut Scope<'_>, prototype: &Value, name: &str, f: NativeFn, arity: u32) {
    let function = scope.function(name, arity, f);
    let _ = scope.define(prototype, name, function, Attributes::METHOD);
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    if let Ok(intl) = scope.get(global, "Intl")
        && let Some(constructor) = scope
            .get(&intl, "DateTimeFormat")
            .ok()
            .filter(Value::is_object)
    {
        if let Ok(prototype) = scope.get(&constructor, "prototype") {
            let getter = scope.function("get format", 0, format_getter);
            let _ = scope.define_accessor(
                &prototype,
                "format",
                Some(&getter),
                None,
                Attributes::CONFIGURABLE,
            );
        }
        replace_method(
            scope,
            &constructor,
            "supportedLocalesOf",
            supported_locales_of,
            1,
        );
    }
    if let Some(prototype) = prototype_of(scope, global, "Date") {
        replace_method(
            scope,
            &prototype,
            "toLocaleString",
            date_to_locale_string,
            0,
        );
        replace_method(
            scope,
            &prototype,
            "toLocaleDateString",
            date_to_locale_date_string,
            0,
        );
        replace_method(
            scope,
            &prototype,
            "toLocaleTimeString",
            date_to_locale_time_string,
            0,
        );
    }
    let Some(temporal) = scope.get(global, "Temporal").ok().filter(Value::is_object) else {
        return;
    };
    for (name, method) in TEMPORAL_METHODS {
        if let Some(prototype) = prototype_of(scope, &temporal, name) {
            replace_method(scope, &prototype, "toLocaleString", method, 0);
        }
    }
    install_duration(scope, &temporal);
}
