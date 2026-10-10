//! Southstar — DOMTokenList over a space-separated attribute: classList, relList, sandbox and the other token lists.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::attrs;
use southstar_js_engine::{Scope, Value};

use crate::{Element, INVALID_CHARACTER_ERR, JsResult, SYNTAX_ERR, c_string, ffi, string_arg};

const LINK_REL_TOKENS: &[&str] = &[
    "alternate",
    "apple-touch-icon",
    "apple-touch-icon-precomposed",
    "canonical",
    "dns-prefetch",
    "expect",
    "icon",
    "manifest",
    "modulepreload",
    "next",
    "preconnect",
    "prefetch",
    "preload",
    "prerender",
    "stylesheet",
];

const HYPERLINK_REL_TOKENS: &[&str] = &["noopener", "noreferrer", "opener"];

const SANDBOX_TOKENS: &[&str] = &[
    "allow-downloads",
    "allow-forms",
    "allow-modals",
    "allow-orientation-lock",
    "allow-pointer-lock",
    "allow-popups",
    "allow-popups-to-escape-sandbox",
    "allow-presentation",
    "allow-same-origin",
    "allow-scripts",
    "allow-top-navigation",
    "allow-top-navigation-by-user-activation",
    "allow-top-navigation-to-custom-protocols",
    "allow-storage-access-by-user-activation",
];

type TokenSet = Vec<Vec<u8>>;

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn tokens(value: &[u8]) -> impl Iterator<Item = &[u8]> {
    value.split(|&c| is_space(c)).filter(|t| !t.is_empty())
}

fn parse(value: Option<&CStr>) -> TokenSet {
    let mut set: TokenSet = Vec::new();
    for token in tokens(value.map_or(&[][..], CStr::to_bytes)) {
        if !set.iter().any(|have| have == token) {
            set.push(token.to_vec());
        }
    }
    set
}

fn position(set: &TokenSet, token: &[u8]) -> Option<usize> {
    set.iter().position(|have| have == token)
}

fn current(node: Element, attr: &CStr) -> Option<&'static CStr> {
    attrs::get(node, attr)
}

fn update(scope: &Scope<'_>, node: Element, attr: &CStr, set: &TokenSet) {
    if current(node, attr).is_none() && set.is_empty() {
        return;
    }
    let joined = set.join(&b' ');
    ffi::set_attr(ffi::js_of(scope), node, attr, &c_string(&joined));
}

fn validate(scope: &mut Scope<'_>, token: &[u8]) -> JsResult<()> {
    if token.is_empty() {
        return Err(ffi::dom_exception(
            scope,
            "SyntaxError",
            SYNTAX_ERR,
            "The token provided must not be empty.",
        ));
    }
    if token.iter().any(|&c| is_space(c)) {
        return Err(ffi::dom_exception(
            scope,
            "InvalidCharacterError",
            INVALID_CHARACTER_ERR,
            "The token provided contains HTML space characters, which are not valid in tokens.",
        ));
    }
    Ok(())
}

fn token_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    string_arg(scope, value).map(|token| token.into_bytes())
}

fn collect(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<TokenSet> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        let token = token_arg(scope, arg)?;
        validate(scope, &token)?;
        out.push(token);
    }
    Ok(out)
}

fn argument_count_error(scope: &mut Scope<'_>, required: usize, present: usize) -> Value {
    let message = match required {
        1 => "1 argument required, but only 0 present".to_owned(),
        _ => format!("{required} arguments required, but only {present} present"),
    };
    scope.type_error(&message)
}

pub(crate) fn indexed_token(node: Element, attr: &CStr, name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() || !name.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let index = name.iter().fold(0usize, |acc, &d| {
        acc.saturating_mul(10).saturating_add(usize::from(d - b'0'))
    });
    parse(current(node, attr)).into_iter().nth(index)
}

pub(crate) fn contains(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::boolean(false));
    };
    let Some(arg) = args.first() else {
        return Ok(Value::boolean(false));
    };
    let token = token_arg(scope, arg)?;
    let value = current(node, attr).map_or(&[][..], CStr::to_bytes);
    Ok(Value::boolean(tokens(value).any(|t| t == token.as_slice())))
}

pub(crate) fn add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::undefined());
    };
    let wanted = collect(scope, args)?;
    let mut set = parse(current(node, attr));
    for token in wanted {
        if position(&set, &token).is_none() {
            set.push(token);
        }
    }
    update(scope, node, attr, &set);
    Ok(Value::undefined())
}

pub(crate) fn remove(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::undefined());
    };
    let unwanted = collect(scope, args)?;
    let mut set = parse(current(node, attr));
    for token in unwanted {
        if let Some(index) = position(&set, &token) {
            set.remove(index);
        }
    }
    update(scope, node, attr, &set);
    Ok(Value::undefined())
}

pub(crate) fn replace(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (node, attr) = ffi::token_list_node(this);
    if args.len() < 2 {
        return Err(argument_count_error(scope, 2, args.len()));
    }
    let Some(node) = node else {
        return Ok(Value::boolean(false));
    };
    let old_token = token_arg(scope, &args[0])?;
    let new_token = token_arg(scope, &args[1])?;
    if old_token.is_empty() || new_token.is_empty() {
        validate(scope, &[])?;
    }
    validate(scope, &old_token)?;
    validate(scope, &new_token)?;
    let mut set = parse(current(node, attr));
    let Some(old_index) = position(&set, &old_token) else {
        return Ok(Value::boolean(false));
    };
    match position(&set, &new_token) {
        Some(new_index) if new_index != old_index => {
            set[old_index.min(new_index)] = new_token;
            set.remove(old_index.max(new_index));
        }
        _ => set[old_index] = new_token,
    }
    update(scope, node, attr, &set);
    Ok(Value::boolean(true))
}

pub(crate) fn get_length(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::int(0));
    };
    Ok(Value::int(parse(current(node, attr)).len() as i32))
}

pub(crate) fn item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::null());
    };
    let Some(arg) = args.first() else {
        return Ok(Value::null());
    };
    let index = scope.to_int32(arg)?;
    let Ok(index) = usize::try_from(index) else {
        return Ok(Value::null());
    };
    Ok(parse(current(node, attr))
        .get(index)
        .map_or_else(Value::null, |token| scope.string_from_bytes(token)))
}

pub(crate) fn supports(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (owner, attr) = ffi::token_list_node(this);
    let Some(arg) = args.first() else {
        return Err(argument_count_error(scope, 1, 0));
    };
    let is_link = owner.is_some_and(|n| n.element_name() == Some(b"link".as_slice()));
    let supported = match attr.to_bytes() {
        b"rel" if is_link => LINK_REL_TOKENS,
        b"rel" => HYPERLINK_REL_TOKENS,
        b"sandbox" => SANDBOX_TOKENS,
        _ => return Err(scope.type_error("DOMTokenList has no supported tokens.")),
    };
    let token = token_arg(scope, arg)?.to_ascii_lowercase();
    Ok(Value::boolean(
        supported.iter().any(|t| t.as_bytes() == token.as_slice()),
    ))
}

pub(crate) fn get_value(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(scope.string(""));
    };
    let value = current(node, attr).map_or(&[][..], CStr::to_bytes);
    Ok(scope.string_from_bytes(value))
}

pub(crate) fn set_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), attr) = ffi::token_list_node(this) else {
        return Ok(Value::undefined());
    };
    let value = string_arg(scope, args.first().unwrap_or(&Value::undefined()))?;
    ffi::set_attr(ffi::js_of(scope), node, attr, &value);
    Ok(Value::undefined())
}

pub(crate) fn toggle(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (node, attr) = ffi::token_list_node(this);
    let Some(arg) = args.first() else {
        return Err(argument_count_error(scope, 1, 0));
    };
    let Some(node) = node else {
        return Ok(Value::boolean(false));
    };
    let token = token_arg(scope, arg)?;
    validate(scope, &token)?;
    let force = match args.get(1) {
        Some(value) if !value.is_undefined() => Some(scope.to_bool(value)),
        _ => None,
    };
    let js = ffi::js_of(scope);
    let value = current(node, attr).filter(|v| !v.is_empty());
    if value.is_none() && force != Some(false) {
        ffi::set_attr(js, node, attr, &c_string(&token));
        return Ok(Value::boolean(true));
    }
    let mut set = parse(value);
    match (position(&set, &token), force) {
        (Some(_), Some(true)) => Ok(Value::boolean(true)),
        (Some(index), _) => {
            set.remove(index);
            update(scope, node, attr, &set);
            Ok(Value::boolean(false))
        }
        (None, Some(false)) => Ok(Value::boolean(false)),
        (None, _) => {
            set.push(token);
            update(scope, node, attr, &set);
            Ok(Value::boolean(true))
        }
    }
}
