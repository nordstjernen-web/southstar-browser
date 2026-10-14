//! Southstar — fuzz target: feeds arbitrary server bytes, in fuzzer-chosen pieces, to a client HTTP/2 connection with requests in flight.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use libfuzzer_sys::fuzz_target;
use southstar_http_wire::h2::{Connection, Event};

const REQUEST: [(&[u8], &[u8]); 4] = [
    (b":method", b"POST"),
    (b":scheme", b"https"),
    (b":authority", b"example.test"),
    (b":path", b"/"),
];

fuzz_target!(|data: &[u8]| {
    let (&step, rest) = data.split_first().unwrap_or((&0, &[]));
    let mut connection = Connection::new();
    connection.submit(&REQUEST, vec![0; 70_000]);
    connection.submit(&REQUEST, Vec::new());
    for piece in rest.chunks(usize::from(step).max(1) * 7) {
        if connection.receive(piece).is_err() {
            break;
        }
        while let Some(event) = connection.next_event() {
            if let Event::Headers { stream, .. } = event {
                connection.reset(stream, 8);
            }
        }
        connection.take_output();
        if connection.can_open() {
            connection.submit(&REQUEST, Vec::new());
        }
    }
});
