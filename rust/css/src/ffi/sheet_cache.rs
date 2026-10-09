//! Southstar — the C ABI of the style sheet caches: parsed sheets kept per <style> element, per merged style text, per linked URL and per imported URL and layer, so a style pass reparses only what changed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr::{self, NonNull};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use super::sheet::{
    RawSheet, ns_css_stylesheet_force_layer, ns_css_stylesheet_free, ns_css_stylesheet_parse,
};
use crate::style_text;

const STYLE_ELEMENTS_MAX: usize = 2048;
const MERGED_TRIM_ABOVE: usize = 64;
const MERGED_TRIM_TO: usize = 48;
const URL_SHEETS_MAX: usize = 256;

unsafe extern "C" {
    fn ns_css_media_viewport_current_w() -> f64;
    fn ns_css_media_viewport_current_h() -> f64;
    fn g_bytes_ref(bytes: *mut c_void) -> *mut c_void;
    fn g_bytes_unref(bytes: *mut c_void);
    fn g_bytes_equal(a: *const c_void, b: *const c_void) -> GBoolean;
    fn g_bytes_get_data(bytes: *mut c_void, size: *mut usize) -> *const c_void;
}

struct Sheet(NonNull<RawSheet>);

unsafe impl Send for Sheet {}

impl Sheet {
    fn adopt(raw: *mut c_void) -> Option<Self> {
        let raw = NonNull::new(raw.cast::<RawSheet>())?;
        unsafe { (*raw.as_ptr()).cached = glib::TRUE };
        Some(Sheet(raw))
    }

    fn ptr(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }
}

impl Drop for Sheet {
    fn drop(&mut self) {
        unsafe {
            (*self.0.as_ptr()).cached = glib::FALSE;
            ns_css_stylesheet_free(self.0.as_ptr());
        }
    }
}

struct Bytes(NonNull<c_void>);

unsafe impl Send for Bytes {}

impl Bytes {
    fn retain(bytes: NonNull<c_void>) -> Self {
        unsafe { g_bytes_ref(bytes.as_ptr()) };
        Bytes(bytes)
    }

    fn same(&self, other: NonNull<c_void>) -> bool {
        self.0 == other || unsafe { g_bytes_equal(self.0.as_ptr(), other.as_ptr()) } != 0
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        unsafe { g_bytes_unref(self.0.as_ptr()) };
    }
}

fn viewport() -> (f64, f64) {
    unsafe {
        (
            ns_css_media_viewport_current_w(),
            ns_css_media_viewport_current_h(),
        )
    }
}

fn viewport_rounded() -> (u64, u64) {
    let (w, h) = viewport();
    (w.round_ties_even().to_bits(), h.round_ties_even().to_bits())
}

struct StyleElementSheet {
    css: Vec<u8>,
    viewport: (f64, f64),
    sheet: Sheet,
}

#[derive(PartialEq, Eq, Hash)]
struct MergedKey {
    viewport: (u64, u64),
    base: Option<Vec<u8>>,
    css: Vec<u8>,
}

struct MergedSheet {
    sheet: Sheet,
    stamp: u64,
}

#[derive(PartialEq, Eq, Hash)]
struct UrlKey {
    viewport: (u64, u64),
    layer: Option<Vec<u8>>,
    url: Vec<u8>,
}

struct ImportSheet {
    bytes: Bytes,
    sheet: Sheet,
}

#[derive(Default)]
struct Caches {
    style_elements: HashMap<usize, StyleElementSheet>,
    merged: HashMap<MergedKey, MergedSheet>,
    clock: u64,
    pass_start: u64,
    links: HashMap<UrlKey, Sheet>,
    imports: HashMap<UrlKey, ImportSheet>,
    relayout_depth: u32,
}

impl Caches {
    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn trim_merged(&mut self, keep_after: u64) {
        if self.merged.len() <= MERGED_TRIM_ABOVE {
            return;
        }
        let mut evictable: Vec<u64> = self
            .merged
            .values()
            .map(|entry| entry.stamp)
            .filter(|&stamp| stamp <= keep_after)
            .collect();
        let excess = (self.merged.len() - MERGED_TRIM_TO).min(evictable.len());
        if excess == 0 {
            return;
        }
        evictable.sort_unstable();
        let newest_evicted = evictable[excess - 1];
        self.merged.retain(|_, entry| entry.stamp > newest_evicted);
    }

    fn nested_relayout(&self) -> bool {
        self.relayout_depth > 1
    }
}

static CACHES: LazyLock<Mutex<Caches>> = LazyLock::new(Mutex::default);

fn with<R>(f: impl FnOnce(&mut Caches) -> R) -> R {
    f(&mut CACHES.lock().unwrap_or_else(PoisonError::into_inner))
}

unsafe fn parse(css: *const c_char, len: isize) -> *mut c_void {
    unsafe { ns_css_stylesheet_parse(css, len) }
}

fn parse_bytes(css: &[u8]) -> *mut c_void {
    unsafe { parse(css.as_ptr().cast(), css.len() as isize) }
}

unsafe fn parse_in_layer(bytes: NonNull<c_void>, layer: *const c_char) -> *mut c_void {
    let mut len = 0;
    let data = unsafe { g_bytes_get_data(bytes.as_ptr(), &mut len) };
    let sheet = unsafe { parse(data.cast(), len as isize) };
    if !sheet.is_null() && !layer.is_null() {
        unsafe { ns_css_stylesheet_force_layer(sheet, layer) };
    }
    sheet
}

unsafe fn owned_bytes(text: *const c_char) -> Option<Vec<u8>> {
    unsafe { glib::bytes(text) }.map(<[u8]>::to_vec)
}

unsafe fn url_key(url: *const c_char, layer: *const c_char) -> Option<UrlKey> {
    let url = unsafe { glib::bytes(url) }.filter(|url| !url.is_empty())?;
    Some(UrlKey {
        viewport: viewport_rounded(),
        layer: unsafe { owned_bytes(layer) },
        url: url.to_vec(),
    })
}

fn take_text(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_style_element_text(style: *mut NsNode) -> *mut c_char {
    take_text(unsafe { Node::from_ptr(style) }.and_then(style_text::style_element_text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_shadow_adopted_css(root: *mut NsNode) -> *mut c_char {
    take_text(unsafe { Node::from_ptr(root) }.and_then(style_text::shadow_adopted_text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_from_style_element_cached(
    style: *mut NsNode,
) -> *mut c_void {
    let Some(style) = (unsafe { Node::from_ptr(style) }) else {
        return ptr::null_mut();
    };
    let Some(css) = style_text::style_element_text(style) else {
        return ptr::null_mut();
    };
    let key = style.as_ptr() as usize;
    let viewport = viewport();
    let hit = with(|caches| {
        caches
            .style_elements
            .get(&key)
            .filter(|entry| entry.css == css && entry.viewport == viewport)
            .map(|entry| entry.sheet.ptr())
    });
    if let Some(sheet) = hit {
        return sheet;
    }
    let Some(sheet) = Sheet::adopt(parse_bytes(&css)) else {
        return ptr::null_mut();
    };
    let out = sheet.ptr();
    let entry = StyleElementSheet {
        css,
        viewport,
        sheet,
    };
    drop(with(|caches| caches.style_elements.insert(key, entry)));
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_merged_styles_cached(
    css: *const c_char,
    len: isize,
    base_url: *const c_char,
) -> *mut c_void {
    if css.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let len = usize::try_from(len).unwrap_or_else(|_| unsafe { CStr::from_ptr(css) }.count_bytes());
    let key = MergedKey {
        viewport: viewport_rounded(),
        base: unsafe { owned_bytes(base_url) },
        css: unsafe { glib::slice(css.cast(), len) }.to_vec(),
    };
    let hit = with(|caches| {
        let Caches { merged, clock, .. } = caches;
        merged.get_mut(&key).map(|entry| {
            *clock += 1;
            entry.stamp = *clock;
            entry.sheet.ptr()
        })
    });
    if let Some(sheet) = hit {
        return sheet;
    }
    let Some(sheet) = Sheet::adopt(parse_bytes(&key.css)) else {
        return ptr::null_mut();
    };
    let out = sheet.ptr();
    drop(with(|caches| {
        let stamp = caches.tick();
        caches.merged.insert(key, MergedSheet { sheet, stamp })
    }));
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_parse_url_cached(
    url: *const c_char,
    css: *const c_char,
    len: isize,
) -> *mut c_void {
    if css.is_null() {
        return ptr::null_mut();
    }
    let Some(key) = (unsafe { url_key(url, ptr::null()) }) else {
        return unsafe { parse(css, len) };
    };
    if let Some(sheet) = with(|caches| caches.links.get(&key).map(Sheet::ptr)) {
        return sheet;
    }
    let Some(sheet) = Sheet::adopt(unsafe { parse(css, len) }) else {
        return ptr::null_mut();
    };
    let out = sheet.ptr();
    drop(with(|caches| caches.links.insert(key, sheet)));
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_stylesheet_parse_import_cached(
    url: *const c_char,
    layer_name: *const c_char,
    bytes: *mut c_void,
) -> *mut c_void {
    let Some(bytes) = NonNull::new(bytes) else {
        return ptr::null_mut();
    };
    let Some(key) = (unsafe { url_key(url, layer_name) }) else {
        return unsafe { parse_in_layer(bytes, layer_name) };
    };
    let hit = with(|caches| {
        caches
            .imports
            .get(&key)
            .filter(|entry| entry.bytes.same(bytes))
            .map(|entry| entry.sheet.ptr())
    });
    if let Some(sheet) = hit {
        return sheet;
    }
    let Some(sheet) = Sheet::adopt(unsafe { parse_in_layer(bytes, layer_name) }) else {
        return ptr::null_mut();
    };
    let out = sheet.ptr();
    let entry = ImportSheet {
        bytes: Bytes::retain(bytes),
        sheet,
    };
    drop(with(|caches| caches.imports.insert(key, entry)));
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_relayout_enter() {
    with(|caches| caches.relayout_depth += 1);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_relayout_leave() {
    with(|caches| caches.relayout_depth = caches.relayout_depth.saturating_sub(1));
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_stylesheet_cache_drop() {
    let dropped = with(|caches| {
        (
            core::mem::take(&mut caches.style_elements),
            core::mem::take(&mut caches.merged),
            core::mem::take(&mut caches.links),
            core::mem::take(&mut caches.imports),
        )
    });
    drop(dropped);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_element_cache_begin() {
    with(|caches| {
        if caches.nested_relayout() {
            return;
        }
        if caches.style_elements.len() > STYLE_ELEMENTS_MAX {
            caches.style_elements.clear();
        }
        caches.trim_merged(u64::MAX);
        caches.pass_start = caches.clock;
        if caches.links.len() > URL_SHEETS_MAX {
            caches.links.clear();
        }
        if caches.imports.len() > URL_SHEETS_MAX {
            caches.imports.clear();
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_style_element_cache_end() {
    with(|caches| {
        if !caches.nested_relayout() {
            caches.trim_merged(caches.pass_start);
        }
    });
}
