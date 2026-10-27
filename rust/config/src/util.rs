//! Southstar — std replacements for GLib utilities: byte paths, private folders and permissions, atomic writes, C-style integer parsing and markup escaping.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

#[cfg(unix)]
pub fn os_path(path: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(until_nul(path))))
}

#[cfg(not(unix))]
pub fn os_path(path: &[u8]) -> Option<PathBuf> {
    core::str::from_utf8(until_nul(path))
        .ok()
        .map(PathBuf::from)
}

#[cfg(unix)]
pub fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

fn is_separator(byte: u8) -> bool {
    byte == b'/' || (cfg!(windows) && byte == b'\\')
}

fn separator_for(parts: &[&[u8]]) -> u8 {
    if !cfg!(windows) {
        return b'/';
    }
    parts
        .iter()
        .flat_map(|part| until_nul(part).iter())
        .copied()
        .find(|&b| is_separator(b))
        .unwrap_or(b'\\')
}

pub fn build_filename(parts: &[&[u8]]) -> Vec<u8> {
    let separator = separator_for(parts);
    let parts: Vec<&[u8]> = parts
        .iter()
        .map(|part| until_nul(part))
        .filter(|part| !part.is_empty())
        .collect();
    let mut out: Vec<u8> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let lead = part.iter().take_while(|&&b| is_separator(b)).count();
        let mut body = &part[lead..];
        if i + 1 < parts.len() {
            let trail = body.iter().rev().take_while(|&&b| is_separator(b)).count();
            body = &body[..body.len() - trail];
        }
        if i == 0 {
            out.extend_from_slice(&part[..lead]);
        }
        if body.is_empty() {
            continue;
        }
        if out.last().is_some_and(|&b| !is_separator(b)) {
            out.push(separator);
        }
        out.extend_from_slice(body);
    }
    out
}

fn has_drive(path: &[u8]) -> bool {
    cfg!(windows) && path.len() >= 2 && path[0].is_ascii_alphabetic() && path[1] == b':'
}

pub fn path_dirname(path: &[u8]) -> Vec<u8> {
    let path = until_nul(path);
    let Some(mut base) = path.iter().rposition(|&b| is_separator(b)) else {
        return if has_drive(path) {
            vec![path[0], b':', b'.']
        } else {
            b".".to_vec()
        };
    };
    while base > 0 && is_separator(path[base]) {
        base -= 1;
    }
    if base == 1 && has_drive(path) {
        base += 1;
    }
    path[..=base].to_vec()
}

pub fn env_bytes(name: &str) -> Option<Vec<u8>> {
    std::env::var_os(name).map(std::ffi::OsString::into_encoded_bytes)
}

fn dir_builder(mode: u32, recursive: bool) -> fs::DirBuilder {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(recursive);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, mode);
    #[cfg(not(unix))]
    let _ = mode;
    builder
}

pub fn make_dirs(path: &Path, mode: u32) -> io::Result<()> {
    dir_builder(mode, true).create(path)
}

pub fn make_dir(path: &Path, mode: u32) -> io::Result<()> {
    dir_builder(mode, false).create(path)
}

pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

fn fill(mut file: File, contents: &[u8]) -> io::Result<()> {
    file.write_all(contents)?;
    file.sync_all()
}

fn create_new(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

pub fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    loop {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let temp = PathBuf::from(name);
        let file = match create_new(&temp, mode) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let written = fill(file, contents).and_then(|()| fs::rename(&temp, path));
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        return written;
    }
}

pub fn random_u32() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seed = (nanos, COUNTER.fetch_add(1, Ordering::Relaxed));
    std::hash::BuildHasher::hash_one(&std::hash::RandomState::new(), seed) as u32
}

fn signed_digits(text: &[u8]) -> Option<(bool, u64)> {
    let text = until_nul(text);
    let start = text
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
        .unwrap_or(text.len());
    let rest = &text[start..];
    let (negative, digits) = match rest.first() {
        Some(b'-') => (true, &rest[1..]),
        Some(b'+') => (false, &rest[1..]),
        _ => (false, rest),
    };
    let count = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    if count == 0 {
        return None;
    }
    let magnitude = digits[..count].iter().fold(0u64, |n, &d| {
        n.saturating_mul(10).saturating_add(u64::from(d - b'0'))
    });
    Some((negative, magnitude))
}

pub fn ascii_strtoull(text: &[u8]) -> u64 {
    match signed_digits(text) {
        Some((true, magnitude)) => magnitude.wrapping_neg(),
        Some((false, magnitude)) => magnitude,
        None => 0,
    }
}

pub fn ascii_strtoll(text: &[u8]) -> Option<i64> {
    let (negative, magnitude) = signed_digits(text)?;
    Some(if negative {
        if magnitude > i64::MAX as u64 {
            i64::MIN
        } else {
            -(magnitude as i64)
        }
    } else {
        i64::try_from(magnitude).unwrap_or(i64::MAX)
    })
}

fn markup_entity(c: char) -> Option<String> {
    let code = u32::from(c);
    Some(match c {
        '&' => "&amp;".to_owned(),
        '<' => "&lt;".to_owned(),
        '>' => "&gt;".to_owned(),
        '\'' => "&#39;".to_owned(),
        '"' => "&quot;".to_owned(),
        _ if matches!(code, 0x1..=0x8 | 0xb..=0xc | 0xe..=0x1f | 0x7f..=0x84 | 0x86..=0x9f) => {
            format!("&#x{code:x};")
        }
        _ => return None,
    })
}

pub fn markup_escape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for chunk in until_nul(text).utf8_chunks() {
        for c in chunk.valid().chars() {
            match markup_entity(c) {
                Some(entity) => out.extend_from_slice(entity.as_bytes()),
                None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }
        out.extend_from_slice(chunk.invalid());
    }
    out
}

pub fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for chunk in until_nul(text).utf8_chunks() {
        out.extend_from_slice(chunk.valid().as_bytes());
        for _ in chunk.invalid() {
            out.extend_from_slice("\u{fffd}".as_bytes());
        }
    }
    out
}
