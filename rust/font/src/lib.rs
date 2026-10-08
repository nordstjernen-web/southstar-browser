//! Southstar — the @font-face web font loader: one fetch per URL, WOFF and WOFF2 to SFNT, the on-disk font cache and the loaded families.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
#[cfg(feature = "freetype")]
mod sfnt;

use core::ffi::{CStr, c_int};
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ffi::{Cancellable, IdleWaiter};

#[derive(Clone, Copy)]
pub struct Descriptors {
    pub weight: c_int,
    pub slant: c_int,
}

struct Entry {
    family: Vec<u8>,
    url: Vec<u8>,
    descriptors: Descriptors,
    loaded: bool,
    inflight: bool,
    cancel: Option<Cancellable>,
}

struct Pending {
    keys: Vec<Vec<u8>>,
    cancel: Cancellable,
}

struct Loader {
    entries: HashMap<Vec<u8>, Entry>,
    pending: HashMap<Vec<u8>, Pending>,
    cache_dir: CString,
}

struct State {
    loader: Option<Loader>,
    generation: u32,
    idle_waiters: Option<Vec<IdleWaiter>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    loader: None,
    generation: 0,
    idle_waiters: None,
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

const FONT_SIGNATURES: [&[u8; 4]; 7] = [
    b"\x00\x01\x00\x00",
    b"OTTO",
    b"true",
    b"typ1",
    b"ttcf",
    b"wOFF",
    b"wOF2",
];
const EXTENSION_BUFFER: usize = 12;

pub fn available() -> bool {
    cfg!(feature = "fontconfig")
}

pub fn generation() -> u32 {
    state().generation
}

pub fn pending_count() -> u32 {
    state()
        .loader
        .as_ref()
        .map_or(0, |loader| loader.pending.len() as u32)
}

pub fn family_loaded(family: &[u8]) -> bool {
    state().loader.as_ref().is_some_and(|loader| {
        loader
            .entries
            .values()
            .any(|e| e.loaded && e.family == family)
    })
}

pub(crate) fn add_idle_waiter(waiter: IdleWaiter) {
    state()
        .idle_waiters
        .get_or_insert_with(Vec::new)
        .push(waiter);
}

pub(crate) fn remove_idle_waiter(waiter: &IdleWaiter) {
    if let Some(waiters) = state().idle_waiters.as_mut() {
        waiters.retain(|w| w != waiter);
    }
}

pub(crate) fn notify_idle() {
    let waiters = {
        let mut state = state();
        if state.idle_waiters.as_ref().is_none_or(Vec::is_empty) {
            return;
        }
        if state
            .loader
            .as_ref()
            .is_some_and(|loader| !loader.pending.is_empty())
        {
            return;
        }
        state.idle_waiters.take().unwrap_or_default()
    };
    for waiter in &waiters {
        waiter.call();
    }
}

pub fn init() {
    {
        let mut state = state();
        if state.loader.is_some() {
            return;
        }
        state.loader = Some(Loader {
            entries: HashMap::new(),
            pending: HashMap::new(),
            cache_dir: ffi::cache_dir(),
        });
    }
    ffi::register_font_oracle();
}

pub fn shutdown() {
    let Some(loader) = state().loader.take() else {
        return;
    };
    for entry in loader.entries.into_values() {
        if let Some(cancel) = entry.cancel {
            cancel.cancel();
        }
    }
    drop(loader.pending);
}

fn entry_key(family: &[u8], url: &[u8], descriptors: Descriptors) -> Vec<u8> {
    let mut key = family.to_vec();
    key.push(0x1f);
    key.extend_from_slice(url);
    key.extend_from_slice(
        format!("\x1f{}\x1f{}", descriptors.weight, descriptors.slant).as_bytes(),
    );
    key
}

pub(crate) fn request(
    family: Option<&CStr>,
    src_url: Option<&CStr>,
    base_url: Option<&CStr>,
    descriptors: Descriptors,
) {
    if !available() {
        return;
    }
    if state().loader.is_none() {
        init();
    }
    let Some(family) = family.filter(|f| !f.is_empty()) else {
        return;
    };
    let Some(src_url) = src_url.filter(|s| !s.is_empty()) else {
        return;
    };
    let Some(url) = ffi::resolve_url(base_url, src_url) else {
        return;
    };
    let key = entry_key(family.to_bytes(), url.to_bytes(), descriptors);
    let cancel = {
        let mut state = state();
        let Some(loader) = state.loader.as_mut() else {
            return;
        };
        let entry = loader.entries.entry(key.clone()).or_insert_with(|| Entry {
            family: family.to_bytes().to_vec(),
            url: url.to_bytes().to_vec(),
            descriptors,
            loaded: false,
            inflight: false,
            cancel: None,
        });
        if entry.loaded || entry.inflight {
            return;
        }
        entry.inflight = true;
        if let Some(pending) = loader.pending.get_mut(&entry.url) {
            if !pending.keys.contains(&key) {
                pending.keys.push(key);
            }
            entry.cancel = Some(pending.cancel.clone());
            return;
        }
        let pending = Pending {
            keys: vec![key],
            cancel: Cancellable::new(),
        };
        entry.cancel = Some(pending.cancel.clone());
        let cancel = pending.cancel.clone();
        loader.pending.insert(entry.url.clone(), pending);
        cancel
    };
    ffi::fetch(&url, base_url, &cancel);
}

fn looks_like_font(data: &[u8]) -> bool {
    data.len() >= 4 && FONT_SIGNATURES.iter().any(|sig| data.starts_with(*sig))
}

fn extension_for(url: &[u8]) -> Vec<u8> {
    let fallback = b".bin".to_vec();
    let mut end = url.iter().position(|&b| b == b'?').unwrap_or(url.len());
    if let Some(fragment) = url[..end].iter().position(|&b| b == b'#') {
        end = fragment;
    }
    let Some(dot) = url[..end].iter().rposition(|&b| b == b'.' || b == b'/') else {
        return fallback;
    };
    let extension = &url[dot + 1..end];
    if url[dot] != b'.' || extension.len() + 2 > EXTENSION_BUFFER {
        return fallback;
    }
    let mut out = vec![b'.'];
    out.extend(extension.iter().map(u8::to_ascii_lowercase));
    out
}

fn cache_file_name(family: &[u8], url: &[u8], forced_extension: Option<&[u8]>) -> Vec<u8> {
    let mut name: Vec<u8> = family
        .iter()
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                b
            } else {
                b'_'
            }
        })
        .collect();
    name.push(b'-');
    name.extend_from_slice(&ffi::sha256_hex(url));
    match forced_extension {
        Some(extension) => name.extend_from_slice(extension),
        None => name.extend(extension_for(url)),
    }
    name
}

fn sfnt_extension(cff: bool) -> &'static [u8] {
    if cff { b".otf" } else { b".ttf" }
}

fn convert(data: &[u8]) -> Option<(Vec<u8>, &'static [u8])> {
    #[cfg(feature = "freetype")]
    if data.starts_with(b"wOFF") || data.starts_with(b"wOF2") {
        if let Some((bytes, cff)) = ffi::Face::open(data).and_then(|face| sfnt::from_face(&face)) {
            return Some((bytes, sfnt_extension(cff)));
        }
    }
    #[cfg(feature = "woff2")]
    if let Some(sfnt) = southstar_woff2::to_sfnt(data) {
        return Some((sfnt.bytes, sfnt_extension(sfnt.cff)));
    }
    let _ = (data, sfnt_extension);
    None
}

fn entry_target(key: &[u8]) -> Option<(Vec<u8>, Vec<u8>, Descriptors, CString)> {
    let state = state();
    let loader = state.loader.as_ref()?;
    let entry = loader.entries.get(key)?;
    Some((
        entry.family.clone(),
        entry.url.clone(),
        entry.descriptors,
        loader.cache_dir.clone(),
    ))
}

fn mark_loaded(key: &[u8]) {
    let mut state = state();
    if let Some(entry) = state
        .loader
        .as_mut()
        .and_then(|loader| loader.entries.get_mut(key))
    {
        entry.loaded = true;
    }
    state.generation = state.generation.wrapping_add(1);
}

fn install(keys: &[Vec<u8>], data: &[u8], forced_extension: Option<&[u8]>) {
    for key in keys {
        let Some((family, url, descriptors, cache_dir)) = entry_target(key) else {
            continue;
        };
        let name = cache_file_name(&family, &url, forced_extension);
        let Some(path) = ffi::build_path(&cache_dir, &name) else {
            continue;
        };
        if ffi::write_file(&path, data) {
            ffi::install_font_file(&path, &family, descriptors);
            mark_loaded(key);
        }
    }
}

pub(crate) fn fetched(url: &CStr, body: Option<&[u8]>) {
    let keys = {
        let mut state = state();
        state.loader.as_mut().and_then(|loader| {
            let keys = loader.pending.get(url.to_bytes())?.keys.clone();
            for key in &keys {
                if let Some(entry) = loader.entries.get_mut(key) {
                    entry.cancel = None;
                    entry.inflight = false;
                }
            }
            Some(keys)
        })
    };
    if let Some(body) = body.filter(|body| looks_like_font(body)) {
        let converted = convert(body);
        if let Some(keys) = &keys {
            match &converted {
                Some((bytes, extension)) => install(keys, bytes, Some(extension)),
                None => install(keys, body, None),
            }
        }
    }
    let finished = state()
        .loader
        .as_mut()
        .and_then(|loader| loader.pending.remove(url.to_bytes()));
    drop(finished);
}
