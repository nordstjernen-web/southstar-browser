//! Southstar — where the network layer keeps its files: the per-user data and cookie folders, the throwaway private-mode folder, the HSTS and Alt-Svc files, and clearing cookies and site storage.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;
use std::sync::Mutex;

use crate::ffi::sys;

const APP_DIR: &[u8] = b"southstar";
const PRIVATE_MODE_DIR: i32 = 0o700;

pub struct Slot(Mutex<Option<CString>>);

impl Slot {
    pub const fn new() -> Slot {
        Slot(Mutex::new(None))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<CString>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get_or_init(&self, init: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        let mut slot = self.lock();
        if slot.is_none() {
            *slot = init().and_then(|path| CString::new(path).ok());
        }
        slot.as_ref().map(|path| path.as_bytes().to_vec())
    }

    pub fn get(&self) -> Option<Vec<u8>> {
        self.lock().as_ref().map(|path| path.as_bytes().to_vec())
    }

    pub fn with_ptr<R>(&self, f: impl FnOnce(Option<&CString>) -> R) -> R {
        f(self.lock().as_ref())
    }

    pub fn set(&self, value: Option<Vec<u8>>) {
        *self.lock() = value.and_then(|v| CString::new(v).ok());
    }

    pub fn take(&self) -> Option<Vec<u8>> {
        self.lock().take().map(CString::into_bytes)
    }
}

pub static COOKIE_DIR: Slot = Slot::new();
pub static PRIVATE_ROOT: Slot = Slot::new();
pub static HSTS_PATH: Slot = Slot::new();

pub fn is_private() -> bool {
    southstar_config::private_mode()
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    sys::build_filename(dir, name)
}

fn app_data_dir() -> Vec<u8> {
    join(&sys::user_data_dir(), APP_DIR)
}

pub fn private_root() -> Option<Vec<u8>> {
    PRIVATE_ROOT.get_or_init(|| {
        let root = sys::make_tmp_dir(c"southstar-private-XXXXXX").unwrap_or_else(|| {
            let root = join(
                &sys::tmp_dir(),
                format!("southstar-private-{}", sys::random_u32()).as_bytes(),
            );
            sys::mkdir_with_parents(&root, PRIVATE_MODE_DIR);
            root
        });
        sys::chmod(&root, PRIVATE_MODE_DIR);
        Some(root)
    })
}

fn data_path(slot: &Slot, basename: &[u8]) -> Option<Vec<u8>> {
    slot.get_or_init(|| {
        let dir = app_data_dir();
        sys::mkdir_with_parents(&dir, PRIVATE_MODE_DIR);
        Some(join(&dir, basename))
    })
}

pub fn cookie_dir() -> Option<Vec<u8>> {
    COOKIE_DIR.get_or_init(|| {
        let dir = if is_private() {
            let root = private_root().unwrap_or_else(sys::tmp_dir);
            join(&root, b"cookies")
        } else {
            join(&join(&sys::user_config_dir(), APP_DIR), b"cookies")
        };
        sys::mkdir_with_parents(&dir, PRIVATE_MODE_DIR);
        Some(dir)
    })
}

pub fn hsts_path() -> Option<Vec<u8>> {
    if let Some(path) = HSTS_PATH.get() {
        return Some(path);
    }
    if !is_private() {
        return data_path(&HSTS_PATH, b"hsts-curl.txt");
    }
    let root = private_root()?;
    HSTS_PATH.get_or_init(|| {
        let path = join(&root, b"hsts-curl.txt");
        let real = join(&app_data_dir(), b"hsts-curl.txt");
        if let Ok(contents) = sys::read_file(&real) {
            sys::write_file(&path, &contents);
        }
        Some(path)
    })
}

pub fn cookie_jar_path(top_origin: Option<&[u8]>, js: bool) -> Option<Vec<u8>> {
    let dir = cookie_dir()?;
    let key = top_origin.filter(|o| !o.is_empty()).unwrap_or(b"default");
    let digest = sys::sha256_hex(key);
    let short = &digest[..digest.len().min(32)];
    let suffix: &[u8] = if js { b".js.txt" } else { b".txt" };
    Some(join(&dir, &[short, suffix].concat()))
}

pub fn clear_cookies() {
    if let Some(dir) = cookie_dir() {
        sys::empty_dir(&dir);
    }
}

pub fn clear_site_storage() {
    let base = app_data_dir();
    sys::remove_tree(&join(&base, b"localstorage"));
    sys::remove_tree(&join(&base, b"indexeddb"));
}

pub fn shutdown() {
    COOKIE_DIR.take();
    HSTS_PATH.take();
    if let Some(root) = PRIVATE_ROOT.take() {
        sys::remove_tree(&root);
    }
}
