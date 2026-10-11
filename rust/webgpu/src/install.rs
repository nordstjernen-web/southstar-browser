//! Southstar — navigator.gpu and the GPUBufferUsage, GPUTextureUsage, GPUShaderStage, GPUColorWrite and GPUMapMode globals.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::sync::atomic::Ordering;

use southstar_js_engine::{Scope, Value};

use crate::context::INSTALLED;
use crate::gpu;
use crate::util::{bind, feature_set, set};

const BUFFER_USAGE: &[(&str, i32)] = &[
    ("MAP_READ", 0x0001),
    ("MAP_WRITE", 0x0002),
    ("COPY_SRC", 0x0004),
    ("COPY_DST", 0x0008),
    ("INDEX", 0x0010),
    ("VERTEX", 0x0020),
    ("UNIFORM", 0x0040),
    ("STORAGE", 0x0080),
    ("INDIRECT", 0x0100),
    ("QUERY_RESOLVE", 0x0200),
];

const TEXTURE_USAGE: &[(&str, i32)] = &[
    ("COPY_SRC", 0x01),
    ("COPY_DST", 0x02),
    ("TEXTURE_BINDING", 0x04),
    ("STORAGE_BINDING", 0x08),
    ("RENDER_ATTACHMENT", 0x10),
];

const SHADER_STAGE: &[(&str, i32)] = &[("VERTEX", 0x1), ("FRAGMENT", 0x2), ("COMPUTE", 0x4)];

const COLOR_WRITE: &[(&str, i32)] = &[
    ("RED", 0x1),
    ("GREEN", 0x2),
    ("BLUE", 0x4),
    ("ALPHA", 0x8),
    ("ALL", 0xF),
];

const MAP_MODE: &[(&str, i32)] = &[("READ", 0x1), ("WRITE", 0x2)];

fn constants(scope: &mut Scope<'_>, global: &Value, name: &str, entries: &[(&str, i32)]) {
    let object = scope.new_object();
    for &(key, value) in entries {
        set(scope, &object, key, Value::int(value));
    }
    set(scope, global, name, object);
}

pub(crate) fn install(scope: &mut Scope<'_>, navigator: &Value) {
    INSTALLED.store(true, Ordering::Relaxed);
    let gpu_object = scope.new_object();
    bind(
        scope,
        &gpu_object,
        "requestAdapter",
        gpu::request_adapter,
        1,
    );
    bind(
        scope,
        &gpu_object,
        "getPreferredCanvasFormat",
        gpu::preferred_canvas_format,
        0,
    );
    let features = feature_set(scope);
    set(scope, &gpu_object, "wgslLanguageFeatures", features);
    set(scope, navigator, "gpu", gpu_object);

    let global = scope.global();
    constants(scope, &global, "GPUBufferUsage", BUFFER_USAGE);
    constants(scope, &global, "GPUTextureUsage", TEXTURE_USAGE);
    constants(scope, &global, "GPUShaderStage", SHADER_STAGE);
    constants(scope, &global, "GPUColorWrite", COLOR_WRITE);
    constants(scope, &global, "GPUMapMode", MAP_MODE);
}
