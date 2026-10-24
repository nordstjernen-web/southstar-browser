//! Southstar — unit tests for WOFF 1.0 decoding.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{is_woff, woff_to_sfnt};

fn woff(tables: &[(&[u8; 4], Vec<u8>, bool)], fix: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut directory = Vec::new();
    let mut body = Vec::new();
    let mut offset = 44 + tables.len() * 20;
    for (tag, data, compress) in tables {
        let stored = if *compress {
            miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
        } else {
            data.clone()
        };
        directory.extend_from_slice(*tag);
        directory.extend_from_slice(&(offset as u32).to_be_bytes());
        directory.extend_from_slice(&(stored.len() as u32).to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
        directory.extend_from_slice(&0u32.to_be_bytes());
        offset += stored.len();
        body.extend_from_slice(&stored);
    }
    let mut out = b"wOFF".to_vec();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(offset as u32).to_be_bytes());
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0; 2]);
    out.extend_from_slice(&[0; 28]);
    out.extend_from_slice(&directory);
    out.extend_from_slice(&body);
    fix(&mut out);
    out
}

fn sfnt_table(sfnt: &[u8], tag: &[u8; 4]) -> Option<Vec<u8>> {
    let count = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
    (0..count).find_map(|i| {
        let entry = &sfnt[12 + i * 16..28 + i * 16];
        let word = |at: usize| u32::from_be_bytes(entry[at..at + 4].try_into().unwrap()) as usize;
        (&entry[..4] == tag).then(|| sfnt[word(8)..word(8) + word(12)].to_vec())
    })
}

#[test]
fn decodes_stored_and_compressed_tables() {
    let compressible = vec![7u8; 300];
    let raw = b"abc".to_vec();
    let data = woff(
        &[
            (b"cmap", compressible.clone(), true),
            (b"name", raw.clone(), false),
        ],
        |_| {},
    );
    assert!(is_woff(&data));
    let sfnt = woff_to_sfnt(&data).expect("valid WOFF decodes");
    assert!(!sfnt.cff);
    assert_eq!(sfnt_table(&sfnt.bytes, b"cmap"), Some(compressible));
    assert_eq!(sfnt_table(&sfnt.bytes, b"name"), Some(raw));
}

#[test]
fn rejects_malformed_files() {
    let table = [(b"cmap", vec![1u8; 64], true)];
    assert!(woff_to_sfnt(&woff(&table, |w| w.push(0))).is_none());
    assert!(woff_to_sfnt(&woff(&table, |w| w[15] = 1)).is_none());
    assert!(woff_to_sfnt(&woff(&table, |w| w[44 + 15] = 65)).is_none());
    let duplicate = [
        (b"cmap", vec![1u8; 8], false),
        (b"cmap", vec![2u8; 8], false),
    ];
    assert!(woff_to_sfnt(&woff(&duplicate, |_| {})).is_none());
    assert!(woff_to_sfnt(b"wOFF").is_none());
}
