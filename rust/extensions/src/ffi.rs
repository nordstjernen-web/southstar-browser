//! Southstar — the C ABI of the extension host, as declared in src/ext.h, and the GLib, GRegex and libpsl calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
#[cfg(feature = "quickjs")]
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[cfg(windows)]
pub(crate) const DIR_SEPARATOR: u8 = b'\\';
#[cfg(not(windows))]
pub(crate) const DIR_SEPARATOR: u8 = b'/';
#[cfg(windows)]
pub(crate) const SEARCHPATH_SEPARATOR: u8 = b';';
#[cfg(not(windows))]
pub(crate) const SEARCHPATH_SEPARATOR: u8 = b':';

const G_REGEX_CASELESS: c_int = 1 << 0;

#[repr(C)]
struct GDir {
    _private: [u8; 0],
}

#[repr(C)]
struct GRegex {
    _private: [u8; 0],
}

#[repr(C)]
struct PslContext {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn g_canonicalize_filename(filename: *const c_char, relative_to: *const c_char) -> *mut c_char;
    fn g_path_get_basename(file_name: *const c_char) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut glib::GError,
    ) -> GBoolean;
    fn g_dir_open(path: *const c_char, flags: c_uint, error: *mut *mut glib::GError) -> *mut GDir;
    fn g_dir_read_name(dir: *mut GDir) -> *const c_char;
    fn g_dir_close(dir: *mut GDir);
    fn g_compute_checksum_for_string(
        checksum_type: glib::GChecksumType,
        text: *const c_char,
        length: isize,
    ) -> *mut c_char;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_int,
        match_options: c_int,
        error: *mut *mut glib::GError,
    ) -> *mut GRegex;
    fn g_regex_match(
        regex: *const GRegex,
        string: *const c_char,
        match_options: c_int,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut GRegex);
    fn psl_builtin() -> *const PslContext;
    fn psl_registrable_domain(psl: *const PslContext, domain: *const c_char) -> *const c_char;
}

pub(crate) struct Regex(NonNull<GRegex>);

unsafe impl Send for Regex {}
unsafe impl Sync for Regex {}

impl Regex {
    pub(crate) fn new(pattern: &[u8], case_sensitive: bool) -> Option<Regex> {
        let pattern = cstring(pattern);
        let options = if case_sensitive { 0 } else { G_REGEX_CASELESS };
        let regex = unsafe { g_regex_new(pattern.as_ptr(), options, 0, ptr::null_mut()) };
        NonNull::new(regex).map(Regex)
    }

    pub(crate) fn is_match(&self, text: &[u8]) -> bool {
        let text = cstring(text);
        unsafe { g_regex_match(self.0.as_ptr(), text.as_ptr(), 0, ptr::null_mut()) != 0 }
    }
}

impl Drop for Regex {
    fn drop(&mut self) {
        unsafe { g_regex_unref(self.0.as_ptr()) };
    }
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn take_string(text: *mut c_char) -> Vec<u8> {
    let bytes = unsafe { glib::bytes(text) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(text.cast()) };
    bytes
}

pub(crate) fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let owned: Vec<CString> = parts.iter().map(|part| cstring(part)).collect();
    let mut pointers: Vec<*mut c_char> = owned.iter().map(|p| p.as_ptr().cast_mut()).collect();
    pointers.push(ptr::null_mut());
    take_string(unsafe { glib::g_build_filenamev(pointers.as_mut_ptr()) })
}

pub(crate) fn canonicalize(path: &[u8]) -> Vec<u8> {
    let path = cstring(path);
    take_string(unsafe { g_canonicalize_filename(path.as_ptr(), ptr::null()) })
}

pub(crate) fn basename(path: &[u8]) -> Vec<u8> {
    let path = cstring(path);
    take_string(unsafe { g_path_get_basename(path.as_ptr()) })
}

pub(crate) fn user_data_dir() -> Vec<u8> {
    unsafe { glib::bytes(glib::g_get_user_data_dir()) }
        .unwrap_or_default()
        .to_vec()
}

pub(crate) fn getenv(name: &CStr) -> Option<Vec<u8>> {
    unsafe { glib::bytes(glib::g_getenv(name.as_ptr())) }.map(<[u8]>::to_vec)
}

pub(crate) fn language_name() -> Option<Vec<u8>> {
    let names = unsafe { glib::g_get_language_names() };
    if names.is_null() {
        return None;
    }
    unsafe { glib::bytes(*names) }.map(<[u8]>::to_vec)
}

pub(crate) fn sha256_hex(text: &[u8]) -> Vec<u8> {
    let text = cstring(text);
    take_string(unsafe {
        g_compute_checksum_for_string(glib::G_CHECKSUM_SHA256, text.as_ptr(), -1)
    })
}

fn file_test(path: &[u8], test: glib::GFileTest) -> bool {
    let path = cstring(path);
    unsafe { glib::g_file_test(path.as_ptr(), test) != 0 }
}

pub(crate) fn exists(path: &[u8]) -> bool {
    file_test(path, glib::FILE_TEST_EXISTS)
}

pub(crate) fn is_dir(path: &[u8]) -> bool {
    file_test(path, glib::FILE_TEST_IS_DIR)
}

pub(crate) fn list_dir(path: &[u8]) -> Vec<Vec<u8>> {
    let path = cstring(path);
    let dir = unsafe { g_dir_open(path.as_ptr(), 0, ptr::null_mut()) };
    if dir.is_null() {
        return Vec::new();
    }
    let mut names = Vec::new();
    while let Some(name) = unsafe { glib::bytes(g_dir_read_name(dir)) } {
        names.push(name.to_vec());
    }
    unsafe { g_dir_close(dir) };
    names
}

pub(crate) fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = cstring(path);
    let mut contents: *mut c_char = ptr::null_mut();
    let mut length = 0usize;
    let ok = unsafe {
        glib::g_file_get_contents(path.as_ptr(), &mut contents, &mut length, ptr::null_mut())
    };
    if ok == 0 {
        return None;
    }
    let data = unsafe { glib::slice(contents.cast(), length) }.to_vec();
    unsafe { glib::g_free(contents.cast()) };
    Some(data)
}

pub(crate) fn make_private_dir(path: &[u8]) {
    let path = cstring(path);
    unsafe {
        g_mkdir_with_parents(path.as_ptr(), 0o700);
        g_chmod(path.as_ptr(), 0o700);
    }
}

pub(crate) fn write_private_file(path: &[u8], data: &[u8]) -> bool {
    let path = cstring(path);
    let written = unsafe {
        g_file_set_contents(
            path.as_ptr(),
            data.as_ptr().cast(),
            data.len() as isize,
            ptr::null_mut(),
        )
    };
    if written == 0 {
        return false;
    }
    unsafe { g_chmod(path.as_ptr(), 0o600) };
    true
}

pub(crate) fn registrable_domain(host: &[u8]) -> Option<Vec<u8>> {
    let psl = unsafe { psl_builtin() };
    if psl.is_null() {
        return None;
    }
    let host = cstring(host);
    unsafe { glib::bytes(psl_registrable_domain(psl, host.as_ptr())) }.map(<[u8]>::to_vec)
}

#[cfg(feature = "quickjs")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ext_content_scripts_for_url(
    ctx: *mut JSContext,
    global: JSValue,
    url: *const c_char,
    at_start: GBoolean,
) -> *mut c_char {
    let url = unsafe { glib::bytes(url) };
    let script = unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::content_scripts_for_url(scope, &global, url, at_start != 0)
        })
    };
    script.map_or(ptr::null_mut(), |script| glib::strdup(&script))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ext_should_block(
    url: *const c_char,
    initiator: *const c_char,
) -> GBoolean {
    let url = unsafe { glib::bytes(url) };
    let initiator = unsafe { glib::bytes(initiator) };
    glib::boolean(crate::should_block(url, initiator))
}
