//! Southstar — the cookie jar: Netscape-format files per top-level site, read for requests and document.cookie and written from Set-Cookie headers and scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use southstar_netstore::cookies::{self as rules, Context, Cookie};

use crate::ffi::sys;
use crate::{storage, url};

static JAR_WRITE: Mutex<()> = Mutex::new(());
static JAR_WRITES: AtomicU64 = AtomicU64::new(0);

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn read_jar(path: &[u8]) -> Option<Vec<u8>> {
    sys::read_file(path)
        .ok()
        .map(|contents| until_nul(&contents).to_vec())
}

pub fn collect(target: &[u8], include_httponly: bool) -> Option<Vec<u8>> {
    if target.is_empty() {
        return None;
    }
    let parts = url::parts(target).filter(|p| !p.hostname.is_empty())?;
    let site = url::site_from(target).filter(|s| !s.is_empty())?;
    let jars = [
        storage::cookie_jar_path(Some(&site), false),
        storage::cookie_jar_path(Some(&site), true),
    ];
    collect_from(&parts, &jars, include_httponly, Context::SameSite)
}

pub fn for_js(target: &[u8]) -> Option<Vec<u8>> {
    collect(target, false)
}

pub fn jar_version(target: &[u8]) -> Option<u64> {
    let site = url::site_from(target).filter(|s| !s.is_empty())?;
    let mut hasher = DefaultHasher::new();
    JAR_WRITES.load(Ordering::Acquire).hash(&mut hasher);
    for js in [false, true] {
        storage::cookie_jar_path(Some(&site), js)
            .and_then(|jar| sys::version(&jar))
            .hash(&mut hasher);
    }
    Some(hasher.finish())
}

pub fn collect_in(target: &[u8], jars: &[Option<Vec<u8>>], context: Context) -> Option<Vec<u8>> {
    let parts = url::parts(target).filter(|p| !p.hostname.is_empty())?;
    collect_from(&parts, jars, true, context)
}

fn collect_from(
    parts: &url::Parts,
    jars: &[Option<Vec<u8>>],
    include_httponly: bool,
    context: Context,
) -> Option<Vec<u8>> {
    let now = sys::now_seconds();
    let contents: Vec<Vec<u8>> = jars.iter().flatten().filter_map(|j| read_jar(j)).collect();
    rules::header(
        parts,
        contents.iter().map(Vec::as_slice),
        include_httponly,
        context,
        now,
    )
}

fn parse(target: &[u8], cookie: &[u8]) -> Option<(Cookie, Vec<u8>)> {
    let parsed = rules::parse(target, cookie, sys::now_seconds(), sys::is_public_suffix)?;
    let site = url::site_from(target).filter(|s| !s.is_empty())?;
    Some((parsed, site))
}

pub fn store(target: &[u8], cookie: &[u8], from_http: bool) {
    let Some((c, site)) = parse(target, cookie) else {
        return;
    };
    let Some(jar) = storage::cookie_jar_path(Some(&site), !from_http) else {
        return;
    };
    write_cookie(&jar, from_http, &c, &site);
}

pub fn store_from_js(target: &[u8], cookie: &[u8]) {
    store(target, cookie, false);
}

pub fn store_in(target: &[u8], cookie: &[u8], jar: &[u8], context: Context) {
    if let Some((c, site)) = parse(target, cookie).filter(|(c, _)| rules::settable_in(c, context)) {
        write_cookie(jar, true, &c, &site);
    }
}

fn write_cookie(jar: &[u8], from_http: bool, c: &Cookie, site: &[u8]) {
    let _writing = JAR_WRITE.lock().unwrap_or_else(PoisonError::into_inner);
    let _locked = storage::lock_cookie_jars();
    if !from_http {
        let http_jar = storage::cookie_jar_path(Some(site), false);
        if http_jar
            .and_then(|j| read_jar(&j))
            .is_some_and(|contents| rules::has_live_httponly(&contents, c))
        {
            return;
        }
    }
    let out = rules::store(read_jar(jar).as_deref(), c, from_http);
    if sys::write_file(jar, &out) {
        sys::chmod(jar, 0o600);
    }
    JAR_WRITES.fetch_add(1, Ordering::Release);
}
