//! Southstar — the in-process debug log: entries, listeners, the optional log file, level names and the line format.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint};
use std::collections::VecDeque;
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

mod ffi;

pub const CAPACITY: usize = 1024;

pub struct Entry {
    pub monotonic_us: i64,
    pub level: c_int,
    pub category: Vec<u8>,
    pub message: Vec<u8>,
}

type Listener = Arc<dyn Fn(&Entry) + Send + Sync>;

#[derive(Clone)]
struct Subscriber {
    id: c_uint,
    listener: Listener,
}

struct Log {
    entries: VecDeque<Entry>,
    subscribers: Vec<Subscriber>,
    next_id: c_uint,
    file: Option<File>,
    file_tried: bool,
}

static LOG: Mutex<Log> = Mutex::new(Log {
    entries: VecDeque::new(),
    subscribers: Vec::new(),
    next_id: 1,
    file: None,
    file_tried: false,
});
static FILE_PATH: Mutex<Option<CString>> = Mutex::new(None);
static INIT: OnceLock<()> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn level_name(level: c_int) -> &'static CStr {
    match level {
        0 => c"info",
        1 => c"warn",
        2 => c"error",
        3 => c"render",
        4 => c"net",
        5 => c"js",
        _ => c"?",
    }
}

pub fn file_line(millis: i64, level: c_int, category: &[u8], message: &[u8]) -> Vec<u8> {
    let mut line = format!(
        "{} {:<5} ",
        millis,
        level_name(level).to_str().unwrap_or("?")
    )
    .into_bytes();
    line.extend_from_slice(category);
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    line
}

fn file_enabled() -> bool {
    if cfg!(windows) {
        ffi::env(c"NS_NO_LOG_FILE").is_none()
    } else {
        ffi::env(c"NS_LOG_FILE").is_some()
    }
}

fn path_buf(path: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(path))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(path).into_owned())
    }
}

pub fn file_path() -> Option<CString> {
    let mut path = lock(&FILE_PATH);
    if path.is_none() && file_enabled() {
        *path = match ffi::env(c"NS_LOG_FILE") {
            Some(fixed) if !fixed.is_empty() => Some(fixed.to_owned()),
            _ => ffi::default_log_path(),
        };
    }
    path.clone()
}

fn open_log_file() -> Option<File> {
    let path = file_path()?;
    ffi::make_parent_dir(&path);
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(path_buf(path.to_bytes()))
        .ok()
}

fn write_file(log: &mut Log, entry: &Entry) {
    if !file_enabled() {
        return;
    }
    if log.file.is_none() && !log.file_tried {
        log.file_tried = true;
        log.file = open_log_file();
    }
    if let Some(file) = log.file.as_mut() {
        let line = file_line(
            ffi::real_time_us() / 1000,
            entry.level,
            &entry.category,
            &entry.message,
        );
        let _ = file.write_all(&line);
        let _ = file.flush();
    }
}

fn up_to_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

pub fn init() {
    INIT.get_or_init(|| ());
}

pub fn emit(level: c_int, category: &[u8], message: &[u8]) {
    init();
    let entry = Entry {
        monotonic_us: ffi::monotonic_us(),
        level,
        category: up_to_nul(category).to_vec(),
        message: up_to_nul(message).to_vec(),
    };
    let subscribers = lock(&LOG).subscribers.clone();
    for subscriber in &subscribers {
        (subscriber.listener)(&entry);
    }
    let mut log = lock(&LOG);
    write_file(&mut log, &entry);
    log.entries.push_back(entry);
    while log.entries.len() > CAPACITY {
        log.entries.pop_front();
    }
}

pub struct Subscription(c_uint);

pub fn subscribe(listener: impl Fn(&Entry) + Send + Sync + 'static) -> Subscription {
    init();
    let mut log = lock(&LOG);
    let id = log.next_id;
    log.next_id = log.next_id.wrapping_add(1);
    log.subscribers.push(Subscriber {
        id,
        listener: Arc::new(listener),
    });
    Subscription(id)
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut log = lock(&LOG);
        if let Some(i) = log.subscribers.iter().position(|s| s.id == self.0) {
            log.subscribers.remove(i);
        }
    }
}
