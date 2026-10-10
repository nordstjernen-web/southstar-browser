//! Southstar — CSSStyleDeclaration: element.style over the style attribute, and the read-only declaration getComputedStyle returns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::computed::{self, COMPUTED_PROPS};
use crate::ffi::{self, Js};
use crate::page_or_new;

const PROXY_SOURCE: &str = "(function(t){\
 function kebab(k){ if (k.indexOf('-') >= 0) return k;\
  var s = k.replace(/[A-Z]/g, function(m){ return '-' + m.toLowerCase(); });\
  return /^webkit-/.test(s) ? '-' + s : s; }\
 return new Proxy(t, {\
  get: function(o, k) {\
   if (typeof k !== 'string') return Reflect.get(o, k);\
   if (Reflect.has(o, k)) return Reflect.get(o, k);\
   if (/^[0-9]+$/.test(k)) return o.item(+k);\
   var kb = kebab(k);\
   var v = o.getPropertyValue(kb);\
   if ((v == null || v === '') && !__ns_css_supported(kb))\
    return undefined;\
   return v == null ? '' : v;\
  },\
  set: function(o, k, v) { o.setProperty(kebab(String(k)), v); return false; },\
  deleteProperty: function(o, k) { o.removeProperty(kebab(String(k))); return false; },\
  has: function(o, k) {\
   if (typeof k !== 'string') return Reflect.has(o, k);\
   if (Reflect.has(o, k)) return true;\
   if (/^[0-9]+$/.test(k)) return +k < o.length;\
   var kb = kebab(k);\
   return __ns_css_supported(kb) || o.getPropertyValue(kb) !== '';\
  },\
  ownKeys: function(o) {\
   var keys = [];\
   for (var i = 0; i < o.length; i++) keys.push(String(i));\
   var own = Reflect.ownKeys(o);\
   for (var j = 0; j < own.length; j++)\
    if (keys.indexOf(own[j]) < 0) keys.push(own[j]);\
   return keys;\
  },\
  getOwnPropertyDescriptor: function(o, k) {\
   if (typeof k === 'string' && /^[0-9]+$/.test(k) && +k < o.length)\
    return { value: o.item(+k), writable: false, enumerable: true, configurable: true };\
   return Reflect.getOwnPropertyDescriptor(o, k);\
  }\
 });\
})";

fn pseudo_name(raw: &[u8]) -> Option<(String, usize)> {
    let mut colons = 0usize;
    while colons < raw.len() && raw[colons] == b':' && colons < 2 {
        colons += 1;
    }
    if colons == 0 || colons >= raw.len() || raw[colons] == b':' {
        return None;
    }
    let mut name = String::new();
    let mut i = colons;
    while i < raw.len() {
        let c = raw[i];
        if c == b'\\' {
            i += 1;
            if i >= raw.len() {
                return None;
            }
            if raw[i].is_ascii_hexdigit() {
                let mut codepoint: u32 = 0;
                let mut digits = 0;
                while i < raw.len() && digits < 6 && raw[i].is_ascii_hexdigit() {
                    codepoint = codepoint * 16 + (raw[i] as char).to_digit(16).unwrap_or(0);
                    i += 1;
                    digits += 1;
                }
                if i < raw.len() && raw[i].is_ascii_whitespace() {
                    i += 1;
                }
                let ch = char::from_u32(codepoint)
                    .filter(|_| codepoint != 0)
                    .unwrap_or('\u{FFFD}');
                name.push(ch.to_ascii_lowercase());
                continue;
            }
            name.push((raw[i] as char).to_ascii_lowercase());
            i += 1;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
            return None;
        }
        name.push((c as char).to_ascii_lowercase());
        i += 1;
    }
    Some((name, colons))
}

fn pseudo_supported(name: &str, colons: usize) -> bool {
    const LEGACY: [&str; 4] = ["before", "after", "first-line", "first-letter"];
    const MODERN: [&str; 5] = [
        "selection",
        "marker",
        "placeholder",
        "backdrop",
        "file-selector-button",
    ];
    LEGACY.contains(&name) || (colons == 2 && MODERN.contains(&name))
}

fn bind(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    f: NativeFn,
    arity: u32,
) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    scope.set(object, name, function)
}

fn custom_names(js: Js, node: Node<'_>) -> Vec<Vec<u8>> {
    ffi::flush_layout(js);
    ffi::style_of(js, node).map_or_else(Vec::new, |s| s.var_names())
}

fn proxy(scope: &mut Scope<'_>) -> Option<Value> {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    let page = page_or_new(js);
    if let Some(proxy) = page.computed_proxy.borrow().clone() {
        return Some(proxy);
    }
    let helper = scope
        .eval_native_script(PROXY_SOURCE, "<getComputedStyle>")
        .ok()?;
    *page.computed_proxy.borrow_mut() = Some(helper.clone());
    Some(helper)
}

pub(crate) fn get_computed_style(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let cs = scope.new_object();
    let proto = ffi::style_decl_proto(scope);
    if proto.is_object() {
        scope.set_prototype(&cs, &proto)?;
    }
    if let Some(node) = args.first() {
        scope.set(&cs, "_node", node.clone())?;
    }
    let rendered = match args.first() {
        Some(node) => ffi::element_is_rendered(scope, node),
        None => false,
    };
    if !rendered {
        scope.set(&cs, "_empty", Value::boolean(true))?;
    }
    if let Some(pseudo) = args.get(1).filter(|p| p.is_string()) {
        let raw = scope.to_bytes(pseudo).unwrap_or_default();
        if raw.first() == Some(&b':') {
            match pseudo_name(&raw).filter(|(name, colons)| pseudo_supported(name, *colons)) {
                Some((name, _)) => {
                    let name = scope.string(&name);
                    scope.set(&cs, "_pseudo", name)?;
                }
                None => scope.set(&cs, "_empty", Value::boolean(true))?,
            }
        }
    }
    bind(
        scope,
        &cs,
        "getPropertyValue",
        computed_get_property_value,
        1,
    )?;
    bind(
        scope,
        &cs,
        "getPropertyPriority",
        computed_get_property_priority,
        1,
    )?;
    bind(scope, &cs, "setProperty", computed_readonly, 2)?;
    bind(scope, &cs, "removeProperty", computed_readonly, 1)?;
    let empty_value = scope.get(&cs, "_empty")?;
    let empty = scope.to_bool(&empty_value);
    let js = ffi::js_of(scope);
    let custom_count = match args.first().and_then(ffi::unwrap_element) {
        Some(node) if !empty && !js.is_null() => custom_names(js, node).len(),
        _ => 0,
    };
    let length = if empty {
        0
    } else {
        (COMPUTED_PROPS.len() + custom_count) as i32
    };
    scope.define(&cs, "length", Value::int(length), Attributes::CONFIGURABLE)?;
    bind(scope, &cs, "item", computed_item, 1)?;

    if let Some(helper) = proxy(scope)
        && let Ok(wrapped) = scope.call(&helper, &Value::undefined(), core::slice::from_ref(&cs))
    {
        return Ok(wrapped);
    }
    Ok(cs)
}

fn computed_get_property_value(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let Some(name) = args.first() else {
        return Ok(scope.string(""));
    };
    let empty = scope.get(this, "_empty")?;
    if scope.to_bool(&empty) {
        return Ok(scope.string(""));
    }
    let Ok(name) = scope.to_string(name) else {
        return Ok(scope.string(""));
    };
    let node = scope.get(this, "_node")?;
    let Some(node) = ffi::unwrap_element(&node) else {
        return Ok(scope.string(""));
    };
    let pseudo = scope.get(this, "_pseudo")?;
    let pseudo = if pseudo.is_string() {
        scope.to_string(&pseudo).ok()
    } else {
        None
    };
    let js = ffi::js_of(scope);
    let value = match pseudo.as_deref().filter(|p| !p.is_empty()) {
        Some(pseudo) => computed::lookup_pseudo(js, node, pseudo, &name),
        None => computed::lookup(js, node, &name),
    };
    Ok(scope.string(value.as_deref().unwrap_or("")))
}

fn computed_get_property_priority(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    Ok(scope.string(""))
}

fn computed_readonly(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    Err(scope.dom_exception(
        "NoModificationAllowedError",
        "Computed styles are read-only.",
    ))
}

fn computed_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let i = match args.first() {
        Some(i) => scope.to_int32(i).unwrap_or(0),
        None => 0,
    };
    let length = scope.get(this, "length")?;
    let length = scope.to_int32(&length).unwrap_or(0);
    if i < 0 || i >= length {
        return Ok(scope.string(""));
    }
    let index = i as usize;
    if let Some(name) = COMPUTED_PROPS.get(index) {
        return Ok(scope.string(name));
    }
    let node = scope.get(this, "_node")?;
    let js = ffi::js_of(scope);
    let Some(node) = ffi::unwrap_element(&node).filter(|_| !js.is_null()) else {
        return Ok(scope.string(""));
    };
    let names = custom_names(js, node);
    let name = names.get(index - COMPUTED_PROPS.len());
    Ok(scope.string_from_bytes(name.map_or(&b""[..], Vec::as_slice)))
}

pub(crate) fn camel_to_kebab(s: &str) -> String {
    if s == "cssFloat" {
        return "float".into();
    }
    if s.starts_with("--") {
        return s.into();
    }
    let has_upper = s.bytes().any(|b| b.is_ascii_uppercase());
    if !has_upper && !s.starts_with("webkit") {
        return s.into();
    }
    let mut out = String::with_capacity(s.len() + 4);
    if s.starts_with("webkit") && s.as_bytes().get(6).is_some_and(u8::is_ascii_uppercase) {
        out.push('-');
    }
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn is_prototype_name(name: &str) -> bool {
    const RESERVED: [&str; 12] = [
        "cssText",
        "constructor",
        "length",
        "parentRule",
        "cssFloat",
        "item",
        "getPropertyValue",
        "setProperty",
        "removeProperty",
        "getPropertyPriority",
        "getPropertyCSSValue",
        "toString",
    ];
    RESERVED.contains(&name)
}

fn declared_names(node: Node<'_>) -> Vec<String> {
    let style = ffi::inline_style_serialize(node.attr(c"style"));
    let mut names = Vec::new();
    let mut p = style.as_str();
    loop {
        p = p.trim_start_matches([' ', ';']);
        if p.is_empty() {
            break;
        }
        let Some(colon) = p.find(':') else {
            break;
        };
        names.push(p[..colon].trim_end_matches(' ').to_string());
        let after = &p[colon..];
        match after.find(';') {
            Some(end) => p = &after[end..],
            None => break,
        }
    }
    names
}

fn parse_index(name: &str) -> Option<usize> {
    if !name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(name.parse::<usize>().unwrap_or(usize::MAX))
}

fn specified_value(node: Node<'_>, name: &str) -> Option<String> {
    let mut value = ffi::inline_style_get(node.attr(c"style"), name)?;
    ffi::strip_important(&mut value);
    if !name.starts_with("--")
        && let Some(canonical) = ffi::specified_canonical(name, &value)
    {
        value = canonical;
    }
    Some(value)
}

pub(crate) fn own_value(node: Node<'_>, name: &str) -> Option<(String, bool)> {
    if is_prototype_name(name) {
        return None;
    }
    if name.starts_with(|c: char| c.is_ascii_digit())
        && let Some(index) = parse_index(name)
    {
        return declared_names(node)
            .into_iter()
            .nth(index)
            .map(|prop| (prop, false));
    }
    let css = camel_to_kebab(name);
    let custom = css.starts_with("--");
    if !custom
        && css != "css-text"
        && css != "length"
        && css != "css-float"
        && !ffi::named_property_supported(&css)
    {
        return None;
    }
    Some((specified_value(node, &css).unwrap_or_default(), true))
}

pub(crate) fn set_named(scope: &mut Scope<'_>, node: Node<'_>, name: &str, value: &Value) {
    let js = ffi::js_of(scope);
    if name == "cssText" {
        if let Ok(text) = scope.to_bytes(value) {
            ffi::set_attr_recorded(js, node, c"style", &text);
        }
        return;
    }
    let css = camel_to_kebab(name);
    let text = scope.to_string(value).ok();
    if let Some(text) = text.as_deref().filter(|t| !t.is_empty())
        && !ffi::named_declaration_valid(&css, text)
    {
        return;
    }
    let updated = ffi::inline_style_set(node.attr(c"style"), &css, text.as_deref().unwrap_or(""));
    ffi::set_attr_recorded(js, node, c"style", &updated);
}

fn decl_node(this: &Value) -> Option<Node<'static>> {
    ffi::style_decl_node(this)
}

fn get_css_text(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let text = decl_node(this).map_or_else(String::new, |n| {
        ffi::inline_style_serialize(n.attr(c"style"))
    });
    Ok(scope.string(&text))
}

fn set_css_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    if let Some(node) = decl_node(this) {
        let value = args.first().cloned().unwrap_or_else(Value::undefined);
        if let Ok(text) = scope.to_bytes(&value) {
            ffi::set_attr_recorded(ffi::js_of(scope), node, c"style", &text);
        }
    }
    Ok(Value::undefined())
}

fn get_length(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let Some(node) = decl_node(this) else {
        return Ok(Value::int(0));
    };
    let style = ffi::inline_style_serialize(node.attr(c"style"));
    let mut count = 0;
    let mut p = style.as_str();
    loop {
        p = p.trim_start_matches([' ', ';']);
        if p.is_empty() {
            break;
        }
        let Some(colon) = p.find(':') else {
            break;
        };
        count += 1;
        let after = &p[colon..];
        match after.find(';') {
            Some(end) => p = &after[end..],
            None => break,
        }
    }
    Ok(Value::int(count))
}

fn get_null(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    Ok(Value::null())
}

fn call_own(
    scope: &mut Scope<'_>,
    this: &Value,
    method: &str,
    args: &[Value],
) -> Result<Value, Value> {
    let function = scope.get(this, method)?;
    if !scope.is_function(&function) {
        return Ok(Value::undefined());
    }
    scope.call(&function, this, args)
}

fn get_css_float(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let name = scope.string("float");
    call_own(scope, this, "getPropertyValue", &[name])
}

fn set_css_float(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let name = scope.string("float");
    let value = args.first().cloned().unwrap_or_else(Value::undefined);
    call_own(scope, this, "setProperty", &[name, value])?;
    Ok(Value::undefined())
}

fn get_property_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (Some(node), Some(name)) = (decl_node(this), args.first()) else {
        return Ok(scope.string(""));
    };
    let Ok(name) = scope.to_string(name) else {
        return Ok(scope.string(""));
    };
    let value = specified_value(node, &name).unwrap_or_default();
    Ok(scope.string(&value))
}

fn set_property(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (Some(node), [name, value, rest @ ..]) = (decl_node(this), args) else {
        return Ok(Value::undefined());
    };
    let name = scope.to_string(name).ok();
    let value = if value.is_null() {
        None
    } else {
        scope.to_string(value).ok()
    };
    let priority = match rest.first() {
        Some(p) if !p.is_null() && !p.is_undefined() => scope.to_string(p).ok(),
        _ => None,
    };
    let important = priority
        .as_deref()
        .is_some_and(|p| p.eq_ignore_ascii_case("important"));
    let Some(name) = name else {
        return Ok(Value::undefined());
    };
    let css_name = if name.starts_with("--") {
        name.clone()
    } else {
        name.to_ascii_lowercase()
    };
    let priority_valid = priority.as_deref().is_none_or(str::is_empty) || important;
    let value_text = value.as_deref().filter(|v| !v.is_empty());
    let accepted = match value_text {
        None => true,
        Some(v) => priority_valid && ffi::named_declaration_valid(&css_name, v),
    };
    if accepted {
        let stored = match value_text {
            Some(v) if important => format!("{v} !important"),
            _ => value.unwrap_or_default(),
        };
        let updated = ffi::inline_style_set(node.attr(c"style"), &css_name, &stored);
        ffi::set_attr_recorded(ffi::js_of(scope), node, c"style", &updated);
    }
    Ok(Value::undefined())
}

fn remove_property(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let (Some(node), Some(name)) = (decl_node(this), args.first()) else {
        return Ok(scope.string(""));
    };
    let Ok(name) = scope.to_string(name) else {
        return Ok(scope.string(""));
    };
    let style = node.attr(c"style");
    let old = ffi::inline_style_get(style, &name);
    let updated = ffi::inline_style_set(style, &name, "");
    ffi::set_attr_recorded(ffi::js_of(scope), node, c"style", &updated);
    let old = old.map(|mut v| {
        ffi::strip_important(&mut v);
        v
    });
    Ok(scope.string(old.as_deref().unwrap_or("")))
}

fn to_uint32(number: f64) -> u32 {
    if !number.is_finite() {
        return 0;
    }
    number.trunc().rem_euclid(4_294_967_296.0) as u32
}

fn item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(index) = args.first() else {
        return Ok(scope.string(""));
    };
    let index = to_uint32(scope.to_number(index).unwrap_or(0.0));
    let value = scope.get_index(this, index)?;
    if value.is_string() {
        return Ok(value);
    }
    Ok(scope.string(""))
}

fn get_property_priority(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (Some(node), Some(name)) = (decl_node(this), args.first()) else {
        return Ok(scope.string(""));
    };
    let Ok(name) = scope.to_string(name) else {
        return Ok(scope.string(""));
    };
    let important = ffi::inline_style_get(node.attr(c"style"), &name)
        .is_some_and(|mut v| ffi::strip_important(&mut v));
    Ok(scope.string(if important { "important" } else { "" }))
}

fn accessor(
    scope: &mut Scope<'_>,
    proto: &Value,
    name: &str,
    getter: NativeFn,
    setter: Option<NativeFn>,
) -> Result<(), Value> {
    let getter = scope.function(&format!("get {name}"), 0, getter);
    let setter = setter.map(|s| scope.function(&format!("set {name}"), 1, s));
    scope.define_accessor(
        proto,
        name,
        Some(&getter),
        setter.as_ref(),
        Attributes::CONFIGURABLE,
    )
}

fn method(
    scope: &mut Scope<'_>,
    proto: &Value,
    name: &str,
    f: NativeFn,
    arity: u32,
) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    scope.define(proto, name, function, Attributes::METHOD)
}

pub(crate) fn install_style_proto(scope: &mut Scope<'_>, proto: &Value) -> Result<(), Value> {
    accessor(scope, proto, "cssText", get_css_text, Some(set_css_text))?;
    accessor(scope, proto, "length", get_length, None)?;
    accessor(scope, proto, "parentRule", get_null, None)?;
    accessor(scope, proto, "cssFloat", get_css_float, Some(set_css_float))?;
    method(scope, proto, "getPropertyValue", get_property_value, 1)?;
    method(scope, proto, "setProperty", set_property, 2)?;
    method(scope, proto, "removeProperty", remove_property, 1)?;
    method(scope, proto, "item", item, 1)?;
    method(
        scope,
        proto,
        "getPropertyPriority",
        get_property_priority,
        1,
    )
}
