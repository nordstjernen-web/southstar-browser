//! Southstar — the C ABI of the rule index: css.c's ns_css_rule_index built as the GLib tables of candidate arrays its gather loop reads, with the pseudo-element masks recorded on the sheet and its rules on the way.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint, c_void};
use core::mem::size_of;
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable};

use super::declarations::RawRule;
use super::selector_view::SelectorRef;
use super::sheet::RawSheet;
use crate::rule_index::{self, Key, Table};

#[repr(C)]
pub(super) struct RawIndex {
    pub(super) by_id: *mut GHashTable,
    pub(super) by_class: *mut GHashTable,
    pub(super) by_tag: *mut GHashTable,
    pub(super) by_attr: *mut GHashTable,
    pub(super) universal: *mut GArray,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) rule_idx: c_uint,
    pub(super) selector_idx: c_uint,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawIndex>() == 40 && size_of::<Candidate>() == 8);

unsafe extern "C" {
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
}

unsafe extern "C" fn bucket_free(data: *mut c_void) {
    unsafe { g_array_free(data.cast(), glib::TRUE) };
}

fn new_bucket() -> *mut GArray {
    unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, size_of::<Candidate>() as c_uint) }
}

fn new_table() -> *mut GHashTable {
    unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            Some(bucket_free),
        )
    }
}

unsafe fn push(bucket: *mut GArray, cand: Candidate) {
    unsafe {
        let len = (*bucket).len as usize;
        if len > 0 && *(*bucket).data.cast::<Candidate>().add(len - 1) == cand {
            return;
        }
        glib::g_array_append_vals(bucket, ptr::from_ref(&cand).cast(), 1);
    }
}

unsafe fn bucket_for(table: *mut GHashTable, key: &[u8]) -> *mut GArray {
    let key = CString::new(key).unwrap_or_default();
    unsafe {
        let found = glib::g_hash_table_lookup(table, key.as_ptr().cast()).cast::<GArray>();
        if !found.is_null() {
            return found;
        }
        let bucket = new_bucket();
        glib::g_hash_table_insert(table, glib::strdup(key.to_bytes()).cast(), bucket.cast());
        bucket
    }
}

unsafe fn rule_slice<'a>(sheet: &RawSheet) -> &'a [*mut RawRule] {
    match unsafe { sheet.rules.as_ref() } {
        Some(a) if !a.pdata.is_null() && a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.pdata.cast::<*mut RawRule>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe fn selector_slice<'a>(rule: &RawRule) -> &'a [*mut c_void] {
    match unsafe { rule.selectors.as_ref() } {
        Some(a) if !a.pdata.is_null() && a.len > 0 => unsafe {
            core::slice::from_raw_parts(a.pdata.cast::<*mut c_void>(), a.len as usize)
        },
        _ => &[],
    }
}

pub(super) unsafe fn index_free(idx: *mut RawIndex) {
    let Some(index) = (unsafe { idx.as_ref() }) else {
        return;
    };
    unsafe {
        for table in [index.by_id, index.by_class, index.by_tag, index.by_attr] {
            if !table.is_null() {
                glib::g_hash_table_destroy(table);
            }
        }
        if !index.universal.is_null() {
            g_array_free(index.universal, glib::TRUE);
        }
        glib::g_free(idx.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_rule_index_build(sheet: *mut c_void) -> *mut c_void {
    let Some(sheet) = (unsafe { sheet.cast::<RawSheet>().as_mut() }) else {
        return ptr::null_mut();
    };
    let mut rules = Vec::new();
    for &rule in unsafe { rule_slice(sheet) } {
        let Some(rule) = (unsafe { rule.as_mut() }) else {
            rules.push(Vec::new());
            continue;
        };
        let mut selectors = Vec::new();
        for &sel in unsafe { selector_slice(rule) } {
            let view = unsafe { SelectorRef::from_ptr(sel) };
            if let Some(pe) = view.map(SelectorRef::pseudo_element).filter(|&pe| pe != 0) {
                sheet.pseudo_mask |= 1 << pe;
                rule.pe_mask |= 1 << pe;
            }
            selectors.push(view);
        }
        rules.push(selectors);
    }
    let index = RawIndex {
        by_id: new_table(),
        by_class: new_table(),
        by_tag: new_table(),
        by_attr: new_table(),
        universal: new_bucket(),
    };
    for (key, rule_idx, selector_idx) in rule_index::plan(&rules) {
        let bucket = match &key {
            Key::Universal => index.universal,
            Key::In(table, name) => {
                let table = match table {
                    Table::Id => index.by_id,
                    Table::Class => index.by_class,
                    Table::Tag => index.by_tag,
                    Table::Attr => index.by_attr,
                };
                unsafe { bucket_for(table, name) }
            }
        };
        unsafe {
            push(
                bucket,
                Candidate {
                    rule_idx,
                    selector_idx,
                },
            )
        };
    }
    let raw = unsafe { glib::g_malloc0(size_of::<RawIndex>()) }.cast::<RawIndex>();
    unsafe { raw.write(index) };
    raw.cast()
}
