//! Southstar — registered custom properties: CSS.registerProperty()'s process-wide registry, and the per-pass table of @property rules and script registrations the cascade checks custom properties against.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use southstar_css_prop_syntax as prop_syntax;

use super::selector_view::SheetRef;
use super::sheet::PropertyRule;

#[derive(Default)]
struct Registry {
    rules: HashMap<Vec<u8>, Arc<PropertyRule>>,
    serial: u64,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

thread_local! {
    static PASS: RefCell<Option<Rc<RegisteredProps>>> = const { RefCell::new(None) };
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
    let rule = PropertyRule::registered(
        key,
        syntax,
        registration.inherits,
        registration.initial,
        registration.has_initial,
    );
    let mut registry = registry();
    let replaced = registry.rules.insert(key.to_vec(), Arc::new(rule));
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

#[derive(Default)]
pub(super) struct RegisteredProps(HashMap<Vec<u8>, Arc<PropertyRule>>);

impl RegisteredProps {
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn get(&self, name: &[u8]) -> Option<&PropertyRule> {
        self.0.get(name).map(|rule| &**rule)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&[u8], &PropertyRule)> {
        self.0.iter().map(|(name, rule)| (name.as_slice(), &**rule))
    }

    fn put(&mut self, rule: &Arc<PropertyRule>) {
        self.0
            .insert(rule.name().to_bytes().to_vec(), Arc::clone(rule));
    }
}

pub(super) fn pass_registered() -> Option<Rc<RegisteredProps>> {
    PASS.with(|pass| pass.borrow().clone())
}

pub(super) fn registered_props_end() {
    let table = PASS.with(|pass| pass.borrow_mut().take());
    drop(table);
}

pub(super) fn registered_props_begin(sheets: &[SheetRef<'_>]) -> Rc<RegisteredProps> {
    let mut table = RegisteredProps::default();
    for sheet in sheets {
        for rule in sheet.raw().property_rules() {
            table.put(rule);
        }
    }
    for rule in registry().rules.values() {
        table.put(rule);
    }
    let table = Rc::new(table);
    PASS.with(|pass| *pass.borrow_mut() = Some(Rc::clone(&table)));
    table
}
