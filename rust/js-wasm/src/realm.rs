//! Southstar — one wasmi store per JavaScript realm, with the JS values its instances reach: imported functions, externrefs and memory buffers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use southstar_js_engine::{Scope, Trace, Value};
use wasmi::{Engine, Memory, Store, StoreContextMut};

use crate::ffi::{self, Entry};

pub(crate) const PAGE_SIZE: u64 = 65536;
pub(crate) const MEMORY_MAX_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) struct HostState {
    pub realm: Weak<Realm>,
}

pub(crate) struct View {
    pub memory: Memory,
    pub buffer: Option<Value>,
    pub base: usize,
    pub len: usize,
}

pub(crate) struct Realm {
    store: RefCell<Store<HostState>>,
    pub entry: Entry,
    pub imports: RefCell<HashMap<u32, Vec<Value>>>,
    next_instance: Cell<u32>,
    pub externs: RefCell<Vec<Value>>,
    pub views: RefCell<Vec<View>>,
    pub pending: RefCell<Option<Value>>,
}

pub(crate) struct Protos {
    pub module: Value,
    pub instance: Value,
    pub memory: Value,
    pub table: Value,
    pub global: Value,
}

pub(crate) struct RealmHandle {
    pub realm: Rc<Realm>,
    pub protos: Protos,
}

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(Engine::default)
}

impl Realm {
    pub fn new() -> Rc<Realm> {
        Rc::new_cyclic(|weak| Realm {
            store: RefCell::new(Store::new(
                engine(),
                HostState {
                    realm: weak.clone(),
                },
            )),
            entry: Entry::new(),
            imports: RefCell::new(HashMap::new()),
            next_instance: Cell::new(0),
            externs: RefCell::new(Vec::new()),
            views: RefCell::new(Vec::new()),
            pending: RefCell::new(None),
        })
    }

    pub fn with_store<R>(&self, f: impl FnOnce(StoreContextMut<'_, HostState>) -> R) -> R {
        match ffi::with_active_caller(&self.entry, f) {
            Ok(result) => result,
            Err(f) => f(wasmi::AsContextMut::as_context_mut(
                &mut *self.store.borrow_mut(),
            )),
        }
    }

    pub fn next_instance_id(&self) -> u32 {
        let id = self.next_instance.get();
        self.next_instance.set(id.wrapping_add(1));
        id
    }

    pub fn add_extern(&self, value: Value) -> u32 {
        let mut externs = self.externs.borrow_mut();
        externs.push(value);
        (externs.len() - 1) as u32
    }

    pub fn extern_value(&self, index: u32) -> Value {
        self.externs
            .borrow()
            .get(index as usize)
            .cloned()
            .unwrap_or_else(Value::null)
    }

    pub fn import(&self, instance: u32, index: usize) -> Option<Value> {
        self.imports
            .borrow()
            .get(&instance)
            .and_then(|functions| functions.get(index))
            .cloned()
    }

    pub fn add_view(&self, memory: Memory) -> usize {
        let mut views = self.views.borrow_mut();
        views.push(View {
            memory,
            buffer: None,
            base: 0,
            len: 0,
        });
        views.len() - 1
    }

    pub fn view_memory(&self, view: usize) -> Option<Memory> {
        self.views.borrow().get(view).map(|view| view.memory)
    }

    pub fn detach_view(&self, scope: &mut Scope<'_>, view: usize) {
        let buffer = self
            .views
            .borrow_mut()
            .get_mut(view)
            .and_then(|view| view.buffer.take());
        if let Some(buffer) = buffer {
            drop(scope.detach_array_buffer(&buffer));
        }
    }

    pub fn sync_views(&self, scope: &mut Scope<'_>) {
        let memories: Vec<(usize, Memory)> = self
            .views
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, view)| view.buffer.is_some())
            .map(|(index, view)| (index, view.memory))
            .collect();
        if memories.is_empty() {
            return;
        }
        let current: Vec<(usize, usize, usize)> = self.with_store(|ctx| {
            memories
                .iter()
                .map(|&(index, memory)| {
                    (
                        index,
                        memory.data_ptr(&ctx) as usize,
                        memory.data_size(&ctx),
                    )
                })
                .collect()
        });
        for (index, base, len) in current {
            let stale = self
                .views
                .borrow()
                .get(index)
                .is_some_and(|view| view.base != base || view.len != len);
            if stale {
                self.detach_view(scope, index);
            }
        }
    }

    pub fn view_buffer(&self, scope: &mut Scope<'_>, view: usize) -> Result<Value, Value> {
        self.sync_views(scope);
        let existing = self
            .views
            .borrow()
            .get(view)
            .and_then(|view| view.buffer.clone());
        if let Some(buffer) = existing {
            return Ok(buffer);
        }
        let Some(memory) = self.view_memory(view) else {
            return Err(scope.type_error("wasm memory is gone"));
        };
        let (base, len) = self.with_store(|mut ctx| {
            let data = memory.data_mut(&mut ctx);
            (data.as_mut_ptr(), data.len())
        });
        let buffer = ffi::memory_buffer(scope, base, len)?;
        if let Some(entry) = self.views.borrow_mut().get_mut(view) {
            entry.buffer = Some(buffer.clone());
            entry.base = base as usize;
            entry.len = len;
        }
        Ok(buffer)
    }

    pub fn is_memory_buffer(&self, value: &Value) -> bool {
        self.views.borrow().iter().any(|view| {
            view.buffer
                .as_ref()
                .is_some_and(|buffer| buffer.same_object(value))
        })
    }
}

impl Trace for RealmHandle {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.protos.module);
        visit(&self.protos.instance);
        visit(&self.protos.memory);
        visit(&self.protos.table);
        visit(&self.protos.global);
        for functions in self.realm.imports.borrow().values() {
            functions.iter().for_each(&mut *visit);
        }
        self.realm.externs.borrow().iter().for_each(&mut *visit);
        for view in self.realm.views.borrow().iter() {
            if let Some(buffer) = &view.buffer {
                visit(buffer);
            }
        }
    }
}

impl Drop for RealmHandle {
    fn drop(&mut self) {
        let imports = core::mem::take(&mut *self.realm.imports.borrow_mut());
        let externs = core::mem::take(&mut *self.realm.externs.borrow_mut());
        let buffers: Vec<Value> = self
            .realm
            .views
            .borrow_mut()
            .iter_mut()
            .filter_map(|view| view.buffer.take())
            .collect();
        drop(imports);
        drop(externs);
        drop(buffers);
    }
}
