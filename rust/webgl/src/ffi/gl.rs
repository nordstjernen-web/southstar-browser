//! Southstar — the OpenGL ES entry points WebGL calls, read from libepoxy's dispatch pointers and wrapped over slices.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

macro_rules! gl_entry_points {
    (
        safe { $($safe:ident = $safe_sym:ident($($sarg:ident: $sty:ty),*) $(-> $sret:ty)?;)* }
        raw { $($raw:ident = $raw_sym:ident($($rarg:ident: $rty:ty),*) $(-> $rret:ty)?;)* }
    ) => {
        #[cfg_attr(windows, link(name = "epoxy", kind = "dylib"))]
        unsafe extern "C" {
            $(static $safe_sym: unsafe extern "system" fn($($sty),*) $(-> $sret)?;)*
            $(static $raw_sym: unsafe extern "system" fn($($rty),*) $(-> $rret)?;)*
            fn epoxy_is_desktop_gl() -> c_int;
            fn epoxy_gl_version() -> c_int;
        }
        $(
            #[allow(clippy::too_many_arguments)]
            pub(crate) fn $safe($($sarg: $sty),*) $(-> $sret)? {
                unsafe { $safe_sym($($sarg),*) }
            }
        )*
        $(
            #[allow(clippy::too_many_arguments)]
            unsafe fn $raw($($rarg: $rty),*) $(-> $rret)? {
                unsafe { $raw_sym($($rarg),*) }
            }
        )*
    };
}

gl_entry_points! {
    safe {
        bind_framebuffer = epoxy_glBindFramebuffer(target: u32, framebuffer: u32);
        bind_texture = epoxy_glBindTexture(target: u32, texture: u32);
        tex_parameter_i = epoxy_glTexParameteri(target: u32, pname: u32, param: i32);
        tex_parameter_f = epoxy_glTexParameterf(target: u32, pname: u32, param: f32);
        framebuffer_texture_2d = epoxy_glFramebufferTexture2D(
            target: u32, attachment: u32, textarget: u32, texture: u32, level: i32);
        bind_renderbuffer = epoxy_glBindRenderbuffer(target: u32, renderbuffer: u32);
        renderbuffer_storage_multisample = epoxy_glRenderbufferStorageMultisample(
            target: u32, samples: i32, format: u32, w: i32, h: i32);
        framebuffer_renderbuffer = epoxy_glFramebufferRenderbuffer(
            target: u32, attachment: u32, rbtarget: u32, renderbuffer: u32);
        check_framebuffer_status = epoxy_glCheckFramebufferStatus(target: u32) -> u32;
        renderbuffer_storage = epoxy_glRenderbufferStorage(target: u32, format: u32, w: i32, h: i32);
        viewport = epoxy_glViewport(x: i32, y: i32, w: i32, h: i32);
        clear_color = epoxy_glClearColor(r: f32, g: f32, b: f32, a: f32);
        clear = epoxy_glClear(mask: u32);
        clear_depthf = epoxy_glClearDepthf(depth: f32);
        clear_stencil = epoxy_glClearStencil(s: i32);
        scissor = epoxy_glScissor(x: i32, y: i32, w: i32, h: i32);
        enable = epoxy_glEnable(cap: u32);
        disable = epoxy_glDisable(cap: u32);
        is_enabled = epoxy_glIsEnabled(cap: u32) -> u8;
        depth_func = epoxy_glDepthFunc(func: u32);
        depth_mask = epoxy_glDepthMask(flag: u8);
        depth_rangef = epoxy_glDepthRangef(near: f32, far: f32);
        color_mask = epoxy_glColorMask(r: u8, g: u8, b: u8, a: u8);
        stencil_mask = epoxy_glStencilMask(mask: u32);
        stencil_func = epoxy_glStencilFunc(func: u32, reference: i32, mask: u32);
        stencil_op = epoxy_glStencilOp(fail: u32, zfail: u32, zpass: u32);
        blend_func = epoxy_glBlendFunc(sfactor: u32, dfactor: u32);
        blend_func_separate = epoxy_glBlendFuncSeparate(
            src_rgb: u32, dst_rgb: u32, src_alpha: u32, dst_alpha: u32);
        blend_equation = epoxy_glBlendEquation(mode: u32);
        blend_equation_separate = epoxy_glBlendEquationSeparate(mode_rgb: u32, mode_alpha: u32);
        blend_color = epoxy_glBlendColor(r: f32, g: f32, b: f32, a: f32);
        cull_face = epoxy_glCullFace(mode: u32);
        front_face = epoxy_glFrontFace(mode: u32);
        line_width = epoxy_glLineWidth(width: f32);
        polygon_offset = epoxy_glPolygonOffset(factor: f32, units: f32);
        hint = epoxy_glHint(target: u32, mode: u32);
        finish = epoxy_glFinish();
        flush = epoxy_glFlush();
        pixel_storei = epoxy_glPixelStorei(pname: u32, param: i32);
        get_error = epoxy_glGetError() -> u32;
        active_texture = epoxy_glActiveTexture(texture: u32);
        create_shader = epoxy_glCreateShader(kind: u32) -> u32;
        delete_shader = epoxy_glDeleteShader(shader: u32);
        compile_shader = epoxy_glCompileShader(shader: u32);
        create_program = epoxy_glCreateProgram() -> u32;
        delete_program = epoxy_glDeleteProgram(program: u32);
        attach_shader = epoxy_glAttachShader(program: u32, shader: u32);
        detach_shader = epoxy_glDetachShader(program: u32, shader: u32);
        link_program = epoxy_glLinkProgram(program: u32);
        validate_program = epoxy_glValidateProgram(program: u32);
        use_program = epoxy_glUseProgram(program: u32);
        bind_buffer = epoxy_glBindBuffer(target: u32, buffer: u32);
        enable_vertex_attrib_array = epoxy_glEnableVertexAttribArray(index: u32);
        disable_vertex_attrib_array = epoxy_glDisableVertexAttribArray(index: u32);
        vertex_attrib1f = epoxy_glVertexAttrib1f(index: u32, x: f32);
        vertex_attrib2f = epoxy_glVertexAttrib2f(index: u32, x: f32, y: f32);
        vertex_attrib3f = epoxy_glVertexAttrib3f(index: u32, x: f32, y: f32, z: f32);
        vertex_attrib4f = epoxy_glVertexAttrib4f(index: u32, x: f32, y: f32, z: f32, w: f32);
        uniform1f = epoxy_glUniform1f(location: i32, x: f32);
        uniform2f = epoxy_glUniform2f(location: i32, x: f32, y: f32);
        uniform3f = epoxy_glUniform3f(location: i32, x: f32, y: f32, z: f32);
        uniform4f = epoxy_glUniform4f(location: i32, x: f32, y: f32, z: f32, w: f32);
        uniform1i = epoxy_glUniform1i(location: i32, x: i32);
        uniform2i = epoxy_glUniform2i(location: i32, x: i32, y: i32);
        uniform3i = epoxy_glUniform3i(location: i32, x: i32, y: i32, z: i32);
        uniform4i = epoxy_glUniform4i(location: i32, x: i32, y: i32, z: i32, w: i32);
        uniform1ui = epoxy_glUniform1ui(location: i32, x: u32);
        uniform2ui = epoxy_glUniform2ui(location: i32, x: u32, y: u32);
        uniform3ui = epoxy_glUniform3ui(location: i32, x: u32, y: u32, z: u32);
        uniform4ui = epoxy_glUniform4ui(location: i32, x: u32, y: u32, z: u32, w: u32);
        unmap_buffer = epoxy_glUnmapBuffer(target: u32) -> u8;
        draw_arrays = epoxy_glDrawArrays(mode: u32, first: i32, count: i32);
        draw_arrays_instanced = epoxy_glDrawArraysInstanced(
            mode: u32, first: i32, count: i32, instances: i32);
        generate_mipmap = epoxy_glGenerateMipmap(target: u32);
        blit_framebuffer = epoxy_glBlitFramebuffer(
            sx0: i32, sy0: i32, sx1: i32, sy1: i32,
            dx0: i32, dy0: i32, dx1: i32, dy1: i32, mask: u32, filter: u32);
        is_buffer = epoxy_glIsBuffer(name: u32) -> u8;
        is_program = epoxy_glIsProgram(name: u32) -> u8;
        is_shader = epoxy_glIsShader(name: u32) -> u8;
        is_texture = epoxy_glIsTexture(name: u32) -> u8;
        is_framebuffer = epoxy_glIsFramebuffer(name: u32) -> u8;
        is_renderbuffer = epoxy_glIsRenderbuffer(name: u32) -> u8;
        is_vertex_array = epoxy_glIsVertexArray(name: u32) -> u8;
        is_sampler = epoxy_glIsSampler(name: u32) -> u8;
        is_query = epoxy_glIsQuery(name: u32) -> u8;
        is_transform_feedback = epoxy_glIsTransformFeedback(name: u32) -> u8;
        bind_vertex_array = epoxy_glBindVertexArray(array: u32);
        vertex_attrib_divisor = epoxy_glVertexAttribDivisor(index: u32, divisor: u32);
        tex_storage_2d = epoxy_glTexStorage2D(
            target: u32, levels: i32, format: u32, w: i32, h: i32);
        tex_storage_3d = epoxy_glTexStorage3D(
            target: u32, levels: i32, format: u32, w: i32, h: i32, d: i32);
        framebuffer_texture_layer = epoxy_glFramebufferTextureLayer(
            target: u32, attachment: u32, texture: u32, level: i32, layer: i32);
        read_buffer = epoxy_glReadBuffer(src: u32);
        copy_buffer_sub_data = epoxy_glCopyBufferSubData(
            read_target: u32, write_target: u32, read_offset: isize, write_offset: isize,
            size: isize);
        clear_bufferfi = epoxy_glClearBufferfi(buffer: u32, drawbuffer: i32, depth: f32, stencil: i32);
        bind_sampler = epoxy_glBindSampler(unit: u32, sampler: u32);
        sampler_parameter_i = epoxy_glSamplerParameteri(sampler: u32, pname: u32, param: i32);
        sampler_parameter_f = epoxy_glSamplerParameterf(sampler: u32, pname: u32, param: f32);
        uniform_block_binding = epoxy_glUniformBlockBinding(program: u32, index: u32, binding: u32);
        bind_buffer_base = epoxy_glBindBufferBase(target: u32, index: u32, buffer: u32);
        bind_buffer_range = epoxy_glBindBufferRange(
            target: u32, index: u32, buffer: u32, offset: isize, size: isize);
        copy_tex_image_2d = epoxy_glCopyTexImage2D(
            target: u32, level: i32, format: u32, x: i32, y: i32, w: i32, h: i32, border: i32);
        copy_tex_sub_image_2d = epoxy_glCopyTexSubImage2D(
            target: u32, level: i32, xoff: i32, yoff: i32, x: i32, y: i32, w: i32, h: i32);
        copy_tex_sub_image_3d = epoxy_glCopyTexSubImage3D(
            target: u32, level: i32, xoff: i32, yoff: i32, zoff: i32,
            x: i32, y: i32, w: i32, h: i32);
        vertex_attrib_i4i = epoxy_glVertexAttribI4i(index: u32, x: i32, y: i32, z: i32, w: i32);
        vertex_attrib_i4ui = epoxy_glVertexAttribI4ui(index: u32, x: u32, y: u32, z: u32, w: u32);
        begin_query = epoxy_glBeginQuery(target: u32, query: u32);
        end_query = epoxy_glEndQuery(target: u32);
        bind_transform_feedback = epoxy_glBindTransformFeedback(target: u32, feedback: u32);
        begin_transform_feedback = epoxy_glBeginTransformFeedback(mode: u32);
        end_transform_feedback = epoxy_glEndTransformFeedback();
        pause_transform_feedback = epoxy_glPauseTransformFeedback();
        resume_transform_feedback = epoxy_glResumeTransformFeedback();
        sample_coverage = epoxy_glSampleCoverage(value: f32, invert: u8);
        stencil_func_separate = epoxy_glStencilFuncSeparate(
            face: u32, func: u32, reference: i32, mask: u32);
        stencil_op_separate = epoxy_glStencilOpSeparate(face: u32, fail: u32, zfail: u32, zpass: u32);
        stencil_mask_separate = epoxy_glStencilMaskSeparate(face: u32, mask: u32);
    }
    raw {
        raw_tex_image_2d = epoxy_glTexImage2D(
            target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32,
            format: u32, kind: u32, pixels: *const c_void);
        raw_tex_sub_image_2d = epoxy_glTexSubImage2D(
            target: u32, level: i32, xoff: i32, yoff: i32, w: i32, h: i32,
            format: u32, kind: u32, pixels: *const c_void);
        raw_tex_image_3d = epoxy_glTexImage3D(
            target: u32, level: i32, internal: i32, w: i32, h: i32, d: i32, border: i32,
            format: u32, kind: u32, pixels: *const c_void);
        raw_tex_sub_image_3d = epoxy_glTexSubImage3D(
            target: u32, level: i32, xoff: i32, yoff: i32, zoff: i32, w: i32, h: i32, d: i32,
            format: u32, kind: u32, pixels: *const c_void);
        raw_read_pixels = epoxy_glReadPixels(
            x: i32, y: i32, w: i32, h: i32, format: u32, kind: u32, pixels: *mut c_void);
        raw_get_integerv = epoxy_glGetIntegerv(pname: u32, data: *mut i32);
        raw_get_floatv = epoxy_glGetFloatv(pname: u32, data: *mut f32);
        raw_get_integeri_v = epoxy_glGetIntegeri_v(target: u32, index: u32, data: *mut i32);
        raw_get_integer64i_v = epoxy_glGetInteger64i_v(target: u32, index: u32, data: *mut i64);
        raw_gen_framebuffers = epoxy_glGenFramebuffers(n: i32, names: *mut u32);
        raw_gen_textures = epoxy_glGenTextures(n: i32, names: *mut u32);
        raw_gen_renderbuffers = epoxy_glGenRenderbuffers(n: i32, names: *mut u32);
        raw_gen_buffers = epoxy_glGenBuffers(n: i32, names: *mut u32);
        raw_gen_vertex_arrays = epoxy_glGenVertexArrays(n: i32, names: *mut u32);
        raw_gen_samplers = epoxy_glGenSamplers(n: i32, names: *mut u32);
        raw_gen_queries = epoxy_glGenQueries(n: i32, names: *mut u32);
        raw_gen_transform_feedbacks = epoxy_glGenTransformFeedbacks(n: i32, names: *mut u32);
        raw_delete_framebuffers = epoxy_glDeleteFramebuffers(n: i32, names: *const u32);
        raw_delete_textures = epoxy_glDeleteTextures(n: i32, names: *const u32);
        raw_delete_renderbuffers = epoxy_glDeleteRenderbuffers(n: i32, names: *const u32);
        raw_delete_buffers = epoxy_glDeleteBuffers(n: i32, names: *const u32);
        raw_delete_vertex_arrays = epoxy_glDeleteVertexArrays(n: i32, names: *const u32);
        raw_delete_samplers = epoxy_glDeleteSamplers(n: i32, names: *const u32);
        raw_delete_queries = epoxy_glDeleteQueries(n: i32, names: *const u32);
        raw_delete_transform_feedbacks = epoxy_glDeleteTransformFeedbacks(
            n: i32, names: *const u32);
        raw_get_programiv = epoxy_glGetProgramiv(program: u32, pname: u32, out: *mut i32);
        raw_get_shaderiv = epoxy_glGetShaderiv(shader: u32, pname: u32, out: *mut i32);
        raw_get_shader_info_log = epoxy_glGetShaderInfoLog(
            shader: u32, size: i32, len: *mut i32, buf: *mut c_char);
        raw_get_shader_source = epoxy_glGetShaderSource(
            shader: u32, size: i32, len: *mut i32, buf: *mut c_char);
        raw_get_program_info_log = epoxy_glGetProgramInfoLog(
            program: u32, size: i32, len: *mut i32, buf: *mut c_char);
        raw_get_active_attrib = epoxy_glGetActiveAttrib(
            program: u32, index: u32, size: i32, len: *mut i32, out_size: *mut i32,
            out_type: *mut u32, name: *mut c_char);
        raw_get_active_uniform = epoxy_glGetActiveUniform(
            program: u32, index: u32, size: i32, len: *mut i32, out_size: *mut i32,
            out_type: *mut u32, name: *mut c_char);
        raw_get_transform_feedback_varying = epoxy_glGetTransformFeedbackVarying(
            program: u32, index: u32, size: i32, len: *mut i32, out_size: *mut i32,
            out_type: *mut u32, name: *mut c_char);
        raw_get_attrib_location = epoxy_glGetAttribLocation(
            program: u32, name: *const c_char) -> i32;
        raw_get_uniform_location = epoxy_glGetUniformLocation(
            program: u32, name: *const c_char) -> i32;
        raw_get_frag_data_location = epoxy_glGetFragDataLocation(
            program: u32, name: *const c_char) -> i32;
        raw_get_uniform_block_index = epoxy_glGetUniformBlockIndex(
            program: u32, name: *const c_char) -> u32;
        raw_bind_attrib_location = epoxy_glBindAttribLocation(
            program: u32, index: u32, name: *const c_char);
        raw_shader_source = epoxy_glShaderSource(
            shader: u32, count: i32, strings: *const *const c_char, lengths: *const i32);
        raw_get_vertex_attribiv = epoxy_glGetVertexAttribiv(index: u32, pname: u32, out: *mut i32);
        raw_get_vertex_attribfv = epoxy_glGetVertexAttribfv(index: u32, pname: u32, out: *mut f32);
        raw_get_vertex_attrib_pointerv = epoxy_glGetVertexAttribPointerv(
            index: u32, pname: u32, out: *mut *mut c_void);
        raw_get_shader_precision_format = epoxy_glGetShaderPrecisionFormat(
            shader: u32, precision: u32, range: *mut i32, out: *mut i32);
        raw_buffer_data = epoxy_glBufferData(
            target: u32, size: isize, data: *const c_void, usage: u32);
        raw_buffer_sub_data = epoxy_glBufferSubData(
            target: u32, offset: isize, size: isize, data: *const c_void);
        raw_map_buffer_range = epoxy_glMapBufferRange(
            target: u32, offset: isize, len: isize, access: u32) -> *mut c_void;
        raw_vertex_attrib_pointer = epoxy_glVertexAttribPointer(
            index: u32, size: i32, kind: u32, normalized: u8, stride: i32, offset: *const c_void);
        raw_vertex_attrib_i_pointer = epoxy_glVertexAttribIPointer(
            index: u32, size: i32, kind: u32, stride: i32, offset: *const c_void);
        raw_draw_elements = epoxy_glDrawElements(
            mode: u32, count: i32, kind: u32, offset: *const c_void);
        raw_draw_elements_instanced = epoxy_glDrawElementsInstanced(
            mode: u32, count: i32, kind: u32, offset: *const c_void, instances: i32);
        raw_draw_range_elements = epoxy_glDrawRangeElements(
            mode: u32, start: u32, end: u32, count: i32, kind: u32, offset: *const c_void);
        raw_uniform1fv = epoxy_glUniform1fv(location: i32, count: i32, v: *const f32);
        raw_uniform2fv = epoxy_glUniform2fv(location: i32, count: i32, v: *const f32);
        raw_uniform3fv = epoxy_glUniform3fv(location: i32, count: i32, v: *const f32);
        raw_uniform4fv = epoxy_glUniform4fv(location: i32, count: i32, v: *const f32);
        raw_uniform1iv = epoxy_glUniform1iv(location: i32, count: i32, v: *const i32);
        raw_uniform2iv = epoxy_glUniform2iv(location: i32, count: i32, v: *const i32);
        raw_uniform3iv = epoxy_glUniform3iv(location: i32, count: i32, v: *const i32);
        raw_uniform4iv = epoxy_glUniform4iv(location: i32, count: i32, v: *const i32);
        raw_uniform1uiv = epoxy_glUniform1uiv(location: i32, count: i32, v: *const u32);
        raw_uniform2uiv = epoxy_glUniform2uiv(location: i32, count: i32, v: *const u32);
        raw_uniform3uiv = epoxy_glUniform3uiv(location: i32, count: i32, v: *const u32);
        raw_uniform4uiv = epoxy_glUniform4uiv(location: i32, count: i32, v: *const u32);
        raw_uniform_matrix2fv = epoxy_glUniformMatrix2fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix3fv = epoxy_glUniformMatrix3fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix4fv = epoxy_glUniformMatrix4fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix2x3fv = epoxy_glUniformMatrix2x3fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix3x2fv = epoxy_glUniformMatrix3x2fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix2x4fv = epoxy_glUniformMatrix2x4fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix4x2fv = epoxy_glUniformMatrix4x2fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix3x4fv = epoxy_glUniformMatrix3x4fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_uniform_matrix4x3fv = epoxy_glUniformMatrix4x3fv(
            location: i32, count: i32, transpose: u8, v: *const f32);
        raw_vertex_attrib1fv = epoxy_glVertexAttrib1fv(index: u32, v: *const f32);
        raw_vertex_attrib2fv = epoxy_glVertexAttrib2fv(index: u32, v: *const f32);
        raw_vertex_attrib3fv = epoxy_glVertexAttrib3fv(index: u32, v: *const f32);
        raw_vertex_attrib4fv = epoxy_glVertexAttrib4fv(index: u32, v: *const f32);
        raw_vertex_attrib_i4iv = epoxy_glVertexAttribI4iv(index: u32, v: *const i32);
        raw_vertex_attrib_i4uiv = epoxy_glVertexAttribI4uiv(index: u32, v: *const u32);
        raw_get_buffer_parameteriv = epoxy_glGetBufferParameteriv(
            target: u32, pname: u32, out: *mut i32);
        raw_get_tex_parameteriv = epoxy_glGetTexParameteriv(target: u32, pname: u32, out: *mut i32);
        raw_get_renderbuffer_parameteriv = epoxy_glGetRenderbufferParameteriv(
            target: u32, pname: u32, out: *mut i32);
        raw_get_framebuffer_attachment_parameteriv = epoxy_glGetFramebufferAttachmentParameteriv(
            target: u32, attachment: u32, pname: u32, out: *mut i32);
        raw_draw_buffers = epoxy_glDrawBuffers(n: i32, bufs: *const u32);
        raw_invalidate_framebuffer = epoxy_glInvalidateFramebuffer(
            target: u32, n: i32, attachments: *const u32);
        raw_invalidate_sub_framebuffer = epoxy_glInvalidateSubFramebuffer(
            target: u32, n: i32, attachments: *const u32, x: i32, y: i32, w: i32, h: i32);
        raw_clear_bufferfv = epoxy_glClearBufferfv(buffer: u32, drawbuffer: i32, v: *const f32);
        raw_clear_bufferiv = epoxy_glClearBufferiv(buffer: u32, drawbuffer: i32, v: *const i32);
        raw_clear_bufferuiv = epoxy_glClearBufferuiv(buffer: u32, drawbuffer: i32, v: *const u32);
        raw_get_internalformativ = epoxy_glGetInternalformativ(
            target: u32, format: u32, pname: u32, count: i32, out: *mut i32);
        raw_get_query_objectuiv = epoxy_glGetQueryObjectuiv(query: u32, pname: u32, out: *mut u32);
        raw_get_queryiv = epoxy_glGetQueryiv(target: u32, pname: u32, out: *mut i32);
        raw_transform_feedback_varyings = epoxy_glTransformFeedbackVaryings(
            program: u32, count: i32, varyings: *const *const c_char, mode: u32);
        raw_get_uniform_indices = epoxy_glGetUniformIndices(
            program: u32, count: i32, names: *const *const c_char, indices: *mut u32);
        raw_get_active_uniformsiv = epoxy_glGetActiveUniformsiv(
            program: u32, count: i32, indices: *const u32, pname: u32, out: *mut i32);
        raw_get_active_uniform_blockiv = epoxy_glGetActiveUniformBlockiv(
            program: u32, index: u32, pname: u32, out: *mut i32);
        raw_get_active_uniform_block_name = epoxy_glGetActiveUniformBlockName(
            program: u32, index: u32, size: i32, len: *mut i32, name: *mut c_char);
        raw_get_attached_shaders = epoxy_glGetAttachedShaders(
            program: u32, max: i32, count: *mut i32, shaders: *mut u32);
        raw_get_sampler_parameterfv = epoxy_glGetSamplerParameterfv(
            sampler: u32, pname: u32, out: *mut f32);
        raw_get_sampler_parameteriv = epoxy_glGetSamplerParameteriv(
            sampler: u32, pname: u32, out: *mut i32);
        raw_fence_sync = epoxy_glFenceSync(condition: u32, flags: u32) -> *mut c_void;
        raw_is_sync = epoxy_glIsSync(sync: *mut c_void) -> u8;
        raw_delete_sync = epoxy_glDeleteSync(sync: *mut c_void);
        raw_client_wait_sync = epoxy_glClientWaitSync(
            sync: *mut c_void, flags: u32, timeout: u64) -> u32;
        raw_wait_sync = epoxy_glWaitSync(sync: *mut c_void, flags: u32, timeout: u64);
        raw_get_synciv = epoxy_glGetSynciv(
            sync: *mut c_void, pname: u32, size: i32, len: *mut i32, out: *mut i32);
    }
}

pub(crate) fn is_desktop_gl() -> bool {
    unsafe { epoxy_is_desktop_gl() != 0 }
}

pub(crate) fn gl_version() -> i32 {
    unsafe { epoxy_gl_version() }
}

fn c_name(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn pixels(data: Option<&[u8]>) -> *const c_void {
    data.map_or(ptr::null(), |d| d.as_ptr().cast())
}

fn len_i32<T>(v: &[T]) -> i32 {
    i32::try_from(v.len()).unwrap_or(i32::MAX)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sync(usize);

pub(crate) fn get_integer(pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_integerv(pname, &mut v) };
    v
}

pub(crate) fn get_integers(pname: u32, out: &mut [i32]) {
    unsafe { raw_get_integerv(pname, out.as_mut_ptr()) };
}

pub(crate) fn get_floats(pname: u32, out: &mut [f32; 4]) {
    unsafe { raw_get_floatv(pname, out.as_mut_ptr()) };
}

pub(crate) fn get_integer_indexed(target: u32, index: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_integeri_v(target, index, &mut v) };
    v
}

pub(crate) fn get_integer64_indexed(target: u32, index: u32) -> i64 {
    let mut v = 0;
    unsafe { raw_get_integer64i_v(target, index, &mut v) };
    v
}

macro_rules! gen_delete {
    ($($gen:ident / $delete:ident = $raw_gen:ident / $raw_delete:ident;)*) => {
        $(
            pub(crate) fn $gen() -> u32 {
                let mut name = 0;
                unsafe { $raw_gen(1, &mut name) };
                name
            }

            pub(crate) fn $delete(name: u32) {
                unsafe { $raw_delete(1, &name) };
            }
        )*
    };
}

gen_delete! {
    gen_framebuffer / delete_framebuffer = raw_gen_framebuffers / raw_delete_framebuffers;
    gen_texture / delete_texture = raw_gen_textures / raw_delete_textures;
    gen_renderbuffer / delete_renderbuffer = raw_gen_renderbuffers / raw_delete_renderbuffers;
    gen_buffer / delete_buffer = raw_gen_buffers / raw_delete_buffers;
    gen_vertex_array / delete_vertex_array = raw_gen_vertex_arrays / raw_delete_vertex_arrays;
    gen_sampler / delete_sampler = raw_gen_samplers / raw_delete_samplers;
    gen_query / delete_query = raw_gen_queries / raw_delete_queries;
    gen_transform_feedback / delete_transform_feedback =
        raw_gen_transform_feedbacks / raw_delete_transform_feedbacks;
}

pub(crate) fn tex_image_2d(
    target: u32,
    level: i32,
    internal: i32,
    size: (i32, i32),
    border: i32,
    pixel: (u32, u32),
    data: Option<&[u8]>,
) {
    let (format, kind) = pixel;
    unsafe {
        raw_tex_image_2d(
            target,
            level,
            internal,
            size.0,
            size.1,
            border,
            format,
            kind,
            pixels(data),
        )
    };
}

pub(crate) fn tex_sub_image_2d(
    target: u32,
    level: i32,
    offset: (i32, i32),
    size: (i32, i32),
    pixel: (u32, u32),
    data: &[u8],
) {
    let (format, kind) = pixel;
    unsafe {
        raw_tex_sub_image_2d(
            target,
            level,
            offset.0,
            offset.1,
            size.0,
            size.1,
            format,
            kind,
            data.as_ptr().cast(),
        )
    };
}

pub(crate) fn tex_image_3d(
    target: u32,
    level: i32,
    internal: i32,
    size: (i32, i32, i32),
    border: i32,
    pixel: (u32, u32),
    data: Option<&[u8]>,
) {
    let (format, kind) = pixel;
    unsafe {
        raw_tex_image_3d(
            target,
            level,
            internal,
            size.0,
            size.1,
            size.2,
            border,
            format,
            kind,
            pixels(data),
        )
    };
}

pub(crate) fn tex_sub_image_3d(
    target: u32,
    level: i32,
    offset: (i32, i32, i32),
    size: (i32, i32, i32),
    pixel: (u32, u32),
    data: &[u8],
) {
    let (format, kind) = pixel;
    unsafe {
        raw_tex_sub_image_3d(
            target,
            level,
            offset.0,
            offset.1,
            offset.2,
            size.0,
            size.1,
            size.2,
            format,
            kind,
            data.as_ptr().cast(),
        )
    };
}

pub(crate) fn read_pixels(rect: (i32, i32, i32, i32), format: u32, kind: u32, out: &mut [u8]) {
    unsafe {
        raw_read_pixels(
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            format,
            kind,
            out.as_mut_ptr().cast(),
        )
    };
}

pub(crate) fn get_programiv(program: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_programiv(program, pname, &mut v) };
    v
}

pub(crate) fn get_shaderiv(shader: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_shaderiv(shader, pname, &mut v) };
    v
}

#[derive(Clone, Copy)]
pub(crate) enum LogKind {
    ShaderInfo,
    ShaderSource,
    ProgramInfo,
}

pub(crate) fn object_log(kind: LogKind, name: u32, len: i32) -> Vec<u8> {
    let mut buf = vec![0u8; len as usize + 1];
    let out = buf.as_mut_ptr().cast::<c_char>();
    unsafe {
        match kind {
            LogKind::ShaderInfo => raw_get_shader_info_log(name, len, ptr::null_mut(), out),
            LogKind::ShaderSource => raw_get_shader_source(name, len, ptr::null_mut(), out),
            LogKind::ProgramInfo => raw_get_program_info_log(name, len, ptr::null_mut(), out),
        }
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    buf
}

#[derive(Clone, Copy)]
pub(crate) enum ActiveKind {
    Attrib,
    Uniform,
    TransformFeedbackVarying,
}

pub(crate) struct Active {
    pub size: i32,
    pub kind: u32,
    pub name: Vec<u8>,
}

pub(crate) fn active_variable(which: ActiveKind, program: u32, index: u32, max: i32) -> Active {
    let mut name = vec![0u8; max as usize + 1];
    let (mut size, mut kind) = (0i32, 0u32);
    let out = name.as_mut_ptr().cast::<c_char>();
    unsafe {
        match which {
            ActiveKind::Attrib => raw_get_active_attrib(
                program,
                index,
                max,
                ptr::null_mut(),
                &mut size,
                &mut kind,
                out,
            ),
            ActiveKind::Uniform => raw_get_active_uniform(
                program,
                index,
                max,
                ptr::null_mut(),
                &mut size,
                &mut kind,
                out,
            ),
            ActiveKind::TransformFeedbackVarying => raw_get_transform_feedback_varying(
                program,
                index,
                max,
                ptr::null_mut(),
                &mut size,
                &mut kind,
                out,
            ),
        }
    }
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    name.truncate(end);
    Active { size, kind, name }
}

pub(crate) fn get_attrib_location(program: u32, name: &[u8]) -> i32 {
    let name = c_name(name);
    unsafe { raw_get_attrib_location(program, name.as_ptr()) }
}

pub(crate) fn get_uniform_location(program: u32, name: &[u8]) -> i32 {
    let name = c_name(name);
    unsafe { raw_get_uniform_location(program, name.as_ptr()) }
}

pub(crate) fn get_frag_data_location(program: u32, name: &[u8]) -> i32 {
    let name = c_name(name);
    unsafe { raw_get_frag_data_location(program, name.as_ptr()) }
}

pub(crate) fn get_uniform_block_index(program: u32, name: &[u8]) -> u32 {
    let name = c_name(name);
    unsafe { raw_get_uniform_block_index(program, name.as_ptr()) }
}

pub(crate) fn bind_attrib_location(program: u32, index: u32, name: &[u8]) {
    let name = c_name(name);
    unsafe { raw_bind_attrib_location(program, index, name.as_ptr()) };
}

pub(crate) fn shader_source(shader: u32, source: &[u8]) {
    let text = source.as_ptr().cast::<c_char>();
    let len = len_i32(source);
    unsafe { raw_shader_source(shader, 1, &text, &len) };
}

pub(crate) fn get_vertex_attribi(index: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_vertex_attribiv(index, pname, &mut v) };
    v
}

pub(crate) fn get_vertex_attribf4(index: u32, pname: u32) -> [f32; 4] {
    let mut v = [0f32; 4];
    unsafe { raw_get_vertex_attribfv(index, pname, v.as_mut_ptr()) };
    v
}

pub(crate) fn get_vertex_attrib_offset(index: u32, pname: u32) -> usize {
    let mut p: *mut c_void = ptr::null_mut();
    unsafe { raw_get_vertex_attrib_pointerv(index, pname, &mut p) };
    p as usize
}

pub(crate) fn get_shader_precision_format(shader: u32, precision: u32) -> ([i32; 2], i32) {
    let mut range = [0i32; 2];
    let mut out = 0;
    unsafe { raw_get_shader_precision_format(shader, precision, range.as_mut_ptr(), &mut out) };
    (range, out)
}

pub(crate) fn buffer_data(target: u32, size: usize, data: Option<&[u8]>, usage: u32) {
    unsafe { raw_buffer_data(target, size as isize, pixels(data), usage) };
}

pub(crate) fn buffer_sub_data(target: u32, offset: isize, data: &[u8]) {
    unsafe { raw_buffer_sub_data(target, offset, data.len() as isize, data.as_ptr().cast()) };
}

pub(crate) fn read_buffer_range(target: u32, offset: isize, out: &mut [u8]) -> bool {
    let src = unsafe { raw_map_buffer_range(target, offset, out.len() as isize, MAP_READ_BIT) };
    if src.is_null() {
        return false;
    }
    unsafe { ptr::copy_nonoverlapping(src.cast::<u8>(), out.as_mut_ptr(), out.len()) };
    unmap_buffer(target);
    true
}

const MAP_READ_BIT: u32 = 0x0001;

pub(crate) fn vertex_attrib_pointer(
    index: u32,
    size: i32,
    kind: u32,
    normalized: bool,
    stride: i32,
    offset: usize,
) {
    unsafe {
        raw_vertex_attrib_pointer(
            index,
            size,
            kind,
            u8::from(normalized),
            stride,
            offset as *const c_void,
        )
    };
}

pub(crate) fn vertex_attrib_i_pointer(
    index: u32,
    size: i32,
    kind: u32,
    stride: i32,
    offset: usize,
) {
    unsafe { raw_vertex_attrib_i_pointer(index, size, kind, stride, offset as *const c_void) };
}

pub(crate) fn draw_elements(mode: u32, count: i32, kind: u32, offset: usize) {
    unsafe { raw_draw_elements(mode, count, kind, offset as *const c_void) };
}

pub(crate) fn draw_elements_instanced(
    mode: u32,
    count: i32,
    kind: u32,
    offset: usize,
    instances: i32,
) {
    unsafe { raw_draw_elements_instanced(mode, count, kind, offset as *const c_void, instances) };
}

pub(crate) fn draw_range_elements(
    mode: u32,
    range: (u32, u32),
    count: i32,
    kind: u32,
    offset: usize,
) {
    unsafe {
        raw_draw_range_elements(mode, range.0, range.1, count, kind, offset as *const c_void)
    };
}

pub(crate) fn uniform_fv(n: i32, location: i32, count: i32, v: &[f32]) {
    unsafe {
        match n {
            1 => raw_uniform1fv(location, count, v.as_ptr()),
            2 => raw_uniform2fv(location, count, v.as_ptr()),
            3 => raw_uniform3fv(location, count, v.as_ptr()),
            _ => raw_uniform4fv(location, count, v.as_ptr()),
        }
    }
}

pub(crate) fn uniform_iv(n: i32, location: i32, count: i32, v: &[i32]) {
    unsafe {
        match n {
            1 => raw_uniform1iv(location, count, v.as_ptr()),
            2 => raw_uniform2iv(location, count, v.as_ptr()),
            3 => raw_uniform3iv(location, count, v.as_ptr()),
            _ => raw_uniform4iv(location, count, v.as_ptr()),
        }
    }
}

pub(crate) fn uniform_uiv(n: i32, location: i32, count: i32, v: &[u32]) {
    unsafe {
        match n {
            1 => raw_uniform1uiv(location, count, v.as_ptr()),
            2 => raw_uniform2uiv(location, count, v.as_ptr()),
            3 => raw_uniform3uiv(location, count, v.as_ptr()),
            _ => raw_uniform4uiv(location, count, v.as_ptr()),
        }
    }
}

pub(crate) fn uniform_matrix_fv(rows: i32, cols: i32, location: i32, count: i32, v: &[f32]) {
    let p = v.as_ptr();
    unsafe {
        match (rows, cols) {
            (2, 2) => raw_uniform_matrix2fv(location, count, 0, p),
            (3, 3) => raw_uniform_matrix3fv(location, count, 0, p),
            (4, 4) => raw_uniform_matrix4fv(location, count, 0, p),
            (2, 3) => raw_uniform_matrix2x3fv(location, count, 0, p),
            (3, 2) => raw_uniform_matrix3x2fv(location, count, 0, p),
            (2, 4) => raw_uniform_matrix2x4fv(location, count, 0, p),
            (4, 2) => raw_uniform_matrix4x2fv(location, count, 0, p),
            (3, 4) => raw_uniform_matrix3x4fv(location, count, 0, p),
            _ => raw_uniform_matrix4x3fv(location, count, 0, p),
        }
    }
}

pub(crate) fn vertex_attrib_fv(n: i32, index: u32, v: &[f32; 4]) {
    unsafe {
        match n {
            1 => raw_vertex_attrib1fv(index, v.as_ptr()),
            2 => raw_vertex_attrib2fv(index, v.as_ptr()),
            3 => raw_vertex_attrib3fv(index, v.as_ptr()),
            _ => raw_vertex_attrib4fv(index, v.as_ptr()),
        }
    }
}

pub(crate) fn vertex_attrib_i4iv(index: u32, v: &[i32; 4]) {
    unsafe { raw_vertex_attrib_i4iv(index, v.as_ptr()) };
}

pub(crate) fn vertex_attrib_i4uiv(index: u32, v: &[u32; 4]) {
    unsafe { raw_vertex_attrib_i4uiv(index, v.as_ptr()) };
}

#[derive(Clone, Copy)]
pub(crate) enum TargetQuery {
    Buffer,
    Texture,
    Renderbuffer,
}

pub(crate) fn get_target_parameter(which: TargetQuery, target: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe {
        match which {
            TargetQuery::Buffer => raw_get_buffer_parameteriv(target, pname, &mut v),
            TargetQuery::Texture => raw_get_tex_parameteriv(target, pname, &mut v),
            TargetQuery::Renderbuffer => raw_get_renderbuffer_parameteriv(target, pname, &mut v),
        }
    }
    v
}

pub(crate) fn get_framebuffer_attachment_parameter(
    target: u32,
    attachment: u32,
    pname: u32,
) -> i32 {
    let mut v = 0;
    unsafe { raw_get_framebuffer_attachment_parameteriv(target, attachment, pname, &mut v) };
    v
}

pub(crate) fn draw_buffers(bufs: &[u32]) {
    unsafe { raw_draw_buffers(len_i32(bufs), bufs.as_ptr()) };
}

pub(crate) fn invalidate_framebuffer(target: u32, attachments: &[u32]) {
    unsafe { raw_invalidate_framebuffer(target, len_i32(attachments), attachments.as_ptr()) };
}

pub(crate) fn invalidate_sub_framebuffer(
    target: u32,
    attachments: &[u32],
    rect: (i32, i32, i32, i32),
) {
    unsafe {
        raw_invalidate_sub_framebuffer(
            target,
            len_i32(attachments),
            attachments.as_ptr(),
            rect.0,
            rect.1,
            rect.2,
            rect.3,
        )
    };
}

pub(crate) fn clear_bufferfv(buffer: u32, drawbuffer: i32, v: &[f32; 4]) {
    unsafe { raw_clear_bufferfv(buffer, drawbuffer, v.as_ptr()) };
}

pub(crate) fn clear_bufferiv(buffer: u32, drawbuffer: i32, v: &[i32; 4]) {
    unsafe { raw_clear_bufferiv(buffer, drawbuffer, v.as_ptr()) };
}

pub(crate) fn clear_bufferuiv(buffer: u32, drawbuffer: i32, v: &[u32; 4]) {
    unsafe { raw_clear_bufferuiv(buffer, drawbuffer, v.as_ptr()) };
}

pub(crate) fn get_internalformativ(target: u32, format: u32, pname: u32, out: &mut [i32]) {
    unsafe { raw_get_internalformativ(target, format, pname, len_i32(out), out.as_mut_ptr()) };
}

pub(crate) fn get_query_objectui(query: u32, pname: u32) -> u32 {
    let mut v = 0;
    unsafe { raw_get_query_objectuiv(query, pname, &mut v) };
    v
}

pub(crate) fn get_queryi(target: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_queryiv(target, pname, &mut v) };
    v
}

fn c_names(names: &[Vec<u8>]) -> (Vec<CString>, Vec<*const c_char>) {
    let owned: Vec<CString> = names.iter().map(|n| c_name(n)).collect();
    let pointers = owned.iter().map(|n| n.as_ptr()).collect();
    (owned, pointers)
}

pub(crate) fn transform_feedback_varyings(program: u32, names: &[Vec<u8>], mode: u32) {
    let (_owned, pointers) = c_names(names);
    unsafe {
        raw_transform_feedback_varyings(program, len_i32(&pointers), pointers.as_ptr(), mode)
    };
}

pub(crate) fn get_uniform_indices(program: u32, names: &[Vec<u8>]) -> Vec<u32> {
    let (_owned, pointers) = c_names(names);
    let mut indices = vec![0u32; pointers.len()];
    unsafe {
        raw_get_uniform_indices(
            program,
            len_i32(&pointers),
            pointers.as_ptr(),
            indices.as_mut_ptr(),
        )
    };
    indices
}

pub(crate) fn get_active_uniforms(program: u32, indices: &[u32], pname: u32) -> Vec<i32> {
    let mut out = vec![0i32; indices.len()];
    unsafe {
        raw_get_active_uniformsiv(
            program,
            len_i32(indices),
            indices.as_ptr(),
            pname,
            out.as_mut_ptr(),
        )
    };
    out
}

pub(crate) fn get_active_uniform_block(program: u32, index: u32, pname: u32, out: &mut [i32]) {
    unsafe { raw_get_active_uniform_blockiv(program, index, pname, out.as_mut_ptr()) };
}

pub(crate) fn get_active_uniform_block_name(program: u32, index: u32) -> Vec<u8> {
    let mut name = [0u8; 256];
    unsafe {
        raw_get_active_uniform_block_name(
            program,
            index,
            255,
            ptr::null_mut(),
            name.as_mut_ptr().cast(),
        )
    };
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    name[..end].to_vec()
}

pub(crate) fn get_attached_shaders(program: u32) -> Vec<u32> {
    let mut shaders = [0u32; 16];
    let mut count = 0;
    unsafe { raw_get_attached_shaders(program, 16, &mut count, shaders.as_mut_ptr()) };
    shaders[..count.clamp(0, 16) as usize].to_vec()
}

pub(crate) fn get_sampler_parameterf(sampler: u32, pname: u32) -> f32 {
    let mut v = 0f32;
    unsafe { raw_get_sampler_parameterfv(sampler, pname, &mut v) };
    v
}

pub(crate) fn get_sampler_parameteri(sampler: u32, pname: u32) -> i32 {
    let mut v = 0;
    unsafe { raw_get_sampler_parameteriv(sampler, pname, &mut v) };
    v
}

pub(crate) fn fence_sync(condition: u32, flags: u32) -> Option<Sync> {
    let sync = unsafe { raw_fence_sync(condition, flags) };
    (!sync.is_null()).then_some(Sync(sync as usize))
}

pub(crate) fn is_sync(sync: Sync) -> bool {
    unsafe { raw_is_sync(sync.0 as *mut c_void) != 0 }
}

pub(crate) fn delete_sync(sync: Sync) {
    unsafe { raw_delete_sync(sync.0 as *mut c_void) };
}

pub(crate) fn client_wait_sync(sync: Sync, flags: u32, timeout: u64) -> u32 {
    unsafe { raw_client_wait_sync(sync.0 as *mut c_void, flags, timeout) }
}

pub(crate) fn wait_sync(sync: Sync, flags: u32, timeout: u64) {
    unsafe { raw_wait_sync(sync.0 as *mut c_void, flags, timeout) };
}

pub(crate) fn get_sync_parameter(sync: Sync, pname: u32) -> i32 {
    let (mut v, mut len) = (0, 0);
    unsafe { raw_get_synciv(sync.0 as *mut c_void, pname, 1, &mut len, &mut v) };
    v
}
