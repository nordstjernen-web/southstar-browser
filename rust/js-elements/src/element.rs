//! Southstar — the small element getters and setters: isContentEditable, hidden, disabled, labels, form, list, option index, select length, enctype, content and the constant stubs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{MAX_DEPTH, ancestors_and_self, children};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, ffi, named};

const FLAG_SHEET_DISABLED: u32 = 1 << 20;

pub(crate) fn zero(_scope: &mut Scope<'_>, _this: &Value) -> JsResult {
    Ok(Value::int(0))
}

pub(crate) fn null(_scope: &mut Scope<'_>, _this: &Value) -> JsResult {
    Ok(Value::null())
}

pub(crate) fn empty_array(scope: &mut Scope<'_>, _this: &Value) -> JsResult {
    Ok(scope.new_array())
}

pub(crate) fn ignore(_scope: &mut Scope<'_>, _this: &Value, _value: &Value) -> JsResult<()> {
    Ok(())
}

pub(crate) fn is_content_editable(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::boolean(false));
    };
    for current in ancestors_and_self(node) {
        let Some(value) = current.attr(c"contenteditable") else {
            continue;
        };
        let value = value.to_bytes();
        if value.eq_ignore_ascii_case(b"false") {
            return Ok(Value::boolean(false));
        }
        if value.is_empty()
            || value.eq_ignore_ascii_case(b"true")
            || value.eq_ignore_ascii_case(b"plaintext-only")
        {
            return Ok(Value::boolean(true));
        }
    }
    Ok(Value::boolean(false))
}

pub(crate) fn labels(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    if !ffi::is_labelable(ffi::unwrap_node(this)) {
        return Ok(Value::null());
    }
    ffi::make_labels(scope, this)
}

pub(crate) fn build_labels(scope: &mut Scope<'_>, control: Option<Element>) -> JsResult {
    fn walk(scan: Element, control: Option<Element>, out: &mut Vec<Element>, depth: i32) {
        if depth >= MAX_DEPTH {
            return;
        }
        if named(Some(scan), b"label") && ffi::label_associated_control(scan) == control {
            out.push(scan);
        }
        for child in children(scan) {
            walk(child, control, out, depth + 1);
        }
    }
    let mut found = Vec::new();
    if let Some(node) = control {
        walk(node.root(), control, &mut found, 0);
    }
    crate::wrap_all(scope, found.into_iter())
}

pub(crate) fn form_enctype(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let value = ffi::unwrap_node(this).and_then(|n| n.attr(c"enctype"));
    let normalized = ffi::enum_normalize(c"enctype", value);
    Ok(scope.string_from_bytes(normalized.to_bytes()))
}

pub(crate) fn set_form_enctype(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let text = scope.to_bytes(value)?;
    ffi::set_attr_recorded_bytes(ffi::js_of(scope), node, c"enctype", &text);
    Ok(())
}

pub(crate) fn hidden(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(value) = ffi::unwrap_node(this).and_then(|n| n.attr(c"hidden")) else {
        return Ok(Value::boolean(false));
    };
    if value.to_bytes().eq_ignore_ascii_case(b"until-found") {
        return Ok(scope.string("until-found"));
    }
    Ok(Value::boolean(true))
}

pub(crate) fn set_hidden(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let js = ffi::js_of(scope);
    if value.is_string() {
        let text = scope.to_bytes(value)?;
        if text.eq_ignore_ascii_case(b"until-found") {
            ffi::set_attr_recorded(js, node, c"hidden", c"until-found");
        } else if text.is_empty() {
            ffi::remove_attr_recorded(js, node, c"hidden");
        } else {
            ffi::set_attr_recorded(js, node, c"hidden", c"");
        }
    } else if scope.to_bool(value) {
        ffi::set_attr_recorded(js, node, c"hidden", c"");
    } else {
        ffi::remove_attr_recorded(js, node, c"hidden");
    }
    Ok(())
}

pub(crate) fn disabled(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::boolean(false));
    };
    if named(Some(node), b"style") {
        return Ok(Value::boolean(node.flags() & FLAG_SHEET_DISABLED != 0));
    }
    Ok(Value::boolean(node.attr(c"disabled").is_some()))
}

pub(crate) fn set_disabled(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let js = ffi::js_of(scope);
    let disabled = scope.to_bool(value);
    if named(Some(node), b"style") {
        let was_disabled = node.flags() & FLAG_SHEET_DISABLED != 0;
        if disabled {
            node.add_flags(FLAG_SHEET_DISABLED);
        } else {
            node.remove_flags(FLAG_SHEET_DISABLED);
        }
        if was_disabled != disabled
            && let Some(js) = js
        {
            ffi::mark_mutated(js);
        }
        return Ok(());
    }
    if disabled {
        ffi::set_attr_recorded(js, node, c"disabled", c"");
    } else {
        ffi::remove_attr_recorded(js, node, c"disabled");
    }
    Ok(())
}

fn options(select: Element) -> impl Iterator<Item = Element> {
    children(select).flat_map(|child| {
        let direct = named(Some(child), b"option").then_some(child);
        let grouped = named(Some(child), b"optgroup")
            .then(|| children(child).filter(|&c| named(Some(c), b"option")))
            .into_iter()
            .flatten();
        direct.into_iter().chain(grouped)
    })
}

pub(crate) fn option_index(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(option) = ffi::unwrap_node(this) else {
        return Ok(Value::int(-1));
    };
    let select = southstar_dom::ancestors(option).find(|&p| named(Some(p), b"select"));
    let index = select
        .and_then(|select| options(select).position(|o| o == option))
        .map_or(-1, |i| i as i32);
    Ok(Value::int(index))
}

pub(crate) fn select_length(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let count = ffi::unwrap_node(this)
        .filter(|&n| named(Some(n), b"select"))
        .map_or(0, |select| options(select).count() as i32);
    Ok(Value::int(count))
}

pub(crate) fn set_list(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    if ffi::is_custom_element(ffi::unwrap_node(this)) {
        let attributes = Attributes {
            writable: true,
            enumerable: true,
            configurable: true,
        };
        let _ = scope.define(this, "list", value.clone(), attributes);
    }
    Ok(())
}

pub(crate) fn list(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if ffi::is_custom_element(Some(node)) {
        return Ok(Value::undefined());
    }
    let Some(id) = node.attr(c"list").filter(|id| !id.is_empty()) else {
        return Ok(Value::null());
    };
    let Some(doc) = ffi::js_of(scope).and_then(ffi::current_document) else {
        return Ok(Value::null());
    };
    let found = southstar_dom::index::find_by_id(doc, id).filter(|&found| {
        found
            .name()
            .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"datalist"))
    });
    match found {
        Some(datalist) => Ok(ffi::make_node(scope, Some(datalist))),
        None => Ok(Value::null()),
    }
}

pub(crate) fn form(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let mut node = ffi::unwrap_node(this);
    if named(node, b"label") {
        node = node.and_then(ffi::label_associated_control);
    }
    match node.and_then(southstar_dom::controls::form_owner) {
        Some(owner) => Ok(ffi::make_node(scope, Some(owner))),
        None => Ok(Value::null()),
    }
}

pub(crate) fn set_content(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    if named(ffi::unwrap_node(this), b"template") {
        return Ok(());
    }
    ffi::reflect_str_set(scope, this, value, c"content").map(drop)
}
