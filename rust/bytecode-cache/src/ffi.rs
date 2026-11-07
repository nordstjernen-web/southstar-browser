//! Southstar — the GLib call behind the JavaScript bytecode cache: the user cache directory.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::path::{Path, PathBuf};

use southstar_glib as glib;

pub(crate) fn user_cache_dir() -> PathBuf {
    glib::path_from_bytes_lossy(&glib::user_cache_dir())
}

pub(crate) fn create_private_dir(path: &Path) {
    let _ = southstar_config::util::make_dirs(path, 0o700);
}
