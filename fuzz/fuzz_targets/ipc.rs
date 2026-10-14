//! Southstar — fuzz target: reads arbitrary bytes from a socket as a renderer-protocol message head and body, and parses the flat JSON bodies.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![no_main]

use std::io::Write;
use std::net::Shutdown;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use libfuzzer_sys::fuzz_target;
use southstar_ipc::{Conn, Head, json_double, json_string, json_value};

fuzz_target!(|data: &[u8]| {
    let (mut writer, reader) = UnixStream::pair().expect("socketpair");
    writer
        .write_all(data)
        .expect("the fuzz input fits the socket buffer");
    writer.shutdown(Shutdown::Write).expect("shutdown");
    let mut conn = Conn::boxed(reader.as_raw_fd());
    let mut head = Head::boxed();
    while conn.read_head(&mut head) {
        let length = u64::try_from(head.content_length).expect("read_head bounds Content-Length");
        if !conn.skip_body(length) {
            break;
        }
    }
    for key in [&b"url"[..], b"x", b"title", b"n"] {
        if let Some(value) = json_value(data, key) {
            let _ = (json_string(value), json_double(value));
        }
    }
});
