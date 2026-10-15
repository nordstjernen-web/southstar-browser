//! Southstar — the native, ICU-free ECMA-402 Intl API, installed through the engine-neutral JavaScript layer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod case;
mod collator;
mod datetime;
mod ecma402;
mod ffi;
mod locale;
mod negotiate;
mod number;
mod plural;
mod text;

use southstar_intl_core::{cldr_index, collation, tag};
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

type Method = (&'static str, NativeFn, u32);

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn lang_subtag(locale: &[u8]) -> Vec<u8> {
    locale
        .iter()
        .take_while(|&&c| c != b'-')
        .take(15)
        .map(u8::to_ascii_lowercase)
        .collect()
}

fn object_property(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Value> {
    scope.get(object, key).ok().filter(Value::is_object)
}

fn length(scope: &mut Scope<'_>, object: &Value) -> u32 {
    scope
        .get(object, "length")
        .and_then(|length| scope.to_int32(&length))
        .map_or(0, |length| length as u32)
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn, arity: u32) {
    let function = scope.function(name, arity, f);
    let _ = scope.define(object, name, function, Attributes::METHOD);
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let requested = negotiate::canonicalize_locale_list(scope, &arg(args, 0))?;
    negotiate::supported_locales(scope, &requested, &arg(args, 1))
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

fn legacy_constructed_symbol(scope: &mut Scope<'_>) -> Value {
    let global = scope.global();
    let description = scope.string("IntlLegacyConstructedSymbol");
    scope
        .get(&global, "Symbol")
        .and_then(|symbol| scope.call(&symbol, &Value::undefined(), &[description]))
        .unwrap_or_else(|_| Value::undefined())
}

fn global_constructor(scope: &mut Scope<'_>, name: &str) -> Result<Value, Value> {
    let global = scope.global();
    let intl = scope.get(&global, "Intl")?;
    scope.get(&intl, name)
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
    let fallback_symbol = legacy_constructed_symbol(scope);
    collator::install(scope, &intl);
    number::install(scope, &intl, &fallback_symbol);
    datetime::install_constructor(scope, &intl, &fallback_symbol);
    plural::install(scope, &intl);
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
    text::install(scope, &intl);
    locale::install(scope, &intl);
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
    number::install_locale_string_hooks(scope, global);
    datetime::install(scope, global);
    install_proto_hook(
        scope,
        global,
        "String",
        "localeCompare",
        collator::string_locale_compare,
        1,
    );
    case::install(scope, global);
}
