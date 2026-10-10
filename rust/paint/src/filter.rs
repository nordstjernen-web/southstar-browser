//! Southstar — CSS filter functions on bitmaps: the colour filters and blur applied to image and layer pixels, and drop-shadow() rendered from an image's alpha.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_layout::BoxRef;
use southstar_style::{PropId as P, parse_color};

use crate::blur::box_blur_argb;
use crate::ffi::cairo::{self, Context, Cr, Surface, SurfaceRef};
use crate::ffi::engine;
use crate::util::{Rgba, is_space, rgba_of, skip_spaces, style_of};

fn strtod(text: &[u8], pos: usize) -> Option<(f64, usize)> {
    let (v, n) = southstar_glib::ascii_strtod_prefix(&text[pos..]);
    (n > 0).then_some((v, pos + n))
}

fn parse_filter_amount(text: &[u8], pos: usize) -> f64 {
    let mut p = pos;
    while p < text.len() && (text[p] == b' ' || text[p] == b'\t') {
        p += 1;
    }
    let Some((mut v, end)) = strtod(text, p) else {
        return -1.0;
    };
    if text.get(end) == Some(&b'%') {
        v /= 100.0;
    }
    v
}

struct FilterFn<'a> {
    name: &'a [u8],
    body: usize,
    body_end: usize,
    next: usize,
}

fn filter_function_next(text: &[u8], pos: usize) -> Option<FilterFn<'_>> {
    let mut p = skip_spaces(text, pos);
    if p >= text.len() {
        return None;
    }
    let name_start = p;
    while p < text.len() && (text[p].is_ascii_alphabetic() || text[p] == b'-') {
        p += 1;
    }
    let name = &text[name_start..p];
    p = skip_spaces(text, p);
    if name.is_empty() || text.get(p) != Some(&b'(') {
        return None;
    }
    p += 1;
    let body = p;
    let mut depth = 1;
    while p < text.len() {
        if text[p] == b'(' {
            depth += 1;
        } else if text[p] == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(FilterFn {
                    name,
                    body,
                    body_end: p,
                    next: p + 1,
                });
            }
        }
        p += 1;
    }
    None
}

fn name_is(name: &[u8], want: &[u8]) -> bool {
    name.eq_ignore_ascii_case(want)
}

pub fn filter_has_bitmap_effect(filter: Option<&CStr>) -> bool {
    let Some(filter) = filter.map(CStr::to_bytes) else {
        return false;
    };
    let mut p = 0;
    while p < filter.len() {
        let Some(f) = filter_function_next(filter, p) else {
            break;
        };
        if [
            &b"grayscale"[..],
            b"sepia",
            b"invert",
            b"brightness",
            b"contrast",
            b"saturate",
            b"blur",
        ]
        .iter()
        .any(|w| name_is(f.name, w))
        {
            return true;
        }
        p = f.next;
    }
    false
}

fn parse_filter_length_px(token: &[u8]) -> Option<f64> {
    let s = skip_spaces(token, 0);
    let (v, end) = strtod(token, s)?;
    let unit = &token[skip_spaces(token, end)..];
    let is = |w: &[u8]| unit.eq_ignore_ascii_case(w);
    let (vw, vh) = (engine::viewport_w(), engine::viewport_h());
    if unit.is_empty() || is(b"px") {
        Some(v)
    } else if is(b"em") || is(b"rem") || is(b"lh") || is(b"rlh") {
        Some(v * 16.0)
    } else if is(b"pt") {
        Some(v * (96.0 / 72.0))
    } else if is(b"pc") {
        Some(v * 16.0)
    } else if is(b"cm") {
        Some(v * (96.0 / 2.54))
    } else if is(b"mm") {
        Some(v * (96.0 / 25.4))
    } else if is(b"q") {
        Some(v * (96.0 / 101.6))
    } else if is(b"in") {
        Some(v * 96.0)
    } else if is(b"vw") || is(b"dvw") || is(b"svw") || is(b"lvw") {
        Some(v * vw / 100.0)
    } else if is(b"vh") || is(b"dvh") || is(b"svh") || is(b"lvh") {
        Some(v * vh / 100.0)
    } else if is(b"vmin") || is(b"dvmin") || is(b"svmin") || is(b"lvmin") {
        Some(v * crate::util::cmin(vw, vh) / 100.0)
    } else if is(b"vmax") || is(b"dvmax") || is(b"svmax") || is(b"lvmax") {
        Some(v * crate::util::cmax(vw, vh) / 100.0)
    } else {
        None
    }
}

fn filter_split_ws(text: &[u8], max: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut tok: Option<usize> = None;
    for p in 0..=text.len() {
        let done = p == text.len();
        let c = if done { 0 } else { text[p] };
        if !done && tok.is_none() && !is_space(c) {
            tok = Some(p);
        }
        if !done && c == b'(' {
            depth += 1;
        } else if !done && c == b')' && depth > 0 {
            depth -= 1;
        }
        if (done || (is_space(c) && depth == 0))
            && let Some(start) = tok.take()
            && out.len() < max
        {
            out.push(&text[start..p]);
        }
    }
    out
}

fn strip(token: &[u8]) -> &[u8] {
    let start = token
        .iter()
        .position(|&c| !is_space(c))
        .unwrap_or(token.len());
    let end = token
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |e| e + 1);
    &token[start..end.max(start)]
}

#[derive(Clone, Copy)]
struct DropShadow {
    x: f64,
    y: f64,
    blur: f64,
    color: Rgba,
}

fn parse_filter_shadow_body(body: &[u8], current: Rgba) -> Option<DropShadow> {
    let mut lengths = Vec::new();
    let mut color = current;
    let mut ok = true;
    for token in filter_split_ws(body, 8) {
        let token = strip(token);
        if let Some(len) = parse_filter_length_px(token) {
            if lengths.len() < 3 {
                lengths.push(len);
            } else {
                ok = false;
            }
        } else if token.eq_ignore_ascii_case(b"currentcolor") {
            color = current;
        } else if let Some(c) = CString::new(token).ok().and_then(|t| parse_color(&t)) {
            color = Rgba::from_bytes(c);
        } else {
            ok = false;
        }
    }
    if !ok || lengths.len() < 2 {
        return None;
    }
    Some(DropShadow {
        x: lengths[0],
        y: lengths[1],
        blur: if lengths.len() >= 3 && lengths[2] > 0.0 {
            lengths[2]
        } else {
            0.0
        },
        color,
    })
}

fn parse_filter_drop_shadows(filter: Option<&CStr>, current: Rgba, max: usize) -> Vec<DropShadow> {
    let mut out = Vec::new();
    let Some(filter) = filter.map(CStr::to_bytes) else {
        return out;
    };
    let mut p = 0;
    while p < filter.len() && out.len() < max {
        let Some(f) = filter_function_next(filter, p) else {
            break;
        };
        if name_is(f.name, b"drop-shadow")
            && let Some(s) = parse_filter_shadow_body(&filter[f.body..f.body_end], current)
        {
            out.push(s);
        }
        p = f.next;
    }
    out
}

struct ShadowPlacement {
    cw: i32,
    ch: i32,
    ox: f64,
    oy: f64,
    sx: f64,
    sy: f64,
}

fn drop_shadow_surface(
    src: SurfaceRef,
    at: &ShadowPlacement,
    shadow: DropShadow,
) -> Option<(Surface, i32)> {
    if at.cw <= 0 || at.ch <= 0 || at.sx <= 0.0 || at.sy <= 0.0 {
        return None;
    }
    let cw = at.cw.min(4096);
    let ch = at.ch.min(4096);
    let radius = if shadow.blur > 0.0 {
        ((shadow.blur + 0.5) as i32).min(512)
    } else {
        0
    };
    let pad = radius * 2 + 2;
    let sw = cw + pad * 2;
    let sh = ch + pad * 2;
    let shadow_surf = Surface::image(cairo::FORMAT_ARGB32, sw, sh)?;
    {
        let ctx = Context::new(shadow_surf.as_ref());
        let s_cr = ctx.cr();
        s_cr.set_operator(cairo::OPERATOR_CLEAR);
        s_cr.paint();
        s_cr.set_operator(cairo::OPERATOR_OVER);
        s_cr.rectangle(f64::from(pad), f64::from(pad), f64::from(cw), f64::from(ch));
        s_cr.clip();
        s_cr.translate(f64::from(pad) + at.ox, f64::from(pad) + at.oy);
        s_cr.scale(at.sx, at.sy);
        s_cr.set_source_surface(src, 0.0, 0.0);
        s_cr.paint();
    }
    let mut s = shadow_surf.as_ref();
    s.flush();
    let stride = s.stride();
    let data = s.data_mut();
    let c = shadow.color;
    for y in 0..sh as usize {
        let row = &mut data[y * stride as usize..];
        for x in 0..sw as usize {
            let px = &mut row[x * 4..x * 4 + 4];
            let a = f64::from(px[3]) / 255.0 * c.a;
            px[0] = (c.b * a * 255.0 + 0.5) as u8;
            px[1] = (c.g * a * 255.0 + 0.5) as u8;
            px[2] = (c.r * a * 255.0 + 0.5) as u8;
            px[3] = (a * 255.0 + 0.5) as u8;
        }
    }
    if radius > 0 {
        box_blur_argb(data, stride, sw, sh, radius);
    }
    s.mark_dirty();
    Some((shadow_surf, pad))
}

pub struct TextureFit {
    pub sx: f64,
    pub sy: f64,
    pub ox: f64,
    pub oy: f64,
}

pub fn paint_texture_drop_shadows(
    cr: Cr,
    surf: SurfaceRef,
    b: BoxRef<'_>,
    fit: &TextureFit,
    filter: Option<&CStr>,
) {
    let st = style_of(b);
    let current = rgba_of(
        st.and_then(|s| s.get(P::Color)),
        Rgba::new(0.0, 0.0, 0.0, 1.0),
    );
    let shadows = parse_filter_drop_shadows(filter, current, 4);
    if shadows.is_empty() {
        return;
    }
    let at = ShadowPlacement {
        cw: (b.content_width().ceil() as i32).max(1),
        ch: (b.content_height().ceil() as i32).max(1),
        ox: fit.ox,
        oy: fit.oy,
        sx: fit.sx,
        sy: fit.sy,
    };
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    for shadow in shadows {
        let Some((surface, pad)) = drop_shadow_surface(surf, &at, shadow) else {
            continue;
        };
        let padf = f64::from(pad);
        cr.save();
        cr.set_source_surface(
            surface.as_ref(),
            b.x() + m.left + bd.left + p.left + shadow.x - padf,
            b.y() + m.top + bd.top + p.top + shadow.y - padf,
        );
        cr.paint();
        cr.restore();
    }
}

#[derive(Clone, Copy)]
enum Op {
    Grayscale,
    Sepia,
    Invert,
    Brightness,
    Contrast,
    Saturate,
}

pub fn apply_image_filter(data: &mut [u8], stride: i32, w: i32, h: i32, filter: Option<&CStr>) {
    let Some(filter) = filter.map(CStr::to_bytes).filter(|f| !f.is_empty()) else {
        return;
    };
    let mut ops: Vec<(Op, f64)> = Vec::new();
    let mut blur_radius = 0.0;
    let mut q = 0;
    while q < filter.len() && ops.len() < 16 {
        let Some(f) = filter_function_next(filter, q) else {
            break;
        };
        let amt = parse_filter_amount(filter, f.body);
        q = f.next;
        let op = if name_is(f.name, b"grayscale") {
            Some(Op::Grayscale)
        } else if name_is(f.name, b"sepia") {
            Some(Op::Sepia)
        } else if name_is(f.name, b"invert") {
            Some(Op::Invert)
        } else if name_is(f.name, b"brightness") {
            Some(Op::Brightness)
        } else if name_is(f.name, b"contrast") {
            Some(Op::Contrast)
        } else if name_is(f.name, b"saturate") {
            Some(Op::Saturate)
        } else {
            if name_is(f.name, b"blur") && amt >= 0.0 && amt > blur_radius {
                blur_radius = amt;
            }
            None
        };
        if let Some(op) = op
            && amt >= 0.0
        {
            ops.push((op, amt));
        }
    }
    if ops.is_empty() && blur_radius <= 0.0 {
        return;
    }
    let stride_u = stride as usize;
    for y in 0..h as usize {
        let row = &mut data[y * stride_u..];
        for x in 0..w as usize {
            let px = &mut row[x * 4..x * 4 + 4];
            let a = f64::from(px[3]) / 255.0;
            let mut b = f64::from(px[0]) / 255.0;
            let mut g = f64::from(px[1]) / 255.0;
            let mut r = f64::from(px[2]) / 255.0;
            if a > 0.0001 {
                r /= a;
                g /= a;
                b /= a;
            }
            for &(op, t) in &ops {
                match op {
                    Op::Grayscale => {
                        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                        r = r * (1.0 - t) + lum * t;
                        g = g * (1.0 - t) + lum * t;
                        b = b * (1.0 - t) + lum * t;
                    }
                    Op::Sepia => {
                        let sr = r * 0.393 + g * 0.769 + b * 0.189;
                        let sg = r * 0.349 + g * 0.686 + b * 0.168;
                        let sb = r * 0.272 + g * 0.534 + b * 0.131;
                        r = r * (1.0 - t) + sr * t;
                        g = g * (1.0 - t) + sg * t;
                        b = b * (1.0 - t) + sb * t;
                    }
                    Op::Invert => {
                        r = r * (1.0 - t) + (1.0 - r) * t;
                        g = g * (1.0 - t) + (1.0 - g) * t;
                        b = b * (1.0 - t) + (1.0 - b) * t;
                    }
                    Op::Brightness => {
                        r *= t;
                        g *= t;
                        b *= t;
                    }
                    Op::Contrast => {
                        r = (r - 0.5) * t + 0.5;
                        g = (g - 0.5) * t + 0.5;
                        b = (b - 0.5) * t + 0.5;
                    }
                    Op::Saturate => {
                        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                        r = lum + (r - lum) * t;
                        g = lum + (g - lum) * t;
                        b = lum + (b - lum) * t;
                    }
                }
            }
            r = r.clamp(0.0, 1.0);
            g = g.clamp(0.0, 1.0);
            b = b.clamp(0.0, 1.0);
            px[0] = (b * a * 255.0 + 0.5) as u8;
            px[1] = (g * a * 255.0 + 0.5) as u8;
            px[2] = (r * a * 255.0 + 0.5) as u8;
        }
    }
    if blur_radius > 0.0 {
        let r = ((blur_radius + 0.5) as i32).min(512);
        box_blur_argb(data, stride, w, h, r);
    }
}
