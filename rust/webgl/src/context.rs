//! Southstar — one WebGL context: its GL context, the drawing-buffer framebuffers and the state WebGL tracks beside GL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::ptr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::consts::*;
use crate::ffi::gl;
use crate::ffi::host::{self, GlContext, Surface};
use crate::limits;

#[derive(Clone, Copy)]
pub(crate) struct Attributes {
    pub(crate) alpha: bool,
    pub(crate) depth: bool,
    pub(crate) stencil: bool,
    pub(crate) antialias: bool,
    pub(crate) preserve: bool,
    pub(crate) premultiplied_alpha: bool,
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) fbo: u32,
    pub(crate) color_tex: u32,
    pub(crate) depth_rb: u32,
    pub(crate) draw_fbo: u32,
    pub(crate) msaa_color_rb: u32,
    pub(crate) msaa_depth_rb: u32,
    pub(crate) user_draw_fbo: u32,
    pub(crate) user_read_fbo: u32,
    pub(crate) bound_draw_fbo: u32,
    pub(crate) bound_read_fbo: u32,
    pub(crate) samples: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
    size_attr_gen: u32,
    size_synced: bool,
    pub(crate) surf: Option<Surface>,
    readback: Vec<u8>,
    dirty: bool,
    repaint_queued: bool,
    pub(crate) injected_error: u32,
    pub(crate) drawing_p3: bool,
    pub(crate) unpack_p3: bool,
    pub(crate) unpack_flip_y: bool,
    pub(crate) premultiply: bool,
    pub(crate) syncs: HashMap<i32, gl::Sync>,
    pub(crate) buffer_sizes: HashMap<u32, usize>,
    pub(crate) elem_data: HashMap<u32, Vec<u8>>,
    pub(crate) next_sync: i32,
}

pub(crate) struct WebGl {
    pub(crate) gl: GlContext,
    pub(crate) js: usize,
    pub(crate) canvas: usize,
    pub(crate) version: i32,
    pub(crate) attrs: Attributes,
    pub(crate) st: RefCell<State>,
}

thread_local! {
    static ACTIVE: RefCell<Weak<WebGl>> = const { RefCell::new(Weak::new()) };
}

pub(crate) fn active() -> Weak<WebGl> {
    ACTIVE.with(|a| a.borrow().clone())
}

fn set_active(g: &Rc<WebGl>) {
    ACTIVE.with(|a| *a.borrow_mut() = Rc::downgrade(g));
}

pub(crate) fn reassert(keep: &Weak<WebGl>) {
    if let Some(g) = keep.upgrade() {
        g.gl.make_current();
        set_active(&g);
    }
}

fn parse_long(text: &[u8]) -> i64 {
    let mut rest = text;
    while let Some((&c, tail)) = rest.split_first() {
        if matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r') {
            rest = tail;
        } else {
            break;
        }
    }
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'-' | b'+')) {
        rest = &rest[1..];
    }
    let mut value: i64 = 0;
    for &c in rest.iter().take_while(|c| c.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(i64::from(c - b'0'));
    }
    if negative { -value } else { value }
}

fn dim(canvas: usize, name: &core::ffi::CStr, default: i32) -> i32 {
    let Some(text) = host::canvas_node(canvas).and_then(|n| n.attr(name)) else {
        return default;
    };
    let text = text.to_bytes();
    if text.is_empty() {
        return default;
    }
    let v = parse_long(text);
    if v <= 0 {
        return default;
    }
    v.min(8192) as i32
}

impl State {
    pub(crate) fn draw_target(&self) -> u32 {
        if self.samples > 1 {
            self.draw_fbo
        } else {
            self.fbo
        }
    }

    pub(crate) fn bind_framebuffer(&mut self, target: u32, fbo: u32) {
        match target {
            FRAMEBUFFER => {
                if self.bound_draw_fbo == fbo && self.bound_read_fbo == fbo {
                    return;
                }
                gl::bind_framebuffer(FRAMEBUFFER, fbo);
                self.bound_draw_fbo = fbo;
                self.bound_read_fbo = fbo;
            }
            DRAW_FRAMEBUFFER => {
                if self.bound_draw_fbo == fbo {
                    return;
                }
                gl::bind_framebuffer(DRAW_FRAMEBUFFER, fbo);
                self.bound_draw_fbo = fbo;
            }
            READ_FRAMEBUFFER => {
                if self.bound_read_fbo == fbo {
                    return;
                }
                gl::bind_framebuffer(READ_FRAMEBUFFER, fbo);
                self.bound_read_fbo = fbo;
            }
            _ => gl::bind_framebuffer(target, fbo),
        }
    }

    fn bind_current_targets(&mut self) {
        let dt = self.draw_target();
        let draw = if self.user_draw_fbo != 0 {
            self.user_draw_fbo
        } else {
            dt
        };
        let read = if self.user_read_fbo != 0 {
            self.user_read_fbo
        } else {
            dt
        };
        if draw == read {
            self.bind_framebuffer(FRAMEBUFFER, draw);
        } else {
            self.bind_framebuffer(DRAW_FRAMEBUFFER, draw);
            self.bind_framebuffer(READ_FRAMEBUFFER, read);
        }
    }

    pub(crate) fn alloc_storage(&mut self, attrs: &Attributes, w: i32, h: i32) -> bool {
        let ds_format = if attrs.depth && attrs.stencil {
            DEPTH24_STENCIL8
        } else if attrs.stencil {
            STENCIL_INDEX8
        } else if attrs.depth {
            DEPTH_COMPONENT16
        } else {
            0
        };
        let ds_attach = if attrs.depth && attrs.stencil {
            DEPTH_STENCIL_ATTACHMENT
        } else if attrs.stencil {
            STENCIL_ATTACHMENT
        } else {
            DEPTH_ATTACHMENT
        };

        self.bind_framebuffer(FRAMEBUFFER, self.fbo);
        gl::bind_texture(TEXTURE_2D, self.color_tex);
        gl::tex_image_2d(
            TEXTURE_2D,
            0,
            RGBA as i32,
            (w, h),
            0,
            (RGBA, UNSIGNED_BYTE),
            None,
        );
        gl::tex_parameter_i(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR);
        gl::tex_parameter_i(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR);
        gl::tex_parameter_i(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE);
        gl::tex_parameter_i(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE);
        gl::framebuffer_texture_2d(
            FRAMEBUFFER,
            COLOR_ATTACHMENT0,
            TEXTURE_2D,
            self.color_tex,
            0,
        );

        if self.samples > 1 {
            self.bind_framebuffer(FRAMEBUFFER, self.draw_fbo);
            gl::bind_renderbuffer(RENDERBUFFER, self.msaa_color_rb);
            gl::renderbuffer_storage_multisample(RENDERBUFFER, self.samples, RGBA8, w, h);
            gl::framebuffer_renderbuffer(
                FRAMEBUFFER,
                COLOR_ATTACHMENT0,
                RENDERBUFFER,
                self.msaa_color_rb,
            );
            if ds_format != 0 {
                gl::bind_renderbuffer(RENDERBUFFER, self.msaa_depth_rb);
                gl::renderbuffer_storage_multisample(RENDERBUFFER, self.samples, ds_format, w, h);
                gl::framebuffer_renderbuffer(
                    FRAMEBUFFER,
                    ds_attach,
                    RENDERBUFFER,
                    self.msaa_depth_rb,
                );
            }
            if gl::check_framebuffer_status(FRAMEBUFFER) != FRAMEBUFFER_COMPLETE {
                self.samples = 1;
            }
        }

        if self.samples <= 1 && ds_format != 0 {
            self.bind_framebuffer(FRAMEBUFFER, self.fbo);
            gl::bind_renderbuffer(RENDERBUFFER, self.depth_rb);
            gl::renderbuffer_storage(RENDERBUFFER, ds_format, w, h);
            gl::framebuffer_renderbuffer(FRAMEBUFFER, ds_attach, RENDERBUFFER, self.depth_rb);
        }

        let target = self.draw_target();
        self.bind_framebuffer(FRAMEBUFFER, target);
        gl::check_framebuffer_status(FRAMEBUFFER) == FRAMEBUFFER_COMPLETE
    }

    fn mark_dirty(&mut self) -> bool {
        self.dirty = true;
        let request = !self.repaint_queued;
        self.repaint_queued = true;
        request
    }

    pub(crate) fn buffer_size(&self, name: u32) -> usize {
        if name == 0 {
            return 0;
        }
        self.buffer_sizes.get(&name).copied().unwrap_or(0)
    }

    pub(crate) fn set_buffer_size(&mut self, name: u32, size: usize) {
        if name != 0 {
            self.buffer_sizes.insert(name, size);
        }
    }

    pub(crate) fn elem_clear(&mut self, name: u32) {
        if name != 0 {
            self.elem_data.remove(&name);
        }
    }

    pub(crate) fn elem_set(&mut self, name: u32, data: Option<&[u8]>, len: usize) {
        if name == 0 {
            return;
        }
        let bytes = data.map_or_else(|| vec![0u8; len], |d| d[..len].to_vec());
        self.elem_data.insert(name, bytes);
    }

    pub(crate) fn elem_patch(&mut self, name: u32, offset: usize, data: &[u8]) {
        if let Some(shadow) = self.elem_data.get_mut(&name)
            && let Some(end) = offset.checked_add(data.len())
            && end <= shadow.len()
        {
            shadow[offset..end].copy_from_slice(data);
        }
    }

    fn elem_load(&mut self, version: i32, name: u32) -> bool {
        self.elem_clear(name);
        let size = self.buffer_size(name);
        if version < 2 || size == 0 {
            return false;
        }
        let mut data = vec![0u8; size];
        if !gl::read_buffer_range(ELEMENT_ARRAY_BUFFER, 0, &mut data) {
            return false;
        }
        if name != 0 {
            self.elem_data.insert(name, data);
            true
        } else {
            false
        }
    }

    pub(crate) fn buffer_range_ok(&self, target: u32, offset: isize, len: usize) -> bool {
        if offset < 0 {
            return false;
        }
        let size = self.buffer_size(limits::bound_buffer(target));
        if size == 0 {
            return false;
        }
        (offset as u64)
            .checked_add(len as u64)
            .is_some_and(|end| end <= size as u64)
    }

    pub(crate) fn attribs_cover(&self, version: i32, vertex_last: i64, instances: i64) -> bool {
        if vertex_last < 0 {
            return true;
        }
        let used = limits::program_attrib_mask();
        for i in 0..64u32 {
            if used & (1u64 << i) == 0 {
                continue;
            }
            if gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_ENABLED) == 0 {
                continue;
            }
            let buffer = gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_BUFFER_BINDING);
            if buffer <= 0 {
                return false;
            }
            let size = gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_SIZE);
            let kind = gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_TYPE);
            let stride = gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_STRIDE);
            let divisor = if version >= 2 {
                gl::get_vertex_attribi(i, VERTEX_ATTRIB_ARRAY_DIVISOR)
            } else {
                0
            };
            let pointer = gl::get_vertex_attrib_offset(i, VERTEX_ATTRIB_ARRAY_POINTER) as u64;
            let ebytes = limits::attr_elem_bytes(kind as u32, size);
            if ebytes == 0 || stride < 0 {
                return false;
            }
            let last = if divisor == 0 {
                vertex_last
            } else {
                (instances - 1) / i64::from(divisor as u32)
            };
            if last < 0 {
                continue;
            }
            let eff = if stride != 0 { stride as u64 } else { ebytes };
            let need = eff
                .checked_mul(last as u64)
                .and_then(|n| n.checked_add(pointer))
                .and_then(|n| n.checked_add(ebytes));
            match need {
                Some(need) if need <= self.buffer_size(buffer as u32) as u64 => {}
                _ => return false,
            }
        }
        true
    }

    fn elements_in_range(&self, count: i32, kind: u32, offset: isize) -> bool {
        if count < 0 || offset < 0 {
            return false;
        }
        let isz = limits::index_bytes(kind);
        if isz == 0 {
            return false;
        }
        let size = self.buffer_size(limits::bound_buffer(ELEMENT_ARRAY_BUFFER));
        if size == 0 {
            return false;
        }
        (count as u64)
            .checked_mul(isz as u64)
            .and_then(|n| n.checked_add(offset as u64))
            .is_some_and(|span| span <= size as u64)
    }

    pub(crate) fn draw_elements_ok(
        &mut self,
        version: i32,
        count: i32,
        kind: u32,
        offset: isize,
        instances: i64,
    ) -> bool {
        if !self.elements_in_range(count, kind, offset) {
            return false;
        }
        if count <= 0 || instances <= 0 {
            return true;
        }
        let ebuf = limits::bound_buffer(ELEMENT_ARRAY_BUFFER);
        let have = ebuf != 0 && self.elem_data.contains_key(&ebuf);
        if (!have || transform_feedback_active(version)) && !self.elem_load(version, ebuf) {
            return false;
        }
        let Some(shadow) = self.elem_data.get(&ebuf) else {
            return false;
        };
        let Some(max) = limits::elem_max_index(
            shadow,
            offset as usize,
            count as usize,
            limits::index_bytes(kind),
            version,
        ) else {
            return false;
        };
        self.attribs_cover(version, max as i64, instances)
    }
}

fn transform_feedback_active(version: i32) -> bool {
    version >= 2 && gl::get_integer(TRANSFORM_FEEDBACK_ACTIVE) != 0
}

impl WebGl {
    pub(crate) fn make(
        gl: GlContext,
        js: usize,
        canvas: usize,
        version: i32,
        mut attrs: Attributes,
    ) -> Option<Rc<WebGl>> {
        let mut st = State {
            w: dim(canvas, c"width", 300),
            h: dim(canvas, c"height", 150),
            dirty: true,
            samples: 1,
            ..State::default()
        };
        if attrs.antialias {
            let max_samples = gl::get_integer(MAX_SAMPLES);
            st.samples = if max_samples >= 4 {
                4
            } else if max_samples > 1 {
                max_samples
            } else {
                1
            };
        }
        attrs.antialias = st.samples > 1;
        st.fbo = gl::gen_framebuffer();
        st.color_tex = gl::gen_texture();
        st.depth_rb = gl::gen_renderbuffer();
        if st.samples > 1 {
            st.draw_fbo = gl::gen_framebuffer();
            st.msaa_color_rb = gl::gen_renderbuffer();
            st.msaa_depth_rb = gl::gen_renderbuffer();
        }
        let (w, h) = (st.w, st.h);
        let ok = st.alloc_storage(&attrs, w, h);
        let g = Rc::new(WebGl {
            gl,
            js,
            canvas,
            version,
            attrs,
            st: RefCell::new(st),
        });
        if !ok {
            return None;
        }
        gl::viewport(0, 0, w, h);
        gl::clear_color(0.0, 0.0, 0.0, 0.0);
        gl::clear(COLOR_BUFFER_BIT);
        Some(g)
    }

    pub(crate) fn mark_dirty(&self) {
        let request = self.st.borrow_mut().mark_dirty();
        if request {
            host::request_repaint(self.js);
        }
    }

    pub(crate) fn sync_size(&self) {
        let attr_gen = host::canvas_node(self.canvas).map_or(0, |n| n.attr_gen());
        {
            let mut st = self.st.borrow_mut();
            if st.size_synced && attr_gen == st.size_attr_gen {
                return;
            }
            st.size_attr_gen = attr_gen;
            st.size_synced = true;
        }
        let w = dim(self.canvas, c"width", 300);
        let h = dim(self.canvas, c"height", 150);
        if !self.resize(w, h, false) {
            return;
        }
        self.mark_dirty();
        self.st.borrow_mut().surf = None;
    }

    pub(crate) fn resize(&self, w: i32, h: i32, force: bool) -> bool {
        let mut st = self.st.borrow_mut();
        if !force && w == st.w && h == st.h {
            return false;
        }
        st.w = w;
        st.h = h;
        st.alloc_storage(&self.attrs, w, h);
        gl::viewport(0, 0, w, h);
        true
    }

    pub(crate) fn enter(self: &Rc<Self>) {
        self.gl.make_current();
        set_active(self);
        self.sync_size();
        self.st.borrow_mut().bind_current_targets();
    }

    pub(crate) fn enter_synced(&self) {
        self.gl.make_current();
        self.sync_size();
    }

    pub(crate) fn canvas_surface(self: &Rc<Self>) -> *mut c_void {
        self.gl.make_current();
        self.sync_size();
        let mut st = self.st.borrow_mut();
        let (w, h) = (st.w, st.h);
        if w <= 0 || h <= 0 {
            return ptr::null_mut();
        }
        if !st.dirty
            && let Some(surf) = &st.surf
        {
            return surf.as_ptr();
        }
        if st.samples > 1 {
            let (draw_fbo, fbo) = (st.draw_fbo, st.fbo);
            st.bind_framebuffer(READ_FRAMEBUFFER, draw_fbo);
            st.bind_framebuffer(DRAW_FRAMEBUFFER, fbo);
            gl::blit_framebuffer(0, 0, w, h, 0, 0, w, h, COLOR_BUFFER_BIT, NEAREST);
        }
        let fbo = st.fbo;
        st.bind_framebuffer(FRAMEBUFFER, fbo);
        if st.surf.is_none() {
            match Surface::new(w, h) {
                Some(surf) => st.surf = Some(surf),
                None => return ptr::null_mut(),
            }
        }
        let st = &mut *st;
        let Some(surf) = st.surf.as_mut() else {
            return ptr::null_mut();
        };
        surf.flush();
        let need = w as usize * h as usize * 4;
        if st.readback.len() < need {
            st.readback.resize(need, 0);
        }
        let rgba = &mut st.readback[..need];
        let pack = PackState::tight();
        gl::read_pixels((0, 0, w, h), RGBA, UNSIGNED_BYTE, rgba);
        pack.restore();
        let alpha = self.attrs.alpha;
        if let Some((dst, stride)) = surf.pixels() {
            for y in 0..h as usize {
                let src = &rgba[(h as usize - 1 - y) * w as usize * 4..][..w as usize * 4];
                let row = &mut dst[y * stride..][..w as usize * 4];
                copy_row(row, src, alpha);
            }
        }
        surf.mark_dirty();
        st.dirty = false;
        st.repaint_queued = false;
        surf.as_ptr()
    }
}

fn copy_row(row: &mut [u8], src: &[u8], alpha: bool) {
    let opaque = !alpha || src.chunks_exact(4).all(|p| p[3] == 255);
    for (o, p) in row.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let (r, g, b, a) = (
            u32::from(p[0]),
            u32::from(p[1]),
            u32::from(p[2]),
            u32::from(p[3]),
        );
        if opaque {
            o[0] = b as u8;
            o[1] = g as u8;
            o[2] = r as u8;
            o[3] = if alpha { a as u8 } else { 255 };
        } else if a == 255 {
            o[0] = b as u8;
            o[1] = g as u8;
            o[2] = r as u8;
            o[3] = 255;
        } else {
            o[0] = ((b * a + 127) / 255) as u8;
            o[1] = ((g * a + 127) / 255) as u8;
            o[2] = ((r * a + 127) / 255) as u8;
            o[3] = a as u8;
        }
    }
}

struct PackState {
    extended: bool,
    align: i32,
    row_length: i32,
    skip_rows: i32,
    skip_pixels: i32,
    pack_buffer: i32,
}

impl PackState {
    fn tight() -> PackState {
        let extended = gl::is_desktop_gl() || gl::gl_version() >= 30;
        let mut s = PackState {
            extended,
            align: gl::get_integer(PACK_ALIGNMENT),
            row_length: 0,
            skip_rows: 0,
            skip_pixels: 0,
            pack_buffer: 0,
        };
        gl::pixel_storei(PACK_ALIGNMENT, 4);
        if !extended {
            return s;
        }
        s.row_length = gl::get_integer(PACK_ROW_LENGTH);
        s.skip_rows = gl::get_integer(PACK_SKIP_ROWS);
        s.skip_pixels = gl::get_integer(PACK_SKIP_PIXELS);
        s.pack_buffer = gl::get_integer(PIXEL_PACK_BUFFER_BINDING);
        gl::pixel_storei(PACK_ROW_LENGTH, 0);
        gl::pixel_storei(PACK_SKIP_ROWS, 0);
        gl::pixel_storei(PACK_SKIP_PIXELS, 0);
        if s.pack_buffer != 0 {
            gl::bind_buffer(PIXEL_PACK_BUFFER, 0);
        }
        s
    }

    fn restore(&self) {
        gl::pixel_storei(PACK_ALIGNMENT, self.align);
        if !self.extended {
            return;
        }
        gl::pixel_storei(PACK_ROW_LENGTH, self.row_length);
        gl::pixel_storei(PACK_SKIP_ROWS, self.skip_rows);
        gl::pixel_storei(PACK_SKIP_PIXELS, self.skip_pixels);
        if self.pack_buffer != 0 {
            gl::bind_buffer(PIXEL_PACK_BUFFER, self.pack_buffer as u32);
        }
    }
}

impl Drop for WebGl {
    fn drop(&mut self) {
        host::forget_context(self.canvas);
        self.gl.make_current();
        let st = self.st.get_mut();
        gl::delete_framebuffer(st.fbo);
        gl::delete_texture(st.color_tex);
        gl::delete_renderbuffer(st.depth_rb);
        if st.draw_fbo != 0 {
            gl::delete_framebuffer(st.draw_fbo);
        }
        if st.msaa_color_rb != 0 {
            gl::delete_renderbuffer(st.msaa_color_rb);
        }
        if st.msaa_depth_rb != 0 {
            gl::delete_renderbuffer(st.msaa_depth_rb);
        }
        for sync in st.syncs.values() {
            gl::delete_sync(*sync);
        }
        self.gl.release();
        let me = self as *const WebGl;
        ACTIVE.with(|a| {
            let Ok(mut active) = a.try_borrow_mut() else {
                return;
            };
            if ptr::eq(active.as_ptr(), me) {
                *active = Weak::new();
            } else if let Some(other) = active.upgrade() {
                other.gl.make_current();
            }
        });
    }
}
