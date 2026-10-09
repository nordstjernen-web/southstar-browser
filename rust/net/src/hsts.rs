//! Southstar — the HSTS host list in curl's file format: recording Strict-Transport-Security headers and deciding, from a cache reread when the file changes, which http:// URLs go to https:// first.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ffi::sys::{self, ReadError};
use crate::{storage, url};

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Host,
    WithSubdomains,
}

struct Cache {
    hosts: HashMap<Vec<u8>, Scope>,
    mtime_us: i64,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn strip(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

fn load(path: &[u8]) -> HashMap<Vec<u8>, Scope> {
    let mut hosts = HashMap::new();
    let content = match sys::read_file(path) {
        Ok(content) => content,
        Err(ReadError::Missing) => return hosts,
        Err(ReadError::Other(message)) => {
            sys::warn_read_failure(c"hsts", path, &message);
            return hosts;
        }
    };
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

fn mtime_us(path: &[u8]) -> i64 {
    sys::stat(path).map_or(0, |s| s.mtime.wrapping_mul(1_000_000))
}

fn lookup(hosts: &HashMap<Vec<u8>, Scope>, host: &[u8]) -> bool {
    if hosts.contains_key(host) {
        return true;
    }
    host.iter()
        .enumerate()
        .filter(|&(_, &c)| c == b'.')
        .any(|(dot, _)| hosts.get(&host[dot + 1..]) == Some(&Scope::WithSubdomains))
}

pub fn should_upgrade(host: &[u8]) -> bool {
    if host.is_empty() {
        return false;
    }
    let Some(path) = storage::hsts_path() else {
        return false;
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let mtime = mtime_us(&path);
    if cache.as_ref().is_none_or(|c| c.mtime_us != mtime) {
        *cache = Some(Cache {
            hosts: load(&path),
            mtime_us: mtime,
        });
    }
    let mut lower = host.to_ascii_lowercase();
    while lower.last() == Some(&b'.') {
        lower.pop();
    }
    cache.as_ref().is_some_and(|c| lookup(&c.hosts, &lower))
}

pub fn upgrade(target: &[u8]) -> Option<Vec<u8>> {
    let rest = target.strip_prefix(b"http://")?;
    let host = url::host_from(target)?;
    should_upgrade(&host).then(|| [&b"https://"[..], rest].concat())
}

#[cfg(not(feature = "http-curl"))]
mod recording {
    use super::{CACHE, strip};
    use crate::ffi::sys;
    use crate::storage;

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

    fn expiry_text(at: i64) -> Vec<u8> {
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

    struct Policy {
        max_age: Option<i64>,
        subdomains: bool,
    }

    fn parse_policy(value: &[u8]) -> Policy {
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

    fn is_ip_literal(host: &[u8]) -> bool {
        host.first() == Some(&b'[') || host.iter().all(|&c| c.is_ascii_digit() || c == b'.')
    }

    pub fn record(host: &[u8], value: &[u8]) {
        let mut host = host.to_ascii_lowercase();
        while host.last() == Some(&b'.') {
            host.pop();
        }
        let policy = parse_policy(value);
        let Some(max_age) = policy.max_age else {
            return;
        };
        if host.is_empty() || is_ip_literal(&host) {
            return;
        }
        let Some(path) = storage::hsts_path() else {
            return;
        };
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let existing = sys::read_file(&path).unwrap_or_default();
        let mut out = Vec::new();
        for line in existing.split(|&c| c == b'\n') {
            let trimmed = strip(line);
            if trimmed.is_empty() {
                continue;
            }
            let entry = trimmed.split(|&c| c == b' ').next().unwrap_or_default();
            let entry = entry.strip_prefix(b".").unwrap_or(entry);
            if trimmed[0] != b'#' && entry.eq_ignore_ascii_case(&host) {
                continue;
            }
            out.extend_from_slice(trimmed);
            out.push(b'\n');
        }
        if max_age > 0 {
            if policy.subdomains {
                out.push(b'.');
            }
            out.extend_from_slice(&host);
            out.push(b' ');
            out.extend_from_slice(&expiry_text(sys::now_seconds().saturating_add(max_age)));
            out.push(b'\n');
        }
        sys::write_file(&path, &out);
        *cache = None;
    }
}

#[cfg(not(feature = "http-curl"))]
pub use recording::record;

pub fn shutdown() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
