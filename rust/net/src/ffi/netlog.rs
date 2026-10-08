//! Southstar — the C ABI of the network log, the fetch counters and connection statistics, and the switch that turns their collection on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long};
use core::sync::atomic::Ordering;

use southstar_glib::{self as glib, GBoolean};

use crate::netlog::{self, Entry};

const DLOG_NET: c_int = 4;

unsafe extern "C" {
    fn ns_debug_log_emit_take(level: c_int, category: *const c_char, message: *mut c_char);
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn owned(p: *const c_char) -> Vec<u8> {
    text(p).map(<[u8]>::to_vec).unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_set_log_fetches(on: GBoolean) {
    netlog::LOG_FETCHES.store(on != 0, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_log_fetches_enabled() -> GBoolean {
    glib::boolean(netlog::logging())
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_net_log_record(
    method: *const c_char,
    url: *const c_char,
    status: c_long,
    content_type: *const c_char,
    body_len: u64,
    duration_ms: f64,
    req_headers: *const c_char,
    resp_headers: *const c_char,
    error: *const c_char,
) {
    let Some(url) = text(url).filter(|u| !u.is_empty()) else {
        return;
    };
    netlog::record(Entry {
        method: text(method)
            .filter(|m| !m.is_empty())
            .unwrap_or(b"GET")
            .to_vec(),
        url: url.to_vec(),
        status,
        content_type: owned(content_type),
        body_len,
        duration_ms,
        request_headers: owned(req_headers),
        response_headers: owned(resp_headers),
        error: text(error).filter(|e| !e.is_empty()).map(<[u8]>::to_vec),
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_log_clear() {
    netlog::clear();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_log_dump() -> *mut c_char {
    glib::strdup(&netlog::dump())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_perf_record(start_us: i64, end_us: i64, bytes: u64) {
    netlog::record_timing(start_us, end_us, bytes);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_perf_snapshot(
    fetches: *mut u64,
    bytes: *mut u64,
    sum_ms: *mut f64,
    span_ms: *mut f64,
) {
    let s = netlog::snapshot();
    unsafe {
        if let Some(f) = fetches.as_mut() {
            *f = s.fetches;
        }
        if let Some(b) = bytes.as_mut() {
            *b = s.bytes;
        }
        if let Some(m) = sum_ms.as_mut() {
            *m = s.sum_ms;
        }
        if let Some(m) = span_ms.as_mut() {
            *m = s.span_ms;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_conn_stat_record(
    url: *const c_char,
    http_version: c_long,
    new_connections: c_long,
) {
    let Some(message) =
        text(url).and_then(|u| netlog::record_connection(u, http_version, new_connections))
    else {
        return;
    };
    unsafe { ns_debug_log_emit_take(DLOG_NET, c"conn".as_ptr(), glib::strdup(&message)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_http_version_name(version: c_long) -> *const c_char {
    netlog::version_name(version).as_ptr()
}
