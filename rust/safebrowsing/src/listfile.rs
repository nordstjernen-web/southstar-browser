//! Southstar — the Safe Browsing hash-prefix list files in the cache directory: atomic saves, binary search on disk and the one-updater lock.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Duration;

use southstar_config::util;

use crate::protobuf::MAX_ENTRIES;

const MAGIC: u32 = 0x3242_5353;
const MAX_VERSION_BYTES: u32 = 4096;

#[derive(Default)]
pub(crate) struct List {
    pub version: Vec<u8>,
    pub updated_at: i64,
    pub next_update: i64,
    pub values: Vec<u32>,
}

pub(crate) struct Header {
    pub version: Vec<u8>,
    pub updated_at: i64,
    pub next_update: i64,
    pub count: u32,
    data_offset: u64,
}

pub(crate) fn dir() -> Option<PathBuf> {
    let cache = southstar_glib::user_cache_dir();
    util::os_path(&util::build_filename(&[
        &cache,
        b"southstar",
        b"safebrowsing",
    ]))
}

fn path(name: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("{name}.db")))
}

fn read_u32(file: &mut File) -> Option<u32> {
    let mut buf = [0u8; 4];
    file.read_exact(&mut buf).ok()?;
    Some(u32::from_le_bytes(buf))
}

fn read_i64(file: &mut File) -> Option<i64> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf).ok()?;
    Some(i64::from_le_bytes(buf))
}

fn read_header(file: &mut File) -> Option<Header> {
    if read_u32(file)? != MAGIC {
        return None;
    }
    let version_len = read_u32(file)?;
    if version_len > MAX_VERSION_BYTES {
        return None;
    }
    let mut version = vec![0u8; version_len as usize];
    file.read_exact(&mut version).ok()?;
    let updated_at = read_i64(file)?;
    let next_update = read_i64(file)?;
    let count = read_u32(file)?;
    if count > MAX_ENTRIES {
        return None;
    }
    Some(Header {
        version,
        updated_at,
        next_update,
        count,
        data_offset: 4 + 4 + u64::from(version_len) + 8 + 8 + 4,
    })
}

fn open(name: &str) -> Option<File> {
    File::open(path(name)?).ok()
}

pub(crate) fn header(name: &str) -> Option<Header> {
    read_header(&mut open(name)?)
}

pub(crate) fn load(name: &str) -> Option<List> {
    let mut file = open(name)?;
    let header = read_header(&mut file)?;
    let mut raw = vec![0u8; header.count as usize * 4];
    file.read_exact(&mut raw).ok()?;
    Some(List {
        version: header.version,
        updated_at: header.updated_at,
        next_update: header.next_update,
        values: raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    })
}

pub(crate) fn save(name: &str, list: &List) -> bool {
    let (Some(target), Ok(count)) = (path(name), u32::try_from(list.values.len())) else {
        return false;
    };
    let mut buf = Vec::with_capacity(32 + list.version.len() + list.values.len() * 4);
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&(list.version.len() as u32).to_le_bytes());
    buf.extend_from_slice(&list.version);
    buf.extend_from_slice(&list.updated_at.to_le_bytes());
    buf.extend_from_slice(&list.next_update.to_le_bytes());
    buf.extend_from_slice(&count.to_le_bytes());
    for value in &list.values {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    let temp = target.with_extension(format!("db.{}.tmp", std::process::id()));
    if std::fs::write(&temp, &buf).is_err() {
        let _ = std::fs::remove_file(&temp);
        return false;
    }
    for attempt in 0..5 {
        if std::fs::rename(&temp, &target).is_ok() {
            return true;
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let _ = std::fs::remove_file(&temp);
    false
}

pub(crate) fn remove(name: &str) {
    if let Some(path) = path(name) {
        let _ = std::fs::remove_file(path);
    }
}

fn contains(file: &mut File, header: &Header, want: u32) -> bool {
    let (mut lo, mut hi) = (0u64, u64::from(header.count));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if file
            .seek(SeekFrom::Start(header.data_offset + mid * 4))
            .is_err()
        {
            return false;
        }
        let Some(value) = read_u32(file) else {
            return false;
        };
        match value.cmp(&want) {
            core::cmp::Ordering::Equal => return true,
            core::cmp::Ordering::Less => lo = mid + 1,
            core::cmp::Ordering::Greater => hi = mid,
        }
    }
    false
}

pub(crate) fn mark_hits(name: &str, prefixes: &[u32], hit: &mut [bool]) {
    let Some(mut file) = open(name) else {
        return;
    };
    let Some(header) = read_header(&mut file) else {
        return;
    };
    for (prefix, hit) in prefixes.iter().zip(hit.iter_mut()) {
        if !*hit && contains(&mut file, &header, *prefix) {
            *hit = true;
        }
    }
}

pub(crate) fn try_lock() -> Option<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir()?.join("update.lock"))
        .ok()?;
    file.try_lock().ok()?;
    Some(file)
}
