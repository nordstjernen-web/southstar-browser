//! Southstar — MutationObserver: the targets each observer watches, the records DOM changes queue on it and the microtask that delivers them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use southstar_dom::Node;
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::{bind, bind_if_not_callable, page, page_or_new, set, set_index};

#[derive(Default)]
struct Target {
    node: usize,
    subtree: bool,
    child_list: bool,
    attributes: bool,
    character_data: bool,
    attribute_old_value: bool,
    character_data_old_value: bool,
    attribute_filter: Option<Vec<Vec<u8>>>,
}

impl Target {
    fn covers(&self, node: usize) -> bool {
        if self.node == 0 || node == 0 {
            return false;
        }
        if self.node == node {
            return true;
        }
        if !self.subtree {
            return false;
        }
        let mut parent = unsafe { Node::from_ptr(node as *const _) }.and_then(Node::parent);
        while let Some(p) = parent {
            if p.as_ptr() as usize == self.node {
                return true;
            }
            parent = p.parent();
        }
        false
    }

    fn filter_matches(&self, name: Option<&[u8]>) -> bool {
        let Some(filter) = &self.attribute_filter else {
            return true;
        };
        let Some(name) = name else {
            return false;
        };
        filter.iter().any(|f| f.eq_ignore_ascii_case(name))
    }
}

struct Record {
    kind: Vec<u8>,
    target: usize,
    added: Option<Vec<usize>>,
    removed: Option<Vec<usize>>,
    previous_sibling: usize,
    next_sibling: usize,
    attribute_name: Option<Vec<u8>>,
    attribute_namespace: Option<Vec<u8>>,
    old_value: Option<Vec<u8>>,
}

#[derive(Default)]
struct State {
    disconnected: bool,
    pin: Option<Value>,
    targets: Vec<Target>,
    records: Vec<Record>,
}

pub(crate) struct Observer {
    js: Js,
    callback: Value,
    state: RefCell<State>,
}

#[derive(Clone)]
struct Handle(Rc<Observer>);

impl Drop for Observer {
    fn drop(&mut self) {
        let Some(page) = page(self.js) else {
            return;
        };
        let gone: *const Observer = self;
        if let Ok(mut list) = page.mutation.try_borrow_mut()
            && let Some(list) = list.as_mut()
        {
            crate::forget(list, gone);
        }
    }
}

impl Observer {
    pub(crate) fn disconnect(&self) {
        let pin = {
            let mut state = self.state.borrow_mut();
            state.disconnected = true;
            state.targets.clear();
            state.records.clear();
            state.pin.take()
        };
        drop(pin);
    }

    pub(crate) fn teardown(&self) {
        let mut state = self.state.borrow_mut();
        state.disconnected = true;
        state.records.clear();
        state.targets.clear();
    }
}

pub(crate) struct Emit<'a> {
    pub kind: Option<&'a [u8]>,
    pub target: usize,
    pub added: Option<Vec<usize>>,
    pub removed: Option<Vec<usize>>,
    pub previous_sibling: usize,
    pub next_sibling: usize,
    pub attribute_name: Option<&'a [u8]>,
    pub attribute_namespace: Option<&'a [u8]>,
    pub old_value: Option<&'a [u8]>,
}

fn observers(js: Js) -> Option<Vec<Rc<Observer>>> {
    let page = page(js)?;
    let list = page.mutation.borrow();
    let list = list.as_ref()?;
    Some(list.iter().filter_map(Weak::upgrade).collect())
}

fn arm(nodes: &[usize]) {
    for &node in nodes {
        ffi::arm_invalidate(node);
    }
}

pub(crate) fn emit(js: Js, emit: &Emit<'_>) {
    if emit.target == 0 {
        return;
    }
    let Some(observers) = observers(js) else {
        return;
    };
    let kind = emit.kind;
    let wants_child = kind == Some(b"childList");
    let wants_attr = kind == Some(b"attributes");
    let wants_cdata = kind == Some(b"characterData");
    if wants_child && emit.added.is_none() && emit.removed.is_none() {
        return;
    }
    arm(&[emit.target]);
    arm(emit.added.as_deref().unwrap_or_default());
    arm(emit.removed.as_deref().unwrap_or_default());
    arm(&[emit.previous_sibling, emit.next_sibling]);
    for observer in &observers {
        let mut state = observer.state.borrow_mut();
        if state.disconnected {
            continue;
        }
        let Some(target) = state.targets.iter().find(|t| {
            t.covers(emit.target)
                && (!wants_child || t.child_list)
                && (!wants_attr || t.attributes)
                && (!wants_cdata || t.character_data)
                && (!wants_attr || t.filter_matches(emit.attribute_name))
        }) else {
            continue;
        };
        let keeps_old = (wants_attr && target.attribute_old_value)
            || (wants_cdata && target.character_data_old_value);
        let record = Record {
            kind: kind.unwrap_or_default().to_vec(),
            target: emit.target,
            added: emit.added.clone(),
            removed: emit.removed.clone(),
            previous_sibling: emit.previous_sibling,
            next_sibling: emit.next_sibling,
            attribute_name: emit.attribute_name.map(<[u8]>::to_vec),
            attribute_namespace: emit.attribute_namespace.map(<[u8]>::to_vec),
            old_value: emit.old_value.filter(|_| keeps_old).map(<[u8]>::to_vec),
        };
        state.records.push(record);
    }
    schedule_drain(js);
}

pub(crate) fn emit_child_list(
    js: Js,
    target: usize,
    added: Option<Vec<usize>>,
    removed: Option<Vec<usize>>,
    previous_sibling: usize,
    next_sibling: usize,
) {
    if target == 0 {
        return;
    }
    let Some(observers) = observers(js) else {
        return;
    };
    if added.is_none() && removed.is_none() {
        return;
    }
    arm(&[target]);
    arm(added.as_deref().unwrap_or_default());
    arm(removed.as_deref().unwrap_or_default());
    arm(&[previous_sibling, next_sibling]);
    for observer in &observers {
        let mut state = observer.state.borrow_mut();
        if state.disconnected {
            continue;
        }
        if !state
            .targets
            .iter()
            .any(|t| t.child_list && t.covers(target))
        {
            continue;
        }
        state.records.push(Record {
            kind: b"childList".to_vec(),
            target,
            added: added.clone(),
            removed: removed.clone(),
            previous_sibling,
            next_sibling,
            attribute_name: None,
            attribute_namespace: None,
            old_value: None,
        });
    }
    schedule_drain(js);
}

pub(crate) fn scrub_node(js: Js, node: usize) {
    let Some(observers) = observers(js) else {
        return;
    };
    let clear = |slot: &mut usize| {
        if *slot == node {
            *slot = 0;
        }
    };
    let remove = |list: &mut Option<Vec<usize>>| {
        if let Some(list) = list
            && let Some(index) = list.iter().position(|&n| n == node)
        {
            list.swap_remove(index);
        }
    };
    for observer in &observers {
        let mut state = observer.state.borrow_mut();
        for target in &mut state.targets {
            clear(&mut target.node);
        }
        for record in &mut state.records {
            clear(&mut record.target);
            clear(&mut record.previous_sibling);
            clear(&mut record.next_sibling);
            remove(&mut record.added);
            remove(&mut record.removed);
        }
    }
}

fn schedule_drain(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    let any = page
        .mutation
        .borrow()
        .as_ref()
        .is_some_and(|list| !list.is_empty());
    if !any || page.drain_scheduled.get() || !ffi::has_main_context(js) {
        return;
    }
    page.drain_scheduled.set(true);
    ffi::with_main_context(js, |scope| {
        let _ = scope.enqueue_job(drain);
    });
}

fn node_or_null(scope: &mut Scope<'_>, node: usize) -> Value {
    if node == 0 {
        Value::null()
    } else {
        ffi::make_element(scope, node)
    }
}

fn text_or_null(scope: &mut Scope<'_>, text: Option<&[u8]>) -> Value {
    text.map_or_else(Value::null, |text| scope.string_from_bytes(text))
}

fn node_array(scope: &mut Scope<'_>, nodes: Option<&[usize]>) -> Value {
    let array = scope.new_array();
    for (i, &node) in nodes.unwrap_or_default().iter().enumerate() {
        let element = ffi::make_element(scope, node);
        set_index(scope, &array, i as u32, element);
    }
    array
}

fn record_value(scope: &mut Scope<'_>, record: &Record) -> Value {
    let global = scope.global();
    let constructor = crate::get(scope, &global, "MutationRecord");
    let proto = crate::get(scope, &constructor, "prototype");
    let object = if proto.is_object() {
        scope.new_object_with_proto(&proto)
    } else {
        scope.new_object()
    };
    let kind = scope.string_from_bytes(&record.kind);
    set(scope, &object, "type", kind);
    let target = node_or_null(scope, record.target);
    set(scope, &object, "target", target);
    let added = node_array(scope, record.added.as_deref());
    set(scope, &object, "addedNodes", added);
    let removed = node_array(scope, record.removed.as_deref());
    set(scope, &object, "removedNodes", removed);
    let previous = node_or_null(scope, record.previous_sibling);
    set(scope, &object, "previousSibling", previous);
    let next = node_or_null(scope, record.next_sibling);
    set(scope, &object, "nextSibling", next);
    let name = text_or_null(scope, record.attribute_name.as_deref());
    set(scope, &object, "attributeName", name);
    let namespace = text_or_null(scope, record.attribute_namespace.as_deref());
    set(scope, &object, "attributeNamespace", namespace);
    let old_value = text_or_null(scope, record.old_value.as_deref());
    set(scope, &object, "oldValue", old_value);
    object
}

fn records_array(scope: &mut Scope<'_>, records: &[Record]) -> Value {
    let array = scope.new_array();
    for (i, record) in records.iter().enumerate() {
        let value = record_value(scope, record);
        set_index(scope, &array, i as u32, value);
    }
    array
}

fn drain(scope: &mut Scope<'_>) {
    let js = ffi::js_of(scope);
    let Some(page) = page(js) else {
        return;
    };
    if page.mutation.borrow().is_none() {
        return;
    }
    page.drain_scheduled.set(false);
    let pending: Vec<(Rc<Observer>, Value)> = observers(js)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|observer| {
            let state = observer.state.borrow();
            if state.disconnected || state.records.is_empty() {
                return None;
            }
            let wrapper = state.pin.clone()?;
            drop(state);
            Some((observer, wrapper))
        })
        .collect();
    for (observer, wrapper) in &pending {
        let records = {
            let mut state = observer.state.borrow_mut();
            if state.disconnected || state.records.is_empty() {
                continue;
            }
            core::mem::take(&mut state.records)
        };
        let array = records_array(scope, &records);
        drop(records);
        let callback = observer.callback.clone();
        let args = [array, wrapper.clone()];
        if let Err(exception) = ffi::call_observer(scope, &callback, wrapper, &args, None, false) {
            crate::report_error(js, scope, "MutationObserver", &exception);
        }
    }
}

fn observer_of(scope: &mut Scope<'_>, this: &Value) -> Option<Rc<Observer>> {
    scope.host_data::<Handle>(this).map(|handle| handle.0)
}

fn option(scope: &mut Scope<'_>, options: &Value, key: &str) -> (bool, bool) {
    match scope.get(options, key) {
        Ok(value) if value.is_undefined() => (false, false),
        Ok(value) => (scope.to_bool(&value), true),
        Err(_) => (false, true),
    }
}

fn attribute_filter(scope: &mut Scope<'_>, options: &Value) -> Option<Vec<Vec<u8>>> {
    let filter = scope.get(options, "attributeFilter").ok()?;
    if !filter.is_object() {
        return None;
    }
    let length = crate::array_length(scope, &filter);
    let mut names = Vec::new();
    for i in 0..length {
        let item = scope
            .get_index(&filter, i)
            .unwrap_or_else(|_| Value::undefined());
        if let Ok(name) = crate::text_of(scope, &item) {
            names.push(name);
        }
    }
    Some(names)
}

pub(crate) fn observe(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(observer) = observer_of(scope, this) else {
        return Ok(Value::undefined());
    };
    let Some(target) = args.first() else {
        return Ok(Value::undefined());
    };
    let node = ffi::node_address(target);
    if node == 0 {
        return Ok(Value::undefined());
    }
    let mut t = Target {
        node,
        child_list: true,
        ..Target::default()
    };
    if let Some(options) = args.get(1).filter(|options| options.is_object()) {
        t.subtree = option(scope, options, "subtree").0;
        let (child_list, child_list_set) = option(scope, options, "childList");
        t.child_list = child_list;
        let (attributes, attributes_set) = option(scope, options, "attributes");
        t.attributes = attributes;
        let (character_data, character_data_set) = option(scope, options, "characterData");
        t.character_data = character_data;
        let (attribute_old_value, attribute_old_set) = option(scope, options, "attributeOldValue");
        t.attribute_old_value = attribute_old_value;
        if attribute_old_set && !attributes_set {
            t.attributes = true;
        }
        let (character_data_old_value, character_data_old_set) =
            option(scope, options, "characterDataOldValue");
        t.character_data_old_value = character_data_old_value;
        if character_data_old_set && !character_data_set {
            t.character_data = true;
        }
        t.attribute_filter = attribute_filter(scope, options);
        if t.attribute_filter.is_some() && !attributes_set {
            t.attributes = true;
        }
        if !child_list_set {
            t.child_list = false;
        }
    }
    if !t.child_list && !t.attributes && !t.character_data {
        return Err(scope.type_error(
            "MutationObserver.observe: at least one of childList, attributes, characterData required",
        ));
    }
    if (t.attribute_old_value || t.attribute_filter.is_some()) && !t.attributes {
        return Err(scope.type_error(
            "MutationObserver.observe: attributeOldValue/attributeFilter require attributes:true",
        ));
    }
    if t.character_data_old_value && !t.character_data {
        return Err(scope.type_error(
            "MutationObserver.observe: characterDataOldValue requires characterData:true",
        ));
    }
    let mut state = observer.state.borrow_mut();
    if let Some(index) = state.targets.iter().position(|e| e.node == node) {
        state.targets.remove(index);
    }
    state.targets.push(t);
    state.disconnected = false;
    if state.pin.is_none() {
        state.pin = Some(this.clone());
    }
    Ok(Value::undefined())
}

pub(crate) fn disconnect(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    if let Some(observer) = observer_of(scope, this) {
        observer.disconnect();
    }
    Ok(Value::undefined())
}

pub(crate) fn take_records(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let Some(observer) = observer_of(scope, this) else {
        return Ok(scope.new_array());
    };
    let records = core::mem::take(&mut observer.state.borrow_mut().records);
    Ok(records_array(scope, &records))
}

const METHODS: [(&str, u32, NativeFn); 3] = [
    ("observe", 2, observe),
    ("disconnect", 0, disconnect),
    ("takeRecords", 0, take_records),
];

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    new_target: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let Some(callback) = args.first().filter(|cb| scope.is_function(cb)).cloned() else {
        return Err(scope.type_error("MutationObserver callback must be callable"));
    };
    let js = ffi::js_of(scope);
    let observer = Rc::new(Observer {
        js,
        callback,
        state: RefCell::new(State::default()),
    });
    let (object, proto_bound) =
        crate::new_instance(scope, new_target, Handle(observer.clone()), &METHODS);
    if !proto_bound {
        for (name, arity, f) in METHODS {
            bind(scope, &object, name, arity, f);
        }
    }
    for (name, arity, f) in METHODS {
        bind_if_not_callable(scope, &object, name, arity, f);
    }
    if !js.is_null() {
        let page = page_or_new(js);
        page.mutation
            .borrow_mut()
            .get_or_insert_with(Vec::new)
            .push(Rc::downgrade(&observer));
    }
    Ok(object)
}
