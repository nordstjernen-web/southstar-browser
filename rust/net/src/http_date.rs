//! Southstar — parsing the dates HTTP headers and cookies carry (RFC 1123, RFC 850, asctime and the looser forms servers send) into seconds since the epoch, with the rules curl_getdate applied.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const WEEKDAYS_LONG: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const ZONES: [(&str, i64); 69] = [
    ("GMT", 0),
    ("UT", 0),
    ("UTC", 0),
    ("WET", 0),
    ("BST", 60),
    ("WAT", -60),
    ("AST", -240),
    ("ADT", -180),
    ("EST", -300),
    ("EDT", -240),
    ("CST", -360),
    ("CDT", -300),
    ("MST", -420),
    ("MDT", -360),
    ("PST", -480),
    ("PDT", -420),
    ("YST", -540),
    ("YDT", -480),
    ("HST", -600),
    ("HDT", -540),
    ("CAT", -600),
    ("AHST", -600),
    ("NT", -660),
    ("IDLW", -720),
    ("CET", 60),
    ("MET", 60),
    ("MEWT", 60),
    ("MEST", 120),
    ("CEST", 120),
    ("MESZ", 120),
    ("FWT", 60),
    ("FST", 120),
    ("EET", 120),
    ("WAST", 420),
    ("WADT", 480),
    ("CCT", 480),
    ("JST", 540),
    ("EAST", 600),
    ("EADT", 660),
    ("GST", 600),
    ("NZT", 720),
    ("NZST", 720),
    ("NZDT", 780),
    ("IDLE", 720),
    ("A", 60),
    ("B", 120),
    ("C", 180),
    ("D", 240),
    ("E", 300),
    ("F", 360),
    ("G", 420),
    ("H", 480),
    ("I", 540),
    ("K", 600),
    ("L", 660),
    ("M", 720),
    ("N", -60),
    ("O", -120),
    ("P", -180),
    ("Q", -240),
    ("R", -300),
    ("S", -360),
    ("T", -420),
    ("U", -480),
    ("V", -540),
    ("W", -600),
    ("X", -660),
    ("Y", -720),
    ("Z", 0),
];

#[derive(PartialEq)]
enum Next {
    Day,
    Year,
}

fn zone(word: &str) -> Option<i64> {
    ZONES
        .iter()
        .find(|(name, _)| *name == word)
        .map(|&(_, minutes)| minutes * 60)
}

fn digits(s: &[u8], at: usize, max: usize) -> Option<(i64, usize)> {
    let n = s[at..]
        .iter()
        .take(max)
        .take_while(|c| c.is_ascii_digit())
        .count();
    (n > 0).then(|| {
        let v = s[at..at + n]
            .iter()
            .fold(0i64, |v, &d| v * 10 + i64::from(d - b'0'));
        (v, at + n)
    })
}

fn clock(s: &[u8], at: usize) -> Option<((i64, i64, i64), usize)> {
    let (h, p) = digits(s, at, 2)?;
    if h >= 24 || s.get(p) != Some(&b':') {
        return None;
    }
    let (m, p) = digits(s, p + 1, 2)?;
    if m >= 60 {
        return None;
    }
    if s.get(p) == Some(&b':') && s.get(p + 1).is_some_and(u8::is_ascii_digit) {
        let (sec, q) = digits(s, p + 1, 2)?;
        return (sec <= 60).then_some(((h, m, sec), q));
    }
    Some(((h, m, 0), p))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn parse(text: &[u8]) -> Option<i64> {
    let (mut weekday, mut month, mut day, mut year) = (false, None, None, None);
    let mut time: Option<(i64, i64, i64)> = None;
    let mut offset: Option<i64> = None;
    let mut next = Next::Day;
    let mut parts = 0;
    let mut i = 0;
    while i < text.len() && parts < 6 {
        let c = text[i];
        if c.is_ascii_alphabetic() {
            let end = i + text[i..]
                .iter()
                .take_while(|c| c.is_ascii_alphabetic())
                .count();
            let word = core::str::from_utf8(&text[i..end]).ok()?;
            let as_weekday = if word.len() == 3 {
                WEEKDAYS.iter().any(|w| w.eq_ignore_ascii_case(word))
            } else {
                WEEKDAYS_LONG.iter().any(|w| w.eq_ignore_ascii_case(word))
            };
            if !weekday && as_weekday {
                weekday = true;
            } else if let Some(m) = (month.is_none() && word.len() == 3)
                .then(|| MONTHS.iter().position(|n| n.eq_ignore_ascii_case(word)))
                .flatten()
            {
                month = Some(m as i64 + 1);
            } else if let Some(z) = offset.is_none().then(|| zone(word)).flatten() {
                offset = Some(z);
            } else {
                return None;
            }
            parts += 1;
            i = end;
        } else if c.is_ascii_digit() {
            if time.is_none()
                && let Some((t, end)) = clock(text, i)
            {
                time = Some(t);
                parts += 1;
                i = end;
                continue;
            }
            let end = i + text[i..].iter().take_while(|c| c.is_ascii_digit()).count();
            let len = end - i;
            let value = text[i..end].iter().try_fold(0i64, |v, &d| {
                v.checked_mul(10)?.checked_add(i64::from(d - b'0'))
            })?;
            let signed = i > 0 && matches!(text[i - 1], b'+' | b'-');
            if offset.is_none() && len == 4 && value <= 1400 && signed {
                let seconds = (value / 100 * 60 + value % 100) * 60;
                offset = Some(if text[i - 1] == b'+' {
                    seconds
                } else {
                    -seconds
                });
            } else if len == 8 && year.is_none() && month.is_none() && day.is_none() {
                year = Some(value / 10_000);
                month = Some(value % 10_000 / 100);
                day = Some(value % 100);
            } else {
                let mut found = false;
                if next == Next::Day && day.is_none() {
                    if value > 0 && value < 32 {
                        day = Some(value);
                        found = true;
                    }
                    next = Next::Year;
                }
                if !found && next == Next::Year && year.is_none() {
                    year = Some(match value {
                        0..=70 => value + 2000,
                        71..=99 => value + 1900,
                        _ => value,
                    });
                    found = true;
                    if day.is_none() {
                        next = Next::Day;
                    }
                }
                if !found {
                    return None;
                }
            }
            parts += 1;
            i = end;
        } else {
            i += 1;
        }
    }
    let (hour, minute, second) = time.unwrap_or((0, 0, 0));
    let (day, month, year) = (day?, month?, year?);
    if day > 31 || year < 1583 || !(1..=12).contains(&month) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let local = days * 86_400 + hour * 3600 + minute * 60 + second;
    Some(local - offset.unwrap_or(0))
}
