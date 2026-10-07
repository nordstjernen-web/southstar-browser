//! Southstar — optional spell checking: language tags, dictionary choice and word checks over Enchant.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub fn normalize_lang(raw: &[u8]) -> Option<Vec<u8>> {
    let end = raw
        .iter()
        .position(|c| b".@: \t".contains(c))
        .unwrap_or(raw.len());
    (end > 0).then(|| {
        raw[..end]
            .iter()
            .map(|&c| if c == b'-' { b'_' } else { c })
            .collect()
    })
}

pub fn pick<T: AsRef<[u8]>>(tags: &[T], lang: Option<&[u8]>) -> Option<usize> {
    if tags.is_empty() {
        return None;
    }
    let Some(wanted) = lang.and_then(normalize_lang) else {
        return Some(0);
    };
    let exact = tags
        .iter()
        .position(|tag| tag.as_ref().eq_ignore_ascii_case(&wanted));
    let primary_len = wanted
        .iter()
        .position(|&c| c == b'_')
        .unwrap_or(wanted.len());
    let primary = &wanted[..primary_len];
    let same_language = || {
        tags.iter().position(|tag| {
            let tag = tag.as_ref();
            tag.len() >= primary_len
                && tag[..primary_len].eq_ignore_ascii_case(primary)
                && matches!(tag.get(primary_len), None | Some(b'_'))
        })
    };
    Some(exact.or_else(same_language).unwrap_or(0))
}
