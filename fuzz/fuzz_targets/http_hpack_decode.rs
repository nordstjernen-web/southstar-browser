//! Southstar — fuzz target: decodes arbitrary bytes as a run of HPACK header blocks sharing one dynamic table, and as a Huffman string.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_http_wire::hpack;

fuzz_target!(|data: &[u8]| {
    let mut decoder = hpack::Decoder::new(4096);
    for block in data.split(|&b| b == 0xff) {
        if decoder.decode(block).is_err() {
            break;
        }
    }
    let _ = hpack::huffman_decode(data);
});
