//! Southstar — unit tests for Intl.DateTimeFormat's language tags, offset zones, calendar arithmetic and patterns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::calendar::{self, CalendarDate, Wall};
use super::data::{self, Cycle};
use super::locales::{self, Keyword, Requested, Tag};
use super::pattern::{self, Field, Moment, Renderer, Request, Style, Token};
use super::zone;

fn canonical(text: &str) -> Option<String> {
    locales::canonical_tag(text).map(|tag| tag.render())
}

fn pattern_text(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| match token {
            Token::Text(text) => text.clone(),
            Token::Field(letter, count) => {
                char::from(*letter).to_string().repeat(usize::from(*count))
            }
        })
        .collect()
}

fn request(fields: &[(Field, Style)]) -> Request {
    let mut request: Request = [None; 11];
    for (field, style) in fields {
        request[field.index()] = Some(*style);
    }
    request
}

fn shape(language: &str, region: Option<&str>, fields: &[(Field, Style)], cycle: Cycle) -> String {
    let data = data::locale_data(language, region);
    let format = pattern::components(data, "gregory", &request(fields), cycle, ".");
    pattern_text(&format.tokens)
}

fn moment(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> Moment {
    let wall = Wall {
        year,
        month,
        day,
        hour,
        minute,
        ..Wall::default()
    };
    Moment {
        wall,
        date: CalendarDate {
            era: Some("ce".to_string()),
            era_year: Some(year),
            year,
            ordinal: month,
            month,
            leap_month: false,
            day,
            months_in_year: 12,
        },
        weekday: calendar::weekday(&wall),
        offset_ms: 0,
        daylight: false,
    }
}

fn joined(parts: &[pattern::Part]) -> String {
    parts.iter().map(|(_, value)| value.as_str()).collect()
}

#[test]
fn language_tags_are_validated_and_canonicalized() {
    assert_eq!(canonical("EN-latn-us").as_deref(), Some("en-Latn-US"));
    assert_eq!(
        canonical("en-US-u-CA-Islamicc").as_deref(),
        Some("en-US-u-ca-islamic-civil")
    );
    assert_eq!(
        canonical("en-u-nu-thai-ca-buddhist").as_deref(),
        Some("en-u-ca-buddhist-nu-thai")
    );
    assert_eq!(canonical("en-u-kn-true").as_deref(), Some("en-u-kn"));
    assert_eq!(canonical("en-x-foo").as_deref(), Some("en-x-foo"));
    assert_eq!(canonical("iw").as_deref(), Some("he"));
    assert!(canonical("en-t-de-k0-qwertz").is_some());
    for invalid in [
        "",
        "i",
        "x-private",
        "de_DE",
        "en-",
        "en--US",
        "en-a-bbb-a-ccc",
        "de-1996-1996",
        "abcdefghi",
        "en-u",
        "en-ü",
    ] {
        assert!(canonical(invalid).is_none(), "{invalid} should be rejected");
    }
}

#[test]
fn lookup_truncates_and_keeps_the_unicode_extension() {
    let available = |tag: &Tag| tag.language == "de" && tag.variants.is_empty();
    let requested = vec!["de-CH-1996-u-ca-buddhist".to_string()];
    let (found, source) = locales::lookup(&requested, &available).expect("a German locale");
    assert_eq!(found.render(), "de-CH");
    assert_eq!(
        source
            .and_then(|tag| tag.keyword("ca").map(str::to_string))
            .as_deref(),
        Some("buddhist")
    );
}

#[test]
fn keywords_follow_resolve_locale() {
    let available = |tag: &Tag| tag.language == "en";
    let fallback = locales::canonical_tag("en-US").unwrap_or_default();
    let calendars = |value: &str| matches!(value, "gregory" | "iso8601");
    let defaults = |_: &Tag, _: &str| Some("gregory".to_string());
    let resolve = |locale: &str, option: Requested| {
        locales::resolve(
            &[locale.to_string()],
            &available,
            &fallback,
            &[Keyword {
                key: "ca",
                requested: option,
                supported: &calendars,
            }],
            &defaults,
        )
    };
    let kept = resolve("en-u-ca-iso8601", Requested::Text("iso8601".to_string()));
    assert_eq!(kept.locale, "en-u-ca-iso8601");
    let overridden = resolve("en-u-ca-iso8601", Requested::Text("gregory".to_string()));
    assert_eq!(overridden.locale, "en");
    assert_eq!(overridden.values[0].as_deref(), Some("gregory"));
    let ignored = resolve("en-u-ca-iso8601", Requested::Text("invalid".to_string()));
    assert_eq!(ignored.locale, "en-u-ca-iso8601");
    let unsupported = resolve("en-u-ca-invalid", Requested::Absent);
    assert_eq!(unsupported.locale, "en");
}

#[test]
fn offset_time_zones_parse_and_format() {
    assert_eq!(zone::parse_offset("+05:30"), Some(330));
    assert_eq!(zone::parse_offset("-0800"), Some(-480));
    assert_eq!(zone::parse_offset("+03"), Some(180));
    assert_eq!(zone::parse_offset("-00"), Some(0));
    for invalid in [
        "+24",
        "+1:00",
        "\u{2212}05",
        "+15:59:00",
        "+0:003",
        "-10.50",
        "UTC",
    ] {
        assert_eq!(
            zone::parse_offset(invalid),
            None,
            "{invalid} should be rejected"
        );
    }
    assert_eq!(zone::offset_id(330), "+05:30");
    assert_eq!(zone::offset_id(-420), "-07:00");
    assert_eq!(zone::offset_id(0), "+00:00");
}

#[test]
fn civil_days_round_trip() {
    assert_eq!(calendar::days_from_civil(1970, 1, 1), 0);
    assert_eq!(calendar::civil_from_days(-1), (1969, 12, 31));
    assert_eq!(calendar::civil_from_days(-719_468), (0, 3, 1));
    for days in (-1_000_000..1_000_000).step_by(997) {
        let (year, month, day) = calendar::civil_from_days(days);
        assert_eq!(calendar::days_from_civil(year, month, day), days);
    }
    let thursday = calendar::wall_from_ms(0);
    assert_eq!(calendar::weekday(&thursday), 4);
    let before_epoch = calendar::wall_from_ms(-1);
    assert_eq!(
        (
            before_epoch.year,
            before_epoch.hour,
            before_epoch.millisecond
        ),
        (1969, 23, 999)
    );
}

#[test]
fn skeletons_match_cldr_patterns() {
    let numeric = Style::Numeric;
    let ymd = [
        (Field::Year, numeric),
        (Field::Month, numeric),
        (Field::Day, numeric),
    ];
    assert_eq!(shape("en", Some("US"), &ymd, Cycle::H12), "M/d/y");
    assert_eq!(shape("en", Some("GB"), &ymd, Cycle::H23), "dd/MM/y");
    assert_eq!(shape("sv", None, &ymd, Cycle::H23), "y-MM-dd");
    let clock = [(Field::Hour, numeric), (Field::Minute, numeric)];
    assert_eq!(shape("en", None, &clock, Cycle::H12), "h:mm a");
    assert_eq!(shape("en", None, &clock, Cycle::H23), "HH:mm");
    assert_eq!(shape("en", None, &clock, Cycle::H11), "K:mm a");
    let long_date = [(Field::Month, Style::Long), (Field::Day, numeric)];
    assert_eq!(shape("en", None, &long_date, Cycle::H12), "MMMM d");
    let era = [
        (Field::Era, Style::Narrow),
        (Field::Year, numeric),
        (Field::Month, numeric),
    ];
    assert_eq!(shape("en", None, &era, Cycle::H12), "M/y GGGGG");
    let german = [
        (Field::Weekday, Style::Short),
        (Field::Month, Style::Long),
        (Field::Day, numeric),
    ];
    assert_eq!(shape("de", None, &german, Cycle::H23), "E, d. MMMM");
    let period = [(Field::DayPeriod, Style::Long), (Field::Hour, numeric)];
    assert_eq!(shape("en", None, &period, Cycle::H12), "h BBBB");
    let fraction = [
        (Field::Minute, numeric),
        (Field::Second, numeric),
        (Field::Fraction, Style::Digits(2)),
    ];
    assert_eq!(shape("en", None, &fraction, Cycle::H12), "mm:ss.SS");
}

#[test]
fn parts_render_names_and_digits() {
    let data = data::locale_data("en", Some("US"));
    let tokens = pattern::parse_pattern("EEEE, MMMM d, y 'at' h:mm a");
    let renderer = Renderer::new(data, "gregory", "UTC", "latn");
    let parts = renderer.parts(&tokens, &moment(2019, 1, 3, 14, 5));
    assert_eq!(joined(&parts), "Thursday, January 3, 2019 at 2:05 PM");
    assert_eq!(parts[0], ("weekday", "Thursday".to_string()));
    let hanidec = Renderer::new(data, "gregory", "UTC", "hanidec");
    let parts = hanidec.parts(&pattern::parse_pattern("HH:mm"), &moment(2019, 1, 3, 2, 35));
    assert_eq!(joined(&parts), "〇二:三五");
    let narrow = renderer.parts(
        &pattern::parse_pattern("y GGGGG"),
        &moment(2019, 1, 3, 0, 0),
    );
    assert_eq!(joined(&narrow), "2019 A");
}

#[test]
fn ranges_share_the_common_fields() {
    let data = data::locale_data("en", Some("US"));
    let renderer = Renderer::new(data, "gregory", "UTC", "latn");
    let text = |pattern: &str, start: Moment, end: Moment| -> String {
        renderer
            .range(&pattern::parse_pattern(pattern), &start, &end)
            .iter()
            .map(|(_, value, _)| value.as_str())
            .collect()
    };
    assert_eq!(
        text(
            "MMM d, y",
            moment(2019, 1, 3, 0, 0),
            moment(2019, 1, 5, 0, 0)
        ),
        "Jan 3 – 5, 2019"
    );
    assert_eq!(
        text(
            "MMM d, y",
            moment(2019, 1, 3, 0, 0),
            moment(2019, 3, 4, 0, 0)
        ),
        "Jan 3 – Mar 4, 2019"
    );
    assert_eq!(
        text("M/d/y", moment(2019, 1, 3, 0, 0), moment(2019, 1, 5, 0, 0)),
        "1/3/2019 – 1/5/2019"
    );
    assert_eq!(
        text(
            "h:mm a",
            moment(2019, 1, 3, 13, 0),
            moment(2019, 1, 3, 15, 30)
        ),
        "1:00 – 3:30 PM"
    );
    assert_eq!(
        text(
            "h:mm a",
            moment(2019, 1, 3, 1, 0),
            moment(2019, 1, 3, 13, 0)
        ),
        "1:00 AM – 1:00 PM"
    );
    assert_eq!(
        text(
            "MMM d, y",
            moment(2019, 1, 3, 1, 0),
            moment(2019, 1, 3, 2, 0)
        ),
        "Jan 3, 2019"
    );
}
