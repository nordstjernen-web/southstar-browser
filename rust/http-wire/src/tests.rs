//! Southstar — unit tests for HPACK against RFC 7541 Appendix C, the chunked coding, HTTP/1.1 header lines and an HTTP/2 exchange.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::h2::{Connection, Event};
use crate::{frame, h1, hpack};

fn hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text.bytes().filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).expect("ascii"), 16).expect("hex"))
        .collect()
}

fn fields(list: &[(&str, &str)]) -> Vec<hpack::Field> {
    list.iter()
        .map(|(n, v)| (n.as_bytes().to_vec(), v.as_bytes().to_vec()))
        .collect()
}

fn decode_requests(blocks: [&str; 3]) {
    let mut decoder = hpack::Decoder::new(4096);
    let first = [
        (":method", "GET"),
        (":scheme", "http"),
        (":path", "/"),
        (":authority", "www.example.com"),
    ];
    assert_eq!(
        decoder.decode(&hex(blocks[0])).expect("C.x.1"),
        fields(&first)
    );
    let second = [
        (":method", "GET"),
        (":scheme", "http"),
        (":path", "/"),
        (":authority", "www.example.com"),
        ("cache-control", "no-cache"),
    ];
    assert_eq!(
        decoder.decode(&hex(blocks[1])).expect("C.x.2"),
        fields(&second)
    );
    let third = [
        (":method", "GET"),
        (":scheme", "https"),
        (":path", "/index.html"),
        (":authority", "www.example.com"),
        ("custom-key", "custom-value"),
    ];
    assert_eq!(
        decoder.decode(&hex(blocks[2])).expect("C.x.3"),
        fields(&third)
    );
}

#[test]
fn hpack_decodes_rfc7541_c3_requests_without_huffman() {
    decode_requests([
        "8286 8441 0f77 7777 2e65 7861 6d70 6c65 2e63 6f6d",
        "8286 84be 5808 6e6f 2d63 6163 6865",
        "8287 85bf 400a 6375 7374 6f6d 2d6b 6579 0c63 7573 746f 6d2d 7661 6c75 65",
    ]);
}

#[test]
fn hpack_decodes_rfc7541_c4_requests_with_huffman() {
    decode_requests([
        "8286 8441 8cf1 e3c2 e5f2 3a6b a0ab 90f4 ff",
        "8286 84be 5886 a8eb 1064 9cbf",
        "8287 85bf 4088 25a8 49e9 5ba9 7d7f 8925 a849 e95b b8e8 b4bf",
    ]);
}

#[test]
fn hpack_huffman_and_errors() {
    assert_eq!(
        hpack::huffman_decode(&hex("f1e3 c2e5 f23a 6ba0 ab90 f4ff")).expect("C.4.1"),
        b"www.example.com"
    );
    assert!(hpack::huffman_decode(&[0xff, 0xff, 0xff, 0xff]).is_err());
    let mut decoder = hpack::Decoder::new(4096);
    assert!(decoder.decode(&[0x80]).is_err());
    assert!(decoder.decode(&[0xbe]).is_err());
    assert!(
        decoder
            .decode(&[0x3f, 0xe1, 0xff, 0xff, 0xff, 0x0f])
            .is_err()
    );
}

#[test]
fn hpack_encoder_round_trips() {
    let headers: [(&[u8], &[u8]); 3] = [
        (b":method", b"GET"),
        (b"accept", b"*/*"),
        (b"x-long", &[b'a'; 300]),
    ];
    let block = hpack::encode(&headers);
    let decoded = hpack::Decoder::new(4096)
        .decode(&block)
        .expect("own encoding decodes");
    let expected: Vec<hpack::Field> = headers
        .iter()
        .map(|(n, v)| (n.to_vec(), v.to_vec()))
        .collect();
    assert_eq!(decoded, expected);
}

#[test]
fn chunked_body_in_any_split() {
    let wire =
        b"4\r\nWiki\r\n5;ext=1\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\nTrailer: x\r\n\r\n";
    for step in 1..wire.len() {
        let mut chunked = h1::Chunked::new();
        let mut out = Vec::new();
        for piece in wire.chunks(step) {
            chunked.feed(piece, &mut out).expect("valid chunked body");
        }
        assert!(chunked.done(), "step {step}");
        assert_eq!(out, b"Wikipedia in\r\n\r\nchunks.");
    }
}

#[test]
fn chunked_rejects_an_endless_size_line() {
    let mut chunked = h1::Chunked::new();
    assert!(chunked.feed(&[b'1'; 5000], &mut Vec::new()).is_err());
}

#[test]
fn http1_lines() {
    assert_eq!(h1::status_code(b"HTTP/1.1 404 Not Found"), Some(404));
    assert_eq!(h1::status_code(b"garbage"), None);
    assert_eq!(
        h1::split_header(b"Content-Type:  text/html"),
        Some((&b"Content-Type"[..], &b"text/html"[..]))
    );
    assert_eq!(h1::split_header(b"no colon"), None);
    assert_eq!(h1::strtoll(b" 0x1F", 16), 31);
    assert_eq!(h1::strtoll(b"-12abc", 10), -12);
    assert_eq!(h1::strtoll(b"99999999999999999999999", 10), i64::MAX);
    assert_eq!(h1::until_nul(b"ab\0cd"), b"ab");
}

fn server_frame(kind: u8, flags: u8, stream: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    frame::write(&mut out, kind, flags, stream, payload);
    out
}

#[test]
fn http2_request_response() {
    let mut connection = Connection::new();
    let request: [(&[u8], &[u8]); 4] = [
        (b":method", b"GET"),
        (b":scheme", b"https"),
        (b":authority", b"example.test"),
        (b":path", b"/"),
    ];
    let stream = connection
        .submit(&request, Vec::new())
        .expect("a stream opens");
    assert_eq!(stream, 1);
    assert!(connection.take_output().starts_with(frame::PREFACE));

    let mut input = server_frame(frame::SETTINGS, 0, 0, &[]);
    input.extend(server_frame(
        frame::HEADERS,
        frame::FLAG_END_HEADERS,
        1,
        &hex("88"),
    ));
    input.extend(server_frame(
        frame::DATA,
        frame::FLAG_END_STREAM,
        1,
        b"hello",
    ));
    let (first, second) = input.split_at(13);
    connection.receive(first).expect("partial frames buffer");
    connection.receive(second).expect("valid frames");

    let mut events = Vec::new();
    while let Some(event) = connection.next_event() {
        events.push(event);
    }
    assert!(
        matches!(&events[0], Event::Headers { stream: 1, fields } if fields == &fields_of(&[(":status", "200")]))
    );
    assert!(matches!(&events[1], Event::Data { stream: 1, bytes } if bytes == b"hello"));
    let output = connection.take_output();
    let ack = server_frame(frame::SETTINGS, frame::FLAG_ACK, 0, &[]);
    assert!(output.windows(ack.len()).any(|w| w == ack.as_slice()));
}

fn fields_of(list: &[(&str, &str)]) -> Vec<(Vec<u8>, Vec<u8>)> {
    fields(list)
}

#[test]
fn http2_connection_errors() {
    let mut connection = Connection::new();
    let oversized = [0x00, 0x40, 0x01, frame::DATA, 0, 0, 0, 0, 1];
    assert!(connection.receive(&oversized).is_err());
    assert!(connection.goaway());

    let mut connection = Connection::new();
    assert!(
        connection
            .receive(&server_frame(frame::PUSH_PROMISE, 0, 1, &[0, 0, 0, 2]))
            .is_err()
    );

    let mut connection = Connection::new();
    assert!(
        connection
            .receive(&server_frame(frame::PING, 0, 0, &[1, 2, 3]))
            .is_err()
    );
}

#[test]
fn request_head_drops_header_lines_that_would_split() {
    let head = h1::request_head(
        b"GET",
        b"/",
        b"example.com",
        &[(b"User-Agent", b"ua\r\nX-Injected: 1"), (b"Accept", b"*/*")],
        &[b"Last-Event-ID: 1\nX-Injected: 2", b"X-Fine: 3"],
        0,
        true,
    );
    let text = String::from_utf8(head).expect("ascii");
    assert!(!text.contains("X-Injected"));
    assert!(text.contains("Accept: */*\r\n"));
    assert!(text.contains("X-Fine: 3\r\n"));
}

fn literal_with_long_value(value_len: usize) -> Vec<u8> {
    let mut block = vec![0x40, 0x01, b'x', 0x7f];
    let mut rest = value_len - 127;
    while rest >= 0x80 {
        block.push((rest & 0x7f) as u8 | 0x80);
        rest >>= 7;
    }
    block.push(rest as u8);
    block.extend(std::iter::repeat_n(b'v', value_len));
    block
}

#[test]
fn hpack_header_list_is_bounded() {
    let mut block = literal_with_long_value(4000);
    block.extend(std::iter::repeat_n(0xbe, 300));
    assert!(hpack::Decoder::new(4096).decode(&block).is_err());
    let mut block = literal_with_long_value(4000);
    block.extend(std::iter::repeat_n(0xbe, 100));
    let decoded = hpack::Decoder::new(4096)
        .decode(&block)
        .expect("a list under the limit decodes");
    assert_eq!(decoded.len(), 101);
}

#[test]
fn field_lines_reject_line_breaks() {
    assert!(h1::field_is_one_line(b"x-a", b"value"));
    assert!(!h1::field_is_one_line(b"x-a", b"v\r\nset-cookie: a=b"));
    assert!(!h1::field_is_one_line(b"x\na", b"v"));
    assert!(!h1::field_is_one_line(b"x-a", b"v\0"));
}

#[test]
fn chunk_sizes_are_hex_digits_with_an_optional_extension() {
    assert_eq!(h1::chunk_size(b"E"), Some(14));
    assert_eq!(h1::chunk_size(b"1a\r"), Some(26));
    assert_eq!(h1::chunk_size(b"5;ext=1"), Some(5));
    assert_eq!(h1::chunk_size(b"5 ;ext=1\r"), Some(5));
    assert_eq!(h1::chunk_size(b"0\r"), Some(0));
    for bad in [
        &b""[..],
        b"\r",
        b" 5",
        b"+5",
        b"-1",
        b"0x5",
        b"5 x",
        b"5g",
        b"ffffffffffffffff",
    ] {
        assert_eq!(h1::chunk_size(bad), None, "{bad:?}");
    }
    for wire in [
        &b"+4\r\nWiki\r\n0\r\n\r\n"[..],
        b" 4\r\nWiki\r\n0\r\n\r\n",
        b"-1\r\n\r\n",
    ] {
        let mut chunked = h1::Chunked::new();
        assert!(chunked.feed(wire, &mut Vec::new()).is_err(), "{wire:?}");
    }
}
