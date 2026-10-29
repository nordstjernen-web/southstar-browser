//! Southstar — unit tests for the localStorage key-file reader and writer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{read_group, write_groups};

fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

#[test]
fn reads_what_glib_writes() {
    let data = b"[meta]\norigin=https://example.com\n\n[storage]\na=1\nlines=one\\ntwo\\\\three\n";
    assert_eq!(
        read_group(data, "storage"),
        Some(pairs(&[("a", "1"), ("lines", "one\ntwo\\three")]))
    );
    assert_eq!(
        read_group(data, "meta"),
        Some(pairs(&[("origin", "https://example.com")]))
    );
}

#[test]
fn reads_comments_whitespace_and_crlf() {
    let data = b"# comment\r\n[storage]\r\n  key  =  \\s value \r\nbad=\\q\nkey2=x=y\n";
    assert_eq!(
        read_group(data, "storage"),
        Some(pairs(&[("key", "  value "), ("key2", "x=y")]))
    );
}

#[test]
fn rejects_malformed_files() {
    assert_eq!(read_group(b"a=1\n[storage]\n", "storage"), None);
    assert_eq!(read_group(b"[storage\na=1\n", "storage"), None);
    assert_eq!(read_group(b"[storage]\nno equals\n", "storage"), None);
    assert_eq!(read_group(b"[storage]\na=\xff\n", "storage"), None);
}

#[test]
fn round_trips_values() {
    let values = [
        ("plain", "text"),
        ("lead", "  \tspaced"),
        ("ctrl", "a\nb\rc\td\\e"),
        ("empty", ""),
        ("unicode", "blåbær ✓"),
    ];
    let text = write_groups([
        ("meta", vec![("origin", "o")]),
        ("storage", values.to_vec()),
    ]);
    assert_eq!(read_group(text.as_bytes(), "storage"), Some(pairs(&values)));
    assert_eq!(
        read_group(text.as_bytes(), "meta"),
        Some(pairs(&[("origin", "o")]))
    );
}

#[test]
fn skips_keys_the_format_cannot_hold() {
    let text = write_groups([(
        "storage",
        vec![
            ("a=b", "1"),
            ("[x]", "2"),
            ("new\nline", "3"),
            (" lead", "4"),
            ("", "5"),
            ("ok", "6"),
            ("nul\0tail", "7"),
        ],
    )]);
    assert_eq!(
        read_group(text.as_bytes(), "storage"),
        Some(pairs(&[("ok", "6"), ("nul", "7")]))
    );
}
