//! Southstar — the file system, clock and GLib calls behind file: and FTP responses and the network stores: file names and URIs, folders, stat, local times, content types and the public suffix list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_void};

use std::ffi::CString;
use std::io::Read;
use std::path::{Path, PathBuf};

use southstar_config::util;
use southstar_netutil::file_uri;

const TMP_NAME_CHARS: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

unsafe extern "C" {
    safe fn psl_builtin() -> *const c_void;
    fn psl_is_public_suffix(psl: *const c_void, domain: *const c_char) -> c_int;
}

fn c(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn io_message(error: &std::io::Error) -> Vec<u8> {
    let text = error.to_string();
    let text = match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => &text[..at],
        _ => &text[..],
    };
    text.as_bytes().to_vec()
}

pub fn utf8_collate(a: &[u8], b: &[u8]) -> Ordering {
    southstar_glib::utf8_collate(a, b)
}

pub fn ascii_strtoull(text: &[u8]) -> u64 {
    util::ascii_strtoull(text)
}

pub fn filename_display_name(name: &[u8]) -> Vec<u8> {
    util::utf8_make_valid(name)
}

pub fn filename_to_uri(path: &[u8]) -> Option<Vec<u8>> {
    file_uri::filename_to_uri(path)
}

pub fn filename_from_uri(uri: &[u8]) -> Option<Vec<u8>> {
    file_uri::filename_from_uri(uri)
}

pub fn canonicalize_filename(path: &[u8]) -> Vec<u8> {
    file_uri::canonicalize_filename(path)
}

pub fn build_filename(dir: &[u8], name: &[u8]) -> Vec<u8> {
    util::build_filename(&[dir, name])
}

pub fn path_dirname(path: &[u8]) -> Vec<u8> {
    util::path_dirname(path)
}

pub fn is_dir(path: &[u8]) -> bool {
    os_path(path).is_some_and(|path| path.is_dir())
}

pub struct DirError {
    pub denied: bool,
    pub message: Option<Vec<u8>>,
}

pub fn read_dir(path: &[u8]) -> Result<Vec<Vec<u8>>, DirError> {
    let opened = os_path(path)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))
        .and_then(std::fs::read_dir);
    let entries = opened.map_err(|e| DirError {
        denied: e.kind() == std::io::ErrorKind::PermissionDenied,
        message: Some(
            [
                "Error opening directory “".as_bytes(),
                &filename_display_name(path),
                "”: ".as_bytes(),
                &io_message(&e),
            ]
            .concat(),
        ),
    })?;
    Ok(entries
        .flatten()
        .map(|entry| util::path_bytes(Path::new(&entry.file_name())))
        .collect())
}

#[derive(Clone, Copy)]
pub struct Stat {
    pub size: i64,
    pub mtime: i64,
}

fn os_path(path: &[u8]) -> Option<PathBuf> {
    util::os_path(path)
}

#[cfg(unix)]
fn stat_of(meta: &std::fs::Metadata) -> Stat {
    use std::os::unix::fs::MetadataExt;
    Stat {
        size: meta.size() as i64,
        mtime: meta.mtime(),
    }
}

#[cfg(not(unix))]
fn stat_of(meta: &std::fs::Metadata) -> Stat {
    let mtime = meta
        .modified()
        .ok()
        .map_or(0, |t| match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(after) => after.as_secs() as i64,
            Err(before) => -(before.duration().as_secs() as i64),
        });
    Stat {
        size: meta.len() as i64,
        mtime,
    }
}

pub fn stat(path: &[u8]) -> Option<Stat> {
    std::fs::metadata(os_path(path)?)
        .ok()
        .map(|meta| stat_of(&meta))
}

pub fn version(path: &[u8]) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(os_path(path)?).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

pub fn lock_exclusive(path: &[u8]) -> Option<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(os_path(path)?)
        .ok()?;
    file.lock().ok()?;
    Some(file)
}

pub fn local_time_label(secs: i64) -> Option<Vec<u8>> {
    southstar_glib::LocalDateTime::from_unix(secs)?.format(c"%Y-%m-%d %H:%M")
}

pub fn mime_type_guess(filename: &[u8], data: Option<&[u8]>) -> Vec<u8> {
    southstar_mime::guess(filename, data)
        .unwrap_or("application/octet-stream")
        .as_bytes()
        .to_vec()
}

pub fn url_pathname(url: &[u8]) -> Option<Vec<u8>> {
    crate::url::parts(url).map(|p| p.pathname)
}

pub struct CFile {
    file: std::fs::File,
    error: Option<std::io::Error>,
}

impl CFile {
    pub fn open(path: &[u8]) -> Result<CFile, Vec<u8>> {
        let path = os_path(path).ok_or_else(|| b"Invalid argument".to_vec())?;
        let file = std::fs::File::open(path).map_err(|e| io_message(&e))?;
        Ok(CFile { file, error: None })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut filled = 0;
        while filled < buf.len() {
            match self.file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.error = Some(e);
                    break;
                }
            }
        }
        filled
    }

    pub fn error(&mut self) -> Option<Vec<u8>> {
        self.error.as_ref().map(io_message)
    }
}

pub fn tmp_dir() -> Vec<u8> {
    southstar_glib::tmp_dir()
}

pub fn user_config_dir() -> Vec<u8> {
    southstar_glib::user_config_dir()
}

pub fn user_data_dir() -> Vec<u8> {
    southstar_glib::user_data_dir()
}

fn tmp_name(prefix: &[u8]) -> Vec<u8> {
    let mut name = prefix.to_vec();
    let mut bits = util::random_u32();
    for _ in 0..6 {
        name.push(TMP_NAME_CHARS[(bits % 62) as usize]);
        bits /= 62;
    }
    name
}

pub fn make_tmp_dir(template: &CStr) -> Option<Vec<u8>> {
    let prefix = template.to_bytes().strip_suffix(b"XXXXXX")?;
    let tmp = tmp_dir();
    for _ in 0..100 {
        let path = build_filename(&tmp, &tmp_name(prefix));
        match util::make_dir(&os_path(&path)?, 0o700) {
            Ok(()) => return Some(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

pub fn mkdir_with_parents(path: &[u8], mode: u32) {
    if let Some(path) = os_path(path) {
        let _ = util::make_dirs(&path, mode);
    }
}

pub fn chmod(path: &[u8], mode: u32) {
    if let Some(path) = os_path(path) {
        let _ = util::set_mode(&path, mode);
    }
}

pub fn random_u32() -> u32 {
    util::random_u32()
}

pub fn now_seconds() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

pub fn empty_dir(dir: &[u8]) {
    let Ok(names) = read_dir(dir) else {
        return;
    };
    for name in names {
        let child = build_filename(dir, &name);
        let Some(child_path) = os_path(&child) else {
            continue;
        };
        if child_path.is_dir() && !is_symlink(&child_path) {
            remove_tree(&child);
        } else {
            let _ = std::fs::remove_file(&child_path);
        }
    }
}

pub fn remove_tree(path: &[u8]) {
    empty_dir(path);
    if let Some(path) = os_path(path) {
        let _ = std::fs::remove_dir(path);
    }
}

pub enum ReadError {
    Missing,
    Other(Vec<u8>),
}

pub fn read_file(path: &[u8]) -> Result<Vec<u8>, ReadError> {
    let path = os_path(path).ok_or_else(|| ReadError::Other(b"Invalid argument".to_vec()))?;
    std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ReadError::Missing,
        _ => ReadError::Other(io_message(&e)),
    })
}

pub fn write_file(path: &[u8], contents: &[u8]) -> bool {
    os_path(path).is_some_and(|path| util::write_atomic(&path, contents, 0o666).is_ok())
}

pub fn is_public_suffix(domain: &[u8]) -> bool {
    let psl = psl_builtin();
    if psl.is_null() {
        return false;
    }
    let domain = c(domain);
    // SAFETY: `psl` is the non-null built-in list and `domain` a NUL-terminated string that
    // outlives the call.
    unsafe { psl_is_public_suffix(psl, domain.as_ptr()) != 0 }
}

pub fn warn_read_failure(what: &CStr, path: &[u8], message: &[u8]) {
    let mut text = what.to_bytes().to_vec();
    text.extend_from_slice(b": failed to read ");
    text.extend_from_slice(path);
    text.extend_from_slice(b": ");
    text.extend_from_slice(message);
    southstar_glib::warning(text);
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub fn info(message: &CStr) {
    southstar_glib::log(southstar_glib::LogLevel::Info, message.to_bytes());
}

pub fn exists(path: &[u8]) -> bool {
    os_path(path).is_some_and(|path| path.exists())
}

#[cfg(target_vendor = "apple")]
fn platform_exe() -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    std::env::current_exe()
        .ok()
        .map(|p| p.as_os_str().as_bytes().to_vec())
}

#[cfg(windows)]
fn platform_exe() -> Option<Vec<u8>> {
    std::env::current_exe()
        .ok()?
        .to_str()
        .map(|s| s.as_bytes().to_vec())
}

#[cfg(target_os = "linux")]
fn platform_exe() -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::read_link("/proc/self/exe")
        .ok()
        .map(|p| p.as_os_str().as_bytes().to_vec())
}

#[cfg(not(any(target_vendor = "apple", windows, target_os = "linux")))]
fn platform_exe() -> Option<Vec<u8>> {
    None
}

pub fn exe_dir() -> Option<Vec<u8>> {
    let own = super::host::self_exe().filter(|e| !e.is_empty());
    own.or_else(platform_exe).map(|exe| path_dirname(&exe))
}

#[cfg(target_os = "linux")]
pub fn available_memory_bytes() -> u64 {
    let Ok(meminfo) = std::fs::read("/proc/meminfo") else {
        return 0;
    };
    for line in meminfo.split(|&c| c == b'\n') {
        let Some(rest) = line.strip_prefix(b"MemAvailable:") else {
            continue;
        };
        let rest = &rest[rest
            .iter()
            .position(|c| !c.is_ascii_whitespace())
            .unwrap_or(rest.len())..];
        let rest = rest.strip_prefix(b"+").unwrap_or(rest);
        let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            continue;
        }
        let kb = rest[..digits].iter().fold(0u64, |n, &d| {
            n.wrapping_mul(10).wrapping_add(u64::from(d - b'0'))
        });
        return kb.wrapping_mul(1024);
    }
    0
}

#[cfg(windows)]
pub fn available_memory_bytes() -> u64 {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> c_int;
    }
    let mut status = MemoryStatusEx {
        length: core::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // SAFETY: `status` is a writable MEMORYSTATUSEX whose length field holds its size.
    if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
        status.avail_phys
    } else {
        0
    }
}

#[cfg(target_vendor = "apple")]
pub fn available_memory_bytes() -> u64 {
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const c_char,
            oldp: *mut c_void,
            oldlenp: *mut usize,
            newp: *mut c_void,
            newlen: usize,
        ) -> c_int;
    }
    let mut mem = 0u64;
    let mut len = core::mem::size_of::<u64>();
    // SAFETY: the name is NUL-terminated, `mem` is writable for the `len` bytes passed, and no
    // new value is set.
    let ok = unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut mem as *mut u64).cast(),
            &mut len,
            core::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 { mem } else { 0 }
}

#[cfg(any(target_os = "openbsd", target_os = "solaris", target_os = "illumos"))]
pub fn available_memory_bytes() -> u64 {
    unsafe extern "C" {
        fn sysconf(name: c_int) -> core::ffi::c_long;
    }
    const AVPHYS_PAGES: c_int = 501;
    const PAGESIZE: c_int = if cfg!(target_os = "openbsd") { 28 } else { 11 };
    // SAFETY: sysconf takes a plain selector and only returns a value.
    let pages = unsafe { sysconf(AVPHYS_PAGES) };
    // SAFETY: sysconf takes a plain selector and only returns a value.
    let size = unsafe { sysconf(PAGESIZE) };
    if pages > 0 && size > 0 {
        pages as u64 * size as u64
    } else {
        0
    }
}

#[cfg(not(any(
    target_os = "linux",
    windows,
    target_vendor = "apple",
    target_os = "openbsd",
    target_os = "solaris",
    target_os = "illumos"
)))]
pub fn available_memory_bytes() -> u64 {
    0
}
