//! Southstar — NodeIterator, its per-page registry and the pre-removal steps that keep its reference node live.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_dom::serialize::is_embedded_doc;
use southstar_js_engine::{Attributes, NativeFn, Scope, Trace, Value};

use crate::ffi::{Js, js_of, main_scope, unwrap_node, wrap_node};
use crate::walker::{ACCEPT, filter, init_common};
use crate::{Element, JsResult, arg, bind, get, is_inclusive_ancestor, noop};

const GETTER: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) struct NodeIter {
    root: Value,
    reference: RefCell<Value>,
    before: Cell<bool>,
}

impl NodeIter {
    fn reference_node(&self) -> Option<Element> {
        unwrap_node(&self.reference.borrow())
    }

    fn set_reference(&self, value: Value) {
        let old = self.reference.replace(value);
        drop(old);
    }
}

#[derive(Clone)]
struct Handle(Rc<NodeIter>);

impl Trace for Handle {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.0.root);
        if let Ok(reference) = self.0.reference.try_borrow() {
            visit(&reference);
        }
    }
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Vec<Weak<NodeIter>>>> = RefCell::new(HashMap::new());
}

fn register(js: Js, iter: &Rc<NodeIter>) {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let list = pages.entry(js).or_default();
        list.retain(|weak| weak.strong_count() > 0);
        list.push(Rc::downgrade(iter));
    });
}

fn live_iters(js: Js) -> Vec<Rc<NodeIter>> {
    PAGES
        .try_with(|pages| {
            let pages = pages.try_borrow().ok()?;
            let list = pages.get(&js)?;
            Some(list.iter().filter_map(Weak::upgrade).collect())
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn teardown(js: Js) {
    let removed = PAGES.with(|pages| pages.borrow_mut().remove(&js));
    drop(removed);
}

fn handle(scope: &mut Scope<'_>, this: &Value) -> Option<Rc<NodeIter>> {
    scope.host_data::<Handle>(this).map(|h| h.0)
}

fn skip_documents(mut node: Option<Element>, backward: bool) -> Option<Element> {
    while let Some(n) = node.filter(|&n| is_embedded_doc(n)) {
        node = if backward {
            n.prev_sibling()
        } else {
            n.next_sibling()
        };
    }
    node
}

fn following(node: Element, root: Element) -> Option<Element> {
    if let Some(first) = node.first_child() {
        let child = if is_embedded_doc(first) {
            skip_documents(first.next_sibling(), false)
        } else {
            Some(first)
        };
        if child.is_some() {
            return child;
        }
    }
    southstar_dom::ancestors_and_self(node)
        .take_while(|&n| n != root)
        .find_map(|n| skip_documents(n.next_sibling(), false))
}

fn preceding(node: Element, root: Element) -> Option<Element> {
    if node == root {
        return None;
    }
    let Some(mut p) = skip_documents(node.prev_sibling(), true) else {
        return node.parent();
    };
    while let Some(last) = skip_documents(p.last_child(), true) {
        p = last;
    }
    Some(p)
}

fn traverse(scope: &mut Scope<'_>, this: &Value, forward: bool) -> JsResult {
    let Some(it) = handle(scope, this) else {
        return Ok(Value::null());
    };
    let (Some(root), Some(mut node)) = (unwrap_node(&it.root), it.reference_node()) else {
        return Ok(Value::null());
    };
    let mut before = it.before.get();
    let mut resets = 0;
    loop {
        if forward {
            if before {
                before = false;
            } else {
                let Some(n) = following(node, root) else {
                    return Ok(Value::null());
                };
                node = n;
            }
        } else if before {
            let Some(n) = preceding(node, root) else {
                return Ok(Value::null());
            };
            node = n;
        } else {
            before = true;
        }
        let result = filter(scope, this, node)?;
        let detached = !is_inclusive_ancestor(root, node);
        if result == ACCEPT {
            if !detached {
                let wrapped = wrap_node(scope, node);
                it.set_reference(wrapped);
                it.before.set(before);
            }
            return Ok(wrap_node(scope, node));
        }
        if detached {
            if resets > 0 {
                return Ok(Value::null());
            }
            resets += 1;
            let Some(reference) = it.reference_node() else {
                return Ok(Value::null());
            };
            node = reference;
            before = it.before.get();
        }
    }
}

fn next_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    traverse(scope, this, true)
}

fn previous_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    traverse(scope, this, false)
}

fn reference_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(handle(scope, this).map_or_else(Value::null, |it| it.reference.borrow().clone()))
}

fn pointer_before(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(
        handle(scope, this).is_some_and(|it| it.before.get()),
    ))
}

fn root(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    Ok(handle(scope, this).map_or_else(Value::null, |it| it.root.clone()))
}

fn define_getter(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn) {
    let getter = scope.function(name, 0, f);
    let _ = scope.define_accessor(object, name, Some(&getter), None, GETTER);
}

pub(crate) fn create(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let root_value = arg(args, 0);
    if !root_value.is_object() || unwrap_node(&root_value).is_none() {
        return Err(scope.type_error(
            "Failed to execute 'createNodeIterator' on 'Document': parameter 1 is not of type 'Node'.",
        ));
    }
    let iter = Rc::new(NodeIter {
        root: root_value.clone(),
        reference: RefCell::new(root_value),
        before: Cell::new(true),
    });
    let global = scope.global();
    let object_ctor = get(scope, &global, "Object");
    let object_proto = get(scope, &object_ctor, "prototype");
    let prototype = object_proto.is_object().then_some(&object_proto);
    let object = scope.new_traced_host_object(prototype, Handle(iter.clone()));
    let js = js_of(scope);
    register(js, &iter);
    init_common(scope, &object, args);
    bind(scope, &object, "nextNode", 0, next_node);
    bind(scope, &object, "previousNode", 0, previous_node);
    bind(scope, &object, "detach", 0, noop);
    define_getter(scope, &object, "root", root);
    define_getter(scope, &object, "referenceNode", reference_node);
    define_getter(scope, &object, "pointerBeforeReferenceNode", pointer_before);
    let _ = scope.define_to_string_tag(&object, "NodeIterator");
    Ok(object)
}

fn last_inclusive(mut node: Element) -> Element {
    while let Some(last) = node.last_child() {
        node = last;
    }
    node
}

fn following_outside(node: Element, root: Option<Element>) -> Option<Element> {
    southstar_dom::ancestors_and_self(node)
        .take_while(|&n| Some(n) != root)
        .find_map(|n| n.next_sibling())
}

pub(crate) fn pre_remove(js: Js, removed: Element) {
    let iters = live_iters(js);
    if iters.is_empty() {
        return;
    }
    main_scope(js, |scope| {
        for it in &iters {
            let root = unwrap_node(&it.root);
            let Some(reference) = it.reference_node() else {
                continue;
            };
            if root.is_some_and(|root| is_inclusive_ancestor(removed, root))
                || !is_inclusive_ancestor(removed, reference)
            {
                continue;
            }
            if it.before.get() {
                if let Some(next) = following_outside(removed, root) {
                    let wrapped = wrap_node(scope, next);
                    it.set_reference(wrapped);
                    continue;
                }
                it.before.set(false);
            }
            let new_reference = match removed.prev_sibling() {
                Some(prev) => Some(last_inclusive(prev)),
                None => removed.parent(),
            };
            let wrapped = new_reference.map_or_else(Value::null, |n| wrap_node(scope, n));
            it.set_reference(wrapped);
        }
    });
}
