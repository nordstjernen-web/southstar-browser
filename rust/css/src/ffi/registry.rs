//! Southstar — the C ABI of registered custom properties: CSS.registerProperty()'s process-wide registry, and the per-pass table of @property rules and script registrations the cascade checks custom properties against.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{c_char, c_int};
use core::ptr;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use southstar_css_prop_syntax as prop_syntax;
use southstar_glib::{self as glib, GBoolean, GHashTable};

use super::selector_view::SheetRef;
use super::sheet::{RawPropertyRule, RawSheet, property_rule_clear};

const REGISTER_OK: c_int = 0;
const REGISTER_BAD_NAME: c_int = 1;
const REGISTER_BAD_SYNTAX: c_int = 2;
const REGISTER_BAD_INITIAL: c_int = 3;
const REGISTER_EXISTS: c_int = 4;

struct Registered(Box<RawPropertyRule>);

unsafe impl Send for Registered {}

impl Drop for Registered {
    fn drop(&mut self) {
        unsafe { property_rule_clear(ptr::from_mut(&mut *self.0).cast()) };
    }
}

#[derive(Default)]
struct Registry {
    rules: HashMap<Vec<u8>, Registered>,
    serial: u64,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

thread_local! {
    static PASS: Cell<*mut GHashTable> = const { Cell::new(ptr::null_mut()) };
}

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

fn valid_name(name: &[u8]) -> bool {
    name.len() > 2
        && name.starts_with(b"--")
        && !name[2..]
            .iter()
            .any(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
}

fn syntax_rejects_initial(
    syntax: &prop_syntax::SyntaxDef,
    initial_value: Option<&[u8]>,
    has_initial: bool,
) -> bool {
    (!syntax.universal() && !has_initial)
        || (has_initial
            && !initial_value.is_some_and(|value| prop_syntax::initial_valid(syntax, value)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_register_property(
    name: *const c_char,
    syntax_text: *const c_char,
    inherits: GBoolean,
    initial_value: *const c_char,
    has_initial: GBoolean,
) -> c_int {
    let Some(key) = (unsafe { glib::bytes(name) }).filter(|name| valid_name(name)) else {
        return REGISTER_BAD_NAME;
    };
    if registry().rules.contains_key(key) {
        return REGISTER_EXISTS;
    }
    let Some(syntax) = unsafe { glib::bytes(syntax_text) }.and_then(prop_syntax::parse) else {
        return REGISTER_BAD_SYNTAX;
    };
    if syntax_rejects_initial(
        &syntax,
        unsafe { glib::bytes(initial_value) },
        has_initial != 0,
    ) {
        return REGISTER_BAD_INITIAL;
    }
    let rule = unsafe {
        RawPropertyRule::registered(
            name,
            syntax_text,
            syntax,
            inherits,
            initial_value,
            has_initial,
        )
    };
    let mut registry = registry();
    let replaced = registry
        .rules
        .insert(key.to_vec(), Registered(Box::new(rule)));
    registry.serial += 1;
    drop(registry);
    drop(replaced);
    REGISTER_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_clear_registered_properties() {
    let mut registry = registry();
    if registry.rules.is_empty() {
        return;
    }
    let cleared = core::mem::take(&mut registry.rules);
    registry.serial += 1;
    drop(registry);
    drop(cleared);
}

pub(super) fn registered_property_serial() -> u64 {
    registry().serial
}

fn put_rules(table: *mut GHashTable, sheet: &RawSheet) {
    for rule in sheet.property_rules() {
        if !rule.name().is_null() {
            unsafe {
                glib::g_hash_table_replace(
                    table,
                    rule.name().cast_mut().cast(),
                    ptr::from_ref(rule).cast_mut().cast(),
                )
            };
        }
    }
}

pub(super) fn pass_registered() -> *mut GHashTable {
    PASS.get()
}

pub(super) fn registered_props_end() {
    let table = PASS.replace(ptr::null_mut());
    if !table.is_null() {
        unsafe { glib::g_hash_table_destroy(table) };
    }
}

pub(super) fn registered_props_begin(sheets: &[SheetRef<'_>]) -> *mut GHashTable {
    let table = unsafe { glib::g_hash_table_new(Some(glib::g_str_hash), Some(glib::g_str_equal)) };
    for sheet in sheets {
        put_rules(table, sheet.raw());
    }
    for registered in registry().rules.values() {
        let rule = &*registered.0;
        unsafe {
            glib::g_hash_table_replace(
                table,
                rule.name().cast_mut().cast(),
                ptr::from_ref(rule).cast_mut().cast(),
            )
        };
    }
    PASS.set(table);
    table
}
