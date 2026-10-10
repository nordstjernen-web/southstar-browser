//! Southstar — TreeWalker and the whatToShow/NodeFilter check it shares with NodeIterator.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{unwrap_node, wrap_node};
use crate::{
    Element, JsResult, READ_ONLY, bind, get, hidden_child, put, set_instance_proto, truthy,
};

pub(crate) const ACCEPT: i32 = 1;
pub(crate) const REJECT: i32 = 2;
pub(crate) const SKIP: i32 = 3;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_CDATA: u32 = 1 << 10;
const FLAG_PI: u32 = 1 << 11;

const CURRENT_SLOT: &str = "\u{fffd}cur";

const HIDDEN_SLOT: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

const METHODS: &[(&str, NativeFn)] = &[
    ("parentNode", parent_node),
    ("firstChild", first_child),
    ("lastChild", last_child),
    ("nextSibling", next_sibling),
    ("previousSibling", previous_sibling),
    ("nextNode", next_node),
    ("previousNode", previous_node),
];

fn node_bit(node: Element) -> u32 {
    let flagged = |flag: u32| node.flags() & flag != 0;
    match node.kind() {
        Kind::Element => 0x1,
        Kind::Text if flagged(FLAG_CDATA) => 0x8,
        Kind::Text => 0x4,
        Kind::Comment if flagged(FLAG_PI) => 0x40,
        Kind::Comment => 0x80,
        Kind::Document if flagged(FLAG_FRAGMENT) => 0x400,
        Kind::Document => 0x100,
        Kind::Doctype => 0x200,
        Kind::Other => 0,
    }
}

pub(crate) fn filter(scope: &mut Scope<'_>, object: &Value, node: Element) -> JsResult<i32> {
    let what = get(scope, object, "whatToShow");
    let what = scope.to_int64(&what).unwrap_or(0xFFFF_FFFF);
    if (what as u32) & node_bit(node) == 0 {
        return Ok(SKIP);
    }
    let filter = get(scope, object, "filter");
    let is_function = scope.is_function(&filter);
    if !is_function && !filter.is_object() {
        return Ok(ACCEPT);
    }
    if truthy(scope, object, "_active") {
        return Err(scope.dom_exception("InvalidStateError", "A NodeFilter is already being run."));
    }
    let callback = if is_function {
        filter.clone()
    } else {
        scope.get(&filter, "acceptNode")?
    };
    if !scope.is_function(&callback) {
        return Err(scope.type_error(
            "Failed to execute 'acceptNode' on 'NodeFilter': The provided value is not callable.",
        ));
    }
    put(scope, object, "_active", Value::boolean(true));
    let argument = wrap_node(scope, node);
    let this = if is_function {
        Value::undefined()
    } else {
        filter
    };
    let result = scope.call(&callback, &this, &[argument]);
    put(scope, object, "_active", Value::boolean(false));
    let result = result?;
    scope.to_int32(&result)
}

pub(crate) fn init_common(scope: &mut Scope<'_>, object: &Value, args: &[Value]) {
    let mask = match args.get(1) {
        Some(value) if !value.is_undefined() => scope.to_int64(value).unwrap_or(0),
        _ => 0xFFFF_FFFF,
    };
    let _ = scope.define(
        object,
        "whatToShow",
        Value::int64(i64::from(mask as u32)),
        READ_ONLY,
    );
    let filter = match args.get(2) {
        Some(value) if value.is_object() || scope.is_function(value) => value.clone(),
        _ => Value::null(),
    };
    let _ = scope.define(object, "filter", filter, READ_ONLY);
    put(scope, object, "_active", Value::boolean(false));
}

fn node_prop(scope: &mut Scope<'_>, walker: &Value, name: &str) -> Option<Element> {
    let value = get(scope, walker, name);
    unwrap_node(&value)
}

fn set_current(scope: &mut Scope<'_>, walker: &Value, node: Element) -> Value {
    let wrapped = wrap_node(scope, node);
    put(scope, walker, "currentNode", wrapped);
    wrap_node(scope, node)
}

fn visible(mut node: Option<Element>, backward: bool) -> Option<Element> {
    while let Some(n) = node.filter(|&n| hidden_child(n)) {
        node = if backward {
            n.prev_sibling()
        } else {
            n.next_sibling()
        };
    }
    node
}

fn first(n: Element) -> Option<Element> {
    visible(n.first_child(), false)
}

fn last(n: Element) -> Option<Element> {
    visible(n.last_child(), true)
}

fn next(n: Element) -> Option<Element> {
    visible(n.next_sibling(), false)
}

fn prev(n: Element) -> Option<Element> {
    visible(n.prev_sibling(), true)
}

fn child_toward(n: Element, from_end: bool) -> Option<Element> {
    if from_end { last(n) } else { first(n) }
}

fn sibling_toward(n: Element, backward: bool) -> Option<Element> {
    if backward { prev(n) } else { next(n) }
}

fn traverse_children(
    scope: &mut Scope<'_>,
    walker: &Value,
    current: Element,
    from_end: bool,
) -> JsResult<Option<Element>> {
    let mut node = child_toward(current, from_end);
    while let Some(mut n) = node {
        let result = filter(scope, walker, n)?;
        if result == ACCEPT {
            return Ok(Some(n));
        }
        if result == SKIP
            && let Some(child) = child_toward(n, from_end)
        {
            node = Some(child);
            continue;
        }
        loop {
            if let Some(sibling) = sibling_toward(n, from_end) {
                node = Some(sibling);
                break;
            }
            match n.parent() {
                Some(parent) if parent != current => n = parent,
                _ => return Ok(None),
            }
        }
    }
    Ok(None)
}

fn traverse_siblings(
    scope: &mut Scope<'_>,
    walker: &Value,
    current: Element,
    root: Element,
    backward: bool,
) -> JsResult<Option<Element>> {
    let mut node = current;
    if node == root {
        return Ok(None);
    }
    loop {
        let mut sibling = sibling_toward(node, backward);
        while let Some(s) = sibling {
            node = s;
            let result = filter(scope, walker, node)?;
            if result == ACCEPT {
                return Ok(Some(node));
            }
            sibling = child_toward(node, backward);
            if result == REJECT || sibling.is_none() {
                sibling = sibling_toward(node, backward);
            }
        }
        match node.parent() {
            Some(parent) if parent != root => node = parent,
            _ => return Ok(None),
        }
        if filter(scope, walker, node)? == ACCEPT {
            return Ok(None);
        }
    }
}

fn children_method(scope: &mut Scope<'_>, walker: &Value, from_end: bool) -> JsResult {
    let Some(current) = node_prop(scope, walker, "currentNode") else {
        return Ok(Value::null());
    };
    Ok(match traverse_children(scope, walker, current, from_end)? {
        Some(n) => set_current(scope, walker, n),
        None => Value::null(),
    })
}

fn siblings_method(scope: &mut Scope<'_>, walker: &Value, backward: bool) -> JsResult {
    let current = node_prop(scope, walker, "currentNode");
    let root = node_prop(scope, walker, "root");
    let (Some(current), Some(root)) = (current, root) else {
        return Ok(Value::null());
    };
    Ok(
        match traverse_siblings(scope, walker, current, root, backward)? {
            Some(n) => set_current(scope, walker, n),
            None => Value::null(),
        },
    )
}

fn first_child(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    children_method(scope, this, false)
}

fn last_child(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    children_method(scope, this, true)
}

fn next_sibling(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    siblings_method(scope, this, false)
}

fn previous_sibling(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    siblings_method(scope, this, true)
}

fn parent_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let current = node_prop(scope, this, "currentNode");
    let root = node_prop(scope, this, "root");
    let (Some(mut node), Some(root)) = (current, root) else {
        return Ok(Value::null());
    };
    while node != root {
        let Some(parent) = node.parent() else {
            break;
        };
        node = parent;
        if filter(scope, this, node)? == ACCEPT {
            return Ok(set_current(scope, this, node));
        }
    }
    Ok(Value::null())
}

fn next_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let current = node_prop(scope, this, "currentNode");
    let root = node_prop(scope, this, "root");
    let (Some(mut node), Some(root)) = (current, root) else {
        return Ok(Value::null());
    };
    let mut result = ACCEPT;
    loop {
        while result != REJECT
            && let Some(child) = first(node)
        {
            node = child;
            result = filter(scope, this, node)?;
            if result == ACCEPT {
                return Ok(set_current(scope, this, node));
            }
        }
        let mut following = None;
        let mut temp = Some(node);
        while let Some(t) = temp {
            if t == root {
                return Ok(Value::null());
            }
            if let Some(sibling) = next(t) {
                following = Some(sibling);
                break;
            }
            temp = t.parent();
        }
        let Some(following) = following else {
            return Ok(Value::null());
        };
        node = following;
        result = filter(scope, this, node)?;
        if result == ACCEPT {
            return Ok(set_current(scope, this, node));
        }
    }
}

fn previous_node(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let current = node_prop(scope, this, "currentNode");
    let root = node_prop(scope, this, "root");
    let (Some(mut node), Some(root)) = (current, root) else {
        return Ok(Value::null());
    };
    while node != root {
        let mut sibling = prev(node);
        while let Some(s) = sibling {
            node = s;
            let mut result = filter(scope, this, node)?;
            while result != REJECT
                && let Some(child) = last(node)
            {
                node = child;
                result = filter(scope, this, node)?;
            }
            if result == ACCEPT {
                return Ok(set_current(scope, this, node));
            }
            sibling = prev(node);
        }
        let Some(parent) = node.parent().filter(|_| node != root) else {
            return Ok(Value::null());
        };
        node = parent;
        if filter(scope, this, node)? == ACCEPT {
            return Ok(set_current(scope, this, node));
        }
    }
    Ok(Value::null())
}

fn current_get(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    scope.get(this, CURRENT_SLOT)
}

fn current_set(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = crate::arg(args, 0);
    if unwrap_node(&value).is_none() {
        return Err(scope.type_error(
            "Failed to set the 'currentNode' property on 'TreeWalker': The provided value is not of type 'Node'.",
        ));
    }
    put(scope, this, CURRENT_SLOT, value);
    Ok(Value::undefined())
}

fn define_current(scope: &mut Scope<'_>, walker: &Value, node: &Value) {
    let _ = scope.define(walker, CURRENT_SLOT, node.clone(), HIDDEN_SLOT);
    let getter = scope.function("get", 0, current_get);
    let setter = scope.function("set", 1, current_set);
    let _ = scope.define_accessor(
        walker,
        "currentNode",
        Some(&getter),
        Some(&setter),
        READ_ONLY,
    );
}

pub(crate) fn create(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let root = crate::arg(args, 0);
    if !root.is_object() || unwrap_node(&root).is_none() {
        return Err(scope.type_error(
            "Failed to execute 'createTreeWalker' on 'Document': parameter 1 is not of type 'Node'.",
        ));
    }
    let walker = scope.new_object();
    set_instance_proto(scope, &walker, "TreeWalker");
    let _ = scope.define(&walker, "root", root.clone(), READ_ONLY);
    define_current(scope, &walker, &root);
    init_common(scope, &walker, args);
    let _ = scope.define_to_string_tag(&walker, "TreeWalker");
    for &(name, f) in METHODS {
        bind(scope, &walker, name, 0, f);
    }
    Ok(walker)
}

pub(crate) fn install_proto(scope: &mut Scope<'_>, proto: &Value) {
    if !proto.is_object() {
        return;
    }
    for &(name, f) in METHODS {
        let function = scope.function(name, 0, f);
        let _ = scope.define(proto, name, function, Attributes::METHOD);
    }
}
