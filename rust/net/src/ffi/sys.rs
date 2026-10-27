//! Southstar — the file system, clock and GLib calls behind file: and FTP responses and the network stores: file names and URIs, folders, stat, local times, content types and the public suffix list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;
use std::io::Read;
use std::path::{Path, PathBuf};

use southstar_config::util;
use southstar_glib::{self as glib, GBoolean, GStr};
use southstar_netutil::file_uri;

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

const LOG_LEVEL_WARNING: c_int = 1 << 4;
const TMP_NAME_CHARS: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

unsafe extern "C" {
    fn g_utf8_collate(str1: *const c_char, str2: *const c_char) -> c_int;
    fn g_date_time_new_from_unix_local(t: i64) -> *mut GDateTime;
    fn g_date_time_format(datetime: *mut GDateTime, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut GDateTime);
    fn g_content_type_guess(
        filename: *const c_char,
        data: *const u8,
        data_size: usize,
        result_uncertain: *mut GBoolean,
    ) -> *mut c_char;
    fn g_content_type_get_mime_type(content_type: *const c_char) -> *mut c_char;
    fn g_get_tmp_dir() -> *const c_char;

    fn psl_builtin() -> *const c_void;
    fn psl_is_public_suffix(psl: *const c_void, domain: *const c_char) -> c_int;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_get_monotonic_time() -> i64;
}

fn c(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
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
    let (a, b) = (c(a), c(b));
    unsafe { g_utf8_collate(a.as_ptr(), b.as_ptr()) }.cmp(&0)
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
    let dt = unsafe { g_date_time_new_from_unix_local(secs) };
    if dt.is_null() {
        return None;
    }
    let out = take(unsafe { g_date_time_format(dt, c"%Y-%m-%d %H:%M".as_ptr()) });
    unsafe { g_date_time_unref(dt) };
    out
}

pub fn mime_type_guess(filename: &[u8], data: Option<&[u8]>) -> Vec<u8> {
    let filename = c(filename);
    let (data, len) = data.map_or((ptr::null(), 0), |d| (d.as_ptr(), d.len()));
    let mut uncertain: GBoolean = 0;
    let content_type =
        unsafe { g_content_type_guess(filename.as_ptr(), data, len, &mut uncertain) };
    let mime = if content_type.is_null() {
        None
    } else {
        take(unsafe { g_content_type_get_mime_type(content_type) })
    };
    unsafe { glib::g_free(content_type.cast()) };
    mime.unwrap_or_else(|| b"application/octet-stream".to_vec())
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
    unsafe { glib::bytes(g_get_tmp_dir()) }
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

pub fn user_config_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_config_dir()) }
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

pub fn user_data_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_data_dir()) }
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
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
    let psl = unsafe { psl_builtin() };
    if psl.is_null() {
        return false;
    }
    let domain = c(domain);
    unsafe { psl_is_public_suffix(psl, domain.as_ptr()) != 0 }
}

pub fn warn_read_failure(what: &CStr, path: &[u8], message: &[u8]) {
    let (path, message) = (c(path), c(message));
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"%s: failed to read %s: %s".as_ptr(),
            what.as_ptr(),
            path.as_ptr(),
            message.as_ptr(),
        )
    };
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub fn info(message: &CStr) {
    unsafe { g_log(ptr::null(), 1 << 6, c"%s".as_ptr(), message.as_ptr()) };
}

pub fn exists(path: &[u8]) -> bool {
    os_path(path).is_some_and(|path| path.exists())
}

#[cfg(target_vendor = "apple")]
fn platform_exe() -> Option<Vec<u8>> {
    unsafe extern "C" {
        fn _NSGetExecutablePath(buf: *mut c_char, size: *mut u32) -> c_int;
    }
    let mut size = 0u32;
    unsafe { _NSGetExecutablePath(ptr::null_mut(), &mut size) };
    if size == 0 || size > 32768 {
        return None;
    }
    let mut raw = vec![0u8; size as usize];
    if unsafe { _NSGetExecutablePath(raw.as_mut_ptr().cast(), &mut size) } != 0 {
        return None;
    }
    Some(raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())].to_vec())
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
    let ok = unsafe {
        sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut mem as *mut u64).cast(),
            &mut len,
            ptr::null_mut(),
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
    let pages = unsafe { sysconf(AVPHYS_PAGES) };
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
