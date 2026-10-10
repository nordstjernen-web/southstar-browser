//! Southstar — CSS 3D rendering contexts: flattening preserve-3d subtrees into depth-sorted textured quads, drawing each through perspective-correct mesh cells, and picking the box under a point through the same projection.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use std::collections::HashMap;

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef, NsBox};
use southstar_mat4::Mat4;
use southstar_style::{Kind, PropId as P, ValueRef};

use crate::decor::{ANIM_TARGET_BG_COLOR, ANIM_TARGET_COLOR, paint_block};
use crate::ffi::cairo::{self, Context, Cr, Matrix, Pattern, Surface};
use crate::ffi::engine;
use crate::filter::{apply_image_filter, filter_has_bitmap_effect};
use crate::radii::border_box_size;
use crate::state;
use crate::util::{cmax, cmin, get, length_or, style_keyword, style_of};
use crate::walk::{self, box_is_hidden, box_skips_contents, merge_sort_by, paint_walk};

#[derive(Clone, Copy)]
pub struct Quad3 {
    b: *const NsBox,
    m: Mat4,
    bx: f64,
    by: f64,
    bw: f64,
    bh: f64,
    depth: f64,
    seq: u32,
    own_only: bool,
}

struct QuadTex {
    surf: Surface,
    tw: i32,
    th: i32,
}

struct Registry {
    tex: HashMap<*const NsBox, QuadTex>,
    quads: HashMap<*const NsBox, Vec<Quad3>>,
}

impl Registry {
    fn new() -> Registry {
        Registry {
            tex: HashMap::new(),
            quads: HashMap::new(),
        }
    }
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::new());
}

pub fn box_border_rect(b: BoxRef<'_>) -> (f64, f64, f64, f64) {
    let m = b.margin();
    let (bw, bh) = border_box_size(b);
    (b.x() + m.left, b.y() + m.top, bw, bh)
}

fn box_preserve3d(b: BoxRef<'_>) -> bool {
    let s = style_of(b);
    if get(s, P::TransformStyle).is_none() {
        return false;
    }
    style_keyword(s, P::TransformStyle).is_some_and(|k| k.to_bytes() == b"preserve-3d")
}

fn box_perspective_px(b: BoxRef<'_>) -> f64 {
    match get(style_of(b), P::Perspective).and_then(ValueRef::length) {
        Some((v, _)) if v > 0.0 => v,
        _ => 0.0,
    }
}

pub fn box_establishes_3d(b: BoxRef<'_>) -> bool {
    if box_perspective_px(b) > 0.0 {
        return true;
    }
    if !box_preserve3d(b) {
        return false;
    }
    !b.parent().is_some_and(box_preserve3d)
}

fn box_has_own_decor(b: BoxRef<'_>) -> bool {
    let Some(st) = style_of(b) else {
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
        .get(P::BackgroundImage)
        .is_some_and(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
    {
        return true;
    }
    let bd = b.border();
    if bd.left > 0.0 || bd.right > 0.0 || bd.top > 0.0 || bd.bottom > 0.0 {
        return true;
    }
    let ow = st.get(P::OutlineWidth);
    let os = st
        .get(P::OutlineStyle)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text);
    ow.is_some() && os.is_some_and(|k| k.to_bytes() != b"none") && length_or(ow, 0.0) > 0.0
}

fn box_subtree_paints(b: BoxRef<'_>) -> bool {
    if box_is_hidden(b) {
        return false;
    }
    if matches!(
        b.kind(),
        BoxKind::Image | BoxKind::Video | BoxKind::Inline | BoxKind::Text
    ) {
        return true;
    }
    if box_has_own_decor(b) {
        return true;
    }
    if box_skips_contents(b) {
        return false;
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        if box_subtree_paints(child) {
            return true;
        }
        c = child.next_sibling();
    }
    false
}

pub fn box_transform_origin(b: BoxRef<'_>, rect: (f64, f64, f64, f64), prop: P) -> (f64, f64, f64) {
    let (bx, by, bw, bh) = rect;
    let origin = get(style_of(b), prop)
        .and_then(ValueRef::transform)
        .filter(|t| t.n_ops > 0);
    match origin {
        Some(t) => {
            let o = &t.ops[0];
            (
                bx + if o.a_is_percent != 0 {
                    o.a / 100.0 * bw
                } else {
                    o.a
                },
                by + if o.b_is_percent != 0 {
                    o.b / 100.0 * bh
                } else {
                    o.b
                },
                o.c,
            )
        }
        None => (bx + bw / 2.0, by + bh / 2.0, 0.0),
    }
}

fn collect_3d_quads(b: BoxRef<'_>, pm: &Mat4, quads: &mut Vec<Quad3>) {
    if box_is_hidden(b) || walk::with(|w| w.skip_box.get()) == b.as_ptr() {
        return;
    }
    let rect = box_border_rect(b);
    let (bx, by, bw, bh) = rect;
    let mut m = *pm;
    let eff = engine::effective_transform(b, style_of(b));
    if eff.n_ops > 0 {
        let (ox, oy, oz) = box_transform_origin(b, rect, P::TransformOrigin);
        let tm = engine::transform_to_mat4(&eff, bw, bh);
        m.translate(ox, oy, oz);
        m = m.multiply(&tm);
        m.translate(-ox, -oy, -oz);
    }
    let p3d = box_preserve3d(b);
    let quad = |own_only: bool, seq: usize| Quad3 {
        b: b.as_ptr(),
        m,
        bx,
        by,
        bw,
        bh,
        depth: 0.0,
        seq: seq as u32,
        own_only,
    };
    if !p3d || b.first_child().is_none() {
        if box_subtree_paints(b) {
            let q = quad(false, quads.len());
            quads.push(q);
        }
        return;
    }
    if box_has_own_decor(b) {
        let q = quad(true, quads.len());
        quads.push(q);
    }
    let mut cm = m;
    let d = box_perspective_px(b);
    if d > 0.0 {
        let (pox, poy, _) = box_transform_origin(b, rect, P::PerspectiveOrigin);
        cm.translate(pox, poy, 0.0);
        cm.perspective(d);
        cm.translate(-pox, -poy, 0.0);
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        collect_3d_quads(child, &cm, quads);
        c = child.next_sibling();
    }
}

fn quad_box<'a>(q: &Quad3) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(q.b) }
}

fn clamp_like_c(x: f64, lo: f64, hi: f64) -> f64 {
    if x > hi {
        hi
    } else if x < lo {
        lo
    } else {
        x
    }
}

fn quad_texture(q: &Qb<'_>, tw: i32, th: i32, k: f64, highlight: Option<&CStr>) -> Option<Surface> {
    let b = q.b;
    let mut cacheable = !b.dom_ptr().is_null()
        && !q.q.own_only
        && b.first_child().is_none()
        && b.kind() == BoxKind::Block
        && unsafe { Node::from_ptr(b.dom_ptr().cast()) }
            .and_then(Node::name)
            .is_none_or(|n| n.to_bytes() != b"canvas");
    if cacheable
        && (engine::anim_opacity(b).is_some()
            || engine::anim_color(b, ANIM_TARGET_COLOR).is_some()
            || engine::anim_color(b, ANIM_TARGET_BG_COLOR).is_some())
    {
        cacheable = false;
    }
    if cacheable {
        let hit = REGISTRY.with(|r| {
            r.borrow()
                .tex
                .get(&b.as_ptr())
                .filter(|e| e.tw == tw && e.th == th)
                .map(|e| e.surf.clone())
        });
        if hit.is_some() {
            return hit;
        }
    }
    let tex = Surface::image(cairo::FORMAT_ARGB32, tw, th)?;
    {
        let ctx = Context::new(tex.as_ref());
        let tcr = ctx.cr();
        tcr.scale(k, k);
        tcr.translate(-q.q.bx, -q.q.by);
        let (saved_root, saved_flush) = walk::with(|w| {
            w.no_cull.set(w.no_cull.get() + 1);
            (
                w.tex_root.replace(b.as_ptr()),
                w.flush_box.replace(b.as_ptr()),
            )
        });
        if q.q.own_only {
            paint_block(tcr, b);
        } else {
            paint_walk(tcr, b, highlight);
        }
        walk::with(|w| {
            w.no_cull.set(w.no_cull.get() - 1);
            w.tex_root.set(saved_root);
            w.flush_box.set(saved_flush);
        });
    }
    let filter_kw = get(style_of(b), P::Filter)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text);
    if filter_kw.is_some_and(|f| filter_has_bitmap_effect(Some(f))) {
        let mut s = tex.as_ref();
        s.flush();
        let stride = s.stride();
        apply_image_filter(s.data_mut(), stride, tw, th, filter_kw);
        s.mark_dirty();
    }
    if cacheable {
        REGISTRY.with(|r| {
            r.borrow_mut().tex.insert(
                b.as_ptr(),
                QuadTex {
                    surf: tex.clone(),
                    tw,
                    th,
                },
            );
        });
    }
    Some(tex)
}

struct Qb<'a> {
    q: Quad3,
    b: BoxRef<'a>,
}

fn paint_quad3(cr: Cr, quad: &Quad3, highlight: Option<&CStr>) {
    let Some(b) = quad_box(quad) else {
        return;
    };
    let q = Qb { q: *quad, b };
    let quad = &q.q;
    if quad.bw < 0.5 || quad.bh < 0.5 {
        return;
    }
    const WEPS: f64 = 0.02;
    let mut px = [0.0; 4];
    let mut py = [0.0; 4];
    let mut pws = [0.0; 4];
    let cxs = [quad.bx, quad.bx + quad.bw, quad.bx + quad.bw, quad.bx];
    let cys = [quad.by, quad.by, quad.by + quad.bh, quad.by + quad.bh];
    let mut behind = 0;
    for i in 0..4 {
        let [ox, oy, _oz, ow] = quad.m.apply(cxs[i], cys[i], 0.0);
        pws[i] = ow;
        if ow < WEPS {
            behind += 1;
            continue;
        }
        px[i] = ox / ow;
        py[i] = oy / ow;
    }
    if behind == 4 {
        return;
    }
    let clipped = behind > 0;
    let (clx0, cly0, clx1, cly1) = cr.clip_extents();
    let (minx, maxx, miny, maxy);
    if !clipped {
        let area2 = (px[1] - px[0]) * (py[3] - py[0]) - (px[3] - px[0]) * (py[1] - py[0]);
        if area2.abs() < 0.01 {
            return;
        }
        if area2 < 0.0
            && style_keyword(style_of(b), P::BackfaceVisibility)
                .is_some_and(|k| k.to_bytes() == b"hidden")
        {
            return;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (px[0], px[0], py[0], py[0]);
        for i in 1..4 {
            x0 = cmin(x0, px[i]);
            x1 = cmax(x1, px[i]);
            y0 = cmin(y0, py[i]);
            y1 = cmax(y1, py[i]);
        }
        if x1 < clx0 || x0 > clx1 || y1 < cly0 || y0 > cly1 {
            return;
        }
        (minx, maxx, miny, maxy) = (x0, x1, y0, y1);
    } else {
        (minx, maxx, miny, maxy) = (clx0, clx1, cly0, cly1);
    }

    let mut k = if clipped {
        2.0
    } else {
        cmax(
            (maxx - minx) / cmax(quad.bw, 1.0),
            (maxy - miny) / cmax(quad.bh, 1.0),
        )
    };
    k = clamp_like_c(k, 1.0, 3.0);
    let maxdim = cmax(quad.bw, quad.bh);
    if maxdim > 0.0 && maxdim * k > 4096.0 {
        k = 4096.0 / maxdim;
    }
    let mut tw = (quad.bw * k).ceil() as i32;
    let mut th = (quad.bh * k).ceil() as i32;
    if tw < 1 || th < 1 {
        return;
    }
    if tw > 4096 {
        k *= 4096.0 / f64::from(tw);
        tw = 4096;
        th = (quad.bh * k).ceil() as i32;
    }
    if th > 4096 {
        k *= 4096.0 / f64::from(th);
        th = 4096;
        tw = (quad.bw * k).ceil() as i32;
    }
    let Some(tex) = quad_texture(&q, tw, th, k, highlight) else {
        return;
    };

    let mut wmin = pws[0];
    let mut wmax = pws[0];
    for &w in &pws[1..] {
        wmin = cmin(wmin, w);
        wmax = cmax(wmax, w);
    }
    let mut n: i32 = 1;
    if clipped {
        n = 32;
    } else if (wmax - wmin) / cmax(wmin, 1e-9) > 0.02 {
        let dim = cmax(maxx - minx, maxy - miny);
        n = ((dim / 24.0) as i32).clamp(2, 32);
    }
    let np = (n + 1) as usize;
    let nf = f64::from(n);
    let mut gx = vec![0.0; np * np];
    let mut gy = vec![0.0; np * np];
    let mut gw = vec![0.0; np * np];
    for j in 0..np {
        for i in 0..np {
            let [ox, oy, _oz, ow] = quad.m.apply(
                quad.bx + quad.bw * i as f64 / nf,
                quad.by + quad.bh * j as f64 / nf,
                0.0,
            );
            gw[j * np + i] = ow;
            if ow >= WEPS {
                gx[j * np + i] = ox / ow;
                gy[j * np + i] = oy / ow;
            }
        }
    }
    let pat = Pattern::for_surface(tex.as_ref());
    pat.set_extend(cairo::EXTEND_PAD);
    pat.set_filter(cairo::FILTER_GOOD);
    let (twf, thf) = (f64::from(tw), f64::from(th));
    for j in 0..n as usize {
        for i in 0..n as usize {
            let cws = [
                gw[j * np + i],
                gw[j * np + i + 1],
                gw[(j + 1) * np + i + 1],
                gw[(j + 1) * np + i],
            ];
            let cell_behind = cws.iter().filter(|&&w| w < WEPS).count();
            if cell_behind == 4 {
                continue;
            }
            let mut sx = [0.0; 8];
            let mut sy = [0.0; 8];
            let mut ss = [0.0; 8];
            let mut st = [0.0; 8];
            let mut nv;
            if cell_behind == 0 {
                sx[0] = gx[j * np + i];
                sy[0] = gy[j * np + i];
                sx[1] = gx[j * np + i + 1];
                sy[1] = gy[j * np + i + 1];
                sx[2] = gx[(j + 1) * np + i + 1];
                sy[2] = gy[(j + 1) * np + i + 1];
                sx[3] = gx[(j + 1) * np + i];
                sy[3] = gy[(j + 1) * np + i];
                ss[..4].copy_from_slice(&[0.0, 1.0, 1.0, 0.0]);
                st[..4].copy_from_slice(&[0.0, 0.0, 1.0, 1.0]);
                nv = 4;
            } else {
                const CS: [f64; 4] = [0.0, 1.0, 1.0, 0.0];
                const CT: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
                nv = 0;
                for c2 in 0..4 {
                    let c3 = (c2 + 1) & 3;
                    let in_a = cws[c2] >= WEPS;
                    let in_b = cws[c3] >= WEPS;
                    if in_a {
                        ss[nv] = CS[c2];
                        st[nv] = CT[c2];
                        nv += 1;
                    }
                    if in_a != in_b {
                        let t = (WEPS - cws[c2]) / (cws[c3] - cws[c2]);
                        ss[nv] = CS[c2] + (CS[c3] - CS[c2]) * t;
                        st[nv] = CT[c2] + (CT[c3] - CT[c2]) * t;
                        nv += 1;
                    }
                }
                if nv < 3 {
                    continue;
                }
                for v in 0..nv {
                    let [ox, oy, _oz, ow] = quad.m.apply(
                        quad.bx + quad.bw * (i as f64 + ss[v]) / nf,
                        quad.by + quad.bh * (j as f64 + st[v]) / nf,
                        0.0,
                    );
                    if ow < WEPS * 0.5 {
                        nv = 0;
                        break;
                    }
                    sx[v] = ox / ow;
                    sy[v] = oy / ow;
                }
                if nv < 3 {
                    continue;
                }
            }
            let d_s1 = ss[1] - ss[0];
            let d_t1 = st[1] - st[0];
            let d_s2 = ss[2] - ss[0];
            let d_t2 = st[2] - st[0];
            let pdet = d_s1 * d_t2 - d_s2 * d_t1;
            if pdet.abs() < 1e-9 {
                continue;
            }
            let ma = ((sx[1] - sx[0]) * d_t2 - (sx[2] - sx[0]) * d_t1) / pdet;
            let mc = ((sx[2] - sx[0]) * d_s1 - (sx[1] - sx[0]) * d_s2) / pdet;
            let mb = ((sy[1] - sy[0]) * d_t2 - (sy[2] - sy[0]) * d_t1) / pdet;
            let md = ((sy[2] - sy[0]) * d_s1 - (sy[1] - sy[0]) * d_s2) / pdet;
            let me = sx[0] - ma * ss[0] - mc * st[0];
            let mf = sy[0] - mb * ss[0] - md * st[0];
            let mut ccx = 0.0;
            let mut ccy = 0.0;
            for v in 0..nv {
                ccx += sx[v];
                ccy += sy[v];
            }
            ccx /= nv as f64;
            ccy /= nv as f64;
            let pad = if n > 1 { 0.35 } else { 0.0 };
            cr.save();
            cr.new_path();
            for v in 0..nv {
                let dx2 = sx[v] - ccx;
                let dy2 = sy[v] - ccy;
                let dlen = (dx2 * dx2 + dy2 * dy2).sqrt();
                let mut ex = sx[v];
                let mut ey = sy[v];
                if dlen > 1e-9 {
                    ex += dx2 / dlen * pad;
                    ey += dy2 / dlen * pad;
                }
                if v == 0 {
                    cr.move_to(ex, ey);
                } else {
                    cr.line_to(ex, ey);
                }
            }
            cr.close_path();
            cr.clip();
            cr.transform(&Matrix::new(ma, mb, mc, md, me, mf));
            pat.set_matrix(&Matrix::new(
                twf / nf,
                0.0,
                0.0,
                thf / nf,
                twf * i as f64 / nf,
                thf * j as f64 / nf,
            ));
            cr.set_source(&pat);
            cr.rectangle(-0.5, -0.5, 2.0, 2.0);
            cr.fill();
            cr.restore();
        }
    }
}

fn quad3_cmp(a: &Quad3, b: &Quad3) -> i32 {
    if a.depth < b.depth {
        return -1;
    }
    if a.depth > b.depth {
        return 1;
    }
    a.seq.cmp(&b.seq) as i32
}

pub fn invalidate() {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        r.quads.clear();
        r.tex.clear();
    });
}

pub fn registered(b: BoxRef<'_>) -> bool {
    REGISTRY.with(|r| r.borrow().quads.contains_key(&b.as_ptr())) || box_establishes_3d(b)
}

fn register(root: BoxRef<'_>, quads: &[Quad3]) {
    REGISTRY.with(|r| r.borrow_mut().quads.insert(root.as_ptr(), quads.to_vec()));
}

fn quad3_unproject(q: &Quad3, sx: f64, sy: f64) -> Option<(f64, f64, f64)> {
    let m = &q.m.m;
    let a = m[0] * q.bw;
    let b = m[1] * q.bh;
    let c = m[0] * q.bx + m[1] * q.by + m[3];
    let d = m[4] * q.bw;
    let e = m[5] * q.bh;
    let f = m[4] * q.bx + m[5] * q.by + m[7];
    let g = m[12] * q.bw;
    let h = m[13] * q.bh;
    let k = m[12] * q.bx + m[13] * q.by + m[15];
    let i0 = e * k - f * h;
    let i1 = c * h - b * k;
    let i2 = b * f - c * e;
    let i3 = f * g - d * k;
    let i4 = a * k - c * g;
    let i5 = c * d - a * f;
    let i6 = d * h - e * g;
    let i7 = b * g - a * h;
    let i8 = a * e - b * d;
    let det = a * i0 + b * i3 + c * i6;
    if det.abs() < 1e-12 {
        return None;
    }
    let tu = i0 * sx + i1 * sy + i2;
    let tv = i3 * sx + i4 * sy + i5;
    let tw = i6 * sx + i7 * sy + i8;
    if tw.abs() < 1e-12 {
        return None;
    }
    let u = tu / tw;
    let v = tv / tw;
    if !(-0.002..=1.002).contains(&u) || !(-0.002..=1.002).contains(&v) {
        return None;
    }
    let px = q.bx + u * q.bw;
    let py = q.by + v * q.bh;
    let w = m[12] * px + m[13] * py + m[15];
    if w < 1e-6 {
        return None;
    }
    let z = m[8] * px + m[9] * py + m[11];
    Some((u, v, z / w))
}

pub fn pick(root3d: BoxRef<'_>, x: f64, y: f64) -> *const NsBox {
    let existing = REGISTRY.with(|r| r.borrow().quads.get(&root3d.as_ptr()).cloned());
    let quads = match existing {
        Some(q) => q,
        None => {
            let fresh = collect_root_quads(root3d);
            register(root3d, &fresh);
            fresh
        }
    };
    let mut cands: Vec<(usize, f64, f64, f64)> = Vec::new();
    for (i, q) in quads.iter().enumerate() {
        if let Some((u, v, depth)) = quad3_unproject(q, x, y) {
            cands.push((i, depth, u, v));
        }
    }
    merge_sort_by(
        &mut cands,
        &|a: &(usize, f64, f64, f64), b: &(usize, f64, f64, f64)| {
            if a.1 > b.1 {
                -1
            } else if a.1 < b.1 {
                1
            } else if a.0 > b.0 {
                -1
            } else if a.0 < b.0 {
                1
            } else {
                0
            }
        },
    );
    for &(idx, _, u, v) in &cands {
        let q = &quads[idx];
        let Some(qb) = quad_box(q) else {
            continue;
        };
        let lx = q.bx + u * q.bw;
        let ly = q.by + v * q.bh;
        let hit = engine::box_hit_test(qb, lx, ly);
        if !hit.is_null() {
            return hit;
        }
    }
    core::ptr::null()
}

fn collect_root_quads(b: BoxRef<'_>) -> Vec<Quad3> {
    let mut quads = Vec::new();
    let mut root = Mat4::IDENTITY;
    if box_establishes_3d(b) {
        let d = box_perspective_px(b);
        if d > 0.0 {
            let rect = box_border_rect(b);
            let (pox, poy, _) = box_transform_origin(b, rect, P::PerspectiveOrigin);
            root.translate(pox, poy, 0.0);
            root.perspective(d);
            root.translate(-pox, -poy, 0.0);
        }
        let mut c = b.first_child();
        while let Some(child) = c {
            collect_3d_quads(child, &root, &mut quads);
            c = child.next_sibling();
        }
    } else {
        collect_3d_quads(b, &root, &mut quads);
    }
    for q in &mut quads {
        let [_, _, oz, ow] = q.m.apply(q.bx + q.bw / 2.0, q.by + q.bh / 2.0, 0.0);
        if ow > 1e-6 {
            q.depth = oz / ow;
        } else {
            q.depth = -1e30;
            let cxs = [q.bx, q.bx + q.bw, q.bx + q.bw, q.bx];
            let cys = [q.by, q.by, q.by + q.bh, q.by + q.bh];
            for c in 0..4 {
                let [_, _, oz, ow] = q.m.apply(cxs[c], cys[c], 0.0);
                if ow > 1e-6 && oz / ow > q.depth {
                    q.depth = oz / ow;
                }
            }
        }
    }
    merge_sort_by(&mut quads, &quad3_cmp);
    quads
}

fn debug_3d(b: BoxRef<'_>, quads: &[Quad3]) {
    if std::env::var_os("NS_3D_DEBUG").is_none() {
        return;
    }
    let name = |bx: BoxRef<'_>| {
        unsafe { Node::from_ptr(bx.dom_ptr().cast()) }
            .and_then(Node::name)
            .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned())
    };
    let class = |bx: BoxRef<'_>| {
        unsafe { Node::from_ptr(bx.dom_ptr().cast()) }
            .and_then(|n| n.attr(c"class"))
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    };
    let mut out = format!(
        "3d root <{} class={}> {} quads\n",
        name(b),
        class(b),
        quads.len()
    );
    for q in quads {
        let Some(qb) = quad_box(q) else {
            continue;
        };
        out.push_str(&format!(
            "  quad <{} class={}> rect {:.0},{:.0} {}x{} depth {:.2} corners",
            name(qb),
            class(qb),
            q.bx,
            q.by,
            q.bw,
            q.bh,
            q.depth
        ));
        let cxs = [q.bx, q.bx + q.bw, q.bx + q.bw, q.bx];
        let cys = [q.by, q.by, q.by + q.bh, q.by + q.bh];
        for c in 0..4 {
            let [ox, oy, _, ow] = q.m.apply(cxs[c], cys[c], 0.0);
            if ow > 1e-6 {
                out.push_str(&format!(" ({:.1},{:.1})", ox / ow, oy / ow));
            } else {
                out.push_str(&format!(" (w={ow:.3}!)"));
            }
        }
        out.push('\n');
    }
    southstar_glib::stderr_write(out.as_bytes());
}

pub fn paint_3d_root(cr: Cr, b: BoxRef<'_>, highlight: Option<&CStr>) {
    if box_establishes_3d(b)
        && matches!(
            b.kind(),
            BoxKind::Block | BoxKind::Table | BoxKind::TableCaption | BoxKind::TableCell
        )
    {
        paint_block(cr, b);
    }
    let quads = collect_root_quads(b);
    register(b, &quads);
    debug_3d(b, &quads);
    walk::with(|w| w.no_cull.set(w.no_cull.get() + 1));
    let debug = state::debug_point().is_some();
    for q in &quads {
        let before = if debug {
            cr.clip_extents()
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        paint_quad3(cr, q, highlight);
        if debug {
            let after = cr.clip_extents();
            if ((after.0 - before.0).abs() > 0.5
                || (after.1 - before.1).abs() > 0.5
                || (after.2 - before.2).abs() > 0.5
                || (after.3 - before.3).abs() > 0.5)
                && let Some(qb) = quad_box(q)
            {
                let node = unsafe { Node::from_ptr(qb.dom_ptr().cast()) };
                let line = format!(
                    "[quad3-LEAK] <{} class={}> rect {:.0},{:.0} {}x{}\n",
                    node.and_then(Node::name)
                        .map_or_else(|| "?".to_string(), |n| n.to_string_lossy().into_owned()),
                    node.and_then(|n| n.attr(c"class"))
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    q.bx,
                    q.by,
                    q.bw,
                    q.bh
                );
                southstar_glib::stderr_write(line.as_bytes());
            }
        }
    }
    walk::with(|w| w.no_cull.set(w.no_cull.get() - 1));
}
