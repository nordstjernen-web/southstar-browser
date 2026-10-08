//! Southstar — the C ABI of the runtime configuration, as declared in src/config.h, and the GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, FALSE, GBoolean, GError, TRUE};

use crate::NsConfig;

const LOG_LEVEL_WARNING: c_int = 1 << 4;
const FILE_ERROR_NOENT: c_int = 4;

#[repr(C)]
union GMutex {
    p: *mut c_void,
    i: [c_uint; 2],
}

struct Global {
    config: UnsafeCell<NsConfig>,
    path: UnsafeCell<*mut c_char>,
    mutex: UnsafeCell<GMutex>,
}

unsafe impl Sync for Global {}

static GLOBAL: Global = Global {
    config: UnsafeCell::new(NsConfig::ZERO),
    path: UnsafeCell::new(ptr::null_mut()),
    mutex: UnsafeCell::new(GMutex { i: [0, 0] }),
};

unsafe extern "C" {
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_path_get_dirname(file_name: *const c_char) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        error: *mut *mut GError,
    ) -> GBoolean;
    fn g_file_error_quark() -> u32;
    fn g_file_error_from_errno(err_no: c_int) -> c_int;
    fn g_strerror(errnum: c_int) -> *const c_char;
    fn g_set_error(err: *mut *mut GError, domain: u32, code: c_int, format: *const c_char, ...);
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> i64;
    fn g_strcmp0(a: *const c_char, b: *const c_char) -> c_int;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
    fn g_mutex_lock(mutex: *mut GMutex);
    fn g_mutex_unlock(mutex: *mut GMutex);
    fn ns_net_default_accept_language() -> *const c_char;
    fn ns_net_proxy_mask(proxy_url: *const c_char) -> *mut c_char;
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

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Option<Vec<u8>> {
    let bytes = unsafe { glib::bytes(p) }?.to_vec();
    unsafe { glib::g_free(p.cast()) };
    Some(bytes)
}

fn global_config() -> &'static mut NsConfig {
    unsafe { &mut *GLOBAL.config.get() }
}

fn global_path() -> &'static mut *mut c_char {
    unsafe { &mut *GLOBAL.path.get() }
}

pub(crate) fn config() -> Option<&'static NsConfig> {
    Some(unsafe { &*GLOBAL.config.get() })
}

pub(crate) fn enable_camera() {
    let config = global_config();
    if config.camera_enabled == FALSE {
        config.camera_enabled = TRUE;
        unsafe { ns_config_save(ptr::null_mut()) };
    }
}

pub(crate) fn text(p: *const c_char) -> Option<Vec<u8>> {
    unsafe { glib::bytes(p) }.map(<[u8]>::to_vec)
}

pub(crate) fn set_text(slot: &mut *mut c_char, value: &[u8]) {
    unsafe { glib::g_free((*slot).cast()) };
    *slot = glib::strdup(value);
}

pub(crate) fn adopt_text(to: &mut *mut c_char, from: &mut *mut c_char) {
    unsafe {
        if g_strcmp0(*to, *from) != 0 {
            glib::g_free((*to).cast());
            *to = *from;
        } else {
            glib::g_free((*from).cast());
        }
    }
    *from = ptr::null_mut();
}

pub(crate) fn ascii_strtoll(value: &[u8]) -> Option<i64> {
    let value = c_string(value);
    let mut end: *mut c_char = ptr::null_mut();
    let n = unsafe { g_ascii_strtoll(value.as_ptr(), &mut end, 10) };
    (end.cast_const() != value.as_ptr()).then_some(n)
}

pub(crate) fn null_text() -> Vec<u8> {
    unsafe { take(g_strdup_printf(c"%s".as_ptr(), ptr::null::<c_char>())) }.unwrap_or_default()
}

pub(crate) fn default_accept_language() -> Vec<u8> {
    text(unsafe { ns_net_default_accept_language() }).unwrap_or_else(null_text)
}

pub(crate) fn proxy_mask(proxy: *const c_char) -> Option<Vec<u8>> {
    unsafe { take(ns_net_proxy_mask(proxy)) }
}

fn getenv(name: &str) -> Option<Vec<u8>> {
    let name = c_string(name.as_bytes());
    text(unsafe { glib::g_getenv(name.as_ptr()) })
}

fn read_config_file(path: *const c_char) -> Option<Vec<u8>> {
    unsafe {
        let mut contents: *mut c_char = ptr::null_mut();
        let mut len = 0usize;
        let mut err: *mut GError = ptr::null_mut();
        if glib::g_file_get_contents(path, &mut contents, &mut len, &mut err) == FALSE {
            if !((*err).domain == g_file_error_quark() && (*err).code == FILE_ERROR_NOENT) {
                g_log(
                    ptr::null(),
                    LOG_LEVEL_WARNING,
                    c"config: failed to read %s: %s".as_ptr(),
                    path,
                    (*err).message,
                );
            }
            glib::g_error_free(err);
            return None;
        }
        let bytes = glib::slice(contents.cast_const().cast(), len).to_vec();
        glib::g_free(contents.cast());
        Some(bytes)
    }
}

fn load(config: &mut NsConfig, path: *const c_char) {
    let contents = read_config_file(path);
    crate::load(config, contents.as_deref(), getenv);
}

fn free_texts(config: &mut NsConfig) {
    for p in [
        &mut config.home_url,
        &mut config.user_agent,
        &mut config.compat_mode,
        &mut config.accept_language,
        &mut config.search_engine,
        &mut config.ai_model_mirror,
        &mut config.http_proxy,
        &mut config.https_proxy,
        &mut config.no_proxy,
        &mut config.doh_url,
        &mut config.gsk_renderer,
    ] {
        unsafe { glib::g_free((*p).cast()) };
        *p = ptr::null_mut();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_init() {
    let path = global_path();
    if !path.is_null() {
        return;
    }
    *path = unsafe {
        g_build_filename(
            glib::g_get_user_config_dir(),
            c"southstar".as_ptr(),
            c"southstar.conf".as_ptr(),
            ptr::null::<c_char>(),
        )
    };
    load(global_config(), *path);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_reload() {
    let path = *global_path();
    if path.is_null() {
        return;
    }
    let mut fresh = NsConfig::ZERO;
    load(&mut fresh, path);
    crate::adopt(global_config(), &mut fresh);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_shutdown() {
    let config = global_config();
    free_texts(config);
    *config = NsConfig::ZERO;
    let path = global_path();
    unsafe { glib::g_free((*path).cast()) };
    *path = ptr::null_mut();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_get() -> *const NsConfig {
    GLOBAL.config.get()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_mut() -> *mut NsConfig {
    GLOBAL.config.get()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_lock() {
    unsafe { g_mutex_lock(GLOBAL.mutex.get()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_unlock() {
    unsafe { g_mutex_unlock(GLOBAL.mutex.get()) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_config_save(error: *mut *mut GError) -> GBoolean {
    let path = *global_path();
    unsafe {
        if path.is_null() {
            g_set_error(
                error,
                g_file_error_quark(),
                FILE_ERROR_NOENT,
                c"config path not initialized".as_ptr(),
            );
            return FALSE;
        }
        let dir = g_path_get_dirname(path);
        if g_mkdir_with_parents(dir, 0o700) != 0 {
            let err_no = errno();
            g_set_error(
                error,
                g_file_error_quark(),
                g_file_error_from_errno(err_no),
                c"could not create config directory %s: %s".as_ptr(),
                dir,
                g_strerror(err_no),
            );
            glib::g_free(dir.cast());
            return FALSE;
        }
        glib::g_free(dir.cast());
        let contents = crate::serialize(global_config());
        let ok = g_file_set_contents(
            path,
            contents.as_ptr().cast(),
            contents.len() as isize,
            error,
        );
        if ok != FALSE {
            g_chmod(path, 0o600);
        }
        ok
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_config_dump() -> *mut c_char {
    let path = text(*global_path());
    glib::strdup(&crate::dump(global_config(), path.as_deref()))
}
