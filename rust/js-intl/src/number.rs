//! Southstar — Intl.NumberFormat and Number/BigInt toLocaleString: options, internal slots and the JavaScript methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod data;
pub(crate) mod decimal;
pub(crate) mod digits;
pub(crate) mod format;

use std::cell::RefCell;

use southstar_js_engine::{Attributes, BoundFn, ObjectKind, Scope, Trace, Value};

use crate::ecma402::{
    KeyRequest, OptionsBag, canonicalize_locale_list, create_data_property, create_string_property,
    is_new_target, ordinary_has_instance, prototype_from_constructor, resolve_locale,
    supported_locales,
};
use crate::{HIDDEN, arg, bind};
use decimal::Numeric;
use digits::{DigitOptions, read_digit_options};
use format::{Part, join, partition, partition_range};

#[derive(Clone)]
pub(crate) struct NumberFormatOptions {
    pub locale: String,
    pub data_locale: String,
    pub numbering_system: String,
    pub style: &'static str,
    pub currency: Option<String>,
    pub currency_display: &'static str,
    pub currency_sign: &'static str,
    pub unit: Option<String>,
    pub unit_display: &'static str,
    pub notation: &'static str,
    pub compact_display: &'static str,
    pub digits: DigitOptions,
    pub use_grouping: Option<&'static str>,
    pub sign_display: &'static str,
}

struct NumberFormatSlots {
    options: NumberFormatOptions,
    bound_format: RefCell<Option<Value>>,
}

impl Trace for NumberFormatSlots {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(format) = self.bound_format.borrow().as_ref() {
            visit(format);
        }
    }
}

const PROTOTYPE_METHODS: [(&str, BoundFn, u32); 4] = [
    ("formatToParts", format_to_parts, 1),
    ("formatRange", format_range, 2),
    ("formatRangeToParts", format_range_to_parts, 2),
    ("resolvedOptions", resolved_options, 0),
];

fn is_well_formed_currency(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|c| c.is_ascii_alphabetic())
}

fn is_well_formed_unit(unit: &str) -> bool {
    if data::simple_unit(unit).is_some() {
        return true;
    }
    unit.split_once("-per-")
        .is_some_and(|(numerator, denominator)| {
            data::simple_unit(numerator).is_some() && data::simple_unit(denominator).is_some()
        })
}

fn is_unicode_type(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            (3..=8).contains(&part.len()) && part.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}

fn grouping_option(
    scope: &mut Scope<'_>,
    options: &OptionsBag,
    fallback: &'static str,
) -> Result<Option<&'static str>, Value> {
    let value = options.get(scope, "useGrouping")?;
    if value.is_undefined() {
        return Ok(Some(fallback));
    }
    if value.is_bool() && scope.to_bool(&value) {
        return Ok(Some("always"));
    }
    if !scope.to_bool(&value) {
        return Ok(None);
    }
    let text = scope.to_string(&value)?;
    match text.as_str() {
        "true" | "false" => Ok(Some(fallback)),
        "min2" => Ok(Some("min2")),
        "auto" => Ok(Some("auto")),
        "always" => Ok(Some("always")),
        _ => Err(scope.range_error(&format!("{text} is not a valid value for useGrouping"))),
    }
}

fn resolve_numbering(requested: &[String], numbering: Option<String>) -> (String, String, String) {
    let supported = |_: &str, value: &str| data::is_numbering_system(value);
    let default = |locale: &str| data::default_numbering_system(locale).to_owned();
    let keys = [KeyRequest {
        key: "nu",
        option: numbering,
        supported: &supported,
        default: &default,
    }];
    let mut resolution = resolve_locale(requested, &data::is_available, &keys);
    let numbering = resolution.values.pop().unwrap_or_else(|| "latn".to_owned());
    (resolution.locale, resolution.data_locale, numbering)
}

pub(crate) fn initialize(
    scope: &mut Scope<'_>,
    locales: &Value,
    options: &Value,
) -> Result<NumberFormatOptions, Value> {
    let requested = canonicalize_locale_list(scope, locales)?;
    let options = OptionsBag::coerce(scope, options)?;
    options.string(scope, "localeMatcher", &["lookup", "best fit"], "best fit")?;
    let numbering = options.free_string(scope, "numberingSystem")?;
    if numbering.as_deref().is_some_and(|n| !is_unicode_type(n)) {
        return Err(scope.range_error("numberingSystem is not a well-formed Unicode type"));
    }
    let (locale, data_locale, numbering_system) = resolve_numbering(&requested, numbering);
    let style = options.string(
        scope,
        "style",
        &["decimal", "percent", "currency", "unit"],
        "decimal",
    )?;
    let currency = options.free_string(scope, "currency")?;
    match &currency {
        None if style == "currency" => {
            return Err(scope.type_error("currency style requires a currency"));
        }
        Some(code) if !is_well_formed_currency(code) => {
            return Err(scope.range_error(&format!("{code} is not a well-formed currency code")));
        }
        _ => {}
    }
    let currency_display = options.string(
        scope,
        "currencyDisplay",
        &["code", "symbol", "narrowSymbol", "name"],
        "symbol",
    )?;
    let currency_sign = options.string(
        scope,
        "currencySign",
        &["standard", "accounting"],
        "standard",
    )?;
    let unit = options.free_string(scope, "unit")?;
    match &unit {
        None if style == "unit" => {
            return Err(scope.type_error("unit style requires a unit"));
        }
        Some(unit) if !is_well_formed_unit(unit) => {
            return Err(scope.range_error(&format!("{unit} is not a sanctioned unit")));
        }
        _ => {}
    }
    let unit_display =
        options.string(scope, "unitDisplay", &["short", "narrow", "long"], "short")?;
    let notation = options.string(
        scope,
        "notation",
        &["standard", "scientific", "engineering", "compact"],
        "standard",
    )?;
    let currency = currency
        .filter(|_| style == "currency")
        .map(|code| code.to_ascii_uppercase());
    let (minimum_fraction, maximum_fraction) = match &currency {
        Some(code) if notation == "standard" => {
            let digits = data::currency_digits(code);
            (digits, digits)
        }
        _ => (0, if style == "percent" { 0 } else { 3 }),
    };
    let digits = read_digit_options(
        scope,
        &options,
        minimum_fraction,
        maximum_fraction,
        notation,
    )?;
    let compact_display = options.string(scope, "compactDisplay", &["short", "long"], "short")?;
    let default_grouping = if notation == "compact" {
        "min2"
    } else {
        "auto"
    };
    let use_grouping = grouping_option(scope, &options, default_grouping)?;
    let sign_display = options.string(
        scope,
        "signDisplay",
        &["auto", "never", "always", "exceptZero", "negative"],
        "auto",
    )?;
    Ok(NumberFormatOptions {
        locale,
        data_locale,
        numbering_system,
        style,
        currency,
        currency_display,
        currency_sign,
        unit: unit.filter(|_| style == "unit"),
        unit_display,
        notation,
        compact_display,
        digits,
        use_grouping,
        sign_display,
    })
}

fn options_of(value: &Value) -> Option<NumberFormatOptions> {
    value.with_host::<NumberFormatSlots, _>(|slots| slots.options.clone())
}

fn require(
    scope: &mut Scope<'_>,
    this: &Value,
    method: &str,
) -> Result<NumberFormatOptions, Value> {
    options_of(this).ok_or_else(|| {
        scope.type_error(&format!(
            "Intl.NumberFormat.prototype.{method} called on an incompatible receiver"
        ))
    })
}

fn unwrap(
    scope: &mut Scope<'_>,
    this: &Value,
    data: &[Value],
    method: &str,
) -> Result<Value, Value> {
    if this.is_object()
        && options_of(this).is_none()
        && ordinary_has_instance(scope, &arg(data, 1), this)?
    {
        let inner = scope.get_key(this, &arg(data, 0))?;
        if options_of(&inner).is_some() {
            return Ok(inner);
        }
    }
    require(scope, this, method)?;
    Ok(this.clone())
}

fn construct(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let symbol = arg(data, 0);
    let intrinsic = arg(data, 1);
    let new_target = is_new_target(scope, this);
    let receiver = if new_target {
        Value::undefined()
    } else {
        scope.constructor_receiver()
    };
    let prototype = if new_target {
        prototype_from_constructor(scope, this, &intrinsic, "NumberFormat")?
    } else {
        intrinsic.clone()
    };
    let options = initialize(scope, &arg(args, 0), &arg(args, 1))?;
    let slots = NumberFormatSlots {
        options,
        bound_format: RefCell::new(None),
    };
    let format = scope.new_traced_host_object(Some(&prototype), slots);
    if !new_target && ordinary_has_instance(scope, &intrinsic, &receiver)? {
        scope.define_with_key(&receiver, &symbol, format, HIDDEN)?;
        return Ok(receiver);
    }
    Ok(format)
}

pub(crate) fn to_intl_mathematical_value(
    scope: &mut Scope<'_>,
    value: &Value,
) -> Result<Numeric, Value> {
    if value.is_number() {
        return scope.to_number(value).map(Numeric::from_f64);
    }
    let primitive = if value.is_object() {
        scope.to_primitive_number(value)?
    } else {
        value.clone()
    };
    if primitive.is_bigint() {
        let text = scope.to_string(&primitive)?;
        return Ok(Numeric::from_bigint_text(&text));
    }
    if primitive.is_string() {
        let text = scope.to_string(&primitive)?;
        return Ok(Numeric::from_decimal_text(&text));
    }
    scope.to_number(&primitive).map(Numeric::from_f64)
}

fn format_text(scope: &mut Scope<'_>, options: &NumberFormatOptions, x: &Numeric) -> Value {
    let text = join(&partition(options, x));
    scope.string(&text)
}

fn parts_array(
    scope: &mut Scope<'_>,
    parts: impl Iterator<Item = (Part, Option<&'static str>)>,
) -> Result<Value, Value> {
    let array = scope.new_array();
    for (index, (part, source)) in parts.enumerate() {
        let object = scope.new_object();
        create_string_property(scope, &object, "type", part.kind)?;
        create_string_property(scope, &object, "value", &part.value)?;
        if let Some(source) = source {
            create_string_property(scope, &object, "source", source)?;
        }
        scope.define_entry(&array, &Value::int(index as i32), object)?;
    }
    Ok(array)
}

fn bound_format(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let options = require(scope, &arg(data, 0), "format")?;
    let x = to_intl_mathematical_value(scope, &arg(args, 0))?;
    Ok(format_text(scope, &options, &x))
}

fn format_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let format = unwrap(scope, this, data, "format")?;
    let existing = format
        .with_host::<NumberFormatSlots, _>(|slots| slots.bound_format.borrow().clone())
        .flatten();
    if let Some(function) = existing {
        return Ok(function);
    }
    let function = scope.bound_function("", 1, bound_format, std::slice::from_ref(&format));
    format.with_host::<NumberFormatSlots, _>(|slots| {
        *slots.bound_format.borrow_mut() = Some(function.clone());
    });
    Ok(function)
}

fn format_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let options = require(scope, this, "formatToParts")?;
    let x = to_intl_mathematical_value(scope, &arg(args, 0))?;
    let parts = partition(&options, &x);
    parts_array(scope, parts.into_iter().map(|part| (part, None)))
}

fn range_arguments(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    method: &str,
) -> Result<(NumberFormatOptions, Numeric, Numeric), Value> {
    let options = require(scope, this, method)?;
    let (start, end) = (arg(args, 0), arg(args, 1));
    if start.is_undefined() || end.is_undefined() {
        return Err(scope.type_error(&format!("{method} requires a start and an end")));
    }
    let x = to_intl_mathematical_value(scope, &start)?;
    let y = to_intl_mathematical_value(scope, &end)?;
    if x.is_nan() || y.is_nan() {
        return Err(scope.range_error(&format!("{method} cannot format a range with NaN")));
    }
    Ok((options, x, y))
}

fn format_range(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let (options, x, y) = range_arguments(scope, this, args, "formatRange")?;
    let parts: Vec<Part> = partition_range(&options, &x, &y)
        .into_iter()
        .map(|(part, _)| part)
        .collect();
    Ok(scope.string(&join(&parts)))
}

fn format_range_to_parts(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    _data: &[Value],
) -> Result<Value, Value> {
    let (options, x, y) = range_arguments(scope, this, args, "formatRangeToParts")?;
    let parts = partition_range(&options, &x, &y);
    parts_array(
        scope,
        parts.into_iter().map(|(part, source)| (part, Some(source))),
    )
}

fn create_number_property(
    scope: &mut Scope<'_>,
    object: &Value,
    key: &str,
    value: u32,
) -> Result<(), Value> {
    create_data_property(scope, object, key, Value::number(f64::from(value)))
}

fn resolved_options(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let format = unwrap(scope, this, data, "resolvedOptions")?;
    let options = require(scope, &format, "resolvedOptions")?;
    let out = scope.new_object();
    create_string_property(scope, &out, "locale", &options.locale)?;
    create_string_property(scope, &out, "numberingSystem", &options.numbering_system)?;
    create_string_property(scope, &out, "style", options.style)?;
    if let Some(currency) = &options.currency {
        create_string_property(scope, &out, "currency", currency)?;
        create_string_property(scope, &out, "currencyDisplay", options.currency_display)?;
        create_string_property(scope, &out, "currencySign", options.currency_sign)?;
    }
    if let Some(unit) = &options.unit {
        create_string_property(scope, &out, "unit", unit)?;
        create_string_property(scope, &out, "unitDisplay", options.unit_display)?;
    }
    let digits = &options.digits;
    create_number_property(
        scope,
        &out,
        "minimumIntegerDigits",
        digits.minimum_integer_digits,
    )?;
    if let Some((minimum, maximum)) = digits.fraction_digits {
        create_number_property(scope, &out, "minimumFractionDigits", minimum)?;
        create_number_property(scope, &out, "maximumFractionDigits", maximum)?;
    }
    if let Some((minimum, maximum)) = digits.significant_digits {
        create_number_property(scope, &out, "minimumSignificantDigits", minimum)?;
        create_number_property(scope, &out, "maximumSignificantDigits", maximum)?;
    }
    let grouping = match options.use_grouping {
        Some(strategy) => scope.string(strategy),
        None => Value::boolean(false),
    };
    create_data_property(scope, &out, "useGrouping", grouping)?;
    create_string_property(scope, &out, "notation", options.notation)?;
    if options.notation == "compact" {
        create_string_property(scope, &out, "compactDisplay", options.compact_display)?;
    }
    create_string_property(scope, &out, "signDisplay", options.sign_display)?;
    create_number_property(scope, &out, "roundingIncrement", digits.rounding_increment)?;
    create_string_property(scope, &out, "roundingMode", digits.rounding_mode)?;
    create_string_property(scope, &out, "roundingPriority", digits.rounding_priority)?;
    create_string_property(
        scope,
        &out,
        "trailingZeroDisplay",
        digits.trailing_zero_display,
    )?;
    Ok(out)
}

fn supported_locales_of(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    supported_locales(scope, &arg(args, 0), &arg(args, 1), &data::is_available)
}

pub(crate) fn install(scope: &mut Scope<'_>, intl: &Value, fallback_symbol: &Value) {
    let prototype = scope.new_object();
    let data = [fallback_symbol.clone(), prototype.clone()];
    let function = scope.bound_constructor("NumberFormat", 0, construct, &data);
    let getter = scope.bound_function("get format", 0, format_getter, &data);
    let accessor = Attributes {
        writable: false,
        enumerable: false,
        configurable: true,
    };
    let _ = scope.define_accessor(&prototype, "format", Some(&getter), None, accessor);
    for (name, method, arity) in PROTOTYPE_METHODS {
        let method = scope.bound_function(name, arity, method, &data);
        let _ = scope.define(&prototype, name, method, Attributes::METHOD);
    }
    let _ = scope.define_to_string_tag(&prototype, "Intl.NumberFormat");
    let _ = scope.set_constructor(&function, &prototype);
    bind(
        scope,
        &function,
        "supportedLocalesOf",
        supported_locales_of,
        1,
    );
    let _ = scope.define(intl, "NumberFormat", function, Attributes::METHOD);
}

fn locale_string(scope: &mut Scope<'_>, x: &Numeric, args: &[Value]) -> Result<Value, Value> {
    let options = initialize(scope, &arg(args, 0), &arg(args, 1))?;
    Ok(format_text(scope, &options, x))
}

fn number_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let number = if this.is_number() {
        this.clone()
    } else if scope.object_kind(this) == Some(ObjectKind::Number) {
        scope.call(&arg(data, 0), this, &[])?
    } else {
        return Err(
            scope.type_error("Number.prototype.toLocaleString requires that 'this' be a Number")
        );
    };
    let number = scope.to_number(&number)?;
    locale_string(scope, &Numeric::from_f64(number), args)
}

fn bigint_to_locale_string(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let bigint = if this.is_bigint() {
        this.clone()
    } else if scope.object_kind(this) == Some(ObjectKind::BigInt) {
        scope.call(&arg(data, 0), this, &[])?
    } else {
        return Err(
            scope.type_error("BigInt.prototype.toLocaleString requires that 'this' be a BigInt")
        );
    };
    let text = scope.to_string(&bigint)?;
    locale_string(scope, &Numeric::from_bigint_text(&text), args)
}

fn install_to_locale_string(scope: &mut Scope<'_>, global: &Value, constructor: &str, f: BoundFn) {
    let prototype = scope
        .get(global, constructor)
        .and_then(|constructor| scope.get(&constructor, "prototype"));
    let Ok(prototype) = prototype else {
        return;
    };
    if !prototype.is_object() {
        return;
    }
    let Ok(value_of) = scope.get(&prototype, "valueOf") else {
        return;
    };
    let function = scope.bound_function("toLocaleString", 0, f, &[value_of]);
    let _ = scope.define(&prototype, "toLocaleString", function, Attributes::METHOD);
}

pub(crate) fn install_locale_string_hooks(scope: &mut Scope<'_>, global: &Value) {
    install_to_locale_string(scope, global, "Number", number_to_locale_string);
    install_to_locale_string(scope, global, "BigInt", bigint_to_locale_string);
}
