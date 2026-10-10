//! Southstar — rebuilds an SFNT font file from the tables FreeType reads out of a WOFF font.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_ulong};

use crate::ffi::Face;

const MAX_TABLES: c_ulong = 4096;
const MAX_SFNT: usize = 64 * 1024 * 1024;
const CFF_TAG: c_ulong = 0x4346_4620;
const OTTO_TAG: u32 = 0x4f54_544f;
const TRUETYPE_VERSION: u32 = 0x0001_0000;

struct Table {
    tag: c_ulong,
    len: c_ulong,
    offset: usize,
}

fn put_be16(buf: &mut [u8], at: usize, value: u16) {
    buf[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

fn put_be32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn padded(len: c_ulong) -> usize {
    (len as usize + 3) & !3
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.as_chunks::<4>().0.iter().fold(0u32, |sum, &word| {
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn layout(tables: &mut [Table]) -> Option<usize> {
    let mut offset = 12 + tables.len() * 16;
    for table in tables {
        table.offset = offset;
        if table.len as usize > MAX_SFNT {
            return None;
        }
        let size = padded(table.len);
        if size > MAX_SFNT || offset > MAX_SFNT - size {
            return None;
        }
        offset += size;
    }
    Some(offset)
}

pub(crate) fn from_face(face: &Face) -> Option<(Vec<u8>, bool)> {
    let count = face.table_count().filter(|&n| n != 0 && n <= MAX_TABLES)?;
    let mut tables = Vec::with_capacity(count as usize);
    let mut cff = false;
    for index in 0..count as c_uint {
        let (tag, len) = face.table_info(index)?;
        cff |= tag == CFF_TAG;
        tables.push(Table {
            tag,
            len,
            offset: 0,
        });
    }
    let mut entry_selector: u32 = 0;
    while (1usize << (entry_selector + 1)) <= count as usize {
        entry_selector += 1;
    }
    let search_range = ((1u32 << entry_selector) * 16) as u16;
    let range_shift = (count as usize * 16 - search_range as usize) as u16;
    let size = layout(&mut tables)?;
    let mut buf = Vec::new();
    buf.try_reserve_exact(size).ok()?;
    buf.resize(size, 0);
    put_be32(&mut buf, 0, if cff { OTTO_TAG } else { TRUETYPE_VERSION });
    put_be16(&mut buf, 4, count as u16);
    put_be16(&mut buf, 6, search_range);
    put_be16(&mut buf, 8, entry_selector as u16);
    put_be16(&mut buf, 10, range_shift);
    for (index, table) in tables.iter().enumerate() {
        if !face.load_table(table.tag, &mut buf[table.offset..], table.len) {
            return None;
        }
        let sum = checksum(&buf[table.offset..table.offset + padded(table.len)]);
        let dir = 12 + index * 16;
        put_be32(&mut buf, dir, table.tag as u32);
        put_be32(&mut buf, dir + 4, sum);
        put_be32(&mut buf, dir + 8, table.offset as u32);
        put_be32(&mut buf, dir + 12, table.len as u32);
    }
    Some((buf, cff))
}
