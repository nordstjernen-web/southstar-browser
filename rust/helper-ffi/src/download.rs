//! Southstar — downloading a media URL to a file for the helpers: http and https through rust/http with redirects, size cap and timeout, and data: URLs decoded in place.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::File;
use std::io::Write;
use std::time::Duration;

use southstar_http::{Fetch, Handler, Route, TlsSettings};

struct Sink {
    file: File,
    written: u64,
    max: u64,
    failed: bool,
}

impl Handler for Sink {
    fn should_abort(&self) -> bool {
        false
    }

    fn status_line(&mut self, _line: &[u8]) {}

    fn header(&mut self, _line: &[u8], _name: &[u8], _value: &[u8]) {}

    fn body(&mut self, data: &[u8]) -> bool {
        if self.written + data.len() as u64 > self.max || self.file.write_all(data).is_err() {
            self.failed = true;
            return false;
        }
        self.written += data.len() as u64;
        true
    }
}

fn percent_decode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(s[i + 1]), hex(s[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for &c in input {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\t' | b'\r' | b'\n' | 0x0c => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

pub fn data_url(url: &[u8]) -> Option<Vec<u8>> {
    let rest = url.strip_prefix(b"data:")?;
    let comma = rest.iter().position(|&c| c == b',')?;
    let (meta, payload) = (&rest[..comma], &rest[comma + 1..]);
    let decoded = percent_decode(payload);
    if meta.len() >= 7 && meta[meta.len() - 7..].eq_ignore_ascii_case(b";base64") {
        base64_decode(&decoded)
    } else {
        Some(decoded)
    }
}

fn ca_bundle() -> Option<Vec<u8>> {
    ["CURL_CA_BUNDLE", "SSL_CERT_FILE"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|v| !v.is_empty())
        .map(String::into_bytes)
}

pub fn to_file(url: &[u8], path: &str, max: u64) -> bool {
    let Ok(mut file) = File::create(path) else {
        return false;
    };
    if url.starts_with(b"data:") {
        return data_url(url)
            .filter(|d| d.len() as u64 <= max)
            .is_some_and(|d| file.write_all(&d).is_ok());
    }
    let route = |_: &[u8], _: &str| Route {
        tls: TlsSettings {
            ca_bundle: ca_bundle(),
            curves: b"X25519:P-256:P-384".to_vec(),
        },
        proxy: None,
        allow_insecure: false,
    };
    let request = Fetch {
        url: url.to_vec(),
        method: b"GET",
        body: b"",
        user_agent: Some(b"Southstar-Audio"),
        headers: Vec::new(),
        timeout: Duration::from_secs(30),
        connect_timeout: Duration::from_secs(30),
        max_redirects: 8,
        https_only_redirects: url.starts_with(b"https://"),
        route: &route,
    };
    let mut sink = Sink {
        file,
        written: 0,
        max,
        failed: false,
    };
    let (outcome, _) = southstar_http::fetch(&request, &mut sink);
    outcome.ok && outcome.status < 400 && !sink.failed && sink.file.flush().is_ok()
}
