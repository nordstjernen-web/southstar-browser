//! Southstar — the document's string properties: title, dir, the legacy body colours, referrer, readyState, compatMode and the fixed-value getters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::index::find_first_element;
use southstar_js_engine::{Scope, Value};

use crate::ffi;
use crate::{Element, FLAG_QUIRKS, JsResult, document_for, until_nul};

const READY_STATES: [&str; 3] = ["loading", "interactive", "complete"];

pub(crate) fn get_title(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let title = document_for(scope, this).and_then(|doc| find_first_element(doc, c"title"));
    let text = title.map(|t| southstar_dom::serialize::collect_text(Some(t)));
    Ok(scope.string_from_bytes(until_nul(&text.unwrap_or_default())))
}

fn title_element(scope: &Scope<'_>, this: &Value) -> Option<Element> {
    let doc = document_for(scope, this)?;
    if let Some(title) = find_first_element(doc, c"title") {
        return Some(title);
    }
    let head = find_first_element(doc, c"head")?;
    let title = ffi::new_element(b"title");
    ffi::append_child(head, title);
    let js = ffi::js_of(scope);
    if !js.is_null() {
        ffi::record_child_change(js, head, Some(title), None, title.prev_sibling(), None);
    }
    Some(title)
}

pub(crate) fn set_title(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let text = scope.to_bytes(&args[0])?;
    if let Some(title) = title_element(scope, this) {
        let js = ffi::js_of(scope);
        let added = (!text.is_empty()).then(|| ffi::new_text(&text));
        ffi::replace_all_recorded(js, title, added);
        if !js.is_null() {
            ffi::mark_mutated(js);
        }
    }
    Ok(Value::undefined())
}

fn root_element_named(scope: &Scope<'_>, tag: &CStr) -> Option<Element> {
    let doc = ffi::current_document(ffi::js_of(scope))?;
    find_first_element(doc, tag)
}

fn canonical_dir(value: Option<&[u8]>) -> &'static str {
    let Some(value) = value else {
        return "";
    };
    ["ltr", "rtl", "auto"]
        .into_iter()
        .find(|kw| value.eq_ignore_ascii_case(kw.as_bytes()))
        .unwrap_or("")
}

pub(crate) fn get_dir(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let value = root_element_named(scope, c"html").and_then(|h| ffi::attr_bytes(h, c"dir"));
    Ok(scope.string(canonical_dir(value)))
}

pub(crate) fn set_dir(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> JsResult {
    if let Some(html) = root_element_named(scope, c"html") {
        let value = scope.to_bytes(&args[0])?;
        ffi::set_attr_recorded(ffi::js_of(scope), html, c"dir", &value);
    }
    Ok(Value::undefined())
}

fn color_attr(magic: i32) -> &'static CStr {
    match magic {
        0 => c"text",
        1 => c"bgcolor",
        2 => c"link",
        3 => c"vlink",
        4 => c"alink",
        _ => c"",
    }
}

pub(crate) fn get_color(scope: &mut Scope<'_>, magic: i32) -> JsResult {
    let value =
        root_element_named(scope, c"body").and_then(|b| ffi::attr_bytes(b, color_attr(magic)));
    Ok(scope.string_from_bytes(value.unwrap_or_default()))
}

pub(crate) fn set_color(scope: &mut Scope<'_>, value: &Value, magic: i32) -> JsResult {
    let Some(body) = root_element_named(scope, c"body") else {
        return Ok(Value::undefined());
    };
    let text = if value.is_null() {
        Vec::new()
    } else {
        scope.to_bytes(value)?
    };
    ffi::set_attr_recorded(ffi::js_of(scope), body, color_attr(magic), &text);
    Ok(Value::undefined())
}

pub(crate) fn get_referrer(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(scope.string(""));
    }
    let referrer = match document_for(scope, this).and_then(|doc| doc.parent()) {
        Some(frame) => ffi::frame_referrer(js, frame),
        None => ffi::referrer(js),
    };
    Ok(scope.string_from_bytes(&referrer.unwrap_or_default()))
}

pub(crate) fn get_ready_state(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(scope.string(READY_STATES[0]));
    }
    let state = ffi::ready_state(js, ffi::unwrap_node(this));
    let name = usize::try_from(state)
        .ok()
        .and_then(|i| READY_STATES.get(i))
        .unwrap_or(&READY_STATES[0]);
    Ok(scope.string(name))
}

pub(crate) fn get_design_mode(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.string("off"))
}

pub(crate) fn get_last_modified(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let text = ffi::local_now_formatted(c"%m/%d/%Y %H:%M:%S").unwrap_or_default();
    Ok(scope.string_from_bytes(&text))
}

pub(crate) fn get_xml_version(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.string("1.0"))
}

pub(crate) fn get_hidden(_: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(Value::boolean(false))
}

pub(crate) fn get_visibility_state(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    Ok(scope.string("visible"))
}

pub(crate) fn get_compat_mode(scope: &mut Scope<'_>, _: &Value, _: &[Value]) -> JsResult {
    let quirks =
        ffi::current_document(ffi::js_of(scope)).is_some_and(|d| d.flags() & FLAG_QUIRKS != 0);
    Ok(scope.string(if quirks { "BackCompat" } else { "CSS1Compat" }))
}
