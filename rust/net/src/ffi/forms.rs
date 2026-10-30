//! Southstar — form encoding over GLib: urlencoded pairs in the submission charset, and multipart boundaries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib as glib;

use crate::forms;

fn converted(text: &[u8]) -> Option<Vec<u8>> {
    glib::convert(text, &forms::submission_charset()?, b"UTF-8")
}

fn urlencoded_append(out: &mut Vec<u8>, text: &[u8]) {
    match converted(text) {
        Some(text) => forms::urlencode(out, &text),
        None => forms::urlencode(out, text),
    }
}

pub fn urlencoded_append_pair(
    out: &mut Vec<u8>,
    first: &mut bool,
    name: &[u8],
    value: Option<&[u8]>,
) {
    if !*first {
        out.push(b'&');
    }
    *first = false;
    urlencoded_append(out, name);
    out.push(b'=');
    if let Some(value) = value {
        urlencoded_append(out, value);
    }
}

pub fn multipart_boundary() -> Vec<u8> {
    let mut words = [0u32; 4];
    let mut bytes = [0u8; 16];
    if southstar_sandbox::csprng_fill(&mut bytes) {
        for (word, chunk) in words.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *word = u32::from_ne_bytes(*chunk);
        }
    } else {
        for w in &mut words {
            *w = southstar_config::util::random_u32();
        }
    }
    forms::boundary(words)
}
