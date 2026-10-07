//! Southstar — the GLib functions and types ported modules share with the C side, linked by meson.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;

pub type GBoolean = c_int;
pub const TRUE: GBoolean = 1;
pub const FALSE: GBoolean = 0;

pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;

pub type GFileTest = c_uint;
pub const FILE_TEST_IS_REGULAR: GFileTest = 1 << 0;
pub const FILE_TEST_EXISTS: GFileTest = 1 << 4;

#[repr(C)]
pub struct GPtrArray {
    pub pdata: *mut *mut c_void,
    pub len: c_uint,
}

#[repr(C)]
pub struct GHashTable {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GChecksum {
    _private: [u8; 0],
}

pub type GChecksumType = c_int;
pub const G_CHECKSUM_SHA256: GChecksumType = 2;
pub const G_CHECKSUM_SHA512: GChecksumType = 3;
pub const G_CHECKSUM_SHA384: GChecksumType = 4;

#[repr(C)]
pub struct GError {
    pub domain: u32,
    pub code: c_int,
    pub message: *mut c_char,
}

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
    pub fn g_ptr_array_new_with_free_func(free_func: GDestroyNotify) -> *mut GPtrArray;
    pub fn g_ptr_array_add(array: *mut GPtrArray, data: *mut c_void);
    pub fn g_ptr_array_unref(array: *mut GPtrArray);
    pub fn g_ptr_array_free(array: *mut GPtrArray, free_segment: GBoolean) -> *mut *mut c_void;
    pub fn g_hash_table_new_full(
        hash_func: Option<unsafe extern "C" fn(key: *const c_void) -> c_uint>,
        key_equal_func: Option<
            unsafe extern "C" fn(a: *const c_void, b: *const c_void) -> GBoolean,
        >,
        key_destroy_func: GDestroyNotify,
        value_destroy_func: GDestroyNotify,
    ) -> *mut GHashTable;
    pub fn g_hash_table_insert(
        table: *mut GHashTable,
        key: *mut c_void,
        value: *mut c_void,
    ) -> GBoolean;
    pub fn g_hash_table_lookup(table: *mut GHashTable, key: *const c_void) -> *mut c_void;
    pub fn g_hash_table_unref(table: *mut GHashTable);
    pub fn g_str_hash(v: *const c_void) -> c_uint;
    pub fn g_str_equal(a: *const c_void, b: *const c_void) -> GBoolean;
    pub fn g_ascii_strtod(nptr: *const c_char, endptr: *mut *mut c_char) -> f64;
    pub fn g_base64_encode(data: *const u8, len: usize) -> *mut c_char;
    pub fn g_checksum_free(checksum: *mut GChecksum);
    pub fn g_checksum_get_digest(checksum: *mut GChecksum, buffer: *mut u8, digest_len: *mut usize);
    pub fn g_checksum_new(checksum_type: GChecksumType) -> *mut GChecksum;
    pub fn g_checksum_update(checksum: *mut GChecksum, data: *const u8, length: isize);
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
    let mut terminated = Vec::with_capacity(text.len() + 1);
    terminated.extend_from_slice(text);
    terminated.push(0);
    unsafe { g_ascii_strtod(terminated.as_ptr().cast(), ptr::null_mut()) }
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
