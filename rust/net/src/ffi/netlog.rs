//! Southstar — the network log, the fetch counters and connection statistics, and the switch that turns their collection on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_long};
use core::sync::atomic::Ordering;

use crate::netlog::{self, Entry};

const DLOG_NET: c_int = 4;

pub fn set_log_fetches(on: bool) {
    netlog::LOG_FETCHES.store(on, Ordering::Relaxed);
}

pub struct LogRecord<'a> {
    pub method: Option<&'a [u8]>,
    pub url: &'a [u8],
    pub status: c_long,
    pub content_type: Option<&'a [u8]>,
    pub body_len: u64,
    pub duration_ms: f64,
    pub request_headers: Option<&'a [u8]>,
    pub response_headers: Option<&'a [u8]>,
    pub error: Option<&'a [u8]>,
}

pub fn log_record(r: &LogRecord<'_>) {
    if r.url.is_empty() {
        return;
    }
    let owned = |v: Option<&[u8]>| v.map(<[u8]>::to_vec).unwrap_or_default();
    netlog::record(Entry {
        method: r
            .method
            .filter(|m| !m.is_empty())
            .unwrap_or(b"GET")
            .to_vec(),
        url: r.url.to_vec(),
        status: r.status,
        content_type: owned(r.content_type),
        body_len: r.body_len,
        duration_ms: r.duration_ms,
        request_headers: owned(r.request_headers),
        response_headers: owned(r.response_headers),
        error: r.error.filter(|e| !e.is_empty()).map(<[u8]>::to_vec),
    });
}

pub fn ns_net_perf_record(start_us: i64, end_us: i64, bytes: u64) {
    netlog::record_timing(start_us, end_us, bytes);
}

pub fn conn_stat_record(url: &[u8], http_version: c_long, new_connections: c_long) {
    let Some(message) = netlog::record_connection(url, http_version, new_connections) else {
        return;
    };
    southstar_debuglog::emit(DLOG_NET, b"conn", &message);
}
