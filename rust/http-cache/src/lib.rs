//! Southstar — SQLite-indexed HTTP cache with on-disk bodies.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::ffi::{CStr, CString};
use std::sync::Mutex;

use southstar_sqlite::{Db, Step};

const MAX_AGE_SECONDS: i64 = 30 * 24 * 60 * 60;
const SCHEMA_VERSION: i32 = 2;

struct Cache {
    dir: Option<Vec<u8>>,
    db: Option<Db>,
    disabled: bool,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    dir: None,
    db: None,
    disabled: false,
});

fn lock() -> std::sync::MutexGuard<'static, Cache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct Entry {
    pub final_url: Option<Vec<u8>>,
    pub status: i64,
    pub content_type: Option<Vec<u8>>,
    pub cors_allow_origin: Option<Vec<u8>>,
    pub etag: Option<Vec<u8>>,
    pub last_modified: Option<Vec<u8>>,
    pub expires_at: i64,
    pub fetched_at: i64,
    pub body: Vec<u8>,
}

pub struct Response<'a> {
    pub final_url: Option<&'a [u8]>,
    pub status: i64,
    pub content_type: Option<&'a [u8]>,
    pub cors_allow_origin: Option<&'a [u8]>,
    pub etag: Option<&'a [u8]>,
    pub last_modified: Option<&'a [u8]>,
    pub cache_control: Option<&'a [u8]>,
    pub expires_header: Option<&'a [u8]>,
    pub vary: Option<&'a [u8]>,
    pub body: &'a [u8],
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn cap_bytes() -> u64 {
    let mb = southstar_config::get().map_or(256, |c| c.cache_cap_mb);
    let mb = if mb <= 0 { 256 } else { mb };
    mb as u64 * 1024 * 1024
}

fn base_key_for_url(url: &[u8], partition: Option<&[u8]>) -> Vec<u8> {
    let mut parts = vec![url];
    if let Some(partition) = partition.filter(|p| !p.is_empty()) {
        parts.push(b"\x1f");
        parts.push(partition);
    }
    ffi::sha256_hex(&parts)
}

fn row_key_for(base_key: &[u8], selector: &[u8]) -> Vec<u8> {
    if selector.is_empty() {
        return base_key.to_vec();
    }
    ffi::sha256_hex(&[base_key, b"\x1f", selector])
}

fn request_header_value<'a>(headers: &[&'a [u8]], name: &[u8]) -> Option<&'a [u8]> {
    for header in headers {
        if header.len() < name.len() || !header[..name.len()].eq_ignore_ascii_case(name) {
            continue;
        }
        let rest = &header[name.len()..];
        if rest.first() != Some(&b':') {
            continue;
        }
        let value = &rest[1..];
        let skip = value
            .iter()
            .take_while(|&&c| c == b' ' || c == b'\t')
            .count();
        return Some(&value[skip..]);
    }
    None
}

fn vary_selector(vary: Option<&[u8]>, headers: &[&[u8]]) -> Option<Vec<u8>> {
    let Some(vary) = vary.filter(|v| !v.is_empty()) else {
        return Some(Vec::new());
    };
    let mut selector = Vec::new();
    for token in vary.split(|&c| c == b',') {
        let name = token.trim_ascii().to_ascii_lowercase();
        if name.is_empty() || name == b"accept-encoding" {
            continue;
        }
        if !matches!(
            name.as_slice(),
            b"accept" | b"accept-language" | b"user-agent" | b"origin"
        ) {
            return None;
        }
        let value = request_header_value(headers, &name)?;
        selector.extend_from_slice(&name);
        selector.push(b':');
        selector.extend_from_slice(value);
        selector.push(b'\n');
    }
    Some(selector)
}

fn body_path_for_key(dir: &[u8], key: &[u8], ensure_dir: bool) -> Vec<u8> {
    let sub = ffi::build_filename(&[dir, b"bodies", &key[..2.min(key.len())]]);
    if ensure_dir && ffi::mkdir_with_parents(&sub) {
        ffi::restrict_to_owner(&sub, true);
    }
    ffi::build_filename(&[&sub, key.get(2..).unwrap_or_default()])
}

impl Cache {
    fn enabled(&self) -> bool {
        !self.disabled && self.db.is_some()
    }

    fn exec(&self, sql: &CStr) -> bool {
        let Some(db) = &self.db else {
            return false;
        };
        match db.exec(sql) {
            Ok(()) => true,
            Err(message) => {
                ffi::warn(&[b"cache: sqlite exec failed: ".as_slice(), &message].concat());
                false
            }
        }
    }

    fn drop_if_legacy(&self) {
        let Some(st) = self
            .db
            .as_ref()
            .and_then(|db| db.prepare(c"SELECT name FROM pragma_table_info('entries')"))
        else {
            return;
        };
        let (mut table_exists, mut has_body_size) = (false, false);
        while st.step() {
            table_exists = true;
            if st.column_text(0).as_deref() == Some(b"body_size") {
                has_body_size = true;
            }
        }
        drop(st);
        if table_exists && !has_body_size {
            self.exec(c"DROP TABLE entries");
        }
    }

    fn user_version(&self) -> i32 {
        let Some(st) = self
            .db
            .as_ref()
            .and_then(|db| db.prepare(c"PRAGMA user_version"))
        else {
            return -1;
        };
        if st.step() { st.column_int(0) } else { -1 }
    }

    fn schema(&self) -> bool {
        if !self.exec(c"PRAGMA journal_mode=WAL") || !self.exec(c"PRAGMA synchronous=NORMAL") {
            return false;
        }
        if self.user_version() != SCHEMA_VERSION {
            if !self.exec(c"DROP TABLE IF EXISTS entries") {
                return false;
            }
            if let Some(dir) = &self.dir {
                ffi::rmrf(&ffi::build_filename(&[dir, b"bodies"]));
            }
        }
        self.exec(
            c"CREATE TABLE IF NOT EXISTS entries(\
key TEXT PRIMARY KEY,\
base_key TEXT NOT NULL,\
url TEXT NOT NULL,\
final_url TEXT,\
status INTEGER NOT NULL,\
content_type TEXT,\
cors_allow_origin TEXT,\
etag TEXT,\
last_modified TEXT,\
vary TEXT,\
expires_at INTEGER NOT NULL,\
fetched_at INTEGER NOT NULL,\
last_used INTEGER NOT NULL,\
body_size INTEGER NOT NULL DEFAULT 0)",
        ) && self.exec(c"CREATE INDEX IF NOT EXISTS idx_entries_last_used ON entries(last_used)")
            && self.exec(c"CREATE INDEX IF NOT EXISTS idx_entries_base_key ON entries(base_key)")
            && self.exec(c"PRAGMA user_version=2")
    }

    fn find_row_key(&self, base_key: &[u8], headers: &[&[u8]]) -> Option<Vec<u8>> {
        let st = self
            .db
            .as_ref()?
            .prepare(c"SELECT key,vary FROM entries WHERE base_key=?")?;
        st.bind_text(1, Some(&cstring(base_key)));
        while st.step() {
            let stored = st.column_text(0);
            let vary = st.column_text(1);
            let Some(selector) = vary_selector(vary.as_deref(), headers) else {
                continue;
            };
            let want = row_key_for(base_key, &selector);
            if stored.as_deref() == Some(want.as_slice()) {
                return stored;
            }
        }
        None
    }

    fn delete_key(&self, key: &[u8]) {
        if let Some(dir) = &self.dir {
            ffi::unlink(&body_path_for_key(dir, key, false));
        }
        let Some(st) = self
            .db
            .as_ref()
            .and_then(|db| db.prepare(c"DELETE FROM entries WHERE key=?"))
        else {
            return;
        };
        st.bind_text(1, Some(&cstring(key)));
        st.step();
    }

    fn touch_key(&self, key: &[u8]) {
        let Some(st) = self
            .db
            .as_ref()
            .and_then(|db| db.prepare(c"UPDATE entries SET last_used=? WHERE key=?"))
        else {
            return;
        };
        st.bind_int64(1, ffi::now_seconds());
        st.bind_text(2, Some(&cstring(key)));
        st.step();
    }

    fn evict_by_select(&self, sql: &CStr, bind_value: i64) {
        let Some(db) = &self.db else {
            return;
        };
        let Some(st) = db.prepare(sql) else {
            return;
        };
        st.bind_int64(1, bind_value);
        let mut keys = Vec::new();
        while st.step() {
            if let Some(key) = st.column_text(0) {
                keys.push(key);
            }
        }
        drop(st);
        for key in keys {
            self.delete_key(&key);
        }
    }

    fn evict_to_cap(&self) {
        if self.db.is_none() {
            return;
        }
        self.evict_by_select(
            c"SELECT key FROM entries WHERE last_used < ?",
            ffi::now_seconds() - MAX_AGE_SECONDS,
        );
        self.evict_by_select(
            c"SELECT key FROM (SELECT key, SUM(body_size) OVER (ORDER BY last_used DESC, key DESC) AS running FROM entries) WHERE running > ?",
            cap_bytes() as i64,
        );
    }
}

pub fn init() {
    let mut cache = lock();
    if southstar_config::get().is_some()
        && (!southstar_config::cache_enabled() || southstar_config::private_mode())
    {
        cache.disabled = true;
        return;
    }
    let dir = ffi::build_filename(&[&ffi::user_cache_dir(), b"southstar", b"cache"]);
    ffi::mkdir_with_parents(&dir);
    ffi::restrict_to_owner(&dir, true);
    cache.dir = Some(dir.clone());
    let db_path = ffi::build_filename(&[&dir, b"http-cache.sqlite"]);
    let db = match Db::open(&cstring(&db_path)) {
        Ok(db) => db,
        Err(message) => {
            ffi::warn(
                &[
                    b"cache: could not open ".as_slice(),
                    &db_path,
                    b": ",
                    &message,
                ]
                .concat(),
            );
            cache.disabled = true;
            return;
        }
    };
    db.harden();
    db.busy_timeout(2500);
    cache.db = Some(db);
    cache.drop_if_legacy();
    if !cache.schema() {
        cache.db = None;
        cache.disabled = true;
        return;
    }
    ffi::restrict_to_owner(&db_path, false);
    cache.evict_to_cap();
}

pub fn shutdown() {
    let mut cache = lock();
    cache.db = None;
    cache.dir = None;
}

pub fn clear() {
    let cache = lock();
    if cache.db.is_some() {
        cache.exec(c"DELETE FROM entries");
    }
    if let Some(dir) = &cache.dir {
        ffi::rmrf(&ffi::build_filename(&[dir, b"bodies"]));
    }
}

fn is_cacheable_status(status: i64) -> bool {
    matches!(status, 200 | 203 | 301 | 410)
}

pub fn get(url: &[u8], partition: Option<&[u8]>, headers: &[&[u8]]) -> Option<Entry> {
    let cache = lock();
    if !cache.enabled() {
        return None;
    }
    let base = base_key_for_url(url, partition);
    let key = cache.find_row_key(&base, headers)?;
    let st = cache.db.as_ref()?.prepare(
        c"SELECT final_url,status,content_type,cors_allow_origin,etag,last_modified,expires_at,fetched_at,body_size FROM entries WHERE key=?",
    )?;
    st.bind_text(1, Some(&cstring(&key)));
    if !st.step() {
        return None;
    }
    let status = st.column_int64(1);
    if !is_cacheable_status(status) {
        drop(st);
        cache.delete_key(&key);
        return None;
    }
    let body_size = st.column_int64(8);
    if body_size < 0 || body_size as u64 > cap_bytes() {
        return None;
    }
    let mut entry = Entry {
        final_url: st.column_text(0),
        status,
        content_type: st.column_text(2),
        cors_allow_origin: st.column_text(3),
        etag: st.column_text(4),
        last_modified: st.column_text(5),
        expires_at: st.column_int64(6),
        fetched_at: st.column_int64(7),
        body: Vec::new(),
    };
    if entry.final_url.is_none() {
        entry.final_url = Some(url.to_vec());
    }
    drop(st);
    let dir = cache.dir.as_deref().unwrap_or_default();
    let body_path = body_path_for_key(dir, &key, false);
    let readable = ffi::file_size(&body_path).is_some_and(|size| size <= cap_bytes());
    let Some(body) = readable.then(|| ffi::read_file(&body_path)).flatten() else {
        cache.delete_key(&key);
        return None;
    };
    if body.len() as u64 > u64::from(u32::MAX) || body.len() as u64 > cap_bytes() {
        cache.delete_key(&key);
        return None;
    }
    entry.body = body;
    cache.touch_key(&key);
    Some(entry)
}

pub fn is_fresh(expires_at: i64) -> bool {
    expires_at > ffi::now_seconds()
}

fn freshness(cache_control: Option<&[u8]>, expires_header: Option<&[u8]>) -> i64 {
    if let Some(cc) = cache_control {
        if contains(cc, b"no-store") {
            return -1;
        }
        if contains(cc, b"no-cache") {
            return 0;
        }
        if let Some(at) = find(cc, b"max-age") {
            if let Some(eq) = cc[at..].iter().position(|&c| c == b'=') {
                let ma = ffi::ascii_strtoll(&cc[at + eq + 1..]).clamp(0, 86400 * 3650);
                return ffi::now_seconds() + ma;
            }
        }
        if contains(cc, b"immutable") {
            return ffi::now_seconds() + 86400 * 30;
        }
    }
    if let Some(expires) = expires_header {
        let t = ffi::parse_http_date(expires);
        if t > 0 {
            return t;
        }
    }
    0
}

fn url_should_cache(url: &[u8]) -> bool {
    ffi::url_is_http_or_https(url) && !url.iter().any(|&c| c < 0x20 || c == 0x7f)
}

pub fn put(url: &[u8], partition: Option<&[u8]>, response: &Response<'_>, headers: &[&[u8]]) {
    let cache = lock();
    if !cache.enabled() || !url_should_cache(url) {
        return;
    }
    if response
        .cache_control
        .is_some_and(|cc| contains(cc, b"no-store") || contains(cc, b"private"))
    {
        return;
    }
    let body_len = response.body.len();
    if !is_cacheable_status(response.status)
        || body_len > i32::MAX as usize
        || body_len as u64 > cap_bytes()
    {
        return;
    }
    let expires_at = freshness(response.cache_control, response.expires_header);
    if expires_at < 0 {
        return;
    }
    let Some(selector) = vary_selector(response.vary, headers) else {
        return;
    };
    let base = base_key_for_url(url, partition);
    let key = row_key_for(&base, &selector);
    let Some(st) = cache.db.as_ref().and_then(|db| {
        db.prepare(
            c"INSERT INTO entries(key,base_key,url,final_url,status,content_type,cors_allow_origin,etag,last_modified,vary,expires_at,fetched_at,last_used,body_size) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(key) DO UPDATE SET url=excluded.url,final_url=excluded.final_url,status=excluded.status,content_type=excluded.content_type,cors_allow_origin=excluded.cors_allow_origin,etag=excluded.etag,last_modified=excluded.last_modified,vary=excluded.vary,expires_at=excluded.expires_at,fetched_at=excluded.fetched_at,last_used=excluded.last_used,body_size=excluded.body_size",
        )
    }) else {
        return;
    };
    let now = ffi::now_seconds();
    let text = |value: Option<&[u8]>| value.map(cstring);
    st.bind_text(1, Some(&cstring(&key)));
    st.bind_text(2, Some(&cstring(&base)));
    st.bind_text(3, Some(&cstring(url)));
    st.bind_text(4, Some(&cstring(response.final_url.unwrap_or(url))));
    st.bind_int64(5, response.status);
    st.bind_text(6, text(response.content_type).as_deref());
    st.bind_text(7, text(response.cors_allow_origin).as_deref());
    st.bind_text(8, text(response.etag).as_deref());
    st.bind_text(9, text(response.last_modified).as_deref());
    st.bind_text(10, text(response.vary).as_deref());
    st.bind_int64(11, expires_at);
    st.bind_int64(12, now);
    st.bind_int64(13, now);
    st.bind_int64(14, body_len as i64);
    let done = st.step_result() == Step::Done;
    drop(st);
    if !done {
        return;
    }
    let dir = cache.dir.as_deref().unwrap_or_default();
    let body_path = body_path_for_key(dir, &key, true);
    if let Err(message) = ffi::write_file_consistent(&body_path, response.body) {
        ffi::warn(
            &[
                b"cache: failed to write ".as_slice(),
                &body_path,
                b": ",
                &message,
            ]
            .concat(),
        );
        cache.delete_key(&key);
        return;
    }
    cache.evict_to_cap();
}

pub fn promote_304(
    url: &[u8],
    partition: Option<&[u8]>,
    headers: &[&[u8]],
    cache_control: Option<&[u8]>,
    expires_header: Option<&[u8]>,
) {
    let cache = lock();
    if !cache.enabled() || !url_should_cache(url) {
        return;
    }
    let base = base_key_for_url(url, partition);
    let Some(key) = cache.find_row_key(&base, headers) else {
        return;
    };
    let Some(db) = &cache.db else {
        return;
    };
    let Some(st) = db.prepare(c"SELECT expires_at FROM entries WHERE key=?") else {
        return;
    };
    st.bind_text(1, Some(&cstring(&key)));
    if !st.step() {
        return;
    }
    let old_expires = st.column_int64(0);
    drop(st);
    let expires_at = freshness(cache_control, expires_header).max(0);
    if expires_at <= old_expires {
        return;
    }
    let Some(st) =
        db.prepare(c"UPDATE entries SET expires_at=?,fetched_at=?,last_used=? WHERE key=?")
    else {
        return;
    };
    let now = ffi::now_seconds();
    st.bind_int64(1, expires_at);
    st.bind_int64(2, now);
    st.bind_int64(3, now);
    st.bind_text(4, Some(&cstring(&key)));
    st.step();
}
