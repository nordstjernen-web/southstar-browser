//! Southstar — the WebGLRenderingContext methods: state, shaders, buffers, textures, framebuffers and draws.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ElementType, Scope, Value};

use crate::args::*;
use crate::consts::*;
use crate::context::WebGl;
use crate::ffi::gl;
use crate::ffi::host::{self, GlObject};
use crate::limits::{self, MAX_ALLOC, MAX_SHADER};

pub(crate) const HIDDEN_ACTIVEINFO: i32 = 7;
pub(crate) const HIDDEN_PRECISION: i32 = 8;

pub(crate) const OBJECT_IFACES: [&str; 12] = [
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
];

pub(crate) const KIND_BUFFER: u8 = 0;
pub(crate) const KIND_FRAMEBUFFER: u8 = 1;
pub(crate) const KIND_PROGRAM: u8 = 2;
pub(crate) const KIND_RENDERBUFFER: u8 = 3;
pub(crate) const KIND_SHADER: u8 = 4;
pub(crate) const KIND_TEXTURE: u8 = 5;
pub(crate) const KIND_QUERY: u8 = 6;
pub(crate) const KIND_SAMPLER: u8 = 7;
pub(crate) const KIND_TRANSFORM_FEEDBACK: u8 = 9;
pub(crate) const KIND_VERTEX_ARRAY: u8 = 10;

pub(crate) fn undefined() -> JsResult {
    Ok(Value::undefined())
}

pub(crate) fn new_object(s: &mut Scope<'_>, g: &WebGl, name: u32, kind: u8) -> Value {
    let proto = host::api_proto(s, g.canvas, OBJECT_IFACES[kind as usize]);
    s.new_host_object(proto.is_object().then_some(&proto), GlObject { kind, name })
}

pub(crate) fn wrap(s: &mut Scope<'_>, g: &WebGl, name: u32, kind: u8) -> Value {
    if name == 0 {
        Value::null()
    } else {
        new_object(s, g, name, kind)
    }
}

pub(crate) fn words<T: Copy, const N: usize>(v: &[T], to: fn(T) -> [u8; N]) -> Vec<u8> {
    v.iter().flat_map(|&x| to(x)).collect()
}

pub(crate) fn int_array(s: &mut Scope<'_>, v: &[i32]) -> JsResult {
    s.new_typed_array(ElementType::Int32, &words(v, i32::to_ne_bytes))
}

pub(crate) fn uint_array(s: &mut Scope<'_>, v: &[i32]) -> JsResult {
    s.new_typed_array(ElementType::Uint32, &words(v, i32::to_ne_bytes))
}

pub(crate) fn float_array(s: &mut Scope<'_>, v: &[f32]) -> JsResult {
    s.new_typed_array(ElementType::Float32, &words(v, f32::to_ne_bytes))
}

pub(crate) fn uint_value(v: u32) -> Value {
    Value::int64(i64::from(v))
}

pub(crate) fn active_info(s: &mut Scope<'_>, g: &WebGl, active: gl::Active) -> Value {
    let o = host::new_info(s, g.canvas, HIDDEN_ACTIVEINFO, "WebGLActiveInfo");
    host::hidden_set(s, &o, "size", Value::int(active.size));
    host::hidden_set(s, &o, "type", Value::int(active.kind as i32));
    let name = s.string_from_bytes(&active.name);
    host::hidden_set(s, &o, "name", name);
    o
}

macro_rules! simple {
    ($($name:ident(|$s:ident, $a:ident| $body:expr);)*) => {
        $(
            pub(crate) fn $name($s: &mut Scope<'_>, this: &Value, $a: &[Value]) -> JsResult {
                let _g = current($s, this)?;
                $body;
                undefined()
            }
        )*
    };
}

simple! {
    clear_color(|s, a| gl::clear_color(float(s, a, 0), float(s, a, 1), float(s, a, 2), float(s, a, 3)));
    clear_depth(|s, a| gl::clear_depthf(float(s, a, 0)));
    clear_stencil(|s, a| gl::clear_stencil(int(s, a, 0)));
    viewport(|s, a| gl::viewport(int(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3)));
    scissor(|s, a| gl::scissor(int(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3)));
    enable(|s, a| gl::enable(uint(s, a, 0)));
    disable(|s, a| gl::disable(uint(s, a, 0)));
    depth_func(|s, a| gl::depth_func(uint(s, a, 0)));
    depth_mask(|s, a| gl::depth_mask(u8::from(boolean(s, a, 0))));
    depth_range(|s, a| gl::depth_rangef(float(s, a, 0), float(s, a, 1)));
    color_mask(|s, a| gl::color_mask(
        u8::from(boolean(s, a, 0)),
        u8::from(boolean(s, a, 1)),
        u8::from(boolean(s, a, 2)),
        u8::from(boolean(s, a, 3)),
    ));
    stencil_mask(|s, a| gl::stencil_mask(uint(s, a, 0)));
    stencil_func(|s, a| gl::stencil_func(uint(s, a, 0), int(s, a, 1), uint(s, a, 2)));
    stencil_op(|s, a| gl::stencil_op(uint(s, a, 0), uint(s, a, 1), uint(s, a, 2)));
    blend_func(|s, a| gl::blend_func(uint(s, a, 0), uint(s, a, 1)));
    blend_func_separate(|s, a| gl::blend_func_separate(
        uint(s, a, 0), uint(s, a, 1), uint(s, a, 2), uint(s, a, 3)));
    blend_equation(|s, a| gl::blend_equation(uint(s, a, 0)));
    blend_equation_separate(|s, a| gl::blend_equation_separate(uint(s, a, 0), uint(s, a, 1)));
    blend_color(|s, a| gl::blend_color(float(s, a, 0), float(s, a, 1), float(s, a, 2), float(s, a, 3)));
    cull_face(|s, a| gl::cull_face(uint(s, a, 0)));
    front_face(|s, a| gl::front_face(uint(s, a, 0)));
    line_width(|s, a| gl::line_width(float(s, a, 0)));
    polygon_offset(|s, a| gl::polygon_offset(float(s, a, 0), float(s, a, 1)));
    hint(|s, a| gl::hint(uint(s, a, 0), uint(s, a, 1)));
    finish(|_s, _a| gl::finish());
    flush(|_s, _a| gl::flush());
    sample_coverage(|s, a| gl::sample_coverage(float(s, a, 0), u8::from(boolean(s, a, 1))));
    stencil_func_separate(|s, a| gl::stencil_func_separate(
        uint(s, a, 0), uint(s, a, 1), int(s, a, 2), uint(s, a, 3)));
    stencil_op_separate(|s, a| gl::stencil_op_separate(
        uint(s, a, 0), uint(s, a, 1), uint(s, a, 2), uint(s, a, 3)));
    stencil_mask_separate(|s, a| gl::stencil_mask_separate(uint(s, a, 0), uint(s, a, 1)));
    active_texture(|s, a| gl::active_texture(uint(s, a, 0)));
    delete_shader(|_s, a| gl::delete_shader(name(a, 0)));
    compile_shader(|_s, a| gl::compile_shader(name(a, 0)));
    delete_program(|_s, a| gl::delete_program(name(a, 0)));
    attach_shader(|_s, a| gl::attach_shader(name(a, 0), name(a, 1)));
    detach_shader(|_s, a| gl::detach_shader(name(a, 0), name(a, 1)));
    link_program(|_s, a| gl::link_program(name(a, 0)));
    validate_program(|_s, a| gl::validate_program(name(a, 0)));
    use_program(|_s, a| gl::use_program(name(a, 0)));
    delete_texture(|_s, a| gl::delete_texture(name(a, 0)));
    bind_texture(|s, a| gl::bind_texture(uint(s, a, 0), name(a, 1)));
    tex_parameter_i(|s, a| gl::tex_parameter_i(uint(s, a, 0), uint(s, a, 1), int(s, a, 2)));
    tex_parameter_f(|s, a| gl::tex_parameter_f(uint(s, a, 0), uint(s, a, 1), float(s, a, 2)));
    generate_mipmap(|s, a| gl::generate_mipmap(uint(s, a, 0)));
    framebuffer_texture_2d(|s, a| gl::framebuffer_texture_2d(
        uint(s, a, 0), uint(s, a, 1), uint(s, a, 2), name(a, 3), int(s, a, 4)));
    framebuffer_renderbuffer(|s, a| gl::framebuffer_renderbuffer(
        uint(s, a, 0), uint(s, a, 1), uint(s, a, 2), name(a, 3)));
    delete_renderbuffer(|_s, a| gl::delete_renderbuffer(name(a, 0)));
    bind_renderbuffer(|s, a| gl::bind_renderbuffer(uint(s, a, 0), name(a, 1)));
    renderbuffer_storage(|s, a| gl::renderbuffer_storage(
        uint(s, a, 0), uint(s, a, 1), int(s, a, 2), int(s, a, 3)));
    enable_vertex_attrib_array(|s, a| gl::enable_vertex_attrib_array(uint(s, a, 0)));
    disable_vertex_attrib_array(|s, a| gl::disable_vertex_attrib_array(uint(s, a, 0)));
    copy_tex_image_2d(|s, a| gl::copy_tex_image_2d(
        uint(s, a, 0), int(s, a, 1), uint(s, a, 2), int(s, a, 3),
        int(s, a, 4), int(s, a, 5), int(s, a, 6), int(s, a, 7)));
    copy_tex_sub_image_2d(|s, a| gl::copy_tex_sub_image_2d(
        uint(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3),
        int(s, a, 4), int(s, a, 5), int(s, a, 6), int(s, a, 7)));
}

pub(crate) fn clear(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    gl::clear(uint(s, a, 0));
    g.mark_dirty();
    undefined()
}

pub(crate) fn is_enabled(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    Ok(Value::boolean(gl::is_enabled(uint(s, a, 0)) != 0))
}

pub(crate) fn pixel_storei(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let pname = uint(s, a, 0);
    let param = int(s, a, 1);
    match pname {
        UNPACK_FLIP_Y_WEBGL => g.st.borrow_mut().unpack_flip_y = param != 0,
        UNPACK_PREMULTIPLY_ALPHA_WEBGL => g.st.borrow_mut().premultiply = param != 0,
        UNPACK_COLORSPACE_CONVERSION_WEBGL => {}
        _ if g.version < 2 && pname != PACK_ALIGNMENT && pname != UNPACK_ALIGNMENT => {
            gl::pixel_storei(0, 0)
        }
        _ => gl::pixel_storei(pname, param),
    }
    undefined()
}

pub(crate) fn get_error(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mut err = gl::get_error();
    let mut st = g.st.borrow_mut();
    if st.injected_error != NO_ERROR {
        if err == NO_ERROR {
            err = st.injected_error;
        }
        st.injected_error = NO_ERROR;
    }
    Ok(Value::int(err as i32))
}

fn text(s: &mut Scope<'_>, t: &str) -> JsResult {
    Ok(s.string(t))
}

pub(crate) fn get_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let pname = uint(s, a, 0);
    let v2 = g.version >= 2;
    match pname {
        VENDOR => text(s, "WebKit"),
        UNMASKED_VENDOR_WEBGL => text(s, "Google Inc. (Intel)"),
        RENDERER => text(s, "WebKit WebGL"),
        UNMASKED_RENDERER_WEBGL => text(
            s,
            "ANGLE (Intel, Mesa Intel(R) UHD Graphics (CML GT2), OpenGL 4.6)",
        ),
        VERSION => text(
            s,
            if v2 {
                "WebGL 2.0 (OpenGL ES 3.0 Chromium)"
            } else {
                "WebGL 1.0 (OpenGL ES 2.0 Chromium)"
            },
        ),
        SHADING_LANGUAGE_VERSION => text(
            s,
            if v2 {
                "WebGL GLSL ES 3.00 (OpenGL ES GLSL ES 3.0 Chromium)"
            } else {
                "WebGL GLSL ES 1.0 (OpenGL ES GLSL ES 1.0 Chromium)"
            },
        ),
        VIEWPORT | SCISSOR_BOX | MAX_VIEWPORT_DIMS => {
            let mut v = [0i32; 4];
            gl::get_integers(pname, &mut v);
            let n = if pname == MAX_VIEWPORT_DIMS {
                v[0] = v[0].min(16384);
                v[1] = v[1].min(16384);
                2
            } else {
                4
            };
            int_array(s, &v[..n])
        }
        COLOR_CLEAR_VALUE
        | DEPTH_CLEAR_VALUE
        | BLEND_COLOR
        | DEPTH_RANGE
        | ALIASED_LINE_WIDTH_RANGE
        | ALIASED_POINT_SIZE_RANGE => {
            let mut v = [0f32; 4];
            gl::get_floats(pname, &mut v);
            let n = match pname {
                DEPTH_CLEAR_VALUE => 1,
                COLOR_CLEAR_VALUE | BLEND_COLOR => 4,
                _ => 2,
            };
            if pname == ALIASED_LINE_WIDTH_RANGE {
                v[0] = 1.0;
                v[1] = 1.0;
            } else if pname == ALIASED_POINT_SIZE_RANGE {
                v[0] = 1.0;
                if v[1] > 1024.0 {
                    v[1] = 1024.0;
                }
            }
            float_array(s, &v[..n])
        }
        NUM_COMPRESSED_TEXTURE_FORMATS => Ok(Value::int(0)),
        STENCIL_WRITEMASK
        | STENCIL_BACK_WRITEMASK
        | STENCIL_VALUE_MASK
        | STENCIL_BACK_VALUE_MASK => Ok(uint_value(gl::get_integer(pname) as u32)),
        UNPACK_FLIP_Y_WEBGL => Ok(Value::boolean(g.st.borrow().unpack_flip_y)),
        UNPACK_PREMULTIPLY_ALPHA_WEBGL => Ok(Value::boolean(g.st.borrow().premultiply)),
        DEPTH_TEST | BLEND | CULL_FACE | STENCIL_TEST | SCISSOR_TEST | DITHER => {
            Ok(Value::boolean(gl::is_enabled(pname) != 0))
        }
        COMPRESSED_TEXTURE_FORMATS => uint_array(s, &[]),
        _ => {
            let mut v = [0i32; 32];
            gl::get_integers(pname, &mut v);
            let cap = limits::param_cap(pname);
            if cap != 0 && v[0] > cap {
                v[0] = cap;
            }
            Ok(Value::int(v[0]))
        }
    }
}

pub(crate) fn get_context_attributes(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = context(s, this)?;
    let o = s.new_object();
    let at = g.attrs;
    let entries = [
        ("alpha", Value::boolean(at.alpha)),
        ("antialias", Value::boolean(at.antialias)),
        ("depth", Value::boolean(at.depth)),
        ("desynchronized", Value::boolean(false)),
        ("failIfMajorPerformanceCaveat", Value::boolean(false)),
        ("powerPreference", s.string("default")),
        ("premultipliedAlpha", Value::boolean(at.premultiplied_alpha)),
        ("preserveDrawingBuffer", Value::boolean(at.preserve)),
        ("stencil", Value::boolean(at.stencil)),
        ("xrCompatible", Value::boolean(false)),
    ];
    for (key, value) in entries {
        let _ = s.set(&o, key, value);
    }
    Ok(o)
}

pub(crate) fn is_context_lost(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    context(s, this)?;
    Ok(Value::boolean(false))
}

const SUPPORTED_EXTENSIONS: [&str; 2] = [
    "WEBGL_debug_renderer_info",
    "EXT_texture_filter_anisotropic",
];

pub(crate) fn get_supported_extensions(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    context(s, this)?;
    let arr = s.new_array();
    for (i, ext) in SUPPORTED_EXTENSIONS.iter().enumerate() {
        let v = s.string(ext);
        let _ = s.set_index(&arr, i as u32, v);
    }
    Ok(arr)
}

pub(crate) fn get_extension(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    context(s, this)?;
    let Some(v) = a.first() else {
        return Ok(Value::null());
    };
    let Some(ext_name) = bytes(s, v) else {
        return Ok(Value::null());
    };
    let end = ext_name
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(ext_name.len());
    let ext_name = &ext_name[..end];
    let constants: &[(&str, i32)] = if ext_name.eq_ignore_ascii_case(b"WEBGL_debug_renderer_info") {
        &[
            ("UNMASKED_VENDOR_WEBGL", UNMASKED_VENDOR_WEBGL as i32),
            ("UNMASKED_RENDERER_WEBGL", UNMASKED_RENDERER_WEBGL as i32),
        ]
    } else if ext_name.eq_ignore_ascii_case(b"EXT_texture_filter_anisotropic") {
        &[
            (
                "MAX_TEXTURE_MAX_ANISOTROPY_EXT",
                MAX_TEXTURE_MAX_ANISOTROPY_EXT,
            ),
            ("TEXTURE_MAX_ANISOTROPY_EXT", TEXTURE_MAX_ANISOTROPY_EXT),
        ]
    } else {
        return Ok(Value::null());
    };
    let ext = s.new_object();
    for &(key, value) in constants {
        let _ = s.set(&ext, key, Value::int(value));
    }
    Ok(ext)
}

pub(crate) fn create_shader(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let shader = gl::create_shader(uint(s, a, 0));
    Ok(wrap(s, &g, shader, KIND_SHADER))
}

fn with_version_prefix(source: Vec<u8>) -> Vec<u8> {
    if cfg!(target_os = "macos") {
        let end = source.iter().position(|&b| b == 0).unwrap_or(source.len());
        if !source[..end].windows(8).any(|w| w == b"#version") {
            let mut prefixed = b"#version 100\n".to_vec();
            prefixed.extend_from_slice(&source[..end]);
            return prefixed;
        }
    }
    source
}

pub(crate) fn shader_source(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    if a.len() < 2 {
        return undefined();
    }
    if let Some(source) = bytes(s, &a[1])
        && source.len() <= MAX_SHADER
    {
        gl::shader_source(name(a, 0), &with_version_prefix(source));
    }
    undefined()
}

pub(crate) fn get_shader_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let pname = uint(s, a, 1);
    let v = gl::get_shaderiv(name(a, 0), pname);
    if pname == COMPILE_STATUS || pname == DELETE_STATUS {
        return Ok(Value::boolean(v != 0));
    }
    Ok(Value::int(v))
}

fn object_log(s: &mut Scope<'_>, kind: gl::LogKind, object: u32, len_pname: u32) -> JsResult {
    let len = match kind {
        gl::LogKind::ProgramInfo => gl::get_programiv(object, len_pname),
        _ => gl::get_shaderiv(object, len_pname),
    };
    if len <= 0 {
        return Ok(s.string(""));
    }
    let text = gl::object_log(kind, object, len);
    Ok(s.string_from_bytes(&text))
}

pub(crate) fn get_shader_info_log(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    object_log(s, gl::LogKind::ShaderInfo, name(a, 0), INFO_LOG_LENGTH)
}

pub(crate) fn get_shader_source(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    object_log(
        s,
        gl::LogKind::ShaderSource,
        name(a, 0),
        SHADER_SOURCE_LENGTH,
    )
}

pub(crate) fn get_program_info_log(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    object_log(s, gl::LogKind::ProgramInfo, name(a, 0), INFO_LOG_LENGTH)
}

pub(crate) fn create_program(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let program = gl::create_program();
    Ok(wrap(s, &g, program, KIND_PROGRAM))
}

pub(crate) fn get_program_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let pname = uint(s, a, 1);
    let v = gl::get_programiv(name(a, 0), pname);
    if pname == LINK_STATUS || pname == VALIDATE_STATUS || pname == DELETE_STATUS {
        return Ok(Value::boolean(v != 0));
    }
    Ok(Value::int(v))
}

fn string_arg(s: &mut Scope<'_>, a: &[Value], i: usize, min_args: usize) -> Option<Vec<u8>> {
    if a.len() < min_args {
        return None;
    }
    bytes(s, &a[i])
}

pub(crate) fn bind_attrib_location(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    if let Some(attrib) = string_arg(s, a, 2, 3) {
        let index = uint(s, a, 1);
        gl::bind_attrib_location(name(a, 0), index, &attrib);
    }
    undefined()
}

pub(crate) fn get_attrib_location(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let loc =
        string_arg(s, a, 1, 2).map_or(-1, |attrib| gl::get_attrib_location(name(a, 0), &attrib));
    Ok(Value::int(loc))
}

pub(crate) fn get_uniform_location(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let loc =
        string_arg(s, a, 1, 2).map_or(-1, |uniform| gl::get_uniform_location(name(a, 0), &uniform));
    if loc < 0 {
        return Ok(Value::null());
    }
    Ok(new_object(s, &g, loc as u32, KIND_LOCATION))
}

fn active_var(s: &mut Scope<'_>, this: &Value, a: &[Value], which: gl::ActiveKind) -> JsResult {
    let g = current(s, this)?;
    let program = name(a, 0);
    let index = uint(s, a, 1);
    let active = gl::active_variable(which, program, index, 255);
    Ok(active_info(s, &g, active))
}

pub(crate) fn get_active_attrib(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    active_var(s, this, a, gl::ActiveKind::Attrib)
}

pub(crate) fn get_active_uniform(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    active_var(s, this, a, gl::ActiveKind::Uniform)
}

pub(crate) fn get_transform_feedback_varying(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    active_var(s, this, a, gl::ActiveKind::TransformFeedbackVarying)
}

pub(crate) fn get_shader_precision_format(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let g = current(s, this)?;
    let shader_type = uint(s, a, 0);
    let precision_type = uint(s, a, 1);
    let (range, precision) = gl::get_shader_precision_format(shader_type, precision_type);
    let o = host::new_info(s, g.canvas, HIDDEN_PRECISION, "WebGLShaderPrecisionFormat");
    host::hidden_set(s, &o, "rangeMin", Value::int(range[0]));
    host::hidden_set(s, &o, "rangeMax", Value::int(range[1]));
    host::hidden_set(s, &o, "precision", Value::int(precision));
    Ok(o)
}

pub(crate) fn gen_object(
    s: &mut Scope<'_>,
    this: &Value,
    kind: u8,
    generate: fn() -> u32,
) -> JsResult {
    let g = current(s, this)?;
    let n = generate();
    Ok(wrap(s, &g, n, kind))
}

pub(crate) fn create_buffer(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_BUFFER, gl::gen_buffer)
}

pub(crate) fn create_texture(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_TEXTURE, gl::gen_texture)
}

pub(crate) fn create_framebuffer(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_FRAMEBUFFER, gl::gen_framebuffer)
}

pub(crate) fn create_renderbuffer(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_RENDERBUFFER, gl::gen_renderbuffer)
}

pub(crate) fn delete_buffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let n = name(a, 0);
    if n != 0 {
        let mut st = g.st.borrow_mut();
        st.buffer_sizes.remove(&n);
        st.elem_clear(n);
        gl::delete_buffer(n);
    }
    undefined()
}

pub(crate) fn bind_buffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let target = uint(s, a, 0);
    gl::bind_buffer(target, name(a, 1));
    undefined()
}

pub(crate) fn buffer_data(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let usage = uint(s, a, 2);
    let bn = limits::bound_buffer(target);
    let is_elem = target == ELEMENT_ARRAY_BUFFER;
    g.st.borrow_mut().elem_clear(bn);
    if a.len() >= 2 && a[1].is_number() {
        let size = s.to_int64(&a[1]).unwrap_or(0);
        if size < 0 || size as u64 > MAX_ALLOC as u64 {
            return undefined();
        }
        let size = size as usize;
        let mut st = g.st.borrow_mut();
        if is_elem {
            let zero = (size > 0).then(|| vec![0u8; size]);
            gl::buffer_data(target, size, zero.as_deref(), usage);
            st.elem_set(bn, None, size);
        } else {
            gl::buffer_data(target, size, None, usage);
        }
        st.set_buffer_size(bn, size);
        return undefined();
    }
    let source = arg(a, 1);
    let upload = |data: Option<&mut [u8]>| {
        let data = data.map(|d| &*d);
        let len = data.map_or(0, <[u8]>::len);
        if len > MAX_ALLOC {
            return;
        }
        gl::buffer_data(target, len, data, usage);
        let mut st = g.st.borrow_mut();
        st.set_buffer_size(bn, len);
        if is_elem {
            st.elem_set(bn, data, len);
        }
    };
    if a.len() >= 2 {
        with_view(s, &source, upload);
    } else {
        upload(None);
    }
    undefined()
}

pub(crate) fn buffer_sub_data(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let offset = int(s, a, 1) as isize;
    if a.len() < 3 {
        return undefined();
    }
    with_view(s, &a[2], |data| {
        let Some(data) = data else {
            return;
        };
        let mut st = g.st.borrow_mut();
        if st.buffer_range_ok(target, offset, data.len()) {
            gl::buffer_sub_data(target, offset, data);
            let bn = limits::bound_buffer(target);
            if target == ELEMENT_ARRAY_BUFFER {
                st.elem_patch(bn, offset as usize, data);
            } else {
                st.elem_clear(bn);
            }
        }
    });
    undefined()
}

pub(crate) fn vertex_attrib_pointer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let index = uint(s, a, 0);
    let size = int(s, a, 1);
    let kind = uint(s, a, 2);
    let normalized = boolean(s, a, 3);
    let stride = int(s, a, 4);
    let offset = int(s, a, 5);
    if size < 0 || stride < 0 || offset < 0 {
        return undefined();
    }
    gl::vertex_attrib_pointer(index, size, kind, normalized, stride, offset as usize);
    undefined()
}

fn vertex_attrib_f(s: &mut Scope<'_>, this: &Value, a: &[Value], n: usize) -> JsResult {
    let _g = current(s, this)?;
    let index = uint(s, a, 0);
    let mut v = [0.0, 0.0, 0.0, 1.0f32];
    for (i, slot) in v.iter_mut().enumerate().take(n) {
        *slot = float(s, a, i + 1);
    }
    match n {
        1 => gl::vertex_attrib1f(index, v[0]),
        2 => gl::vertex_attrib2f(index, v[0], v[1]),
        3 => gl::vertex_attrib3f(index, v[0], v[1], v[2]),
        _ => gl::vertex_attrib4f(index, v[0], v[1], v[2], v[3]),
    }
    undefined()
}

pub(crate) fn vertex_attrib1f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_f(s, t, a, 1)
}

pub(crate) fn vertex_attrib2f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_f(s, t, a, 2)
}

pub(crate) fn vertex_attrib3f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_f(s, t, a, 3)
}

pub(crate) fn vertex_attrib4f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_f(s, t, a, 4)
}

fn vertex_attrib_fv(s: &mut Scope<'_>, this: &Value, a: &[Value], n: usize) -> JsResult {
    let _g = current(s, this)?;
    let index = uint(s, a, 0);
    let mut v = [0.0, 0.0, 0.0, 1.0f32];
    if a.len() >= 2 {
        floats(s, &a[1], &mut v[..n]);
    }
    gl::vertex_attrib_fv(n as i32, index, &v);
    undefined()
}

pub(crate) fn vertex_attrib1fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_fv(s, t, a, 1)
}

pub(crate) fn vertex_attrib2fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_fv(s, t, a, 2)
}

pub(crate) fn vertex_attrib3fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_fv(s, t, a, 3)
}

pub(crate) fn vertex_attrib4fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    vertex_attrib_fv(s, t, a, 4)
}

fn uniform_f(s: &mut Scope<'_>, this: &Value, a: &[Value], n: usize) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut v = [0f32; 4];
    for (i, slot) in v.iter_mut().enumerate().take(n) {
        *slot = float(s, a, i + 1);
    }
    match n {
        1 => gl::uniform1f(loc, v[0]),
        2 => gl::uniform2f(loc, v[0], v[1]),
        3 => gl::uniform3f(loc, v[0], v[1], v[2]),
        _ => gl::uniform4f(loc, v[0], v[1], v[2], v[3]),
    }
    undefined()
}

pub(crate) fn uniform1f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_f(s, t, a, 1)
}

pub(crate) fn uniform2f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_f(s, t, a, 2)
}

pub(crate) fn uniform3f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_f(s, t, a, 3)
}

pub(crate) fn uniform4f(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_f(s, t, a, 4)
}

fn uniform_i(s: &mut Scope<'_>, this: &Value, a: &[Value], n: usize) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut v = [0i32; 4];
    for (i, slot) in v.iter_mut().enumerate().take(n) {
        *slot = int(s, a, i + 1);
    }
    match n {
        1 => gl::uniform1i(loc, v[0]),
        2 => gl::uniform2i(loc, v[0], v[1]),
        3 => gl::uniform3i(loc, v[0], v[1], v[2]),
        _ => gl::uniform4i(loc, v[0], v[1], v[2], v[3]),
    }
    undefined()
}

pub(crate) fn uniform1i(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_i(s, t, a, 1)
}

pub(crate) fn uniform2i(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_i(s, t, a, 2)
}

pub(crate) fn uniform3i(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_i(s, t, a, 3)
}

pub(crate) fn uniform4i(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_i(s, t, a, 4)
}

const UNIFORM_MAX: usize = 4096;

fn uniform_fv(s: &mut Scope<'_>, this: &Value, a: &[Value], n: i32) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut buf = vec![0f32; UNIFORM_MAX];
    let cnt = if a.len() >= 2 {
        floats(s, &a[1], &mut buf)
    } else {
        0
    };
    gl::uniform_fv(n, loc, cnt / n, &buf);
    undefined()
}

pub(crate) fn uniform1fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_fv(s, t, a, 1)
}

pub(crate) fn uniform2fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_fv(s, t, a, 2)
}

pub(crate) fn uniform3fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_fv(s, t, a, 3)
}

pub(crate) fn uniform4fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_fv(s, t, a, 4)
}

fn uniform_iv(s: &mut Scope<'_>, this: &Value, a: &[Value], n: i32) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut buf = vec![0i32; UNIFORM_MAX];
    let cnt = if a.len() >= 2 {
        ints(s, &a[1], &mut buf)
    } else {
        0
    };
    gl::uniform_iv(n, loc, cnt / n, &buf);
    undefined()
}

pub(crate) fn uniform1iv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_iv(s, t, a, 1)
}

pub(crate) fn uniform2iv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_iv(s, t, a, 2)
}

pub(crate) fn uniform3iv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_iv(s, t, a, 3)
}

pub(crate) fn uniform4iv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_iv(s, t, a, 4)
}

pub(crate) fn uniform_matrix(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
    rows: i32,
    cols: i32,
) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut buf = vec![0f32; UNIFORM_MAX];
    let cnt = if a.len() >= 3 {
        floats(s, &a[2], &mut buf)
    } else {
        0
    };
    gl::uniform_matrix_fv(rows, cols, loc, cnt / (rows * cols), &buf);
    undefined()
}

pub(crate) fn uniform_matrix2fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 2, 2)
}

pub(crate) fn uniform_matrix3fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 3, 3)
}

pub(crate) fn uniform_matrix4fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 4, 4)
}

pub(crate) fn draw_arrays(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mode = uint(s, a, 0);
    let first = int(s, a, 1);
    let count = int(s, a, 2);
    if first < 0 || count < 0 {
        return undefined();
    }
    if count > 0
        && !g
            .st
            .borrow()
            .attribs_cover(g.version, i64::from(first) + i64::from(count) - 1, 1)
    {
        return undefined();
    }
    gl::draw_arrays(mode, first, count);
    g.mark_dirty();
    undefined()
}

pub(crate) fn draw_elements(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mode = uint(s, a, 0);
    let count = int(s, a, 1);
    let kind = uint(s, a, 2);
    let offset = int(s, a, 3) as isize;
    if !g
        .st
        .borrow_mut()
        .draw_elements_ok(g.version, count, kind, offset, 1)
    {
        return undefined();
    }
    gl::draw_elements(mode, count, kind, offset as usize);
    g.mark_dirty();
    undefined()
}

struct ImageDataPixels {
    size: (i32, i32),
    data: Value,
}

fn image_data(s: &mut Scope<'_>, src: &Value) -> Option<ImageDataPixels> {
    let wv = s.get(src, "width").unwrap_or_else(|_| Value::undefined());
    let hv = s.get(src, "height").unwrap_or_else(|_| Value::undefined());
    let data = s.get(src, "data").unwrap_or_else(|_| Value::undefined());
    let w = s.to_int32(&wv).unwrap_or(0);
    let h = s.to_int32(&hv).unwrap_or(0);
    (w > 0 && h > 0 && data.is_object()).then_some(ImageDataPixels { size: (w, h), data })
}

fn upload_image_data(
    s: &mut Scope<'_>,
    g: &WebGl,
    src: &Value,
    pixel: (u32, u32),
    upload: impl FnOnce((i32, i32), &[u8]),
) -> bool {
    let Some(image) = image_data(s, src) else {
        return false;
    };
    let flip_y = g.st.borrow().unpack_flip_y;
    let version = g.version;
    let size = image.size;
    with_view(s, &image.data, |px| {
        let Some(px) = px else {
            return false;
        };
        let need = limits::transfer_bytes(version, (size.0, size.1, 1), pixel.0, pixel.1, false);
        if need > 0 && need <= MAX_ALLOC && px.len() >= need {
            if limits::flip_safe(flip_y, size, pixel.0, pixel.1, need, px.len()) {
                let flipped = limits::flip_rows(px, size, limits::components(pixel.0));
                upload(size, &flipped);
            } else {
                upload(size, px);
            }
        }
        true
    })
}

fn luminance(r: u32, g: u32, b: u32) -> u8 {
    ((r * 77 + g * 150 + b * 29) >> 8) as u8
}

fn unpremultiply(c: u32, a: u32) -> u32 {
    ((c * 255 + a / 2) / a).min(255)
}

struct SourcePixels {
    size: (i32, i32),
    data: Vec<u8>,
}

fn source_pixels(
    s: &mut Scope<'_>,
    src: &Value,
    format: u32,
    flip_y: bool,
    premultiply: bool,
) -> Result<Option<SourcePixels>, Value> {
    let Some(mut image) = host::drawimage_source(s, src)? else {
        return Ok(None);
    };
    let (w, h) = (image.w, image.h);
    let Some((data, stride)) = image.pixels() else {
        return Ok(None);
    };
    if w <= 0 || h <= 0 {
        return Ok(None);
    }
    let (wu, hu) = (w as usize, h as usize);
    let comps = limits::components(format);
    let total = wu as u64 * hu as u64 * comps as u64;
    if total == 0 || total > MAX_ALLOC as u64 || (hu - 1) * stride + wu * 4 > data.len() {
        return Ok(None);
    }
    let mut out = vec![0u8; total as usize];
    for y in 0..hu {
        let sy = if flip_y { hu - 1 - y } else { y };
        let srow = &data[sy * stride..sy * stride + wu * 4];
        let orow = &mut out[y * wu * comps..(y + 1) * wu * comps];
        for (p, o) in srow.chunks_exact(4).zip(orow.chunks_exact_mut(comps)) {
            let (mut b, mut gg, mut r, a) = (
                u32::from(p[0]),
                u32::from(p[1]),
                u32::from(p[2]),
                u32::from(p[3]),
            );
            if !premultiply && a > 0 && a < 255 {
                r = unpremultiply(r, a);
                gg = unpremultiply(gg, a);
                b = unpremultiply(b, a);
            }
            match format {
                RGBA => o.copy_from_slice(&[r as u8, gg as u8, b as u8, a as u8]),
                RGB => o.copy_from_slice(&[r as u8, gg as u8, b as u8]),
                LUMINANCE_ALPHA => o.copy_from_slice(&[luminance(r, gg, b), a as u8]),
                ALPHA => o[0] = a as u8,
                _ => o[0] = luminance(r, gg, b),
            }
        }
    }
    Ok(Some(SourcePixels {
        size: (w, h),
        data: out,
    }))
}

struct UnpackState {
    align: i32,
    row_length: i32,
    skip_rows: i32,
    skip_pixels: i32,
}

fn unpack_tight(version: i32) -> UnpackState {
    let mut saved = UnpackState {
        align: gl::get_integer(UNPACK_ALIGNMENT),
        row_length: 0,
        skip_rows: 0,
        skip_pixels: 0,
    };
    gl::pixel_storei(UNPACK_ALIGNMENT, 1);
    if version < 2 {
        return saved;
    }
    saved.row_length = gl::get_integer(UNPACK_ROW_LENGTH);
    saved.skip_rows = gl::get_integer(UNPACK_SKIP_ROWS);
    saved.skip_pixels = gl::get_integer(UNPACK_SKIP_PIXELS);
    gl::pixel_storei(UNPACK_ROW_LENGTH, 0);
    gl::pixel_storei(UNPACK_SKIP_ROWS, 0);
    gl::pixel_storei(UNPACK_SKIP_PIXELS, 0);
    saved
}

fn unpack_restore(version: i32, saved: &UnpackState) {
    gl::pixel_storei(UNPACK_ALIGNMENT, saved.align);
    if version < 2 {
        return;
    }
    gl::pixel_storei(UNPACK_ROW_LENGTH, saved.row_length);
    gl::pixel_storei(UNPACK_SKIP_ROWS, saved.skip_rows);
    gl::pixel_storei(UNPACK_SKIP_PIXELS, saved.skip_pixels);
}

fn upload_source(
    s: &mut Scope<'_>,
    g: &WebGl,
    src: &Value,
    pixel: (u32, u32),
    upload: impl FnOnce((i32, i32), &[u8]),
) -> Result<(), Value> {
    if pixel.1 != UNSIGNED_BYTE {
        return Ok(());
    }
    let (flip_y, premultiply) = {
        let st = g.st.borrow();
        (st.unpack_flip_y, st.premultiply)
    };
    if let Some(rgba) = source_pixels(s, src, pixel.0, flip_y, premultiply)? {
        let saved = unpack_tight(g.version);
        upload(rgba.size, &rgba.data);
        unpack_restore(g.version, &saved);
    }
    Ok(())
}

struct Upload<'a> {
    flip_y: bool,
    version: i32,
    size: (i32, i32),
    pixel: (u32, u32),
    zero_fill: bool,
    call: &'a dyn Fn(Option<&[u8]>),
}

fn upload_view(s: &mut Scope<'_>, source: &Value, up: Upload<'_>) {
    let has_source = !source.is_null() && !source.is_undefined();
    let need = limits::transfer_bytes(
        up.version,
        (up.size.0, up.size.1, 1),
        up.pixel.0,
        up.pixel.1,
        false,
    );
    let send = |px: Option<&[u8]>| {
        let len = px.map_or(0, <[u8]>::len);
        if need > MAX_ALLOC || (px.is_some() && len < need) {
            return;
        }
        match px {
            None if up.zero_fill => {
                let zero = (need > 0).then(|| vec![0u8; need]);
                (up.call)(zero.as_deref());
            }
            None => {}
            Some(px) => {
                if limits::flip_safe(up.flip_y, up.size, up.pixel.0, up.pixel.1, need, len) {
                    let flipped = limits::flip_rows(px, up.size, limits::components(up.pixel.0));
                    (up.call)(Some(&flipped));
                } else {
                    (up.call)(Some(px));
                }
            }
        }
    };
    if has_source {
        with_view(s, source, |px| send(px.map(|p| &*p)));
    } else {
        send(None);
    }
}

pub(crate) fn tex_image_2d(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let level = int(s, a, 1);
    let internal = int(s, a, 2);
    if a.len() >= 9 && a[3].is_number() {
        let w = int(s, a, 3);
        let h = int(s, a, 4);
        let border = int(s, a, 5);
        let format = uint(s, a, 6);
        let kind = uint(s, a, 7);
        let call = |px: Option<&[u8]>| {
            gl::tex_image_2d(target, level, internal, (w, h), border, (format, kind), px)
        };
        let up = Upload {
            flip_y: g.st.borrow().unpack_flip_y,
            version: g.version,
            size: (w, h),
            pixel: (format, kind),
            zero_fill: true,
            call: &call,
        };
        upload_view(s, &a[8], up);
        return undefined();
    }
    let format = uint(s, a, 3);
    let kind = uint(s, a, 4);
    if a.len() >= 6 && a[5].is_object() {
        let call = |size: (i32, i32), px: &[u8]| {
            gl::tex_image_2d(target, level, internal, size, 0, (format, kind), Some(px))
        };
        if !upload_image_data(s, &g, &a[5], (format, kind), call) {
            upload_source(s, &g, &a[5], (format, kind), call)?;
        }
    }
    undefined()
}

pub(crate) fn tex_sub_image_2d(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let level = int(s, a, 1);
    let xoff = int(s, a, 2);
    let yoff = int(s, a, 3);
    if a.len() >= 9 && a[4].is_number() {
        let w = int(s, a, 4);
        let h = int(s, a, 5);
        let format = uint(s, a, 6);
        let kind = uint(s, a, 7);
        let call = |px: Option<&[u8]>| {
            if let Some(px) = px {
                gl::tex_sub_image_2d(target, level, (xoff, yoff), (w, h), (format, kind), px);
            }
        };
        let up = Upload {
            flip_y: g.st.borrow().unpack_flip_y,
            version: g.version,
            size: (w, h),
            pixel: (format, kind),
            zero_fill: false,
            call: &call,
        };
        upload_view(s, &a[8], up);
        return undefined();
    }
    let format = uint(s, a, 4);
    let kind = uint(s, a, 5);
    if a.len() >= 7 && a[6].is_object() {
        let call = |size: (i32, i32), px: &[u8]| {
            gl::tex_sub_image_2d(target, level, (xoff, yoff), size, (format, kind), px)
        };
        if !upload_image_data(s, &g, &a[6], (format, kind), call) {
            upload_source(s, &g, &a[6], (format, kind), call)?;
        }
    }
    undefined()
}

pub(crate) fn delete_framebuffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    if !a.is_empty() {
        let n = name(a, 0);
        if n != 0 {
            let mut st = g.st.borrow_mut();
            if st.user_draw_fbo == n {
                st.user_draw_fbo = 0;
            }
            if st.user_read_fbo == n {
                st.user_read_fbo = 0;
            }
            if st.bound_draw_fbo == n {
                st.bound_draw_fbo = 0;
            }
            if st.bound_read_fbo == n {
                st.bound_read_fbo = 0;
            }
        }
    }
    let _g = current(s, this)?;
    gl::delete_framebuffer(name(a, 0));
    undefined()
}

pub(crate) fn bind_framebuffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let f = if a.len() >= 2 { name(a, 1) } else { 0 };
    let mut st = g.st.borrow_mut();
    match target {
        FRAMEBUFFER => {
            st.user_draw_fbo = f;
            st.user_read_fbo = f;
        }
        DRAW_FRAMEBUFFER => st.user_draw_fbo = f,
        READ_FRAMEBUFFER => st.user_read_fbo = f,
        _ => {}
    }
    let fbo = if f != 0 { f } else { st.draw_target() };
    st.bind_framebuffer(target, fbo);
    undefined()
}

pub(crate) fn check_framebuffer_status(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    Ok(Value::int(
        gl::check_framebuffer_status(uint(s, a, 0)) as i32
    ))
}

pub(crate) fn read_pixels(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let x = int(s, a, 0);
    let y = int(s, a, 1);
    let w = int(s, a, 2);
    let h = int(s, a, 3);
    let format = uint(s, a, 4);
    let kind = uint(s, a, 5);
    if a.len() < 7 || !a[6].is_object() {
        return undefined();
    }
    with_view(s, &a[6], |out| {
        let need = limits::transfer_bytes(g.version, (w, h, 1), format, kind, true);
        let Some(out) = out else {
            return;
        };
        if out.len() < need || need == 0 {
            return;
        }
        let mut st = g.st.borrow_mut();
        let bound = gl::get_integer(FRAMEBUFFER_BINDING) as u32;
        let resolve = st.samples > 1 && bound == st.draw_fbo;
        if resolve {
            let (draw_fbo, fbo, gw, gh) = (st.draw_fbo, st.fbo, st.w, st.h);
            st.bind_framebuffer(READ_FRAMEBUFFER, draw_fbo);
            st.bind_framebuffer(DRAW_FRAMEBUFFER, fbo);
            gl::blit_framebuffer(0, 0, gw, gh, 0, 0, gw, gh, COLOR_BUFFER_BIT, NEAREST);
            st.bind_framebuffer(FRAMEBUFFER, fbo);
        }
        gl::read_pixels((x, y, w, h), format, kind, out);
        if resolve {
            let draw_fbo = st.draw_fbo;
            st.bind_framebuffer(FRAMEBUFFER, draw_fbo);
        }
    });
    undefined()
}

pub(crate) fn is_object(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
    test: fn(u32) -> u8,
) -> JsResult {
    let _g = current(s, this)?;
    let n = if a.is_empty() { 0 } else { name(a, 0) };
    Ok(Value::boolean(n != 0 && test(n) != 0))
}

pub(crate) fn is_buffer(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_buffer)
}

pub(crate) fn is_program(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_program)
}

pub(crate) fn is_shader(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_shader)
}

pub(crate) fn is_texture(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_texture)
}

pub(crate) fn is_framebuffer(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_framebuffer)
}

pub(crate) fn is_renderbuffer(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_renderbuffer)
}

fn target_parameter(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
    which: gl::TargetQuery,
) -> JsResult {
    let _g = current(s, this)?;
    let target = uint(s, a, 0);
    let pname = uint(s, a, 1);
    Ok(Value::int(gl::get_target_parameter(which, target, pname)))
}

pub(crate) fn get_buffer_parameter(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    target_parameter(s, t, a, gl::TargetQuery::Buffer)
}

pub(crate) fn get_tex_parameter(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    target_parameter(s, t, a, gl::TargetQuery::Texture)
}

pub(crate) fn get_renderbuffer_parameter(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    target_parameter(s, t, a, gl::TargetQuery::Renderbuffer)
}

pub(crate) fn get_framebuffer_attachment_parameter(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let _g = current(s, this)?;
    let target = uint(s, a, 0);
    let attachment = uint(s, a, 1);
    let pname = uint(s, a, 2);
    Ok(Value::int(gl::get_framebuffer_attachment_parameter(
        target, attachment, pname,
    )))
}

pub(crate) fn get_vertex_attrib(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let index = uint(s, a, 0);
    let pname = uint(s, a, 1);
    if pname == CURRENT_VERTEX_ATTRIB {
        let v = gl::get_vertex_attribf4(index, pname);
        return float_array(s, &v);
    }
    let v = gl::get_vertex_attribi(index, pname);
    match pname {
        VERTEX_ATTRIB_ARRAY_ENABLED | VERTEX_ATTRIB_ARRAY_NORMALIZED => Ok(Value::boolean(v != 0)),
        VERTEX_ATTRIB_ARRAY_BUFFER_BINDING => Ok(wrap(s, &g, v as u32, KIND_BUFFER)),
        _ => Ok(Value::int(v)),
    }
}

pub(crate) fn get_vertex_attrib_offset(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let index = uint(s, a, 0);
    let pname = uint(s, a, 1);
    Ok(Value::int(gl::get_vertex_attrib_offset(index, pname) as i32))
}

pub(crate) fn get_uniform(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    Ok(Value::null())
}

pub(crate) fn compressed_unsupported(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    g.st.borrow_mut().injected_error = INVALID_ENUM;
    undefined()
}

pub(crate) fn get_attached_shaders(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    if a.is_empty() || !a[0].is_object() {
        return Ok(Value::null());
    }
    let shaders = gl::get_attached_shaders(name(a, 0));
    let arr = s.new_array();
    for (i, shader) in shaders.into_iter().enumerate() {
        let v = wrap(s, &g, shader, KIND_SHADER);
        let _ = s.set_index(&arr, i as u32, v);
    }
    Ok(arr)
}

pub(crate) fn drawing_buffer_storage(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let format = uint(s, a, 0);
    let w = int(s, a, 1);
    let h = int(s, a, 2);
    if format != RGBA8 && format != RGB8 {
        g.st.borrow_mut().injected_error = INVALID_ENUM;
        return undefined();
    }
    if w <= 0 || h <= 0 || w > 8192 || h > 8192 {
        g.st.borrow_mut().injected_error = INVALID_VALUE;
        return undefined();
    }
    g.resize(w, h, true);
    g.mark_dirty();
    g.st.borrow_mut().surf = None;
    undefined()
}

pub(crate) fn make_xr_compatible(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    context(s, this)?;
    let (promise, resolve, reject) = s.new_promise()?;
    drop(resolve);
    let err = s.new_error();
    let err_name = s.string("InvalidStateError");
    let _ = s.set(&err, "name", err_name);
    let message = s.string("No XR device is available.");
    let _ = s.set(&err, "message", message);
    let _ = s.call(&reject, &Value::undefined(), &[err]);
    Ok(promise)
}
