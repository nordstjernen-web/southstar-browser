//! Southstar — the byte counts, caps and bounds checks that keep WebGL calls inside their buffers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::consts::*;
use crate::ffi::gl;

pub(crate) const MAX_ALLOC: usize = 1024 * 1024 * 1024;
pub(crate) const MAX_CONTEXTS: usize = 32;
pub(crate) const MAX_SHADER: usize = 4 * 1024 * 1024;

pub(crate) fn components(format: u32) -> usize {
    match format {
        RED | RED_INTEGER | ALPHA | LUMINANCE | DEPTH_COMPONENT => 1,
        RG | RG_INTEGER | LUMINANCE_ALPHA | DEPTH_STENCIL => 2,
        RGB | RGB_INTEGER | SRGB_EXT => 3,
        _ => 4,
    }
}

fn type_bytes(kind: u32) -> usize {
    match kind {
        BYTE | UNSIGNED_BYTE => 1,
        SHORT | UNSIGNED_SHORT | HALF_FLOAT | HALF_FLOAT_OES => 2,
        _ => 4,
    }
}

fn pixel_bytes(format: u32, kind: u32) -> usize {
    match kind {
        UNSIGNED_SHORT_5_6_5 | UNSIGNED_SHORT_4_4_4_4 | UNSIGNED_SHORT_5_5_5_1 => 2,
        UNSIGNED_INT_2_10_10_10_REV
        | UNSIGNED_INT_10F_11F_11F_REV
        | UNSIGNED_INT_5_9_9_9_REV
        | UNSIGNED_INT_24_8 => 4,
        FLOAT_32_UNSIGNED_INT_24_8_REV => 8,
        _ => components(format) * type_bytes(kind),
    }
}

struct Store {
    align: usize,
    row_length: usize,
    skip_rows: usize,
    skip_pixels: usize,
    image_height: usize,
    skip_images: usize,
}

fn store(version: i32, pack: bool) -> Store {
    let read = |pname: u32| gl::get_integer(pname).max(0) as usize;
    let mut s = Store {
        align: gl::get_integer(if pack {
            PACK_ALIGNMENT
        } else {
            UNPACK_ALIGNMENT
        })
        .max(1) as usize,
        row_length: 0,
        skip_rows: 0,
        skip_pixels: 0,
        image_height: 0,
        skip_images: 0,
    };
    if version >= 2 {
        s.row_length = read(if pack {
            PACK_ROW_LENGTH
        } else {
            UNPACK_ROW_LENGTH
        });
        s.skip_rows = read(if pack {
            PACK_SKIP_ROWS
        } else {
            UNPACK_SKIP_ROWS
        });
        s.skip_pixels = read(if pack {
            PACK_SKIP_PIXELS
        } else {
            UNPACK_SKIP_PIXELS
        });
        if !pack {
            s.image_height = read(UNPACK_IMAGE_HEIGHT);
            s.skip_images = read(UNPACK_SKIP_IMAGES);
        }
    }
    s
}

pub(crate) fn transfer_bytes(
    version: i32,
    size: (i32, i32, i32),
    format: u32,
    kind: u32,
    pack: bool,
) -> usize {
    let (w, h, depth) = size;
    if w <= 0 || h <= 0 || depth <= 0 {
        return 0;
    }
    let pix = pixel_bytes(format, kind);
    let s = store(version, pack);
    transfer_span(&s, (w as usize, h as usize, depth as usize), pix).unwrap_or(usize::MAX)
}

fn transfer_span(s: &Store, size: (usize, usize, usize), pix: usize) -> Option<usize> {
    let (w, h, depth) = size;
    let mut row = pix.checked_mul(if s.row_length > 0 { s.row_length } else { w })?;
    let rem = row % s.align;
    if rem != 0 {
        row += s.align - rem;
    }
    let image_height = if s.image_height > 0 {
        s.image_height
    } else {
        h
    };
    let full_rows = s
        .skip_images
        .checked_add(depth - 1)?
        .checked_mul(image_height)?
        .checked_add(s.skip_rows)?
        .checked_add(h - 1)?;
    let last = s.skip_pixels.checked_add(w)?.checked_mul(pix)?;
    row.checked_mul(full_rows)?.checked_add(last)
}

pub(crate) fn flip_safe(
    flip_y: bool,
    size: (i32, i32),
    format: u32,
    kind: u32,
    need: usize,
    len: usize,
) -> bool {
    if !flip_y || kind != UNSIGNED_BYTE {
        return false;
    }
    let (w, h) = size;
    if w <= 0 || h <= 0 {
        return false;
    }
    let tight = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(components(format)));
    tight == Some(need) && need <= len
}

pub(crate) fn flip_rows(src: &[u8], size: (i32, i32), bpp: usize) -> Vec<u8> {
    let row = size.0 as usize * bpp;
    let h = size.1 as usize;
    let mut out = Vec::with_capacity(row * h);
    for y in (0..h).rev() {
        out.extend_from_slice(&src[y * row..(y + 1) * row]);
    }
    out
}

pub(crate) fn attr_elem_bytes(kind: u32, size: i32) -> u64 {
    if kind == INT_2_10_10_10_REV || kind == UNSIGNED_INT_2_10_10_10_REV {
        return 4;
    }
    let comp = match kind {
        BYTE | UNSIGNED_BYTE => 1,
        SHORT | UNSIGNED_SHORT | HALF_FLOAT => 2,
        FLOAT | INT | UNSIGNED_INT | FIXED => 4,
        _ => return 0,
    };
    if !(1..=4).contains(&size) {
        return 0;
    }
    comp * size as u64
}

fn attr_type_cols(kind: u32) -> i32 {
    match kind {
        FLOAT_MAT2 | FLOAT_MAT2X3 | FLOAT_MAT2X4 => 2,
        FLOAT_MAT3 | FLOAT_MAT3X2 | FLOAT_MAT3X4 => 3,
        FLOAT_MAT4 | FLOAT_MAT4X2 | FLOAT_MAT4X3 => 4,
        _ => 1,
    }
}

pub(crate) fn program_attrib_mask() -> u64 {
    let program = gl::get_integer(CURRENT_PROGRAM);
    if program <= 0 {
        return 0;
    }
    let program = program as u32;
    let count = gl::get_programiv(program, ACTIVE_ATTRIBUTES);
    if count <= 0 {
        return 0;
    }
    let mut max = gl::get_programiv(program, ACTIVE_ATTRIBUTE_MAX_LENGTH);
    if max <= 0 {
        max = 64;
    }
    let mut mask = 0u64;
    for i in 0..count {
        let active = gl::active_variable(gl::ActiveKind::Attrib, program, i as u32, max);
        if active.name.starts_with(b"gl_") {
            continue;
        }
        let loc = gl::get_attrib_location(program, &active.name);
        if loc < 0 {
            continue;
        }
        let slots = attr_type_cols(active.kind) * if active.size > 0 { active.size } else { 1 };
        for s in 0..slots {
            let l = loc + s;
            if (0..64).contains(&l) {
                mask |= 1u64 << l;
            }
        }
    }
    mask
}

pub(crate) fn index_bytes(kind: u32) -> usize {
    match kind {
        UNSIGNED_BYTE => 1,
        UNSIGNED_SHORT => 2,
        UNSIGNED_INT => 4,
        _ => 0,
    }
}

pub(crate) fn elem_max_index(
    data: &[u8],
    offset: usize,
    count: usize,
    isz: usize,
    version: i32,
) -> Option<u64> {
    if count == 0 || isz == 0 {
        return None;
    }
    let span = count.checked_mul(isz)?.checked_add(offset)?;
    if span > data.len() {
        return None;
    }
    let restart: u64 = match isz {
        1 => 0xFF,
        2 => 0xFFFF,
        _ => 0xFFFF_FFFF,
    };
    let skip_restart = version >= 2;
    let indices = &data[offset..span];
    let max = indices
        .chunks_exact(isz)
        .map(|c| match isz {
            1 => u64::from(c[0]),
            2 => u64::from(u16::from_ne_bytes([c[0], c[1]])),
            _ => u64::from(u32::from_ne_bytes([c[0], c[1], c[2], c[3]])),
        })
        .filter(|&v| !(skip_restart && v == restart))
        .max();
    Some(max.unwrap_or(0))
}

pub(crate) fn param_cap(pname: u32) -> i32 {
    match pname {
        MAX_TEXTURE_SIZE | MAX_CUBE_MAP_TEXTURE_SIZE | MAX_RENDERBUFFER_SIZE => 16384,
        MAX_VERTEX_ATTRIBS => 16,
        MAX_VERTEX_UNIFORM_VECTORS => 1024,
        MAX_VARYING_VECTORS => 30,
        MAX_FRAGMENT_UNIFORM_VECTORS => 1024,
        MAX_VERTEX_TEXTURE_IMAGE_UNITS | MAX_TEXTURE_IMAGE_UNITS => 16,
        MAX_COMBINED_TEXTURE_IMAGE_UNITS => 32,
        MAX_3D_TEXTURE_SIZE | MAX_ARRAY_TEXTURE_LAYERS => 2048,
        MAX_DRAW_BUFFERS | MAX_COLOR_ATTACHMENTS => 8,
        MAX_SAMPLES => 4,
        MAX_VERTEX_UNIFORM_COMPONENTS | MAX_FRAGMENT_UNIFORM_COMPONENTS => 4096,
        MAX_VERTEX_OUTPUT_COMPONENTS => 64,
        MAX_FRAGMENT_INPUT_COMPONENTS | MAX_VARYING_COMPONENTS => 120,
        MAX_VERTEX_UNIFORM_BLOCKS | MAX_FRAGMENT_UNIFORM_BLOCKS => 12,
        MAX_COMBINED_UNIFORM_BLOCKS | MAX_UNIFORM_BUFFER_BINDINGS => 24,
        MAX_UNIFORM_BLOCK_SIZE => 16384,
        _ => 0,
    }
}

pub(crate) fn buffer_binding_query(target: u32) -> u32 {
    match target {
        ARRAY_BUFFER => ARRAY_BUFFER_BINDING,
        ELEMENT_ARRAY_BUFFER => ELEMENT_ARRAY_BUFFER_BINDING,
        COPY_READ_BUFFER => COPY_READ_BUFFER,
        COPY_WRITE_BUFFER => COPY_WRITE_BUFFER,
        PIXEL_PACK_BUFFER => PIXEL_PACK_BUFFER_BINDING,
        PIXEL_UNPACK_BUFFER => PIXEL_UNPACK_BUFFER_BINDING,
        TRANSFORM_FEEDBACK_BUFFER => TRANSFORM_FEEDBACK_BUFFER_BINDING,
        UNIFORM_BUFFER => UNIFORM_BUFFER_BINDING,
        _ => 0,
    }
}

pub(crate) fn bound_buffer(target: u32) -> u32 {
    let query = buffer_binding_query(target);
    if query == 0 {
        return 0;
    }
    gl::get_integer(query).max(0) as u32
}
