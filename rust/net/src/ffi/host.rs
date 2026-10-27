//! Southstar — the calls behind the about: pages: install data files, the configuration, history and storage, the public suffix list and the library versions about:southstar reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Mutex;

use southstar_config::NsConfig;
use southstar_config::util;
use southstar_glib::{self as glib, GStr};

const OPENSSL_VERSION: c_int = 0;

unsafe extern "C" {
    fn g_get_system_data_dirs() -> *const *const c_char;
    fn g_get_os_info(key: *const c_char) -> *mut c_char;
    fn psl_builtin() -> *const c_void;
    fn psl_registrable_domain(psl: *const c_void, domain: *const c_char) -> *const c_char;
    fn cairo_version_string() -> *const c_char;
    fn sqlite3_libversion() -> *const c_char;
    fn WebPGetDecoderVersion() -> c_int;
    fn OpenSSL_version(kind: c_int) -> *const c_char;
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

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn c(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

fn owned(p: *mut c_char) -> Option<Vec<u8>> {
    unsafe { GStr::take(p) }.map(|s| s.to_bytes().to_vec())
}

fn borrowed(p: *const c_char) -> Option<Vec<u8>> {
    unsafe { glib::bytes(p) }.map(<[u8]>::to_vec)
}

pub fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    std::fs::read(util::os_path(path)?).ok()
}

static SELF_EXE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

pub fn set_self_exe(path: Option<&[u8]>) {
    if let Ok(mut exe) = SELF_EXE.lock() {
        *exe = path.map(<[u8]>::to_vec);
    }
}

pub fn self_exe() -> Option<Vec<u8>> {
    SELF_EXE.lock().ok()?.clone()
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

pub fn markup_escape(text: &[u8]) -> Vec<u8> {
    util::markup_escape(text)
}

pub fn random_below(n: i32) -> usize {
    match u32::try_from(n) {
        Ok(n) if n > 0 => (util::random_u32() % n) as usize,
        _ => 0,
    }
}

pub fn os_info(key: &CStr) -> Option<Vec<u8>> {
    owned(unsafe { g_get_os_info(key.as_ptr()) })
}

pub fn processors() -> u32 {
    std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(u32::MAX))
}

pub fn url_host(url: &[u8]) -> Option<Vec<u8>> {
    crate::url::host_from(until_nul(url))
}

pub fn registrable_domain(host: &[u8]) -> Option<Vec<u8>> {
    let lower = c(&host.to_ascii_lowercase());
    let psl = unsafe { psl_builtin() };
    if psl.is_null() {
        return None;
    }
    borrowed(unsafe { psl_registrable_domain(psl, lower.as_ptr()) })
}

fn form_decode(text: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'%' => {
                let digit = |at: usize| {
                    text.get(at)
                        .and_then(|&b| char::from(b).to_digit(16))
                        .and_then(|d| u8::try_from(d).ok())
                };
                out.push(digit(i + 1)? << 4 | digit(i + 2)?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    Some(until_nul(&out).to_vec())
}

pub fn parse_form(form: &[u8]) -> Option<HashMap<Vec<u8>, Vec<u8>>> {
    let form = until_nul(form);
    let mut fields = HashMap::new();
    if form.is_empty() {
        return Some(fields);
    }
    let mut params: Vec<&[u8]> = form.split(|&b| b == b'&').collect();
    if form.ends_with(b"&") {
        params.pop();
    }
    for param in params {
        let eq = param.iter().position(|&b| b == b'=')?;
        let key = form_decode(&param[..eq])?;
        let value = form_decode(&param[eq + 1..])?;
        fields.insert(key, value);
    }
    Some(fields)
}

pub fn atoi_of(text: &[u8]) -> i32 {
    util::ascii_strtoll(text).unwrap_or(0) as i32
}

pub fn config_text(p: *const c_char) -> Option<Vec<u8>> {
    borrowed(p)
}

pub fn set_config_text(field: &mut *mut c_char, value: &[u8]) {
    unsafe { glib::g_free((*field).cast()) };
    *field = glib::strdup(value);
}

pub fn unlocked_config() -> Option<&'static NsConfig> {
    southstar_config::get()
}

pub fn user_agent_for_mode(compat_mode: *const c_char) -> Option<Vec<u8>> {
    let mode = borrowed(compat_mode);
    Some(
        super::url::user_agent_for_mode(mode.as_deref())
            .as_bytes()
            .to_vec(),
    )
}

pub fn history_page() -> Vec<u8> {
    southstar_history::html_page()
}

pub fn clear_browsing_data() {
    southstar_history::clear();
    southstar_http_cache::clear();
    crate::storage::clear_cookies();
    crate::storage::clear_site_storage();
}

pub struct Versions {
    pub js_engine: Option<Vec<u8>>,
    pub glib: [u32; 3],
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
            js_engine: Some(southstar_js_engine::engine_version().into_bytes()),
            glib: [glib_major_version, glib_minor_version, glib_micro_version],
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
    let modules = crate::build_info::ported_modules();
    RustInfo {
        compiler: Some(crate::build_info::compiler_version().as_bytes().to_vec()),
        minimum: Some(crate::build_info::minimum_version().as_bytes().to_vec()),
        profile: Some(crate::build_info::build_profile().as_bytes().to_vec()),
        module_count: modules.len() as u32,
        modules: Some(modules.join(", ").into_bytes()),
    }
}

pub fn env_set(name: &CStr) -> bool {
    name.to_str()
        .is_ok_and(|name| std::env::var_os(name).is_some())
}
