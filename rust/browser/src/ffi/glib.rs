//! Southstar — the pieces the page lifecycle uses: text buffers, the GLib main context, timeouts, logging and allocation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use core::time::Duration;
use std::ffi::{CString, OsString};
use std::io::Write;

use southstar_glib as glib;

const DLOG_JS: c_int = 5;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

#[derive(Default)]
#[allow(clippy::box_collection)]
pub struct StrBuf(Cell<Option<Box<Vec<u8>>>>);

impl StrBuf {
    fn with<R>(&self, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        let mut buf = self.0.take().unwrap_or_default();
        let result = f(&mut buf);
        self.0.set(Some(buf));
        result
    }

    pub fn len(&self) -> usize {
        self.with(|buf| buf.len())
    }

    pub fn create(&self) {
        self.with(|_| ());
    }

    pub fn append(&self, bytes: &[u8]) {
        self.with(|buf| buf.extend_from_slice(bytes));
    }

    pub fn erase_front(&self, n: usize) {
        self.with(|buf| {
            buf.drain(..n.min(buf.len()));
        });
    }

    pub fn take_bytes(&self) -> Option<Vec<u8>> {
        self.with(|buf| (!buf.is_empty()).then(|| core::mem::take(buf)))
    }

    pub fn take_text(&self) -> *mut c_char {
        self.take_bytes()
            .map_or(ptr::null_mut(), |bytes| glib::strdup(&bytes))
    }

    pub fn free(&self) {
        self.0.take();
    }
}

#[derive(Default)]
pub struct StrBufOwned(RefCell<Vec<u8>>);

impl StrBufOwned {
    pub fn new() -> StrBufOwned {
        StrBufOwned::default()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_inner()
    }

    pub fn append(&self, bytes: &[u8]) {
        self.0.borrow_mut().extend_from_slice(bytes);
    }

    pub fn into_text(self) -> *mut c_char {
        glib::strdup(&self.into_bytes())
    }
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub fn main_context_pending() -> bool {
    glib::default_context_pending()
}

pub fn main_context_iteration() {
    glib::default_context_iteration(false);
}

pub fn usleep(us: u64) {
    std::thread::sleep(Duration::from_micros(us));
}

pub fn log_message(text: &CStr) {
    southstar_glib::log(southstar_glib::LogLevel::Message, text.to_bytes());
}

pub fn printerr(text: &[u8]) {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    let _ = std::io::stderr().write_all(&text[..end]);
}

fn os_bytes(value: OsString) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        value.into_vec()
    }
    #[cfg(not(unix))]
    {
        value.to_string_lossy().into_owned().into_bytes()
    }
}

pub fn env_value(name: &str) -> Option<Vec<u8>> {
    std::env::var_os(name).map(os_bytes)
}

pub fn printerr_paint_profile(ms: f64, width: c_int, height: c_int) {
    printerr(format!("[profile] paint {ms:6.1}ms {width}x{height}\n").as_bytes());
}

pub fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn hex_value(b: u8) -> Option<u8> {
    char::from(b).to_digit(16).map(|d| d as u8)
}

pub fn uri_unescape(escaped: &CStr) -> Option<CString> {
    let input = escaped.to_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            let hi = input.get(i + 1).copied().and_then(hex_value)?;
            let lo = input.get(i + 2).copied().and_then(hex_value)?;
            let c = (hi << 4) | lo;
            if c == 0 {
                return None;
            }
            out.push(c);
            i += 3;
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    owned_text(&out)
}

pub fn dup_text(text: &CStr) -> Option<CString> {
    owned_text(text.to_bytes())
}

pub fn owned_text(bytes: &[u8]) -> Option<CString> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).ok()
}

pub fn debug_log_console(line: &CStr) {
    southstar_debuglog::emit(DLOG_JS, b"console", line.to_bytes());
}

pub fn malloc_dup(bytes: &[u8]) -> *mut c_char {
    // SAFETY: malloc takes a plain size; the C caller owns the result and releases it with free().
    let out = unsafe { malloc(bytes.len() + 1) }.cast::<u8>();
    if out.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: malloc returned `bytes.len() + 1` writable bytes that nothing else refers to yet.
    let copy = unsafe { core::slice::from_raw_parts_mut(out, bytes.len() + 1) };
    copy[..bytes.len()].copy_from_slice(bytes);
    copy[bytes.len()] = 0;
    out.cast()
}

pub enum Threads {
    Unavailable(Vec<u8>),
    Listed(Vec<(Vec<u8>, Vec<u8>)>),
}

fn chomp(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let mut bytes = &bytes[..end];
    while let [rest @ .., b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'] = bytes {
        bytes = rest;
    }
    bytes
}

pub fn list_threads() -> Threads {
    const TASKS: &str = "/proc/self/task";
    let dir = match std::fs::read_dir(TASKS) {
        Ok(dir) => dir,
        Err(error) => {
            return Threads::Unavailable(
                format!("Error opening directory \u{201c}{TASKS}\u{201d}: {error}").into_bytes(),
            );
        }
    };
    let mut threads = Vec::new();
    for entry in dir.flatten() {
        let tid = os_bytes(entry.file_name());
        let mut path = entry.path();
        path.push("comm");
        if let Ok(comm) = std::fs::read(&path) {
            threads.push((tid, chomp(&comm).to_vec()));
        }
    }
    Threads::Listed(threads)
}
