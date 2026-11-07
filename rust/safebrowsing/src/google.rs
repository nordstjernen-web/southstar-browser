//! Southstar — Google Safe Browsing v5 in local-list mode: hash-prefix lists kept current on disk, full hashes confirmed online only on a prefix match.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, Mutex, Once, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use southstar_digest::{Algorithm, Hasher};
use southstar_http::{Fetch, Handler};

use crate::protobuf::{self, HashList, Reader, Value};
use crate::{COUNTERS, Verdict, canon, listfile};

const API_HOST: &str = "https://safebrowsing.googleapis.com/";
const API_VERSION: &str = "v5";
const BUILD_API_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/safebrowsing-api-key"));
pub const LISTS: [&str; 3] = ["se-4b", "mw-4b", "uws-4b"];
const MAX_LIST_BYTES: usize = 64 << 20;
const MAX_SEARCH_BYTES: usize = 1 << 20;
const MAX_PREFIXES: usize = 64;
const MIN_WAIT_S: i64 = 300;
const MAX_WAIT_S: i64 = 86_400;
const RETRY_MIN_S: i64 = 900;
const RETRY_MAX_S: i64 = 28_800;
const MAX_CACHED: usize = 4096;

const THREAT_MALWARE: u64 = 1;
const THREAT_SOCIAL_ENGINEERING: u64 = 2;
const THREAT_UNWANTED_SOFTWARE: u64 = 3;
const THREAT_POTENTIALLY_HARMFUL_APPLICATION: u64 = 4;

#[derive(Default)]
struct Cache {
    full_hits: HashMap<[u8; 32], (Verdict, i64)>,
    prefix_misses: HashMap<u32, i64>,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);
static STARTED: Once = Once::new();

fn valid_key(key: &str) -> bool {
    (20..=128).contains(&key.len())
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn api_key() -> Option<String> {
    std::env::var("NS_SAFE_BROWSING_API_KEY")
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| valid_key(key))
        .or_else(|| valid_key(BUILD_API_KEY).then(|| BUILD_API_KEY.to_owned()))
}

pub(crate) fn api_description() -> String {
    format!("Google Safe Browsing {API_VERSION}, local-list mode")
}

pub fn has_key() -> bool {
    api_key().is_some()
}

pub fn available() -> bool {
    crate::enabled() && has_key()
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn query_escape(text: &[u8]) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for &c in text {
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(c));
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    out
}

fn query_b64(data: &[u8]) -> String {
    query_escape(southstar_digest::base64(data).as_bytes())
}

struct Collect {
    body: Vec<u8>,
    max: usize,
    overflow: bool,
}

impl Handler for Collect {
    fn should_abort(&self) -> bool {
        false
    }

    fn status_line(&mut self, _line: &[u8]) {}

    fn header(&mut self, _line: &[u8], _name: &[u8], _value: &[u8]) {}

    fn body(&mut self, data: &[u8]) -> bool {
        if self.body.len() + data.len() > self.max {
            self.overflow = true;
            return false;
        }
        self.body.extend_from_slice(data);
        true
    }
}

fn http_get(url: String, timeout: Duration, max: usize) -> Option<Vec<u8>> {
    let request = Fetch {
        url: url.into_bytes(),
        method: b"GET",
        body: b"",
        user_agent: None,
        headers: Vec::new(),
        timeout,
        connect_timeout: timeout.min(Duration::from_secs(10)),
        max_redirects: 0,
        https_only_redirects: true,
        allow_redirect: &|_| false,
        route: &southstar_net::route::route,
    };
    let mut collect = Collect {
        body: Vec::new(),
        max,
        overflow: false,
    };
    let (outcome, _) = southstar_http::fetch(&request, &mut collect);
    (outcome.ok && outcome.status == 200 && !collect.overflow).then_some(collect.body)
}

fn apply_update(old: &listfile::List, update: &HashList<'_>) -> Option<Vec<u32>> {
    let base: &[u32] = if update.partial { &old.values } else { &[] };
    let additions = update.additions.decode()?;
    let removals = update.removals.decode()?;
    if !removals.is_empty() && !update.partial {
        return None;
    }
    let mut dropped = vec![false; base.len()];
    for index in removals {
        *dropped.get_mut(index as usize)? = true;
    }
    let mut values: Vec<u32> = base
        .iter()
        .zip(&dropped)
        .filter(|(_, dropped)| !**dropped)
        .map(|(value, _)| *value)
        .collect();
    values.extend(additions);
    values.sort_unstable();
    Some(values)
}

fn checksum_matches(values: &[u32], want: Option<&[u8]>) -> bool {
    let Some(want) = want else {
        return true;
    };
    let mut hasher = Hasher::new(Algorithm::Sha256);
    let mut chunk = Vec::with_capacity(4096);
    for value in values {
        chunk.extend_from_slice(&value.to_be_bytes());
        if chunk.len() == 4096 {
            hasher.update(&chunk);
            chunk.clear();
        }
    }
    hasher.update(&chunk);
    hasher.finish() == want
}

fn update_list(name: &str) -> bool {
    let Some(key) = api_key() else {
        return false;
    };
    let old = listfile::load(name).unwrap_or_default();
    let mut url = format!(
        "{API_HOST}{API_VERSION}/hashList/{name}?key={}",
        query_escape(key.as_bytes())
    );
    if !old.version.is_empty() {
        url.push_str("&version=");
        url.push_str(&query_b64(&old.version));
    }
    let Some(body) = http_get(url, Duration::from_secs(180), MAX_LIST_BYTES) else {
        return false;
    };
    let Some(update) = protobuf::hash_list(&body) else {
        return false;
    };
    let Some(values) = apply_update(&old, &update) else {
        return false;
    };
    if !checksum_matches(&values, update.checksum) {
        listfile::remove(name);
        return false;
    }
    let updated_at = now();
    let wait = i64::try_from(update.min_wait_s).unwrap_or(MAX_WAIT_S);
    listfile::save(
        name,
        &listfile::List {
            version: update.version,
            updated_at,
            next_update: updated_at + wait.clamp(MIN_WAIT_S, MAX_WAIT_S),
            values,
        },
    )
}

#[derive(Default)]
struct Backoff {
    retry_at: [i64; LISTS.len()],
    failures: [u32; LISTS.len()],
}

impl Backoff {
    fn due_at(&self, index: usize) -> i64 {
        listfile::header(LISTS[index])
            .map_or(0, |h| h.next_update)
            .max(self.retry_at[index])
    }

    fn record(&mut self, index: usize, ok: bool) {
        if ok {
            self.failures[index] = 0;
            self.retry_at[index] = 0;
        } else {
            let delay = (RETRY_MIN_S << self.failures[index].min(5)).min(RETRY_MAX_S);
            self.retry_at[index] = now() + delay;
            self.failures[index] += 1;
        }
    }

    fn update_due_lists(&mut self) -> i64 {
        if (0..LISTS.len()).any(|i| self.due_at(i) <= now()) {
            let Some(lock) = listfile::try_lock() else {
                return 60;
            };
            for (index, name) in LISTS.iter().enumerate() {
                if self.due_at(index) <= now() {
                    let ok = update_list(name);
                    self.record(index, ok);
                }
            }
            drop(lock);
        }
        let next = (0..LISTS.len())
            .map(|i| self.due_at(i))
            .min()
            .unwrap_or(i64::MAX);
        next.saturating_sub(now())
    }
}

fn updater() {
    let mut backoff = Backoff::default();
    loop {
        let wait = if available() {
            backoff.update_due_lists()
        } else {
            1800
        };
        std::thread::sleep(Duration::from_secs(wait.clamp(60, 1800) as u64));
    }
}

pub fn start() {
    if !available() {
        return;
    }
    STARTED.call_once(|| {
        if let Some(dir) = listfile::dir() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::thread::Builder::new()
            .name("safebrowsing".to_owned())
            .spawn(updater);
    });
}

fn verdict_for_threat(threat: u64) -> Verdict {
    match threat {
        THREAT_MALWARE | THREAT_POTENTIALLY_HARMFUL_APPLICATION => Verdict::Malware,
        THREAT_SOCIAL_ENGINEERING => Verdict::SocialEngineering,
        THREAT_UNWANTED_SOFTWARE => Verdict::UnwantedSoftware,
        _ => Verdict::Safe,
    }
}

fn detail_verdict(data: &[u8]) -> Verdict {
    let mut verdict = Verdict::Safe;
    let mut usable = true;
    let mut reader = Reader::new(data);
    for field in reader.by_ref() {
        match (field.number, field.value) {
            (1, Value::Varint(threat)) => verdict = verdict_for_threat(threat),
            (2, Value::Varint(_)) => usable = false,
            (2, Value::Bytes(bytes)) if !bytes.is_empty() => usable = false,
            _ => {}
        }
    }
    if reader.bad || !usable {
        Verdict::Safe
    } else {
        verdict
    }
}

fn full_hash(data: &[u8]) -> Option<([u8; 32], Verdict)> {
    let mut hash = None;
    let mut verdict = Verdict::Safe;
    let mut reader = Reader::new(data);
    for field in reader.by_ref() {
        match (field.number, field.value) {
            (1, Value::Bytes(bytes)) => hash = <[u8; 32]>::try_from(bytes).ok(),
            (2, Value::Bytes(bytes)) => verdict = verdict.worse(detail_verdict(bytes)),
            _ => {}
        }
    }
    (!reader.bad && verdict != Verdict::Safe)
        .then_some(hash)
        .flatten()
        .map(|hash| (hash, verdict))
}

fn store_search_response(data: &[u8], queried: &[u32]) {
    let mut cache_s = MIN_WAIT_S;
    let mut found = Vec::new();
    let mut reader = Reader::new(data);
    for field in reader.by_ref() {
        match (field.number, field.value) {
            (1, Value::Bytes(bytes)) => found.push(bytes),
            (2, Value::Bytes(bytes)) => {
                if let Some(seconds) = protobuf::duration_seconds(bytes) {
                    cache_s = i64::try_from(seconds).unwrap_or(MAX_WAIT_S).min(MAX_WAIT_S);
                }
            }
            _ => {}
        }
    }
    if reader.bad {
        return;
    }
    let now = now();
    let expires = now + cache_s.clamp(60, MAX_WAIT_S);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.full_hits.len() + cache.prefix_misses.len() > MAX_CACHED {
        cache.full_hits.retain(|_, (_, until)| *until > now);
        cache.prefix_misses.retain(|_, until| *until > now);
        if cache.full_hits.len() + cache.prefix_misses.len() > MAX_CACHED {
            *cache = Cache::default();
        }
    }
    for (hash, verdict) in found.into_iter().filter_map(full_hash) {
        cache.full_hits.insert(hash, (verdict, expires));
    }
    for &prefix in queried {
        cache.prefix_misses.insert(prefix, expires);
    }
}

fn search(prefixes: &[u32]) -> bool {
    let Some(key) = api_key() else {
        return false;
    };
    let mut url = format!(
        "{API_HOST}{API_VERSION}/hashes:search?key={}",
        query_escape(key.as_bytes())
    );
    for prefix in prefixes {
        url.push_str("&hashPrefixes=");
        url.push_str(&query_b64(&prefix.to_be_bytes()));
    }
    COUNTERS.searches.fetch_add(1, Ordering::Relaxed);
    let Some(body) = http_get(url, Duration::from_secs(5), MAX_SEARCH_BYTES) else {
        COUNTERS.search_failures.fetch_add(1, Ordering::Relaxed);
        return false;
    };
    store_search_response(&body, prefixes);
    true
}

fn cached_verdict(hashes: &[[u8; 32]], prefixes: &[u32], hit: &[bool]) -> (Verdict, bool) {
    let now = now();
    let mut verdict = Verdict::Safe;
    let mut need_search = false;
    let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    for ((hash, prefix), _) in hashes
        .iter()
        .zip(prefixes)
        .zip(hit)
        .filter(|(_, hit)| **hit)
    {
        match cache.full_hits.get(hash) {
            Some(&(cached, until)) if until > now => verdict = verdict.worse(cached),
            _ => need_search |= cache.prefix_misses.get(prefix).is_none_or(|&u| u <= now),
        }
    }
    (verdict, need_search)
}

pub(crate) fn check_url(url: &[u8]) -> Verdict {
    let http = url.len() > 7 && url[..7].eq_ignore_ascii_case(b"http://");
    let https = url.len() > 8 && url[..8].eq_ignore_ascii_case(b"https://");
    if !(http || https) || !available() {
        return Verdict::Safe;
    }
    start();
    let mut expressions = canon::expressions(url);
    expressions.truncate(MAX_PREFIXES);
    if expressions.is_empty() {
        return Verdict::Safe;
    }
    let hashes: Vec<[u8; 32]> = expressions
        .iter()
        .filter_map(|e| <[u8; 32]>::try_from(southstar_digest::sha256(e)).ok())
        .collect();
    let prefixes: Vec<u32> = hashes
        .iter()
        .map(|h| u32::from_be_bytes([h[0], h[1], h[2], h[3]]))
        .collect();
    let mut hit = vec![false; prefixes.len()];
    for name in LISTS {
        listfile::mark_hits(name, &prefixes, &mut hit);
    }
    if !hit.contains(&true) {
        return Verdict::Safe;
    }
    COUNTERS.prefix_matches.fetch_add(1, Ordering::Relaxed);
    let (verdict, need_search) = cached_verdict(&hashes, &prefixes, &hit);
    if verdict != Verdict::Safe || !need_search {
        return verdict;
    }
    let mut query: Vec<u32> = Vec::new();
    for (&prefix, _) in prefixes.iter().zip(&hit).filter(|(_, hit)| **hit) {
        if !query.contains(&prefix) {
            query.push(prefix);
        }
    }
    if !search(&query) {
        return Verdict::Safe;
    }
    cached_verdict(&hashes, &prefixes, &hit).0
}
