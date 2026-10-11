//! Southstar — caches.open(): an empty Cache whose methods settle at once, as no response is ever stored.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_bind::natives;
use southstar_js_engine::{NativeFn, Scope, Value};

const CACHE_METHODS: [(&str, u32, NativeFn); 7] = [
    ("match", 1, natives::resolved_undefined),
    ("matchAll", 1, natives::resolved_empty_array),
    ("put", 2, natives::resolved_undefined),
    ("add", 1, natives::resolved_undefined),
    ("addAll", 1, natives::resolved_undefined),
    ("delete", 1, natives::resolved_false),
    ("keys", 1, natives::resolved_empty_array),
];

pub(crate) fn open(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let cache = scope.new_object();
    for (name, arity, f) in CACHE_METHODS {
        let function = scope.function(name, arity, f);
        let _ = southstar_js_bind::bind_function(scope, &cache, name, function);
    }
    natives::resolved(scope, cache)
}
