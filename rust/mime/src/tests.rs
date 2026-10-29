//! Southstar — tests for media type guessing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::guess;

#[test]
fn extensions_decide_first() {
    assert_eq!(guess(b"/tmp/page.HTML", None), Some("text/html"));
    assert_eq!(guess(b"C:\\Users\\me\\photo.JPG", None), Some("image/jpeg"));
    assert_eq!(guess(b"dir.v2/app.mjs", None), Some("text/javascript"));
    assert_eq!(
        guess(b"font.woff2", Some(b"not a font")),
        Some("font/woff2")
    );
}

#[test]
fn names_without_a_usable_extension_fall_back() {
    assert_eq!(guess(b".bashrc", None), None);
    assert_eq!(guess(b"archive.", None), None);
    assert_eq!(guess(b"dir.d/README", None), None);
    assert_eq!(guess(b"unknown.qqq", None), None);
}

#[test]
fn signatures_identify_untyped_bytes() {
    assert_eq!(
        guess(b"x", Some(b"\x89PNG\r\n\x1a\n....")),
        Some("image/png")
    );
    assert_eq!(guess(b"x", Some(b"GIF89a....")), Some("image/gif"));
    assert_eq!(
        guess(b"x", Some(b"RIFF\x10\0\0\0WEBPVP8 ")),
        Some("image/webp")
    );
    assert_eq!(
        guess(b"x", Some(b"RIFF\x10\0\0\0WAVEfmt ")),
        Some("audio/wav")
    );
    assert_eq!(guess(b"x", Some(b"\0\0\0\x18ftypisom")), Some("video/mp4"));
    assert_eq!(guess(b"x", Some(b"\0\0\0\x1cftypavif")), Some("image/avif"));
    assert_eq!(guess(b"x", Some(b"%PDF-1.7")), Some("application/pdf"));
    assert_eq!(guess(b"x", Some(b"\x1a\x45\xdf\xa3")), Some("video/webm"));
}

#[test]
fn markup_and_text_are_recognised() {
    assert_eq!(
        guess(b"x", Some(b"\n  <!DOCTYPE html><p>")),
        Some("text/html")
    );
    assert_eq!(
        guess(b"x", Some(b"\xef\xbb\xbf<svg xmlns=")),
        Some("image/svg+xml")
    );
    assert_eq!(
        guess(b"x", Some(b"<?xml version=\"1.0\"?><svg>")),
        Some("image/svg+xml")
    );
    assert_eq!(
        guess(b"x", Some(b"<?xml version=\"1.0\"?><feed>")),
        Some("application/xml")
    );
    assert_eq!(
        guess(b"x", Some("plain text, ÆØÅ".as_bytes())),
        Some("text/plain")
    );
    assert_eq!(guess(b"x", Some(b"\x01\x02\x00\x03")), None);
    assert_eq!(guess(b"x", Some(b"")), None);
}

#[test]
fn text_cut_inside_a_character_is_still_text() {
    let mut data = vec![b'a'; 4095];
    data.extend_from_slice("ø".as_bytes());
    assert_eq!(guess(b"x", Some(&data)), Some("text/plain"));
}
