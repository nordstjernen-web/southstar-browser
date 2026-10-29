//! Southstar — the GLib call behind the JavaScript bytecode cache: the user cache directory.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::DirBuilder;
use std::path::{Path, PathBuf};

use southstar_glib as glib;

pub(crate) fn user_cache_dir() -> PathBuf {
    path_from_glib(&glib::user_cache_dir())
}

#[cfg(unix)]
fn path_from_glib(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_glib(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn create_private_dir(path: &Path) {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(path);
}
