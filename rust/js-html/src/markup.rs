//! Southstar — innerHTML, outerHTML, getHTML, setHTMLUnsafe, insertAdjacentHTML and XMLSerializer over the lexbor fragment parser and the DOM serializer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::serialize::{self, Options};
use southstar_dom::{FLAG_SCRIPTING_DISABLED, Kind};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::text::{Position, position_error};
use crate::{
    Element, FLAG_XML_DOC, JsResult, children, insert_after, is_document, prepend,
    scripting_enabled,
};

const NO_MODIFICATION_ALLOWED_ERR: i32 = 7;
const MAX_SHADOW_ROOTS: u32 = 4096;

fn markup_arg(scope: &mut Scope<'_>, val: &Value) -> JsResult<Vec<u8>> {
    if val.is_null() {
        Ok(Vec::new())
    } else {
        scope.to_bytes(val)
    }
}

fn outer_markup(n: Element) -> Vec<u8> {
    if n.flags() & FLAG_XML_DOC != 0 {
        serialize::xml_outer_html(Some(n))
    } else {
        serialize::outer_html(Some(n))
    }
}

pub(crate) fn inner_html_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let html = ffi::unwrap_node(this).map_or_else(Vec::new, |n| serialize::get_html(Some(n), None));
    Ok(scope.string_from_bytes(&html))
}

pub(crate) fn outer_html_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let html = ffi::unwrap_node(this).map_or_else(Vec::new, outer_markup);
    Ok(scope.string_from_bytes(&html))
}

fn shadow_root_option(scope: &mut Scope<'_>, options: &Value) -> JsResult<Vec<Element>> {
    let list = scope.get(options, "shadowRoots")?;
    if !list.is_array() {
        return Ok(Vec::new());
    }
    let length = scope.get(&list, "length")?;
    let count = scope.to_number(&length)? as u32;
    if count == 0 || count >= MAX_SHADOW_ROOTS {
        return Ok(Vec::new());
    }
    let mut roots = Vec::new();
    for i in 0..count {
        let entry = scope.get_index(&list, i)?;
        if let Some(root) = ffi::unwrap_node(&entry) {
            roots.push(root);
        }
    }
    Ok(roots)
}

pub(crate) fn get_html(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(scope.string_from_bytes(b""));
    };
    let mut options = Options {
        include_serializable: false,
        roots: Vec::new(),
    };
    if let Some(arg) = args.first().filter(|a| a.is_object()) {
        let serializable = scope.get(arg, "serializableShadowRoots")?;
        options.include_serializable = scope.to_bool(&serializable);
        options.roots = shadow_root_option(scope, arg)?;
    }
    let html = serialize::get_html(Some(n), Some(&options));
    Ok(scope.string_from_bytes(&html))
}

fn adopt_children(fragment: Element) -> Vec<Element> {
    ffi::mark_scripts_already_started(fragment);
    let kids = children(fragment);
    for &kid in &kids {
        southstar_dom::node::own_strings_deep(kid);
    }
    kids
}

fn clear_children_collect(js: Option<Js>, n: Element) -> Vec<Element> {
    let Some(js) = js else {
        ffi::clear_children_unrecorded(n);
        return Vec::new();
    };
    let mut removed = Vec::new();
    while let Some(child) = n.first_child() {
        ffi::ce_disconnect_subtree(js, child);
        child.detach();
        ffi::track_orphan(js, child);
        removed.push(child);
    }
    removed
}

fn set_template_html(js: Option<Js>, template: Element, markup: &[u8], declarative: bool) {
    let content = southstar_dom::node::template_content(template);
    content.add_flags(FLAG_SCRIPTING_DISABLED);
    let Some(fragment) = ffi::parse_fragment_for_tag(c"template", markup, false) else {
        return;
    };
    if declarative {
        ffi::convert_declarative_shadow(fragment);
    }
    while let Some(child) = content.first_child() {
        child.detach();
        ffi::orphan_node(js, child);
    }
    for kid in adopt_children(fragment) {
        content.append(kid);
    }
    fragment.free_tree();
    if let Some(js) = js {
        ffi::mark_mutated(js);
    }
}

fn set_html(scope: &mut Scope<'_>, this: &Value, val: &Value, declarative: bool) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Element) else {
        return Ok(());
    };
    let markup = markup_arg(scope, val)?;
    let js = ffi::js_of(scope);
    if n.element_name() == Some(b"template") {
        set_template_html(js, n, &markup, declarative);
        return Ok(());
    }
    if let Some(fragment) = ffi::parse_fragment_in_context(n, &markup, scripting_enabled(n)) {
        if declarative {
            ffi::convert_declarative_shadow(fragment);
        }
        let removed = clear_children_collect(js, n);
        let added = adopt_children(fragment);
        for &kid in &added {
            n.append(kid);
        }
        if let Some(js) = js {
            ffi::record_child_changes(js, n, &added, &removed, None, None);
        }
        fragment.free_tree();
    }
    if let Some(js) = js {
        ffi::mark_mutated(js);
        if !ffi::in_template_content(n) {
            ffi::ce_upgrade_subtree_all(js, n);
            ffi::run_inserted_scripts(js, n);
        }
    }
    Ok(())
}

pub(crate) fn inner_html_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    set_html(scope, this, val, false)
}

pub(crate) fn set_html_unsafe(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let val = args.first().cloned().unwrap_or_else(Value::undefined);
    set_html(scope, this, &val, true)?;
    Ok(Value::undefined())
}

pub(crate) fn outer_html_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some((target, parent)) = ffi::unwrap_node(this).and_then(|n| n.parent().map(|p| (n, p)))
    else {
        return Ok(());
    };
    let markup = markup_arg(scope, val)?;
    let Some(fragment) = ffi::parse_fragment_in_context(parent, &markup, scripting_enabled(target))
    else {
        return Ok(());
    };
    let previous = target.prev_sibling();
    let next = target.next_sibling();
    let kids = adopt_children(fragment);
    for &kid in &kids {
        kid.detach();
        ffi::insert_sibling_before(target, kid);
    }
    fragment.free_tree();
    let js = ffi::js_of(scope);
    if let Some(js) = js {
        ffi::ce_disconnect_subtree(js, target);
    }
    target.detach();
    if let Some(js) = js {
        ffi::track_orphan(js, target);
        ffi::record_child_changes(js, parent, &kids, &[target], previous, next);
        ffi::mark_mutated(js);
        ffi::run_inserted_scripts(js, parent);
        ffi::ce_upgrade_subtree_all(js, parent);
    }
    Ok(())
}

pub(crate) fn insert_adjacent_html(
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
    let markup = scope.to_bytes(&args[1])?;
    let Some(position) = Position::parse(&position) else {
        return Err(position_error(scope));
    };
    let beside_self = position.beside_self();
    let context = if beside_self {
        match target.parent().filter(|&p| !is_document(p)) {
            Some(parent) => parent,
            None => {
                return Err(ffi::dom_exception(
                    scope,
                    c"NoModificationAllowedError",
                    NO_MODIFICATION_ALLOWED_ERR,
                    c"insertAdjacentHTML: the element has no parent element",
                ));
            }
        }
    } else {
        target
    };
    let scripting = scripting_enabled(target);
    let fragment = if context.element_name() == Some(b"html") {
        ffi::parse_fragment_for_tag(c"body", &markup, scripting)
    } else {
        ffi::parse_fragment_in_context(context, &markup, scripting)
    };
    let Some(fragment) = fragment else {
        return Ok(Value::undefined());
    };
    let js = ffi::js_of(scope);
    let kids = adopt_children(fragment);
    let record = |parent: Element, kid: Element| {
        if let Some(js) = js {
            ffi::record_child_change(
                js,
                parent,
                Some(kid),
                kid.prev_sibling(),
                kid.next_sibling(),
            );
        }
    };
    match position {
        Position::BeforeBegin => {
            for &kid in &kids {
                ffi::insert_sibling_before(target, kid);
                record(context, kid);
            }
        }
        Position::AfterBegin => {
            for &kid in kids.iter().rev() {
                prepend(target, kid);
                record(target, kid);
            }
        }
        Position::BeforeEnd => {
            for &kid in &kids {
                target.append(kid);
                record(target, kid);
            }
        }
        Position::AfterEnd => {
            let mut reference = target;
            for &kid in &kids {
                insert_after(reference, kid);
                record(context, kid);
                reference = kid;
            }
        }
    }
    let upgrade_root = if beside_self {
        target.parent()
    } else {
        Some(target)
    };
    fragment.free_tree();
    if let Some(js) = js {
        ffi::mark_mutated(js);
        if let Some(root) = upgrade_root {
            ffi::ce_upgrade_subtree_all(js, root);
            ffi::run_inserted_scripts(js, root);
        }
    }
    Ok(Value::undefined())
}

fn serialize_to_string(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let html = args
        .first()
        .and_then(ffi::unwrap_node)
        .map_or_else(Vec::new, outer_markup);
    Ok(scope.string_from_bytes(&html))
}

pub(crate) fn xml_serializer_ctor(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> JsResult {
    let serializer = scope.new_object();
    let method = scope.function("serializeToString", 1, serialize_to_string);
    scope.set(&serializer, "serializeToString", method)?;
    Ok(serializer)
}
