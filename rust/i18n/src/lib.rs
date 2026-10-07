//! Southstar — UI string translation: picks the catalogue matching the operating-system language and translates UI strings through it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

mod ffi;

struct Catalogue {
    language: CString,
    entries: BTreeMap<Vec<u8>, CString>,
}

static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();

const SEARCH_DIRS: [&CStr; 8] = [
    c"../Resources/share/southstar/i18n",
    c"../share/southstar/i18n",
    c"share/southstar/i18n",
    c"data/i18n",
    c"../data/i18n",
    c"../../data/i18n",
    c"../../../data/i18n",
    c"../../../../data/i18n",
];

fn catalogue_path_in(dir: &CStr, lang: &CStr) -> Option<CString> {
    let file = CString::new([lang.to_bytes(), b".lang"].concat()).ok()?;
    let path = ffi::build_filename(&[dir, &file]);
    ffi::is_regular_file(&path).then_some(path)
}

fn find_catalogue(self_exe: Option<&CStr>, lang: &CStr) -> Option<CString> {
    if let Some(dir) = ffi::getenv(c"NS_I18N_DIR").filter(|dir| !dir.is_empty()) {
        return catalogue_path_in(&dir, lang);
    }
    if let Some(exe) = self_exe {
        let exe_dir = ffi::path_get_dirname(exe);
        let found = SEARCH_DIRS
            .iter()
            .find_map(|rel| catalogue_path_in(&ffi::build_filename(&[&exe_dir, rel]), lang));
        if found.is_some() {
            return found;
        }
    }
    catalogue_path_in(c"data/i18n", lang)
}

fn parse_catalogue(text: &[u8]) -> BTreeMap<Vec<u8>, CString> {
    let mut entries = BTreeMap::new();
    for line in text.split(|&byte| byte == b'\n') {
        if line.first().is_none_or(|&byte| byte == b'#') {
            continue;
        }
        let Some(eq) = line.iter().position(|&byte| byte == b'=') else {
            continue;
        };
        let value = line[eq + 1..].trim_ascii_end();
        if eq == 0 || value.is_empty() {
            continue;
        }
        if let Ok(value) = CString::new(value) {
            entries.insert(line[..eq].to_vec(), value);
        }
    }
    entries
}

fn load_catalogue(path: &CStr) -> Option<BTreeMap<Vec<u8>, CString>> {
    let entries = parse_catalogue(&ffi::file_text(path)?);
    (!entries.is_empty()).then_some(entries)
}

pub fn init(self_exe: Option<&CStr>) {
    if CATALOGUE.get().is_some() {
        return;
    }
    for name in ffi::language_names() {
        let bytes = name.to_bytes();
        if bytes == b"C" || bytes == b"POSIX" || bytes.contains(&b'.') || bytes.contains(&b'@') {
            continue;
        }
        let Some(path) = find_catalogue(self_exe, &name) else {
            continue;
        };
        if let Some(entries) = load_catalogue(&path) {
            let _ = CATALOGUE.set(Catalogue {
                language: name,
                entries,
            });
            return;
        }
    }
}

pub fn translate(text: &CStr) -> Option<&'static CStr> {
    let catalogue = CATALOGUE.get()?;
    catalogue
        .entries
        .get(text.to_bytes())
        .map(CString::as_c_str)
}

pub fn language() -> Option<&'static CStr> {
    CATALOGUE
        .get()
        .map(|catalogue| catalogue.language.as_c_str())
}
