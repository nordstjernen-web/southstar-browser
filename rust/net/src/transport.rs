//! Southstar — transport bookkeeping: origins that recently refused connections, the six-connection limit per origin, the CA bundle search, the encodings and TLS curves curl supports, and the memory budget for a response.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::ffi::sys;

const DEAD_HOST_TTL_US: i64 = 120 * 1_000_000;
const MAX_PER_ORIGIN: usize = 6;
const SLOT_WAIT: Duration = Duration::from_millis(250);
const MIN_RESPONSE_BUDGET: u64 = 64 * 1024 * 1024;

static DEAD_HOSTS: Mutex<Option<HashMap<Vec<u8>, i64>>> = Mutex::new(None);
static ORIGIN_SLOTS: Mutex<Option<HashMap<Vec<u8>, usize>>> = Mutex::new(None);
static ORIGIN_FREED: Condvar = Condvar::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn host_recently_dead(origin: &[u8]) -> bool {
    let mut dead = lock(&DEAD_HOSTS);
    let Some(hosts) = dead.as_mut() else {
        return false;
    };
    let Some(&expiry) = hosts.get(origin) else {
        return false;
    };
    if sys::monotonic_us() < expiry {
        return true;
    }
    hosts.remove(origin);
    false
}

pub fn mark_host_dead(origin: &[u8]) {
    let expiry = sys::monotonic_us() + DEAD_HOST_TTL_US;
    lock(&DEAD_HOSTS)
        .get_or_insert_with(HashMap::new)
        .insert(origin.to_vec(), expiry);
}

pub fn mark_host_alive(origin: &[u8]) {
    if let Some(hosts) = lock(&DEAD_HOSTS).as_mut() {
        hosts.remove(origin);
    }
}

fn slot_key(origin: &[u8]) -> Option<Vec<u8>> {
    (!origin.is_empty()).then(|| origin.to_ascii_lowercase())
}

pub fn acquire_origin_slot(origin: &[u8], cancelled: impl Fn() -> bool) -> bool {
    let Some(key) = slot_key(origin) else {
        return false;
    };
    let mut slots = lock(&ORIGIN_SLOTS);
    loop {
        let in_use = slots
            .get_or_insert_with(HashMap::new)
            .entry(key.clone())
            .or_insert(0);
        if *in_use < MAX_PER_ORIGIN {
            *in_use += 1;
            return true;
        }
        if cancelled() {
            return false;
        }
        slots = ORIGIN_FREED
            .wait_timeout(slots, SLOT_WAIT)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

pub fn release_origin_slot(origin: &[u8]) {
    let Some(key) = slot_key(origin) else {
        return;
    };
    let mut slots = lock(&ORIGIN_SLOTS);
    if let Some(in_use) = slots
        .as_mut()
        .and_then(|s| s.get_mut(&key))
        .filter(|n| **n > 0)
    {
        *in_use -= 1;
        ORIGIN_FREED.notify_all();
    }
}

pub fn shutdown_origin_slots() {
    *lock(&ORIGIN_SLOTS) = None;
}

pub fn ca_bundle_candidates() -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    if let Some(dir) = sys::exe_dir() {
        for rel in [
            "etc/ssl/certs/ca-bundle.crt",
            "ssl/certs/ca-bundle.crt",
            "ca-bundle.crt",
            "cert.pem",
            "../etc/ca-certificates/cert.pem",
            "../etc/openssl@3/cert.pem",
            "../etc/openssl/cert.pem",
        ] {
            out.push(sys::build_filename(&dir, rel.as_bytes()));
        }
    }
    let system: &[&str] = if cfg!(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "netbsd"
    )) {
        &[
            "/etc/ssl/certs/ca-certificates.crt",
            "/etc/pki/tls/certs/ca-bundle.crt",
            "/etc/ssl/ca-bundle.pem",
            "/var/lib/ca-certificates/ca-bundle.pem",
            "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
            "/etc/ssl/cert.pem",
            "/usr/local/share/certs/ca-root-nss.crt",
        ]
    } else if cfg!(target_vendor = "apple") {
        &[
            "/opt/homebrew/etc/ca-certificates/cert.pem",
            "/opt/homebrew/etc/openssl@3/cert.pem",
            "/usr/local/etc/ca-certificates/cert.pem",
            "/usr/local/etc/openssl@3/cert.pem",
            "/usr/local/etc/openssl/cert.pem",
            "/etc/ssl/cert.pem",
        ]
    } else if cfg!(windows) {
        &[
            "C:/msys64/mingw64/etc/ssl/certs/ca-bundle.crt",
            "C:/msys64/mingw64/etc/ssl/cert.pem",
            "C:/msys64/ucrt64/etc/ssl/certs/ca-bundle.crt",
            "C:/msys64/clang64/etc/ssl/certs/ca-bundle.crt",
        ]
    } else {
        &[]
    };
    out.extend(system.iter().map(|p| p.as_bytes().to_vec()));
    out
}

fn leading_number(text: &[u8]) -> Option<(u32, &[u8])> {
    let digits = text.iter().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let value = text[..digits].iter().fold(0u32, |n, &d| {
        n.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
    });
    Some((value, &text[digits..]))
}

pub fn post_quantum_curves(ssl_version: &[u8]) -> bool {
    let parsed = ssl_version.strip_prefix(b"OpenSSL/").and_then(|rest| {
        let (major, rest) = leading_number(rest)?;
        let (minor, _) = leading_number(rest.strip_prefix(b".")?)?;
        Some((major, minor))
    });
    parsed.is_some_and(|(major, minor)| major > 3 || (major == 3 && minor >= 5))
}

pub fn accept_encoding(zlib: bool, brotli: bool, zstd: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if zlib {
        out.extend_from_slice(b"gzip, deflate");
    }
    for (on, name) in [(brotli, &b"br"[..]), (zstd, b"zstd")] {
        if on {
            if !out.is_empty() {
                out.extend_from_slice(b", ");
            }
            out.extend_from_slice(name);
        }
    }
    out
}

pub fn response_budget() -> u64 {
    let available = sys::available_memory_bytes();
    if available == 0 {
        return MIN_RESPONSE_BUDGET;
    }
    (available / 2).max(MIN_RESPONSE_BUDGET)
}

pub fn serialize_headers<'a>(lines: impl Iterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        if line.len() >= 5 && line[..5].eq_ignore_ascii_case(b"X-ND-") {
            continue;
        }
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    out
}
