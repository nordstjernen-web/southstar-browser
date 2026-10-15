//! Southstar — Intl.RelativeTimeFormat: "in 3 days" and "yesterday" phrases from CLDR relative-time patterns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Trace, Value};

use super::service::{self, Options, Part};
use crate::{Method, arg};

pub(crate) const METHODS: [Method; 3] = [
    ("format", format, 2),
    ("formatToParts", format_to_parts, 2),
    ("resolvedOptions", resolved_options, 0),
];

const LANGUAGES: [&str; 3] = ["de", "en", "pl"];

const STYLES: [&str; 3] = ["long", "short", "narrow"];
const NUMERIC: [&str; 2] = ["always", "auto"];
const UNITS: [&str; 8] = [
    "second", "minute", "hour", "day", "week", "month", "quarter", "year",
];

#[derive(Clone)]
struct RelativeTimeFormat {
    locale: String,
    numbering_system: String,
    style: &'static str,
    numeric: &'static str,
    number_format: Value,
    plural_rules: Value,
}

impl Trace for RelativeTimeFormat {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.number_format);
        visit(&self.plural_rules);
    }
}

struct Forms {
    one: &'static str,
    few: &'static str,
    many: &'static str,
    other: &'static str,
}

impl Forms {
    fn pick(&self, category: &str) -> &'static str {
        match category {
            "one" => self.one,
            "few" => self.few,
            "many" => self.many,
            _ => self.other,
        }
    }
}

const fn same(word: &'static str) -> Forms {
    Forms {
        one: word,
        few: word,
        many: word,
        other: word,
    }
}

const fn one_other(one: &'static str, other: &'static str) -> Forms {
    Forms {
        one,
        few: other,
        many: other,
        other,
    }
}

const fn polish(
    one: &'static str,
    few: &'static str,
    many: &'static str,
    other: &'static str,
) -> Forms {
    Forms {
        one,
        few,
        many,
        other,
    }
}

struct UnitNames {
    forms: Forms,
    words: &'static [(i32, &'static str)],
}

const fn unit(forms: Forms, words: &'static [(i32, &'static str)]) -> UnitNames {
    UnitNames { forms, words }
}

struct Phrasing {
    future_prefix: &'static str,
    past_prefix: &'static str,
    past_suffix: &'static str,
}

const ENGLISH: Phrasing = Phrasing {
    future_prefix: "in ",
    past_prefix: "",
    past_suffix: " ago",
};

const GERMAN: Phrasing = Phrasing {
    future_prefix: "in ",
    past_prefix: "vor ",
    past_suffix: "",
};

const POLISH: Phrasing = Phrasing {
    future_prefix: "za ",
    past_prefix: "",
    past_suffix: " temu",
};

const EN_DAY: &[(i32, &str)] = &[(-1, "yesterday"), (0, "today"), (1, "tomorrow")];

const EN_LONG: [UnitNames; 8] = [
    unit(one_other("second", "seconds"), &[(0, "now")]),
    unit(one_other("minute", "minutes"), &[(0, "this minute")]),
    unit(one_other("hour", "hours"), &[(0, "this hour")]),
    unit(one_other("day", "days"), EN_DAY),
    unit(
        one_other("week", "weeks"),
        &[(-1, "last week"), (0, "this week"), (1, "next week")],
    ),
    unit(
        one_other("month", "months"),
        &[(-1, "last month"), (0, "this month"), (1, "next month")],
    ),
    unit(
        one_other("quarter", "quarters"),
        &[
            (-1, "last quarter"),
            (0, "this quarter"),
            (1, "next quarter"),
        ],
    ),
    unit(
        one_other("year", "years"),
        &[(-1, "last year"), (0, "this year"), (1, "next year")],
    ),
];

const EN_SHORT: [UnitNames; 8] = [
    unit(same("sec."), &[(0, "now")]),
    unit(same("min."), &[(0, "this minute")]),
    unit(same("hr."), &[(0, "this hour")]),
    unit(one_other("day", "days"), EN_DAY),
    unit(
        same("wk."),
        &[(-1, "last wk."), (0, "this wk."), (1, "next wk.")],
    ),
    unit(
        same("mo."),
        &[(-1, "last mo."), (0, "this mo."), (1, "next mo.")],
    ),
    unit(
        one_other("qtr.", "qtrs."),
        &[(-1, "last qtr."), (0, "this qtr."), (1, "next qtr.")],
    ),
    unit(
        same("yr."),
        &[(-1, "last yr."), (0, "this yr."), (1, "next yr.")],
    ),
];

const DE_DAY: &[(i32, &str)] = &[
    (-2, "vorgestern"),
    (-1, "gestern"),
    (0, "heute"),
    (1, "morgen"),
    (2, "übermorgen"),
];
const DE_WEEK: &[(i32, &str)] = &[
    (-1, "letzte Woche"),
    (0, "diese Woche"),
    (1, "nächste Woche"),
];
const DE_MONTH: &[(i32, &str)] = &[
    (-1, "letzten Monat"),
    (0, "diesen Monat"),
    (1, "nächsten Monat"),
];
const DE_QUARTER: &[(i32, &str)] = &[
    (-1, "letztes Quartal"),
    (0, "dieses Quartal"),
    (1, "nächstes Quartal"),
];
const DE_YEAR: &[(i32, &str)] = &[
    (-1, "letztes Jahr"),
    (0, "dieses Jahr"),
    (1, "nächstes Jahr"),
];
const DE_NOW: &[(i32, &str)] = &[(0, "jetzt")];
const DE_THIS_MINUTE: &[(i32, &str)] = &[(0, "in dieser Minute")];
const DE_THIS_HOUR: &[(i32, &str)] = &[(0, "in dieser Stunde")];
const DE_DAY_FORMS: Forms = one_other("Tag", "Tagen");
const DE_WEEK_FORMS: Forms = one_other("Woche", "Wochen");
const DE_MONTH_FORMS: Forms = one_other("Monat", "Monaten");
const DE_YEAR_FORMS: Forms = one_other("Jahr", "Jahren");

const DE_LONG: [UnitNames; 8] = [
    unit(one_other("Sekunde", "Sekunden"), DE_NOW),
    unit(one_other("Minute", "Minuten"), DE_THIS_MINUTE),
    unit(one_other("Stunde", "Stunden"), DE_THIS_HOUR),
    unit(DE_DAY_FORMS, DE_DAY),
    unit(DE_WEEK_FORMS, DE_WEEK),
    unit(DE_MONTH_FORMS, DE_MONTH),
    unit(one_other("Quartal", "Quartalen"), DE_QUARTER),
    unit(DE_YEAR_FORMS, DE_YEAR),
];

const DE_SHORT: [UnitNames; 8] = [
    unit(same("Sek."), DE_NOW),
    unit(same("Min."), DE_THIS_MINUTE),
    unit(same("Std."), DE_THIS_HOUR),
    unit(DE_DAY_FORMS, DE_DAY),
    unit(DE_WEEK_FORMS, DE_WEEK),
    unit(DE_MONTH_FORMS, DE_MONTH),
    unit(same("Quart."), DE_QUARTER),
    unit(DE_YEAR_FORMS, DE_YEAR),
];

const DE_NARROW: [UnitNames; 8] = [
    unit(same("s"), DE_NOW),
    unit(same("m"), DE_THIS_MINUTE),
    unit(same("Std."), DE_THIS_HOUR),
    unit(DE_DAY_FORMS, DE_DAY),
    unit(same("Wo."), DE_WEEK),
    unit(DE_MONTH_FORMS, DE_MONTH),
    unit(same("Q"), DE_QUARTER),
    unit(DE_YEAR_FORMS, DE_YEAR),
];

const PL_DAY: &[(i32, &str)] = &[
    (-2, "przedwczoraj"),
    (-1, "wczoraj"),
    (0, "dzisiaj"),
    (1, "jutro"),
    (2, "pojutrze"),
];
const PL_DAY_FORMS: Forms = polish("dzień", "dni", "dni", "dnia");
const PL_YEAR_FORMS: Forms = polish("rok", "lata", "lat", "roku");
const PL_WEEK_FORMS: Forms = polish("tydz.", "tyg.", "tyg.", "tyg.");

const PL_LONG: [UnitNames; 8] = [
    unit(
        polish("sekundę", "sekundy", "sekund", "sekundy"),
        &[(0, "teraz")],
    ),
    unit(polish("minutę", "minuty", "minut", "minuty"), &[]),
    unit(polish("godzinę", "godziny", "godzin", "godziny"), &[]),
    unit(PL_DAY_FORMS, PL_DAY),
    unit(polish("tydzień", "tygodnie", "tygodni", "tygodnia"), &[]),
    unit(polish("miesiąc", "miesiące", "miesięcy", "miesiąca"), &[]),
    unit(polish("kwartał", "kwartały", "kwartałów", "kwartału"), &[]),
    unit(PL_YEAR_FORMS, &[]),
];

const PL_SHORT: [UnitNames; 8] = [
    unit(same("sek."), &[(0, "teraz")]),
    unit(same("min"), &[]),
    unit(same("godz."), &[]),
    unit(PL_DAY_FORMS, PL_DAY),
    unit(PL_WEEK_FORMS, &[]),
    unit(same("mies."), &[]),
    unit(same("kw."), &[]),
    unit(PL_YEAR_FORMS, &[]),
];

const PL_NARROW: [UnitNames; 8] = [
    unit(same("s"), &[(0, "teraz")]),
    unit(same("min"), &[]),
    unit(same("g."), &[]),
    unit(PL_DAY_FORMS, PL_DAY),
    unit(PL_WEEK_FORMS, &[]),
    unit(same("mies."), &[]),
    unit(same("kw."), &[]),
    unit(PL_YEAR_FORMS, &[]),
];

fn locale_data(locale: &str, style: &str) -> (&'static Phrasing, &'static [UnitNames; 8]) {
    match (service::language_of(locale), style) {
        ("de", "long") => (&GERMAN, &DE_LONG),
        ("de", "short") => (&GERMAN, &DE_SHORT),
        ("de", _) => (&GERMAN, &DE_NARROW),
        ("pl", "long") => (&POLISH, &PL_LONG),
        ("pl", "short") => (&POLISH, &PL_SHORT),
        ("pl", _) => (&POLISH, &PL_NARROW),
        (_, "long") => (&ENGLISH, &EN_LONG),
        _ => (&ENGLISH, &EN_SHORT),
    }
}

fn singular_unit(unit: &str) -> Option<usize> {
    UNITS
        .iter()
        .position(|&known| known == unit || unit.strip_suffix('s') == Some(known))
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::require_new_target(scope, this, "RelativeTimeFormat")?;
    let prototype = service::prototype_from(scope, this, "RelativeTimeFormat")?;
    let requested = service::requested_locales(scope, &arg(args, 0))?;
    let options = Options::coerced(scope, &arg(args, 1))?;
    options.locale_matcher(scope)?;
    let numbering = options.numbering_system(scope)?;
    let (locale, numbering_system) =
        service::resolve_numbering(scope, &requested, &LANGUAGES, numbering.as_deref());
    let style = options.choice_or(scope, "style", &STYLES, "long")?;
    let numeric = options.choice_or(scope, "numeric", &NUMERIC, "always")?;
    let locale_value = scope.string(&locale);
    let numbering_value = scope.string(&numbering_system);
    let number_options = service::plain_object(scope, &[("numberingSystem", numbering_value)]);
    let number_format = service::construct_intl(
        scope,
        "NumberFormat",
        &[locale_value.clone(), number_options],
    )?;
    let plural_rules = service::construct_intl(scope, "PluralRules", &[locale_value])?;
    let format = RelativeTimeFormat {
        locale,
        numbering_system,
        style,
        numeric,
        number_format,
        plural_rules,
    };
    Ok(scope.new_traced_host_object(Some(&prototype), format))
}

pub(crate) fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::supported_locales(scope, args, &LANGUAGES)
}

fn named_offset(value: f64) -> Option<i32> {
    (value.fract() == 0.0 && value.abs() <= 2.0).then_some(value as i32)
}

fn plural_category(scope: &mut Scope<'_>, rules: &Value, value: &Value) -> Result<String, Value> {
    let select = scope.get(rules, "select")?;
    let category = scope.call(&select, rules, core::slice::from_ref(value))?;
    service::text_of(scope, &category)
}

fn partition(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> Result<Vec<Part>, Value> {
    let format: RelativeTimeFormat = service::receiver(scope, this, method)?;
    let value = scope.to_number(&arg(args, 0))?;
    let unit_name = service::text_of(scope, &arg(args, 1))?;
    if !value.is_finite() {
        return Err(scope.range_error("Intl.RelativeTimeFormat value must be finite"));
    }
    let Some(index) = singular_unit(&unit_name) else {
        return Err(scope.range_error(&format!("Invalid unit argument {unit_name}")));
    };
    let unit = UNITS[index];
    let (phrasing, units) = locale_data(&format.locale, format.style);
    let names = &units[index];
    if format.numeric == "auto"
        && let Some(offset) = named_offset(value)
        && let Some(&(_, word)) = names.words.iter().find(|&&(at, _)| at == offset)
    {
        return Ok(vec![Part::literal(word)]);
    }
    let past = value < 0.0 || (value == 0.0 && value.is_sign_negative());
    let magnitude = Value::number(value.abs());
    let number = service::number_parts(scope, &format.number_format, &magnitude)?;
    let category = plural_category(scope, &format.plural_rules, &magnitude)?;
    let word = names.forms.pick(&category);
    let mut parts = Vec::with_capacity(number.len() + 2);
    let prefix = if past {
        phrasing.past_prefix
    } else {
        phrasing.future_prefix
    };
    if !prefix.is_empty() {
        parts.push(Part::literal(prefix));
    }
    parts.extend(number.into_iter().map(|part| part.with_unit(unit)));
    let suffix = if past { phrasing.past_suffix } else { "" };
    parts.push(Part::literal(format!(" {word}{suffix}")));
    Ok(parts)
}

fn format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = partition(
        scope,
        this,
        args,
        "Intl.RelativeTimeFormat.prototype.format",
    )?;
    Ok(service::joined(scope, &parts))
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let parts = partition(
        scope,
        this,
        args,
        "Intl.RelativeTimeFormat.prototype.formatToParts",
    )?;
    Ok(service::parts_array(scope, &parts))
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let format: RelativeTimeFormat = service::receiver(
        scope,
        this,
        "Intl.RelativeTimeFormat.prototype.resolvedOptions",
    )?;
    let options = scope.new_object();
    service::text_property(scope, &options, "locale", &format.locale);
    service::text_property(scope, &options, "style", format.style);
    service::text_property(scope, &options, "numeric", format.numeric);
    service::text_property(scope, &options, "numberingSystem", &format.numbering_system);
    Ok(options)
}
