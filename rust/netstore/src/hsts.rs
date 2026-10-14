//! Southstar — the HSTS host list's rules over bytes: curl's file format, host lookup with includeSubDomains, Strict-Transport-Security parsing and rewriting the list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use crate::text::strip;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    Host,
    WithSubdomains,
}

pub type Hosts = HashMap<Vec<u8>, Scope>;

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
        if !host.is_empty() {
            hosts.insert(host.to_ascii_lowercase(), scope);
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

pub fn lookup(hosts: &Hosts, host: &[u8]) -> bool {
    if hosts.contains_key(host) {
        return true;
    }
    host.iter()
        .enumerate()
        .filter(|&(_, &c)| c == b'.')
        .any(|(dot, _)| hosts.get(&host[dot + 1..]) == Some(&Scope::WithSubdomains))
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
        let entry = trimmed.split(|&c| c == b' ').next().unwrap_or_default();
        let entry = entry.strip_prefix(b".").unwrap_or(entry);
        if trimmed[0] != b'#' && entry.eq_ignore_ascii_case(host) {
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
        out.extend_from_slice(&expiry_text(now.saturating_add(max_age)));
        out.push(b'\n');
    }
    out
}
