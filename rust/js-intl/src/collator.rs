//! Southstar — Intl.Collator and String.prototype.localeCompare over GLib's Unicode collation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

use southstar_js_engine::{Scope, Value};

use crate::{
    Method, arg, arg_locale, bind_bound, c_string, ffi, hget_int, hget_str, hide, hide_text,
    new_instance, opt_bool, opt_str, set, set_text,
};

pub(crate) const METHODS: [Method; 2] = [("compare", compare, 2), ("resolvedOptions", resolved, 0)];

pub(crate) fn constructor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let locale = arg_locale(scope, &arg(args, 0));
    let options = arg(args, 1);
    let collator = new_instance(scope, this, "Collator");
    let usage = opt_str(scope, &options, "usage");
    let sensitivity = opt_str(scope, &options, "sensitivity");
    let case_first = opt_str(scope, &options, "caseFirst");
    hide_text(scope, &collator, "_locale", &locale);
    hide_text(
        scope,
        &collator,
        "_usage",
        usage.as_deref().unwrap_or(b"sort"),
    );
    hide_text(
        scope,
        &collator,
        "_sensitivity",
        sensitivity.as_deref().unwrap_or(b"variant"),
    );
    hide_text(
        scope,
        &collator,
        "_caseFirst",
        case_first.as_deref().unwrap_or(b"false"),
    );
    let numeric = opt_bool(scope, &options, "numeric", 0) != 0;
    hide(scope, &collator, "_numeric", Value::boolean(numeric));
    bind_bound(scope, &collator, "compare", compare_bound, 2);
    Ok(collator)
}

fn digits_end(text: &[u8], from: usize) -> usize {
    from + text[from..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count()
}

pub(crate) fn numeric_compare(a: &[u8], b: &[u8]) -> i32 {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (end_a, end_b) = (digits_end(a, i), digits_end(b, j));
            while a[i] == b'0' && i + 1 < end_a {
                i += 1;
            }
            while b[j] == b'0' && j + 1 < end_b {
                j += 1;
            }
            let (len_a, len_b) = (end_a - i, end_b - j);
            if len_a != len_b {
                return if len_a < len_b { -1 } else { 1 };
            }
            match a[i..end_a].cmp(&b[j..end_b]) {
                Ordering::Less => return -1,
                Ordering::Greater => return 1,
                Ordering::Equal => {}
            }
            i = end_a;
            j = end_b;
        } else {
            if a[i] != b[j] {
                return if a[i] < b[j] { -1 } else { 1 };
            }
            i += 1;
            j += 1;
        }
    }
    if i < a.len() {
        1
    } else if j < b.len() {
        -1
    } else {
        0
    }
}

fn sign(order: Ordering) -> i32 {
    match order {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn key_compare(a: &[u8], b: &[u8], sensitivity: &[u8], collate_ties: bool) -> i32 {
    let key_a = ffi::collation_key(a, sensitivity);
    let key_b = ffi::collation_key(b, sensitivity);
    let mut order = sign(key_a.cmp(&key_b));
    if order == 0 && collate_ties {
        order = ffi::collate(a, b).signum();
    }
    order
}

fn compare_strings(
    scope: &mut Scope<'_>,
    collator: &Value,
    a: Vec<u8>,
    b: Vec<u8>,
) -> Result<Value, Value> {
    let sensitivity = hget_str(scope, collator, "_sensitivity");
    let numeric = hget_int(scope, collator, "_numeric", 0);
    let result = if numeric != 0 {
        numeric_compare(&a, &b)
    } else {
        let sensitivity = sensitivity.as_deref().unwrap_or(b"variant");
        key_compare(&a, &b, sensitivity, sensitivity == b"variant")
    };
    Ok(Value::int(result))
}

fn compare(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    let a = c_string(scope, &arg(args, 0));
    let b = c_string(scope, &arg(args, 1));
    let b = b?;
    let a = a?;
    compare_strings(scope, this, a, b)
}

fn compare_bound(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    compare(scope, &arg(data, 0), args)
}

fn resolved(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let options = scope.new_object();
    let locale = hget_str(scope, this, "_locale");
    let usage = hget_str(scope, this, "_usage");
    let sensitivity = hget_str(scope, this, "_sensitivity");
    let case_first = hget_str(scope, this, "_caseFirst");
    set_text(
        scope,
        &options,
        "locale",
        locale.as_deref().unwrap_or(b"en-US"),
    );
    set_text(
        scope,
        &options,
        "usage",
        usage.as_deref().unwrap_or(b"sort"),
    );
    set_text(
        scope,
        &options,
        "sensitivity",
        sensitivity.as_deref().unwrap_or(b"variant"),
    );
    set_text(
        scope,
        &options,
        "caseFirst",
        case_first.as_deref().unwrap_or(b"false"),
    );
    set_text(scope, &options, "collation", b"default");
    let numeric = hget_int(scope, this, "_numeric", 0) != 0;
    set(scope, &options, "numeric", Value::boolean(numeric));
    set(scope, &options, "ignorePunctuation", Value::boolean(false));
    Ok(options)
}

pub(crate) fn string_locale_compare(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let a = c_string(scope, this).unwrap_or_default();
    let b = match args.first() {
        Some(other) => c_string(scope, other).unwrap_or_default(),
        None => Vec::new(),
    };
    let (sensitivity, numeric) = match args.get(2) {
        Some(options) if options.is_object() => (
            opt_str(scope, options, "sensitivity"),
            opt_bool(scope, options, "numeric", 0),
        ),
        _ => (None, 0),
    };
    let result = if numeric != 0 {
        numeric_compare(&a, &b)
    } else {
        key_compare(&a, &b, sensitivity.as_deref().unwrap_or(b"variant"), true)
    };
    Ok(Value::int(result))
}
