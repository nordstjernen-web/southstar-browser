//! Southstar — the CSS namespace (supports, escape, registerProperty) and the window helpers the CSSOM polyfill calls for style sheets, adopted sheets and animations.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::attrs;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::animations;
use crate::declaration;
use crate::ffi::{self, RegisterStatus};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

fn bind(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    f: NativeFn,
    arity: u32,
) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    if name.starts_with("__") {
        scope.define(object, name, function, Attributes::METHOD)
    } else {
        scope.set(object, name, function)
    }
}

fn hidden(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    f: NativeFn,
    arity: u32,
) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    scope.define(object, name, function, HIDDEN)
}

pub(crate) fn install_window(scope: &mut Scope<'_>, global: &Value) -> Result<(), Value> {
    bind(scope, global, "__ndMediaListSerialize", media_serialize, 1)?;
    bind(scope, global, "__ndAdoptCss", adopt_css, 2)?;
    bind(
        scope,
        global,
        "getComputedStyle",
        declaration::get_computed_style,
        1,
    )?;
    hidden(scope, global, "__ns_css_supported", supported_property, 1)?;
    hidden(scope, global, "__ns_linked_css", linked_css_text, 1)?;
    hidden(scope, global, "__ns_anim_list", animations::list, 1)?;
    hidden(scope, global, "__ns_anim_query", animations::query, 2)?;
    hidden(scope, global, "__ns_anim_seek", animations::seek, 3)?;
    hidden(scope, global, "__ns_anim_control", animations::control, 3)?;
    hidden(scope, global, "__ns_anim_animate", animations::animate, 3)?;
    hidden(
        scope,
        global,
        "__ns_anim_keyframes",
        animations::keyframes,
        2,
    )?;
    hidden(
        scope,
        global,
        "__ns_anim_base_value",
        animations::base_value,
        2,
    )?;
    hidden(
        scope,
        global,
        "__ns_container_query_canonical",
        container_query_canonical,
        1,
    )
}

pub(crate) fn install_css(scope: &mut Scope<'_>, global: &Value) -> Result<(), Value> {
    let css = scope.new_object();
    bind(scope, &css, "supports", supports, 2)?;
    bind(scope, &css, "escape", escape, 1)?;
    bind(scope, &css, "registerProperty", register_property, 1)?;
    bind(
        scope,
        global,
        "__ns_property_rule_valid",
        property_rule_valid,
        3,
    )?;
    scope.define_to_string_tag(&css, "CSS")?;
    scope.set(global, "CSS", css)
}

fn optional_string(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    scope.to_string(value).ok()
}

fn supports(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let result = match args {
        [] => false,
        [condition] => {
            optional_string(scope, condition).is_some_and(|c| ffi::supports_condition(&c))
        }
        [prop, value, ..] => {
            let prop = optional_string(scope, prop);
            let value = optional_string(scope, value);
            ffi::supports_declaration(prop.as_deref(), value.as_deref())
        }
    };
    Ok(Value::boolean(result))
}

fn property_rule_valid(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let [syntax, initial, inherits, ..] = args else {
        return Ok(Value::boolean(false));
    };
    let syntax = if syntax.is_null() {
        None
    } else {
        optional_string(scope, syntax)
    };
    let initial = if initial.is_null() {
        None
    } else {
        optional_string(scope, initial)
    };
    let has_inherits = scope.to_bool(inherits);
    Ok(Value::boolean(ffi::property_rule_valid(
        syntax.as_deref(),
        initial.as_deref(),
        has_inherits,
    )))
}

fn register_property(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(definition) = args.first().filter(|d| d.is_object()) else {
        return Err(scope.type_error("registerProperty: argument 1 is not a dictionary"));
    };
    let name = scope.get(definition, "name")?;
    if name.is_undefined() {
        return Err(scope.type_error("registerProperty: 'name' is a required member"));
    }
    let inherits = scope.get(definition, "inherits")?;
    if inherits.is_undefined() {
        return Err(scope.type_error("registerProperty: 'inherits' is a required member"));
    }
    let inherits = scope.to_bool(&inherits);
    let syntax = scope.get(definition, "syntax")?;
    let initial = scope.get(definition, "initialValue")?;
    let name = optional_string(scope, &name);
    let syntax = if syntax.is_undefined() {
        None
    } else {
        optional_string(scope, &syntax)
    };
    let initial = if initial.is_undefined() {
        None
    } else {
        Some(optional_string(scope, &initial))
    };
    let has_initial = initial.is_some();
    let initial = initial.flatten();
    let status = ffi::register_property(
        name.as_deref(),
        syntax.as_deref().unwrap_or("*"),
        inherits,
        initial.as_deref(),
        has_initial,
    );
    let (kind, message) = match status {
        RegisterStatus::Ok => {
            ffi::mark_mutated(ffi::js_of(scope));
            return Ok(Value::undefined());
        }
        RegisterStatus::Exists => (
            "InvalidModificationError",
            "registerProperty: the property is already registered",
        ),
        RegisterStatus::BadName => (
            "SyntaxError",
            "registerProperty: 'name' is not a custom property name",
        ),
        RegisterStatus::BadSyntax => (
            "SyntaxError",
            "registerProperty: 'syntax' is not a valid syntax descriptor",
        ),
        RegisterStatus::BadInitial => (
            "SyntaxError",
            "registerProperty: 'initialValue' does not match the syntax",
        ),
    };
    Err(scope.dom_exception(kind, message))
}

fn escape(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(arg) = args.first() else {
        return Err(scope.type_error("CSS.escape: 1 argument required"));
    };
    let s = scope.to_bytes(arg)?;
    let simple = s
        .first()
        .is_some_and(|&c| c.is_ascii_alphabetic() || c == b'_')
        && s[1..]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    if simple {
        return Ok(arg.clone());
    }
    let first_is_dash = s.first() == Some(&b'-');
    let mut out: Vec<u8> = Vec::with_capacity(s.len() + 8);
    let mut cp = 0usize;
    for &c in &s {
        if c & 0xC0 != 0x80 {
            cp += 1;
        }
        let pos = cp.wrapping_sub(1);
        let digit = c.is_ascii_digit();
        if c == 0 {
            out.extend_from_slice("\u{FFFD}".as_bytes());
        } else if c <= 0x1F || c == 0x7F || (digit && (pos == 0 || (pos == 1 && first_is_dash))) {
            out.extend_from_slice(format!("\\{c:x} ").as_bytes());
        } else if c == b'-' && pos == 0 && s.len() == 1 {
            out.extend_from_slice(b"\\-");
        } else if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c >= 0x80 {
            out.push(c);
        } else {
            out.push(b'\\');
            out.push(c);
        }
    }
    Ok(scope.string_from_bytes(&out))
}

fn supported_property(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let Some(name) = args.first().and_then(|n| optional_string(scope, n)) else {
        return Ok(Value::boolean(false));
    };
    Ok(Value::boolean(
        name.starts_with("--") || ffi::prop_id(&name).is_some(),
    ))
}

fn container_query_canonical(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let canonical = args
        .first()
        .and_then(|t| optional_string(scope, t))
        .and_then(|t| ffi::container_condition_canonical(&t));
    Ok(match canonical {
        Some(c) => scope.string(&c),
        None => Value::null(),
    })
}

fn linked_css_text(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let text = args
        .first()
        .and_then(|u| optional_string(scope, u))
        .and_then(|u| ffi::linked_css_text(&u))
        .unwrap_or_default();
    Ok(scope.string(&text))
}

fn adopt_css(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let [root, css, ..] = args else {
        return Ok(Value::undefined());
    };
    if !ffi::host_access(scope) {
        return Ok(Value::undefined());
    }
    let Some(root) = ffi::unwrap_element(root)
        .filter(|r| r.is_element() && r.attr(c"data-nd-shadow-root").is_some())
    else {
        return Ok(Value::undefined());
    };
    let css = scope.to_bytes(css)?;
    if css.is_empty() {
        attrs::remove(root, c"data-nd-adopted-css");
    } else {
        attrs::set_len(root, c"data-nd-adopted-css", Some(&css), css.len());
    }
    ffi::mark_mutated(ffi::js_of(scope));
    Ok(Value::undefined())
}

fn media_serialize(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let query = args.first().and_then(|q| optional_string(scope, q));
    let serialized = ffi::media_list_serialize(query.as_deref());
    Ok(scope.string(&serialized))
}
