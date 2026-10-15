//! Southstar — String.prototype.toLocaleUpperCase and toLocaleLowerCase with the language-sensitive SpecialCasing rules.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_intl_core::casing::{self, CaseRules};
use southstar_js_engine::{Attributes, BoundFn, Scope, Value};

use crate::arg;
use crate::negotiate;
use crate::tag::LocaleId;

fn case_rules(scope: &mut Scope<'_>, locales: &Value) -> Result<CaseRules, Value> {
    let requested = negotiate::canonicalize_locale_list(scope, locales)?;
    let locale = requested
        .first()
        .cloned()
        .unwrap_or_else(|| negotiate::default_locale().to_owned());
    let language = LocaleId::parse(&locale).map(|parsed| parsed.id.language);
    Ok(CaseRules::for_language(language.as_deref().unwrap_or("")))
}

fn transform(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
    upper: bool,
) -> Result<Value, Value> {
    if this.is_undefined() || this.is_null() {
        return Err(
            scope.type_error("String.prototype case conversion called on null or undefined")
        );
    }
    let text = scope.to_string_value(this)?;
    let rules = case_rules(scope, &arg(args, 0))?;
    if rules == CaseRules::Default {
        return scope.call(&arg(data, 0), &text, &[]);
    }
    let text = scope.to_string(&text)?;
    let converted = if upper {
        casing::to_upper(&text, rules)
    } else {
        casing::to_lower(&text, rules)
    };
    Ok(scope.string(&converted))
}

fn to_locale_upper_case(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    transform(scope, this, args, data, true)
}

fn to_locale_lower_case(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    transform(scope, this, args, data, false)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let Ok(string) = scope.get(global, "String") else {
        return;
    };
    let Ok(proto) = scope.get(&string, "prototype") else {
        return;
    };
    let hooks: [(&str, &str, BoundFn); 2] = [
        ("toLocaleUpperCase", "toUpperCase", to_locale_upper_case),
        ("toLocaleLowerCase", "toLowerCase", to_locale_lower_case),
    ];
    for (name, plain, f) in hooks {
        let Ok(original) = scope.get(&proto, plain) else {
            continue;
        };
        let function = scope.bound_function(name, 0, f, &[original]);
        let _ = scope.define(&proto, name, function, Attributes::METHOD);
    }
}
