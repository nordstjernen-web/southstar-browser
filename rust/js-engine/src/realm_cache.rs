//! Southstar — a value built once per realm, such as a helper function compiled from engine script, kept for the realm that built it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{ContextHandle, Scope, Value};

#[derive(Default)]
pub struct RealmCache(Vec<(ContextHandle, Value)>);

impl RealmCache {
    pub fn lookup(&self, scope: &mut Scope<'_>) -> Option<Value> {
        let realm = scope.caller_handle();
        let (_, function) = self.0.iter().find(|(owner, _)| *owner == realm)?;
        (scope.function_handle(function).ok() == Some(realm)).then(|| function.clone())
    }

    pub fn store(&mut self, scope: &mut Scope<'_>, function: Value) {
        let realm = scope.caller_handle();
        self.0.retain(|(owner, _)| *owner != realm);
        self.0.push((realm, function));
    }
}
