//! Southstar — installing the crypto object and crypto.subtle on window and worker globals.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{get, random, set, subtle};

const SUBTLE_METHODS: [(&str, u32, NativeFn); 12] = [
    ("digest", 2, subtle::digest),
    ("encrypt", 3, subtle::encrypt),
    ("decrypt", 3, subtle::decrypt),
    ("sign", 3, subtle::sign),
    ("verify", 4, subtle::verify),
    ("generateKey", 3, subtle::generate_key),
    ("importKey", 5, subtle::import_key),
    ("exportKey", 2, subtle::export_key),
    ("deriveBits", 3, subtle::derive_bits),
    ("deriveKey", 5, subtle::derive_key),
    ("wrapKey", 4, subtle::wrap_key),
    ("unwrapKey", 7, subtle::unwrap_key),
];

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

fn new_subtle(scope: &mut Scope<'_>) -> Value {
    let subtle = scope.new_object();
    for (name, arity, f) in SUBTLE_METHODS {
        bind(scope, &subtle, name, arity, f);
    }
    subtle
}

fn new_crypto(scope: &mut Scope<'_>) -> Value {
    let crypto = scope.new_object();
    bind(
        scope,
        &crypto,
        "getRandomValues",
        1,
        random::get_random_values,
    );
    bind(scope, &crypto, "randomUUID", 0, random::random_uuid);
    crypto
}

pub(crate) fn install_window(scope: &mut Scope<'_>, global: &Value) {
    let crypto = new_crypto(scope);
    let _ = scope.define_to_string_tag(&crypto, "Crypto");
    set(scope, global, "crypto", crypto);
}

pub(crate) fn install_window_subtle(scope: &mut Scope<'_>, global: &Value) {
    let subtle = new_subtle(scope);
    let crypto = get(scope, global, "crypto");
    if !crypto.is_undefined() && !crypto.is_null() {
        set(scope, &crypto, "subtle", subtle);
    }
}

pub(crate) fn install_worker(scope: &mut Scope<'_>, global: &Value) {
    let crypto = new_crypto(scope);
    let subtle = new_subtle(scope);
    set(scope, &crypto, "subtle", subtle);
    set(scope, global, "crypto", crypto);
}
