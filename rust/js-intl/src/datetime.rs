//! Southstar — Intl.DateTimeFormat: options, calendar names for a few languages, field order and the local time zone.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

use crate::{
    Method, Parts, arg, arg_locale, bind_bound, ffi, hget_str, hide, hide_text, join_parts,
    join_range, lang_in, lang_subtag, new_instance, number_arg, opt_bool, opt_present, opt_str,
    set, set_text,
};

pub(crate) const METHODS: [Method; 5] = [
    ("format", format, 1),
    ("formatToParts", format_to_parts, 1),
    ("formatRange", format_range, 2),
    ("formatRangeToParts", format_to_parts, 2),
    ("resolvedOptions", resolved, 0),
];

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

struct CalendarNames {
    lang: &'static [u8],
    months: [&'static str; 12],
    days: [&'static str; 7],
}

const CALENDARS: [CalendarNames; 14] = [
    CalendarNames {
        lang: b"nb",
        months: [
            "januar",
            "februar",
            "mars",
            "april",
            "mai",
            "juni",
            "juli",
            "august",
            "september",
            "oktober",
            "november",
            "desember",
        ],
        days: [
            "søndag", "mandag", "tirsdag", "onsdag", "torsdag", "fredag", "lørdag",
        ],
    },
    CalendarNames {
        lang: b"nn",
        months: [
            "januar",
            "februar",
            "mars",
            "april",
            "mai",
            "juni",
            "juli",
            "august",
            "september",
            "oktober",
            "november",
            "desember",
        ],
        days: [
            "sundag", "måndag", "tysdag", "onsdag", "torsdag", "fredag", "laurdag",
        ],
    },
    CalendarNames {
        lang: b"da",
        months: [
            "januar",
            "februar",
            "marts",
            "april",
            "maj",
            "juni",
            "juli",
            "august",
            "september",
            "oktober",
            "november",
            "december",
        ],
        days: [
            "søndag", "mandag", "tirsdag", "onsdag", "torsdag", "fredag", "lørdag",
        ],
    },
    CalendarNames {
        lang: b"sv",
        months: [
            "januari",
            "februari",
            "mars",
            "april",
            "maj",
            "juni",
            "juli",
            "augusti",
            "september",
            "oktober",
            "november",
            "december",
        ],
        days: [
            "söndag", "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag",
        ],
    },
    CalendarNames {
        lang: b"fi",
        months: [
            "tammikuuta",
            "helmikuuta",
            "maaliskuuta",
            "huhtikuuta",
            "toukokuuta",
            "kesäkuuta",
            "heinäkuuta",
            "elokuuta",
            "syyskuuta",
            "lokakuuta",
            "marraskuuta",
            "joulukuuta",
        ],
        days: [
            "sunnuntaina",
            "maanantaina",
            "tiistaina",
            "keskiviikkona",
            "torstaina",
            "perjantaina",
            "lauantaina",
        ],
    },
    CalendarNames {
        lang: b"is",
        months: [
            "janúar",
            "febrúar",
            "mars",
            "apríl",
            "maí",
            "júní",
            "júlí",
            "ágúst",
            "september",
            "október",
            "nóvember",
            "desember",
        ],
        days: [
            "sunnudagur",
            "mánudagur",
            "þriðjudagur",
            "miðvikudagur",
            "fimmtudagur",
            "föstudagur",
            "laugardagur",
        ],
    },
    CalendarNames {
        lang: b"de",
        months: [
            "Januar",
            "Februar",
            "März",
            "April",
            "Mai",
            "Juni",
            "Juli",
            "August",
            "September",
            "Oktober",
            "November",
            "Dezember",
        ],
        days: [
            "Sonntag",
            "Montag",
            "Dienstag",
            "Mittwoch",
            "Donnerstag",
            "Freitag",
            "Samstag",
        ],
    },
    CalendarNames {
        lang: b"nl",
        months: [
            "januari",
            "februari",
            "maart",
            "april",
            "mei",
            "juni",
            "juli",
            "augustus",
            "september",
            "oktober",
            "november",
            "december",
        ],
        days: [
            "zondag",
            "maandag",
            "dinsdag",
            "woensdag",
            "donderdag",
            "vrijdag",
            "zaterdag",
        ],
    },
    CalendarNames {
        lang: b"fr",
        months: [
            "janvier",
            "février",
            "mars",
            "avril",
            "mai",
            "juin",
            "juillet",
            "août",
            "septembre",
            "octobre",
            "novembre",
            "décembre",
        ],
        days: [
            "dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi",
        ],
    },
    CalendarNames {
        lang: b"es",
        months: [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ],
        days: [
            "domingo",
            "lunes",
            "martes",
            "miércoles",
            "jueves",
            "viernes",
            "sábado",
        ],
    },
    CalendarNames {
        lang: b"it",
        months: [
            "gennaio",
            "febbraio",
            "marzo",
            "aprile",
            "maggio",
            "giugno",
            "luglio",
            "agosto",
            "settembre",
            "ottobre",
            "novembre",
            "dicembre",
        ],
        days: [
            "domenica",
            "lunedì",
            "martedì",
            "mercoledì",
            "giovedì",
            "venerdì",
            "sabato",
        ],
    },
    CalendarNames {
        lang: b"pt",
        months: [
            "janeiro",
            "fevereiro",
            "março",
            "abril",
            "maio",
            "junho",
            "julho",
            "agosto",
            "setembro",
            "outubro",
            "novembro",
            "dezembro",
        ],
        days: [
            "domingo",
            "segunda-feira",
            "terça-feira",
            "quarta-feira",
            "quinta-feira",
            "sexta-feira",
            "sábado",
        ],
    },
    CalendarNames {
        lang: b"pl",
        months: [
            "stycznia",
            "lutego",
            "marca",
            "kwietnia",
            "maja",
            "czerwca",
            "lipca",
            "sierpnia",
            "września",
            "października",
            "listopada",
            "grudnia",
        ],
        days: [
            "niedziela",
            "poniedziałek",
            "wtorek",
            "środa",
            "czwartek",
            "piątek",
            "sobota",
        ],
    },
    CalendarNames {
        lang: "ru".as_bytes(),
        months: [
            "января",
            "февраля",
            "марта",
            "апреля",
            "мая",
            "июня",
            "июля",
            "августа",
            "сентября",
            "октября",
            "ноября",
            "декабря",
        ],
        days: [
            "воскресенье",
            "понедельник",
            "вторник",
            "среда",
            "четверг",
            "пятница",
            "суббота",
        ],
    },
];

const STYLE_KEYS: [&str; 11] = [
    "weekday",
    "era",
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "hour12",
    "hourCycle",
    "dayPeriod",
];
const FIELD_KEYS: [&str; 7] = [
    "weekday", "year", "month", "day", "hour", "minute", "second",
];
const H12_LANGS: [&[u8]; 13] = [
    b"en", b"ko", b"hi", b"bn", b"ta", b"te", b"ur", b"fil", b"tl", b"am", b"sw", b"ms", b"ar",
];
const H23_REGIONS: [&[u8]; 3] = [b"gb", b"ie", b"za"];
const DMY: [&[u8]; 15] = [
    b"de", b"nb", b"nn", b"no", b"da", b"fi", b"is", b"fr", b"es", b"it", b"nl", b"pt", b"ru",
    b"pl", b"sv",
];
const DOT_SEPARATED: [&[u8]; 11] = [
    b"de", b"nb", b"nn", b"no", b"da", b"fi", b"is", b"ru", b"pl", b"cs", b"tr",
];
const WEEKDAY_COMMA: [&[u8]; 6] = [b"en", b"de", b"es", b"pt", b"pl", b"ru"];
const DAY_DOT: [&[u8]; 7] = [b"nb", b"nn", b"no", b"da", b"de", b"fi", b"is"];
const DAY_DE: [&[u8]; 2] = [b"es", b"pt"];

#[cfg(windows)]
const WEST_EUROPE: [(&[u8], &[u8]); 10] = [
    (b"NO", b"Europe/Oslo"),
    (b"SE", b"Europe/Stockholm"),
    (b"DK", b"Europe/Copenhagen"),
    (b"DE", b"Europe/Berlin"),
    (b"NL", b"Europe/Amsterdam"),
    (b"BE", b"Europe/Brussels"),
    (b"AT", b"Europe/Vienna"),
    (b"CH", b"Europe/Zurich"),
    (b"IT", b"Europe/Rome"),
    (b"ES", b"Europe/Madrid"),
];

#[cfg(windows)]
const WINDOWS_ZONES: [(&[u8], &[u8]); 26] = [
    (b"UTC", b"UTC"),
    (b"GMT Standard Time", b"Europe/London"),
    (b"W. Europe Standard Time", b"Europe/Berlin"),
    (b"Romance Standard Time", b"Europe/Paris"),
    (b"Central Europe Standard Time", b"Europe/Budapest"),
    (b"Central European Standard Time", b"Europe/Warsaw"),
    (b"FLE Standard Time", b"Europe/Kyiv"),
    (b"E. Europe Standard Time", b"Europe/Chisinau"),
    (b"Turkey Standard Time", b"Europe/Istanbul"),
    (b"Russian Standard Time", b"Europe/Moscow"),
    (b"Israel Standard Time", b"Asia/Jerusalem"),
    (b"Arabian Standard Time", b"Asia/Dubai"),
    (b"India Standard Time", b"Asia/Kolkata"),
    (b"China Standard Time", b"Asia/Shanghai"),
    (b"Tokyo Standard Time", b"Asia/Tokyo"),
    (b"Korea Standard Time", b"Asia/Seoul"),
    (b"AUS Eastern Standard Time", b"Australia/Sydney"),
    (b"New Zealand Standard Time", b"Pacific/Auckland"),
    (b"Pacific Standard Time", b"America/Los_Angeles"),
    (b"Mountain Standard Time", b"America/Denver"),
    (b"Central Standard Time", b"America/Chicago"),
    (b"Eastern Standard Time", b"America/New_York"),
    (b"Atlantic Standard Time", b"America/Halifax"),
    (b"SA Eastern Standard Time", b"America/Sao_Paulo"),
    (
        b"Argentina Standard Time",
        b"America/Argentina/Buenos_Aires",
    ),
    (b"South Africa Standard Time", b"Africa/Johannesburg"),
];

fn calendar_for(locale: &[u8]) -> Option<&'static CalendarNames> {
    let mut lang = lang_subtag(locale);
    if lang == b"no" {
        lang = b"nb".to_vec();
    }
    CALENDARS.iter().find(|names| names.lang == lang)
}

fn abbreviate(name: &str, chars: usize) -> Vec<u8> {
    name.chars().take(chars).collect::<String>().into_bytes()
}

fn prefers_h12(locale: &[u8]) -> bool {
    let lang = lang_subtag(locale);
    if !H12_LANGS.contains(&&lang[..]) {
        return false;
    }
    if lang != b"en" {
        return true;
    }
    let Some(dash) = locale.iter().position(|&c| c == b'-') else {
        return true;
    };
    let region = &locale[dash + 1..];
    !H23_REGIONS.iter().any(|known| {
        region.len() >= 2
            && region[..2].eq_ignore_ascii_case(known)
            && (region.len() == 2 || region[2] == b'-')
    })
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let locale = arg_locale(scope, &arg(args, 0));
    let options = arg(args, 1);
    let format = new_instance(scope, this, "DateTimeFormat");
    hide_text(scope, &format, "_locale", &locale);

    let fields = scope.new_object();
    let date_style = opt_str(scope, &options, "dateStyle");
    let time_style = opt_str(scope, &options, "timeStyle");
    let zone = opt_str(scope, &options, "timeZone");
    hide_text(scope, &format, "_tz", zone.as_deref().unwrap_or(b"local"));

    if let Some(date_style) = &date_style {
        let preset: &[(&str, &[u8])] = match &date_style[..] {
            b"full" => &[
                ("weekday", b"long"),
                ("year", b"numeric"),
                ("month", b"long"),
                ("day", b"numeric"),
            ],
            b"long" => &[
                ("year", b"numeric"),
                ("month", b"long"),
                ("day", b"numeric"),
            ],
            b"medium" => &[
                ("year", b"numeric"),
                ("month", b"short"),
                ("day", b"numeric"),
            ],
            _ => &[
                ("year", b"2-digit"),
                ("month", b"numeric"),
                ("day", b"numeric"),
            ],
        };
        for &(key, value) in preset {
            set_text(scope, &fields, key, value);
        }
    }
    if let Some(time_style) = &time_style {
        set_text(scope, &fields, "hour", b"numeric");
        set_text(scope, &fields, "minute", b"2-digit");
        if time_style != b"short" {
            set_text(scope, &fields, "second", b"2-digit");
        }
    }
    for key in STYLE_KEYS {
        if opt_present(scope, &options, key) {
            if let Ok(value) = scope.get(&options, key) {
                set(scope, &fields, key, value);
            }
        }
    }
    let mut any = false;
    for key in FIELD_KEYS {
        any |= scope
            .get(&fields, key)
            .map_or(true, |value| !value.is_undefined());
    }
    if !any {
        for key in ["year", "month", "day"] {
            set_text(scope, &fields, key, b"numeric");
        }
    }
    hide(scope, &format, "_opts", fields);
    bind_bound(scope, &format, "format", format_bound, 1);
    Ok(format)
}

fn two_digits(value: i32) -> Vec<u8> {
    format!("{value:02}").into_bytes()
}

fn digits(value: i32) -> Vec<u8> {
    value.to_string().into_bytes()
}

fn tagged(kind: &[u8], value: &[u8]) -> (Vec<u8>, Vec<u8>) {
    (kind.to_vec(), value.to_vec())
}

fn core_parts(scope: &mut Scope<'_>, fields: &Value, locale: &[u8], zone: &[u8], ms: f64) -> Value {
    let mut out = Parts::new(scope);
    if ms.is_nan() {
        out.push(scope, b"literal", b"Invalid Date");
        return out.array;
    }
    let seconds = crate::c_trunc_i64((ms / 1000.0).floor());
    let Some(time) = ffi::break_down(seconds, zone == b"UTC") else {
        out.push(scope, b"literal", b"Invalid Date");
        return out.array;
    };

    let weekday = opt_str(scope, fields, "weekday");
    let month = opt_str(scope, fields, "month");
    let day = opt_str(scope, fields, "day");
    let year = opt_str(scope, fields, "year");
    let hour = opt_str(scope, fields, "hour");
    let minute = opt_str(scope, fields, "minute");
    let second = opt_str(scope, fields, "second");
    let hour_cycle = opt_str(scope, fields, "hourCycle");
    let hour12 = opt_bool(scope, fields, "hour12", -1);

    let calendar = calendar_for(locale);
    let lang = lang_subtag(locale);
    let numeric =
        |field: &Option<Vec<u8>>| matches!(field.as_deref(), Some(b"numeric" | b"2-digit"));
    let numeric_date = weekday.is_none() && numeric(&month) && numeric(&day);
    let ymd_order = numeric_date && (lang == b"sv" || lang == b"lt");

    let mut date: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut clock: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    if let Some(style) = &weekday {
        let index = time.weekday.rem_euclid(7) as usize;
        let name = calendar.map_or(DAYS[index], |names| names.days[index]);
        let value = match &style[..] {
            b"narrow" => abbreviate(name, 1),
            b"short" => abbreviate(name, 3),
            _ => name.as_bytes().to_vec(),
        };
        date.push(tagged(b"weekday", &value));
    }
    if let Some(style) = &month {
        let index = time.month.rem_euclid(12) as usize;
        let name = calendar.map_or(MONTHS[index], |names| names.months[index]);
        let value = match &style[..] {
            b"long" => name.as_bytes().to_vec(),
            b"short" => abbreviate(name, 3),
            b"narrow" => abbreviate(name, 1),
            b"2-digit" => two_digits(time.month + 1),
            _ if ymd_order => two_digits(time.month + 1),
            _ => digits(time.month + 1),
        };
        date.push(tagged(b"month", &value));
    }
    if let Some(style) = &day {
        let value = if style == b"2-digit" || ymd_order {
            two_digits(time.day)
        } else {
            digits(time.day)
        };
        date.push(tagged(b"day", &value));
    }
    if let Some(style) = &year {
        let value = if style == b"2-digit" {
            two_digits((time.year % 100 + 100) % 100)
        } else {
            digits(time.year)
        };
        date.push(tagged(b"year", &value));
    }

    let h12 = if hour12 >= 0 {
        hour12 != 0
    } else if let Some(cycle) = &hour_cycle {
        cycle == b"h11" || cycle == b"h12"
    } else {
        prefers_h12(locale)
    };
    if hour.is_some() || minute.is_some() || second.is_some() {
        let mut shown_hour = time.hour;
        let mut period: Option<&[u8]> = None;
        if h12 {
            period = Some(if time.hour < 12 { b"AM" } else { b"PM" });
            shown_hour = time.hour % 12;
            if shown_hour == 0 {
                shown_hour = 12;
            }
        }
        for (kind, style, value) in [
            (&b"hour"[..], &hour, shown_hour),
            (b"minute", &minute, time.minute),
            (b"second", &second, time.second),
        ] {
            if let Some(style) = style {
                let text = if style == b"2-digit" {
                    two_digits(value)
                } else {
                    digits(value)
                };
                clock.push(tagged(kind, &text));
            }
        }
        if let Some(period) = period {
            clock.push(tagged(b"dayPeriod", period));
        }
    }

    let dmy_order = DMY.contains(&&lang[..]);
    let mut ordered: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    if numeric_date {
        let separator: &[u8] = if ymd_order {
            b"-"
        } else if lang_in(locale, &DOT_SEPARATED) {
            b"."
        } else {
            b"/"
        };
        let order: [&[u8]; 3] = if ymd_order {
            [b"year", b"month", b"day"]
        } else if dmy_order {
            [b"day", b"month", b"year"]
        } else {
            [b"month", b"day", b"year"]
        };
        let mut first = true;
        for field in order {
            for entry in date.iter().filter(|(kind, _)| kind.starts_with(field)) {
                if !first {
                    ordered.push(tagged(b"literal", separator));
                }
                ordered.push(entry.clone());
                first = false;
            }
        }
    } else {
        let order: [&[u8]; 4] = if dmy_order {
            [b"weekday", b"day", b"month", b"year"]
        } else {
            [b"weekday", b"month", b"day", b"year"]
        };
        for field in order {
            for entry in date.iter().filter(|(kind, _)| kind.starts_with(field)) {
                if let Some((previous, _)) = ordered.last() {
                    let literal: &[u8] = if entry.0.starts_with(b"year") {
                        if !dmy_order {
                            b", "
                        } else if lang_in(locale, &DAY_DE) {
                            b" de "
                        } else {
                            b" "
                        }
                    } else if previous.starts_with(b"weekday") {
                        if lang_in(locale, &WEEKDAY_COMMA) {
                            b", "
                        } else {
                            b" "
                        }
                    } else if previous.starts_with(b"day") {
                        if lang_in(locale, &DAY_DOT) {
                            b". "
                        } else if lang_in(locale, &DAY_DE) {
                            b" de "
                        } else {
                            b" "
                        }
                    } else {
                        b" "
                    };
                    ordered.push(tagged(b"literal", literal));
                }
                ordered.push(entry.clone());
            }
        }
    }
    if !clock.is_empty() {
        if !ordered.is_empty() {
            ordered.push(tagged(b"literal", b", "));
        }
        for (i, entry) in clock.into_iter().enumerate() {
            if i > 0 {
                let separator: &[u8] = if entry.0.starts_with(b"dayPeriod") {
                    b" "
                } else {
                    b":"
                };
                ordered.push(tagged(b"literal", separator));
            }
            ordered.push(entry);
        }
    }
    for (kind, value) in ordered {
        out.push(scope, &kind, &value);
    }
    out.array
}

pub(crate) fn parts(scope: &mut Scope<'_>, format: &Value, ms: f64) -> Value {
    let fields = scope
        .get(format, "_opts")
        .unwrap_or_else(|_| Value::undefined());
    let locale = hget_str(scope, format, "_locale");
    let zone = hget_str(scope, format, "_tz");
    core_parts(
        scope,
        &fields,
        locale.as_deref().unwrap_or(b"en-US"),
        zone.as_deref().unwrap_or(b"local"),
        ms,
    )
}

fn to_ms(scope: &mut Scope<'_>, args: &[Value]) -> f64 {
    match args.first() {
        Some(value) if !value.is_undefined() => scope.to_number(value).unwrap_or(f64::NAN),
        _ => ffi::real_time_ms(),
    }
}

fn format(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let ms = to_ms(scope, args);
    let parts = parts(scope, this, ms);
    Ok(join_parts(scope, &parts))
}

fn format_bound(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    format(scope, &arg(data, 0), args)
}

fn format_to_parts(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let ms = to_ms(scope, args);
    Ok(parts(scope, this, ms))
}

fn format_range(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let a = number_arg(scope, args, 0, f64::NAN);
    let b = number_arg(scope, args, 1, a);
    let parts_a = parts(scope, this, a);
    let start = join_parts(scope, &parts_a);
    let parts_b = parts(scope, this, b);
    let end = join_parts(scope, &parts_b);
    Ok(join_range(scope, &start, &end))
}

#[cfg(windows)]
fn windows_zone() -> Option<Vec<u8>> {
    let (key, locale) = ffi::windows::zone()?;
    let region = locale.as_deref().and_then(|locale| {
        locale
            .iter()
            .rposition(|&c| c == b'-')
            .map(|at| &locale[at + 1..])
    });
    if let Some(region) = region {
        if key == b"W. Europe Standard Time" {
            if let Some((_, zone)) = WEST_EUROPE
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(region))
            {
                return Some(zone.to_vec());
            }
        }
    }
    WINDOWS_ZONES
        .iter()
        .find(|(windows, _)| *windows == key)
        .map(|(_, zone)| zone.to_vec())
}

#[cfg(not(windows))]
fn windows_zone() -> Option<Vec<u8>> {
    None
}

fn local_zone_id() -> &'static [u8] {
    static ZONE: OnceLock<Vec<u8>> = OnceLock::new();
    ZONE.get_or_init(|| {
        let mut zone = windows_zone();
        let local = ffi::local_zone_identifier();
        if zone.is_none() {
            zone = local.filter(|id| {
                !id.is_empty()
                    && id != b"UTC"
                    && id[0] != b'+'
                    && id[0] != b'-'
                    && id.contains(&b'/')
            });
        }
        if zone.is_none() {
            zone = ffi::getenv(c"TZ").filter(|tz| !tz.is_empty() && tz.contains(&b'/'));
        }
        zone.unwrap_or_else(|| b"UTC".to_vec())
    })
}

fn resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let options = scope.new_object();
    let locale = hget_str(scope, this, "_locale");
    let zone = hget_str(scope, this, "_tz");
    set_text(
        scope,
        &options,
        "locale",
        locale.as_deref().unwrap_or(b"en-US"),
    );
    set_text(scope, &options, "calendar", b"gregory");
    set_text(scope, &options, "numberingSystem", b"latn");
    let zone = match zone.as_deref() {
        Some(zone) if zone != b"local" => zone.to_vec(),
        _ => local_zone_id().to_vec(),
    };
    set_text(scope, &options, "timeZone", &zone);
    let fields = scope.get(this, "_opts");
    for key in FIELD_KEYS {
        let value = match &fields {
            Ok(fields) => scope.get(fields, key),
            Err(error) => Err(error.clone()),
        };
        if let Ok(value) = value {
            if !value.is_undefined() {
                set(scope, &options, key, value);
            }
        }
    }
    Ok(options)
}
