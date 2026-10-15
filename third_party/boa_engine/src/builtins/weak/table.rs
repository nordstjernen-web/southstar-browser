//! The entries of `WeakMap` and `WeakSet` objects, keyed by values that `CanBeHeldWeakly`.
//!
//! More information:
//!  - [ECMAScript reference][spec]
//!
//! [spec]: https://tc39.es/ecma262/#sec-canbeheldweakly

use boa_gc::{Finalize, Trace};

use crate::{
    JsValue, builtins::symbol::is_registered_symbol, object::ErasedVTableObject,
    object::JsData, symbol::JsSymbol,
};

/// `CanBeHeldWeakly ( v )`
///
/// [spec]: https://tc39.es/ecma262/#sec-canbeheldweakly
pub(crate) fn can_be_held_weakly(value: &JsValue) -> bool {
    // 1. If v is an Object, return true.
    if value.is_object() {
        return true;
    }
    // 2. If v is a Symbol and KeyForSymbol(v) is undefined, return true.
    // 3. Return false.
    value
        .as_symbol()
        .is_some_and(|symbol| !is_registered_symbol(&symbol))
}

/// The `[[WeakMapData]]` or `[[WeakSetData]]` of a weak collection.
///
/// Entries keyed by objects are ephemerons that the garbage collector clears with their key.
/// Entries keyed by symbols are held strongly, since Boa never collects symbols; no program
/// can observe the difference.
#[derive(Trace, Finalize)]
pub(crate) struct WeakTable<V: Trace + Clone + 'static> {
    objects: boa_gc::WeakMap<ErasedVTableObject, V>,
    symbols: Vec<(JsSymbol, V)>,
}

impl<V: Trace + Clone + 'static> JsData for WeakTable<V> {}

impl<V: Trace + Clone + 'static> Default for WeakTable<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Trace + Clone + 'static> WeakTable<V> {
    /// Creates an empty table.
    pub(crate) fn new() -> Self {
        Self {
            objects: boa_gc::WeakMap::new(),
            symbols: Vec::new(),
        }
    }

    /// Returns the value stored for `key`, if any.
    pub(crate) fn get(&self, key: &JsValue) -> Option<V> {
        if let Some(object) = key.as_object() {
            let entry = self.objects.get(object.inner())?;
            return entry.value().map(|value| value.clone());
        }
        let symbol = key.as_symbol()?;
        self.symbols
            .iter()
            .find(|(held, _)| *held == symbol)
            .map(|(_, value)| value.clone())
    }

    /// Returns `true` if the table has an entry for `key`.
    pub(crate) fn contains(&self, key: &JsValue) -> bool {
        if let Some(object) = key.as_object() {
            return self.objects.contains_key(object.inner());
        }
        key.as_symbol()
            .is_some_and(|symbol| self.symbols.iter().any(|(held, _)| *held == symbol))
    }

    /// Stores `value` for `key`, which must satisfy [`can_be_held_weakly`].
    pub(crate) fn insert(&mut self, key: &JsValue, value: V) {
        if let Some(object) = key.as_object() {
            self.objects.insert(object.inner(), value);
        } else if let Some(symbol) = key.as_symbol() {
            if let Some(entry) = self.symbols.iter_mut().find(|(held, _)| *held == symbol) {
                entry.1 = value;
            } else {
                self.symbols.push((symbol, value));
            }
        }
    }

    /// Removes the entry for `key`, returning `true` if there was one.
    pub(crate) fn remove(&mut self, key: &JsValue) -> bool {
        if let Some(object) = key.as_object() {
            return self.objects.remove(object.inner());
        }
        let Some(symbol) = key.as_symbol() else {
            return false;
        };
        let before = self.symbols.len();
        self.symbols.retain(|(held, _)| *held != symbol);
        self.symbols.len() != before
    }
}
