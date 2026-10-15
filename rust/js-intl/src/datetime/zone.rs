//! Southstar — Intl.DateTimeFormat time zones: offset identifiers, IANA names and offsets through Temporal, and display names.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::OnceCell;
use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

use super::calendar::days_from_civil;
use crate::ffi;

const UTC_ALIASES: [&str; 14] = [
    "UTC",
    "Etc/UTC",
    "Etc/UCT",
    "Etc/Universal",
    "Etc/Zulu",
    "UCT",
    "Universal",
    "Zulu",
    "Etc/GMT",
    "GMT",
    "Etc/GMT0",
    "GMT0",
    "Etc/Greenwich",
    "Greenwich",
];

struct Metazone {
    zones: &'static str,
    long: &'static str,
    short: &'static str,
}

const METAZONES: [Metazone; 28] = [
    Metazone {
        zones: "UTC|Etc/UTC|Etc/UCT|Etc/Universal|Etc/Zulu|UCT|Universal|Zulu",
        long: "Coordinated Universal Time|Coordinated Universal Time|Coordinated Universal Time",
        short: "UTC|UTC|UTC",
    },
    Metazone {
        zones: "Europe/London|GB|GB-Eire|Europe/Belfast|Europe/Guernsey|Europe/Isle_of_Man|Europe/Jersey",
        long: "Greenwich Mean Time|British Summer Time|United Kingdom Time",
        short: "GMT||",
    },
    Metazone {
        zones: "Europe/Dublin|Eire",
        long: "Greenwich Mean Time|Irish Standard Time|Ireland Time",
        short: "GMT||",
    },
    Metazone {
        zones: "Etc/GMT|GMT|Etc/GMT0|GMT0|Etc/GMT+0|Etc/GMT-0|GMT+0|GMT-0|Etc/Greenwich|Greenwich|Africa/Abidjan|Africa/Accra|Africa/Bamako|Africa/Banjul|Africa/Bissau|Africa/Conakry|Africa/Dakar|Africa/Freetown|Africa/Lome|Africa/Monrovia|Africa/Nouakchott|Africa/Ouagadougou|Africa/Sao_Tome|Atlantic/Reykjavik|Atlantic/St_Helena|Iceland",
        long: "Greenwich Mean Time|Greenwich Mean Time|Greenwich Mean Time",
        short: "GMT|GMT|GMT",
    },
    Metazone {
        zones: "America/New_York|America/Detroit|America/Toronto|America/Nassau|America/Iqaluit|America/Nipigon|America/Thunder_Bay|America/Montreal|America/Indiana/Indianapolis|America/Indianapolis|America/Indiana/Marengo|America/Indiana/Petersburg|America/Indiana/Vevay|America/Indiana/Vincennes|America/Indiana/Winamac|America/Kentucky/Louisville|America/Louisville|America/Kentucky/Monticello|America/Fort_Wayne|America/Cancun|America/Jamaica|America/Panama|America/Port-au-Prince|America/Grand_Turk|America/Cayman|America/Coral_Harbour|America/Atikokan|US/Eastern|US/Michigan|US/East-Indiana|EST5EDT|EST|Canada/Eastern|Jamaica",
        long: "Eastern Standard Time|Eastern Daylight Time|Eastern Time",
        short: "EST|EDT|ET",
    },
    Metazone {
        zones: "America/Chicago|America/Winnipeg|America/Menominee|America/Indiana/Knox|America/Indiana/Tell_City|America/Knox_IN|America/North_Dakota/Beulah|America/North_Dakota/Center|America/North_Dakota/New_Salem|America/Rainy_River|America/Rankin_Inlet|America/Resolute|America/Matamoros|America/Mexico_City|America/Monterrey|America/Merida|America/Bahia_Banderas|America/Belize|America/Costa_Rica|America/El_Salvador|America/Guatemala|America/Tegucigalpa|America/Managua|America/Regina|America/Swift_Current|America/Ojinaga|US/Central|US/Indiana-Starke|CST6CDT|Canada/Central|Canada/Saskatchewan|Mexico/General",
        long: "Central Standard Time|Central Daylight Time|Central Time",
        short: "CST|CDT|CT",
    },
    Metazone {
        zones: "America/Denver|America/Boise|America/Edmonton|America/Phoenix|America/Cambridge_Bay|America/Inuvik|America/Yellowknife|America/Creston|America/Dawson_Creek|America/Fort_Nelson|America/Ciudad_Juarez|America/Shiprock|US/Mountain|US/Arizona|MST7MDT|MST|Canada/Mountain|Navajo",
        long: "Mountain Standard Time|Mountain Daylight Time|Mountain Time",
        short: "MST|MDT|MT",
    },
    Metazone {
        zones: "America/Los_Angeles|America/Vancouver|America/Tijuana|America/Ensenada|America/Santa_Isabel|US/Pacific|PST8PDT|Canada/Pacific|Mexico/BajaNorte",
        long: "Pacific Standard Time|Pacific Daylight Time|Pacific Time",
        short: "PST|PDT|PT",
    },
    Metazone {
        zones: "America/Anchorage|America/Juneau|America/Metlakatla|America/Nome|America/Sitka|America/Yakutat|US/Alaska",
        long: "Alaska Standard Time|Alaska Daylight Time|Alaska Time",
        short: "AKST|AKDT|AKT",
    },
    Metazone {
        zones: "Pacific/Honolulu|Pacific/Johnston|America/Adak|America/Atka|US/Hawaii|US/Aleutian|HST",
        long: "Hawaii-Aleutian Standard Time|Hawaii-Aleutian Daylight Time|Hawaii-Aleutian Time",
        short: "HST|HDT|HST",
    },
    Metazone {
        zones: "America/Halifax|America/Glace_Bay|America/Goose_Bay|America/Moncton|America/Thule|Atlantic/Bermuda|America/Puerto_Rico|America/Barbados|America/Martinique|America/Santo_Domingo|America/Anguilla|America/Antigua|America/Aruba|America/Curacao|America/Dominica|America/Grenada|America/Guadeloupe|America/Kralendijk|America/Lower_Princes|America/Marigot|America/Montserrat|America/Port_of_Spain|America/St_Barthelemy|America/St_Kitts|America/St_Lucia|America/St_Thomas|America/St_Vincent|America/Tortola|America/Virgin|Canada/Atlantic",
        long: "Atlantic Standard Time|Atlantic Daylight Time|Atlantic Time",
        short: "AST|ADT|AT",
    },
    Metazone {
        zones: "Europe/Amsterdam|Europe/Andorra|Europe/Belgrade|Europe/Berlin|Europe/Bratislava|Europe/Brussels|Europe/Budapest|Europe/Busingen|Europe/Copenhagen|Europe/Gibraltar|Europe/Ljubljana|Europe/Luxembourg|Europe/Madrid|Europe/Malta|Europe/Monaco|Europe/Oslo|Europe/Paris|Europe/Podgorica|Europe/Prague|Europe/Rome|Europe/San_Marino|Europe/Sarajevo|Europe/Skopje|Europe/Stockholm|Europe/Tirane|Europe/Vaduz|Europe/Vatican|Europe/Vienna|Europe/Warsaw|Europe/Zagreb|Europe/Zurich|Africa/Algiers|Africa/Ceuta|Africa/Tunis|Arctic/Longyearbyen|Atlantic/Jan_Mayen|CET|MET|Poland",
        long: "Central European Standard Time|Central European Summer Time|Central European Time",
        short: "||",
    },
    Metazone {
        zones: "Europe/Athens|Europe/Bucharest|Europe/Chisinau|Europe/Tiraspol|Europe/Helsinki|Europe/Kiev|Europe/Kyiv|Europe/Mariehamn|Europe/Riga|Europe/Sofia|Europe/Tallinn|Europe/Uzhgorod|Europe/Vilnius|Europe/Zaporozhye|Asia/Beirut|Asia/Famagusta|Asia/Gaza|Asia/Hebron|Asia/Nicosia|Europe/Nicosia|Africa/Cairo|Africa/Tripoli|Egypt|Libya|EET",
        long: "Eastern European Standard Time|Eastern European Summer Time|Eastern European Time",
        short: "||",
    },
    Metazone {
        zones: "Europe/Lisbon|Atlantic/Canary|Atlantic/Faroe|Atlantic/Faeroe|Atlantic/Madeira|Portugal|WET",
        long: "Western European Standard Time|Western European Summer Time|Western European Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Tokyo|Japan",
        long: "Japan Standard Time|Japan Daylight Time|Japan Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Kolkata|Asia/Calcutta",
        long: "India Standard Time|India Standard Time|India Standard Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Shanghai|Asia/Chongqing|Asia/Chungking|Asia/Harbin|PRC",
        long: "China Standard Time|China Daylight Time|China Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Seoul|ROK",
        long: "Korean Standard Time|Korean Daylight Time|Korean Time",
        short: "||",
    },
    Metazone {
        zones: "Australia/Sydney|Australia/Melbourne|Australia/Brisbane|Australia/Hobart|Australia/Canberra|Australia/ACT|Australia/NSW|Australia/Victoria|Australia/Queensland|Australia/Tasmania|Australia/Lindeman|Australia/Currie",
        long: "Australian Eastern Standard Time|Australian Eastern Daylight Time|Eastern Australia Time",
        short: "||",
    },
    Metazone {
        zones: "Australia/Adelaide|Australia/Darwin|Australia/Broken_Hill|Australia/South|Australia/North|Australia/Yancowinna",
        long: "Australian Central Standard Time|Australian Central Daylight Time|Central Australia Time",
        short: "||",
    },
    Metazone {
        zones: "Australia/Perth|Australia/West",
        long: "Australian Western Standard Time|Australian Western Daylight Time|Western Australia Time",
        short: "||",
    },
    Metazone {
        zones: "Pacific/Auckland|Antarctica/McMurdo|Antarctica/South_Pole|NZ",
        long: "New Zealand Standard Time|New Zealand Daylight Time|New Zealand Time",
        short: "||",
    },
    Metazone {
        zones: "Europe/Moscow|Europe/Simferopol|Europe/Kirov|Europe/Volgograd|W-SU",
        long: "Moscow Standard Time|Moscow Summer Time|Moscow Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Jerusalem|Asia/Tel_Aviv|Israel",
        long: "Israel Standard Time|Israel Daylight Time|Israel Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Hong_Kong|Hongkong",
        long: "Hong Kong Standard Time|Hong Kong Summer Time|Hong Kong Time",
        short: "||",
    },
    Metazone {
        zones: "Asia/Singapore|Singapore",
        long: "Singapore Standard Time|Singapore Standard Time|Singapore Standard Time",
        short: "||",
    },
    Metazone {
        zones: "America/Sao_Paulo|Brazil/East",
        long: "Brasilia Standard Time|Brasilia Summer Time|Brasilia Time",
        short: "||",
    },
    Metazone {
        zones: "Pacific/Apia",
        long: "Apia Standard Time|Apia Daylight Time|Apia Time",
        short: "||",
    },
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameStyle {
    Short,
    Long,
    ShortOffset,
    LongOffset,
    ShortGeneric,
    LongGeneric,
}

pub(crate) fn parse_offset(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    let sign = match bytes.first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = |range: core::ops::Range<usize>| -> Option<i32> {
        let part = bytes.get(range)?;
        if part.iter().all(u8::is_ascii_digit) {
            Some(
                part.iter()
                    .fold(0, |sum, &d| sum * 10 + i32::from(d - b'0')),
            )
        } else {
            None
        }
    };
    let hours = digits(1..3)?;
    let minutes = match bytes.len() {
        3 => 0,
        5 => digits(3..5)?,
        6 if bytes[3] == b':' => digits(4..6)?,
        _ => return None,
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 60 + minutes))
}

pub(crate) fn offset_id(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.abs();
    format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

fn plausible_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'_' | b'-' | b'+'))
}

fn temporal_member(scope: &mut Scope<'_>, name: &str) -> Option<Value> {
    let global = scope.global();
    let temporal = scope.get(&global, "Temporal").ok()?;
    if !temporal.is_object() {
        return None;
    }
    scope.get(&temporal, name).ok().filter(Value::is_object)
}

fn zoned(scope: &mut Scope<'_>, zone: &str, epoch_ms: f64) -> Option<Value> {
    let instant = temporal_member(scope, "Instant")?;
    let from = scope.get(&instant, "fromEpochMilliseconds").ok()?;
    let moment = scope
        .call(&from, &instant, &[Value::number(epoch_ms)])
        .ok()?;
    let convert = scope.get(&moment, "toZonedDateTimeISO").ok()?;
    let zone = scope.string(zone);
    scope.call(&convert, &moment, &[zone]).ok()
}

pub(crate) fn is_utc(zone: &str) -> bool {
    zone == "UTC"
}

pub(crate) fn named(scope: &mut Scope<'_>, text: &str) -> Option<String> {
    if !plausible_name(text) {
        return None;
    }
    if text.eq_ignore_ascii_case("UTC") {
        return Some("UTC".to_string());
    }
    if let Some(zoned) = zoned(scope, text, 0.0) {
        let id = scope.get(&zoned, "timeZoneId").ok()?;
        return scope.to_string(&id).ok();
    }
    let local = local_zone_id();
    local.eq_ignore_ascii_case(text).then(|| local.to_string())
}

#[cfg(windows)]
fn windows_zone() -> Option<String> {
    const WEST_EUROPE: [(&str, &str); 10] = [
        ("NO", "Europe/Oslo"),
        ("SE", "Europe/Stockholm"),
        ("DK", "Europe/Copenhagen"),
        ("DE", "Europe/Berlin"),
        ("NL", "Europe/Amsterdam"),
        ("BE", "Europe/Brussels"),
        ("AT", "Europe/Vienna"),
        ("CH", "Europe/Zurich"),
        ("IT", "Europe/Rome"),
        ("ES", "Europe/Madrid"),
    ];
    const WINDOWS_ZONES: [(&str, &str); 26] = [
        ("UTC", "UTC"),
        ("GMT Standard Time", "Europe/London"),
        ("W. Europe Standard Time", "Europe/Berlin"),
        ("Romance Standard Time", "Europe/Paris"),
        ("Central Europe Standard Time", "Europe/Budapest"),
        ("Central European Standard Time", "Europe/Warsaw"),
        ("FLE Standard Time", "Europe/Kyiv"),
        ("E. Europe Standard Time", "Europe/Chisinau"),
        ("Turkey Standard Time", "Europe/Istanbul"),
        ("Russian Standard Time", "Europe/Moscow"),
        ("Israel Standard Time", "Asia/Jerusalem"),
        ("Arabian Standard Time", "Asia/Dubai"),
        ("India Standard Time", "Asia/Kolkata"),
        ("China Standard Time", "Asia/Shanghai"),
        ("Tokyo Standard Time", "Asia/Tokyo"),
        ("Korea Standard Time", "Asia/Seoul"),
        ("AUS Eastern Standard Time", "Australia/Sydney"),
        ("New Zealand Standard Time", "Pacific/Auckland"),
        ("Pacific Standard Time", "America/Los_Angeles"),
        ("Mountain Standard Time", "America/Denver"),
        ("Central Standard Time", "America/Chicago"),
        ("Eastern Standard Time", "America/New_York"),
        ("Atlantic Standard Time", "America/Halifax"),
        ("SA Eastern Standard Time", "America/Sao_Paulo"),
        ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
        ("South Africa Standard Time", "Africa/Johannesburg"),
    ];
    let (key, locale) = ffi::windows::zone()?;
    let key = String::from_utf8_lossy(&key).into_owned();
    let region = locale.as_deref().and_then(|locale| {
        locale
            .iter()
            .rposition(|&c| c == b'-')
            .map(|at| String::from_utf8_lossy(&locale[at + 1..]).into_owned())
    });
    if let Some(region) = region
        && key == "W. Europe Standard Time"
        && let Some((_, zone)) = WEST_EUROPE
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(&region))
    {
        return Some(zone.to_string());
    }
    WINDOWS_ZONES
        .iter()
        .find(|(windows, _)| *windows == key)
        .map(|(_, zone)| zone.to_string())
}

#[cfg(not(windows))]
fn windows_zone() -> Option<String> {
    None
}

fn local_zone_id() -> &'static str {
    static ZONE: OnceLock<String> = OnceLock::new();
    ZONE.get_or_init(|| {
        let usable = |id: &Vec<u8>| !id.is_empty() && id[0] != b'+' && id[0] != b'-';
        let zone = windows_zone()
            .or_else(|| {
                ffi::local_zone_identifier()
                    .filter(usable)
                    .map(|id| String::from_utf8_lossy(&id).into_owned())
            })
            .or_else(|| {
                ffi::getenv(c"TZ")
                    .filter(|tz| usable(tz) && tz.contains(&b'/'))
                    .map(|tz| String::from_utf8_lossy(&tz).into_owned())
            })
            .unwrap_or_else(|| "UTC".to_string());
        if UTC_ALIASES.contains(&zone.as_str()) {
            "UTC".to_string()
        } else {
            zone
        }
    })
}

thread_local! {
    static SYSTEM_ZONE: OnceCell<String> = const { OnceCell::new() };
}

pub(crate) fn system_zone(scope: &mut Scope<'_>) -> String {
    if let Some(zone) = SYSTEM_ZONE.with(|zone| zone.get().cloned()) {
        return zone;
    }
    let local = local_zone_id();
    let zone = if local == "UTC" {
        local.to_string()
    } else {
        named(scope, local).unwrap_or_else(|| "UTC".to_string())
    };
    SYSTEM_ZONE.with(|cached| cached.get_or_init(|| zone.clone()).clone())
}

fn libc_offset_ms(epoch_ms: f64) -> i64 {
    let seconds = (epoch_ms / 1000.0).floor() as i64;
    let Some(local) = ffi::break_down(seconds, false) else {
        return 0;
    };
    let days = days_from_civil(
        i64::from(local.year),
        local.month as u32 + 1,
        local.day as u32,
    );
    let local_seconds = days * 86_400
        + i64::from(local.hour) * 3600
        + i64::from(local.minute) * 60
        + i64::from(local.second);
    (local_seconds - seconds) * 1000
}

pub(crate) fn offset_ms(scope: &mut Scope<'_>, zone: &str, epoch_ms: f64) -> i64 {
    if is_utc(zone) {
        return 0;
    }
    if let Some(minutes) = parse_offset(zone) {
        return i64::from(minutes) * 60_000;
    }
    let offset = zoned(scope, zone, epoch_ms).and_then(|zoned| {
        let value = scope.get(&zoned, "offsetNanoseconds").ok()?;
        scope.to_number(&value).ok()
    });
    match offset {
        Some(nanoseconds) => (nanoseconds / 1_000_000.0).round() as i64,
        None if zone == local_zone_id() => libc_offset_ms(epoch_ms),
        None => 0,
    }
}

pub(crate) fn in_daylight_time(scope: &mut Scope<'_>, zone: &str, year: i64, offset: i64) -> bool {
    if is_utc(zone) || parse_offset(zone).is_some() {
        return false;
    }
    let january = days_from_civil(year, 1, 1) as f64 * 86_400_000.0;
    let july = days_from_civil(year, 7, 1) as f64 * 86_400_000.0;
    let standard = offset_ms(scope, zone, january).min(offset_ms(scope, zone, july));
    offset > standard
}

fn metazone(zone: &str) -> Option<&'static Metazone> {
    METAZONES
        .iter()
        .find(|meta| meta.zones.split('|').any(|known| known == zone))
}

fn gmt(offset_ms: i64, long: bool) -> String {
    let minutes = offset_ms.div_euclid(60_000);
    if minutes == 0 {
        return "GMT".to_string();
    }
    let sign = if minutes < 0 { '-' } else { '+' };
    let (hours, rest) = (minutes.abs() / 60, minutes.abs() % 60);
    if long {
        format!("GMT{sign}{hours:02}:{rest:02}")
    } else if rest == 0 {
        format!("GMT{sign}{hours}")
    } else {
        format!("GMT{sign}{hours}:{rest:02}")
    }
}

pub(crate) fn display(
    zone: &str,
    offset_ms: i64,
    daylight: bool,
    style: NameStyle,
    english: bool,
) -> String {
    let index = match style {
        NameStyle::ShortGeneric | NameStyle::LongGeneric => 2,
        _ => usize::from(daylight),
    };
    let meta = if parse_offset(zone).is_some() {
        None
    } else {
        metazone(zone)
    };
    let utc = meta.is_some_and(|meta| meta.zones.starts_with("UTC|"));
    let named = |list: &'static str| -> Option<String> {
        let name = super::data::pick(list, index);
        (!name.is_empty()).then(|| name.to_string())
    };
    let found = match (style, meta) {
        (NameStyle::Short | NameStyle::ShortGeneric, Some(meta)) if english || utc => {
            named(meta.short)
        }
        (NameStyle::Long | NameStyle::LongGeneric, Some(meta)) if english => named(meta.long),
        _ => None,
    };
    found.unwrap_or_else(|| {
        gmt(
            offset_ms,
            matches!(
                style,
                NameStyle::Long | NameStyle::LongOffset | NameStyle::LongGeneric
            ),
        )
    })
}
