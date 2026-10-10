//! Southstar — JavaScript bytecode cache: an LRU in memory over versioned files on disk, keyed by the source's SHA-256.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

const MEM_CAP_BYTES: usize = 16 * 1024 * 1024;
const VALUE_CAP_BYTES: usize = 4 * 1024 * 1024;
const FORMAT_VERSION: u32 = 2026100301;
const MAGIC: u32 = 0x4E4A4243;
const HEADER_LEN: usize = 8;
const APP_DIR_NAME: &str = "southstar";

struct Entry {
    bytes: Vec<u8>,
    used: u64,
}

struct Cache {
    mem: Option<HashMap<String, Entry>>,
    mem_bytes: usize,
    dir: Option<PathBuf>,
    clock: u64,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    mem: None,
    mem_bytes: 0,
    dir: None,
    clock: 0,
});

fn lock() -> MutexGuard<'static, Cache> {
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn cache_dir() -> PathBuf {
    ffi::user_cache_dir().join(APP_DIR_NAME).join("jsbc")
}

impl Cache {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn evict_until_fits(&mut self, want: usize) {
        let Some(mem) = self.mem.as_mut() else {
            return;
        };
        while self.mem_bytes + want > MEM_CAP_BYTES {
            let Some(oldest) = mem
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = mem.remove(&oldest) {
                self.mem_bytes -= entry.bytes.len();
            }
        }
    }

    fn insert(&mut self, key: &str, bytes: &[u8]) {
        self.evict_until_fits(bytes.len());
        let used = self.tick();
        if let Some(mem) = self.mem.as_mut() {
            mem.insert(
                key.to_owned(),
                Entry {
                    bytes: bytes.to_vec(),
                    used,
                },
            );
            self.mem_bytes += bytes.len();
        }
    }
}

pub fn init() {
    let mut cache = lock();
    cache.mem.get_or_insert_with(HashMap::new);
    if cache.dir.is_none() && southstar_config::cache_enabled() {
        let dir = cache_dir();
        ffi::create_private_dir(&dir);
        cache.dir = Some(dir);
    }
}

pub fn shutdown() {
    let mut cache = lock();
    cache.mem = None;
    cache.mem_bytes = 0;
    cache.dir = None;
}

fn disk_path_for(key: &str) -> Option<PathBuf> {
    let base = lock().dir.clone()?;
    let dir = base.join(&key[..2]);
    ffi::create_private_dir(&dir);
    Some(dir.join(&key[2..]))
}

fn header() -> [u8; HEADER_LEN] {
    let mut header = [0; HEADER_LEN];
    header[..4].copy_from_slice(&MAGIC.to_le_bytes());
    header[4..].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    header
}

fn read_disk(key: &str) -> Option<Vec<u8>> {
    let path = disk_path_for(key)?;
    let size = fs::metadata(&path).ok()?.len();
    if size < HEADER_LEN as u64 || size > (VALUE_CAP_BYTES + HEADER_LEN) as u64 {
        return None;
    }
    let contents = fs::read(&path).ok()?;
    let body = contents.strip_prefix(&header())?;
    (!body.is_empty() && body.len() <= VALUE_CAP_BYTES).then(|| body.to_vec())
}

fn write_file(path: &Path, bytecode: &[u8]) -> bool {
    let Ok(mut file) = fs::File::create(path) else {
        return false;
    };
    file.write_all(&header()).is_ok() && file.write_all(bytecode).is_ok()
}

fn write_disk(key: &str, bytecode: &[u8]) {
    let Some(path) = disk_path_for(key) else {
        return;
    };
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if !write_file(&tmp, bytecode) || fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

pub fn get(src: &[u8]) -> Option<Vec<u8>> {
    if src.is_empty() {
        return None;
    }
    let key = ffi::sha256_hex(src);
    {
        let mut cache = lock();
        let used = cache.tick();
        let entry = cache.mem.as_mut()?.get_mut(&key);
        if let Some(entry) = entry {
            entry.used = used;
            return Some(entry.bytes.clone());
        }
    }
    let disk = read_disk(&key)?;
    let mut cache = lock();
    if cache
        .mem
        .as_ref()
        .is_some_and(|mem| !mem.contains_key(&key))
    {
        cache.insert(&key, &disk);
    }
    Some(disk)
}

pub fn put(src: &[u8], bytecode: &[u8]) {
    if src.is_empty() || bytecode.is_empty() || bytecode.len() > VALUE_CAP_BYTES {
        return;
    }
    let key = ffi::sha256_hex(src);
    {
        let mut cache = lock();
        let used = cache.tick();
        let Some(mem) = cache.mem.as_mut() else {
            return;
        };
        if let Some(existing) = mem.get_mut(&key) {
            existing.used = used;
            return;
        }
        cache.insert(&key, bytecode);
    }
    if !southstar_config::private_mode() {
        write_disk(&key, bytecode);
    }
}
