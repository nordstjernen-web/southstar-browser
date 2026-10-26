//! Southstar — reading a printed sheet the renderer left in the exchange directory without following links it may have planted.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const MAX_SHEETS: usize = 1000;
const MAX_FILE_BYTES: u64 = 256 << 20;
const MAX_DIM: u32 = 16384;
pub const MAX_TOTAL_PIXELS: u64 = 1 << 30;

#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "m68k"
    )
))]
const O_NOFOLLOW: i32 = 0o100000;
#[cfg(all(
    target_os = "linux",
    not(any(
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "m68k"
    ))
))]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(all(target_os = "linux", any(target_arch = "mips", target_arch = "mips64")))]
const O_NONBLOCK: i32 = 0x80;
#[cfg(all(
    target_os = "linux",
    any(target_arch = "sparc", target_arch = "sparc64")
))]
const O_NONBLOCK: i32 = 0x4000;
#[cfg(all(
    target_os = "linux",
    not(any(
        target_arch = "mips",
        target_arch = "mips64",
        target_arch = "sparc",
        target_arch = "sparc64"
    ))
))]
const O_NONBLOCK: i32 = 0o4000;
#[cfg(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const O_NOFOLLOW: i32 = 0x100;
#[cfg(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const O_NONBLOCK: i32 = 0x4;
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
const O_NOFOLLOW: i32 = 0;
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))
))]
const O_NONBLOCK: i32 = 0;

#[cfg(unix)]
fn open_no_follow(path: &Path) -> Option<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .ok()
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> Option<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()
}

#[cfg(unix)]
fn same_single_file(linked: &Metadata, opened: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    linked.dev() == opened.dev() && linked.ino() == opened.ino() && opened.nlink() == 1
}

#[cfg(windows)]
fn same_single_file(linked: &Metadata, opened: &Metadata) -> bool {
    linked.len() == opened.len()
}

#[cfg(unix)]
pub fn path_from(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(windows)]
pub fn path_from(bytes: &[u8]) -> Option<PathBuf> {
    core::str::from_utf8(bytes).ok().map(PathBuf::from)
}

pub fn read(path: &Path) -> Option<Vec<u8>> {
    let linked = std::fs::symlink_metadata(path).ok()?;
    if !linked.file_type().is_file() {
        return None;
    }
    let mut file = open_no_follow(path)?;
    let opened = file.metadata().ok()?;
    if !opened.file_type().is_file()
        || !same_single_file(&linked, &opened)
        || opened.len() > MAX_FILE_BYTES
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    file.by_ref()
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_FILE_BYTES).then_some(bytes)
}

pub fn png_pixels(bytes: &[u8]) -> Option<u64> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let dim =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    let (width, height) = (dim(16), dim(20));
    ((1..=MAX_DIM).contains(&width) && (1..=MAX_DIM).contains(&height))
        .then(|| u64::from(width) * u64::from(height))
}
