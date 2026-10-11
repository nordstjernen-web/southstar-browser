//! Southstar — WebGPU descriptor strings mapped to their wgpu-native enum values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::sys::*;

pub(crate) fn texture_format(name: Option<&str>) -> u32 {
    match name {
        Some("bgra8unorm-srgb") => TEXTURE_FORMAT_BGRA8_UNORM_SRGB,
        Some("rgba8unorm") => TEXTURE_FORMAT_RGBA8_UNORM,
        Some("rgba8unorm-srgb") => TEXTURE_FORMAT_RGBA8_UNORM_SRGB,
        Some("rgba16float") => TEXTURE_FORMAT_RGBA16_FLOAT,
        Some("rgba32float") => TEXTURE_FORMAT_RGBA32_FLOAT,
        Some("r8unorm") => TEXTURE_FORMAT_R8_UNORM,
        Some("rg8unorm") => TEXTURE_FORMAT_RG8_UNORM,
        Some("r16float") => TEXTURE_FORMAT_R16_FLOAT,
        Some("rg16float") => TEXTURE_FORMAT_RG16_FLOAT,
        Some("r32float") => TEXTURE_FORMAT_R32_FLOAT,
        Some("rg32float") => TEXTURE_FORMAT_RG32_FLOAT,
        Some("rgb10a2unorm") => TEXTURE_FORMAT_RGB10_A2_UNORM,
        Some("depth16unorm") => TEXTURE_FORMAT_DEPTH16_UNORM,
        Some("depth24plus") => TEXTURE_FORMAT_DEPTH24_PLUS,
        Some("depth24plus-stencil8") => TEXTURE_FORMAT_DEPTH24_PLUS_STENCIL8,
        Some("depth32float") => TEXTURE_FORMAT_DEPTH32_FLOAT,
        _ => TEXTURE_FORMAT_BGRA8_UNORM,
    }
}

pub(crate) fn format_name(format: u32) -> &'static str {
    if format == TEXTURE_FORMAT_RGBA8_UNORM {
        "rgba8unorm"
    } else {
        "bgra8unorm"
    }
}

pub(crate) fn compare_function(name: Option<&str>) -> u32 {
    match name {
        Some("never") => COMPARE_FUNCTION_NEVER,
        Some("less") => COMPARE_FUNCTION_LESS,
        Some("equal") => COMPARE_FUNCTION_EQUAL,
        Some("less-equal") => COMPARE_FUNCTION_LESS_EQUAL,
        Some("greater") => COMPARE_FUNCTION_GREATER,
        Some("greater-equal") => COMPARE_FUNCTION_GREATER_EQUAL,
        Some("always") => COMPARE_FUNCTION_ALWAYS,
        _ => COMPARE_FUNCTION_UNDEFINED,
    }
}

pub(crate) fn address_mode(name: &str) -> u32 {
    match name {
        "repeat" => ADDRESS_MODE_REPEAT,
        "mirror-repeat" => ADDRESS_MODE_MIRROR_REPEAT,
        _ => ADDRESS_MODE_CLAMP_TO_EDGE,
    }
}

pub(crate) fn blend_factor(name: Option<&str>) -> u32 {
    match name {
        Some("zero") => BLEND_FACTOR_ZERO,
        Some("src") => BLEND_FACTOR_SRC,
        Some("one-minus-src") => BLEND_FACTOR_ONE_MINUS_SRC,
        Some("src-alpha") => BLEND_FACTOR_SRC_ALPHA,
        Some("one-minus-src-alpha") => BLEND_FACTOR_ONE_MINUS_SRC_ALPHA,
        Some("dst") => BLEND_FACTOR_DST,
        Some("one-minus-dst") => BLEND_FACTOR_ONE_MINUS_DST,
        Some("dst-alpha") => BLEND_FACTOR_DST_ALPHA,
        Some("one-minus-dst-alpha") => BLEND_FACTOR_ONE_MINUS_DST_ALPHA,
        Some("src-alpha-saturated") => BLEND_FACTOR_SRC_ALPHA_SATURATED,
        Some("constant") => BLEND_FACTOR_CONSTANT,
        Some("one-minus-constant") => BLEND_FACTOR_ONE_MINUS_CONSTANT,
        _ => BLEND_FACTOR_ONE,
    }
}

pub(crate) fn blend_operation(name: Option<&str>) -> u32 {
    match name {
        Some("subtract") => BLEND_OPERATION_SUBTRACT,
        Some("reverse-subtract") => BLEND_OPERATION_REVERSE_SUBTRACT,
        Some("min") => BLEND_OPERATION_MIN,
        Some("max") => BLEND_OPERATION_MAX,
        _ => BLEND_OPERATION_ADD,
    }
}

pub(crate) fn view_dimension(name: &str) -> u32 {
    match name {
        "1d" => TEXTURE_VIEW_DIMENSION_1_D,
        "2d" => TEXTURE_VIEW_DIMENSION_2_D,
        "2d-array" => TEXTURE_VIEW_DIMENSION_2_D_ARRAY,
        "cube" => TEXTURE_VIEW_DIMENSION_CUBE,
        "cube-array" => TEXTURE_VIEW_DIMENSION_CUBE_ARRAY,
        "3d" => TEXTURE_VIEW_DIMENSION_3_D,
        _ => TEXTURE_VIEW_DIMENSION_UNDEFINED,
    }
}

pub(crate) fn vertex_format(name: Option<&str>) -> u32 {
    match name {
        Some("float32x2") => VERTEX_FORMAT_FLOAT32X2,
        Some("float32x4") => VERTEX_FORMAT_FLOAT32X4,
        Some("float32") => VERTEX_FORMAT_FLOAT32,
        Some("uint32") => VERTEX_FORMAT_UINT32,
        _ => VERTEX_FORMAT_FLOAT32X3,
    }
}

pub(crate) fn topology(name: Option<&str>) -> u32 {
    match name {
        Some("triangle-strip") => PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP,
        Some("line-list") => PRIMITIVE_TOPOLOGY_LINE_LIST,
        Some("line-strip") => PRIMITIVE_TOPOLOGY_LINE_STRIP,
        Some("point-list") => PRIMITIVE_TOPOLOGY_POINT_LIST,
        _ => PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
    }
}

pub(crate) fn buffer_binding_type(name: Option<&str>) -> u32 {
    match name {
        Some("storage") => BUFFER_BINDING_TYPE_STORAGE,
        Some("read-only-storage") => BUFFER_BINDING_TYPE_READ_ONLY_STORAGE,
        _ => BUFFER_BINDING_TYPE_UNIFORM,
    }
}

pub(crate) fn sampler_binding_type(name: Option<&str>) -> u32 {
    match name {
        Some("non-filtering") => SAMPLER_BINDING_TYPE_NON_FILTERING,
        Some("comparison") => SAMPLER_BINDING_TYPE_COMPARISON,
        _ => SAMPLER_BINDING_TYPE_FILTERING,
    }
}

pub(crate) fn sample_type(name: Option<&str>) -> u32 {
    match name {
        Some("unfilterable-float") => TEXTURE_SAMPLE_TYPE_UNFILTERABLE_FLOAT,
        Some("depth") => TEXTURE_SAMPLE_TYPE_DEPTH,
        Some("uint") => TEXTURE_SAMPLE_TYPE_UINT,
        _ => TEXTURE_SAMPLE_TYPE_FLOAT,
    }
}
