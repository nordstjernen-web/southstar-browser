//! Southstar — the C ABI the media helpers call to set up networking and download a URL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

#[unsafe(no_mangle)]
pub extern "C" fn ns_helper_net_init() {
    southstar_http::init();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_helper_download(
    url: *const c_char,
    path: *const c_char,
    max_bytes: u64,
) -> c_int {
    if url.is_null() || path.is_null() {
        return 0;
    }
    let url = unsafe { CStr::from_ptr(url) }.to_bytes();
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return 0;
    };
    c_int::from(crate::download::to_file(url, path, max_bytes))
}
