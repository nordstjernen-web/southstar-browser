//! Southstar — the HSTS host list in curl's file format: recording Strict-Transport-Security headers and deciding, from a cache reread when the file changes, which http:// URLs go to https:// first until their entries expire.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::Mutex;

use southstar_netstore::hsts::{self as rules, Hosts};

use crate::ffi::sys::{self, ReadError};
use crate::{storage, url};

struct Cache {
    hosts: Hosts,
    mtime_us: i64,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn load(path: &[u8]) -> Hosts {
    match sys::read_file(path) {
        Ok(content) => rules::parse_list(&content),
        Err(ReadError::Missing) => Hosts::new(),
        Err(ReadError::Other(message)) => {
            sys::warn_read_failure(c"hsts", path, &message);
            Hosts::new()
        }
    }
}

fn mtime_us(path: &[u8]) -> i64 {
    sys::stat(path).map_or(0, |s| s.mtime.wrapping_mul(1_000_000))
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
    let host = rules::normalize_host(host);
    cache
        .as_ref()
        .is_some_and(|c| rules::lookup(&c.hosts, &host, sys::now_seconds()))
}

pub fn upgrade(target: &[u8]) -> Option<Vec<u8>> {
    let rest = target.strip_prefix(b"http://")?;
    let host = url::host_from(target)?;
    should_upgrade(&host).then(|| [&b"https://"[..], rest].concat())
}

pub fn record(host: &[u8], value: &[u8]) {
    let host = rules::normalize_host(host);
    let policy = rules::parse_policy(value);
    let Some(max_age) = policy.max_age else {
        return;
    };
    if host.is_empty() || rules::is_ip_literal(&host) {
        return;
    }
    let Some(path) = storage::hsts_path() else {
        return;
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let existing = sys::read_file(&path).unwrap_or_default();
    let out = rules::record(&existing, &host, &policy, max_age, sys::now_seconds());
    sys::write_file(&path, &out);
    *cache = None;
}

pub fn shutdown() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}
