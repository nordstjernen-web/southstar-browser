//! Southstar — the native, ICU-free ECMA-402 Intl API, installed through the engine-neutral JavaScript layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod collator;
mod datetime;
mod ffi;
mod locale;
mod number;
mod printf;
mod text;

use southstar_js_engine::{Attributes, BoundFn, NativeFn, Scope, Value};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

type Method = (&'static str, NativeFn, u32);

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&c| c == 0)
        .map_or(bytes, |nul| &bytes[..nul])
}

fn c_string(scope: &mut Scope<'_>, value: &Value) -> Result<Vec<u8>, Value> {
    scope
        .to_bytes(value)
        .map(|bytes| until_nul(&bytes).to_vec())
}

fn text(scope: &mut Scope<'_>, bytes: &[u8]) -> Value {
    scope.string_from_bytes(bytes)
}

fn number_arg(scope: &mut Scope<'_>, args: &[Value], index: usize, absent: f64) -> f64 {
    match args.get(index) {
        Some(value) => scope.to_number(value).unwrap_or(f64::NAN),
        None => absent,
    }
}

fn c_trunc_i64(d: f64) -> i64 {
    if cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        && !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&d)
    {
        return i64::MIN;
    }
    d as i64
}

fn c_trunc_i32(d: f64) -> i32 {
    if cfg!(any(target_arch = "x86", target_arch = "x86_64"))
        && !(d > -2_147_483_649.0 && d < 2_147_483_648.0)
    {
        return i32::MIN;
    }
    d as i32
}

fn c_long_trunc(d: f64) -> i64 {
    if cfg!(windows) {
        i64::from(c_trunc_i32(d))
    } else {
        c_trunc_i64(d)
    }
}

fn norm_locale(locale: &[u8]) -> Option<Vec<u8>> {
    (!locale.is_empty()).then(|| {
        locale
            .iter()
            .map(|&c| if c == b'_' { b'-' } else { c })
            .collect()
    })
}

fn default_locale() -> Vec<u8> {
    if let Some(locale) = southstar_i18n::language().and_then(|tag| norm_locale(tag.to_bytes())) {
        return locale;
    }
    for name in ffi::language_names() {
        if name == b"C" || name == b"POSIX" || name.contains(&b'.') || name.contains(&b'@') {
            continue;
        }
        if let Some(locale) = norm_locale(&name) {
            return locale;
        }
    }
    b"en-US".to_vec()
}

fn arg_locale(scope: &mut Scope<'_>, value: &Value) -> Vec<u8> {
    if value.is_string() {
        let locale = c_string(scope, value).ok().and_then(|s| norm_locale(&s));
        return locale.unwrap_or_else(default_locale);
    }
    if value.is_array()
        && let Ok(first) = scope.get_index(value, 0)
        && first.is_string()
    {
        return arg_locale(scope, &first);
    }
    default_locale()
}

fn lang_subtag(locale: &[u8]) -> Vec<u8> {
    locale
        .iter()
        .take_while(|&&c| c != b'-')
        .take(15)
        .map(u8::to_ascii_lowercase)
        .collect()
}

fn lang_in(locale: &[u8], langs: &[&[u8]]) -> bool {
    let lang = lang_subtag(locale);
    langs.iter().any(|&known| known == lang)
}

fn hide(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.define(object, key, value, HIDDEN);
}

fn hide_text(scope: &mut Scope<'_>, object: &Value, key: &str, value: &[u8]) {
    let value = text(scope, value);
    hide(scope, object, key, value);
}

fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

fn set_text(scope: &mut Scope<'_>, object: &Value, key: &str, value: &[u8]) {
    let value = text(scope, value);
    set(scope, object, key, value);
}

fn string_value(scope: &mut Scope<'_>, value: Result<Value, Value>) -> Option<Vec<u8>> {
    let value = value.ok().filter(Value::is_string)?;
    c_string(scope, &value).ok()
}

fn hget_str(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = scope.get(object, key);
    string_value(scope, value)
}

fn hget_int(scope: &mut Scope<'_>, object: &Value, key: &str, default: i32) -> i32 {
    match scope.get(object, key) {
        Ok(value) if !value.is_undefined() && !value.is_null() => {
            scope.to_int32(&value).unwrap_or(default)
        }
        _ => default,
    }
}

fn opt_str(scope: &mut Scope<'_>, options: &Value, key: &str) -> Option<Vec<u8>> {
    if !options.is_object() {
        return None;
    }
    hget_str(scope, options, key)
}

fn opt_present(scope: &mut Scope<'_>, options: &Value, key: &str) -> bool {
    if !options.is_object() {
        return false;
    }
    scope
        .get(options, key)
        .map_or(true, |value| !value.is_undefined() && !value.is_null())
}

fn opt_num(scope: &mut Scope<'_>, options: &Value, key: &str) -> Option<f64> {
    if !options.is_object() {
        return None;
    }
    match scope.get(options, key) {
        Ok(value) if !value.is_undefined() && !value.is_null() => scope.to_number(&value).ok(),
        _ => None,
    }
}

fn opt_bool(scope: &mut Scope<'_>, options: &Value, key: &str, default: i32) -> i32 {
    if !options.is_object() {
        return default;
    }
    match scope.get(options, key) {
        Ok(value) if value.is_undefined() || value.is_null() => default,
        Ok(value) => i32::from(scope.to_bool(&value)),
        Err(_) => 1,
    }
}

fn object_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Value> {
    scope.get(object, key).ok().filter(Value::is_object)
}

fn instance_proto(scope: &mut Scope<'_>, this: &Value, service: &str) -> Option<Value> {
    if let Some(proto) = object_property(scope, this, "prototype") {
        return Some(proto);
    }
    if let Ok(constructor) = scope.get(this, service)
        && let Some(proto) = object_property(scope, &constructor, "prototype")
    {
        return Some(proto);
    }
    let global = scope.global();
    let constructor = scope
        .get(&global, "Intl")
        .and_then(|intl| scope.get(&intl, service));
    constructor
        .ok()
        .and_then(|constructor| object_property(scope, &constructor, "prototype"))
}

fn new_instance(scope: &mut Scope<'_>, this: &Value, service: &str) -> Value {
    match instance_proto(scope, this, service) {
        Some(proto) => scope.new_object_with_proto(&proto),
        None => scope.new_object(),
    }
}

fn part(scope: &mut Scope<'_>, kind: &[u8], value: &[u8]) -> Value {
    let part = scope.new_object();
    set_text(scope, &part, "type", kind);
    set_text(scope, &part, "value", value);
    part
}

struct Parts {
    array: Value,
    count: u32,
}

impl Parts {
    fn new(scope: &mut Scope<'_>) -> Parts {
        Parts {
            array: scope.new_array(),
            count: 0,
        }
    }

    fn push_value(&mut self, scope: &mut Scope<'_>, value: Value) {
        let _ = scope.set_index(&self.array, self.count, value);
        self.count += 1;
    }

    fn push(&mut self, scope: &mut Scope<'_>, kind: &[u8], value: &[u8]) {
        let value = part(scope, kind, value);
        self.push_value(scope, value);
    }
}

fn length(scope: &mut Scope<'_>, object: &Value) -> u32 {
    scope
        .get(object, "length")
        .and_then(|length| scope.to_int32(&length))
        .map_or(0, |length| length as u32)
}

fn join_parts(scope: &mut Scope<'_>, parts: &Value) -> Value {
    let mut joined = Vec::new();
    for i in 0..length(scope, parts) {
        let value = scope
            .get_index(parts, i)
            .and_then(|part| scope.get(&part, "value"));
        if let Ok(value) = value
            && let Ok(piece) = c_string(scope, &value)
        {
            joined.extend_from_slice(&piece);
        }
    }
    text(scope, &joined)
}

fn join_range(scope: &mut Scope<'_>, a: &Value, b: &Value) -> Value {
    let a = c_string(scope, a).unwrap_or_default();
    let b = c_string(scope, b).unwrap_or_default();
    let joined = [&a[..], "\u{2013}".as_bytes(), &b[..]].concat();
    text(scope, &joined)
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn, arity: u32) {
    let function = scope.function(name, arity, f);
    let _ = scope.define(object, name, function, Attributes::METHOD);
}

fn bind_bound(scope: &mut Scope<'_>, object: &Value, name: &str, f: BoundFn, arity: u32) {
    let function = scope.bound_function("", arity, f, std::slice::from_ref(object));
    let _ = scope.define(object, name, function, Attributes::METHOD);
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let mut out = Parts::new(scope);
    let Some(locales) = args.first() else {
        return Ok(out.array);
    };
    if locales.is_string() {
        out.push_value(scope, locales.clone());
    } else if locales.is_array() {
        for i in 0..length(scope, locales) {
            if let Ok(entry) = scope.get_index(locales, i)
                && entry.is_string()
            {
                out.push_value(scope, entry);
            }
        }
    }
    Ok(out.array)
}

fn register(
    scope: &mut Scope<'_>,
    intl: &Value,
    name: &str,
    constructor: NativeFn,
    arity: u32,
    methods: &[Method],
) {
    let function = scope.constructor(name, arity, constructor);
    let proto = scope.new_object();
    for &(method, f, method_arity) in methods {
        bind(scope, &proto, method, f, method_arity);
    }
    let _ = scope.define_to_string_tag(&proto, &format!("Intl.{name}"));
    let _ = scope.set_constructor(&function, &proto);
    bind(
        scope,
        &function,
        "supportedLocalesOf",
        supported_locales_of,
        1,
    );
    let _ = scope.define(intl, name, function, Attributes::METHOD);
}

fn global_constructor(scope: &mut Scope<'_>, name: &str) -> Result<Value, Value> {
    let global = scope.global();
    let intl = scope.get(&global, "Intl")?;
    scope.get(&intl, name)
}

fn construct_global(scope: &mut Scope<'_>, name: &str, args: &[Value]) -> Value {
    global_constructor(scope, name)
        .and_then(|constructor| scope.construct(&constructor, args))
        .unwrap_or_else(|_| Value::undefined())
}

fn number_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let number = scope.to_number(this).unwrap_or(f64::NAN);
    let format = construct_global(scope, "NumberFormat", &[arg(args, 0), arg(args, 1)]);
    let parts = number::parts(scope, &format, number);
    Ok(join_parts(scope, &parts))
}

fn date_options(scope: &mut Scope<'_>, args: &[Value], defaults: &[(&str, &[u8])]) -> Value {
    match args.get(1) {
        Some(options) if options.is_object() => options.clone(),
        _ => {
            let options = scope.new_object();
            for &(key, value) in defaults {
                set_text(scope, &options, key, value);
            }
            options
        }
    }
}

fn date_format_with(scope: &mut Scope<'_>, this: &Value, locales: Value, options: Value) -> Value {
    let ms = scope.to_number(this).unwrap_or(f64::NAN);
    let format = construct_global(scope, "DateTimeFormat", &[locales, options]);
    let parts = datetime::parts(scope, &format, ms);
    join_parts(scope, &parts)
}

const DATE_DEFAULTS: [(&str, &[u8]); 3] = [
    ("year", b"numeric"),
    ("month", b"numeric"),
    ("day", b"numeric"),
];

const TIME_DEFAULTS: [(&str, &[u8]); 3] = [
    ("hour", b"numeric"),
    ("minute", b"2-digit"),
    ("second", b"2-digit"),
];

fn date_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let defaults = [&DATE_DEFAULTS[..], &TIME_DEFAULTS[..]].concat();
    let options = date_options(scope, args, &defaults);
    Ok(date_format_with(scope, this, arg(args, 0), options))
}

fn date_to_locale_date_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let options = date_options(scope, args, &DATE_DEFAULTS);
    Ok(date_format_with(scope, this, arg(args, 0), options))
}

fn date_to_locale_time_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let options = date_options(scope, args, &TIME_DEFAULTS);
    Ok(date_format_with(scope, this, arg(args, 0), options))
}

fn install_proto_hook(
    scope: &mut Scope<'_>,
    global: &Value,
    constructor: &str,
    method: &str,
    f: NativeFn,
    arity: u32,
) {
    let proto = scope
        .get(global, constructor)
        .ok()
        .and_then(|constructor| object_property(scope, &constructor, "prototype"));
    if let Some(proto) = proto {
        bind(scope, &proto, method, f, arity);
    }
}

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    if scope.has_property(global, "Intl").is_ok_and(|has| has) {
        return;
    }
    let intl = scope.new_object();
    register(
        scope,
        &intl,
        "Collator",
        collator::constructor,
        0,
        &collator::METHODS,
    );
    register(
        scope,
        &intl,
        "NumberFormat",
        number::constructor,
        0,
        &number::METHODS,
    );
    register(
        scope,
        &intl,
        "DateTimeFormat",
        datetime::constructor,
        0,
        &datetime::METHODS,
    );
    register(
        scope,
        &intl,
        "PluralRules",
        text::plural_constructor,
        0,
        &text::PLURAL_METHODS,
    );
    register(
        scope,
        &intl,
        "ListFormat",
        text::list_constructor,
        0,
        &text::LIST_METHODS,
    );
    register(
        scope,
        &intl,
        "RelativeTimeFormat",
        text::relative_constructor,
        0,
        &text::RELATIVE_METHODS,
    );
    register(
        scope,
        &intl,
        "DisplayNames",
        text::display_constructor,
        2,
        &text::DISPLAY_METHODS,
    );
    register(
        scope,
        &intl,
        "DurationFormat",
        text::duration_constructor,
        0,
        &text::DURATION_METHODS,
    );
    register(
        scope,
        &intl,
        "Segmenter",
        text::segmenter_constructor,
        0,
        &text::SEGMENTER_METHODS,
    );
    register(
        scope,
        &intl,
        "Locale",
        locale::constructor,
        1,
        &locale::METHODS,
    );
    bind(
        scope,
        &intl,
        "getCanonicalLocales",
        locale::get_canonical_locales,
        1,
    );
    bind(
        scope,
        &intl,
        "supportedValuesOf",
        locale::supported_values_of,
        1,
    );
    let _ = scope.define_to_string_tag(&intl, "Intl");
    let _ = scope.define(global, "Intl", intl, Attributes::METHOD);
    install_proto_hook(
        scope,
        global,
        "Number",
        "toLocaleString",
        number_to_locale_string,
        0,
    );
    install_proto_hook(
        scope,
        global,
        "Date",
        "toLocaleString",
        date_to_locale_string,
        0,
    );
    install_proto_hook(
        scope,
        global,
        "Date",
        "toLocaleDateString",
        date_to_locale_date_string,
        0,
    );
    install_proto_hook(
        scope,
        global,
        "Date",
        "toLocaleTimeString",
        date_to_locale_time_string,
        0,
    );
    install_proto_hook(
        scope,
        global,
        "String",
        "localeCompare",
        collator::string_locale_compare,
        1,
    );
}
