//! Southstar — textContent, the text property and insertAdjacentElement/insertAdjacentText.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Kind;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::{
    Element, JsResult, inclusive_ancestor, insert_after, is_document, is_fragment, prepend,
};

const HIERARCHY_REQUEST_ERR: i32 = 3;
const SYNTAX_ERR: i32 = 12;

const DATA_PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn text_content_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if is_document(n) || n.kind() == Kind::Doctype {
        return Ok(Value::null());
    }
    if matches!(n.kind(), Kind::Text | Kind::Comment) {
        return Ok(scope.string_from_bytes(n.text_with_len().unwrap_or_default()));
    }
    let Some(first) = n.first_child() else {
        return Ok(scope.string_from_bytes(b""));
    };
    if n.last_child() == Some(first) && first.kind() == Kind::Text {
        return Ok(scope.string_from_bytes(first.text_with_len().unwrap_or_default()));
    }
    let text = southstar_dom::serialize::collect_all_text(Some(n));
    Ok(scope.string_from_bytes(&text))
}

pub(crate) fn text_content_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    if matches!(n.kind(), Kind::Text | Kind::Comment) {
        return ffi::set_node_value(scope, this, val);
    }
    if n.kind() != Kind::Element && !is_fragment(n) {
        return Ok(());
    }
    let text = if val.is_null() || val.is_undefined() {
        Vec::new()
    } else {
        scope.to_bytes(val)?
    };
    if text.is_empty() && n.first_child().is_none() {
        return Ok(());
    }
    let js = ffi::js_of(scope);
    let added = (!text.is_empty()).then(|| ffi::new_text(&text));
    ffi::replace_all_recorded(js, n, added);
    if let Some(js) = js {
        ffi::mark_mutated(js);
        if added.is_some() {
            ffi::script_needs_prepare(js, n);
        }
    }
    Ok(())
}

fn name_is(n: Element, names: &[&[u8]]) -> bool {
    n.name().is_some_and(|name| {
        names
            .iter()
            .any(|candidate| name.to_bytes().eq_ignore_ascii_case(candidate))
    })
}

pub(crate) fn text_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(n) =
        ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Element && n.name().is_some())
    else {
        return Ok(Value::undefined());
    };
    if name_is(n, &[b"option"]) {
        let text = southstar_dom::select::option_text(n);
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        return Ok(scope.string_from_bytes(&text[..end]));
    }
    if name_is(n, &[b"a"]) {
        return text_content_get(scope, this);
    }
    if name_is(n, &[b"script", b"title"]) {
        let mut text = Vec::new();
        for child in southstar_dom::children(n) {
            if child.kind() == Kind::Text
                && let Some(t) = child.text()
            {
                text.extend_from_slice(t.to_bytes());
            }
        }
        return Ok(scope.string_from_bytes(&text));
    }
    Ok(Value::undefined())
}

pub(crate) fn text_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) =
        ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Element && n.name().is_some())
    else {
        return Ok(());
    };
    if name_is(n, &[b"option", b"a", b"script", b"title"]) {
        return text_content_set(scope, this, val);
    }
    let _ = scope.define(this, "text", val.clone(), DATA_PROPERTY);
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Position {
    BeforeBegin,
    AfterBegin,
    BeforeEnd,
    AfterEnd,
}

impl Position {
    pub(crate) fn parse(text: &[u8]) -> Option<Position> {
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        let text = &text[..end];
        [
            (&b"beforebegin"[..], Position::BeforeBegin),
            (b"afterbegin", Position::AfterBegin),
            (b"beforeend", Position::BeforeEnd),
            (b"afterend", Position::AfterEnd),
        ]
        .into_iter()
        .find(|(name, _)| text.eq_ignore_ascii_case(name))
        .map(|(_, position)| position)
    }

    pub(crate) fn beside_self(self) -> bool {
        matches!(self, Position::BeforeBegin | Position::AfterEnd)
    }
}

pub(crate) fn position_error(scope: &mut Scope<'_>) -> Value {
    ffi::dom_exception(
        scope,
        c"SyntaxError",
        SYNTAX_ERR,
        c"The value provided is not one of 'beforebegin', 'afterbegin', 'beforeend', or 'afterend'.",
    )
}

fn check_adjacent(
    scope: &mut Scope<'_>,
    target: Element,
    position: &[u8],
    is_text: bool,
    inserting: Option<Element>,
) -> JsResult<Option<Position>> {
    let Some(position) = Position::parse(position) else {
        return Err(position_error(scope));
    };
    let parent = if position.beside_self() {
        match target.parent() {
            Some(parent) => parent,
            None => return Ok(None),
        }
    } else {
        target
    };
    if is_document(parent) {
        if is_text {
            return Err(ffi::dom_exception(
                scope,
                c"HierarchyRequestError",
                HIERARCHY_REQUEST_ERR,
                c"Nodes of type 'Text' may not be inserted inside a Document.",
            ));
        }
        let has_other_element = southstar_dom::children(parent)
            .any(|c| c.kind() == Kind::Element && Some(c) != inserting);
        if has_other_element {
            return Err(ffi::dom_exception(
                scope,
                c"HierarchyRequestError",
                HIERARCHY_REQUEST_ERR,
                c"A Document may contain at most one element child.",
            ));
        }
    }
    Ok(Some(position))
}

fn insert_adjacent(target: Element, position: Position, node: Element) -> Option<Element> {
    if inclusive_ancestor(node, target) {
        return None;
    }
    match position {
        Position::BeforeBegin => {
            let parent = target.parent()?;
            ffi::insert_sibling_before(target, node);
            Some(parent)
        }
        Position::AfterBegin => {
            prepend(target, node);
            Some(target)
        }
        Position::BeforeEnd => {
            target.append(node);
            Some(target)
        }
        Position::AfterEnd => {
            let parent = target.parent()?;
            insert_after(target, node);
            Some(parent)
        }
    }
}

fn record_inserted(js: Js, parent: Element, node: Element) {
    ffi::record_child_change(
        js,
        parent,
        Some(node),
        node.prev_sibling(),
        node.next_sibling(),
    );
    ffi::mark_mutated(js);
}

pub(crate) fn insert_adjacent_element(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(target) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if args.len() < 2 {
        return Ok(Value::null());
    }
    let position = scope.to_bytes(&args[0])?;
    let Some(child) = ffi::unwrap_node(&args[1]) else {
        return Ok(Value::null());
    };
    let Some(position) = check_adjacent(scope, target, &position, false, Some(child))? else {
        return Ok(Value::null());
    };
    let js = ffi::js_of(scope);
    if let Some(js) = js {
        ffi::unorphan(js, child);
    }
    let Some(parent) = insert_adjacent(target, position, child) else {
        return Ok(Value::null());
    };
    if let Some(js) = js {
        record_inserted(js, parent, child);
        ffi::run_inserted_scripts(js, child);
        ffi::ce_upgrade_subtree_all(js, child);
    }
    Ok(ffi::wrap_node(scope, child))
}

pub(crate) fn insert_adjacent_text(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(target) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    let position = scope.to_bytes(&args[0])?;
    let Some(position) = check_adjacent(scope, target, &position, true, None)? else {
        return Ok(Value::undefined());
    };
    let text = scope.to_bytes(&args[1])?;
    let node = ffi::new_text(&text);
    let Some(parent) = insert_adjacent(target, position, node) else {
        node.free_tree();
        return Ok(Value::undefined());
    };
    if let Some(js) = ffi::js_of(scope) {
        record_inserted(js, parent, node);
    }
    Ok(Value::undefined())
}
