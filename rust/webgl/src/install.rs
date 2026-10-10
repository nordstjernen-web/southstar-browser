//! Southstar — the WebGL interface objects, their method tables and accessors, and context creation for getContext.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::args::{JsResult, context};
use crate::consts::{RGB8, RGBA8};
use crate::context::{Attributes as ContextAttributes, WebGl};
use crate::ffi::host::{self, GlContext};
use crate::limits::MAX_CONTEXTS;
use crate::methods::{self as m, HIDDEN_ACTIVEINFO, HIDDEN_PRECISION};
use crate::methods2 as m2;
use crate::permission;
use crate::tables::{WEBGL1, WEBGL2};

type Method = (&'static str, NativeFn, u32);

const METHODS1: &[Method] = &[
    ("getContextAttributes", m::get_context_attributes, 0),
    ("isContextLost", m::is_context_lost, 0),
    ("getSupportedExtensions", m::get_supported_extensions, 0),
    ("getExtension", m::get_extension, 1),
    ("getParameter", m::get_parameter, 1),
    ("getError", m::get_error, 0),
    ("clearColor", m::clear_color, 4),
    ("clearDepth", m::clear_depth, 1),
    ("clearStencil", m::clear_stencil, 1),
    ("clear", m::clear, 1),
    ("viewport", m::viewport, 4),
    ("scissor", m::scissor, 4),
    ("enable", m::enable, 1),
    ("disable", m::disable, 1),
    ("isEnabled", m::is_enabled, 1),
    ("depthFunc", m::depth_func, 1),
    ("depthMask", m::depth_mask, 1),
    ("depthRange", m::depth_range, 2),
    ("colorMask", m::color_mask, 4),
    ("stencilMask", m::stencil_mask, 1),
    ("stencilFunc", m::stencil_func, 3),
    ("stencilOp", m::stencil_op, 3),
    ("blendFunc", m::blend_func, 2),
    ("blendFuncSeparate", m::blend_func_separate, 4),
    ("blendEquation", m::blend_equation, 1),
    ("blendEquationSeparate", m::blend_equation_separate, 2),
    ("blendColor", m::blend_color, 4),
    ("cullFace", m::cull_face, 1),
    ("frontFace", m::front_face, 1),
    ("lineWidth", m::line_width, 1),
    ("polygonOffset", m::polygon_offset, 2),
    ("hint", m::hint, 2),
    ("finish", m::finish, 0),
    ("flush", m::flush, 0),
    ("pixelStorei", m::pixel_storei, 2),
    ("sampleCoverage", m::sample_coverage, 2),
    ("stencilFuncSeparate", m::stencil_func_separate, 4),
    ("stencilOpSeparate", m::stencil_op_separate, 4),
    ("stencilMaskSeparate", m::stencil_mask_separate, 2),
    ("activeTexture", m::active_texture, 1),
    ("createShader", m::create_shader, 1),
    ("deleteShader", m::delete_shader, 1),
    ("shaderSource", m::shader_source, 2),
    ("compileShader", m::compile_shader, 1),
    ("getShaderParameter", m::get_shader_parameter, 2),
    ("getShaderInfoLog", m::get_shader_info_log, 1),
    ("getShaderSource", m::get_shader_source, 1),
    ("createProgram", m::create_program, 0),
    ("deleteProgram", m::delete_program, 1),
    ("attachShader", m::attach_shader, 2),
    ("detachShader", m::detach_shader, 2),
    ("linkProgram", m::link_program, 1),
    ("validateProgram", m::validate_program, 1),
    ("useProgram", m::use_program, 1),
    ("getProgramParameter", m::get_program_parameter, 2),
    ("getProgramInfoLog", m::get_program_info_log, 1),
    ("bindAttribLocation", m::bind_attrib_location, 3),
    ("getAttribLocation", m::get_attrib_location, 2),
    ("getUniformLocation", m::get_uniform_location, 2),
    ("getActiveAttrib", m::get_active_attrib, 2),
    (
        "getShaderPrecisionFormat",
        m::get_shader_precision_format,
        2,
    ),
    ("getActiveUniform", m::get_active_uniform, 2),
    ("createBuffer", m::create_buffer, 0),
    ("deleteBuffer", m::delete_buffer, 1),
    ("bindBuffer", m::bind_buffer, 2),
    ("bufferData", m::buffer_data, 3),
    ("bufferSubData", m::buffer_sub_data, 3),
    ("enableVertexAttribArray", m::enable_vertex_attrib_array, 1),
    (
        "disableVertexAttribArray",
        m::disable_vertex_attrib_array,
        1,
    ),
    ("vertexAttribPointer", m::vertex_attrib_pointer, 6),
    ("vertexAttrib1f", m::vertex_attrib1f, 2),
    ("vertexAttrib2f", m::vertex_attrib2f, 3),
    ("vertexAttrib3f", m::vertex_attrib3f, 4),
    ("vertexAttrib4f", m::vertex_attrib4f, 5),
    ("uniform1f", m::uniform1f, 2),
    ("uniform2f", m::uniform2f, 3),
    ("uniform3f", m::uniform3f, 4),
    ("uniform4f", m::uniform4f, 5),
    ("uniform1i", m::uniform1i, 2),
    ("uniform2i", m::uniform2i, 3),
    ("uniform3i", m::uniform3i, 4),
    ("uniform4i", m::uniform4i, 5),
    ("uniform1fv", m::uniform1fv, 2),
    ("uniform2fv", m::uniform2fv, 2),
    ("uniform3fv", m::uniform3fv, 2),
    ("uniform4fv", m::uniform4fv, 2),
    ("uniform1iv", m::uniform1iv, 2),
    ("uniform2iv", m::uniform2iv, 2),
    ("uniform3iv", m::uniform3iv, 2),
    ("uniform4iv", m::uniform4iv, 2),
    ("uniformMatrix2fv", m::uniform_matrix2fv, 3),
    ("uniformMatrix3fv", m::uniform_matrix3fv, 3),
    ("uniformMatrix4fv", m::uniform_matrix4fv, 3),
    ("drawArrays", m::draw_arrays, 3),
    ("drawElements", m::draw_elements, 4),
    ("createTexture", m::create_texture, 0),
    ("deleteTexture", m::delete_texture, 1),
    ("bindTexture", m::bind_texture, 2),
    ("texParameteri", m::tex_parameter_i, 3),
    ("texParameterf", m::tex_parameter_f, 3),
    ("generateMipmap", m::generate_mipmap, 1),
    ("texImage2D", m::tex_image_2d, 6),
    ("texSubImage2D", m::tex_sub_image_2d, 7),
    ("createFramebuffer", m::create_framebuffer, 0),
    ("deleteFramebuffer", m::delete_framebuffer, 1),
    ("bindFramebuffer", m::bind_framebuffer, 2),
    ("framebufferTexture2D", m::framebuffer_texture_2d, 5),
    ("framebufferRenderbuffer", m::framebuffer_renderbuffer, 4),
    ("checkFramebufferStatus", m::check_framebuffer_status, 1),
    ("createRenderbuffer", m::create_renderbuffer, 0),
    ("deleteRenderbuffer", m::delete_renderbuffer, 1),
    ("bindRenderbuffer", m::bind_renderbuffer, 2),
    ("renderbufferStorage", m::renderbuffer_storage, 4),
    ("readPixels", m::read_pixels, 7),
    ("isBuffer", m::is_buffer, 1),
    ("isProgram", m::is_program, 1),
    ("isShader", m::is_shader, 1),
    ("isTexture", m::is_texture, 1),
    ("isFramebuffer", m::is_framebuffer, 1),
    ("isRenderbuffer", m::is_renderbuffer, 1),
    ("getBufferParameter", m::get_buffer_parameter, 2),
    ("getTexParameter", m::get_tex_parameter, 2),
    ("getRenderbufferParameter", m::get_renderbuffer_parameter, 2),
    (
        "getFramebufferAttachmentParameter",
        m::get_framebuffer_attachment_parameter,
        3,
    ),
    ("getVertexAttrib", m::get_vertex_attrib, 2),
    ("getVertexAttribOffset", m::get_vertex_attrib_offset, 2),
    ("getUniform", m::get_uniform, 2),
    ("vertexAttrib1fv", m::vertex_attrib1fv, 2),
    ("vertexAttrib2fv", m::vertex_attrib2fv, 2),
    ("vertexAttrib3fv", m::vertex_attrib3fv, 2),
    ("vertexAttrib4fv", m::vertex_attrib4fv, 2),
    ("compressedTexImage2D", m::compressed_unsupported, 7),
    ("compressedTexSubImage2D", m::compressed_unsupported, 8),
    ("getAttachedShaders", m::get_attached_shaders, 1),
    ("drawingBufferStorage", m::drawing_buffer_storage, 3),
    ("makeXRCompatible", m::make_xr_compatible, 0),
    ("copyTexImage2D", m::copy_tex_image_2d, 8),
    ("copyTexSubImage2D", m::copy_tex_sub_image_2d, 8),
];

const METHODS2: &[Method] = &[
    ("createVertexArray", m2::create_vertex_array, 0),
    ("deleteVertexArray", m2::delete_vertex_array, 1),
    ("bindVertexArray", m2::bind_vertex_array, 1),
    ("isVertexArray", m2::is_vertex_array, 1),
    ("drawArraysInstanced", m2::draw_arrays_instanced, 4),
    ("drawElementsInstanced", m2::draw_elements_instanced, 5),
    ("vertexAttribDivisor", m2::vertex_attrib_divisor, 2),
    ("drawBuffers", m2::draw_buffers, 1),
    ("vertexAttribIPointer", m2::vertex_attrib_i_pointer, 5),
    ("uniform1ui", m2::uniform1ui, 2),
    ("uniform2ui", m2::uniform2ui, 3),
    ("uniform3ui", m2::uniform3ui, 4),
    ("uniform4ui", m2::uniform4ui, 5),
    ("uniform1uiv", m2::uniform1uiv, 2),
    ("uniform2uiv", m2::uniform2uiv, 2),
    ("uniform3uiv", m2::uniform3uiv, 2),
    ("uniform4uiv", m2::uniform4uiv, 2),
    ("uniformMatrix2x3fv", m2::uniform_matrix2x3fv, 3),
    ("uniformMatrix3x2fv", m2::uniform_matrix3x2fv, 3),
    ("uniformMatrix2x4fv", m2::uniform_matrix2x4fv, 3),
    ("uniformMatrix4x2fv", m2::uniform_matrix4x2fv, 3),
    ("uniformMatrix3x4fv", m2::uniform_matrix3x4fv, 3),
    ("uniformMatrix4x3fv", m2::uniform_matrix4x3fv, 3),
    ("texStorage2D", m2::tex_storage_2d, 5),
    (
        "renderbufferStorageMultisample",
        m2::renderbuffer_storage_multisample,
        5,
    ),
    ("blitFramebuffer", m2::blit_framebuffer, 10),
    ("framebufferTextureLayer", m2::framebuffer_texture_layer, 5),
    ("invalidateFramebuffer", m2::invalidate_framebuffer, 2),
    ("readBuffer", m2::read_buffer, 1),
    ("copyBufferSubData", m2::copy_buffer_sub_data, 5),
    ("getBufferSubData", m2::get_buffer_sub_data, 3),
    ("clearBufferfv", m2::clear_buffer_fv, 3),
    ("clearBufferiv", m2::clear_buffer_iv, 3),
    ("clearBufferuiv", m2::clear_buffer_uiv, 3),
    ("clearBufferfi", m2::clear_bufferfi, 4),
    ("createSampler", m2::create_sampler, 0),
    ("deleteSampler", m2::delete_sampler, 1),
    ("bindSampler", m2::bind_sampler, 2),
    ("samplerParameteri", m2::sampler_parameter_i, 3),
    ("samplerParameterf", m2::sampler_parameter_f, 3),
    ("isSampler", m2::is_sampler, 1),
    ("getUniformBlockIndex", m2::get_uniform_block_index, 2),
    ("uniformBlockBinding", m2::uniform_block_binding, 3),
    ("bindBufferBase", m2::bind_buffer_base, 3),
    ("bindBufferRange", m2::bind_buffer_range, 5),
    ("copyTexSubImage3D", m2::copy_tex_sub_image_3d, 9),
    ("drawRangeElements", m2::draw_range_elements, 6),
    ("vertexAttribI4i", m2::vertex_attrib_i4i, 5),
    ("vertexAttribI4ui", m2::vertex_attrib_i4ui, 5),
    ("vertexAttribI4iv", m2::vertex_attrib_i4iv, 2),
    ("vertexAttribI4uiv", m2::vertex_attrib_i4uiv, 2),
    ("getFragDataLocation", m2::get_frag_data_location, 2),
    (
        "getInternalformatParameter",
        m2::get_internalformat_parameter,
        3,
    ),
    ("texImage3D", m2::tex_image_3d, 10),
    ("texSubImage3D", m2::tex_sub_image_3d, 11),
    ("texStorage3D", m2::tex_storage_3d, 6),
    ("createQuery", m2::create_query, 0),
    ("deleteQuery", m2::delete_query, 1),
    ("isQuery", m2::is_query, 1),
    ("beginQuery", m2::begin_query, 2),
    ("endQuery", m2::end_query, 1),
    ("getQuery", m2::get_query, 2),
    ("getQueryParameter", m2::get_query_parameter, 2),
    ("createTransformFeedback", m2::create_transform_feedback, 0),
    ("deleteTransformFeedback", m2::delete_transform_feedback, 1),
    ("isTransformFeedback", m2::is_transform_feedback, 1),
    ("bindTransformFeedback", m2::bind_transform_feedback, 2),
    ("beginTransformFeedback", m2::begin_transform_feedback, 1),
    ("endTransformFeedback", m2::end_transform_feedback, 0),
    ("pauseTransformFeedback", m2::pause_transform_feedback, 0),
    ("resumeTransformFeedback", m2::resume_transform_feedback, 0),
    (
        "transformFeedbackVaryings",
        m2::transform_feedback_varyings,
        3,
    ),
    ("getActiveUniforms", m2::get_active_uniforms, 3),
    (
        "getActiveUniformBlockParameter",
        m2::get_active_uniform_block_parameter,
        3,
    ),
    (
        "getActiveUniformBlockName",
        m2::get_active_uniform_block_name,
        2,
    ),
    ("fenceSync", m2::fence_sync, 2),
    ("isSync", m2::is_sync, 1),
    ("deleteSync", m2::delete_sync, 1),
    ("clientWaitSync", m2::client_wait_sync, 3),
    ("waitSync", m2::wait_sync, 3),
    ("getSyncParameter", m2::get_sync_parameter, 2),
    ("compressedTexImage3D", m::compressed_unsupported, 8),
    ("compressedTexSubImage3D", m::compressed_unsupported, 10),
    ("getIndexedParameter", m2::get_indexed_parameter, 2),
    ("getSamplerParameter", m2::get_sampler_parameter, 2),
    (
        "getTransformFeedbackVarying",
        m::get_transform_feedback_varying,
        2,
    ),
    ("getUniformIndices", m2::get_uniform_indices, 2),
    (
        "invalidateSubFramebuffer",
        m2::invalidate_sub_framebuffer,
        6,
    ),
];

const V2_METHODS: i32 = 0x4000;

const INTERFACES: [&str; 17] = [
    "WebGLObject",
    "WebGLBuffer",
    "WebGLFramebuffer",
    "WebGLProgram",
    "WebGLRenderbuffer",
    "WebGLShader",
    "WebGLTexture",
    "WebGLQuery",
    "WebGLSampler",
    "WebGLSync",
    "WebGLTransformFeedback",
    "WebGLVertexArrayObject",
    "WebGLUniformLocation",
    "WebGLActiveInfo",
    "WebGLShaderPrecisionFormat",
    "WebGLRenderingContext",
    "WebGL2RenderingContext",
];

const INFO_FIELDS: [(i32, [&str; 3]); 2] = [
    (HIDDEN_ACTIVEINFO, ["name", "size", "type"]),
    (HIDDEN_PRECISION, ["precision", "rangeMax", "rangeMin"]),
];

const ACCESSOR: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

fn method(index: i32) -> &'static Method {
    if index >= V2_METHODS {
        &METHODS2[(index - V2_METHODS) as usize]
    } else {
        &METHODS1[index as usize]
    }
}

fn dispatch(s: &mut Scope<'_>, this: &Value, args: &[Value], data: &[Value]) -> JsResult {
    let index = match data.first() {
        Some(v) => s.to_int32(v).unwrap_or(0),
        None => 0,
    };
    let (name, f, length) = *method(index);
    if (args.len() as u32) < length {
        let g = context(s, this)?;
        let iface = if g.version >= 2 {
            "WebGL2RenderingContext"
        } else {
            "WebGLRenderingContext"
        };
        let message = format!(
            "Failed to execute '{name}' on '{iface}': {length} argument{} required, but only {} present.",
            if length == 1 { "" } else { "s" },
            args.len()
        );
        return Err(s.type_error(&message));
    }
    f(s, this, args)
}

fn bind_methods(s: &mut Scope<'_>, proto: &Value, table: &[Method], base: i32) {
    for (i, &(name, _, length)) in table.iter().enumerate() {
        let f = s.bound_function(name, length, dispatch, &[Value::int(base + i as i32)]);
        let _ = s.set(proto, name, f);
    }
}

fn define_constants(s: &mut Scope<'_>, object: &Value, table: &[(&str, i64)]) {
    for &(name, value) in table {
        let _ = s.define(object, name, Value::int64(value), Attributes::ENUMERABLE);
    }
}

fn get_canvas(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    context(s, this)?;
    Ok(host::canvas_object_of(this).unwrap_or_else(Value::undefined))
}

fn synced(s: &mut Scope<'_>, this: &Value) -> Result<Rc<WebGl>, Value> {
    let g = context(s, this)?;
    g.enter_synced();
    Ok(g)
}

fn get_drawing_buffer_width(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = synced(s, this)?;
    Ok(Value::int(g.st.borrow().w))
}

fn get_drawing_buffer_height(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = synced(s, this)?;
    Ok(Value::int(g.st.borrow().h))
}

fn get_drawing_buffer_format(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = context(s, this)?;
    Ok(Value::int(if g.attrs.alpha { RGBA8 } else { RGB8 } as i32))
}

fn color_space(s: &mut Scope<'_>, p3: bool) -> JsResult {
    Ok(s.string(if p3 { "display-p3" } else { "srgb" }))
}

fn get_drawing_buffer_color_space(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = context(s, this)?;
    let p3 = g.st.borrow().drawing_p3;
    color_space(s, p3)
}

fn get_unpack_color_space(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = context(s, this)?;
    let p3 = g.st.borrow().unpack_p3;
    color_space(s, p3)
}

fn set_color_space(s: &mut Scope<'_>, this: &Value, a: &[Value], drawing: bool) -> JsResult {
    let g = context(s, this)?;
    let value = a.first().cloned().unwrap_or_else(Value::undefined);
    let text = s.to_bytes(&value)?;
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    let p3 = match &text[..end] {
        b"srgb" => Some(false),
        b"display-p3" => Some(true),
        _ => None,
    };
    if let Some(p3) = p3 {
        let mut st = g.st.borrow_mut();
        if drawing {
            st.drawing_p3 = p3;
        } else {
            st.unpack_p3 = p3;
        }
    }
    Ok(Value::undefined())
}

fn set_drawing_buffer_color_space(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    set_color_space(s, this, a, true)
}

fn set_unpack_color_space(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    set_color_space(s, this, a, false)
}

fn define_accessor(
    s: &mut Scope<'_>,
    proto: &Value,
    name: &str,
    getter: NativeFn,
    setter: Option<NativeFn>,
) {
    let get = s.function(&format!("get {name}"), 0, getter);
    let set = setter.map(|f| s.function(&format!("set {name}"), 1, f));
    let _ = s.define_accessor(proto, name, Some(&get), set.as_ref(), ACCESSOR);
}

fn bind_accessors(s: &mut Scope<'_>, proto: &Value) {
    define_accessor(s, proto, "canvas", get_canvas, None);
    define_accessor(
        s,
        proto,
        "drawingBufferWidth",
        get_drawing_buffer_width,
        None,
    );
    define_accessor(
        s,
        proto,
        "drawingBufferHeight",
        get_drawing_buffer_height,
        None,
    );
    define_accessor(
        s,
        proto,
        "drawingBufferFormat",
        get_drawing_buffer_format,
        None,
    );
    define_accessor(
        s,
        proto,
        "drawingBufferColorSpace",
        get_drawing_buffer_color_space,
        Some(set_drawing_buffer_color_space),
    );
    define_accessor(
        s,
        proto,
        "unpackColorSpace",
        get_unpack_color_space,
        Some(set_unpack_color_space),
    );
}

fn illegal_constructor<const N: usize>(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    if this.is_undefined() {
        return Err(s.type_error("Illegal constructor"));
    }
    let message = format!(
        "Failed to construct '{}': Illegal constructor",
        INTERFACES[N]
    );
    Err(s.type_error(&message))
}

const CONSTRUCTORS: [NativeFn; 17] = [
    illegal_constructor::<0>,
    illegal_constructor::<1>,
    illegal_constructor::<2>,
    illegal_constructor::<3>,
    illegal_constructor::<4>,
    illegal_constructor::<5>,
    illegal_constructor::<6>,
    illegal_constructor::<7>,
    illegal_constructor::<8>,
    illegal_constructor::<9>,
    illegal_constructor::<10>,
    illegal_constructor::<11>,
    illegal_constructor::<12>,
    illegal_constructor::<13>,
    illegal_constructor::<14>,
    illegal_constructor::<15>,
    illegal_constructor::<16>,
];

fn info_get<const IFACE: usize, const FIELD: usize>(
    s: &mut Scope<'_>,
    this: &Value,
    _a: &[Value],
) -> JsResult {
    let (kind, names) = INFO_FIELDS[IFACE];
    if !host::hidden_is(this, kind) {
        return Err(s.type_error("Illegal invocation"));
    }
    host::hidden_get(s, this, names[FIELD])
}

const INFO_GETTERS: [[NativeFn; 3]; 2] = [
    [info_get::<0, 0>, info_get::<0, 1>, info_get::<0, 2>],
    [info_get::<1, 0>, info_get::<1, 1>, info_get::<1, 2>],
];

fn install_one(
    s: &mut Scope<'_>,
    global: &Value,
    index: usize,
    parent: Option<&str>,
) -> (Value, Value) {
    let name = INTERFACES[index];
    let ctor = s.constructor_or_function(name, 0, CONSTRUCTORS[index]);
    let proto = host::api_interface(s, global, name, ctor.clone(), parent);
    (ctor, proto)
}

pub(crate) fn install(s: &mut Scope<'_>, global: &Value) {
    for i in 0..=12 {
        let parent = (1..=11).contains(&i).then_some("WebGLObject");
        install_one(s, global, i, parent);
    }
    for (i, getters) in INFO_GETTERS.iter().enumerate() {
        let (_, proto) = install_one(s, global, 13 + i, None);
        for (field, &getter) in INFO_FIELDS[i].1.iter().zip(getters) {
            let get = s.function(&format!("get {field}"), 0, getter);
            let _ = s.define_accessor(&proto, field, Some(&get), None, ACCESSOR);
        }
    }
    for version in 1..=2 {
        let (ctor, proto) = install_one(s, global, 14 + version, None);
        define_constants(s, &ctor, WEBGL1);
        define_constants(s, &proto, WEBGL1);
        bind_methods(s, &proto, METHODS1, 0);
        if version >= 2 {
            define_constants(s, &ctor, WEBGL2);
            define_constants(s, &proto, WEBGL2);
            bind_methods(s, &proto, METHODS2, V2_METHODS);
        }
        bind_accessors(s, &proto);
    }
}

fn attribute(s: &mut Scope<'_>, attrs: &Value, name: &str, default: bool) -> bool {
    if !attrs.is_object() {
        return default;
    }
    match s.get(attrs, name) {
        Ok(v) if !v.is_undefined() && !v.is_null() => s.to_bool(&v),
        _ => default,
    }
}

pub(crate) fn new_context(
    s: &mut Scope<'_>,
    js: usize,
    canvas: usize,
    version: i32,
    attrs: &Value,
) -> Option<(Rc<WebGl>, Value)> {
    if host::context_count() >= MAX_CONTEXTS || !permission::permitted(js) {
        return None;
    }
    let gl = GlContext::create()?;
    if !gl.make_current() {
        return None;
    }
    let attributes = ContextAttributes {
        alpha: attribute(s, attrs, "alpha", true),
        depth: attribute(s, attrs, "depth", true),
        stencil: attribute(s, attrs, "stencil", false),
        antialias: attribute(s, attrs, "antialias", true),
        preserve: attribute(s, attrs, "preserveDrawingBuffer", false),
        premultiplied_alpha: attribute(s, attrs, "premultipliedAlpha", true),
    };
    let g = WebGl::make(gl, js, canvas, version, attributes)?;
    let iface = if version >= 2 {
        "WebGL2RenderingContext"
    } else {
        "WebGLRenderingContext"
    };
    let proto = host::api_proto(s, canvas, iface);
    Some((g, proto))
}
