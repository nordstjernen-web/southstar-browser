//! Southstar — the C ABI of var() substitution: css.c's ns_var_map chain read through a mirror, the registered @property rules consulted for initial values, and the variable names an element's map exposes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::OnceCell;
use core::ffi::{CStr, c_int};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

use super::declarations::lookup_key;
use super::registry::RegisteredProps;
use crate::vars::{self, Lookup};

pub(super) type VarTable = HashMap<CString, CString>;

pub(super) struct RawVarMap {
    ref_count: c_int,
    pub(super) own: VarTable,
    parent: *mut RawVarMap,
    names: OnceCell<Vec<CString>>,
}

pub(super) fn new_map(own: VarTable, parent: *mut RawVarMap) -> *mut RawVarMap {
    Box::into_raw(Box::new(RawVarMap {
        ref_count: 1,
        own,
        parent,
        names: OnceCell::new(),
    }))
}

pub(super) unsafe fn map_ref(map: *mut RawVarMap) -> *mut RawVarMap {
    if let Some(m) = unsafe { map.as_mut() } {
        m.ref_count += 1;
    }
    map
}

pub(super) unsafe fn var_map_unref(mut map: *mut RawVarMap) {
    while let Some(m) = unsafe { map.as_mut() } {
        m.ref_count -= 1;
        if m.ref_count > 0 {
            return;
        }
        let parent = m.parent;
        // SAFETY: map came from new_map's Box::into_raw and its last reference is being released.
        drop(unsafe { Box::from_raw(map) });
        map = parent;
    }
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

struct Chain<'a> {
    map: Option<&'a RawVarMap>,
    registered: Option<&'a RegisteredProps>,
}

pub(super) fn chain_lookup_text<'a>(
    mut map: Option<&'a RawVarMap>,
    name: &[u8],
) -> Option<&'a CStr> {
    let key = c_string(name);
    while let Some(m) = map {
        if let Some(value) = m.own.get(&key) {
            return Some(value);
        }
        map = unsafe { m.parent.as_ref() };
    }
    None
}

pub(super) fn chain_lookup<'a>(map: Option<&'a RawVarMap>, name: &[u8]) -> Option<&'a [u8]> {
    chain_lookup_text(map, name).map(CStr::to_bytes)
}

impl Lookup for Chain<'_> {
    fn value(&self, name: &[u8]) -> Option<&[u8]> {
        chain_lookup(self.map, name)
    }

    fn registered_initial(&self, name: &[u8]) -> Option<Option<&[u8]>> {
        self.registered?.get(lookup_key(name))?.registered_initial()
    }
}

pub(super) fn substitute_in(
    value: &[u8],
    map: *const RawVarMap,
    registered: Option<&RegisteredProps>,
) -> Option<Vec<u8>> {
    let chain = Chain {
        map: unsafe { map.as_ref() },
        registered,
    };
    let lookup: Option<&dyn Lookup> = Some(&chain);
    vars::substitute(value, lookup.filter(|_| !map.is_null()), 0)
}

pub(super) fn chain_names(map: &RawVarMap) -> Vec<&CStr> {
    map.names
        .get_or_init(|| names_of(map))
        .iter()
        .map(CString::as_c_str)
        .collect()
}

fn names_of(map: &RawVarMap) -> Vec<CString> {
    let mut names = Vec::new();
    let mut seen: HashSet<&[u8]> = HashSet::new();
    let mut current = Some(map);
    while let Some(m) = current {
        for (name, value) in &m.own {
            if !seen.insert(name.to_bytes()) {
                continue;
            }
            if !value.to_bytes().eq_ignore_ascii_case(b"initial") {
                names.push(name.clone());
            }
        }
        current = unsafe { m.parent.as_ref() };
    }
    names.sort();
    names
}
