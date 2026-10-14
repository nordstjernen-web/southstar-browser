//! Southstar — fuzz target: parses arbitrary bytes as an HTTP/1.1 response head and a chunked body fed in fuzzer-chosen pieces.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_http_wire::h1;

fuzz_target!(|data: &[u8]| {
    let (head, body) = match data.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(end) => (&data[..end], &data[end + 4..]),
        None => (data, &[][..]),
    };
    let mut lines = head.split(|&b| b == b'\n');
    if let Some(status) = lines.next() {
        let _ = h1::status_code(status);
    }
    for line in lines {
        if let Some((name, value)) = h1::split_header(h1::until_nul(line)) {
            let _ = h1::strtoll(value, if name.len() % 2 == 0 { 10 } else { 16 });
        }
    }
    let (&step, body) = body.split_first().unwrap_or((&0, &[]));
    let mut chunked = h1::Chunked::new();
    let mut out = Vec::new();
    for piece in body.chunks(usize::from(step).max(1) * 7) {
        if chunked.feed(piece, &mut out).is_err() || chunked.done() {
            break;
        }
    }
    assert!(out.len() <= body.len());
});
