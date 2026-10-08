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
pub struct GHashTable {
    _private: [u8; 0],
}

#[repr(C)]
pub struct GHashTableIter {
    _dummy: [*mut c_void; 6],
}

impl GHashTableIter {
    pub const fn new() -> GHashTableIter {
        GHashTableIter {
            _dummy: [ptr::null_mut(); 6],
        }
    }
}

impl Default for GHashTableIter {
    fn default() -> GHashTableIter {
        GHashTableIter::new()
    }
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
    pub fn g_hash_table_new(
        hash_func: Option<unsafe extern "C" fn(key: *const c_void) -> c_uint>,
        key_equal_func: Option<
            unsafe extern "C" fn(a: *const c_void, b: *const c_void) -> GBoolean,
        >,
    ) -> *mut GHashTable;
    pub fn g_hash_table_replace(
        table: *mut GHashTable,
        key: *mut c_void,
        value: *mut c_void,
    ) -> GBoolean;
    pub fn g_hash_table_add(table: *mut GHashTable, key: *mut c_void) -> GBoolean;
    pub fn g_hash_table_remove(table: *mut GHashTable, key: *const c_void) -> GBoolean;
    pub fn g_hash_table_contains(table: *mut GHashTable, key: *const c_void) -> GBoolean;
    pub fn g_hash_table_remove_all(table: *mut GHashTable);
    pub fn g_hash_table_size(table: *mut GHashTable) -> c_uint;
    pub fn g_hash_table_iter_init(iter: *mut GHashTableIter, table: *mut GHashTable);
    pub fn g_hash_table_iter_next(
        iter: *mut GHashTableIter,
        key: *mut *mut c_void,
        value: *mut *mut c_void,
    ) -> GBoolean;
    pub fn g_hash_table_destroy(table: *mut GHashTable);
    pub fn g_hash_table_get_keys_as_array(
        table: *mut GHashTable,
        length: *mut c_uint,
    ) -> *mut *mut c_void;
    pub fn g_direct_hash(v: *const c_void) -> c_uint;
    pub fn g_direct_equal(a: *const c_void, b: *const c_void) -> GBoolean;
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
    pub fn g_ascii_strtod(nptr: *const c_char, endptr: *mut *mut c_char) -> f64;
    pub fn g_ascii_dtostr(buffer: *mut c_char, buf_len: c_int, d: f64) -> *mut c_char;
    pub fn g_base64_encode(data: *const u8, len: usize) -> *mut c_char;
    pub fn g_checksum_free(checksum: *mut GChecksum);
    pub fn g_checksum_get_digest(checksum: *mut GChecksum, buffer: *mut u8, digest_len: *mut usize);
    pub fn g_checksum_new(checksum_type: GChecksumType) -> *mut GChecksum;
    pub fn g_checksum_update(checksum: *mut GChecksum, data: *const u8, length: isize);
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

pub struct HashTableEntries {
    iter: GHashTableIter,
    live: bool,
}

impl Iterator for HashTableEntries {
    type Item = (*mut c_void, *mut c_void);

    fn next(&mut self) -> Option<Self::Item> {
        if !self.live {
            return None;
        }
        let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
        self.live = unsafe { g_hash_table_iter_next(&mut self.iter, &mut key, &mut value) } != 0;
        self.live.then_some((key, value))
    }
}

pub unsafe fn hash_table_entries(table: *mut GHashTable) -> HashTableEntries {
    let mut iter = GHashTableIter::new();
    if !table.is_null() {
        unsafe { g_hash_table_iter_init(&mut iter, table) };
    }
    HashTableEntries {
        iter,
        live: !table.is_null(),
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
    let mut terminated = Vec::with_capacity(text.len() + 1);
    terminated.extend_from_slice(text);
    terminated.push(0);
    unsafe { g_ascii_strtod(terminated.as_ptr().cast(), ptr::null_mut()) }
}

pub fn ascii_dtostr(value: f64) -> Vec<u8> {
    let mut buffer = [0 as c_char; 39];
    unsafe {
        g_ascii_dtostr(buffer.as_mut_ptr(), buffer.len() as c_int, value);
        CStr::from_ptr(buffer.as_ptr()).to_bytes().to_vec()
    }
}

pub fn ascii_strtod_prefix(text: &[u8]) -> (f64, usize) {
    let mut terminated = Vec::with_capacity(text.len() + 1);
    terminated.extend_from_slice(text);
    terminated.push(0);
    let start = terminated.as_ptr().cast::<c_char>();
    let mut end: *mut c_char = ptr::null_mut();
    let value = unsafe { g_ascii_strtod(start, &mut end) };
    let consumed = if end.is_null() {
        0
    } else {
        (end.cast_const() as usize).saturating_sub(start as usize)
    };
    (value, consumed.min(text.len()))
}

pub fn ascii_strtod_at(text: &CStr, pos: usize) -> (f64, usize) {
    let pos = pos.min(text.to_bytes().len());
    let start = unsafe { text.as_ptr().add(pos) };
    let mut end: *mut c_char = ptr::null_mut();
    let value = unsafe { g_ascii_strtod(start, &mut end) };
    let consumed = if end.is_null() {
        0
    } else {
        (end.cast_const() as usize).saturating_sub(start as usize)
    };
    (value, pos + consumed.min(text.to_bytes().len() - pos))
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
