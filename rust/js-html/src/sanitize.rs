//! Southstar — the Sanitizer configuration subset used by setHTML and the HTML insertion methods: element and attribute allow and remove lists, comments, data attributes and the safe removal of script-capable markup.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Kind, attrs};
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult, children};

const MAX_LIST: u32 = 4096;

const UNSAFE_ELEMENTS: &[&[u8]] = &[b"script", b"frame", b"iframe", b"object", b"embed", b"use"];
const URL_ATTRIBUTES: &[&[u8]] = &[b"href", b"xlink:href", b"action", b"formaction"];

#[derive(Default)]
pub struct Config {
    elements: Option<Vec<Vec<u8>>>,
    remove_elements: Vec<Vec<u8>>,
    replace_with_children: Vec<Vec<u8>>,
    attributes: Option<Vec<Vec<u8>>>,
    remove_attributes: Vec<Vec<u8>>,
    comments: bool,
    data_attributes: bool,
}

impl Config {
    pub fn allow_all() -> Config {
        Config {
            comments: true,
            data_attributes: true,
            ..Config::default()
        }
    }
}

fn entry_name(scope: &mut Scope<'_>, entry: &Value) -> JsResult<Vec<u8>> {
    let name = if entry.is_object() {
        scope.get(entry, "name")?
    } else {
        entry.clone()
    };
    Ok(scope.to_bytes(&name)?.to_ascii_lowercase())
}

fn name_list(scope: &mut Scope<'_>, dict: &Value, key: &str) -> JsResult<Option<Vec<Vec<u8>>>> {
    let list = scope.get(dict, key)?;
    if list.is_undefined() || list.is_null() || !list.is_object() {
        return Ok(None);
    }
    let length = scope.get(&list, "length")?;
    let count = (scope.to_number(&length)? as u32).min(MAX_LIST);
    let mut names = Vec::new();
    for i in 0..count {
        let entry = scope.get_index(&list, i)?;
        names.push(entry_name(scope, &entry)?);
    }
    Ok(Some(names))
}

fn flag(scope: &mut Scope<'_>, dict: &Value, key: &str) -> JsResult<bool> {
    let v = scope.get(dict, key)?;
    Ok(v.is_undefined() || scope.to_bool(&v))
}

pub fn from_value(scope: &mut Scope<'_>, value: &Value) -> JsResult<Config> {
    if !value.is_object() {
        return Ok(Config::allow_all());
    }
    let dict = match scope.get(value, "get")? {
        getter if scope.is_function(&getter) => scope.call(&getter, value, &[])?,
        _ => value.clone(),
    };
    Ok(Config {
        elements: name_list(scope, &dict, "elements")?,
        remove_elements: name_list(scope, &dict, "removeElements")?.unwrap_or_default(),
        replace_with_children: name_list(scope, &dict, "replaceWithChildrenElements")?
            .unwrap_or_default(),
        attributes: name_list(scope, &dict, "attributes")?,
        remove_attributes: name_list(scope, &dict, "removeAttributes")?.unwrap_or_default(),
        comments: flag(scope, &dict, "comments")?,
        data_attributes: flag(scope, &dict, "dataAttributes")?,
    })
}

fn listed(list: &[Vec<u8>], name: &[u8]) -> bool {
    list.iter().any(|n| n.eq_ignore_ascii_case(name))
}

enum Verdict {
    Keep,
    Remove,
    Unwrap,
}

fn element_verdict(config: &Config, safe: bool, name: &[u8]) -> Verdict {
    if (safe && UNSAFE_ELEMENTS.iter().any(|u| u.eq_ignore_ascii_case(name)))
        || listed(&config.remove_elements, name)
    {
        return Verdict::Remove;
    }
    if listed(&config.replace_with_children, name) {
        return Verdict::Unwrap;
    }
    match &config.elements {
        Some(allowed) if !listed(allowed, name) => Verdict::Remove,
        _ => Verdict::Keep,
    }
}

fn is_javascript_url(value: &[u8]) -> bool {
    let trimmed: Vec<u8> = value
        .iter()
        .copied()
        .skip_while(|c| *c <= b' ')
        .filter(|c| !matches!(c, b'\t' | b'\n' | b'\r'))
        .collect();
    trimmed.len() >= 11 && trimmed[..11].eq_ignore_ascii_case(b"javascript:")
}

fn attribute_removed(config: &Config, safe: bool, name: &[u8], value: &[u8]) -> bool {
    if safe
        && (name.len() > 2 && name[..2].eq_ignore_ascii_case(b"on")
            || URL_ATTRIBUTES.iter().any(|u| u.eq_ignore_ascii_case(name))
                && is_javascript_url(value))
    {
        return true;
    }
    if listed(&config.remove_attributes, name) {
        return true;
    }
    match &config.attributes {
        Some(allowed) => {
            !(listed(allowed, name)
                || config.data_attributes
                    && name.len() > 5
                    && name[..5].eq_ignore_ascii_case(b"data-"))
        }
        None => {
            !config.data_attributes && name.len() > 5 && name[..5].eq_ignore_ascii_case(b"data-")
        }
    }
}

fn sanitize_attributes(el: Element, config: &Config, safe: bool) {
    let doomed: Vec<Vec<u8>> = el
        .attrs()
        .filter_map(|a| {
            let name = a.name()?.to_bytes();
            let value = a.value_bytes().unwrap_or_default();
            attribute_removed(config, safe, name, value).then(|| name.to_vec())
        })
        .collect();
    for name in doomed {
        if let Ok(name) = std::ffi::CString::new(name) {
            attrs::remove(el, &name);
        }
    }
}

pub fn sanitize(root: Element, config: &Config, safe: bool) {
    for child in children(root) {
        match child.kind() {
            Kind::Comment if !config.comments => {
                child.detach();
                child.free_tree();
            }
            Kind::Element => {
                let name = child.element_name().unwrap_or_default().to_vec();
                match element_verdict(config, safe, &name) {
                    Verdict::Remove => {
                        child.detach();
                        child.free_tree();
                    }
                    Verdict::Unwrap => {
                        sanitize(child, config, safe);
                        for grandchild in children(child) {
                            grandchild.detach();
                            crate::ffi::insert_sibling_before(child, grandchild);
                        }
                        child.detach();
                        child.free_tree();
                    }
                    Verdict::Keep => {
                        sanitize_attributes(child, config, safe);
                        if let Some(content) = child.tpl_content() {
                            sanitize(content, config, safe);
                        }
                        sanitize(child, config, safe);
                    }
                }
            }
            Kind::Document => sanitize(child, config, safe),
            _ => {}
        }
    }
}
