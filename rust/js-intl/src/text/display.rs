//! Southstar — Intl.DisplayNames: English names for languages, regions, scripts, currencies, calendars and date fields.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use super::service::{self, Options};
use crate::{Method, arg};

pub(crate) const METHODS: [Method; 2] = [("of", of, 1), ("resolvedOptions", resolved_options, 0)];

const LANGUAGES: [&str; 1] = ["en"];

const STYLES: [&str; 3] = ["narrow", "short", "long"];
const TYPES: [&str; 6] = [
    "language",
    "region",
    "script",
    "currency",
    "calendar",
    "dateTimeField",
];
const FALLBACKS: [&str; 2] = ["code", "none"];
const LANGUAGE_DISPLAYS: [&str; 2] = ["dialect", "standard"];

#[derive(Clone)]
struct DisplayNames {
    locale: String,
    style: &'static str,
    kind: &'static str,
    fallback: &'static str,
    language_display: Option<&'static str>,
}

const LANGUAGE_NAMES: &[(&str, &str)] = &[
    ("af", "Afrikaans"),
    ("am", "Amharic"),
    ("ar", "Arabic"),
    ("az", "Azerbaijani"),
    ("be", "Belarusian"),
    ("bg", "Bulgarian"),
    ("bn", "Bangla"),
    ("bs", "Bosnian"),
    ("ca", "Catalan"),
    ("cs", "Czech"),
    ("cy", "Welsh"),
    ("da", "Danish"),
    ("de", "German"),
    ("el", "Greek"),
    ("en", "English"),
    ("eo", "Esperanto"),
    ("es", "Spanish"),
    ("et", "Estonian"),
    ("eu", "Basque"),
    ("fa", "Persian"),
    ("fi", "Finnish"),
    ("fil", "Filipino"),
    ("fo", "Faroese"),
    ("fr", "French"),
    ("ga", "Irish"),
    ("gl", "Galician"),
    ("gu", "Gujarati"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hr", "Croatian"),
    ("hu", "Hungarian"),
    ("hy", "Armenian"),
    ("id", "Indonesian"),
    ("is", "Icelandic"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ka", "Georgian"),
    ("kk", "Kazakh"),
    ("km", "Khmer"),
    ("kn", "Kannada"),
    ("ko", "Korean"),
    ("ky", "Kyrgyz"),
    ("la", "Latin"),
    ("lo", "Lao"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("mk", "Macedonian"),
    ("ml", "Malayalam"),
    ("mn", "Mongolian"),
    ("mr", "Marathi"),
    ("ms", "Malay"),
    ("mt", "Maltese"),
    ("my", "Burmese"),
    ("nb", "Norwegian Bokmål"),
    ("ne", "Nepali"),
    ("nl", "Dutch"),
    ("nn", "Norwegian Nynorsk"),
    ("no", "Norwegian"),
    ("pa", "Punjabi"),
    ("pl", "Polish"),
    ("ps", "Pashto"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("si", "Sinhala"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("sq", "Albanian"),
    ("sr", "Serbian"),
    ("sv", "Swedish"),
    ("sw", "Swahili"),
    ("ta", "Tamil"),
    ("te", "Telugu"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("uz", "Uzbek"),
    ("vi", "Vietnamese"),
    ("yue", "Cantonese"),
    ("zh", "Chinese"),
    ("zu", "Zulu"),
];

const DIALECT_NAMES: &[(&str, &str)] = &[
    ("ar-001", "Modern Standard Arabic"),
    ("de-AT", "Austrian German"),
    ("de-CH", "Swiss High German"),
    ("en-AU", "Australian English"),
    ("en-CA", "Canadian English"),
    ("en-GB", "British English"),
    ("en-US", "American English"),
    ("es-419", "Latin American Spanish"),
    ("es-ES", "European Spanish"),
    ("es-MX", "Mexican Spanish"),
    ("fr-CA", "Canadian French"),
    ("fr-CH", "Swiss French"),
    ("nl-BE", "Flemish"),
    ("pt-BR", "Brazilian Portuguese"),
    ("pt-PT", "European Portuguese"),
    ("zh-Hans", "Simplified Chinese"),
    ("zh-Hant", "Traditional Chinese"),
];

const REGION_NAMES: &[(&str, &str)] = &[
    ("001", "world"),
    ("002", "Africa"),
    ("019", "Americas"),
    ("142", "Asia"),
    ("150", "Europe"),
    ("419", "Latin America"),
    ("AE", "United Arab Emirates"),
    ("AR", "Argentina"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("BE", "Belgium"),
    ("BG", "Bulgaria"),
    ("BR", "Brazil"),
    ("CA", "Canada"),
    ("CH", "Switzerland"),
    ("CL", "Chile"),
    ("CN", "China"),
    ("CO", "Colombia"),
    ("CZ", "Czechia"),
    ("DE", "Germany"),
    ("DK", "Denmark"),
    ("EE", "Estonia"),
    ("EG", "Egypt"),
    ("ES", "Spain"),
    ("FI", "Finland"),
    ("FR", "France"),
    ("GB", "United Kingdom"),
    ("GR", "Greece"),
    ("HK", "Hong Kong SAR China"),
    ("HR", "Croatia"),
    ("HU", "Hungary"),
    ("ID", "Indonesia"),
    ("IE", "Ireland"),
    ("IL", "Israel"),
    ("IN", "India"),
    ("IR", "Iran"),
    ("IS", "Iceland"),
    ("IT", "Italy"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("KR", "South Korea"),
    ("LT", "Lithuania"),
    ("LV", "Latvia"),
    ("MX", "Mexico"),
    ("MY", "Malaysia"),
    ("NG", "Nigeria"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NZ", "New Zealand"),
    ("PE", "Peru"),
    ("PH", "Philippines"),
    ("PK", "Pakistan"),
    ("PL", "Poland"),
    ("PT", "Portugal"),
    ("RO", "Romania"),
    ("RS", "Serbia"),
    ("RU", "Russia"),
    ("SA", "Saudi Arabia"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("SI", "Slovenia"),
    ("SK", "Slovakia"),
    ("TH", "Thailand"),
    ("TR", "Türkiye"),
    ("TW", "Taiwan"),
    ("UA", "Ukraine"),
    ("US", "United States"),
    ("VN", "Vietnam"),
    ("ZA", "South Africa"),
];

const SCRIPT_NAMES: &[(&str, &str)] = &[
    ("Arab", "Arabic"),
    ("Armn", "Armenian"),
    ("Beng", "Bangla"),
    ("Cyrl", "Cyrillic"),
    ("Deva", "Devanagari"),
    ("Ethi", "Ethiopic"),
    ("Geor", "Georgian"),
    ("Grek", "Greek"),
    ("Gujr", "Gujarati"),
    ("Guru", "Gurmukhi"),
    ("Hang", "Hangul"),
    ("Hani", "Han"),
    ("Hans", "Simplified"),
    ("Hant", "Traditional"),
    ("Hebr", "Hebrew"),
    ("Hira", "Hiragana"),
    ("Jpan", "Japanese"),
    ("Kana", "Katakana"),
    ("Khmr", "Khmer"),
    ("Knda", "Kannada"),
    ("Kore", "Korean"),
    ("Laoo", "Lao"),
    ("Latn", "Latin"),
    ("Mlym", "Malayalam"),
    ("Mong", "Mongolian"),
    ("Mymr", "Myanmar"),
    ("Sinh", "Sinhala"),
    ("Taml", "Tamil"),
    ("Telu", "Telugu"),
    ("Thaa", "Thaana"),
    ("Thai", "Thai"),
    ("Tibt", "Tibetan"),
    ("Zyyy", "Common"),
];

const CURRENCY_NAMES: &[(&str, &str)] = &[
    ("AUD", "Australian Dollar"),
    ("BRL", "Brazilian Real"),
    ("CAD", "Canadian Dollar"),
    ("CHF", "Swiss Franc"),
    ("CNY", "Chinese Yuan"),
    ("CZK", "Czech Koruna"),
    ("DKK", "Danish Krone"),
    ("EUR", "Euro"),
    ("GBP", "British Pound"),
    ("HKD", "Hong Kong Dollar"),
    ("HUF", "Hungarian Forint"),
    ("IDR", "Indonesian Rupiah"),
    ("ILS", "Israeli New Shekel"),
    ("INR", "Indian Rupee"),
    ("ISK", "Icelandic Króna"),
    ("JPY", "Japanese Yen"),
    ("KRW", "South Korean Won"),
    ("MXN", "Mexican Peso"),
    ("NOK", "Norwegian Krone"),
    ("NZD", "New Zealand Dollar"),
    ("PLN", "Polish Zloty"),
    ("RUB", "Russian Ruble"),
    ("SEK", "Swedish Krona"),
    ("SGD", "Singapore Dollar"),
    ("THB", "Thai Baht"),
    ("TRY", "Turkish Lira"),
    ("TWD", "New Taiwan Dollar"),
    ("UAH", "Ukrainian Hryvnia"),
    ("USD", "US Dollar"),
    ("ZAR", "South African Rand"),
];

const CALENDAR_NAMES: &[(&str, &str)] = &[
    ("buddhist", "Buddhist Calendar"),
    ("chinese", "Chinese Calendar"),
    ("coptic", "Coptic Calendar"),
    ("dangi", "Dangi Calendar"),
    ("ethioaa", "Ethiopic Amete Alem Calendar"),
    ("ethiopic", "Ethiopic Calendar"),
    ("gregory", "Gregorian Calendar"),
    ("hebrew", "Hebrew Calendar"),
    ("indian", "Indian National Calendar"),
    ("islamic", "Hijri Calendar"),
    ("islamic-civil", "Hijri Calendar (tabular, civil epoch)"),
    ("islamic-rgsa", "Hijri Calendar (Saudi Arabia, sighting)"),
    (
        "islamic-tbla",
        "Hijri Calendar (tabular, astronomical epoch)",
    ),
    ("islamic-umalqura", "Hijri Calendar (Umm al-Qura)"),
    ("iso8601", "ISO-8601 Calendar"),
    ("japanese", "Japanese Calendar"),
    ("persian", "Persian Calendar"),
    ("roc", "Minguo Calendar"),
];

const DATE_TIME_FIELDS: &[(&str, &str, &str)] = &[
    ("era", "era", "era"),
    ("year", "year", "yr."),
    ("quarter", "quarter", "qtr."),
    ("month", "month", "mo."),
    ("weekOfYear", "week", "wk."),
    ("weekday", "day of the week", "day of wk."),
    ("day", "day", "day"),
    ("dayPeriod", "AM/PM", "AM/PM"),
    ("hour", "hour", "hr."),
    ("minute", "minute", "min."),
    ("second", "second", "sec."),
    ("timeZoneName", "time zone", "zone"),
];

fn name_in(table: &[(&str, &'static str)], code: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|&&(known, _)| known == code)
        .map(|&(_, name)| name)
}

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::require_new_target(scope, this, "DisplayNames")?;
    let prototype = service::prototype_from(scope, this, "DisplayNames")?;
    let requested = service::requested_locales(scope, &arg(args, 0))?;
    let options = arg(args, 1);
    if options.is_undefined() {
        return Err(scope.type_error("Intl.DisplayNames requires an options object"));
    }
    let options = Options::object(scope, &options)?;
    options.locale_matcher(scope)?;
    let locale = service::resolve(&requested, &LANGUAGES);
    let style = options.choice_or(scope, "style", &STYLES, "long")?;
    let Some(kind) = options.choice(scope, "type", &TYPES)? else {
        return Err(scope.type_error("Intl.DisplayNames requires a type option"));
    };
    let fallback = options.choice_or(scope, "fallback", &FALLBACKS, "code")?;
    let language_display =
        options.choice_or(scope, "languageDisplay", &LANGUAGE_DISPLAYS, "dialect")?;
    let names = DisplayNames {
        locale,
        style,
        kind,
        fallback,
        language_display: (kind == "language").then_some(language_display),
    };
    Ok(scope.new_host_object(Some(&prototype), names))
}

pub(crate) fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    service::supported_locales(scope, args, &LANGUAGES)
}

fn is_alpha(subtag: &str, lengths: core::ops::RangeInclusive<usize>) -> bool {
    lengths.contains(&subtag.len()) && subtag.bytes().all(|c| c.is_ascii_alphabetic())
}

fn is_region_code(code: &str) -> bool {
    is_alpha(code, 2..=2) || (code.len() == 3 && code.bytes().all(|c| c.is_ascii_digit()))
}

fn is_variant(subtag: &str) -> bool {
    let alphanumeric = subtag.bytes().all(|c| c.is_ascii_alphanumeric());
    alphanumeric
        && ((5..=8).contains(&subtag.len())
            || (subtag.len() == 4 && subtag.as_bytes()[0].is_ascii_digit()))
}

struct LanguageId<'a> {
    language: &'a str,
    script: Option<&'a str>,
    region: Option<&'a str>,
    variants: Vec<&'a str>,
}

fn parse_language_id(code: &str) -> Option<LanguageId<'_>> {
    let mut subtags = code.split('-').peekable();
    let language = subtags.next()?;
    if !(is_alpha(language, 2..=3) || is_alpha(language, 5..=8)) {
        return None;
    }
    let script = subtags.next_if(|subtag| is_alpha(subtag, 4..=4));
    let region = subtags.next_if(|subtag| is_region_code(subtag));
    let mut variants: Vec<&str> = Vec::new();
    for subtag in subtags {
        if !is_variant(subtag)
            || variants
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(subtag))
        {
            return None;
        }
        variants.push(subtag);
    }
    Some(LanguageId {
        language,
        script,
        region,
        variants,
    })
}

fn title_case(code: &str) -> String {
    let mut out = code.to_ascii_lowercase();
    if let Some(first) = out.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    out
}

const CALENDAR_ALIASES: [(&str, &str); 3] = [
    ("ethiopic-amete-alem", "ethioaa"),
    ("gregorian", "gregory"),
    ("islamicc", "islamic-civil"),
];

fn calendar_code(code: &str) -> String {
    let code = code.to_ascii_lowercase();
    match CALENDAR_ALIASES.iter().find(|&&(alias, _)| alias == code) {
        Some(&(_, canonical)) => canonical.to_owned(),
        None => code,
    }
}

fn canonical_code(scope: &mut Scope<'_>, kind: &str, code: &str) -> Result<Option<String>, Value> {
    Ok(match kind {
        "language" => match parse_language_id(code) {
            Some(_) => {
                let code = scope.string(code);
                service::requested_locales(scope, &code)?.into_iter().next()
            }
            None => None,
        },
        "region" => is_region_code(code).then(|| code.to_ascii_uppercase()),
        "script" => is_alpha(code, 4..=4).then(|| title_case(code)),
        "calendar" => service::is_unicode_type(code).then(|| calendar_code(code)),
        "dateTimeField" => DATE_TIME_FIELDS
            .iter()
            .any(|&(field, _, _)| field == code)
            .then(|| code.to_owned()),
        _ => is_alpha(code, 3..=3).then(|| code.to_ascii_uppercase()),
    })
}

fn qualified(name: String, qualifiers: &[String]) -> String {
    if qualifiers.is_empty() {
        name
    } else {
        format!("{name} ({})", qualifiers.join(", "))
    }
}

fn language_name(id: &LanguageId<'_>, dialect: bool) -> Option<String> {
    let script = id.script.map(title_case);
    let region = id.region.map(str::to_ascii_uppercase);
    let language = id.language.to_ascii_lowercase();
    let mut used_script = false;
    let mut used_region = false;
    let mut base = None;
    if dialect {
        let candidates = [
            (script.as_ref(), region.as_ref()),
            (None, region.as_ref()),
            (script.as_ref(), None),
        ];
        for (with_script, with_region) in candidates {
            if with_script.is_none() && with_region.is_none() {
                continue;
            }
            let mut key = language.clone();
            for subtag in [with_script, with_region].into_iter().flatten() {
                key.push('-');
                key.push_str(subtag);
            }
            if let Some(name) = name_in(DIALECT_NAMES, &key) {
                base = Some(name.to_owned());
                used_script = with_script.is_some();
                used_region = with_region.is_some();
                break;
            }
        }
    }
    let base = match base {
        Some(base) => base,
        None => name_in(LANGUAGE_NAMES, &language)?.to_owned(),
    };
    let mut qualifiers = Vec::new();
    if let Some(script) = script.filter(|_| !used_script) {
        qualifiers.push(name_in(SCRIPT_NAMES, &script).map_or(script.clone(), str::to_owned));
    }
    if let Some(region) = region.filter(|_| !used_region) {
        qualifiers.push(name_in(REGION_NAMES, &region).map_or(region.clone(), str::to_owned));
    }
    qualifiers.extend(
        id.variants
            .iter()
            .map(|variant| variant.to_ascii_uppercase()),
    );
    Some(qualified(base, &qualifiers))
}

fn display_name(names: &DisplayNames, code: &str) -> Option<String> {
    match names.kind {
        "language" => {
            let id = parse_language_id(code)?;
            language_name(&id, names.language_display == Some("dialect"))
        }
        "region" => name_in(REGION_NAMES, code).map(str::to_owned),
        "script" => name_in(SCRIPT_NAMES, code).map(str::to_owned),
        "calendar" => name_in(CALENDAR_NAMES, code).map(str::to_owned),
        _ => DATE_TIME_FIELDS
            .iter()
            .find(|&&(field, _, _)| field == code)
            .map(|&(_, long, short)| {
                let name = if names.style == "long" { long } else { short };
                name.to_owned()
            }),
    }
}

fn currency_name(scope: &mut Scope<'_>, code: &str) -> Option<String> {
    let currencies = service::supported_values(scope, "currency");
    currencies
        .iter()
        .any(|known| known == code)
        .then(|| name_in(CURRENCY_NAMES, code).unwrap_or(code).to_owned())
}

fn of(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let names: DisplayNames = service::receiver(scope, this, "Intl.DisplayNames.prototype.of")?;
    let code = service::text_of(scope, &arg(args, 0))?;
    let Some(code) = canonical_code(scope, names.kind, &code)? else {
        return Err(scope.range_error(&format!("Invalid {} code {code}", names.kind)));
    };
    let name = if names.kind == "currency" {
        currency_name(scope, &code)
    } else {
        display_name(&names, &code)
    };
    Ok(match name {
        Some(name) => scope.string(&name),
        None if names.fallback == "code" => scope.string(&code),
        None => Value::undefined(),
    })
}

fn resolved_options(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let names: DisplayNames =
        service::receiver(scope, this, "Intl.DisplayNames.prototype.resolvedOptions")?;
    let options = scope.new_object();
    service::text_property(scope, &options, "locale", &names.locale);
    service::text_property(scope, &options, "style", names.style);
    service::text_property(scope, &options, "type", names.kind);
    service::text_property(scope, &options, "fallback", names.fallback);
    if let Some(display) = names.language_display {
        service::text_property(scope, &options, "languageDisplay", display);
    }
    Ok(options)
}
