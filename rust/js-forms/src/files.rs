//! Southstar — input.files: the FileList of a file input, read from the path the shell's file chooser stored on the element.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::{JsResult, ffi, named, text_is};

const CACHE_SLOT: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

fn base_name(path: &[u8]) -> &[u8] {
    let slash = path.iter().rposition(|&b| b == b'/');
    let separator = if cfg!(windows) {
        let backslash = path.iter().rposition(|&b| b == b'\\');
        match (slash, backslash) {
            (Some(s), Some(b)) => Some(s.max(b)),
            (s, b) => s.or(b),
        }
    } else {
        slash
    };
    separator.map_or(path, |at| &path[at + 1..])
}

fn make_file(scope: &mut Scope<'_>, path: &CStr) -> JsResult<Option<Value>> {
    let Some(file) = ffi::read_file(path) else {
        return Ok(None);
    };
    let global = scope.global();
    let file_ctor = scope.get(&global, "File")?;
    if !scope.is_constructor(&file_ctor) {
        return Ok(None);
    }
    let buffer = scope.new_array_buffer(&file.contents)?;
    let parts = scope.new_array();
    scope.set_index(&parts, 0, buffer)?;
    let options = scope.new_object();
    let mime = file
        .mime
        .as_deref()
        .filter(|m| !m.is_empty())
        .unwrap_or("application/octet-stream");
    let mime = scope.string(mime);
    scope.set(&options, "type", mime)?;
    let name = scope.string_from_bytes(base_name(path.to_bytes()));
    Ok(scope.construct(&file_ctor, &[parts, name, options]).ok())
}

pub fn get_files(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(input) = ffi::element(this).filter(|n| named(*n, "input")) else {
        return Ok(Value::null());
    };
    if !text_is(input.attr(c"type"), "file") {
        return Ok(Value::null());
    }
    let path = input.attr(c"data-nd-file-path").map(CStr::to_owned);
    let cached_path = scope.get(this, "__nd_files_path")?;
    let cached_path = if cached_path.is_string() {
        Some(scope.to_bytes(&cached_path)?)
    } else {
        None
    };
    let same = match (&path, &cached_path) {
        (None, None) => true,
        (Some(path), Some(cached)) => path.to_bytes() == crate::until_nul(cached.clone()),
        _ => false,
    };
    if same {
        let cached = scope.get(this, "__nd_files_cache")?;
        if cached.is_array() {
            return Ok(cached);
        }
    }
    let files = scope.new_array();
    if let Some(path) = path.as_deref().filter(|p| !p.is_empty())
        && let Some(file) = make_file(scope, path)?
    {
        scope.set_index(&files, 0, file)?;
    }
    let item = ffi::named_map_item_function(scope);
    scope.set(&files, "item", item)?;
    let stored = scope.string_from_bytes(path.as_deref().map_or(b"", CStr::to_bytes));
    scope.define(this, "__nd_files_path", stored, CACHE_SLOT)?;
    scope.define(this, "__nd_files_cache", files.clone(), CACHE_SLOT)?;
    Ok(files)
}
