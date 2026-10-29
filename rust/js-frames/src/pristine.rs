//! Southstar — the page realm's platform properties as they stood before page script ran, so a frame realm gets the built-ins rather than the page's patches of them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;
use std::rc::Rc;

use southstar_js_engine::Value;

use crate::ffi::ctx::{Ctx, Descriptor, Key, Keys};
use crate::ffi::{self, Js};
use crate::page;

const MAX_CHAIN: usize = 16;

const GLOBALS_SNAPSHOT: &str = "(function(g){ var out = Object.create(null); var names = Object.getOwnPropertyNames(g); for (var i = 0; i < names.length; i++) { try { out[names[i]] = Object.getOwnPropertyDescriptor(g, names[i]); } catch (e) {} } return out; })(globalThis)";

pub(crate) struct Pristine {
    holders: HashMap<usize, (Value, Vec<(Key, Descriptor)>)>,
    globals: Value,
}

impl Pristine {
    pub fn own_properties(&self, object: &Value) -> Option<&[(Key, Descriptor)]> {
        if !object.is_object() {
            return None;
        }
        self.holders
            .get(&(object.object_identity() as usize))
            .map(|(_, properties)| properties.as_slice())
    }

    pub fn globals(&self) -> &Value {
        &self.globals
    }
}

struct Recorder {
    realm: Ctx,
    holders: HashMap<usize, (Value, Vec<(Key, Descriptor)>)>,
}

impl Recorder {
    fn record(&mut self, object: &Value) -> bool {
        if !object.is_object() {
            return false;
        }
        let identity = object.object_identity() as usize;
        if self.holders.contains_key(&identity) {
            return false;
        }
        let keys = self
            .realm
            .own_keys(object, Keys::StringsAndSymbols)
            .unwrap_or_default();
        let properties = keys
            .into_iter()
            .filter_map(|key| {
                let descriptor = self.realm.own_property(object, &key)?;
                Some((key, descriptor))
            })
            .collect();
        self.holders.insert(identity, (object.clone(), properties));
        true
    }

    fn record_chain(&mut self, object: &Value) {
        let mut current = object.clone();
        for _ in 0..MAX_CHAIN {
            if !self.record(&current) {
                return;
            }
            current = self.realm.prototype(&current);
        }
    }

    fn record_interface(&mut self, function: &Value) {
        if !self.realm.is_function(function) || !self.record(function) {
            return;
        }
        let prototype = self.realm.get(function, "prototype");
        self.record_chain(&prototype);
    }

    fn values_of(&self, object: &Value) -> Vec<Value> {
        let identity = object.object_identity() as usize;
        self.holders
            .get(&identity)
            .map(|(_, properties)| {
                properties
                    .iter()
                    .filter(|(_, descriptor)| !descriptor.accessor)
                    .map(|(_, descriptor)| descriptor.value.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn record(realm: Ctx) -> Pristine {
    let mut recorder = Recorder {
        realm,
        holders: HashMap::new(),
    };
    for class in realm.registered_classes() {
        let prototype = realm.class_proto(class);
        recorder.record_chain(&prototype);
    }
    let global = realm.global();
    let global_proto = realm.prototype(&global);
    recorder.record_chain(&global_proto);
    if recorder.record(&global) {
        for value in recorder.values_of(&global) {
            recorder.record_interface(&value);
        }
    }
    let globals = realm
        .eval_hidden(GLOBALS_SNAPSHOT, c"<pristine-globals>")
        .unwrap_or_else(Value::null);
    let global_identity = global.object_identity() as usize;
    recorder.holders.remove(&global_identity);
    Pristine {
        holders: recorder.holders,
        globals,
    }
}

pub(crate) fn snapshot(js: Js) {
    if js.is_null() {
        return;
    }
    let realm = ffi::main_realm(js);
    let pristine = Rc::new(record(realm));
    let replaced = page::with(js, |page| page.pristine.replace(pristine));
    drop(replaced);
}

pub(crate) fn current(js: Js) -> Option<Rc<Pristine>> {
    page::peek(js, |page| page.pristine.clone()).flatten()
}
