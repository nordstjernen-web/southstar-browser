//! Southstar — the C ABI of the custom-property cascade: css.c's matched var_match entries read through a mirror, the registered @property rules and the parent's variable map consulted, and the element's ns_var_map built, reused or taken from the per-pass cache.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::size_of;
use core::ptr;
use core::slice;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable};

use super::sheet::RawPropertyRule;
use super::vars::{RawVarMap, chain_lookup, map_ref, new_map};
use crate::custom_props::{self, Inherited, Own, Registered, Registry, VarMatch};

#[repr(C)]
struct RawVarMatch {
    origin: c_int,
    spec_a: c_int,
    spec_b: c_int,
    spec_c: c_int,
    sheet_index: c_int,
    layer_order: c_int,
    scope_order: c_int,
    source_order: c_int,
    decl_order: c_int,
    important: GBoolean,
    inline_style: GBoolean,
    rule: *const c_void,
    name: *const c_char,
    text: *const c_char,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<RawVarMatch>() == 72);

struct Props(*mut GHashTable);

struct Parent<'a>(&'a RawVarMap);

fn registered(rule: &RawPropertyRule) -> Registered<'_> {
    Registered {
        inherits: rule.inherits(),
        initial: rule.initial(),
    }
}

impl Props {
    fn rule(&self, name: &[u8]) -> Option<&RawPropertyRule> {
        let key = CString::new(name).ok()?;
        let rule = unsafe { glib::g_hash_table_lookup(self.0, key.as_ptr().cast()) };
        unsafe { rule.cast::<RawPropertyRule>().as_ref() }
    }
}

impl Registry for Props {
    fn get(&self, name: &[u8]) -> Option<Registered<'_>> {
        self.rule(name).map(registered)
    }

    fn each(&self, f: &mut dyn FnMut(&[u8], Registered<'_>)) {
        for (key, value) in unsafe { glib::hash_table_entries(self.0) } {
            let rule = unsafe { value.cast::<RawPropertyRule>().as_ref() };
            if let (false, Some(rule)) = (key.is_null(), rule) {
                f(
                    unsafe { CStr::from_ptr(key.cast()) }.to_bytes(),
                    registered(rule),
                );
            }
        }
    }

    fn rejects(&self, name: &[u8], value: &[u8]) -> bool {
        self.rule(name).is_some_and(|rule| rule.rejects(value))
    }
}

impl Inherited for Parent<'_> {
    fn lookup(&self, name: &[u8]) -> Option<&[u8]> {
        chain_lookup(Some(self.0), name)
    }
}

unsafe fn entries<'a>(array: *const GArray) -> &'a [RawVarMatch] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.data.cast::<RawVarMatch>(), a.len as usize)
        },
        _ => &[],
    }
}

fn views(raw: &[RawVarMatch]) -> Vec<VarMatch<'_>> {
    raw.iter()
        .filter(|m| !m.name.is_null() && !m.text.is_null())
        .map(|m| VarMatch {
            origin: m.origin,
            specificity: (m.spec_a, m.spec_b, m.spec_c),
            sheet_index: m.sheet_index,
            layer_order: m.layer_order,
            scope_order: m.scope_order,
            source_order: m.source_order,
            decl_order: m.decl_order,
            important: m.important != 0,
            inline_style: m.inline_style != 0,
            rule: m.rule as usize,
            name: unsafe { CStr::from_ptr(m.name) }.to_bytes(),
            text: unsafe { CStr::from_ptr(m.text) }.to_bytes(),
        })
        .collect()
}

fn own_table(own: Own) -> *mut GHashTable {
    let table = unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            Some(glib::g_free),
        )
    };
    for (name, value) in own {
        unsafe {
            glib::g_hash_table_insert(
                table,
                glib::strdup(&name).cast(),
                glib::strdup(&value).cast(),
            )
        };
    }
    table
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_build_vars(
    parent: *mut c_void,
    var_matches: *mut GArray,
    registered_props: *mut GHashTable,
    adjust_cache: *mut GHashTable,
) -> *mut c_void {
    let parent = parent.cast::<RawVarMap>();
    let raw = unsafe { entries(var_matches) };
    let have_regs =
        !registered_props.is_null() && unsafe { glib::g_hash_table_size(registered_props) } > 0;
    let have_local = !raw.is_empty();
    let inherited = unsafe { parent.as_ref() }.map(Parent);
    let inherited = inherited.as_ref().map(|p| p as &dyn Inherited);
    if parent.is_null() && !have_regs && !have_local {
        return ptr::null_mut();
    }
    let mut matches = views(raw);
    if have_local {
        custom_props::sort(&mut matches);
    }
    if !have_regs {
        if !have_local {
            return unsafe { map_ref(parent) }.cast();
        }
        let own = custom_props::unregistered(&matches, inherited);
        return unsafe { new_map(own_table(own), map_ref(parent)) }.cast();
    }
    let cacheable = !parent.is_null() && !have_local && !adjust_cache.is_null();
    if cacheable {
        let hit = unsafe { glib::g_hash_table_lookup(adjust_cache, parent.cast()) };
        if !hit.is_null() {
            return unsafe { map_ref(hit.cast()) }.cast();
        }
    }
    let own = custom_props::registered(&matches, inherited, &Props(registered_props));
    let built = if !parent.is_null() && !have_local && own.is_empty() {
        unsafe { map_ref(parent) }
    } else {
        unsafe { new_map(own_table(own), map_ref(parent)) }
    };
    if cacheable {
        unsafe {
            glib::g_hash_table_insert(adjust_cache, map_ref(parent).cast(), map_ref(built).cast())
        };
    }
    built.cast()
}
