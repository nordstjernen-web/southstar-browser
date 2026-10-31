//! Southstar — the global runtime configuration behind its lock, and loading and saving it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, RwLock};

use southstar_glib::{FALSE, GBoolean, TRUE};

use crate::NsConfig;
use crate::util as fs;

static GLOBAL: LazyLock<RwLock<Arc<NsConfig>>> =
    LazyLock::new(|| RwLock::new(Arc::new(NsConfig::ZERO)));

static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOCK: Mutex<()> = Mutex::new(());

fn update<R>(f: impl FnOnce(&mut NsConfig) -> R) -> R {
    let mut config = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
    f(Arc::make_mut(&mut config))
}

fn replace(config: NsConfig) {
    *GLOBAL.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(config);
}

fn snapshot() -> NsConfig {
    NsConfig::clone(&config())
}

fn global_path() -> MutexGuard<'static, Option<PathBuf>> {
    PATH.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn config() -> Arc<NsConfig> {
    Arc::clone(&GLOBAL.read().unwrap_or_else(|e| e.into_inner()))
}

fn switch_on(field: impl FnOnce(&mut NsConfig) -> &mut GBoolean) {
    let switched = update(|config| {
        let flag = field(config);
        let off = *flag == FALSE;
        if off {
            *flag = TRUE;
        }
        off
    });
    if switched {
        save();
    }
}

pub(crate) fn enable_camera() {
    switch_on(|config| &mut config.camera_enabled);
}

pub(crate) fn enable_webgl() {
    switch_on(|config| &mut config.webgl_enabled);
}

pub(crate) fn ascii_strtoll(value: &[u8]) -> Option<i64> {
    fs::ascii_strtoll(value)
}

fn getenv(name: &str) -> Option<Vec<u8>> {
    fs::env_bytes(name)
}

fn warn_unreadable(path: &Path, message: &str) {
    let mut text = b"config: failed to read ".to_vec();
    text.extend_from_slice(&fs::path_bytes(path));
    text.extend_from_slice(b": ");
    text.extend_from_slice(message.as_bytes());
    southstar_glib::warning(text);
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
    southstar_glib::user_config_dir()
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
    let mut config = snapshot();
    load(&mut config, &path);
    replace(config);
    *slot = Some(path);
}

pub fn reload() {
    let Some(path) = global_path().clone() else {
        return;
    };
    let mut fresh = NsConfig::ZERO;
    load(&mut fresh, &path);
    update(|config| crate::adopt(config, &mut fresh));
}

pub fn shutdown() {
    replace(NsConfig::ZERO);
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

    pub fn get(&self) -> Arc<NsConfig> {
        config()
    }

    pub fn update<R>(&mut self, f: impl FnOnce(&mut NsConfig) -> R) -> R {
        update(f)
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
    let contents = crate::serialize(&mut snapshot());
    let written = fs::write_atomic(&path, &contents, 0o666).is_ok();
    if written {
        let _ = fs::set_mode(&path, 0o600);
    }
    written
}

pub fn dump(default_language: &[u8]) -> Vec<u8> {
    let path = global_path().as_deref().map(fs::path_bytes);
    crate::render_dump(&mut snapshot(), path.as_deref(), default_language)
}
