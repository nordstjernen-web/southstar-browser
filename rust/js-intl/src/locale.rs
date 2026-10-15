//! Southstar — Intl.Locale with its locale info methods, Intl.getCanonicalLocales and Intl.supportedValuesOf.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, BoundFn, NativeFn, Scope, Value};

use crate::negotiate::{self, Options, create_data_property, string_array};
use crate::tag::{self, LanguageId, LocaleId};
use crate::{arg, bind, cldr_index, collator};

#[derive(Clone)]
pub(crate) struct LocaleSlots {
    pub(crate) locale: String,
    id: LanguageId,
    calendar: Option<String>,
    collation: Option<String>,
    first_day_of_week: Option<String>,
    hour_cycle: Option<String>,
    case_first: Option<String>,
    numeric: bool,
    numbering_system: Option<String>,
}

type Getter = fn(&LocaleSlots) -> Option<String>;

const RELEVANT_KEYS: [&str; 7] = ["ca", "co", "fw", "hc", "kf", "kn", "nu"];

const CALENDARS: [&str; 16] = [
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
const CURRENCIES: [&str; 18] = [
    "AUD", "BRL", "CAD", "CHF", "CNY", "EUR", "GBP", "HKD", "INR", "JPY", "KRW", "MXN", "NOK",
    "NZD", "RUB", "SEK", "USD", "ZAR",
];

const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

const GETTERS: [(&str, Getter); 11] = [
    ("baseName", |slots| Some(slots.id.to_text())),
    ("calendar", |slots| slots.calendar.clone()),
    ("caseFirst", |slots| slots.case_first.clone()),
    ("collation", |slots| slots.collation.clone()),
    ("firstDayOfWeek", |slots| slots.first_day_of_week.clone()),
    ("hourCycle", |slots| slots.hour_cycle.clone()),
    ("numberingSystem", |slots| slots.numbering_system.clone()),
    ("language", |slots| Some(slots.id.language.clone())),
    ("script", |slots| slots.id.script.clone()),
    ("region", |slots| slots.id.region.clone()),
    ("variants", |slots| {
        (!slots.id.variants.is_empty()).then(|| slots.id.variants.join("-"))
    }),
];

const METHODS: [(&str, BoundFn, u32); 9] = [
    ("maximize", maximize, 0),
    ("minimize", minimize, 0),
    ("getCalendars", get_calendars, 0),
    ("getCollations", get_collations, 0),
    ("getHourCycles", get_hour_cycles, 0),
    ("getNumberingSystems", get_numbering_systems, 0),
    ("getTimeZones", get_time_zones, 0),
    ("getTextInfo", get_text_info, 0),
    ("getWeekInfo", get_week_info, 0),
];

fn slots(scope: &mut Scope<'_>, this: &Value, method: &str) -> Result<LocaleSlots, Value> {
    this.with_host(|slots: &LocaleSlots| slots.clone())
        .ok_or_else(|| {
            scope.type_error(&format!(
                "Intl.Locale.prototype.{method} called on an incompatible receiver"
            ))
        })
}

fn optional_text(scope: &mut Scope<'_>, value: Option<String>) -> Value {
    value.map_or_else(Value::undefined, |text| scope.string(&text))
}

fn getter(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let index = scope.to_int32(&arg(data, 0))? as usize;
    let (name, read) = GETTERS[index];
    let slots = slots(scope, this, name)?;
    let value = read(&slots);
    Ok(optional_text(scope, value))
}

fn numeric_getter(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let slots = slots(scope, this, "numeric")?;
    Ok(Value::boolean(slots.numeric))
}

fn weekday_to_string(value: &str) -> String {
    match value {
        "0" | "7" => "sun".to_owned(),
        "1" => "mon".to_owned(),
        "2" => "tue".to_owned(),
        "3" => "wed".to_owned(),
        "4" => "thu".to_owned(),
        "5" => "fri".to_owned(),
        "6" => "sat".to_owned(),
        _ => value.to_owned(),
    }
}

fn string_to_weekday(value: &str) -> Option<i32> {
    WEEKDAYS
        .iter()
        .position(|day| *day == value)
        .map(|index| index as i32 + 1)
}

fn checked_type(
    scope: &mut Scope<'_>,
    value: Option<String>,
    name: &str,
) -> Result<Option<String>, Value> {
    match value {
        Some(text) if !tag::is_type_value(&text) => {
            Err(scope.range_error(&format!("Invalid {name}: {text}")))
        }
        other => Ok(other),
    }
}

fn checked_subtag(
    scope: &mut Scope<'_>,
    value: Option<String>,
    name: &str,
    valid: fn(&str) -> bool,
) -> Result<Option<String>, Value> {
    match value {
        Some(text) if !valid(&text) => Err(scope.range_error(&format!("Invalid {name}: {text}"))),
        other => Ok(other),
    }
}

fn option_or(
    options: &Options,
    scope: &mut Scope<'_>,
    key: &str,
    default: Option<String>,
) -> Result<Option<String>, Value> {
    Ok(options.string(scope, key, &[])?.or(default))
}

fn update_language_id(
    scope: &mut Scope<'_>,
    locale: &LocaleId,
    options: &Options,
) -> Result<LocaleId, Value> {
    let base = &locale.id;
    let language = option_or(options, scope, "language", Some(base.language.clone()))?;
    let language = checked_subtag(scope, language, "language", tag::is_language_subtag)?;
    let script = option_or(options, scope, "script", base.script.clone())?;
    let script = checked_subtag(scope, script, "script", tag::is_script_subtag)?;
    let region = option_or(options, scope, "region", base.region.clone())?;
    let region = checked_subtag(scope, region, "region", tag::is_region_subtag)?;
    let joined = (!base.variants.is_empty()).then(|| base.variants.join("-"));
    let variants = option_or(options, scope, "variants", joined)?;
    let variants = match variants {
        Some(text) => {
            let lower = text.to_ascii_lowercase();
            let parts: Vec<String> = lower.split('-').map(str::to_owned).collect();
            let duplicate = parts
                .iter()
                .enumerate()
                .any(|(i, v)| parts[..i].contains(v));
            if text.is_empty() || duplicate || !parts.iter().all(|v| tag::is_variant_subtag(v)) {
                return Err(scope.range_error(&format!("Invalid variants: {text}")));
            }
            parts
        }
        None => Vec::new(),
    };
    let mut updated = locale.clone();
    updated.id = LanguageId {
        language: language.unwrap_or_default().to_ascii_lowercase(),
        script: script.as_deref().map(tag::title_case),
        region: region.map(|r| r.to_ascii_uppercase()),
        variants,
    };
    Ok(updated)
}

type Keywords = [Option<String>; 7];

fn read_keyword_options(scope: &mut Scope<'_>, options: &Options) -> Result<Keywords, Value> {
    let calendar = options.string(scope, "calendar", &[])?;
    let calendar = checked_type(scope, calendar, "calendar")?;
    let collation = options.string(scope, "collation", &[])?;
    let collation = checked_type(scope, collation, "collation")?;
    let first_day = options.string(scope, "firstDayOfWeek", &[])?;
    let first_day = checked_type(
        scope,
        first_day.map(|day| weekday_to_string(&day)),
        "firstDayOfWeek",
    )?;
    let hour_cycle = options.string(scope, "hourCycle", &["h11", "h12", "h23", "h24"])?;
    let case_first = options.string(scope, "caseFirst", &["upper", "lower", "false"])?;
    let numeric = options
        .boolean(scope, "numeric")?
        .map(|numeric| numeric.to_string());
    let numbering = options.string(scope, "numberingSystem", &[])?;
    let numbering = checked_type(scope, numbering, "numberingSystem")?;
    Ok([
        calendar, collation, first_day, hour_cycle, case_first, numeric, numbering,
    ])
}

fn override_keyword(locale: &mut LocaleId, key: &str, value: &str) -> String {
    let canonical = tag::canonical_unicode_value(key, value);
    match locale.keywords.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1.clone_from(&canonical),
        None => locale.keywords.push((key.to_owned(), canonical.clone())),
    }
    canonical
}

fn make_locale_record(mut locale: LocaleId, overrides: &Keywords) -> LocaleSlots {
    let values: Keywords = core::array::from_fn(|index| {
        let key = RELEVANT_KEYS[index];
        match &overrides[index] {
            Some(value) => Some(override_keyword(&mut locale, key, value)),
            None => locale.keyword(key).map(str::to_owned),
        }
    });
    locale.canonicalize();
    let [
        calendar,
        collation,
        first_day_of_week,
        hour_cycle,
        case_first,
        numeric,
        numbering_system,
    ] = values;
    LocaleSlots {
        locale: locale.to_text(),
        id: locale.id.clone(),
        calendar,
        collation,
        first_day_of_week,
        hour_cycle,
        case_first,
        numeric: matches!(numeric.as_deref(), Some("true" | "")),
        numbering_system,
    }
}

fn create(
    scope: &mut Scope<'_>,
    proto: &Value,
    tag: &Value,
    options: &Value,
) -> Result<Value, Value> {
    if !tag.is_string() && !tag.is_object() {
        return Err(
            scope.type_error("First argument to Intl.Locale must be a string or Locale object")
        );
    }
    let text = match negotiate::locale_slot(tag) {
        Some(locale) => locale,
        None => scope.to_string(tag)?,
    };
    let options = Options::coerce(scope, options)?;
    let Some(mut locale) = LocaleId::parse(&text) else {
        return Err(scope.range_error(&format!("Incorrect locale information provided: {text}")));
    };
    locale.canonicalize();
    let updated = update_language_id(scope, &locale, &options)?;
    let overrides = read_keyword_options(scope, &options)?;
    let record = make_locale_record(updated, &overrides);
    Ok(scope.new_host_object(Some(proto), record))
}

fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    if this.is_undefined() {
        return Err(scope.type_error("Constructor Intl.Locale requires 'new'"));
    }
    let proto = negotiate::prototype_from_constructor(scope, this, "Locale", &arg(data, 0))?;
    create(scope, &proto, &arg(args, 0), &arg(args, 1))
}

fn rebuild(
    scope: &mut Scope<'_>,
    current: &LocaleSlots,
    id: LanguageId,
    data: &[Value],
) -> Result<Value, Value> {
    let mut locale = LocaleId::parse(&current.locale).unwrap_or_default();
    locale.id = id;
    let text = scope.string(&locale.to_text());
    create(scope, &arg(data, 0), &text, &Value::undefined())
}

fn maximize(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let current = slots(scope, this, "maximize")?;
    let maximal = tag::maximize(&current.id).unwrap_or_else(|| current.id.clone());
    rebuild(scope, &current, maximal, data)
}

fn minimize(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let current = slots(scope, this, "minimize")?;
    let minimal = tag::minimize(&current.id);
    rebuild(scope, &current, minimal, data)
}

fn keyword_region(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let region = if bytes.len() >= 3 && bytes[..3].iter().all(u8::is_ascii_digit) {
        &value[..3]
    } else if bytes.len() >= 2 && bytes[..2].iter().all(u8::is_ascii_alphabetic) {
        &value[..2]
    } else {
        return None;
    };
    Some(region.to_ascii_uppercase())
}

fn region_preference(slots: &LocaleSlots) -> String {
    let locale = LocaleId::parse(&slots.locale).unwrap_or_default();
    if let Some(region) = locale
        .keyword("rg")
        .filter(|value| value.len() == 6)
        .and_then(keyword_region)
    {
        return region;
    }
    if let Some(region) = &slots.id.region {
        return region.clone();
    }
    if let Some(region) = locale.keyword("sd").and_then(keyword_region) {
        return region;
    }
    tag::maximize(&slots.id)
        .and_then(|maximal| maximal.region)
        .unwrap_or_else(|| "001".to_owned())
}

fn get_calendars(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getCalendars")?;
    if let Some(calendar) = &slots.calendar {
        return string_array(scope, &[calendar]);
    }
    let region = region_preference(&slots);
    let calendars = cldr_index::calendars(&region)
        .or_else(|| cldr_index::calendars("001"))
        .unwrap_or("gregory");
    let list: Vec<&str> = calendars.split(',').collect();
    string_array(scope, &list)
}

fn get_collations(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getCollations")?;
    if let Some(collation) = &slots.collation {
        return string_array(scope, &[collation]);
    }
    let mut list = collator::collations_for(&slots.id.language);
    list.extend(["emoji", "eor"]);
    list.sort_unstable();
    string_array(scope, &list)
}

fn get_hour_cycles(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getHourCycles")?;
    if let Some(cycle) = &slots.hour_cycle {
        return string_array(scope, &[cycle]);
    }
    let region = region_preference(&slots);
    let language_region = format!("{}-{region}", slots.id.language);
    let cycle = cldr_index::hour_cycle(&language_region)
        .or_else(|| cldr_index::hour_cycle(&region))
        .unwrap_or("h23");
    string_array(scope, &[cycle])
}

fn get_numbering_systems(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getNumberingSystems")?;
    let system = slots.numbering_system.as_deref().unwrap_or("latn");
    string_array(scope, &[system])
}

fn get_time_zones(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getTimeZones")?;
    let Some(region) = &slots.id.region else {
        return Ok(Value::undefined());
    };
    let zones = cldr_index::time_zones(region).unwrap_or(&[]);
    string_array(scope, zones)
}

fn direction(id: &LanguageId) -> Option<&'static str> {
    let script = match &id.script {
        Some(script) => script.clone(),
        None => tag::maximize(id)?.script?,
    };
    if cldr_index::is_rtl_script(&script) {
        Some("rtl")
    } else if cldr_index::is_ltr_script(&script) {
        Some("ltr")
    } else {
        None
    }
}

fn get_text_info(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getTextInfo")?;
    let info = scope.new_object();
    let value = optional_text(scope, direction(&slots.id).map(str::to_owned));
    create_data_property(scope, &info, "direction", value)?;
    Ok(info)
}

fn weekend_days(region: &str) -> Vec<i32> {
    let range = cldr_index::weekend(region).unwrap_or("sat-sun");
    let (start, end) = range.split_once('-').unwrap_or(("sat", "sun"));
    let start = string_to_weekday(start).unwrap_or(6);
    let end = string_to_weekday(end).unwrap_or(7);
    let mut days = Vec::new();
    let mut day = start;
    loop {
        days.push(day);
        if day == end || days.len() == 7 {
            break;
        }
        day = day % 7 + 1;
    }
    days.sort_unstable();
    days
}

fn get_week_info(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let slots = slots(scope, this, "getWeekInfo")?;
    let region = region_preference(&slots);
    let first_day = slots
        .first_day_of_week
        .as_deref()
        .and_then(string_to_weekday)
        .or_else(|| cldr_index::first_day(&region).and_then(string_to_weekday))
        .unwrap_or(1);
    let info = scope.new_object();
    create_data_property(scope, &info, "firstDay", Value::int(first_day))?;
    let weekend = scope.new_array();
    for (index, day) in weekend_days(&region).into_iter().enumerate() {
        scope.define_entry(&weekend, &Value::int(index as i32), Value::int(day))?;
    }
    create_data_property(scope, &info, "weekend", weekend)?;
    Ok(info)
}

fn to_string(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let slots = slots(scope, this, "toString")?;
    Ok(scope.string(&slots.locale))
}

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value) {
    let proto = scope.new_object();
    let constructor =
        scope.bound_constructor("Locale", 1, constructor, std::slice::from_ref(&proto));
    let _ = scope.set_constructor(&constructor, &proto);
    bind(scope, &proto, "toString", to_string as NativeFn, 0);
    for (name, f, arity) in METHODS {
        let method = scope.bound_function(name, arity, f, std::slice::from_ref(&proto));
        let _ = scope.define(&proto, name, method, Attributes::METHOD);
    }
    for (index, (name, _)) in GETTERS.iter().enumerate() {
        let function = scope.bound_function(
            &format!("get {name}"),
            0,
            getter,
            &[Value::int(index as i32)],
        );
        let _ = scope.define_accessor(
            &proto,
            name,
            Some(&function),
            None,
            Attributes::CONFIGURABLE,
        );
    }
    let numeric = scope.function("get numeric", 0, numeric_getter);
    let _ = scope.define_accessor(
        &proto,
        "numeric",
        Some(&numeric),
        None,
        Attributes::CONFIGURABLE,
    );
    let _ = scope.define_to_string_tag(&proto, "Intl.Locale");
    let _ = scope.define(intl, "Locale", constructor, Attributes::METHOD);
}

pub(crate) fn get_canonical_locales(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let locales = negotiate::canonicalize_locale_list(scope, &arg(args, 0))?;
    let list: Vec<&str> = locales.iter().map(String::as_str).collect();
    string_array(scope, &list)
}

pub(crate) fn supported_values_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let key = scope.to_string(&arg(args, 0))?;
    let computed;
    let listed: Vec<&str>;
    let values: &[&str] = match key.as_str() {
        "calendar" => &CALENDARS,
        "collation" => {
            computed = collator::available_collations();
            &computed
        }
        "currency" => &CURRENCIES,
        "numberingSystem" => {
            listed = crate::number::data::numbering_systems().collect();
            &listed
        }
        "timeZone" => {
            computed = cldr_index::all_time_zones();
            &computed
        }
        "unit" => {
            listed = crate::number::data::simple_units().collect();
            &listed
        }
        _ => return Err(scope.range_error(&format!("Invalid key: {key}"))),
    };
    string_array(scope, values)
}
