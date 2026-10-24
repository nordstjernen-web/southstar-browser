//! Southstar — WOFF 1.0 web font decoding: the table directory and its zlib-compressed tables.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{MAX_OUTPUT, MAX_TABLES, OTTO, Reader, Sfnt, TTCF, Table, assemble_sfnt, sfnt_size};

const HEADER_LEN: usize = 44;
const DIRECTORY_ENTRY_LEN: usize = 20;

pub fn is_woff(data: &[u8]) -> bool {
    data.starts_with(b"wOFF")
}

fn inflate(compressed: &[u8], size: usize) -> Option<Vec<u8>> {
    let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(compressed, size).ok()?;
    (out.len() == size).then_some(out)
}

fn read_table(data: &[u8], r: &mut Reader) -> Option<Table> {
    let tag = r.u32()?;
    let offset = r.u32()? as usize;
    let compressed_len = r.u32()? as usize;
    let orig_len = r.u32()?;
    r.u32()?;
    let size = orig_len as usize;
    if offset < HEADER_LEN || compressed_len > size || size > MAX_OUTPUT {
        return None;
    }
    let stored = data.get(offset..)?.get(..compressed_len)?;
    let bytes = if compressed_len < size {
        inflate(stored, size)?
    } else {
        stored.to_vec()
    };
    Some(Table {
        tag,
        transformed: false,
        orig_len,
        stream_off: 0,
        stream_len: 0,
        data: Some(bytes),
    })
}

pub fn woff_to_sfnt(data: &[u8]) -> Option<Sfnt> {
    if !is_woff(data) || data.len() < HEADER_LEN {
        return None;
    }
    let mut r = Reader { data, pos: 4 };
    let flavor = r.u32()?;
    let length = r.u32()? as usize;
    let num_tables = r.u16()?;
    let reserved = r.u16()?;
    if flavor == TTCF
        || length != data.len()
        || reserved != 0
        || num_tables == 0
        || num_tables > MAX_TABLES
        || HEADER_LEN + usize::from(num_tables) * DIRECTORY_ENTRY_LEN > data.len()
    {
        return None;
    }
    r.pos = HEADER_LEN;
    let mut tables: Vec<Table> = Vec::with_capacity(usize::from(num_tables));
    for _ in 0..num_tables {
        let table = read_table(data, &mut r)?;
        if tables.iter().any(|seen| seen.tag == table.tag) {
            return None;
        }
        tables.push(table);
    }
    let size = sfnt_size(&tables)?;
    Some(Sfnt {
        bytes: assemble_sfnt(flavor, &tables, size),
        cff: flavor == OTTO,
    })
}
