//! Southstar — the HSTS host list's rules over bytes: curl's file format, host lookup with includeSubDomains and expiry, Strict-Transport-Security parsing and rewriting the list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use crate::text::strip;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Host,
    WithSubdomains,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Entry {
    pub scope: Scope,
    pub expires: Option<i64>,
}

pub type Hosts = HashMap<Vec<u8>, Entry>;

const UNLIMITED: &[u8] = b"unlimited";
const LAST_EXPIRY: i64 = 253_402_300_799;

pub fn parse_list(content: &[u8]) -> Hosts {
    let mut hosts = HashMap::new();
    let content = &content[..content
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(content.len())];
    for line in content.split(|&c| c == b'\n') {
        let line = strip(line);
        if line.is_empty() || line[0] == b'#' {
            continue;
        }
        let Some(space) = line.iter().position(|&c| c == b' ') else {
            continue;
        };
        let entry = &line[..space];
        let (host, scope) = match entry.strip_prefix(b".") {
            Some(host) => (host, Scope::WithSubdomains),
            None => (entry, Scope::Host),
        };
        let expiry = strip(&line[space + 1..]);
        let expires = if expiry == UNLIMITED {
            None
        } else {
            let Some(at) = parse_expiry(expiry) else {
                continue;
            };
            Some(at)
        };
        if !host.is_empty() {
            hosts.insert(host.to_ascii_lowercase(), Entry { scope, expires });
        }
    }
    hosts
}

pub fn normalize_host(host: &[u8]) -> Vec<u8> {
    let mut lower = host.to_ascii_lowercase();
    while lower.last() == Some(&b'.') {
        lower.pop();
    }
    lower
}

pub fn lookup(hosts: &Hosts, host: &[u8], now: i64) -> bool {
    let live = |name: &[u8]| {
        hosts
            .get(name)
            .filter(|e| e.expires.is_none_or(|at| at > now))
    };
    if live(host).is_some() {
        return true;
    }
    host.iter()
        .enumerate()
        .filter(|&(_, &c)| c == b'.')
        .any(|(dot, _)| live(&host[dot + 1..]).is_some_and(|e| e.scope == Scope::WithSubdomains))
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn parse_expiry(text: &[u8]) -> Option<i64> {
    let text = text.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
    let (date, time) = (text.get(..8)?, text.get(9..)?);
    if text[8] != b' ' || time.len() != 8 || time[2] != b':' || time[5] != b':' {
        return None;
    }
    let num = |digits: &[u8]| -> Option<i64> {
        digits.iter().try_fold(0i64, |n, &d| {
            d.is_ascii_digit().then(|| n * 10 + i64::from(d - b'0'))
        })
    };
    let (y, m, d) = (num(&date[..4])?, num(&date[4..6])?, num(&date[6..])?);
    let (hh, mm, ss) = (num(&time[..2])?, num(&time[3..5])?, num(&time[6..])?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400 + hh * 3600 + mm * 60 + ss)
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

pub fn expiry_text(at: i64) -> Vec<u8> {
    let (y, m, d) = civil(at.div_euclid(86_400));
    let secs = at.rem_euclid(86_400);
    format!(
        "\"{y:04}{m:02}{d:02} {:02}:{:02}:{:02}\"",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
    .into_bytes()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Policy {
    pub max_age: Option<i64>,
    pub subdomains: bool,
}

pub fn parse_policy(value: &[u8]) -> Policy {
    let mut policy = Policy {
        max_age: None,
        subdomains: false,
    };
    for directive in value.split(|&c| c == b';') {
        let directive = strip(directive);
        let (name, arg) = match directive.iter().position(|&c| c == b'=') {
            Some(eq) => (strip(&directive[..eq]), Some(strip(&directive[eq + 1..]))),
            None => (directive, None),
        };
        if name.eq_ignore_ascii_case(b"max-age") {
            let arg = arg.unwrap_or_default();
            let arg = arg
                .strip_prefix(b"\"")
                .and_then(|a| a.strip_suffix(b"\""))
                .unwrap_or(arg);
            if !arg.is_empty() && arg.iter().all(u8::is_ascii_digit) {
                policy.max_age = Some(arg.iter().fold(0i64, |n, &d| {
                    n.saturating_mul(10).saturating_add(i64::from(d - b'0'))
                }));
            }
        } else if name.eq_ignore_ascii_case(b"includesubdomains") {
            policy.subdomains = true;
        }
    }
    policy
}

pub fn is_ip_literal(host: &[u8]) -> bool {
    host.first() == Some(&b'[') || host.iter().all(|&c| c.is_ascii_digit() || c == b'.')
}

pub fn record(existing: &[u8], host: &[u8], policy: &Policy, max_age: i64, now: i64) -> Vec<u8> {
    let mut out = Vec::new();
    for line in existing.split(|&c| c == b'\n') {
        let trimmed = strip(line);
        if trimmed.is_empty() {
            continue;
        }
        let mut fields = trimmed.splitn(2, |&c| c == b' ');
        let entry = fields.next().unwrap_or_default();
        let entry = entry.strip_prefix(b".").unwrap_or(entry);
        let expired = fields
            .next()
            .and_then(|at| parse_expiry(strip(at)))
            .is_some_and(|at| at <= now);
        if trimmed[0] != b'#' && (expired || entry.eq_ignore_ascii_case(host)) {
            continue;
        }
        out.extend_from_slice(trimmed);
        out.push(b'\n');
    }
    if max_age > 0 {
        if policy.subdomains {
            out.push(b'.');
        }
        out.extend_from_slice(host);
        out.push(b' ');
        out.extend_from_slice(&expiry_text(now.saturating_add(max_age).min(LAST_EXPIRY)));
        out.push(b'\n');
    }
    out
}
