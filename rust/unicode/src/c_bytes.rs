//! Southstar unicode — the text before the first NUL in byte strings and strings that came from C buffers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

pub fn until_nul<T: AsRef<[u8]> + ?Sized>(bytes: &T) -> &[u8] {
    let bytes = bytes.as_ref();
    bytes.split(|&b| b == 0).next().unwrap_or(bytes)
}

pub fn until_nul_owned(mut bytes: Vec<u8>) -> Vec<u8> {
    let end = until_nul(&bytes).len();
    bytes.truncate(end);
    bytes
}

pub fn until_nul_str(text: &str) -> &str {
    text.split('\0').next().unwrap_or(text)
}

pub fn c_string_until_nul<T: AsRef<[u8]> + ?Sized>(bytes: &T) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}
