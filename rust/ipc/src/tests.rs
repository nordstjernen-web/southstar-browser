//! Southstar — unit tests for the flat JSON reader of the renderer protocol.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{atol, json_string, json_value};
use core::ffi::c_long;

fn text(body: &[u8], key: &str) -> Option<Vec<u8>> {
    json_value(body, key.as_bytes()).and_then(json_string)
}

fn long(body: &[u8], key: &str) -> Option<c_long> {
    json_value(body, key.as_bytes()).map(atol)
}

#[test]
fn reads_each_member() {
    let body = br#"{"ok":1,"page_width":800,"title":"Sign in","url":"https://a.example/login","security":3}"#;
    assert_eq!(
        text(body, "url").as_deref(),
        Some(&b"https://a.example/login"[..])
    );
    assert_eq!(text(body, "title").as_deref(), Some(&b"Sign in"[..]));
    assert_eq!(long(body, "security"), Some(3));
    assert_eq!(long(body, "page_width"), Some(800));
}

#[test]
fn ignores_a_key_spelled_inside_a_string_value() {
    let body = br#"{"title":"Sign in \"url","url":"https://evil.example/login","security":3}"#;
    assert_eq!(
        text(body, "url").as_deref(),
        Some(&b"https://evil.example/login"[..])
    );
    assert_eq!(
        text(body, "title").as_deref(),
        Some(&br#"Sign in "url"#[..])
    );

    let body = br#"{"title":"x \"security","url":"http://evil.example/","security":3}"#;
    assert_eq!(long(body, "security"), Some(3));
}

#[test]
fn ignores_a_member_pair_spelled_inside_a_string_value() {
    let body = br#"{"title":"\"url\":\"https://bank.example/\"","url":"https://evil.example/","security":1}"#;
    assert_eq!(
        text(body, "url").as_deref(),
        Some(&b"https://evil.example/"[..])
    );
    assert_eq!(long(body, "security"), Some(1));
}

#[test]
fn ignores_a_string_value_equal_to_the_key() {
    let body = br#"{"nav":"url","url":"https://a.example/"}"#;
    assert_eq!(
        text(body, "url").as_deref(),
        Some(&b"https://a.example/"[..])
    );
}

#[test]
fn accepts_whitespace_nesting_and_escaped_backslashes() {
    let body = b"{ \"tiles\" : [ {\"x\":1} ], \"page_width\" :\n 1024 , \"title\" : \"a\\\\\", \"url\" : \"u\" }";
    assert_eq!(long(body, "x"), Some(1));
    assert_eq!(long(body, "page_width"), Some(1024));
    assert_eq!(text(body, "title").as_deref(), Some(&b"a\\"[..]));
    assert_eq!(text(body, "url").as_deref(), Some(&b"u"[..]));
}

#[test]
fn finds_nothing_in_missing_or_truncated_bodies() {
    assert_eq!(json_value(br#"{"ok":1}"#, b"url"), None);
    assert_eq!(json_value(br#"{"title":"abc"#, b"abc"), None);
    assert_eq!(json_value(br#"{"title"#, b"title"), None);
    assert_eq!(json_value(br#"{"title":"a\"#, b"url"), None);
    assert_eq!(json_value(b"\"", b""), None);
    assert_eq!(json_value(b"", b"url"), None);
}

#[test]
fn rejects_a_nul_escape_and_replaces_lone_surrogates() {
    assert_eq!(json_string(br#""a\u0000b""#), None);
    assert_eq!(
        json_string(br#""\ud83d\ude00""#).as_deref(),
        Some("\u{1F600}".as_bytes())
    );
    assert_eq!(
        json_string(br#""x\ud800y""#).as_deref(),
        Some("x\u{FFFD}y".as_bytes())
    );
    assert_eq!(json_string(br#""\u0041""#).as_deref(), Some(&b"A"[..]));
}
