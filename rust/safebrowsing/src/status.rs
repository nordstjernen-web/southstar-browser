//! Southstar — the Safe Browsing rows on about:southstar: whether it is on, the lists on disk and this process's counters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::atomic::Ordering;

use crate::{COUNTERS, google, listfile};

fn civil_date(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn timestamp(seconds: i64) -> String {
    let (year, month, day) = civil_date(seconds.div_euclid(86_400));
    let in_day = seconds.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        in_day / 3600,
        in_day / 60 % 60
    )
}

fn list_label(name: &str) -> &'static str {
    match name {
        "se-4b" => "List se-4b (social engineering)",
        "mw-4b" => "List mw-4b (malware)",
        _ => "List uws-4b (unwanted software)",
    }
}

fn list_row(name: &str) -> String {
    let Some(header) = listfile::header(name) else {
        return String::from("Not downloaded");
    };
    let updated = if header.updated_at > 0 {
        timestamp(header.updated_at)
    } else {
        String::from("unknown")
    };
    let due = if header.next_update <= google::now() {
        String::from("now")
    } else {
        timestamp(header.next_update)
    };
    format!(
        "{} prefixes \u{b7} updated {updated} \u{b7} next update {due}",
        header.count
    )
}

fn count(counter: &std::sync::atomic::AtomicU64) -> String {
    counter.load(Ordering::Relaxed).to_string()
}

pub fn rows() -> Vec<(&'static str, String)> {
    let status = if !crate::enabled() {
        "Off in Settings"
    } else if google::has_key() {
        "On"
    } else {
        "No API key in this build \u{2014} local list only"
    };
    let mut rows = vec![("Status", String::from(status))];
    if google::available() {
        rows.push(("API", google::api_description()));
    }
    rows.push((
        "Local blocklist",
        format!("{} hosts", crate::local_host_count()),
    ));
    for name in google::LISTS {
        rows.push((list_label(name), list_row(name)));
    }
    rows.extend([
        ("URLs checked", count(&COUNTERS.checked)),
        ("Local-list blocks", count(&COUNTERS.local_blocks)),
        ("Prefix matches", count(&COUNTERS.prefix_matches)),
        (
            "Full-hash lookups sent to Google",
            format!(
                "{} ({} failed)",
                count(&COUNTERS.searches),
                count(&COUNTERS.search_failures)
            ),
        ),
        ("Google blocks", count(&COUNTERS.google_blocks)),
    ]);
    rows
}
