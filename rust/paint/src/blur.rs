//! Southstar — box blurs over ARGB and alpha-only pixels, and blurred box shadows cut from a cache of blurred rounded rectangles stretched along their uniform middle bands.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use std::collections::HashMap;

use crate::ffi::cairo::{self, Context, Cr, Surface};
use crate::radii::{CornerRadii, rounded_rect_path};
use crate::util::cmax;

const CACHE_BYTES: usize = 32 << 20;
const ENTRY_BYTES: usize = 4 << 20;

fn clamp_i(v: i32, lo: i32, hi: i32) -> i32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

pub fn box_blur_argb(data: &mut [u8], stride: i32, w: i32, h: i32, radius: i32) {
    if radius < 1 || w < 1 || h < 1 {
        return;
    }
    let win = radius * 2 + 1;
    let stride_u = stride as usize;
    let mut tmp = vec![0u8; stride_u * h as usize];
    for y in 0..h as usize {
        let s = &data[y * stride_u..];
        let d = &mut tmp[y * stride_u..];
        for c in 0..4usize {
            let px = |x: i32| i32::from(s[clamp_i(x, 0, w - 1) as usize * 4 + c]);
            let mut sum = 0;
            for i in -radius..=radius {
                sum += px(i);
            }
            for x in 0..w {
                d[x as usize * 4 + c] = (sum / win) as u8;
                sum += px(x + radius + 1) - px(x - radius);
            }
        }
    }
    for x in 0..w as usize {
        for c in 0..4usize {
            let px = |y: i32| i32::from(tmp[clamp_i(y, 0, h - 1) as usize * stride_u + x * 4 + c]);
            let mut sum = 0;
            for i in -radius..=radius {
                sum += px(i);
            }
            for y in 0..h {
                data[y as usize * stride_u + x * 4 + c] = (sum / win) as u8;
                sum += px(y + radius + 1) - px(y - radius);
            }
        }
    }
}

pub fn box_blur_a8(data: &mut [u8], w: i32, h: i32, stride: i32, radius: i32) {
    if radius < 1 || w <= 0 || h <= 0 {
        return;
    }
    let win = 2 * radius + 1;
    let stride_u = stride as usize;
    let mut tmp = vec![0u8; stride_u * h as usize];
    for y in 0..h as usize {
        let row = &data[y * stride_u..];
        let out = &mut tmp[y * stride_u..];
        let px = |x: i32| i32::from(row[clamp_i(x, 0, w - 1) as usize]);
        let mut sum = 0;
        for k in -radius..=radius {
            sum += px(k);
        }
        for x in 0..w {
            out[x as usize] = (sum / win) as u8;
            sum += px(x + radius + 1) - px(x - radius);
        }
    }
    for x in 0..w as usize {
        let px = |y: i32| i32::from(tmp[clamp_i(y, 0, h - 1) as usize * stride_u + x]);
        let mut sum = 0;
        for k in -radius..=radius {
            sum += px(k);
        }
        for y in 0..h {
            data[y as usize * stride_u + x] = (sum / win) as u8;
            sum += px(y + radius + 1) - px(y - radius);
        }
    }
}

#[derive(Clone, Copy)]
struct ShadowKey {
    sw: f64,
    sh: f64,
    radii: CornerRadii,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    radius: i32,
}

impl ShadowKey {
    fn bits(&self) -> [u64; 15] {
        let c = &self.radii;
        [
            self.sw.to_bits(),
            self.sh.to_bits(),
            c.tl.to_bits(),
            c.tr.to_bits(),
            c.br.to_bits(),
            c.bl.to_bits(),
            c.tlv.to_bits(),
            c.trv.to_bits(),
            c.brv.to_bits(),
            c.blv.to_bits(),
            self.r.to_bits(),
            self.g.to_bits(),
            self.b.to_bits(),
            self.a.to_bits(),
            self.radius as u64,
        ]
    }
}

#[derive(Default)]
struct ShadowCache {
    map: HashMap<[u64; 15], Surface>,
    bytes: usize,
}

impl ShadowCache {
    fn get(&self, k: &ShadowKey) -> Option<Surface> {
        self.map.get(&k.bits()).cloned()
    }

    fn insert(&mut self, k: &ShadowKey, surf: &Surface, bytes: usize) {
        if self.bytes + bytes > CACHE_BYTES {
            self.map.clear();
            self.bytes = 0;
        }
        self.map.insert(k.bits(), surf.clone());
        self.bytes += bytes;
    }
}

thread_local! {
    static SHADOW_CACHE: RefCell<ShadowCache> = RefCell::new(ShadowCache::default());
}

fn surface_bytes(s: &Surface, height: i32) -> usize {
    s.as_ref().stride().max(0) as usize * height.max(0) as usize
}

fn blurred_shadow_surface_new(
    k: &ShadowKey,
    pad: i32,
    surf_w: i32,
    surf_h: i32,
) -> Option<Surface> {
    let surf = Surface::image(cairo::FORMAT_ARGB32, surf_w, surf_h)?;
    {
        let ctx = Context::new(surf.as_ref());
        let scr = ctx.cr();
        rounded_rect_path(scr, f64::from(pad), f64::from(pad), k.sw, k.sh, k.radii);
        scr.set_source_rgba(k.r, k.g, k.b, k.a);
        scr.fill();
    }
    let mut s = surf.as_ref();
    s.flush();
    let stride = s.stride();
    let data = s.data_mut();
    box_blur_argb(data, stride, surf_w, surf_h, k.radius);
    box_blur_argb(data, stride, surf_w, surf_h, k.radius);
    box_blur_argb(data, stride, surf_w, surf_h, k.radius);
    s.mark_dirty();
    Some(surf)
}

fn blurred_shadow_cached(
    k: &ShadowKey,
    pad: i32,
    surf_w: i32,
    surf_h: i32,
    keep_large: bool,
) -> Option<Surface> {
    if let Some(hit) = SHADOW_CACHE.with(|c| c.borrow().get(k)) {
        return Some(hit);
    }
    let surf = blurred_shadow_surface_new(k, pad, surf_w, surf_h)?;
    let bytes = surface_bytes(&surf, surf.as_ref().height());
    if bytes > ENTRY_BYTES && !keep_large {
        return Some(surf);
    }
    if bytes > CACHE_BYTES {
        return Some(surf);
    }
    SHADOW_CACHE.with(|c| c.borrow_mut().insert(k, &surf, bytes));
    Some(surf)
}

fn shadow_band_excess(len: f64, pad: i32, radius: i32, start_r: f64, end_r: f64) -> i32 {
    let first = (f64::from(pad) + start_r).ceil() as i32 + radius * 3 + 1;
    let last = (f64::from(pad) + len - end_r).floor() as i32 - radius * 3 - 1;
    let uniform = last - first;
    if uniform > 1 { uniform - 1 } else { 0 }
}

fn corner_w(w: f64, h: f64) -> f64 {
    if w > 0.0 && h > 0.0 { w } else { 0.0 }
}

fn corner_h(w: f64, h: f64) -> f64 {
    if w > 0.0 && h > 0.0 { h } else { 0.0 }
}

struct Bands {
    dx: i32,
    dy: i32,
    mid_x: i32,
    mid_y: i32,
}

fn shadow_bands(k: &ShadowKey, pad: i32) -> Option<Bands> {
    if k.sw.is_nan() || k.sw <= 0.0 || k.sh.is_nan() || k.sh <= 0.0 {
        return None;
    }
    if !k.radii.fits_unscaled(k.sw, k.sh) {
        return None;
    }
    let c = &k.radii;
    let left_r = cmax(corner_w(c.tl, c.tlv), corner_w(c.bl, c.blv));
    let right_r = cmax(corner_w(c.tr, c.trv), corner_w(c.br, c.brv));
    let top_r = cmax(corner_h(c.tl, c.tlv), corner_h(c.tr, c.trv));
    let bottom_r = cmax(corner_h(c.bl, c.blv), corner_h(c.br, c.brv));
    let bands = Bands {
        dx: shadow_band_excess(k.sw, pad, k.radius, left_r, right_r),
        dy: shadow_band_excess(k.sh, pad, k.radius, top_r, bottom_r),
        mid_x: (f64::from(pad) + left_r).ceil() as i32 + k.radius * 3 + 1,
        mid_y: (f64::from(pad) + top_r).ceil() as i32 + k.radius * 3 + 1,
    };
    (bands.dx > 0 || bands.dy > 0).then_some(bands)
}

fn shadow_expand_bands(small: &Surface, surf_w: i32, surf_h: i32, b: &Bands) -> Option<Surface> {
    let full = Surface::image(cairo::FORMAT_ARGB32, surf_w, surf_h)?;
    let mut dst_s = full.as_ref();
    dst_s.flush();
    let src_s = small.as_ref();
    let src_stride = src_s.stride() as usize;
    let dst_stride = dst_s.stride() as usize;
    let src = src_s.data();
    let dst = dst_s.data_mut();
    let head = (b.mid_x + 1) as usize * 4;
    let tail = (surf_w - b.mid_x - 1 - b.dx) as usize * 4;
    let row_bytes = surf_w as usize * 4;
    let mid_row = b.mid_y as usize * dst_stride;
    for y in 0..surf_h {
        let row = y as usize * dst_stride;
        if y > b.mid_y && y <= b.mid_y + b.dy {
            dst.copy_within(mid_row..mid_row + row_bytes, row);
            continue;
        }
        let sy = if y <= b.mid_y { y } else { y - b.dy };
        let srow = &src[sy as usize * src_stride..];
        dst[row..row + head].copy_from_slice(&srow[..head]);
        let mid = b.mid_x as usize * 4;
        for x in 0..b.dx as usize {
            let at = row + head + x * 4;
            dst[at..at + 4].copy_from_slice(&srow[mid..mid + 4]);
        }
        let at = row + head + b.dx as usize * 4;
        dst[at..at + tail].copy_from_slice(&srow[head..head + tail]);
    }
    dst_s.mark_dirty();
    Some(full)
}

fn blurred_shadow_surface(k: &ShadowKey, pad: i32, surf_w: i32, surf_h: i32) -> Option<Surface> {
    let Some(bands) = shadow_bands(k, pad) else {
        return blurred_shadow_cached(k, pad, surf_w, surf_h, false);
    };
    if let Some(hit) = SHADOW_CACHE.with(|c| c.borrow().get(k)) {
        return Some(hit);
    }
    let mut small_key = *k;
    small_key.sw -= f64::from(bands.dx);
    small_key.sh -= f64::from(bands.dy);
    let small = blurred_shadow_cached(&small_key, pad, surf_w - bands.dx, surf_h - bands.dy, true)?;
    let full = shadow_expand_bands(&small, surf_w, surf_h, &bands)?;
    drop(small);
    let bytes = surface_bytes(&full, surf_h);
    if bytes > ENTRY_BYTES {
        return Some(full);
    }
    SHADOW_CACHE.with(|c| c.borrow_mut().insert(k, &full, bytes));
    Some(full)
}

pub struct BlurredShadow {
    pub rect: (f64, f64, f64, f64),
    pub radii: CornerRadii,
    pub blur: f64,
    pub color: (f64, f64, f64, f64),
    pub clip: (f64, f64, f64, f64),
    pub clip_radii: CornerRadii,
}

pub fn paint_blurred_box_shadow(cr: Cr, s: &BlurredShadow) {
    let (sx, sy, sw, sh_h) = s.rect;
    let radius = ((s.blur * 0.5 + 0.5) as i32).clamp(1, 256);
    let pad = radius * 3 + 2;
    let isw = (sw.ceil() as i32).max(1);
    let ish = (sh_h.ceil() as i32).max(1);
    let surf_w = isw + pad * 2;
    let surf_h = ish + pad * 2;
    if surf_w > 8192 || surf_h > 8192 {
        return;
    }
    let key = ShadowKey {
        sw,
        sh: sh_h,
        radii: s.radii,
        r: s.color.0,
        g: s.color.1,
        b: s.color.2,
        a: s.color.3,
        radius,
    };
    let Some(surf) = blurred_shadow_surface(&key, pad, surf_w, surf_h) else {
        return;
    };
    let padf = f64::from(pad);
    let (cx, cy, cw, ch) = s.clip;
    cr.save();
    cr.new_path();
    cr.rectangle(sx - padf, sy - padf, f64::from(surf_w), f64::from(surf_h));
    rounded_rect_path(cr, cx, cy, cw, ch, s.clip_radii);
    cr.set_fill_rule(cairo::FILL_RULE_EVEN_ODD);
    cr.clip();
    cr.set_fill_rule(cairo::FILL_RULE_WINDING);
    cr.set_source_surface(surf.as_ref(), sx - padf, sy - padf);
    cr.paint();
    cr.restore();
}
