//! Southstar — the HSTS hosts curl records, cached and reread when the file changes, deciding which http:// URLs go to https:// first.
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

pub fn shutdown() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
