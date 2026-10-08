//! Southstar — the GLib, GIO and C stdio calls behind file: and FTP responses: file names and URIs, folders, stat, local times, content types and C strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::path::PathBuf;

use southstar_glib::{self as glib, GBoolean, GError, GStr};

#[repr(C)]
struct GDir {
    _private: [u8; 0],
}

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct File {
    _private: [u8; 0],
}

const FILE_ERROR_ACCES: c_int = 2;
const FILE_ERROR_NOENT: c_int = 4;
const FILE_TEST_IS_SYMLINK: c_uint = 1 << 1;
const CHECKSUM_SHA256: c_int = 2;
const LOG_LEVEL_WARNING: c_int = 1 << 4;

unsafe extern "C" {
    fn g_utf8_collate(str1: *const c_char, str2: *const c_char) -> c_int;
    fn g_ascii_strtoull(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> u64;
    fn g_filename_display_name(filename: *const c_char) -> *mut c_char;
    fn g_filename_to_uri(
        filename: *const c_char,
        hostname: *const c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_filename_from_uri(
        uri: *const c_char,
        hostname: *mut *mut c_char,
        error: *mut *mut GError,
    ) -> *mut c_char;
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_dir_open(path: *const c_char, flags: c_uint, error: *mut *mut GError) -> *mut GDir;
    fn g_dir_read_name(dir: *mut GDir) -> *const c_char;
    fn g_dir_close(dir: *mut GDir);
    fn g_file_error_quark() -> u32;
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
    fn g_fopen(filename: *const c_char, mode: *const c_char) -> *mut File;
    fn fread(ptr: *mut c_void, size: usize, nmemb: usize, stream: *mut File) -> usize;
    fn ferror(stream: *mut File) -> c_int;
    fn fclose(stream: *mut File) -> c_int;
    fn g_strerror(errnum: c_int) -> *const c_char;
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> i64;
    fn g_get_tmp_dir() -> *const c_char;
    fn g_dir_make_tmp(tmpl: *const c_char, error: *mut *mut GError) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_unlink(filename: *const c_char) -> c_int;
    fn g_rmdir(filename: *const c_char) -> c_int;
    fn g_random_int() -> u32;
    fn g_get_real_time() -> i64;
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_compute_checksum_for_string(
        checksum_type: c_int,
        text: *const c_char,
        length: isize,
    ) -> *mut c_char;
    fn curl_getdate(datestring: *const c_char, now: *const i64) -> i64;
    fn psl_builtin() -> *const c_void;
    fn psl_is_public_suffix(psl: *const c_void, domain: *const c_char) -> c_int;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
    fn g_get_monotonic_time() -> i64;
    fn ns_app_self_exe() -> *const c_char;
}

#[cfg(windows)]
unsafe extern "C" {
    fn _errno() -> *mut c_int;
}

fn errno() -> c_int {
    #[cfg(windows)]
    {
        unsafe { *_errno() }
    }
    #[cfg(not(windows))]
    {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
    }
}

fn c(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

fn strerror(errnum: c_int) -> Vec<u8> {
    unsafe { CStr::from_ptr(g_strerror(errnum)) }
        .to_bytes()
        .to_vec()
}

pub fn utf8_collate(a: &[u8], b: &[u8]) -> Ordering {
    let (a, b) = (c(a), c(b));
    unsafe { g_utf8_collate(a.as_ptr(), b.as_ptr()) }.cmp(&0)
}

pub fn ascii_strtoull(text: &[u8]) -> u64 {
    let text = c(text);
    unsafe { g_ascii_strtoull(text.as_ptr(), ptr::null_mut(), 10) }
}

pub fn filename_display_name(name: &[u8]) -> Vec<u8> {
    let name = c(name);
    take(unsafe { g_filename_display_name(name.as_ptr()) }).unwrap_or_default()
}

pub fn filename_to_uri(path: &[u8]) -> Option<Vec<u8>> {
    let path = c(path);
    take(unsafe { g_filename_to_uri(path.as_ptr(), ptr::null(), ptr::null_mut()) })
}

pub fn filename_from_uri(uri: &[u8]) -> Option<Vec<u8>> {
    let uri = c(uri);
    take(unsafe { g_filename_from_uri(uri.as_ptr(), ptr::null_mut(), ptr::null_mut()) })
}

pub fn canonicalize_filename(path: &[u8]) -> Vec<u8> {
    let path = c(path);
    take(unsafe { g_canonicalize_filename(path.as_ptr(), ptr::null()) }).unwrap_or_default()
}

pub fn build_filename(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let (dir, name) = (c(dir), c(name));
    let mut args = [
        dir.as_ptr().cast_mut(),
        name.as_ptr().cast_mut(),
        ptr::null_mut(),
    ];
    take(unsafe { glib::g_build_filenamev(args.as_mut_ptr()) }).unwrap_or_default()
}

pub fn path_dirname(path: &[u8]) -> Vec<u8> {
    let path = c(path);
    take(unsafe { glib::g_path_get_dirname(path.as_ptr()) }).unwrap_or_default()
}

pub fn is_dir(path: &[u8]) -> bool {
    let path = c(path);
    unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_IS_DIR) != 0 }
}

pub struct DirError {
    pub denied: bool,
    pub message: Option<Vec<u8>>,
}

pub fn read_dir(path: &[u8]) -> Result<Vec<Vec<u8>>, DirError> {
    let path = c(path);
    let mut err: *mut GError = ptr::null_mut();
    let dir = unsafe { g_dir_open(path.as_ptr(), 0, &mut err) };
    if dir.is_null() {
        let failure = match unsafe { err.as_ref() } {
            Some(e) => DirError {
                denied: e.domain == unsafe { g_file_error_quark() } && e.code == FILE_ERROR_ACCES,
                message: unsafe { glib::bytes(e.message) }.map(<[u8]>::to_vec),
            },
            None => DirError {
                denied: false,
                message: None,
            },
        };
        if !err.is_null() {
            unsafe { glib::g_error_free(err) };
        }
        return Err(failure);
    }
    let mut names = Vec::new();
    loop {
        let name = unsafe { g_dir_read_name(dir) };
        match unsafe { glib::bytes(name) } {
            Some(name) => names.push(name.to_vec()),
            None => break,
        }
    }
    unsafe { g_dir_close(dir) };
    Ok(names)
}

#[derive(Clone, Copy)]
pub struct Stat {
    pub size: i64,
    pub mtime: i64,
}

#[cfg(unix)]
fn os_path(path: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(path)))
}

#[cfg(not(unix))]
fn os_path(path: &[u8]) -> Option<PathBuf> {
    core::str::from_utf8(path).ok().map(PathBuf::from)
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

pub struct CFile(*mut File);

impl CFile {
    pub fn open(path: &[u8]) -> Result<CFile, Vec<u8>> {
        let path = c(path);
        let file = unsafe { g_fopen(path.as_ptr(), c"rb".as_ptr()) };
        if file.is_null() {
            return Err(strerror(errno()));
        }
        Ok(CFile(file))
    }

    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { fread(buf.as_mut_ptr().cast(), 1, buf.len(), self.0) }
    }

    pub fn error(&mut self) -> Option<Vec<u8>> {
        (unsafe { ferror(self.0) } != 0).then(|| strerror(errno()))
    }
}

impl Drop for CFile {
    fn drop(&mut self) {
        unsafe { fclose(self.0) };
    }
}

pub fn ascii_strtoll(text: &[u8]) -> (i64, usize) {
    let text = c(text);
    let mut end: *mut c_char = ptr::null_mut();
    let value = unsafe { g_ascii_strtoll(text.as_ptr(), &mut end, 10) };
    let consumed = if end.is_null() {
        0
    } else {
        (end as usize).wrapping_sub(text.as_ptr() as usize)
    };
    (value, consumed)
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

pub fn make_tmp_dir(template: &CStr) -> Option<Vec<u8>> {
    take(unsafe { g_dir_make_tmp(template.as_ptr(), ptr::null_mut()) })
}

pub fn mkdir_with_parents(path: &[u8], mode: c_int) {
    let path = c(path);
    unsafe { g_mkdir_with_parents(path.as_ptr(), mode) };
}

pub fn chmod(path: &[u8], mode: c_int) {
    let path = c(path);
    unsafe { g_chmod(path.as_ptr(), mode) };
}

pub fn random_u32() -> u32 {
    unsafe { g_random_int() }
}

pub fn now_seconds() -> i64 {
    (unsafe { g_get_real_time() }) / 1_000_000
}

fn is_symlink(path: &CString) -> bool {
    unsafe { glib::g_file_test(path.as_ptr(), FILE_TEST_IS_SYMLINK) != 0 }
}

pub fn empty_dir(dir: &[u8]) {
    let Ok(names) = read_dir(dir) else {
        return;
    };
    for name in names {
        let child = build_filename(dir, &name);
        let child_c = c(&child);
        if is_dir(&child) && !is_symlink(&child_c) {
            remove_tree(&child);
        } else {
            unsafe { g_unlink(child_c.as_ptr()) };
        }
    }
}

pub fn remove_tree(path: &[u8]) {
    empty_dir(path);
    let path = c(path);
    unsafe { g_rmdir(path.as_ptr()) };
}

pub enum ReadError {
    Missing,
    Other(Vec<u8>),
}

pub fn read_file(path: &[u8]) -> Result<Vec<u8>, ReadError> {
    let path = c(path);
    let mut contents: *mut c_char = ptr::null_mut();
    let mut len = 0usize;
    let mut err: *mut GError = ptr::null_mut();
    let ok = unsafe { glib::g_file_get_contents(path.as_ptr(), &mut contents, &mut len, &mut err) };
    if ok == 0 {
        let failure = match unsafe { err.as_ref() } {
            Some(e)
                if e.domain == unsafe { g_file_error_quark() } && e.code == FILE_ERROR_NOENT =>
            {
                ReadError::Missing
            }
            Some(e) => ReadError::Other(
                unsafe { glib::bytes(e.message) }
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default(),
            ),
            None => ReadError::Other(Vec::new()),
        };
        if !err.is_null() {
            unsafe { glib::g_error_free(err) };
        }
        return Err(failure);
    }
    let bytes = unsafe { glib::slice(contents.cast(), len) }.to_vec();
    unsafe { glib::g_free(contents.cast()) };
    Ok(bytes)
}

pub fn write_file(path: &[u8], contents: &[u8]) -> bool {
    let path = c(path);
    let ok = unsafe {
        g_file_set_contents(
            path.as_ptr(),
            contents.as_ptr().cast(),
            contents.len() as isize,
            ptr::null_mut(),
        )
    };
    ok != 0
}

pub fn sha256_hex(text: &[u8]) -> Vec<u8> {
    let text = c(text);
    take(unsafe { g_compute_checksum_for_string(CHECKSUM_SHA256, text.as_ptr(), -1) })
        .unwrap_or_default()
}

pub fn http_date(text: &[u8]) -> Option<i64> {
    let text = c(text);
    let t = unsafe { curl_getdate(text.as_ptr(), ptr::null()) };
    (t != -1).then_some(t)
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
    let path = c(path);
    unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_EXISTS) != 0 }
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
    let own = unsafe { glib::bytes(ns_app_self_exe()) }
        .filter(|e| !e.is_empty())
        .map(<[u8]>::to_vec);
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
