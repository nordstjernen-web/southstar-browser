//! Southstar — the file calls behind bookmarks storage: locating, reading and atomically writing the bookmarks file.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use southstar_glib as glib;

const APP_DIR_NAME: &str = "southstar";
const BOOKMARKS_FILE: &str = "bookmarks.txt";

fn create_private_dir(dir: &Path) {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(dir);
}

pub fn bookmarks_path() -> CString {
    let config = glib::path_from_bytes(&glib::user_config_dir()).unwrap_or_default();
    let dir = config.join(APP_DIR_NAME);
    create_private_dir(&dir);
    CString::new(glib::path_bytes(&dir.join(BOOKMARKS_FILE))).unwrap_or_default()
}

pub fn read(path: &CStr) -> Option<Vec<u8>> {
    fs::read(glib::path_from_bytes(path.to_bytes())?).ok()
}

fn write_synced(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}

fn replace_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp);
    let written = write_synced(&temp, contents).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

pub fn write(path: &CStr, contents: &[u8]) {
    let Some(target) = glib::path_from_bytes(path.to_bytes()) else {
        return;
    };
    if let Err(reason) = replace_file(&target, contents) {
        let mut text = b"bookmarks: failed to write ".to_vec();
        text.extend_from_slice(path.to_bytes());
        text.extend_from_slice(format!(": {reason}").as_bytes());
        glib::warning(text);
    }
}
