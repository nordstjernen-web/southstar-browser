//! Southstar — the native Range object behind document.createRange and the polyfill's geometry proxy.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value, quickjs};

use crate::ffi::{self, js_of, unwrap_node, wrap_node};
use crate::selection::{bounding_client_rect, client_rects, clone_contents};
use crate::{Element, JsResult, bind, get, noop, put};

const FLAG_FRAGMENT: u32 = 1 << 2;
const MAX_DEPTH: i32 = 512;

const NOOP_METHODS: &[&str] = &[
    "setStartBefore",
    "setStartAfter",
    "setEndBefore",
    "setEndAfter",
    "deleteContents",
    "extractContents",
    "insertNode",
    "surroundContents",
    "detach",
    "compareBoundaryPoints",
    "intersectsNode",
    "isPointInRange",
    "comparePoint",
];

fn int_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> i32 {
    let value = get(scope, object, key);
    scope.to_int32(&value).unwrap_or(0)
}

fn set_start(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let Some(container) = args.first() {
        put(scope, this, "startContainer", container.clone());
    }
    if let Some(offset) = args.get(1) {
        put(scope, this, "startOffset", offset.clone());
    }
    Ok(Value::undefined())
}

fn set_end(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if let Some(container) = args.first() {
        put(scope, this, "endContainer", container.clone());
    }
    if let Some(offset) = args.get(1) {
        put(scope, this, "endOffset", offset.clone());
    }
    let start = get(scope, this, "startContainer");
    let end = get(scope, this, "endContainer");
    let same_container = quickjs::identity(&start) == quickjs::identity(&end);
    let start_offset = int_prop(scope, this, "startOffset");
    let end_offset = int_prop(scope, this, "endOffset");
    let collapsed = same_container && start_offset == end_offset;
    put(scope, this, "collapsed", Value::boolean(collapsed));
    Ok(Value::undefined())
}

fn select_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(target) = args.first() else {
        return Ok(Value::undefined());
    };
    let node = unwrap_node(target);
    let parent = scope.get(target, "parentNode")?;
    put(scope, this, "startContainer", parent.clone());
    put(scope, this, "endContainer", parent);
    let index = node
        .and_then(|n| n.parent().map(|p| (n, p)))
        .map_or(0, |(n, p)| {
            southstar_dom::children(p).take_while(|&c| c != n).count() as i32
        });
    put(scope, this, "startOffset", Value::int(index));
    put(scope, this, "endOffset", Value::int(index + 1));
    put(scope, this, "collapsed", Value::boolean(false));
    Ok(Value::undefined())
}

fn select_node_contents(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(target) = args.first() else {
        return Ok(Value::undefined());
    };
    let node = unwrap_node(target);
    put(scope, this, "startContainer", target.clone());
    put(scope, this, "endContainer", target.clone());
    let count = node.map_or(0, |n| match (n.kind(), n.text()) {
        (Kind::Text, Some(text)) => text.to_bytes().len() as i32,
        _ => southstar_dom::children(n).count() as i32,
    });
    put(scope, this, "startOffset", Value::int(0));
    put(scope, this, "endOffset", Value::int(count));
    put(scope, this, "collapsed", Value::boolean(count == 0));
    Ok(Value::undefined())
}

fn collapse(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let to_start = args.first().is_some_and(|v| scope.to_bool(v));
    let (from, to) = if to_start {
        (
            ("startContainer", "startOffset"),
            ("endContainer", "endOffset"),
        )
    } else {
        (
            ("endContainer", "endOffset"),
            ("startContainer", "startOffset"),
        )
    };
    let container = get(scope, this, from.0);
    let offset = get(scope, this, from.1);
    put(scope, this, to.0, container);
    put(scope, this, to.1, offset);
    put(scope, this, "collapsed", Value::boolean(true));
    Ok(Value::undefined())
}

fn clone_range(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let out = make_range_object(scope);
    for key in [
        "startContainer",
        "endContainer",
        "startOffset",
        "endOffset",
        "collapsed",
    ] {
        let value = get(scope, this, key);
        put(scope, &out, key, value);
    }
    Ok(out)
}

fn char_count(text: &[u8]) -> usize {
    text.iter().filter(|&&b| b & 0xC0 != 0x80).count()
}

fn char_offset(text: &[u8], chars: usize) -> usize {
    let mut seen = 0;
    for (index, &byte) in text.iter().enumerate() {
        if byte & 0xC0 != 0x80 {
            if seen == chars {
                return index;
            }
            seen += 1;
        }
    }
    text.len()
}

fn clamp_offset(offset: i32, total: usize) -> usize {
    offset.clamp(0, total.min(i32::MAX as usize) as i32) as usize
}

#[derive(PartialEq, Eq)]
enum Walk {
    Before,
    Inside,
    Done,
}

struct TextWalk {
    start: Element,
    end: Element,
    start_offset: i32,
    end_offset: i32,
    buf: Vec<u8>,
    state: Walk,
}

impl TextWalk {
    fn text_node(&mut self, n: Element) {
        let Some(text) = n.text().map(|t| t.to_bytes()) else {
            return;
        };
        let total = char_count(text);
        if n == self.start && n == self.end {
            let a = clamp_offset(self.start_offset, total);
            let b = clamp_offset(self.end_offset, total);
            if b > a {
                self.buf
                    .extend_from_slice(&text[char_offset(text, a)..char_offset(text, b)]);
            }
            self.state = Walk::Done;
        } else if n == self.start {
            let a = clamp_offset(self.start_offset, total);
            self.buf.extend_from_slice(&text[char_offset(text, a)..]);
            self.state = Walk::Inside;
        } else if n == self.end {
            let b = clamp_offset(self.end_offset, total);
            self.buf.extend_from_slice(&text[..char_offset(text, b)]);
            self.state = Walk::Done;
        } else if self.state == Walk::Inside {
            self.buf.extend_from_slice(text);
        }
    }

    fn walk(&mut self, n: Element, depth: i32) {
        if self.state == Walk::Done || depth >= MAX_DEPTH {
            return;
        }
        if n.kind() == Kind::Text {
            self.text_node(n);
            return;
        }
        let is_start = n == self.start;
        let is_end = n == self.end;
        let mut index = 0;
        for child in southstar_dom::children(n) {
            if is_start && self.state == Walk::Before && index == self.start_offset {
                self.state = Walk::Inside;
            }
            if is_end && index == self.end_offset {
                self.state = Walk::Done;
                return;
            }
            self.walk(child, depth + 1);
            if self.state == Walk::Done {
                return;
            }
            index += 1;
        }
        if is_start && self.state == Walk::Before && self.start_offset >= index {
            self.state = Walk::Inside;
        }
        if is_end && self.end_offset >= index {
            self.state = Walk::Done;
        }
    }
}

fn to_string(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let start_container = get(scope, this, "startContainer");
    let end_container = get(scope, this, "endContainer");
    let Some(start) = unwrap_node(&start_container) else {
        return Ok(scope.string(""));
    };
    let end = unwrap_node(&end_container).unwrap_or(start);
    let start_offset = int_prop(scope, this, "startOffset");
    let end_offset = int_prop(scope, this, "endOffset");

    if start == end && start.kind() == Kind::Text {
        let Some(text) = start.text().map(|t| t.to_bytes()) else {
            return Ok(scope.string(""));
        };
        let total = char_count(text);
        let a = clamp_offset(start_offset, total);
        let b = clamp_offset(end_offset, total);
        if b <= a {
            return Ok(scope.string(""));
        }
        let slice = &text[char_offset(text, a)..char_offset(text, b)];
        return Ok(scope.string_from_bytes(slice));
    }

    let mut walk = TextWalk {
        start,
        end,
        start_offset,
        end_offset,
        buf: Vec::new(),
        state: Walk::Before,
    };
    let root = southstar_dom::ancestors_and_self(start)
        .last()
        .unwrap_or(start);
    walk.walk(root, 0);
    Ok(scope.string_from_bytes(&walk.buf))
}

fn body_of(document: Element) -> Option<Element> {
    let named = |n: &Element, tag: &[u8]| {
        n.kind() == Kind::Element
            && n.name()
                .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(tag))
    };
    southstar_dom::children(document)
        .filter(|c| named(c, b"html"))
        .find_map(|html| southstar_dom::children(html).find(|c| named(c, b"body")))
}

fn create_contextual_fragment(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    let js = js_of(scope);
    let source = match args.first() {
        Some(value) => scope.to_bytes(value).ok(),
        None => None,
    };
    let fragment = Node::alloc(southstar_dom::node::KIND_DOCUMENT);
    fragment.add_flags(FLAG_FRAGMENT);
    if let Some(parsed) = source
        .filter(|s| !s.is_empty() && s[0] != 0)
        .and_then(|s| ffi::parse_html(&s))
    {
        if let Some(body) = body_of(parsed) {
            while let Some(child) = body.first_child() {
                child.detach();
                southstar_dom::node::own_strings_deep(child);
                fragment.append(child);
            }
        }
        parsed.free_tree();
    }
    ffi::track_orphan(js, fragment);
    Ok(wrap_node(scope, fragment))
}

fn make_range_object(scope: &mut Scope<'_>) -> Value {
    let range = scope.new_object();
    put(scope, &range, "collapsed", Value::boolean(true));
    put(scope, &range, "startContainer", Value::null());
    put(scope, &range, "endContainer", Value::null());
    put(scope, &range, "startOffset", Value::int(0));
    put(scope, &range, "endOffset", Value::int(0));
    put(scope, &range, "commonAncestorContainer", Value::null());
    bind(scope, &range, "setStart", 2, set_start);
    bind(scope, &range, "setEnd", 2, set_end);
    bind(scope, &range, "selectNode", 1, select_node);
    bind(scope, &range, "selectNodeContents", 1, select_node_contents);
    bind(scope, &range, "collapse", 1, collapse);
    bind(scope, &range, "cloneRange", 0, clone_range);
    bind(scope, &range, "toString", 0, to_string);
    bind(scope, &range, "cloneContents", 0, clone_contents);
    bind(
        scope,
        &range,
        "getBoundingClientRect",
        0,
        bounding_client_rect,
    );
    bind(scope, &range, "getClientRects", 0, client_rects);
    bind(
        scope,
        &range,
        "createContextualFragment",
        1,
        create_contextual_fragment,
    );
    for name in NOOP_METHODS {
        bind(scope, &range, name, 0, noop);
    }
    range
}

pub(crate) fn create_range(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let global = scope.global();
    let factory = get(scope, &global, "__ndCreateRange");
    if scope.is_function(&factory)
        && let Ok(range) = scope.call(&factory, &Value::undefined(), core::slice::from_ref(this))
    {
        return Ok(range);
    }
    Ok(make_range_object(scope))
}

pub(crate) fn native_range(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(make_range_object(scope))
}
