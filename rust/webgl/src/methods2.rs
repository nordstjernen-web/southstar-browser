//! Southstar — the WebGL2RenderingContext additions: vertex arrays, instancing, 3D textures, queries, samplers, syncs and uniform blocks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::args::*;
use crate::consts::*;
use crate::context::WebGl;
use crate::ffi::gl;
use crate::limits::{self, MAX_ALLOC};
use crate::methods::*;

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
    delete_vertex_array(|_s, a| gl::delete_vertex_array(name(a, 0)));
    bind_vertex_array(|_s, a| gl::bind_vertex_array(if a.is_empty() { 0 } else { name(a, 0) }));
    vertex_attrib_divisor(|s, a| gl::vertex_attrib_divisor(uint(s, a, 0), uint(s, a, 1)));
    tex_storage_2d(|s, a| gl::tex_storage_2d(
        uint(s, a, 0), int(s, a, 1), uint(s, a, 2), int(s, a, 3), int(s, a, 4)));
    renderbuffer_storage_multisample(|s, a| gl::renderbuffer_storage_multisample(
        uint(s, a, 0), int(s, a, 1), uint(s, a, 2), int(s, a, 3), int(s, a, 4)));
    blit_framebuffer(|s, a| gl::blit_framebuffer(
        int(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3),
        int(s, a, 4), int(s, a, 5), int(s, a, 6), int(s, a, 7),
        uint(s, a, 8), uint(s, a, 9)));
    framebuffer_texture_layer(|s, a| gl::framebuffer_texture_layer(
        uint(s, a, 0), uint(s, a, 1), name(a, 2), int(s, a, 3), int(s, a, 4)));
    read_buffer(|s, a| gl::read_buffer(uint(s, a, 0)));
    delete_sampler(|_s, a| gl::delete_sampler(name(a, 0)));
    bind_sampler(|s, a| gl::bind_sampler(uint(s, a, 0), if a.len() >= 2 { name(a, 1) } else { 0 }));
    sampler_parameter_i(|s, a| gl::sampler_parameter_i(name(a, 0), uint(s, a, 1), int(s, a, 2)));
    sampler_parameter_f(|s, a| gl::sampler_parameter_f(name(a, 0), uint(s, a, 1), float(s, a, 2)));
    uniform_block_binding(|s, a| gl::uniform_block_binding(name(a, 0), uint(s, a, 1), uint(s, a, 2)));
    bind_buffer_base(|s, a| gl::bind_buffer_base(
        uint(s, a, 0), uint(s, a, 1), if a.len() >= 3 { name(a, 2) } else { 0 }));
    bind_buffer_range(|s, a| gl::bind_buffer_range(
        uint(s, a, 0), uint(s, a, 1), if a.len() >= 3 { name(a, 2) } else { 0 },
        int(s, a, 3) as isize, int(s, a, 4) as isize));
    copy_tex_sub_image_3d(|s, a| gl::copy_tex_sub_image_3d(
        uint(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3), int(s, a, 4),
        int(s, a, 5), int(s, a, 6), int(s, a, 7), int(s, a, 8)));
    vertex_attrib_i4i(|s, a| gl::vertex_attrib_i4i(
        uint(s, a, 0), int(s, a, 1), int(s, a, 2), int(s, a, 3), int(s, a, 4)));
    vertex_attrib_i4ui(|s, a| gl::vertex_attrib_i4ui(
        uint(s, a, 0), uint(s, a, 1), uint(s, a, 2), uint(s, a, 3), uint(s, a, 4)));
    tex_storage_3d(|s, a| gl::tex_storage_3d(
        uint(s, a, 0), int(s, a, 1), uint(s, a, 2), int(s, a, 3), int(s, a, 4), int(s, a, 5)));
    delete_query(|_s, a| gl::delete_query(name(a, 0)));
    begin_query(|s, a| gl::begin_query(uint(s, a, 0), name(a, 1)));
    end_query(|s, a| gl::end_query(uint(s, a, 0)));
    delete_transform_feedback(|_s, a| gl::delete_transform_feedback(name(a, 0)));
    bind_transform_feedback(|s, a| gl::bind_transform_feedback(
        uint(s, a, 0), if a.len() >= 2 { name(a, 1) } else { 0 }));
    begin_transform_feedback(|s, a| gl::begin_transform_feedback(uint(s, a, 0)));
    pause_transform_feedback(|_s, _a| gl::pause_transform_feedback());
    resume_transform_feedback(|_s, _a| gl::resume_transform_feedback());
}

pub(crate) fn create_vertex_array(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_VERTEX_ARRAY, gl::gen_vertex_array)
}

pub(crate) fn create_sampler(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_SAMPLER, gl::gen_sampler)
}

pub(crate) fn create_query(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_QUERY, gl::gen_query)
}

pub(crate) fn create_transform_feedback(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    gen_object(s, this, KIND_TRANSFORM_FEEDBACK, gl::gen_transform_feedback)
}

pub(crate) fn is_vertex_array(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_vertex_array)
}

pub(crate) fn is_sampler(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_sampler)
}

pub(crate) fn is_query(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_query)
}

pub(crate) fn is_transform_feedback(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    is_object(s, t, a, gl::is_transform_feedback)
}

pub(crate) fn draw_arrays_instanced(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mode = uint(s, a, 0);
    let first = int(s, a, 1);
    let count = int(s, a, 2);
    let instances = int(s, a, 3);
    if first < 0 || count < 0 || instances < 0 {
        return undefined();
    }
    if count > 0
        && instances > 0
        && !g.st.borrow().attribs_cover(
            g.version,
            i64::from(first) + i64::from(count) - 1,
            i64::from(instances),
        )
    {
        return undefined();
    }
    gl::draw_arrays_instanced(mode, first, count, instances);
    g.mark_dirty();
    undefined()
}

pub(crate) fn draw_elements_instanced(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mode = uint(s, a, 0);
    let count = int(s, a, 1);
    let kind = uint(s, a, 2);
    let offset = int(s, a, 3) as isize;
    let instances = int(s, a, 4);
    if instances < 0
        || !g
            .st
            .borrow_mut()
            .draw_elements_ok(g.version, count, kind, offset, i64::from(instances))
    {
        return undefined();
    }
    gl::draw_elements_instanced(mode, count, kind, offset as usize, instances);
    g.mark_dirty();
    undefined()
}

pub(crate) fn draw_range_elements(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mode = uint(s, a, 0);
    let start = uint(s, a, 1);
    let end = uint(s, a, 2);
    let count = int(s, a, 3);
    let kind = uint(s, a, 4);
    let offset = int(s, a, 5) as isize;
    if !g
        .st
        .borrow_mut()
        .draw_elements_ok(g.version, count, kind, offset, 1)
    {
        return undefined();
    }
    gl::draw_range_elements(mode, (start, end), count, kind, offset as usize);
    g.mark_dirty();
    undefined()
}

pub(crate) fn draw_buffers(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let mut bufs = [0u32; 16];
    let n = if a.is_empty() {
        0
    } else {
        uints(s, &a[0], &mut bufs)
    };
    if n > 0 {
        gl::draw_buffers(&bufs[..n as usize]);
    }
    undefined()
}

pub(crate) fn vertex_attrib_i_pointer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let size = int(s, a, 1);
    let stride = int(s, a, 3);
    let offset = int(s, a, 4);
    if size < 0 || stride < 0 || offset < 0 {
        return undefined();
    }
    let index = uint(s, a, 0);
    let kind = uint(s, a, 2);
    gl::vertex_attrib_i_pointer(index, size, kind, stride, offset as usize);
    undefined()
}

fn uniform_ui(s: &mut Scope<'_>, this: &Value, a: &[Value], n: usize) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut v = [0u32; 4];
    for (i, slot) in v.iter_mut().enumerate().take(n) {
        *slot = uint(s, a, i + 1);
    }
    match n {
        1 => gl::uniform1ui(loc, v[0]),
        2 => gl::uniform2ui(loc, v[0], v[1]),
        3 => gl::uniform3ui(loc, v[0], v[1], v[2]),
        _ => gl::uniform4ui(loc, v[0], v[1], v[2], v[3]),
    }
    undefined()
}

pub(crate) fn uniform1ui(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_ui(s, t, a, 1)
}

pub(crate) fn uniform2ui(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_ui(s, t, a, 2)
}

pub(crate) fn uniform3ui(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_ui(s, t, a, 3)
}

pub(crate) fn uniform4ui(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_ui(s, t, a, 4)
}

fn uniform_uiv(s: &mut Scope<'_>, this: &Value, a: &[Value], n: i32) -> JsResult {
    let _g = current(s, this)?;
    let loc = location(a, 0);
    let mut buf = vec![0u32; 4096];
    let cnt = if a.len() >= 2 {
        uints(s, &a[1], &mut buf)
    } else {
        0
    };
    gl::uniform_uiv(n, loc, cnt / n, &buf);
    undefined()
}

pub(crate) fn uniform1uiv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_uiv(s, t, a, 1)
}

pub(crate) fn uniform2uiv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_uiv(s, t, a, 2)
}

pub(crate) fn uniform3uiv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_uiv(s, t, a, 3)
}

pub(crate) fn uniform4uiv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_uiv(s, t, a, 4)
}

pub(crate) fn uniform_matrix2x3fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 2, 3)
}

pub(crate) fn uniform_matrix3x2fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 3, 2)
}

pub(crate) fn uniform_matrix2x4fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 2, 4)
}

pub(crate) fn uniform_matrix4x2fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 4, 2)
}

pub(crate) fn uniform_matrix3x4fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 3, 4)
}

pub(crate) fn uniform_matrix4x3fv(s: &mut Scope<'_>, t: &Value, a: &[Value]) -> JsResult {
    uniform_matrix(s, t, a, 4, 3)
}

pub(crate) fn invalidate_framebuffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let mut att = [0u32; 16];
    let n = if a.len() >= 2 {
        uints(s, &a[1], &mut att)
    } else {
        0
    };
    if n > 0 {
        let target = uint(s, a, 0);
        gl::invalidate_framebuffer(target, &att[..n as usize]);
    }
    undefined()
}

pub(crate) fn invalidate_sub_framebuffer(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let mut att = [0u32; 16];
    let n = if a.len() >= 2 {
        uints(s, &a[1], &mut att)
    } else {
        0
    };
    if n > 0 {
        let target = uint(s, a, 0);
        let rect = (int(s, a, 2), int(s, a, 3), int(s, a, 4), int(s, a, 5));
        gl::invalidate_sub_framebuffer(target, &att[..n as usize], rect);
    }
    undefined()
}

pub(crate) fn copy_buffer_sub_data(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let read_target = uint(s, a, 0);
    let write_target = uint(s, a, 1);
    let read_offset = int(s, a, 2) as isize;
    let write_offset = int(s, a, 3) as isize;
    let size = int(s, a, 4) as isize;
    gl::copy_buffer_sub_data(read_target, write_target, read_offset, write_offset, size);
    g.st.borrow_mut()
        .elem_clear(limits::bound_buffer(write_target));
    undefined()
}

pub(crate) fn get_buffer_sub_data(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let offset = int(s, a, 1) as isize;
    if a.len() < 3 || !a[2].is_object() {
        return undefined();
    }
    with_view(s, &a[2], |dst| {
        let Some(dst) = dst else {
            return;
        };
        if !dst.is_empty() && g.st.borrow().buffer_range_ok(target, offset, dst.len()) {
            gl::read_buffer_range(target, offset, dst);
        }
    });
    undefined()
}

pub(crate) fn clear_buffer_fv(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mut v = [0f32; 4];
    if a.len() >= 3 {
        floats(s, &a[2], &mut v);
    }
    let buffer = uint(s, a, 0);
    let drawbuffer = int(s, a, 1);
    gl::clear_bufferfv(buffer, drawbuffer, &v);
    g.mark_dirty();
    undefined()
}

pub(crate) fn clear_buffer_iv(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mut v = [0i32; 4];
    if a.len() >= 3 {
        ints(s, &a[2], &mut v);
    }
    let buffer = uint(s, a, 0);
    let drawbuffer = int(s, a, 1);
    gl::clear_bufferiv(buffer, drawbuffer, &v);
    g.mark_dirty();
    undefined()
}

pub(crate) fn clear_buffer_uiv(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let mut v = [0u32; 4];
    if a.len() >= 3 {
        uints(s, &a[2], &mut v);
    }
    let buffer = uint(s, a, 0);
    let drawbuffer = int(s, a, 1);
    gl::clear_bufferuiv(buffer, drawbuffer, &v);
    g.mark_dirty();
    undefined()
}

pub(crate) fn clear_bufferfi(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let buffer = uint(s, a, 0);
    let drawbuffer = int(s, a, 1);
    let depth = float(s, a, 2);
    let stencil = int(s, a, 3);
    gl::clear_bufferfi(buffer, drawbuffer, depth, stencil);
    g.mark_dirty();
    undefined()
}

fn string_arg(s: &mut Scope<'_>, a: &[Value], i: usize, min_args: usize) -> Option<Vec<u8>> {
    if a.len() < min_args {
        return None;
    }
    bytes(s, &a[i])
}

pub(crate) fn get_uniform_block_index(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let index = string_arg(s, a, 1, 2).map_or(INVALID_INDEX, |block| {
        gl::get_uniform_block_index(name(a, 0), &block)
    });
    Ok(uint_value(index))
}

pub(crate) fn get_frag_data_location(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let loc = string_arg(s, a, 1, 2).map_or(-1, |out| gl::get_frag_data_location(name(a, 0), &out));
    Ok(Value::int(loc))
}

pub(crate) fn vertex_attrib_i4iv(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let mut v = [0i32; 4];
    if a.len() >= 2 {
        ints(s, &a[1], &mut v);
    }
    gl::vertex_attrib_i4iv(uint(s, a, 0), &v);
    undefined()
}

pub(crate) fn vertex_attrib_i4uiv(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let mut v = [0u32; 4];
    if a.len() >= 2 {
        uints(s, &a[1], &mut v);
    }
    gl::vertex_attrib_i4uiv(uint(s, a, 0), &v);
    undefined()
}

pub(crate) fn get_internalformat_parameter(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let _g = current(s, this)?;
    let target = uint(s, a, 0);
    let format = uint(s, a, 1);
    let pname = uint(s, a, 2);
    let mut count = [0i32; 1];
    gl::get_internalformativ(target, format, NUM_SAMPLE_COUNTS, &mut count);
    let count = count[0];
    if count <= 0 || count > 64 {
        return int_array(s, &[]);
    }
    let mut vals = vec![0i32; count as usize];
    gl::get_internalformativ(target, format, pname, &mut vals);
    int_array(s, &vals)
}

pub(crate) fn tex_image_3d(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let level = int(s, a, 1);
    let internal = int(s, a, 2);
    let size = (int(s, a, 3), int(s, a, 4), int(s, a, 5));
    let border = int(s, a, 6);
    let format = uint(s, a, 7);
    let kind = uint(s, a, 8);
    let need = limits::transfer_bytes(g.version, size, format, kind, false);
    let send = |px: Option<&[u8]>| {
        let len = px.map_or(0, <[u8]>::len);
        if need > MAX_ALLOC || (px.is_some() && len < need) {
            return;
        }
        let zero = (px.is_none() && need > 0).then(|| vec![0u8; need]);
        let px = px.or(zero.as_deref());
        gl::tex_image_3d(target, level, internal, size, border, (format, kind), px);
    };
    if a.len() >= 10 && a[9].is_object() {
        with_view(s, &a[9], |px| send(px.map(|p| &*p)));
    } else {
        send(None);
    }
    undefined()
}

pub(crate) fn tex_sub_image_3d(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let level = int(s, a, 1);
    let offset = (int(s, a, 2), int(s, a, 3), int(s, a, 4));
    let size = (int(s, a, 5), int(s, a, 6), int(s, a, 7));
    let format = uint(s, a, 8);
    let kind = uint(s, a, 9);
    if a.len() < 11 || !a[10].is_object() {
        return undefined();
    }
    with_view(s, &a[10], |px| {
        let need = limits::transfer_bytes(g.version, size, format, kind, false);
        if let Some(px) = px
            && need > 0
            && need <= MAX_ALLOC
            && px.len() >= need
        {
            gl::tex_sub_image_3d(target, level, offset, size, (format, kind), px);
        }
    });
    undefined()
}

pub(crate) fn get_query_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    let pname = uint(s, a, 1);
    let v = gl::get_query_objectui(name(a, 0), pname);
    if pname == QUERY_RESULT_AVAILABLE {
        return Ok(Value::boolean(v != 0));
    }
    Ok(uint_value(v))
}

pub(crate) fn get_query(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let pname = uint(s, a, 1);
    let v = gl::get_queryi(target, pname);
    Ok(wrap(s, &g, v as u32, KIND_QUERY))
}

pub(crate) fn end_transform_feedback(s: &mut Scope<'_>, this: &Value, _a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    gl::end_transform_feedback();
    g.st.borrow_mut().elem_data.clear();
    undefined()
}

pub(crate) fn transform_feedback_varyings(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let _g = current(s, this)?;
    if a.len() < 3 || !a[1].is_object() {
        return undefined();
    }
    let n = list_len(s, &a[1]);
    if n == 0 || n > 256 {
        return undefined();
    }
    let names = string_list(s, &a[1], n);
    let mode = uint(s, a, 2);
    gl::transform_feedback_varyings(name(a, 0), &names, mode);
    undefined()
}

fn number_array(s: &mut Scope<'_>, values: impl Iterator<Item = Value>) -> JsResult {
    let arr = s.new_array();
    for (i, v) in values.enumerate() {
        let _ = s.set_index(&arr, i as u32, v);
    }
    Ok(arr)
}

pub(crate) fn get_active_uniforms(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    if a.len() < 3 || !a[1].is_object() {
        return Ok(Value::null());
    }
    let n = list_len(s, &a[1]);
    if n == 0 || n > 4096 {
        return Ok(s.new_array());
    }
    let mut indices = Vec::with_capacity(n as usize);
    for i in 0..n {
        let v = match s.get_index(&a[1], i) {
            Ok(e) => s.to_int32(&e).unwrap_or(0),
            Err(_) => 0,
        };
        indices.push(v as u32);
    }
    let pname = uint(s, a, 2);
    let out = gl::get_active_uniforms(name(a, 0), &indices, pname);
    number_array(s, out.into_iter().map(Value::int))
}

pub(crate) fn get_active_uniform_block_parameter(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let _g = current(s, this)?;
    let program = name(a, 0);
    let index = uint(s, a, 1);
    let pname = uint(s, a, 2);
    if pname == UNIFORM_BLOCK_ACTIVE_UNIFORM_INDICES {
        let mut count = [0i32; 1];
        gl::get_active_uniform_block(program, index, UNIFORM_BLOCK_ACTIVE_UNIFORMS, &mut count);
        let count = if count[0] <= 0 || count[0] > 4096 {
            0
        } else {
            count[0] as usize
        };
        let mut vals = vec![0i32; count.max(1)];
        if count > 0 {
            gl::get_active_uniform_block(program, index, pname, &mut vals);
        }
        return uint_array(s, &vals[..count]);
    }
    let mut v = [0i32; 1];
    gl::get_active_uniform_block(program, index, pname, &mut v);
    Ok(Value::int(v[0]))
}

pub(crate) fn get_active_uniform_block_name(
    s: &mut Scope<'_>,
    this: &Value,
    a: &[Value],
) -> JsResult {
    let _g = current(s, this)?;
    let program = name(a, 0);
    let index = uint(s, a, 1);
    let block = gl::get_active_uniform_block_name(program, index);
    Ok(s.string_from_bytes(&block))
}

pub(crate) fn get_uniform_indices(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let _g = current(s, this)?;
    if a.len() < 2 || !a[1].is_object() {
        return Ok(Value::null());
    }
    let n = list_len(s, &a[1]);
    if n == 0 || n > 4096 {
        return Ok(s.new_array());
    }
    let names = string_list(s, &a[1], n);
    let indices = gl::get_uniform_indices(name(a, 0), &names);
    number_array(s, indices.into_iter().map(uint_value))
}

fn sync_of(g: &WebGl, v: &Value) -> Option<gl::Sync> {
    let id = sync_id(v)?;
    g.st.borrow().syncs.get(&id).copied()
}

pub(crate) fn fence_sync(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let condition = uint(s, a, 0);
    let flags = uint(s, a, 1);
    let Some(sync) = gl::fence_sync(condition, flags) else {
        return Ok(Value::null());
    };
    let id = {
        let mut st = g.st.borrow_mut();
        st.next_sync += 1;
        let id = st.next_sync;
        st.syncs.insert(id, sync);
        id
    };
    Ok(new_object(s, &g, id as u32, KIND_SYNC))
}

pub(crate) fn is_sync(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let sync = a.first().and_then(|v| sync_of(&g, v));
    Ok(Value::boolean(sync.is_some_and(gl::is_sync)))
}

pub(crate) fn delete_sync(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let Some(v) = a.first() else {
        return undefined();
    };
    if let Some(sync) = sync_of(&g, v) {
        gl::delete_sync(sync);
        g.st.borrow_mut().syncs.remove(&(name(a, 0) as i32));
    }
    undefined()
}

pub(crate) fn client_wait_sync(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let flags = uint(s, a, 1);
    let timeout = num(s, a, 2);
    let ns = if timeout.is_nan() || timeout <= 0.0 {
        0
    } else if timeout >= 18_446_744_073_709_551_616.0 {
        u64::MAX
    } else {
        timeout as u64
    };
    let Some(sync) = sync_of(&g, &arg(a, 0)) else {
        return Ok(Value::int(WAIT_FAILED as i32));
    };
    Ok(Value::int(gl::client_wait_sync(sync, flags, ns) as i32))
}

pub(crate) fn wait_sync(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let flags = uint(s, a, 1);
    if let Some(sync) = sync_of(&g, &arg(a, 0)) {
        gl::wait_sync(sync, flags, TIMEOUT_IGNORED);
    }
    undefined()
}

pub(crate) fn get_sync_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let pname = uint(s, a, 1);
    let Some(sync) = sync_of(&g, &arg(a, 0)) else {
        return Ok(Value::null());
    };
    Ok(Value::int(gl::get_sync_parameter(sync, pname)))
}

pub(crate) fn get_indexed_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let target = uint(s, a, 0);
    let index = uint(s, a, 1);
    match target {
        TRANSFORM_FEEDBACK_BUFFER_BINDING | UNIFORM_BUFFER_BINDING => {
            let buffer = gl::get_integer_indexed(target, index);
            Ok(wrap(s, &g, buffer as u32, KIND_BUFFER))
        }
        TRANSFORM_FEEDBACK_BUFFER_START
        | TRANSFORM_FEEDBACK_BUFFER_SIZE
        | UNIFORM_BUFFER_START
        | UNIFORM_BUFFER_SIZE => Ok(Value::int64(gl::get_integer64_indexed(target, index))),
        _ => {
            g.st.borrow_mut().injected_error = INVALID_ENUM;
            Ok(Value::null())
        }
    }
}

pub(crate) fn get_sampler_parameter(s: &mut Scope<'_>, this: &Value, a: &[Value]) -> JsResult {
    let g = current(s, this)?;
    let sampler = name(a, 0);
    let pname = uint(s, a, 1);
    match pname {
        TEXTURE_MAX_LOD | TEXTURE_MIN_LOD => Ok(Value::number(f64::from(
            gl::get_sampler_parameterf(sampler, pname),
        ))),
        TEXTURE_COMPARE_FUNC | TEXTURE_COMPARE_MODE | TEXTURE_MAG_FILTER | TEXTURE_MIN_FILTER
        | TEXTURE_WRAP_R | TEXTURE_WRAP_S | TEXTURE_WRAP_T => {
            Ok(Value::int(gl::get_sampler_parameteri(sampler, pname)))
        }
        _ => {
            g.st.borrow_mut().injected_error = INVALID_ENUM;
            Ok(Value::null())
        }
    }
}
