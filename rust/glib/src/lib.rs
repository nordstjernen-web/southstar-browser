//! Southstar — the GLib functions and types ported modules share with the C side, linked by meson.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;

mod main_context;
mod strtod;
#[cfg(test)]
mod tests;

pub use main_context::MainContext;

pub type GBoolean = c_int;
pub const TRUE: GBoolean = 1;
pub const FALSE: GBoolean = 0;

pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;

pub type GFileTest = c_uint;
pub const FILE_TEST_IS_REGULAR: GFileTest = 1 << 0;
pub const FILE_TEST_IS_DIR: GFileTest = 1 << 2;
pub const FILE_TEST_EXISTS: GFileTest = 1 << 4;

#[repr(C)]
pub struct GPtrArray {
    pub pdata: *mut *mut c_void,
    pub len: c_uint,
}

#[repr(C)]
pub struct GArray {
    pub data: *mut c_char,
    pub len: c_uint,
}

#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    pub fn g_malloc(n_bytes: usize) -> *mut c_void;
    pub fn g_malloc0(n_bytes: usize) -> *mut c_void;
    pub fn g_realloc(mem: *mut c_void, n_bytes: usize) -> *mut c_void;
    pub fn g_free(mem: *mut c_void);
    pub fn g_strdup(s: *const c_char) -> *mut c_char;
    pub fn g_strndup(s: *const c_char, n: usize) -> *mut c_char;
    pub fn g_strfreev(v: *mut *mut c_char);
    pub fn g_markup_escape_text(text: *const c_char, length: isize) -> *mut c_char;
    pub fn g_getenv(variable: *const c_char) -> *const c_char;
    pub fn g_get_user_config_dir() -> *const c_char;
    pub fn g_get_user_data_dir() -> *const c_char;
    pub fn g_get_user_cache_dir() -> *const c_char;
    pub fn g_get_language_names() -> *const *const c_char;
    pub fn g_file_get_contents(
        filename: *const c_char,
        contents: *mut *mut c_char,
        length: *mut usize,
        error: *mut *mut GError,
    ) -> GBoolean;
    pub fn g_error_free(error: *mut GError);
    pub fn g_file_test(filename: *const c_char, test: GFileTest) -> GBoolean;
    pub fn g_file_read_link(filename: *const c_char, error: *mut *mut GError) -> *mut c_char;
    pub fn g_build_filenamev(args: *mut *mut c_char) -> *mut c_char;
    pub fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    pub fn g_utf16_to_utf8(
        str: *const u16,
        len: c_long,
        items_read: *mut c_long,
        items_written: *mut c_long,
        error: *mut *mut GError,
    ) -> *mut c_char;
    pub fn g_array_new(
        zero_terminated: GBoolean,
        clear: GBoolean,
        element_size: c_uint,
    ) -> *mut GArray;
    pub fn g_array_append_vals(array: *mut GArray, data: *const c_void, len: c_uint)
    -> *mut GArray;
    pub fn g_ptr_array_new_with_free_func(free_func: GDestroyNotify) -> *mut GPtrArray;
    pub fn g_ptr_array_add(array: *mut GPtrArray, data: *mut c_void);
    pub fn g_ptr_array_unref(array: *mut GPtrArray);
    pub fn g_ptr_array_free(array: *mut GPtrArray, free_segment: GBoolean) -> *mut *mut c_void;
    pub fn g_ptr_array_new() -> *mut GPtrArray;
    pub fn g_ptr_array_insert(array: *mut GPtrArray, index: c_int, data: *mut c_void);
    pub fn g_ptr_array_set_size(array: *mut GPtrArray, length: c_int);
    pub fn g_ptr_array_remove_index(array: *mut GPtrArray, index: c_uint) -> *mut c_void;
    pub fn g_ptr_array_sort(
        array: *mut GPtrArray,
        compare_func: Option<unsafe extern "C" fn(a: *const c_void, b: *const c_void) -> c_int>,
    );
    pub fn g_str_hash(v: *const c_void) -> c_uint;
    pub fn g_str_equal(a: *const c_void, b: *const c_void) -> GBoolean;
}

pub struct GStr(core::ptr::NonNull<c_char>);

impl GStr {
    pub unsafe fn take(s: *mut c_char) -> Option<Self> {
        core::ptr::NonNull::new(s).map(GStr)
    }
}

impl core::ops::Deref for GStr {
    type Target = CStr;

    fn deref(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.0.as_ptr()) }
    }
}

impl Drop for GStr {
    fn drop(&mut self) {
        unsafe { g_free(self.0.as_ptr().cast()) };
    }
}

pub fn boolean(value: bool) -> GBoolean {
    if value { TRUE } else { FALSE }
}

pub fn strdup(bytes: &[u8]) -> *mut c_char {
    unsafe {
        let out = g_malloc(bytes.len() + 1).cast::<u8>();
        ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
        *out.add(bytes.len()) = 0;
        out.cast()
    }
}

pub fn ascii_strtod(text: &[u8]) -> f64 {
    ascii_strtod_prefix(text).0
}

pub fn ascii_strtod_prefix(text: &[u8]) -> (f64, usize) {
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    strtod::parse_prefix(text)
}

pub fn ascii_strtod_at(text: &CStr, pos: usize) -> (f64, usize) {
    let bytes = text.to_bytes();
    let pos = pos.min(bytes.len());
    let (value, consumed) = strtod::parse_prefix(&bytes[pos..]);
    (value, pos + consumed)
}

#[cfg(unix)]
pub fn path_from_bytes(bytes: &[u8]) -> Option<std::path::PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(std::ffi::OsStr::from_bytes(bytes).into())
}

#[cfg(not(unix))]
pub fn path_from_bytes(bytes: &[u8]) -> Option<std::path::PathBuf> {
    core::str::from_utf8(bytes).ok().map(Into::into)
}

#[cfg(unix)]
pub fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
pub fn env_bytes(name: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStringExt;
    std::env::var_os(name).map(OsStringExt::into_vec)
}

#[cfg(not(unix))]
pub fn env_bytes(name: &str) -> Option<Vec<u8>> {
    std::env::var_os(name)?
        .into_string()
        .ok()
        .map(String::into_bytes)
}

pub fn file_get_contents(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(path_from_bytes(path)?).ok()
}

pub unsafe fn bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

pub unsafe fn slice<'a>(p: *const u8, len: usize) -> &'a [u8] {
    if p.is_null() || len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(p, len) }
    }
}

#[cfg(windows)]
unsafe extern "C" {
    fn __acrt_iob_func(index: c_uint) -> *mut c_void;
    fn fwrite(data: *const c_void, size: usize, count: usize, stream: *mut c_void) -> usize;
}

#[cfg(windows)]
pub fn stderr_write(bytes: &[u8]) {
    unsafe { fwrite(bytes.as_ptr().cast(), 1, bytes.len(), __acrt_iob_func(2)) };
}

#[cfg(not(windows))]
pub fn stderr_write(bytes: &[u8]) {
    use std::io::Write;
    let _ = std::io::stderr().write_all(bytes);
}
