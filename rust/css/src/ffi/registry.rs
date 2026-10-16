//! Southstar — the C ABI of registered custom properties: CSS.registerProperty()'s process-wide registry, and the per-pass table of @property rules and script registrations the cascade checks custom properties against.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ptr;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use southstar_css_prop_syntax as prop_syntax;
use southstar_glib::{self as glib, GHashTable};

use super::selector_view::SheetRef;
use super::sheet::{RawPropertyRule, RawSheet, property_rule_clear};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    BadName,
    BadSyntax,
    BadInitial,
    Exists,
}

pub struct Registration<'a> {
    pub name: Option<&'a [u8]>,
    pub syntax: &'a [u8],
    pub inherits: bool,
    pub initial: Option<&'a [u8]>,
    pub has_initial: bool,
}

pub fn register_property(registration: &Registration<'_>) -> Result<(), RegisterError> {
    let Some(key) = registration.name.filter(|name| valid_name(name)) else {
        return Err(RegisterError::BadName);
    };
    if registry().rules.contains_key(key) {
        return Err(RegisterError::Exists);
    }
    let Some(syntax) = prop_syntax::parse(registration.syntax) else {
        return Err(RegisterError::BadSyntax);
    };
    if syntax_rejects_initial(&syntax, registration.initial, registration.has_initial) {
        return Err(RegisterError::BadInitial);
    }
    let rule = RawPropertyRule::registered(
        key,
        registration.syntax,
        syntax,
        registration.inherits,
        registration.initial,
        registration.has_initial,
    );
    let mut registry = registry();
    let replaced = registry
        .rules
        .insert(key.to_vec(), Registered(Box::new(rule)));
    registry.serial += 1;
    drop(registry);
    drop(replaced);
    Ok(())
}

pub fn clear_registered_properties() {
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
