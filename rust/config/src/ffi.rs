//! Southstar — the global runtime configuration behind its lock, and loading and saving it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int};
use core::ptr;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use southstar_glib::{self as glib, FALSE, TRUE};

use crate::NsConfig;
use crate::util as fs;

const LOG_LEVEL_WARNING: c_int = 1 << 4;

struct Global {
    config: UnsafeCell<NsConfig>,
}

unsafe impl Sync for Global {}

static GLOBAL: Global = Global {
    config: UnsafeCell::new(NsConfig::ZERO),
};

static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOCK: Mutex<()> = Mutex::new(());

unsafe extern "C" {
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn global_config() -> &'static mut NsConfig {
    unsafe { &mut *GLOBAL.config.get() }
}

fn global_path() -> MutexGuard<'static, Option<PathBuf>> {
    PATH.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn config() -> Option<&'static NsConfig> {
    Some(unsafe { &*GLOBAL.config.get() })
}

pub(crate) fn enable_camera() {
    let config = global_config();
    if config.camera_enabled == FALSE {
        config.camera_enabled = TRUE;
        save();
    }
}

pub(crate) fn enable_webgl() {
    let config = global_config();
    if config.webgl_enabled == FALSE {
        config.webgl_enabled = TRUE;
        save();
    }
}

pub(crate) fn ascii_strtoll(value: &[u8]) -> Option<i64> {
    fs::ascii_strtoll(value)
}

fn getenv(name: &str) -> Option<Vec<u8>> {
    fs::env_bytes(name)
}

fn warn_unreadable(path: &Path, message: &str) {
    let path = c_string(&fs::path_bytes(path));
    let message = c_string(message.as_bytes());
    // SAFETY: the format takes two NUL-terminated strings, and path and message are CStrings.
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"config: failed to read %s: %s".as_ptr(),
            path.as_ptr(),
            message.as_ptr(),
        )
    };
}

fn read_config_file(path: &Path) -> Option<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn_unreadable(path, &e.to_string());
            }
            None
        }
    }
}

fn load(config: &mut NsConfig, path: &Path) {
    let contents = read_config_file(path);
    crate::load(config, contents.as_deref(), getenv);
}

fn user_config_dir() -> Vec<u8> {
    // SAFETY: g_get_user_config_dir has no preconditions and returns a string GLib owns.
    let dir = unsafe { glib::g_get_user_config_dir() };
    // SAFETY: the directory is null or a NUL-terminated string GLib keeps for the process lifetime.
    unsafe { glib::bytes(dir) }
        .map(<[u8]>::to_vec)
        .unwrap_or_default()
}

pub fn init() {
    let mut slot = global_path();
    if slot.is_some() {
        return;
    }
    let Some(dir) = fs::os_path(&user_config_dir()) else {
        return;
    };
    let path = dir.join("southstar").join("southstar.conf");
    load(global_config(), &path);
    *slot = Some(path);
}

pub fn reload() {
    let Some(path) = global_path().clone() else {
        return;
    };
    let mut fresh = NsConfig::ZERO;
    load(&mut fresh, &path);
    crate::adopt(global_config(), &mut fresh);
}

pub fn shutdown() {
    *global_config() = NsConfig::ZERO;
    *global_path() = None;
}

pub struct ConfigLock {
    _guard: MutexGuard<'static, ()>,
}

pub fn lock() -> ConfigLock {
    ConfigLock {
        _guard: LOCK.lock().unwrap_or_else(|e| e.into_inner()),
    }
}

impl ConfigLock {
    pub fn reload(&self) {
        reload();
    }

    pub fn get(&self) -> &NsConfig {
        global_config()
    }

    pub fn get_mut(&mut self) -> &mut NsConfig {
        global_config()
    }

    pub fn save(&self) -> bool {
        save()
    }
}

pub fn save() -> bool {
    let Some(path) = global_path().clone() else {
        return false;
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    if fs::make_dirs(dir, 0o700).is_err() {
        return false;
    }
    let contents = crate::serialize(global_config());
    let written = fs::write_atomic(&path, &contents, 0o666).is_ok();
    if written {
        let _ = fs::set_mode(&path, 0o600);
    }
    written
}

pub fn dump(default_language: &[u8]) -> Vec<u8> {
    let path = global_path().as_deref().map(fs::path_bytes);
    crate::render_dump(global_config(), path.as_deref(), default_language)
}
