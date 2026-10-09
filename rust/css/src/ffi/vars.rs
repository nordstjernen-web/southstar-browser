//! Southstar — the C ABI of var() substitution: css.c's ns_var_map chain read through a mirror, the registered @property rules consulted for initial values, and the variable names an element's map exposes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::collections::HashSet;

use southstar_glib::{self as glib, GHashTable, GPtrArray};

use super::sheet::RawPropertyRule;
use crate::vars::{self, Lookup};

#[repr(C)]
pub(super) struct RawVarMap {
    ref_count: c_int,
    pub(super) own: *mut GHashTable,
    parent: *mut RawVarMap,
    names: *mut GPtrArray,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawVarMap>() == 32);

pub(super) unsafe fn new_map(own: *mut GHashTable, parent: *mut RawVarMap) -> *mut RawVarMap {
    let map = unsafe { glib::g_malloc0(size_of::<RawVarMap>()) }.cast::<RawVarMap>();
    unsafe {
        (*map).ref_count = 1;
        (*map).own = own;
        (*map).parent = parent;
    }
    map
}

pub(super) unsafe fn map_ref(map: *mut RawVarMap) -> *mut RawVarMap {
    if let Some(m) = unsafe { map.as_mut() } {
        m.ref_count += 1;
    }
    map
}

unsafe extern "C" {
    fn g_ptr_array_ref(array: *mut GPtrArray) -> *mut GPtrArray;
    fn g_ptr_array_sort(
        array: *mut GPtrArray,
        compare: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> c_int>,
    );
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn c_string(text: &[u8]) -> std::ffi::CString {
    std::ffi::CString::new(text).unwrap_or_default()
}

struct Chain<'a> {
    map: Option<&'a RawVarMap>,
    registered: *mut GHashTable,
}

pub(super) fn chain_lookup<'a>(mut map: Option<&'a RawVarMap>, name: &[u8]) -> Option<&'a [u8]> {
    let key = c_string(name);
    while let Some(m) = map {
        if !m.own.is_null() {
            let value = unsafe { glib::g_hash_table_lookup(m.own, key.as_ptr().cast()) };
            if !value.is_null() {
                return unsafe { text(value.cast()) };
            }
        }
        map = unsafe { m.parent.as_ref() };
    }
    None
}

impl Lookup for Chain<'_> {
    fn value(&self, name: &[u8]) -> Option<&[u8]> {
        chain_lookup(self.map, name)
    }

    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>> {
        if self.registered.is_null() {
            return None;
        }
        let key = c_string(name);
        let rule = unsafe { glib::g_hash_table_lookup(self.registered, key.as_ptr().cast()) }
            .cast::<RawPropertyRule>();
        let rule = unsafe { rule.as_ref() }?;
        (rule.has_initial != 0).then(|| unsafe { text(rule.initial_value) })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_substitute_vars(
    value: *const c_char,
    map: *const c_void,
    registered: *mut GHashTable,
    depth: c_int,
) -> *mut c_char {
    let Some(value) = (unsafe { text(value) }) else {
        return ptr::null_mut();
    };
    let chain = Chain {
        map: unsafe { map.cast::<RawVarMap>().as_ref() },
        registered,
    };
    let lookup: Option<&dyn Lookup> = Some(&chain);
    vars::substitute(value, lookup.filter(|_| !map.is_null()), depth)
        .map_or(ptr::null_mut(), |out| glib::strdup(&out))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_var_map_lookup(
    map: *const c_void,
    name: *const c_char,
) -> *const c_char {
    let Some(name) = (unsafe { text(name) }) else {
        return ptr::null();
    };
    chain_lookup(unsafe { map.cast::<RawVarMap>().as_ref() }, name)
        .map_or(ptr::null(), |v| v.as_ptr().cast())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_custom_value_wide_kind(text_value: *const c_char) -> c_int {
    unsafe { text(text_value) }.map_or(0, |t| vars::wide_kind(t) as c_int)
}

unsafe extern "C" fn name_cmp(a: *const c_void, b: *const c_void) -> c_int {
    let (a, b) = unsafe { (*a.cast::<*const c_char>(), *b.cast::<*const c_char>()) };
    unsafe { CStr::from_ptr(a).cmp(CStr::from_ptr(b)) as c_int }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_var_map_names(map: *const c_void) -> *mut GPtrArray {
    let raw = map.cast::<RawVarMap>().cast_mut();
    if let Some(m) = unsafe { raw.as_ref() } {
        if !m.names.is_null() {
            return unsafe { g_ptr_array_ref(m.names) };
        }
    }
    let names = unsafe { glib::g_ptr_array_new_with_free_func(Some(glib::g_free)) };
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut current = unsafe { raw.as_ref() };
    while let Some(m) = current {
        if !m.own.is_null() {
            let mut iter = glib::GHashTableIter::new();
            unsafe { glib::g_hash_table_iter_init(&mut iter, m.own) };
            let (mut key, mut value) = (ptr::null_mut(), ptr::null_mut());
            while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
                let name = unsafe { text(key.cast()) }.unwrap_or_default();
                if !seen.insert(name.to_vec()) {
                    continue;
                }
                let value = unsafe { text(value.cast()) };
                if value.is_some_and(|v| !v.eq_ignore_ascii_case(b"initial")) {
                    unsafe { glib::g_ptr_array_add(names, glib::strdup(name).cast()) };
                }
            }
        }
        current = unsafe { m.parent.as_ref() };
    }
    unsafe { g_ptr_array_sort(names, Some(name_cmp)) };
    let Some(m) = (unsafe { raw.as_mut() }) else {
        return names;
    };
    m.names = names;
    unsafe { g_ptr_array_ref(names) }
}
