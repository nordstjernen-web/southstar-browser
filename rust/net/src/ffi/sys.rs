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
