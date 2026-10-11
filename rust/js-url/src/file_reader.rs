//! Southstar — FileReader: the four read methods, abort, the idle that completes a read and the event sequence it fires.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::CStr;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, HO_FILE_READER, Js, Source};
use crate::{
    ALL, JsResult, bind_fn, get, illegal_invocation, int64_prop, page_holding, set,
    with_existing_page, with_page,
};

const PUMP_RETRY_MS: u32 = 4;

pub(crate) struct PendingRead {
    id: u64,
    reader: Value,
    generation: i64,
    source: Source,
}

thread_local! {
    static NEXT_READ: Cell<u64> = const { Cell::new(1) };
}

const INVALID_STATE_ERR: i32 = 11;
const EMPTY: i32 = 0;
const LOADING: i32 = 1;
const DONE: i32 = 2;

const HANDLERS: [&str; 6] = [
    "onload",
    "onerror",
    "onloadend",
    "onprogress",
    "onabort",
    "onloadstart",
];

#[derive(Clone, Copy)]
enum Read {
    ArrayBuffer,
    BinaryString,
    DataUrl,
    Text,
}

impl Read {
    fn name(self) -> &'static str {
        match self {
            Read::ArrayBuffer => "readAsArrayBuffer",
            Read::BinaryString => "readAsBinaryString",
            Read::DataUrl => "readAsDataURL",
            Read::Text => "readAsText",
        }
    }
}

fn ready_state(scope: &mut Scope<'_>, reader: &Value) -> i32 {
    let state = get(scope, reader, "readyState");
    scope.to_int32(&state).unwrap_or(0)
}

fn is_current(scope: &mut Scope<'_>, reader: &Value, generation: i64) -> bool {
    int64_prop(scope, reader, "_gen", -1) == generation
}

fn fire(scope: &mut Scope<'_>, reader: &Value, kind: &CStr) {
    let total = get(scope, reader, "_total");
    let total = scope.to_number(&total).unwrap_or(0.0);
    let loaded = if kind == c"loadstart" { 0.0 } else { total };
    ffi::fire_progress(scope, reader, kind, loaded, total);
}

pub(crate) fn run(scope: &mut Scope<'_>, reader: &Value, generation: i64) {
    let mut live = is_current(scope, reader, generation);
    if live {
        fire(scope, reader, c"loadstart");
    }
    live = live && is_current(scope, reader, generation);
    if live {
        fire(scope, reader, c"progress");
    }
    live = live && is_current(scope, reader, generation);
    if live {
        let pending = get(scope, reader, "_pending");
        set(scope, reader, "result", pending);
        set(scope, reader, "_pending", Value::undefined());
        set(scope, reader, "readyState", Value::int(DONE));
        fire(scope, reader, c"load");
    }
    live = live && is_current(scope, reader, generation);
    if live {
        fire(scope, reader, c"loadend");
    }
}

fn schedule(scope: &mut Scope<'_>, reader: &Value, generation: i64) {
    let js = Js::of(scope);
    if js.is_null() {
        return;
    }
    let id = NEXT_READ.with(|next| next.replace(next.get() + 1));
    let source = ffi::attach_read_idle(js, id);
    let read = PendingRead {
        id,
        reader: reader.clone(),
        generation,
        source,
    };
    with_page(js, |page| page.reads.push(read));
}

pub(crate) fn complete(id: u64) {
    let Some((js, ())) = page_holding(|page| page.reads.iter().any(|r| r.id == id).then_some(()))
    else {
        return;
    };
    if ffi::in_pump(js) {
        let source = ffi::attach_read_retry(js, id, PUMP_RETRY_MS);
        with_existing_page(js, |page| {
            if let Some(read) = page.reads.iter_mut().find(|r| r.id == id) {
                read.source = source;
            }
        });
        return;
    }
    let pending = with_existing_page(js, |page| {
        page.reads
            .iter()
            .find(|r| r.id == id)
            .map(|r| (r.reader.clone(), r.generation))
    })
    .flatten();
    let Some((reader, generation)) = pending else {
        return;
    };
    ffi::in_main_context(js, |scope| run(scope, &reader, generation));
    drop(reader);
    with_existing_page(js, |page| page.reads.retain(|r| r.id != id));
}

pub(crate) fn pending(js: Js) -> bool {
    with_existing_page(js, |page| !page.reads.is_empty()).unwrap_or(false)
}

pub(crate) fn teardown(js: Js) {
    let reads = with_existing_page(js, |page| core::mem::take(&mut page.reads)).unwrap_or_default();
    for read in &reads {
        ffi::source_remove(js, read.source);
    }
    drop(reads);
}

fn is_blob(scope: &mut Scope<'_>, value: &Value) -> bool {
    value.is_object() && get(scope, value, "__ndBlobBytes").is_object()
}

fn data_url(scope: &mut Scope<'_>, blob: &Value, bytes: &[u8]) -> Value {
    let kind = get(scope, blob, "type");
    let kind = if kind.is_string() {
        scope.to_bytes(&kind).ok().filter(|k| !k.is_empty())
    } else {
        None
    };
    let mut url = b"data:".to_vec();
    url.extend_from_slice(kind.as_deref().unwrap_or(b"application/octet-stream"));
    url.extend_from_slice(b";base64,");
    url.extend_from_slice(&crate::base64::encode(bytes));
    scope.string_from_bytes(&url)
}

fn read(scope: &mut Scope<'_>, this: &Value, args: &[Value], how: Read) -> JsResult {
    if !ffi::host_is(this, HO_FILE_READER) {
        return Err(illegal_invocation(scope));
    }
    let Some(blob) = args.first() else {
        let message = format!(
            "Failed to execute '{}' on 'FileReader': 1 argument required, but only 0 present.",
            how.name()
        );
        return Err(scope.type_error(&message));
    };
    if !is_blob(scope, blob) {
        let message = format!(
            "Failed to execute '{}' on 'FileReader': parameter 1 is not of type 'Blob'.",
            how.name()
        );
        return Err(scope.type_error(&message));
    }
    if ready_state(scope, this) == LOADING {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidStateError",
            INVALID_STATE_ERR,
            c"Failed to execute 'read' on 'FileReader': The object is already busy reading Blobs.",
        ));
    }
    let bytes = crate::blob::blob_bytes(scope, blob);
    let pending = match how {
        Read::ArrayBuffer => scope.new_array_buffer(&bytes)?,
        Read::BinaryString => crate::latin1_string(scope, &bytes),
        Read::DataUrl => data_url(scope, blob, &bytes),
        Read::Text => scope.string_from_bytes(&bytes),
    };
    let generation = int64_prop(scope, this, "_gen", 0) + 1;
    set(scope, this, "_gen", Value::int64(generation));
    set(scope, this, "_total", Value::int64(bytes.len() as i64));
    set(scope, this, "_pending", pending);
    set(scope, this, "result", Value::null());
    set(scope, this, "error", Value::null());
    set(scope, this, "readyState", Value::int(LOADING));
    schedule(scope, this, generation);
    Ok(Value::undefined())
}

fn read_as_array_buffer(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    read(scope, this, args, Read::ArrayBuffer)
}

fn read_as_binary_string(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    read(scope, this, args, Read::BinaryString)
}

fn read_as_data_url(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    read(scope, this, args, Read::DataUrl)
}

fn read_as_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    read(scope, this, args, Read::Text)
}

fn abort(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if !ffi::host_is(this, HO_FILE_READER) {
        return Err(illegal_invocation(scope));
    }
    if ready_state(scope, this) != LOADING {
        set(scope, this, "result", Value::null());
        return Ok(Value::undefined());
    }
    let generation = int64_prop(scope, this, "_gen", 0);
    set(scope, this, "_gen", Value::int64(generation + 1));
    set(scope, this, "_pending", Value::undefined());
    set(scope, this, "readyState", Value::int(DONE));
    set(scope, this, "result", Value::null());
    let error = ffi::abort_error(scope);
    set(scope, this, "error", error);
    fire(scope, this, c"abort");
    fire(scope, this, c"loadend");
    Ok(Value::undefined())
}

pub(crate) fn ctor(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let object = ffi::host_construct(scope, this, HO_FILE_READER)?;
    set(scope, &object, "result", Value::null());
    set(scope, &object, "error", Value::null());
    set(scope, &object, "readyState", Value::int(EMPTY));
    set(scope, &object, "_gen", Value::int(0));
    for handler in HANDLERS {
        set(scope, &object, handler, Value::null());
    }
    Ok(object)
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let ctor = get(scope, global, "FileReader");
    let proto = if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    if !proto.is_object() {
        return;
    }
    let reads: [(Read, southstar_js_engine::NativeFn); 4] = [
        (Read::ArrayBuffer, read_as_array_buffer),
        (Read::BinaryString, read_as_binary_string),
        (Read::DataUrl, read_as_data_url),
        (Read::Text, read_as_text),
    ];
    for (how, f) in reads {
        let function = scope.function(how.name(), 1, f);
        let _ = scope.define(&proto, how.name(), function, ALL);
    }
    bind_fn(scope, &proto, "abort", 0, abort);
    for (name, value) in [("EMPTY", EMPTY), ("LOADING", LOADING), ("DONE", DONE)] {
        let _ = scope.define(&ctor, name, Value::int(value), Attributes::ENUMERABLE);
        let _ = scope.define(&proto, name, Value::int(value), Attributes::ENUMERABLE);
    }
}
