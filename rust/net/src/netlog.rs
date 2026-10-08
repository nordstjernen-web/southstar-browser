//! Southstar — the network log behind the developer tools: the last 256 requests with their timing, size and headers, the fetch counters and the per-origin connection statistics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_long};
use core::sync::atomic::{AtomicBool, Ordering};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use crate::url;

const CAPACITY: usize = 256;
const HTTP_1_0: c_long = 1;
const HTTP_1_1: c_long = 2;
const HTTP_2: c_long = 3;
const HTTP_3: c_long = 30;

pub static LOG_FETCHES: AtomicBool = AtomicBool::new(false);

pub struct Entry {
    pub method: Vec<u8>,
    pub url: Vec<u8>,
    pub status: c_long,
    pub content_type: Vec<u8>,
    pub body_len: u64,
    pub duration_ms: f64,
    pub request_headers: Vec<u8>,
    pub response_headers: Vec<u8>,
    pub error: Option<Vec<u8>>,
}

#[derive(Default)]
struct Perf {
    count: u64,
    bytes: u64,
    sum_us: i64,
    first_us: i64,
    last_us: i64,
}

pub struct Snapshot {
    pub fetches: u64,
    pub bytes: u64,
    pub sum_ms: f64,
    pub span_ms: f64,
}

static LOG: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
static PERF: Mutex<Perf> = Mutex::new(Perf {
    count: 0,
    bytes: 0,
    sum_us: 0,
    first_us: 0,
    last_us: 0,
});
type ConnectionStats = HashMap<Vec<u8>, (u64, u64)>;

static CONNECTIONS: Mutex<Option<ConnectionStats>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn logging() -> bool {
    LOG_FETCHES.load(Ordering::Relaxed)
}

pub fn record(entry: Entry) {
    if entry.url.is_empty() || entry.url.starts_with(b"data:") || entry.url.starts_with(b"about:") {
        return;
    }
    let mut log = lock(&LOG);
    if log.len() >= CAPACITY {
        log.pop_front();
    }
    log.push_back(entry);
}

pub fn clear() {
    lock(&LOG).clear();
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn append_headers(out: &mut Vec<u8>, headers: &[u8]) {
    for line in headers.split(|&c| c == b'\n') {
        let line = &line[..line
            .iter()
            .rposition(|&c| !is_space(c))
            .map_or(0, |i| i + 1)];
        if !line.is_empty() {
            out.extend_from_slice(b"    ");
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
}

pub fn dump() -> Vec<u8> {
    let log = lock(&LOG);
    let n = log.len();
    let mut out = format!("{n} network request{}\n\n", if n == 1 { "" } else { "s" }).into_bytes();
    for e in log.iter() {
        if e.status > 0 {
            out.extend_from_slice(format!("[{}] ", e.status).as_bytes());
        } else {
            out.extend_from_slice(b"[---] ");
        }
        out.extend_from_slice(&e.method);
        out.push(b' ');
        out.extend_from_slice(&e.url);
        out.push(b'\n');
        out.extend_from_slice(
            format!("    {:.0} ms, {} bytes", e.duration_ms, e.body_len).as_bytes(),
        );
        if !e.content_type.is_empty() {
            out.extend_from_slice(b", ");
            out.extend_from_slice(&e.content_type);
        }
        out.push(b'\n');
        if let Some(error) = &e.error {
            out.extend_from_slice(b"    error: ");
            out.extend_from_slice(error);
            out.push(b'\n');
        }
        if !e.request_headers.is_empty() {
            out.extend_from_slice(b"  Request headers:\n");
            append_headers(&mut out, &e.request_headers);
        }
        if !e.response_headers.is_empty() {
            out.extend_from_slice(b"  Response headers:\n");
            append_headers(&mut out, &e.response_headers);
        }
        out.push(b'\n');
    }
    out
}

pub fn record_timing(start_us: i64, end_us: i64, bytes: u64) {
    if !logging() {
        return;
    }
    let mut p = lock(&PERF);
    if p.count == 0 || start_us < p.first_us {
        p.first_us = start_us;
    }
    if end_us > p.last_us {
        p.last_us = end_us;
    }
    p.count += 1;
    p.bytes = p.bytes.wrapping_add(bytes);
    p.sum_us = p.sum_us.wrapping_add(end_us - start_us);
}

pub fn snapshot() -> Snapshot {
    let p = lock(&PERF);
    Snapshot {
        fetches: p.count,
        bytes: p.bytes,
        sum_ms: p.sum_us as f64 / 1000.0,
        span_ms: if p.count > 0 {
            (p.last_us - p.first_us) as f64 / 1000.0
        } else {
            0.0
        },
    }
}

pub fn version_name(version: c_long) -> &'static CStr {
    match version {
        HTTP_1_0 => c"http/1.0",
        HTTP_1_1 => c"http/1.1",
        HTTP_2 => c"h2",
        HTTP_3 => c"h3",
        _ => c"http/?",
    }
}

pub fn record_connection(
    target: &[u8],
    version: c_long,
    new_connections: c_long,
) -> Option<Vec<u8>> {
    if !logging() {
        return None;
    }
    let origin = url::origin_from(target)?;
    let (requests, connections) = {
        let mut stats = lock(&CONNECTIONS);
        let entry = stats
            .get_or_insert_with(HashMap::new)
            .entry(origin.clone())
            .or_insert((0, 0));
        entry.0 += 1;
        if new_connections > 0 {
            entry.1 += new_connections as u64;
        }
        *entry
    };
    let mut message = version_name(version).to_bytes().to_vec();
    message.extend_from_slice(format!(" new={new_connections} origin=").as_bytes());
    message.extend_from_slice(&origin);
    message.extend_from_slice(format!(" reqs={requests} conns={connections}").as_bytes());
    Some(message)
}

pub fn shutdown() {
    *lock(&CONNECTIONS) = None;
}
