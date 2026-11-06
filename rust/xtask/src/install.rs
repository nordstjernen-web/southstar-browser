//! Southstar xtask — installing the built executables, libsouthstar, its header and the data files under a prefix.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const EXECUTABLES: [&str; 5] = [
    "southstar",
    "southstar-renderer",
    "southstar-audio",
    "southstar-video",
    "southstar-launcher",
];

fn copy(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(from, to).map(|_| ())
}

fn files_with_extension(dir: &Path, extension: &str) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    files.sort();
    Ok(files)
}

fn file_name(path: &Path) -> &std::ffi::OsStr {
    path.file_name().expect("a listed file has a name")
}

pub fn install(source: &Path, built: &Path, root: &Path, libdir: &Path) -> io::Result<()> {
    let windows = cfg!(windows);
    let exe = std::env::consts::EXE_SUFFIX;
    let bin = root.join("bin");
    let share = root.join("share");
    let data = share.join("southstar");
    for name in EXECUTABLES {
        let path = built.join(format!("{name}{exe}"));
        if path.is_file() {
            copy(&path, &bin.join(format!("{name}{exe}")))?;
        }
    }
    let (library, library_dir) = if windows {
        ("southstar.dll", bin.clone())
    } else if cfg!(target_os = "macos") {
        ("libsouthstar.dylib", root.join(libdir))
    } else {
        ("libsouthstar.so", root.join(libdir))
    };
    copy(&built.join(library), &library_dir.join(library))?;
    if windows && built.join("libsouthstar.dll.a").is_file() {
        copy(
            &built.join("libsouthstar.dll.a"),
            &root.join(libdir).join("libsouthstar.dll.a"),
        )?;
    }
    copy(
        &source.join("rust/capi/include/libsouthstar.h"),
        &root.join("include/southstar/libsouthstar.h"),
    )?;
    let icons = source.join("data/icons/hicolor/scalable/apps");
    let icon_dir = share.join("icons/hicolor/scalable/apps");
    for icon in files_with_extension(&icons, "svg")? {
        copy(&icon, &icon_dir.join(file_name(&icon)))?;
    }
    copy(
        &source.join("rust/net/data/southstar.gif"),
        &icon_dir.join("southstar.gif"),
    )?;
    for name in ["License.md", "COPYING", "THIRD-PARTY-LICENSES.md"] {
        copy(&source.join(name), &data.join(name))?;
    }
    copy(
        &source.join("data/safebrowsing.list"),
        &data.join("safebrowsing.list"),
    )?;
    for catalogue in files_with_extension(&source.join("data/i18n"), "lang")? {
        copy(&catalogue, &data.join("i18n").join(file_name(&catalogue)))?;
    }
    copy(
        &source.join("data/southstar.desktop"),
        &share.join("applications/org.southstar.WebBrowser.desktop"),
    )?;
    copy(
        &source.join("data/org.southstar.WebBrowser.metainfo.xml"),
        &share.join("metainfo/org.southstar.WebBrowser.metainfo.xml"),
    )?;
    #[cfg(unix)]
    {
        let link = bin.join("internet-browser-southstar");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink("southstar", &link)?;
    }
    Ok(())
}
