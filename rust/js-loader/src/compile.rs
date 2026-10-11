//! Southstar — compiled-script bookkeeping: the bytecode cache over classic scripts and modules, import attributes, import.meta and the script profiler's report lines.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_glib as glib;
use southstar_js_engine::quickjs;
use southstar_js_engine::{Attributes, Scope, Value};

const MIN_CACHED_SOURCE: usize = 1024;

const IMPORT_META_FIELD: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

pub(crate) fn cached_script(scope: &mut Scope<'_>, src: &[u8]) -> Option<Value> {
    if src.len() < MIN_CACHED_SOURCE {
        return None;
    }
    let bytecode = southstar_bytecode_cache::get(src)?;
    quickjs::read_bytecode(scope, &bytecode).ok()
}

pub(crate) fn store_script(scope: &mut Scope<'_>, compiled: &Value, src: &[u8]) {
    if src.len() < MIN_CACHED_SOURCE {
        return;
    }
    store(scope, compiled, src);
}

fn store(scope: &mut Scope<'_>, compiled: &Value, key: &[u8]) {
    if let Some(bytecode) = quickjs::write_bytecode(scope, compiled).filter(|b| !b.is_empty()) {
        southstar_bytecode_cache::put(key, &bytecode);
    }
}

fn module_key(name: &[u8], src: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(name.len() + 1 + src.len());
    key.extend_from_slice(name);
    key.push(0);
    key.extend_from_slice(src);
    key
}

pub(crate) fn cached_module(
    scope: &mut Scope<'_>,
    name: &[u8],
    src: &[u8],
) -> Option<Result<Value, Option<Value>>> {
    if src.len() < MIN_CACHED_SOURCE {
        return None;
    }
    let bytecode = southstar_bytecode_cache::get(&module_key(name, src))?;
    let module = quickjs::read_bytecode(scope, &bytecode)
        .ok()
        .filter(quickjs::is_module)?;
    Some(set_import_meta(scope, &module, false).map(|()| module))
}

pub(crate) fn store_module(scope: &mut Scope<'_>, module: &Value, name: &[u8], src: &[u8]) {
    if src.len() < MIN_CACHED_SOURCE || !quickjs::is_module(module) {
        return;
    }
    store(scope, module, &module_key(name, src));
}

pub(crate) fn set_import_meta(
    scope: &mut Scope<'_>,
    module: &Value,
    is_main: bool,
) -> Result<(), Option<Value>> {
    if !quickjs::is_module(module) {
        return Err(None);
    }
    let name = quickjs::module_name(scope, module).map_err(Some)?;
    let meta = quickjs::import_meta(scope, module).map_err(Some)?;
    let _ = scope.define(&meta, "url", name, IMPORT_META_FIELD);
    let _ = scope.define(&meta, "main", Value::boolean(is_main), IMPORT_META_FIELD);
    Ok(())
}

pub(crate) fn attributes_type_is(scope: &mut Scope<'_>, attributes: &Value, want: &[u8]) -> bool {
    if !attributes.is_object() {
        return false;
    }
    let Ok(kind) = scope.get(attributes, "type") else {
        return false;
    };
    kind.is_string() && scope.to_bytes(&kind).is_ok_and(|kind| kind == want)
}

pub(crate) fn profile_enabled() -> bool {
    static PROFILE: OnceLock<bool> = OnceLock::new();
    *PROFILE.get_or_init(|| std::env::var_os("NS_PROFILE").is_some())
}

pub(crate) fn report_eval(elapsed_us: i64, len: usize, cached: bool, origin: &[u8]) {
    let tag = if cached { "[bc] " } else { "" };
    report(
        format!(
            "[profile] js eval     {:6.1}ms  {len}b  {tag}",
            ms(elapsed_us)
        ),
        origin,
    );
}

pub(crate) fn report_module(elapsed_us: i64, len: usize, origin: &[u8]) {
    report(
        format!("[profile] js module   {:6.1}ms  {len}b  ", ms(elapsed_us)),
        origin,
    );
}

fn ms(elapsed_us: i64) -> f64 {
    elapsed_us as f64 / 1000.0
}

fn report(prefix: String, origin: &[u8]) {
    let mut line = prefix.into_bytes();
    line.extend_from_slice(origin);
    line.push(b'\n');
    glib::stderr_write(&line);
}
