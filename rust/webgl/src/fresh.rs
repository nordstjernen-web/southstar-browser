//! Southstar — zero-initialising storage WebGL has just allocated, so a page never reads memory the driver recycled.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::consts::*;
use crate::context::State;
use crate::ffi::gl;
use crate::limits;

const DEPTH_BUFFER_BIT: u32 = 0x0100;
const STENCIL_BUFFER_BIT: u32 = 0x0400;
const FRONT: u32 = 0x0404;
const BACK: u32 = 0x0405;
const COLOR: u32 = 0x1800;
const COLOR_WRITEMASK: u32 = 0x0C23;
const DEPTH_WRITEMASK: u32 = 0x0B72;
const STENCIL_CLEAR_VALUE: u32 = 0x0B91;
const RENDERBUFFER_BINDING: u32 = 0x8CA7;
const RGBA_INTEGER: u32 = 0x8D99;
const TEXTURE_3D: u32 = 0x806F;
const TEXTURE_2D_ARRAY: u32 = 0x8C1A;
const TEXTURE_CUBE_MAP: u32 = 0x8513;
const TEXTURE_CUBE_MAP_POSITIVE_X: u32 = 0x8515;
const CUBE_FACES: u32 = 6;
const STRIP_BYTES: usize = 4 * 1024 * 1024;
const PENDING_ERRORS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sample {
    Normal,
    Signed,
    Unsigned,
    Depth,
    Stencil,
    DepthStencil,
}

struct Sized {
    internal: u32,
    format: u32,
    kind: u32,
    sample: Sample,
}

const fn sized(internal: u32, format: u32, kind: u32, sample: Sample) -> Sized {
    Sized {
        internal,
        format,
        kind,
        sample,
    }
}

const SIZED_FORMATS: &[Sized] = &[
    sized(0x8229, RED, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8F94, RED, BYTE, Sample::Normal),
    sized(0x822D, RED, HALF_FLOAT, Sample::Normal),
    sized(0x822E, RED, FLOAT, Sample::Normal),
    sized(0x8232, RED_INTEGER, UNSIGNED_BYTE, Sample::Unsigned),
    sized(0x8231, RED_INTEGER, BYTE, Sample::Signed),
    sized(0x8234, RED_INTEGER, UNSIGNED_SHORT, Sample::Unsigned),
    sized(0x8233, RED_INTEGER, SHORT, Sample::Signed),
    sized(0x8236, RED_INTEGER, UNSIGNED_INT, Sample::Unsigned),
    sized(0x8235, RED_INTEGER, INT, Sample::Signed),
    sized(0x822B, RG, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8F95, RG, BYTE, Sample::Normal),
    sized(0x822F, RG, HALF_FLOAT, Sample::Normal),
    sized(0x8230, RG, FLOAT, Sample::Normal),
    sized(0x8238, RG_INTEGER, UNSIGNED_BYTE, Sample::Unsigned),
    sized(0x8237, RG_INTEGER, BYTE, Sample::Signed),
    sized(0x823A, RG_INTEGER, UNSIGNED_SHORT, Sample::Unsigned),
    sized(0x8239, RG_INTEGER, SHORT, Sample::Signed),
    sized(0x823C, RG_INTEGER, UNSIGNED_INT, Sample::Unsigned),
    sized(0x823B, RG_INTEGER, INT, Sample::Signed),
    sized(RGB8, RGB, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8C41, RGB, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8D62, RGB, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8F96, RGB, BYTE, Sample::Normal),
    sized(0x8C3A, RGB, HALF_FLOAT, Sample::Normal),
    sized(0x8C3D, RGB, HALF_FLOAT, Sample::Normal),
    sized(0x881B, RGB, HALF_FLOAT, Sample::Normal),
    sized(0x8815, RGB, FLOAT, Sample::Normal),
    sized(0x8D7D, RGB_INTEGER, UNSIGNED_BYTE, Sample::Unsigned),
    sized(0x8D8F, RGB_INTEGER, BYTE, Sample::Signed),
    sized(0x8D77, RGB_INTEGER, UNSIGNED_SHORT, Sample::Unsigned),
    sized(0x8D89, RGB_INTEGER, SHORT, Sample::Signed),
    sized(0x8D71, RGB_INTEGER, UNSIGNED_INT, Sample::Unsigned),
    sized(0x8D83, RGB_INTEGER, INT, Sample::Signed),
    sized(RGBA8, RGBA, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8C43, RGBA, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8F97, RGBA, BYTE, Sample::Normal),
    sized(0x8057, RGBA, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8056, RGBA, UNSIGNED_BYTE, Sample::Normal),
    sized(0x8059, RGBA, UNSIGNED_INT_2_10_10_10_REV, Sample::Normal),
    sized(0x881A, RGBA, HALF_FLOAT, Sample::Normal),
    sized(0x8814, RGBA, FLOAT, Sample::Normal),
    sized(0x8D7C, RGBA_INTEGER, UNSIGNED_BYTE, Sample::Unsigned),
    sized(0x8D8E, RGBA_INTEGER, BYTE, Sample::Signed),
    sized(
        0x906F,
        RGBA_INTEGER,
        UNSIGNED_INT_2_10_10_10_REV,
        Sample::Unsigned,
    ),
    sized(0x8D76, RGBA_INTEGER, UNSIGNED_SHORT, Sample::Unsigned),
    sized(0x8D88, RGBA_INTEGER, SHORT, Sample::Signed),
    sized(0x8D70, RGBA_INTEGER, UNSIGNED_INT, Sample::Unsigned),
    sized(0x8D82, RGBA_INTEGER, INT, Sample::Signed),
    sized(
        DEPTH_COMPONENT16,
        DEPTH_COMPONENT,
        UNSIGNED_SHORT,
        Sample::Depth,
    ),
    sized(0x81A6, DEPTH_COMPONENT, UNSIGNED_INT, Sample::Depth),
    sized(0x8CAC, DEPTH_COMPONENT, FLOAT, Sample::Depth),
    sized(
        DEPTH24_STENCIL8,
        DEPTH_STENCIL,
        UNSIGNED_INT_24_8,
        Sample::DepthStencil,
    ),
    sized(
        0x8CAD,
        DEPTH_STENCIL,
        FLOAT_32_UNSIGNED_INT_24_8_REV,
        Sample::DepthStencil,
    ),
    sized(ALPHA, ALPHA, UNSIGNED_BYTE, Sample::Normal),
    sized(LUMINANCE, LUMINANCE, UNSIGNED_BYTE, Sample::Normal),
    sized(
        LUMINANCE_ALPHA,
        LUMINANCE_ALPHA,
        UNSIGNED_BYTE,
        Sample::Normal,
    ),
    sized(RGB, RGB, UNSIGNED_BYTE, Sample::Normal),
    sized(RGBA, RGBA, UNSIGNED_BYTE, Sample::Normal),
];

fn format_of(internal: u32) -> Option<&'static Sized> {
    SIZED_FORMATS.iter().find(|f| f.internal == internal)
}

pub(crate) fn upload_format(internal: u32) -> Option<(u32, u32)> {
    format_of(internal).map(|f| (f.format, f.kind))
}

fn renderbuffer_sample(internal: u32) -> Option<Sample> {
    match internal {
        STENCIL_INDEX8 => Some(Sample::Stencil),
        DEPTH_STENCIL => Some(Sample::DepthStencil),
        _ => format_of(internal).map(|f| f.sample),
    }
}

pub(crate) struct Unpack {
    buffer: u32,
    params: [(u32, i32); 6],
    extended: bool,
}

const UNPACK_PARAMS: [(u32, i32); 6] = [
    (UNPACK_ALIGNMENT, 1),
    (UNPACK_ROW_LENGTH, 0),
    (UNPACK_SKIP_ROWS, 0),
    (UNPACK_SKIP_PIXELS, 0),
    (UNPACK_SKIP_IMAGES, 0),
    (UNPACK_IMAGE_HEIGHT, 0),
];

impl Unpack {
    pub(crate) fn neutral(version: i32) -> Unpack {
        let extended = version >= 2;
        let count = if extended { UNPACK_PARAMS.len() } else { 1 };
        let mut params = UNPACK_PARAMS;
        for (pname, neutral) in params.iter_mut().take(count) {
            let saved = gl::get_integer(*pname);
            gl::pixel_storei(*pname, *neutral);
            *neutral = saved;
        }
        let mut buffer = 0;
        if extended {
            buffer = gl::get_integer(PIXEL_UNPACK_BUFFER_BINDING) as u32;
            if buffer != 0 {
                gl::bind_buffer(PIXEL_UNPACK_BUFFER, 0);
            }
        }
        Unpack {
            buffer,
            params,
            extended,
        }
    }

    pub(crate) fn restore(self) {
        let count = if self.extended { self.params.len() } else { 1 };
        for (pname, saved) in self.params.iter().take(count) {
            gl::pixel_storei(*pname, *saved);
        }
        if self.buffer != 0 {
            gl::bind_buffer(PIXEL_UNPACK_BUFFER, self.buffer);
        }
    }
}

fn level_extent(extent: i32, level: i32) -> i32 {
    (extent >> level.min(30)).max(1)
}

fn zero_level(target: u32, level: i32, size: (i32, i32, i32), pixel: (u32, u32), volume: bool) {
    let (w, h, d) = size;
    let row = (w as usize).saturating_mul(limits::pixel_bytes(pixel.0, pixel.1));
    if row == 0 || h <= 0 || d <= 0 {
        return;
    }
    let rows = (STRIP_BYTES / row).clamp(1, h as usize);
    let zero = vec![0u8; row.saturating_mul(rows)];
    for z in 0..d {
        let mut y = 0;
        while y < h {
            let n = (rows as i32).min(h - y);
            let data = &zero[..row * n as usize];
            if volume {
                gl::tex_sub_image_3d(target, level, (0, y, z), (w, n, 1), pixel, data);
            } else {
                gl::tex_sub_image_2d(target, level, (0, y), (w, n), pixel, data);
            }
            y += n;
        }
    }
}

pub(crate) fn storage_known(internal: u32) -> bool {
    format_of(internal).is_some()
}

fn hold_error(st: &mut State, error: u32) {
    if error != NO_ERROR && st.injected_error == NO_ERROR {
        st.injected_error = error;
    }
}

pub(crate) fn succeeded(st: &mut State, call: impl FnOnce()) -> bool {
    for _ in 0..PENDING_ERRORS {
        let pending = gl::get_error();
        if pending == NO_ERROR {
            break;
        }
        hold_error(st, pending);
    }
    call();
    let error = gl::get_error();
    hold_error(st, error);
    error == NO_ERROR
}

pub(crate) fn zero_storage(
    version: i32,
    target: u32,
    levels: i32,
    internal: u32,
    size: (i32, i32, i32),
) {
    let Some(pixel) = upload_format(internal) else {
        return;
    };
    let unpack = Unpack::neutral(version);
    let volume = matches!(target, TEXTURE_3D | TEXTURE_2D_ARRAY);
    for level in 0..levels.clamp(0, 31) {
        let w = level_extent(size.0, level);
        let h = level_extent(size.1, level);
        let d = match target {
            TEXTURE_3D => level_extent(size.2, level),
            TEXTURE_2D_ARRAY => size.2,
            _ => 1,
        };
        if target == TEXTURE_CUBE_MAP {
            for face in 0..CUBE_FACES {
                let face = TEXTURE_CUBE_MAP_POSITIVE_X + face;
                zero_level(face, level, (w, h, 1), pixel, false);
            }
        } else {
            zero_level(target, level, (w, h, d), pixel, volume);
        }
    }
    unpack.restore();
}

pub(crate) fn zero_pixels(version: i32, size: (i32, i32), pixel: (u32, u32)) -> Option<Vec<u8>> {
    let (w, h) = size;
    if w <= 0 || h <= 0 {
        return Some(Vec::new());
    }
    let bytes = (w as usize)
        .checked_mul(h as usize)?
        .checked_mul(limits::pixel_bytes(pixel.0, pixel.1))?;
    let need = limits::transfer_bytes(version, (w, h, 1), pixel.0, pixel.1, false).max(bytes);
    (need <= limits::MAX_ALLOC).then(|| vec![0u8; need])
}

struct ClearState {
    scissor: bool,
    discard: bool,
    color_mask: [i32; 4],
    depth_mask: i32,
    stencil_front: i32,
    stencil_back: i32,
    color: [f32; 4],
    depth: [f32; 4],
    stencil: i32,
}

impl ClearState {
    fn open(version: i32) -> ClearState {
        let mut saved = ClearState {
            scissor: gl::is_enabled(SCISSOR_TEST) != 0,
            discard: version >= 2 && gl::is_enabled(RASTERIZER_DISCARD) != 0,
            color_mask: [0; 4],
            depth_mask: gl::get_integer(DEPTH_WRITEMASK),
            stencil_front: gl::get_integer(STENCIL_WRITEMASK),
            stencil_back: gl::get_integer(STENCIL_BACK_WRITEMASK),
            color: [0.0; 4],
            depth: [0.0; 4],
            stencil: gl::get_integer(STENCIL_CLEAR_VALUE),
        };
        gl::get_integers(COLOR_WRITEMASK, &mut saved.color_mask);
        gl::get_floats(COLOR_CLEAR_VALUE, &mut saved.color);
        gl::get_floats(DEPTH_CLEAR_VALUE, &mut saved.depth);
        if saved.scissor {
            gl::disable(SCISSOR_TEST);
        }
        if saved.discard {
            gl::disable(RASTERIZER_DISCARD);
        }
        gl::color_mask(1, 1, 1, 1);
        gl::depth_mask(1);
        gl::stencil_mask(u32::MAX);
        gl::clear_color(0.0, 0.0, 0.0, 0.0);
        gl::clear_depthf(1.0);
        gl::clear_stencil(0);
        saved
    }

    fn close(self) {
        let [r, g, b, a] = self.color_mask.map(|on| u8::from(on != 0));
        gl::color_mask(r, g, b, a);
        gl::depth_mask(u8::from(self.depth_mask != 0));
        gl::stencil_mask_separate(FRONT, self.stencil_front as u32);
        gl::stencil_mask_separate(BACK, self.stencil_back as u32);
        gl::clear_color(self.color[0], self.color[1], self.color[2], self.color[3]);
        gl::clear_depthf(self.depth[0]);
        gl::clear_stencil(self.stencil);
        if self.scissor {
            gl::enable(SCISSOR_TEST);
        }
        if self.discard {
            gl::enable(RASTERIZER_DISCARD);
        }
    }
}

pub(crate) fn clear_bound_framebuffer(version: i32, depth: bool, stencil: bool) {
    let saved = ClearState::open(version);
    let mut mask = COLOR_BUFFER_BIT;
    if depth {
        mask |= DEPTH_BUFFER_BIT;
    }
    if stencil {
        mask |= STENCIL_BUFFER_BIT;
    }
    gl::clear(mask);
    saved.close();
}

fn clear_sample(sample: Sample) {
    match sample {
        Sample::Normal => gl::clear(COLOR_BUFFER_BIT),
        Sample::Signed => gl::clear_bufferiv(COLOR, 0, &[0; 4]),
        Sample::Unsigned => gl::clear_bufferuiv(COLOR, 0, &[0; 4]),
        Sample::Depth => gl::clear(DEPTH_BUFFER_BIT),
        Sample::Stencil => gl::clear(STENCIL_BUFFER_BIT),
        Sample::DepthStencil => gl::clear(DEPTH_BUFFER_BIT | STENCIL_BUFFER_BIT),
    }
}

fn attachment_of(sample: Sample) -> u32 {
    match sample {
        Sample::Depth => DEPTH_ATTACHMENT,
        Sample::Stencil => STENCIL_ATTACHMENT,
        Sample::DepthStencil => DEPTH_STENCIL_ATTACHMENT,
        _ => COLOR_ATTACHMENT0,
    }
}

pub(crate) fn clear_renderbuffer(st: &mut State, version: i32, internal: u32) {
    let Some(sample) = renderbuffer_sample(internal) else {
        return;
    };
    let renderbuffer = gl::get_integer(RENDERBUFFER_BINDING) as u32;
    if renderbuffer == 0 {
        return;
    }
    let (draw, read) = st.bound_framebuffers();
    let scratch = gl::gen_framebuffer();
    st.bind_framebuffer(FRAMEBUFFER, scratch);
    gl::framebuffer_renderbuffer(
        FRAMEBUFFER,
        attachment_of(sample),
        RENDERBUFFER,
        renderbuffer,
    );
    if gl::check_framebuffer_status(FRAMEBUFFER) == FRAMEBUFFER_COMPLETE {
        let saved = ClearState::open(version);
        clear_sample(sample);
        saved.close();
    }
    if draw == read {
        st.bind_framebuffer(FRAMEBUFFER, draw);
    } else {
        st.bind_framebuffer(DRAW_FRAMEBUFFER, draw);
        st.bind_framebuffer(READ_FRAMEBUFFER, read);
    }
    gl::delete_framebuffer(scratch);
}
