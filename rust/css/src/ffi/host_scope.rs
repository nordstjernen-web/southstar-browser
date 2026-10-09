//! Southstar — the C ABI of host scoping: a shadow tree's or framed document's style text flattened and scoped to its host, remembered per host and text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::collections::HashMap;
use std::sync::Mutex;

use southstar_glib::{self as glib, GBoolean};

use crate::{host_scope, nesting};

const CACHE_MAX: usize = 4096;

static CACHE: Mutex<Option<HashMap<Vec<u8>, Vec<u8>>>> = Mutex::new(None);

fn until_nul(text: &[u8]) -> &[u8] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

pub(crate) fn scoped(css: &[u8], host_id: &[u8], frame_scope: bool) -> Vec<u8> {
    let mut key = Vec::with_capacity(css.len() + host_id.len() + 2);
    key.push(if frame_scope { b'f' } else { b's' });
    key.extend_from_slice(host_id);
    key.push(b'\n');
    key.extend_from_slice(css);
    key.truncate(until_nul(&key).len());
    let mut cache = CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(&key) {
        return hit.clone();
    }
    let flat = nesting::flatten(css);
    let out = host_scope::scope_sheet(until_nul(&flat), host_id, frame_scope);
    if cache.len() >= CACHE_MAX {
        cache.clear();
    }
    cache.insert(key, out.clone());
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_scoped_css(
    css: *const c_char,
    len: usize,
    host_id: *const c_char,
    frame_scope: GBoolean,
) -> *mut c_char {
    if css.is_null() || host_id.is_null() {
        return glib::strdup(b"");
    }
    let css = unsafe { core::slice::from_raw_parts(css.cast::<u8>(), len) };
    let host_id = unsafe { CStr::from_ptr(host_id) }.to_bytes();
    glib::strdup(&scoped(css, host_id, frame_scope != 0))
}
