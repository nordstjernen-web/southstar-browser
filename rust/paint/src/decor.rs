//! Southstar — box decorations: backgrounds with their image and gradient layers, box shadows, borders square, rounded and image-sliced, outlines, the fieldset legend gap, horizontal rules and the CSS chrome of inline elements.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;

use southstar_image::ImageRef;
use southstar_layout::{BoxRef, Edges, InlineAttr};
use southstar_style::{Gradient, Kind, PropId as P, StyleRef, ValueRef, layer, layer_count};

use crate::blur::{BlurredShadow, paint_blurred_box_shadow};
use crate::ffi::cairo::{self, Context, Cr, Matrix, Pattern, Surface};
use crate::ffi::engine::{self, BorderImage, Texture};
use crate::media::texture_surface_cached;
use crate::radii::{
    CornerRadii, box_border_radii, fill_outer_shadow, rounded_rect_path, style_border_radii,
};
use crate::util::{
    Rgba, UNIT_EM, UNIT_NUMBER, UNIT_PERCENT, UNIT_REM, UNIT_VH, UNIT_VMAX, UNIT_VMIN, UNIT_VW,
    cmax, cmin, get, keyword, keyword_is, length_or, rgba_of, style_of,
};

pub const ANIM_TARGET_COLOR: i32 = 4;
pub const ANIM_TARGET_BG_COLOR: i32 = 5;

pub fn rgba_anim(
    b: Option<BoxRef<'_>>,
    which: i32,
    v: Option<ValueRef<'_>>,
    fallback: Rgba,
) -> Rgba {
    if let Some(c) = b.and_then(|b| engine::anim_color(b, which)) {
        return Rgba::from_bytes(c);
    }
    rgba_of(v, fallback)
}

pub fn bg_size_px(v: f64, unit: u32, basis: f64) -> f64 {
    match unit {
        UNIT_PERCENT => v * basis / 100.0,
        UNIT_EM | UNIT_REM => v * 16.0,
        UNIT_VW => v * engine::viewport_w() / 100.0,
        UNIT_VH => v * engine::viewport_h() / 100.0,
        UNIT_VMIN => {
            let m = cmin(engine::viewport_w(), engine::viewport_h());
            v * m / 100.0
        }
        UNIT_VMAX => {
            let m = cmax(engine::viewport_w(), engine::viewport_h());
            v * m / 100.0
        }
        _ => v,
    }
}

pub fn style_side_visible(s: Option<StyleRef<'_>>, wp: P, sp: P) -> bool {
    let Some(s) = s else {
        return false;
    };
    let w = length_or(s.get(wp), 0.0);
    if w <= 0.0 {
        return false;
    }
    let st = s.get(sp);
    st.is_some() && !keyword_is(st, c"none") && !keyword_is(st, c"hidden")
}

const SIDE_WIDTH: [P; 4] = [
    P::BorderTopWidth,
    P::BorderRightWidth,
    P::BorderBottomWidth,
    P::BorderLeftWidth,
];
const SIDE_STYLE: [P; 4] = [
    P::BorderTopStyle,
    P::BorderRightStyle,
    P::BorderBottomStyle,
    P::BorderLeftStyle,
];
const SIDE_COLOR: [P; 4] = [
    P::BorderTopColor,
    P::BorderRightColor,
    P::BorderBottomColor,
    P::BorderLeftColor,
];

pub fn style_has_inline_box_paint(s: Option<StyleRef<'_>>) -> bool {
    let Some(st) = s else {
        return false;
    };
    if st
        .get(P::BackgroundColor)
        .and_then(ValueRef::color)
        .is_some_and(|c| c[3] > 0)
    {
        return true;
    }
    if st
        .get(P::BoxShadow)
        .and_then(ValueRef::shadows)
        .is_some_and(|l| l.n > 0)
    {
        return true;
    }
    if st
        .get(P::BackgroundImage)
        .is_some_and(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
    {
        return true;
    }
    (0..4).any(|i| style_side_visible(s, SIDE_WIDTH[i], SIDE_STYLE[i]))
}

fn border_style_is_solid(v: Option<ValueRef<'_>>) -> bool {
    keyword(v).is_some_and(|k| k.to_bytes() == b"solid")
}

fn side_color(s: StyleRef<'_>, i: usize) -> Rgba {
    rgba_of(
        s.get(SIDE_COLOR[i]).or_else(|| s.get(P::Color)),
        Rgba::new(0.0, 0.0, 0.0, 1.0),
    )
}

fn border_wedge_apex(
    a0: [f64; 2],
    a1: [f64; 2],
    b0: [f64; 2],
    b1: [f64; 2],
    cx: f64,
    cy: f64,
) -> (f64, f64) {
    let dax = a1[0] - a0[0];
    let day = a1[1] - a0[1];
    let dbx = b1[0] - b0[0];
    let dby = b1[1] - b0[1];
    let den = dax * dby - day * dbx;
    if den.abs() < 1e-9 {
        return (cx, cy);
    }
    let t = ((b0[0] - a0[0]) * dby - (b0[1] - a0[1]) * dbx) / den;
    (a0[0] + t * dax, a0[1] + t * day)
}

pub fn snap_device_x(cr: Cr, x: f64) -> f64 {
    let (dx, dy) = cr.user_to_device(x, 0.0);
    cr.device_to_user(dx.round(), dy).0
}

pub fn snap_device_y(cr: Cr, y: f64) -> f64 {
    let (dx, dy) = cr.user_to_device(0.0, y);
    cr.device_to_user(dx, dy.round()).1
}

fn snap_inner_edge(cr: Cr, outer: f64, inner: f64, width: f64, inward: f64, vertical: bool) -> f64 {
    if width <= 0.0 {
        return outer;
    }
    let mut snapped = if vertical {
        snap_device_y(cr, inner)
    } else {
        snap_device_x(cr, inner)
    };
    let (dx, dy) = cr.device_to_user_distance(1.0, 1.0);
    let device_px = if vertical { dy } else { dx }.abs();
    if (snapped - outer) * inward < device_px * 0.5 {
        snapped = outer + inward * device_px;
    }
    snapped
}

struct BorderEdges {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
    il: f64,
    it: f64,
    ir: f64,
    ib: f64,
}

fn snap_border_edges(cr: Cr, e: &mut BorderEdges, border: Edges) {
    let m = cr.matrix();
    if m.xy != 0.0 || m.yx != 0.0 {
        return;
    }
    e.l = snap_device_x(cr, e.l);
    e.r = snap_device_x(cr, e.r);
    e.t = snap_device_y(cr, e.t);
    e.b = snap_device_y(cr, e.b);
    e.il = snap_inner_edge(cr, e.l, e.il, border.left, 1.0, false);
    e.ir = snap_inner_edge(cr, e.r, e.ir, border.right, -1.0, false);
    e.it = snap_inner_edge(cr, e.t, e.it, border.top, 1.0, true);
    e.ib = snap_inner_edge(cr, e.b, e.ib, border.bottom, -1.0, true);
    if e.ir < e.il {
        e.ir = e.il;
    }
    if e.ib < e.it {
        e.ib = e.it;
    }
}

fn paint_rounded_mixed_border(
    cr: Cr,
    b: BoxRef<'_>,
    s: StyleRef<'_>,
    rect: (f64, f64, f64, f64),
    radii: CornerRadii,
) -> bool {
    let (x, y, w, h) = rect;
    let bd = b.border();
    let bw = [bd.top, bd.right, bd.bottom, bd.left];
    for i in 0..4 {
        if bw[i] > 0.0 && !border_style_is_solid(s.get(SIDE_STYLE[i])) {
            return false;
        }
    }
    let outer = radii.fit(w, h);
    let ix = x + bw[3];
    let iy = y + bw[0];
    let iw = w - bw[1] - bw[3];
    let ih = h - bw[0] - bw[2];
    let inner = outer.inset(bw[0], bw[1], bw[2], bw[3]);
    let oc = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    let ic = [[ix, iy], [ix + iw, iy], [ix + iw, iy + ih], [ix, iy + ih]];
    for (i, &side_w) in bw.iter().enumerate() {
        if side_w <= 0.0 {
            continue;
        }
        let c = side_color(s, i);
        if c.a <= 0.0 {
            continue;
        }
        cr.save();
        cr.new_path();
        let a = i;
        let bidx = (i + 1) % 4;
        let (apex_x, apex_y) =
            border_wedge_apex(oc[a], ic[a], oc[bidx], ic[bidx], x + w / 2.0, y + h / 2.0);
        cr.move_to(oc[a][0], oc[a][1]);
        cr.line_to(oc[bidx][0], oc[bidx][1]);
        cr.line_to(apex_x, apex_y);
        cr.close_path();
        cr.clip();
        cr.set_fill_rule(cairo::FILL_RULE_EVEN_ODD);
        rounded_rect_path(cr, x, y, w, h, outer);
        if iw > 0.0 && ih > 0.0 {
            rounded_rect_path(cr, ix, iy, iw, ih, inner);
        }
        c.set_source(cr);
        cr.fill();
        cr.restore();
    }
    true
}

pub fn style_uniform_solid_border(s: Option<StyleRef<'_>>) -> Option<(f64, Rgba)> {
    let st = s?;
    let mut bw = 0.0;
    let mut bc = Rgba::default();
    for i in 0..4 {
        if !style_side_visible(s, SIDE_WIDTH[i], SIDE_STYLE[i]) {
            return None;
        }
        if let Some(kw) = st
            .get(SIDE_STYLE[i])
            .filter(|v| v.kind() == Kind::Keyword)
            .and_then(ValueRef::keyword_text)
            && kw.to_bytes() != b"solid"
        {
            return None;
        }
        let w = length_or(st.get(SIDE_WIDTH[i]), 0.0);
        let c = side_color(st, i);
        if i == 0 {
            bw = w;
            bc = c;
        } else {
            if (w - bw).abs() > 0.01 {
                return None;
            }
            if (c.r - bc.r).abs() > 0.001
                || (c.g - bc.g).abs() > 0.001
                || (c.b - bc.b).abs() > 0.001
                || (c.a - bc.a).abs() > 0.001
            {
                return None;
            }
        }
    }
    (bw > 0.0).then_some((bw, bc))
}

pub fn paint_inline_box_shadow(
    cr: Cr,
    s: Option<StyleRef<'_>>,
    rect: (f64, f64, f64, f64),
    radii: CornerRadii,
) {
    let Some(sl) = get(s, P::BoxShadow).and_then(ValueRef::shadows) else {
        return;
    };
    let (x, y, w, h) = rect;
    for si in (0..sl.n.max(0) as usize).rev() {
        let sh = &sl.s[si];
        if sh.inset != 0 {
            continue;
        }
        let sx = x + sh.x - sh.spread;
        let sy = y + sh.y - sh.spread;
        let sw = w + sh.spread * 2.0;
        let sh_h = h + sh.spread * 2.0;
        let blur = sh.blur as i32;
        let (r, g, b) = (
            f64::from(sh.r) / 255.0,
            f64::from(sh.g) / 255.0,
            f64::from(sh.b) / 255.0,
        );
        if blur > 0 {
            let steps = blur.clamp(1, 12);
            for i in (1..=steps).rev() {
                let t = f64::from(i) / f64::from(steps);
                let pad = sh.blur * t;
                let alpha = (f64::from(sh.a) / 255.0) * (1.0 - t) * 0.7;
                cr.set_source_rgba(r, g, b, alpha);
                fill_outer_shadow(
                    cr,
                    (sx - pad, sy - pad, sw + pad * 2.0, sh_h + pad * 2.0),
                    (x, y, w, h),
                    radii,
                );
            }
        } else {
            cr.set_source_rgba(r, g, b, f64::from(sh.a) / 255.0);
            fill_outer_shadow(cr, (sx, sy, sw, sh_h), (x, y, w, h), radii);
        }
    }
}

pub fn style_pixelated(s: Option<StyleRef<'_>>) -> bool {
    get(s, P::ImageRendering)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .is_some_and(|k| matches!(k.to_bytes(), b"pixelated" | b"crisp-edges"))
}

pub struct BgImage<'a> {
    pub repeat: Option<ValueRef<'a>>,
    pub size: Option<ValueRef<'a>>,
    pub pos_x: Option<ValueRef<'a>>,
    pub pos_y: Option<ValueRef<'a>>,
    pub pixelated: bool,
}

fn bg_offset(v: Option<ValueRef<'_>>, span: f64) -> f64 {
    let Some(v) = v else {
        return 0.0;
    };
    if let Some((len, unit)) = v.length() {
        if unit == UNIT_PERCENT {
            span * (len / 100.0)
        } else {
            len
        }
    } else if let Some((pct, px)) = v.calc() {
        span * (pct / 100.0) + px
    } else {
        0.0
    }
}

pub fn paint_bg_image_core(
    cr: Cr,
    img: Option<ImageRef<'_>>,
    p: &BgImage<'_>,
    area: (f64, f64, f64, f64),
    clip: (f64, f64, f64, f64),
    radii: CornerRadii,
) {
    let Some(img) = img else {
        return;
    };
    let Some(tex) = (unsafe { Texture::from_raw(img.texture()) }) else {
        return;
    };
    if !img.loaded() {
        return;
    }
    let iw = tex.width();
    let ih = tex.height();
    if iw <= 0 || ih <= 0 {
        return;
    }
    let (iwf, ihf) = (f64::from(iw), f64::from(ih));
    let (x, y, w, h) = area;
    let mut tile_x = true;
    let mut tile_y = true;
    match p
        .repeat
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(|k| k.to_bytes())
    {
        Some(b"no-repeat") => {
            tile_x = false;
            tile_y = false;
        }
        Some(b"repeat-x") => tile_y = false,
        Some(b"repeat-y") => tile_x = false,
        _ => {}
    }
    let mut draw_w = iwf;
    let mut draw_h = ihf;
    if let Some(sz) = p.size {
        if let Some(kw) = sz.keyword_text().filter(|_| sz.kind() == Kind::Keyword) {
            match kw.to_bytes() {
                b"cover" => {
                    let sx = w / iwf;
                    let sy = h / ihf;
                    let sc = if sx > sy { sx } else { sy };
                    draw_w = iwf * sc;
                    draw_h = ihf * sc;
                }
                b"contain" => {
                    let sx = w / iwf;
                    let sy = h / ihf;
                    let sc = if sx < sy { sx } else { sy };
                    draw_w = iwf * sc;
                    draw_h = ihf * sc;
                }
                _ => {}
            }
        } else if let Some((v, unit)) = sz.length() {
            draw_w = bg_size_px(v, unit, w);
            draw_h = draw_w * (ihf / iwf);
        } else if let Some(s) = sz.size() {
            if !s.w_auto {
                draw_w = bg_size_px(s.w, s.w_unit, w);
            }
            if !s.h_auto {
                draw_h = bg_size_px(s.h, s.h_unit, h);
            }
            if s.w_auto && !s.h_auto {
                draw_w = draw_h * (iwf / ihf);
            } else if !s.w_auto && s.h_auto {
                draw_h = draw_w * (ihf / iwf);
            } else if s.w_auto && s.h_auto {
                draw_w = iwf;
                draw_h = ihf;
            }
        }
    }
    if draw_w < 1.0 {
        draw_w = 1.0;
    }
    if draw_h < 1.0 {
        draw_h = 1.0;
    }
    let off_x = bg_offset(p.pos_x, w - draw_w);
    let off_y = bg_offset(p.pos_y, h - draw_h);
    let Some(surf) = texture_surface_cached(tex, None) else {
        return;
    };
    let (cx, cy, cw, ch) = clip;
    cr.save();
    rounded_rect_path(cr, cx, cy, cw, ch, radii);
    cr.clip();
    let pat = Pattern::for_surface(surf);
    pat.set_extend(if tile_x || tile_y {
        cairo::EXTEND_REPEAT
    } else {
        cairo::EXTEND_NONE
    });
    if p.pixelated {
        pat.set_filter(cairo::FILTER_NEAREST);
    }
    let mut m = Matrix::identity();
    m.scale(iwf / draw_w, ihf / draw_h);
    m.translate(-(x + off_x), -(y + off_y));
    pat.set_matrix(&m);
    cr.set_source(&pat);
    if tile_x && tile_y {
        cr.paint();
    } else if !tile_x && !tile_y {
        cr.rectangle(x + off_x, y + off_y, draw_w, draw_h);
        cr.fill();
    } else if tile_x {
        cr.rectangle(x, y + off_y, w, draw_h);
        cr.fill();
    } else {
        cr.rectangle(x + off_x, y, draw_w, h);
        cr.fill();
    }
    drop(pat);
    cr.restore();
}

fn conic_color_at(gr: &Gradient, frac: f64) -> (f64, f64, f64, f64) {
    let n = gr.n_stops as usize;
    let mut pos = frac + gr.from_deg / 360.0;
    while pos < 0.0 {
        pos += 1.0;
    }
    while pos >= 1.0 {
        pos -= 1.0;
    }
    if gr.repeating != 0 {
        let cper = gr.stops[n - 1].pos;
        if cper > 0.0 {
            pos %= cper;
        }
    }
    let mut lo = 0;
    while lo + 1 < n && gr.stops[lo + 1].pos < pos {
        lo += 1;
    }
    let mut hi = lo + 1;
    if hi >= n {
        hi = n - 1;
    }
    let (sl, sh) = (&gr.stops[lo], &gr.stops[hi]);
    let mut t = 0.0;
    if sh.pos > sl.pos {
        t = (pos - sl.pos) / (sh.pos - sl.pos);
    }
    t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (f64::from(a) * (1.0 - t) + f64::from(b) * t) / 255.0;
    (
        mix(sl.r, sh.r),
        mix(sl.g, sh.g),
        mix(sl.b, sh.b),
        mix(sl.a, sh.a),
    )
}

pub fn gradient_pattern(
    gr: &Gradient,
    border: (f64, f64, f64, f64),
    cx: f64,
    cy: f64,
    period_scaled_line: bool,
) -> Pattern {
    let (bx, by, bw, bh) = border;
    let mut dxh = 0.0;
    let mut dyh = 0.0;
    let mut r_outer = 1.0;
    let mut r_outer_y = 1.0;
    let mut line_len;
    if gr.radial != 0 {
        (r_outer, r_outer_y) = engine::gradient_radii(gr, bw, bh, cx - bx, cy - by);
        line_len = r_outer;
    } else {
        let rad = engine::gradient_angle(gr, bw, bh) * PI / 180.0;
        let dx = rad.sin();
        let dy = -rad.cos();
        let half = (dx.abs() * bw + dy.abs() * bh) / 2.0;
        dxh = dx * half;
        dyh = dy * half;
        line_len = 2.0 * half;
    }
    if line_len <= 0.0 {
        line_len = 1.0;
    }
    let n = gr.n_stops.max(0) as usize;
    let frac: Vec<f64> = gr.stops[..n]
        .iter()
        .map(|st| st.pos + st.pos_px / line_len)
        .collect();
    let mut period = if gr.repeating != 0 && n > 0 {
        frac[n - 1]
    } else {
        1.0
    };
    if period <= 0.0 {
        period = 1.0;
    }
    let pat = if gr.radial != 0 {
        let pat = Pattern::radial(0.0, 0.0, 0.0, 0.0, 0.0, period);
        let mut m = Matrix::scaling(1.0 / r_outer, 1.0 / r_outer_y);
        m.translate(-cx, -cy);
        pat.set_matrix(&m);
        pat
    } else {
        let x0 = cx - dxh;
        let y0 = cy - dyh;
        if period_scaled_line {
            let x1 = cx + dxh;
            let y1 = cy + dyh;
            Pattern::linear(x0, y0, x0 + (x1 - x0) * period, y0 + (y1 - y0) * period)
        } else {
            Pattern::linear(x0, y0, x0 + 2.0 * dxh * period, y0 + 2.0 * dyh * period)
        }
    };
    for (st, f) in gr.stops[..n].iter().zip(&frac) {
        pat.add_color_stop_rgba(
            f / period,
            f64::from(st.r) / 255.0,
            f64::from(st.g) / 255.0,
            f64::from(st.b) / 255.0,
            f64::from(st.a) / 255.0,
        );
    }
    if gr.repeating != 0 {
        pat.set_extend(cairo::EXTEND_REPEAT);
    }
    pat
}

pub fn paint_bg_gradient_core(
    cr: Cr,
    gr: &Gradient,
    border: (f64, f64, f64, f64),
    clip: (f64, f64, f64, f64),
    radii: CornerRadii,
) {
    let (border_x, border_y, border_w, border_h) = border;
    let (clip_x, clip_y, clip_w, clip_h) = clip;
    let cx = border_x + gr.center_x * border_w + gr.center_x_px;
    let cy = border_y + gr.center_y * border_h + gr.center_y_px;
    if gr.conic != 0 && gr.n_stops > 0 {
        const BASE: usize = 24;
        const CAP: usize = 96;
        let mut bnd: Vec<f64> = Vec::with_capacity(CAP);
        for k in 0..=BASE {
            bnd.push(k as f64 / BASE as f64);
        }
        let off = gr.from_deg / 360.0;
        for s in 0..gr.n_stops as usize {
            if bnd.len() >= CAP {
                break;
            }
            let mut rel_pos = gr.stops[s].pos - off;
            rel_pos -= rel_pos.floor();
            bnd.push(rel_pos);
            if bnd.len() < CAP {
                bnd.push(rel_pos);
            }
        }
        for i in 1..bnd.len() {
            let key = bnd[i];
            let mut j = i as isize - 1;
            while j >= 0 && bnd[j as usize] > key {
                bnd[(j + 1) as usize] = bnd[j as usize];
                j -= 1;
            }
            bnd[(j + 1) as usize] = key;
        }
        let r_outer = (border_w * border_w + border_h * border_h).sqrt() / (PI / BASE as f64).cos();
        cr.save();
        rounded_rect_path(cr, clip_x, clip_y, clip_w, clip_h, radii);
        cr.clip();
        let mesh = Pattern::mesh();
        for i in 0..bnd.len().saturating_sub(1) {
            let f1 = bnd[i];
            let f2 = bnd[i + 1];
            let span = f2 - f1;
            if span < 1e-6 {
                continue;
            }
            let eps = span * 1e-3;
            let a1 = f1 * 2.0 * PI - PI / 2.0;
            let a2 = f2 * 2.0 * PI - PI / 2.0;
            let (r1, g1, b1, al1) = conic_color_at(gr, f1 + eps);
            let (r2, g2, b2, al2) = conic_color_at(gr, f2 - eps);
            mesh.mesh_begin_patch();
            mesh.mesh_move_to(cx, cy);
            mesh.mesh_line_to(cx + r_outer * a1.cos(), cy + r_outer * a1.sin());
            mesh.mesh_line_to(cx + r_outer * a2.cos(), cy + r_outer * a2.sin());
            mesh.mesh_line_to(cx, cy);
            mesh.mesh_corner_rgba(0, r1, g1, b1, al1);
            mesh.mesh_corner_rgba(1, r1, g1, b1, al1);
            mesh.mesh_corner_rgba(2, r2, g2, b2, al2);
            mesh.mesh_corner_rgba(3, r2, g2, b2, al2);
            mesh.mesh_end_patch();
        }
        cr.set_source(&mesh);
        cr.paint();
        drop(mesh);
        cr.restore();
    } else {
        let pat = gradient_pattern(gr, border, cx, cy, true);
        cr.save();
        rounded_rect_path(cr, clip_x, clip_y, clip_w, clip_h, radii);
        cr.clip();
        cr.set_source(&pat);
        cr.paint();
        drop(pat);
        cr.restore();
    }
}

fn inline_bg_image(
    cr: Cr,
    r: &InlineAttr,
    s: Option<StyleRef<'_>>,
    rect: (f64, f64, f64, f64),
    radii: CornerRadii,
) {
    let Some(img) = (unsafe { ImageRef::from_ptr(r.bg_image()) }) else {
        return;
    };
    let p = BgImage {
        repeat: get(s, P::BackgroundRepeat),
        size: get(s, P::BackgroundSize),
        pos_x: get(s, P::BackgroundPositionX),
        pos_y: get(s, P::BackgroundPositionY),
        pixelated: style_pixelated(s),
    };
    paint_bg_image_core(cr, Some(img), &p, rect, rect, radii);
}

pub fn paint_inline_css_chrome(cr: Cr, r: Option<&InlineAttr>, rect: (f64, f64, f64, f64)) {
    let s = r.and_then(|r| unsafe { StyleRef::from_ptr(r.style()) });
    let (x, y, w, h) = rect;
    if !style_has_inline_box_paint(s) || w <= 0.0 || h <= 0.0 {
        return;
    }
    let Some(st) = s else {
        return;
    };
    let radii = style_border_radii(s, w, h);
    cr.save();
    paint_inline_box_shadow(cr, s, rect, radii);
    let bg = rgba_of(st.get(P::BackgroundColor), Rgba::default());
    if bg.a > 0.0 {
        bg.set_source(cr);
        rounded_rect_path(cr, x, y, w, h, radii);
        cr.fill();
    }
    if let Some(r) = r {
        inline_bg_image(cr, r, s, rect, radii);
    }
    if !radii.is_zero()
        && let Some((ubw, ucolor)) = style_uniform_solid_border(s)
    {
        ucolor.set_source(cr);
        cr.set_line_width(ubw);
        rounded_rect_path(cr, x + ubw / 2.0, y + ubw / 2.0, w - ubw, h - ubw, radii);
        cr.stroke();
        cr.restore();
        return;
    }
    let sides = [
        (x, y, x + w, y),
        (x + w, y, x + w, y + h),
        (x, y + h, x + w, y + h),
        (x, y, x, y + h),
    ];
    for (i, &(x1, y1, x2, y2)) in sides.iter().enumerate() {
        let bw = length_or(st.get(SIDE_WIDTH[i]), 0.0);
        if bw <= 0.0 || !style_side_visible(s, SIDE_WIDTH[i], SIDE_STYLE[i]) {
            continue;
        }
        side_color(st, i).set_source(cr);
        cr.set_line_width(bw);
        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        cr.stroke();
    }
    cr.restore();
}

#[derive(Clone, Copy)]
struct BorderImageRun {
    size: f64,
    start: f64,
    step: f64,
    count: i32,
}

fn border_image_stretch(dest: f64) -> BorderImageRun {
    BorderImageRun {
        size: dest,
        start: 0.0,
        step: dest,
        count: 1,
    }
}

const TILE_STRETCH: i32 = 0;
const TILE_ROUND: i32 = 2;
const TILE_SPACE: i32 = 3;

fn border_image_tiling(dest: f64, natural: f64, tile: i32) -> BorderImageRun {
    let mut r = border_image_stretch(dest);
    if dest <= 0.0 || natural <= 0.0 || tile == TILE_STRETCH {
        return r;
    }
    if tile == TILE_ROUND {
        let n = ((dest / natural + 0.5).floor() as i32).max(1);
        r.size = dest / f64::from(n);
        r.step = r.size;
        r.count = n;
        return r;
    }
    if tile == TILE_SPACE {
        let n = (dest / natural).floor() as i32;
        if n < 1 {
            r.count = 0;
            return r;
        }
        let nf = f64::from(n);
        let gap = (dest - nf * natural) / (nf + 1.0);
        r.size = natural;
        r.start = gap;
        r.step = natural + gap;
        r.count = n;
        return r;
    }
    let n = (dest / natural).ceil() as i32 + 1;
    r.size = natural;
    r.step = natural;
    r.count = n;
    r.start = (dest - f64::from(n) * natural) / 2.0;
    r
}

fn paint_border_image_part(
    cr: Cr,
    surf: crate::ffi::cairo::SurfaceRef,
    src: (f64, f64, f64, f64),
    dst: (f64, f64, f64, f64),
    rx: BorderImageRun,
    ry: BorderImageRun,
) {
    let (sx, sy, sw, sh) = src;
    let (dx, dy, dw, dh) = dst;
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return;
    }
    if rx.count <= 0 || ry.count <= 0 || rx.size <= 0.0 || ry.size <= 0.0 {
        return;
    }
    cr.save();
    cr.rectangle(dx, dy, dw, dh);
    cr.clip();
    for iy in 0..ry.count {
        for ix in 0..rx.count {
            cr.save();
            cr.translate(
                dx + rx.start + f64::from(ix) * rx.step,
                dy + ry.start + f64::from(iy) * ry.step,
            );
            cr.rectangle(0.0, 0.0, rx.size, ry.size);
            cr.clip();
            cr.scale(rx.size / sw, ry.size / sh);
            cr.set_source_surface(surf, -sx, -sy);
            cr.set_source_extend(cairo::EXTEND_PAD);
            cr.set_source_filter(cairo::FILTER_GOOD);
            cr.paint();
            cr.restore();
        }
    }
    cr.restore();
}

fn border_image_edge_px(v: f64, unit: u32, border_px: f64, pct_basis: f64, font_size: f64) -> f64 {
    if unit == UNIT_NUMBER {
        return v * border_px;
    }
    if unit == UNIT_EM {
        return v * font_size;
    }
    bg_size_px(v, unit, pct_basis)
}

fn paint_border_image(
    cr: Cr,
    b: BoxRef<'_>,
    s: StyleRef<'_>,
    border: (f64, f64, f64, f64),
) -> bool {
    let Some(src) = engine::border_image_source(s) else {
        return false;
    };
    let bi: BorderImage = engine::border_image_params(s);
    let (border_x, border_y, border_w, border_h) = border;
    let font_size = length_or(s.get(P::FontSize), 16.0);
    let bd = b.border();
    let side = [bd.top, bd.right, bd.bottom, bd.left];
    let mut outset = [0.0; 4];
    for i in 0..4 {
        outset[i] = border_image_edge_px(
            bi.outset[i],
            bi.outset_unit[i] as u32,
            side[i],
            0.0,
            font_size,
        );
    }
    let area_x = border_x - outset[3];
    let area_y = border_y - outset[0];
    let area_w = border_w + outset[1] + outset[3];
    let area_h = border_h + outset[0] + outset[2];
    if area_w <= 0.0 || area_h <= 0.0 {
        return false;
    }
    let owned: Surface;
    let iw;
    let ih;
    let surf = if src.kind() == Kind::Url {
        let Some(img) = b
            .media()
            .and_then(|m| unsafe { ImageRef::from_ptr(m.border_image()) })
        else {
            return false;
        };
        let Some(tex) = (unsafe { Texture::from_raw(img.texture()) }).filter(|_| img.loaded())
        else {
            return false;
        };
        iw = f64::from(tex.width());
        ih = f64::from(tex.height());
        if iw <= 0.0 || ih <= 0.0 {
            return false;
        }
        let Some(s) = texture_surface_cached(tex, None) else {
            return false;
        };
        s
    } else {
        let Some(gr) = src.gradient() else {
            return false;
        };
        iw = area_w.ceil();
        ih = area_h.ceil();
        owned = Surface::image_unchecked(cairo::FORMAT_ARGB32, iw as i32, ih as i32);
        {
            let ctx = Context::new(owned.as_ref());
            paint_bg_gradient_core(
                ctx.cr(),
                gr,
                (0.0, 0.0, iw, ih),
                (0.0, 0.0, iw, ih),
                CornerRadii::default(),
            );
        }
        owned.as_ref()
    };

    let mut slice = [0.0; 4];
    for (i, sl) in slice.iter_mut().enumerate() {
        let basis = if i % 2 == 0 { ih } else { iw };
        *sl = if bi.slice_percent[i] != 0 {
            bi.slice[i] / 100.0 * basis
        } else {
            bi.slice[i]
        };
        if *sl < 0.0 {
            *sl = 0.0;
        }
    }
    if slice[0] + slice[2] > ih {
        let f = ih / (slice[0] + slice[2]);
        slice[0] *= f;
        slice[2] *= f;
    }
    if slice[3] + slice[1] > iw {
        let f = iw / (slice[3] + slice[1]);
        slice[3] *= f;
        slice[1] *= f;
    }
    let mut width = [0.0; 4];
    for i in 0..4 {
        let pct_basis = if i % 2 == 0 { area_h } else { area_w };
        width[i] = if bi.width_auto[i] != 0 {
            slice[i]
        } else {
            border_image_edge_px(
                bi.width[i],
                bi.width_unit[i] as u32,
                side[i],
                pct_basis,
                font_size,
            )
        };
        if width[i] < 0.0 {
            width[i] = 0.0;
        }
    }
    let mut shrink = 1.0;
    if width[3] + width[1] > area_w {
        shrink = cmin(shrink, area_w / (width[3] + width[1]));
    }
    if width[0] + width[2] > area_h {
        shrink = cmin(shrink, area_h / (width[0] + width[2]));
    }
    if shrink < 1.0 {
        for w in &mut width {
            *w *= shrink;
        }
    }
    let mid_sw = iw - slice[3] - slice[1];
    let mid_sh = ih - slice[0] - slice[2];
    let mid_dw = area_w - width[3] - width[1];
    let mid_dh = area_h - width[0] - width[2];
    let scale_y = if slice[0] > 0.0 {
        width[0] / slice[0]
    } else if slice[2] > 0.0 {
        width[2] / slice[2]
    } else {
        1.0
    };
    let scale_x = if slice[3] > 0.0 {
        width[3] / slice[3]
    } else if slice[1] > 0.0 {
        width[1] / slice[1]
    } else {
        1.0
    };
    let st = border_image_stretch;
    let part = |src, dst, rx, ry| paint_border_image_part(cr, surf, src, dst, rx, ry);
    part(
        (0.0, 0.0, slice[3], slice[0]),
        (area_x, area_y, width[3], width[0]),
        st(width[3]),
        st(width[0]),
    );
    part(
        (iw - slice[1], 0.0, slice[1], slice[0]),
        (area_x + area_w - width[1], area_y, width[1], width[0]),
        st(width[1]),
        st(width[0]),
    );
    part(
        (0.0, ih - slice[2], slice[3], slice[2]),
        (area_x, area_y + area_h - width[2], width[3], width[2]),
        st(width[3]),
        st(width[2]),
    );
    part(
        (iw - slice[1], ih - slice[2], slice[1], slice[2]),
        (
            area_x + area_w - width[1],
            area_y + area_h - width[2],
            width[1],
            width[2],
        ),
        st(width[1]),
        st(width[2]),
    );
    part(
        (slice[3], 0.0, mid_sw, slice[0]),
        (area_x + width[3], area_y, mid_dw, width[0]),
        border_image_tiling(mid_dw, mid_sw * scale_y, bi.tile[0]),
        st(width[0]),
    );
    part(
        (slice[3], ih - slice[2], mid_sw, slice[2]),
        (
            area_x + width[3],
            area_y + area_h - width[2],
            mid_dw,
            width[2],
        ),
        border_image_tiling(mid_dw, mid_sw * scale_y, bi.tile[0]),
        st(width[2]),
    );
    part(
        (0.0, slice[0], slice[3], mid_sh),
        (area_x, area_y + width[0], width[3], mid_dh),
        st(width[3]),
        border_image_tiling(mid_dh, mid_sh * scale_x, bi.tile[1]),
    );
    part(
        (iw - slice[1], slice[0], slice[1], mid_sh),
        (
            area_x + area_w - width[1],
            area_y + width[0],
            width[1],
            mid_dh,
        ),
        st(width[1]),
        border_image_tiling(mid_dh, mid_sh * scale_x, bi.tile[1]),
    );
    if bi.fill != 0 {
        part(
            (slice[3], slice[0], mid_sw, mid_sh),
            (area_x + width[3], area_y + width[0], mid_dw, mid_dh),
            border_image_tiling(mid_dw, mid_sw * scale_x, bi.tile[0]),
            border_image_tiling(mid_dh, mid_sh * scale_y, bi.tile[1]),
        );
    }
    true
}

fn bg_layer_keyword(s: Option<StyleRef<'_>>, prop: P, li: i32) -> Option<&[u8]> {
    layer(get(s, prop), li)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(|k| k.to_bytes())
}

fn bg_layer_clip_area(b: BoxRef<'_>, li: i32, area: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let (mut x, mut y, mut w, mut h) = area;
    let (bd, p) = (b.border(), b.padding());
    match bg_layer_keyword(style_of(b), P::BackgroundClip, li) {
        Some(b"padding-box") => {
            x += bd.left;
            y += bd.top;
            w -= bd.left + bd.right;
            h -= bd.top + bd.bottom;
        }
        Some(b"content-box") => {
            x += bd.left + p.left;
            y += bd.top + p.top;
            w -= bd.left + bd.right + p.left + p.right;
            h -= bd.top + bd.bottom + p.top + p.bottom;
        }
        _ => {}
    }
    if w < 0.0 {
        w = 0.0;
    }
    if h < 0.0 {
        h = 0.0;
    }
    (x, y, w, h)
}

fn bg_layer_origin_area(
    b: BoxRef<'_>,
    li: i32,
    area: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    let s = style_of(b);
    if bg_layer_keyword(s, P::BackgroundAttachment, li) == Some(b"fixed") {
        let (have, vx, vy) = engine::viewport_origin();
        return (
            if have { vx } else { 0.0 },
            if have { vy } else { 0.0 },
            cmax(engine::viewport_w(), 1.0),
            cmax(engine::viewport_h(), 1.0),
        );
    }
    let (bx, by, bw, bh) = area;
    let (bd, p) = (b.border(), b.padding());
    let k = bg_layer_keyword(s, P::BackgroundOrigin, li);
    let mut x = bx + bd.left;
    let mut y = by + bd.top;
    let mut w = bw - bd.left - bd.right;
    let mut h = bh - bd.top - bd.bottom;
    match k {
        Some(b"border-box") => {
            x = bx;
            y = by;
            w = bw;
            h = bh;
        }
        Some(b"content-box") => {
            x += p.left;
            y += p.top;
            w -= p.left + p.right;
            h -= p.top + p.bottom;
        }
        _ => {}
    }
    if w < 1.0 {
        w = 1.0;
    }
    if h < 1.0 {
        h = 1.0;
    }
    (x, y, w, h)
}

fn layer_image(b: BoxRef<'_>, li: i32) -> Option<ImageRef<'_>> {
    let media = b.media()?;
    let raw = match media.bg_layer_images() {
        Some(images) if (li as usize) < images.len() => images[li as usize],
        _ => media.bg_image(),
    };
    unsafe { ImageRef::from_ptr(raw) }
}

pub fn paint_block(cr: Cr, b: BoxRef<'_>) {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let border_x = b.x() + m.left;
    let mut border_y = b.y() + m.top;
    let border_w = b.content_width() + p.left + p.right + bd.left + bd.right;
    let mut border_h = b.content_height() + p.top + p.bottom + bd.top + bd.bottom;
    let legend = engine::fieldset_legend_gap(b);
    if let Some((inset, ..)) = legend {
        border_y += inset;
        border_h -= inset;
    }
    if border_w <= 0.0 || border_h <= 0.0 {
        return;
    }
    let s = style_of(b);
    let radii = box_border_radii(Some(b));
    let border = (border_x, border_y, border_w, border_h);

    let bg_head = get(s, P::BackgroundImage);
    let n_bg_layers = layer_count(bg_head);
    let last_bg_layer = if n_bg_layers > 0 { n_bg_layers - 1 } else { 0 };
    let clip = bg_layer_clip_area(b, last_bg_layer, border);
    let pos = bg_layer_origin_area(b, last_bg_layer, border);

    if let Some(sl) = get(s, P::BoxShadow).and_then(ValueRef::shadows) {
        for si in (0..sl.n.max(0) as usize).rev() {
            let sh = &sl.s[si];
            if sh.inset != 0 {
                continue;
            }
            let sx = border_x + sh.x - sh.spread;
            let sy = border_y + sh.y - sh.spread;
            let sw = border_w + sh.spread * 2.0;
            let sh_h = border_h + sh.spread * 2.0;
            let color = (
                f64::from(sh.r) / 255.0,
                f64::from(sh.g) / 255.0,
                f64::from(sh.b) / 255.0,
                f64::from(sh.a) / 255.0,
            );
            cr.save();
            if sh.blur > 0.0 {
                paint_blurred_box_shadow(
                    cr,
                    &BlurredShadow {
                        rect: (sx, sy, sw, sh_h),
                        radii,
                        blur: sh.blur,
                        color,
                        clip: border,
                        clip_radii: radii,
                    },
                );
            } else {
                cr.set_source_rgba(color.0, color.1, color.2, color.3);
                fill_outer_shadow(cr, (sx, sy, sw, sh_h), border, radii);
            }
            cr.restore();
        }
    }

    let has_mask = get(s, P::MaskImage).is_some_and(|v| v.kind() == Kind::Url);
    let bg = rgba_anim(
        Some(b),
        ANIM_TARGET_BG_COLOR,
        get(s, P::BackgroundColor),
        Rgba::default(),
    );
    let (clip_x, clip_y, clip_w, clip_h) = clip;
    if bg.a > 0.0 && !has_mask {
        bg.set_source(cr);
        rounded_rect_path(cr, clip_x, clip_y, clip_w, clip_h, radii);
        cr.fill();
    }

    let mut masked_fill = false;
    let media_bg = b
        .media()
        .and_then(|m| unsafe { ImageRef::from_ptr(m.bg_image()) });
    if bg.a > 0.0
        && has_mask
        && let Some(mimg) = media_bg
    {
        let tex = unsafe { Texture::from_raw(mimg.texture()) }.filter(|_| mimg.loaded());
        let iw = tex.map_or(0, Texture::width);
        let ih = tex.map_or(0, Texture::height);
        if iw > 0 && ih > 0 && clip_w > 0.0 && clip_h > 0.0 {
            let (iwf, ihf) = (f64::from(iw), f64::from(ih));
            let sc = cmin(clip_w / iwf, clip_h / ihf);
            let draw_w = iwf * sc;
            let draw_h = ihf * sc;
            let off_x = (clip_w - draw_w) / 2.0;
            let off_y = (clip_h - draw_h) / 2.0;
            if let Some(surf) = tex.and_then(|t| texture_surface_cached(t, None)) {
                cr.save();
                rounded_rect_path(cr, clip_x, clip_y, clip_w, clip_h, radii);
                cr.clip();
                bg.set_source(cr);
                let mp = Pattern::for_surface(surf);
                let mut mm = Matrix::identity();
                mm.scale(iwf / draw_w, ihf / draw_h);
                mm.translate(-(clip_x + off_x), -(clip_y + off_y));
                mp.set_matrix(&mm);
                cr.mask(&mp);
                drop(mp);
                cr.restore();
                masked_fill = true;
            }
        }
    }

    let bg_has_url = bg_head.is_some_and(|h| h.layers().any(|l| l.kind() == Kind::Url));
    if let Some(img) = media_bg.filter(|_| !bg_has_url && !masked_fill) {
        let params = BgImage {
            repeat: get(s, P::BackgroundRepeat),
            size: get(s, P::BackgroundSize),
            pos_x: get(s, P::BackgroundPositionX),
            pos_y: get(s, P::BackgroundPositionY),
            pixelated: style_pixelated(s),
        };
        paint_bg_image_core(cr, Some(img), &params, pos, clip, radii);
    }

    for li in (0..n_bg_layers).rev() {
        let Some(lv) = layer(bg_head, li) else {
            continue;
        };
        let lclip = bg_layer_clip_area(b, li, border);
        let lpos = bg_layer_origin_area(b, li, border);
        if let Some(gr) = lv.gradient() {
            paint_bg_gradient_core(cr, gr, lpos, lclip, radii);
            continue;
        }
        if lv.kind() != Kind::Url {
            continue;
        }
        let Some(img) = layer_image(b, li) else {
            continue;
        };
        let params = BgImage {
            repeat: layer(get(s, P::BackgroundRepeat), li),
            size: layer(get(s, P::BackgroundSize), li),
            pos_x: layer(get(s, P::BackgroundPositionX), li),
            pos_y: layer(get(s, P::BackgroundPositionY), li),
            pixelated: style_pixelated(s),
        };
        paint_bg_image_core(cr, Some(img), &params, lpos, lclip, radii);
    }

    if let Some(sl) = get(s, P::BoxShadow).and_then(ValueRef::shadows) {
        for si in (0..sl.n.max(0) as usize).rev() {
            let sh = &sl.s[si];
            if sh.inset == 0 {
                continue;
            }
            cr.save();
            rounded_rect_path(cr, border_x, border_y, border_w, border_h, radii);
            cr.clip();
            cr.set_source_rgba(
                f64::from(sh.r) / 255.0,
                f64::from(sh.g) / 255.0,
                f64::from(sh.b) / 255.0,
                f64::from(sh.a) / 255.0,
            );
            cr.set_line_width(if sh.blur > 0.0 { sh.blur } else { 4.0 });
            cr.translate(sh.x, sh.y);
            rounded_rect_path(cr, border_x, border_y, border_w, border_h, radii);
            cr.stroke();
            cr.restore();
        }
    }

    let mut legend_gap = legend.is_some();
    if let Some((_, gap_x0, gap_x1, gap_y0, gap_y1)) = legend {
        cr.save();
        let (cx0, cy0, cx1, cy1) = cr.clip_extents();
        cr.set_fill_rule(cairo::FILL_RULE_EVEN_ODD);
        cr.rectangle(cx0, cy0, cx1 - cx0, cy1 - cy0);
        cr.rectangle(gap_x0, gap_y0, gap_x1 - gap_x0, gap_y1 - gap_y0);
        cr.clip();
        cr.set_fill_rule(cairo::FILL_RULE_WINDING);
    }
    if let Some(st) = s {
        let mut drew_uniform = paint_border_image(cr, b, st, border);
        if !drew_uniform
            && !radii.is_zero()
            && let Some((ubw, ucolor)) = style_uniform_solid_border(s)
        {
            ucolor.set_source(cr);
            cr.set_line_width(ubw);
            rounded_rect_path(
                cr,
                border_x + ubw / 2.0,
                border_y + ubw / 2.0,
                border_w - ubw,
                border_h - ubw,
                radii,
            );
            cr.stroke();
            drew_uniform = true;
        }
        if !drew_uniform && !radii.is_zero() {
            drew_uniform = paint_rounded_mixed_border(cr, b, st, border, radii);
        }
        let sides = [
            (
                bd.top,
                border_x,
                border_y + bd.top / 2.0,
                border_x + border_w,
                border_y + bd.top / 2.0,
            ),
            (
                bd.right,
                border_x + border_w - bd.right / 2.0,
                border_y,
                border_x + border_w - bd.right / 2.0,
                border_y + border_h,
            ),
            (
                bd.bottom,
                border_x,
                border_y + border_h - bd.bottom / 2.0,
                border_x + border_w,
                border_y + border_h - bd.bottom / 2.0,
            ),
            (
                bd.left,
                border_x + bd.left / 2.0,
                border_y,
                border_x + bd.left / 2.0,
                border_y + border_h,
            ),
        ];
        let edge_l = border_x;
        let edge_t = border_y;
        let edge_r = border_x + border_w;
        let edge_b = border_y + border_h;
        let inner_x = edge_l + bd.left;
        let inner_y = edge_t + bd.top;
        let mut e = BorderEdges {
            l: edge_l,
            t: edge_t,
            r: edge_r,
            b: edge_b,
            il: inner_x,
            it: inner_y,
            ir: cmax(inner_x, edge_r - bd.right),
            ib: cmax(inner_y, edge_b - bd.bottom),
        };
        snap_border_edges(cr, &mut e, bd);
        let outer_corner = [[e.l, e.t], [e.r, e.t], [e.r, e.b], [e.l, e.b]];
        let inner_corner = [[e.il, e.it], [e.ir, e.it], [e.ir, e.ib], [e.il, e.ib]];
        for i in 0..4 {
            if drew_uniform {
                break;
            }
            let (w, x1, y1, x2, y2) = sides[i];
            if w <= 0.0 {
                continue;
            }
            let Some(bs) = st
                .get(SIDE_STYLE[i])
                .filter(|v| v.kind() == Kind::Keyword)
                .and_then(ValueRef::keyword_text)
                .map(|k| k.to_bytes())
            else {
                continue;
            };
            if bs == b"none" || bs == b"hidden" {
                continue;
            }
            let c = side_color(st, i);
            if c.a <= 0.0 {
                continue;
            }
            c.set_source(cr);
            if bs == b"solid" {
                let next = (i + 1) % 4;
                cr.new_path();
                cr.move_to(outer_corner[i][0], outer_corner[i][1]);
                cr.line_to(outer_corner[next][0], outer_corner[next][1]);
                cr.line_to(inner_corner[next][0], inner_corner[next][1]);
                cr.line_to(inner_corner[i][0], inner_corner[i][1]);
                cr.close_path();
                cr.fill();
                continue;
            }
            cr.set_line_width(w);
            cr.save();
            if bs == b"dashed" {
                cr.set_dash(&[w * 3.0, w * 2.0]);
            } else if bs == b"dotted" {
                cr.set_dash(&[w, w]);
            }
            let (mut x1, mut y1, mut x2, mut y2) = (x1, y1, x2, y2);
            if w < 1.5 {
                if x1 == x2 {
                    x1 = x1.floor() + 0.5;
                    x2 = x1;
                }
                if y1 == y2 {
                    y1 = y1.floor() + 0.5;
                    y2 = y1;
                }
            }
            cr.move_to(x1, y1);
            cr.line_to(x2, y2);
            cr.stroke();
            cr.restore();
        }
        if legend_gap {
            cr.restore();
            legend_gap = false;
        }
        let ow = length_or(st.get(P::OutlineWidth), 0.0);
        let ostyle = st
            .get(P::OutlineStyle)
            .filter(|v| v.kind() == Kind::Keyword)
            .and_then(ValueRef::keyword_text)
            .map(|k| k.to_bytes())
            .filter(|k| *k != b"none" && *k != b"hidden");
        if let Some(ostyle) = ostyle.filter(|_| ow > 0.0) {
            let off = length_or(st.get(P::OutlineOffset), 0.0);
            let oc = rgba_of(st.get(P::OutlineColor), Rgba::new(0.0, 0.0, 0.0, 1.0));
            cr.save();
            oc.set_source(cr);
            cr.set_line_width(ow);
            if ostyle == b"dashed" {
                cr.set_dash(&[ow * 3.0, ow * 2.0]);
            } else if ostyle == b"dotted" {
                cr.set_dash(&[ow, ow]);
            }
            cr.rectangle(
                border_x - off - ow / 2.0,
                border_y - off - ow / 2.0,
                border_w + (off + ow / 2.0) * 2.0,
                border_h + (off + ow / 2.0) * 2.0,
            );
            cr.stroke();
            cr.restore();
        }
    }
    if legend_gap {
        cr.restore();
    }
}

pub fn paint_hr(cr: Cr, b: BoxRef<'_>) {
    let dom = unsafe { southstar_dom::Node::from_ptr(b.dom_ptr().cast()) };
    if dom
        .and_then(|d| d.name())
        .is_none_or(|n| n.to_bytes() != b"hr")
    {
        return;
    }
    let bd = b.border();
    if bd.top > 0.0 || bd.bottom > 0.0 || bd.left > 0.0 || bd.right > 0.0 {
        return;
    }
    let mut h = 1.0;
    let s = style_of(b);
    if let Some((hv, _)) = get(s, P::Height).and_then(ValueRef::length)
        && hv > 0.0
    {
        h = hv;
    }
    if h > 24.0 {
        h = 24.0;
    }
    let m = b.margin();
    let y = b.y() + m.top + 4.0;
    let x0 = b.x() + m.left;
    let x1 = x0 + b.content_width();
    rgba_of(get(s, P::Color), Rgba::new(0.65, 0.65, 0.65, 1.0)).set_source(cr);
    if h <= 1.5 {
        cr.set_line_width(h);
        cr.move_to(x0, y);
        cr.line_to(x1, y);
        cr.stroke();
    } else {
        cr.rectangle(x0, y, x1 - x0, h);
        cr.fill();
    }
}
