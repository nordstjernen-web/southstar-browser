//! Southstar — the calls behind the about: pages: install data files, the configuration, history and storage, the public suffix list and the library versions about:southstar reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::collections::HashMap;
use std::ffi::CString;

use southstar_config::NsConfig;
use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GStr};

const URI_PARAMS_WWW_FORM: c_uint = 1 << 1;
const OPENSSL_VERSION: c_int = 0;

unsafe extern "C" {
    fn ns_app_self_exe() -> *const c_char;
    fn g_get_system_data_dirs() -> *const *const c_char;
    fn g_random_int_range(begin: i32, end: i32) -> i32;
    fn g_get_os_info(key: *const c_char) -> *mut c_char;
    fn g_get_num_processors() -> c_uint;
    fn g_ascii_strdown(text: *const c_char, len: isize) -> *mut c_char;
    fn g_uri_parse_params(
        params: *const c_char,
        length: isize,
        separators: *const c_char,
        flags: c_uint,
        error: *mut *mut GError,
    ) -> *mut GHashTable;
    fn g_hash_table_iter_init(iter: *mut GHashTableIter, table: *mut GHashTable);
    fn g_hash_table_iter_next(
        iter: *mut GHashTableIter,
        key: *mut *mut c_void,
        value: *mut *mut c_void,
    ) -> GBoolean;
    fn atoi(text: *const c_char) -> c_int;
    fn ns_url_host_from(url: *const c_char) -> *mut c_char;
    fn psl_builtin() -> *const c_void;
    fn psl_registrable_domain(psl: *const c_void, domain: *const c_char) -> *const c_char;
    fn ns_config_lock();
    fn ns_config_unlock();
    fn ns_config_reload();
    fn ns_config_get() -> *const NsConfig;
    fn ns_config_mut() -> *mut NsConfig;
    fn ns_config_save(error: *mut *mut GError) -> GBoolean;
    fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char;
    fn ns_history_html_page() -> *mut c_char;
    fn ns_history_clear();
    fn ns_cache_clear();
    fn ns_net_cookies_clear();
    fn ns_net_site_storage_clear();
    fn ns_js_engine_version() -> *const c_char;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_version_string")]
    fn pango_version_string() -> *const c_char;
    fn cairo_version_string() -> *const c_char;
    fn sqlite3_libversion() -> *const c_char;
    fn WebPGetDecoderVersion() -> c_int;
    fn OpenSSL_version(kind: c_int) -> *const c_char;
    fn ns_rust_compiler_version() -> *const c_char;
    fn ns_rust_minimum_version() -> *const c_char;
    fn ns_rust_build_profile() -> *const c_char;
    fn ns_rust_module_count() -> c_uint;
    fn ns_rust_modules() -> *const c_char;
}

#[cfg(feature = "libav")]
unsafe extern "C" {
    fn av_version_info() -> *const c_char;
}

#[cfg_attr(windows, link(name = "glib-2.0", kind = "dylib"))]
unsafe extern "C" {
    static glib_major_version: c_uint;
    static glib_minor_version: c_uint;
    static glib_micro_version: c_uint;
}

#[repr(C)]
struct GHashTableIter {
    _dummy: [*mut c_void; 6],
}

fn c(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn owned(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

fn borrowed(p: *const c_char) -> Option<Vec<u8>> {
    unsafe { glib::bytes(p) }.map(<[u8]>::to_vec)
}

pub fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let path = c(path);
    let mut contents: *mut c_char = ptr::null_mut();
    let mut len = 0usize;
    let ok = unsafe {
        glib::g_file_get_contents(path.as_ptr(), &mut contents, &mut len, ptr::null_mut())
    };
    if ok == 0 {
        return None;
    }
    let bytes = unsafe { glib::slice(contents.cast(), len) }.to_vec();
    unsafe { glib::g_free(contents.cast()) };
    Some(bytes)
}

pub fn self_exe() -> Option<Vec<u8>> {
    borrowed(unsafe { ns_app_self_exe() })
}

pub fn user_data_dir() -> Option<Vec<u8>> {
    borrowed(unsafe { glib::g_get_user_data_dir() })
}

pub fn system_data_dirs() -> Vec<Vec<u8>> {
    let mut dirs = Vec::new();
    let mut p = unsafe { g_get_system_data_dirs() };
    if p.is_null() {
        return dirs;
    }
    while let Some(dir) = borrowed(unsafe { *p }) {
        dirs.push(dir);
        p = unsafe { p.add(1) };
    }
    dirs
}

pub fn base64(data: &[u8]) -> Vec<u8> {
    owned(unsafe { glib::g_base64_encode(data.as_ptr(), data.len()) }).unwrap_or_default()
}

pub fn markup_escape(text: &[u8]) -> Vec<u8> {
    let text = c(text);
    owned(unsafe { glib::g_markup_escape_text(text.as_ptr(), -1) }).unwrap_or_default()
}

pub fn random_below(n: i32) -> usize {
    usize::try_from(unsafe { g_random_int_range(0, n) }).unwrap_or(0)
}

pub fn os_info(key: &CStr) -> Option<Vec<u8>> {
    owned(unsafe { g_get_os_info(key.as_ptr()) })
}

pub fn processors() -> u32 {
    unsafe { g_get_num_processors() }
}

pub fn url_host(url: &[u8]) -> Option<Vec<u8>> {
    let url = c(url);
    owned(unsafe { ns_url_host_from(url.as_ptr()) })
}

pub fn registrable_domain(host: &[u8]) -> Option<Vec<u8>> {
    let host = c(host);
    let lower = unsafe { GStr::take(g_ascii_strdown(host.as_ptr(), -1)) }?;
    let psl = unsafe { psl_builtin() };
    if psl.is_null() {
        return None;
    }
    borrowed(unsafe { psl_registrable_domain(psl, lower.as_ptr()) })
}

pub fn parse_form(form: &[u8]) -> Option<HashMap<Vec<u8>, Vec<u8>>> {
    let form = c(form);
    let table = unsafe {
        g_uri_parse_params(
            form.as_ptr(),
            -1,
            c"&".as_ptr(),
            URI_PARAMS_WWW_FORM,
            ptr::null_mut(),
        )
    };
    if table.is_null() {
        return None;
    }
    let mut fields = HashMap::new();
    let mut iter = GHashTableIter {
        _dummy: [ptr::null_mut(); 6],
    };
    unsafe { g_hash_table_iter_init(&mut iter, table) };
    let mut key: *mut c_void = ptr::null_mut();
    let mut value: *mut c_void = ptr::null_mut();
    while unsafe { g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
        if let (Some(k), Some(v)) = (borrowed(key.cast()), borrowed(value.cast())) {
            fields.insert(k, v);
        }
    }
    unsafe { glib::g_hash_table_destroy(table) };
    Some(fields)
}

pub fn atoi_of(text: &[u8]) -> i32 {
    let text = c(text);
    unsafe { atoi(text.as_ptr()) }
}

pub struct ConfigGuard;

impl ConfigGuard {
    pub fn lock() -> ConfigGuard {
        unsafe { ns_config_lock() };
        ConfigGuard
    }

    pub fn reload(&self) {
        unsafe { ns_config_reload() };
    }

    pub fn get(&self) -> Option<&NsConfig> {
        unsafe { ns_config_get().as_ref() }
    }

    pub fn get_mut(&mut self) -> Option<&mut NsConfig> {
        unsafe { ns_config_mut().as_mut() }
    }

    pub fn save(&self) {
        unsafe { ns_config_save(ptr::null_mut()) };
    }
}

impl Drop for ConfigGuard {
    fn drop(&mut self) {
        unsafe { ns_config_unlock() };
    }
}

pub fn config_text(p: *const c_char) -> Option<Vec<u8>> {
    borrowed(p)
}

pub fn set_config_text(field: &mut *mut c_char, value: &[u8]) {
    unsafe { glib::g_free((*field).cast()) };
    *field = glib::strdup(value);
}

pub fn unlocked_config() -> Option<&'static NsConfig> {
    unsafe { ns_config_get().as_ref() }
}

pub fn user_agent_for_mode(compat_mode: *const c_char) -> Option<Vec<u8>> {
    borrowed(unsafe { ns_user_agent_for_mode(compat_mode) })
}

pub fn history_page() -> Vec<u8> {
    owned(unsafe { ns_history_html_page() }).unwrap_or_default()
}

pub fn clear_browsing_data() {
    unsafe {
        ns_history_clear();
        ns_cache_clear();
        ns_net_cookies_clear();
        ns_net_site_storage_clear();
    }
}

pub struct Versions {
    pub js_engine: Option<Vec<u8>>,
    pub glib: [u32; 3],
    pub pango: Option<Vec<u8>>,
    pub cairo: Option<Vec<u8>>,
    pub sqlite: Option<Vec<u8>>,
    pub webp: i32,
    pub libav: Option<Vec<u8>>,
    pub openssl: Option<Vec<u8>>,
    pub networking: Vec<u8>,
}

#[cfg(feature = "libav")]
fn libav_version() -> Option<Vec<u8>> {
    borrowed(unsafe { av_version_info() })
}

#[cfg(not(feature = "libav"))]
fn libav_version() -> Option<Vec<u8>> {
    None
}

pub fn versions() -> Versions {
    unsafe {
        Versions {
            js_engine: borrowed(ns_js_engine_version()),
            glib: [glib_major_version, glib_minor_version, glib_micro_version],
            pango: borrowed(pango_version_string()),
            cairo: borrowed(cairo_version_string()),
            sqlite: borrowed(sqlite3_libversion()),
            webp: WebPGetDecoderVersion(),
            libav: libav_version(),
            openssl: borrowed(OpenSSL_version(OPENSSL_VERSION)),
            networking: southstar_http::description().to_vec(),
        }
    }
}

pub struct RustInfo {
    pub compiler: Option<Vec<u8>>,
    pub minimum: Option<Vec<u8>>,
    pub profile: Option<Vec<u8>>,
    pub module_count: u32,
    pub modules: Option<Vec<u8>>,
}

pub fn rust_info() -> RustInfo {
    unsafe {
        RustInfo {
            compiler: borrowed(ns_rust_compiler_version()),
            minimum: borrowed(ns_rust_minimum_version()),
            profile: borrowed(ns_rust_build_profile()),
            module_count: ns_rust_module_count(),
            modules: borrowed(ns_rust_modules()),
        }
    }
}

pub fn env_set(name: &CStr) -> bool {
    !unsafe { glib::g_getenv(name.as_ptr()) }.is_null()
}
