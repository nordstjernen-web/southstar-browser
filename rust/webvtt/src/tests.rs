//! Southstar — unit tests for WebVTT cue parsing: timestamps, cue text cleanup and the active-cue lookup.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{active, parse};

const FILE: &[u8] = b"WEBVTT\r\n\r\n1\r\n00:00:01.000 --> 00:00:02.500 align:start\r\n<v Roger>Hello &amp; welcome</v>\r\nsecond line\r\n\r\n00:01.5 --> 00:03\nshort &lt;form&gt;&lrm;\n\n01:00:00,25 --> 01:00:01\nhours\n";

#[test]
fn parses_cues_and_their_timestamps() {
    let cues = parse(FILE);
    assert_eq!(cues.len(), 3);
    assert_eq!((cues[0].start, cues[0].end), (1.0, 2.5));
    assert_eq!(cues[0].text.to_bytes(), b"Hello & welcome\nsecond line");
    assert_eq!((cues[1].start, cues[1].end), (1.5, 3.0));
    assert_eq!(cues[1].text.to_bytes(), b"short <form>");
    assert_eq!((cues[2].start, cues[2].end), (3600.25, 3601.0));
}

#[test]
fn drops_empty_backwards_and_malformed_cues() {
    let cues = parse(b"00:05.000 --> 00:04.000\nbackwards\n\n00:01.000 --> 00:02.000\n\nxx --> 00:03.000\nno start\n");
    assert!(cues.is_empty());
}

#[test]
fn stops_at_a_nul_byte() {
    let cues = parse(b"00:01.000 --> 00:02.000\nkept\n\n\0\n00:03.000 --> 00:04.000\ndropped\n");
    assert_eq!(cues.len(), 1);
}

#[test]
fn finds_the_active_cue() {
    let cues = parse(FILE);
    assert!(active(&cues, 0.5).is_none());
    assert_eq!(active(&cues, 1.0).map(|c| c.start), Some(1.0));
    assert_eq!(active(&cues, 2.6).map(|c| c.start), Some(1.5));
    assert!(active(&cues, 3.0).is_none());
}
