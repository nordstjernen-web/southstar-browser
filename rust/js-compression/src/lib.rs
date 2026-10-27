//! Southstar — the zlib codec behind CompressionStream and DecompressionStream: gzip, deflate and deflate-raw.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::cell::{Cell, RefCell};

use southstar_js_engine::{Attributes, NativeFn, ObjectKind, Scope, Value};

use crate::ffi::{Step, Zlib};

pub use ffi::nsr_compression_install;

const MAX_OUTPUT: usize = 256 * 1024 * 1024;

const CHUNK: usize = 16384;

struct Codec {
    zlib: RefCell<Zlib>,
    ended: Cell<bool>,
}

fn window_bits(format: &[u8]) -> Option<i32> {
    match format {
        b"gzip" => Some(15 + 16),
        b"deflate" => Some(15),
        b"deflate-raw" => Some(-15),
        _ => None,
    }
}

fn create(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 2 {
        return Err(scope.type_error("zlib codec: format and mode required"));
    }
    let mut format = scope.to_bytes(&args[0])?;
    if let Some(nul) = format.iter().position(|&c| c == 0) {
        format.truncate(nul);
    }
    let Some(bits) = window_bits(&format) else {
        return Err(scope.type_error("Unsupported compression format"));
    };
    let decompress = scope.to_bool(&args[1]);
    let Some(zlib) = Zlib::new(decompress, bits) else {
        return Err(ffi::internal_error(scope, "zlib initialization failed"));
    };
    let codec = Codec {
        zlib: RefCell::new(zlib),
        ended: Cell::new(false),
    };
    Ok(scope.new_host_object(None, codec))
}

fn direction(decompress: bool) -> &'static str {
    if decompress {
        "decompression"
    } else {
        "compression"
    }
}

fn run(scope: &mut Scope<'_>, codec: &Codec, input: &[u8], finish: bool) -> Result<Value, Value> {
    if codec.ended.get() {
        return scope.new_array_buffer(&[]);
    }
    let mut zlib = codec.zlib.borrow_mut();
    let mut out = Vec::new();
    let mut buf = [0u8; CHUNK];
    let mut rest = input;
    loop {
        let (step, produced) = zlib.step(&mut rest, &mut buf, finish);
        if let Step::Failed = step {
            let message = format!("zlib {} error", direction(zlib.decompress()));
            return Err(scope.type_error(&message));
        }
        if produced > MAX_OUTPUT - out.len() {
            let message = format!("zlib {} output too large", direction(zlib.decompress()));
            return Err(scope.range_error(&message));
        }
        out.extend_from_slice(&buf[..produced]);
        match step {
            Step::StreamEnd => {
                codec.ended.set(true);
                break;
            }
            Step::BufError => break,
            _ if produced < CHUNK => break,
            _ => {}
        }
    }
    drop(zlib);
    scope.new_array_buffer(&out)
}

fn is_codec(value: &Value) -> bool {
    value.with_host(|_: &Codec| ()).is_some()
}

fn data_bytes(scope: &mut Scope<'_>, data: &Value) -> Result<Vec<u8>, Value> {
    match scope.object_kind(data) {
        Some(ObjectKind::TypedArray(_) | ObjectKind::DataView) => scope
            .view_data(data)
            .ok_or_else(|| ffi::internal_error(scope, "zlib push: backing buffer unavailable")),
        Some(ObjectKind::ArrayBuffer) => scope
            .array_buffer_bytes(data)
            .ok_or_else(|| scope.type_error("zlib push: ArrayBufferView expected")),
        _ => Err(scope.type_error("zlib push: ArrayBufferView expected")),
    }
}

fn push(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.len() < 2 {
        return Err(scope.type_error("zlib push: codec and data required"));
    }
    if !is_codec(&args[0]) {
        return Err(scope.type_error("zlib push: invalid codec"));
    }
    let bytes = data_bytes(scope, &args[1])?;
    args[0]
        .with_host(|codec: &Codec| run(scope, codec, &bytes, false))
        .unwrap_or_else(|| Ok(Value::undefined()))
}

fn finish(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    if args.is_empty() {
        return Err(scope.type_error("zlib finish: codec required"));
    }
    args[0]
        .with_host(|codec: &Codec| run(scope, codec, &[], true))
        .unwrap_or_else(|| Err(scope.type_error("zlib finish: invalid codec")))
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let natives: [(&str, u32, NativeFn); 3] = [
        ("__ns_zlib_create", 2, create),
        ("__ns_zlib_push", 2, push),
        ("__ns_zlib_finish", 1, finish),
    ];
    let hidden = Attributes {
        writable: true,
        enumerable: false,
        configurable: true,
    };
    for (name, arity, f) in natives {
        let function = scope.function(name, arity, f);
        let _ = scope.define(global, name, function, hidden);
    }
}
