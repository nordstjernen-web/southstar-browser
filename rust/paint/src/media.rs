//! Southstar — replaced content: images and video frames from textures with object-fit, object-position, filters and clip-path, broken and loading images, SVG and MathML boxes, and the video and audio element chrome.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::PI;
use core::ffi::{CStr, c_void};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_image::ImageRef;
use southstar_layout::{BoxKind, BoxRef};
use southstar_style::{Kind, PropId as P, StyleRef, ValueRef};

use crate::decor::{ANIM_TARGET_BG_COLOR, bg_size_px, rgba_anim};
use crate::ffi::cairo::{self, Cr, Surface, SurfaceRef};
use crate::ffi::engine::{self, Texture, Video};
use crate::ffi::pango::{self, FontDescription};
use crate::filter::{
    TextureFit, apply_image_filter, filter_has_bitmap_effect, paint_texture_drop_shadows,
};
use crate::radii::{CornerRadii, box_border_radii, rounded_rect_path};
use crate::text;
use crate::util::{Rgba, UNIT_PERCENT, cmin, get, length_or, style_of};

pub struct TextureSurfaces {
    plain: Option<Surface>,
    filter: Option<Vec<u8>>,
    filtered: Option<Surface>,
}

unsafe extern "C" fn texture_surfaces_free(data: *mut c_void) {
    if !data.is_null() {
        drop(unsafe { Box::from_raw(data.cast::<TextureSurfaces>()) });
    }
}

fn texture_surface_create(
    tex: Texture,
    iw: i32,
    ih: i32,
    filter: Option<&CStr>,
) -> Option<Surface> {
    let surf = Surface::image(cairo::FORMAT_ARGB32, iw, ih)?;
    let mut s = surf.as_ref();
    let stride = s.stride();
    let data = s.data_mut();
    tex.download(data, stride as usize);
    if filter.is_some() {
        apply_image_filter(data, stride, iw, ih, filter);
    }
    s.mark_dirty();
    Some(surf)
}

pub fn texture_surface_cached(tex: Texture, filter: Option<&CStr>) -> Option<SurfaceRef> {
    let iw = tex.width();
    let ih = tex.height();
    if iw <= 0 || ih <= 0 {
        return None;
    }
    let mut ts = tex.user_data().cast::<TextureSurfaces>();
    if ts.is_null() {
        ts = Box::into_raw(Box::new(TextureSurfaces {
            plain: None,
            filter: None,
            filtered: None,
        }));
        unsafe { tex.set_user_data(ts.cast(), texture_surfaces_free) };
    }
    let ts = unsafe { &mut *ts };
    let Some(filter) = filter else {
        if ts.plain.is_none() {
            ts.plain = texture_surface_create(tex, iw, ih, None);
        }
        return ts.plain.as_ref().map(Surface::as_ref);
    };
    if ts.filtered.is_some() && ts.filter.as_deref() == Some(filter.to_bytes()) {
        return ts.filtered.as_ref().map(Surface::as_ref);
    }
    let surf = texture_surface_create(tex, iw, ih, Some(filter))?;
    ts.filtered = Some(surf);
    ts.filter = Some(filter.to_bytes().to_vec());
    ts.filtered.as_ref().map(Surface::as_ref)
}

fn strtod(text: &[u8], pos: usize) -> Option<(f64, usize)> {
    let (v, n) = southstar_glib::ascii_strtod_prefix(&text[pos.min(text.len())..]);
    (n > 0).then_some((v, pos + n))
}

pub fn apply_box_content_clip(cr: Cr, b: BoxRef<'_>) -> bool {
    if b.x().is_nan() || b.y().is_nan() || b.content_width().is_nan() || b.content_height().is_nan()
    {
        return false;
    }
    let mut clipped = false;
    let radii = box_border_radii(Some(b));
    if !radii.is_zero() {
        rounded_rect_path(
            cr,
            b.x(),
            b.y(),
            b.content_width(),
            b.content_height(),
            radii,
        );
        cr.clip();
        clipped = true;
    }
    let st = style_of(b);
    let Some(cpv) = get(st, P::ClipPath) else {
        return clipped;
    };
    if cpv.kind() != Kind::Keyword {
        return clipped;
    }
    let Some(cp) = cpv.keyword_text().map(CStr::to_bytes) else {
        return clipped;
    };
    if cp.is_empty() || cp == b"none" {
        return clipped;
    }
    let w = b.content_width();
    let h = b.content_height();
    let cx = b.x() + w / 2.0;
    let cy = b.y() + h / 2.0;
    let starts =
        |prefix: &[u8]| cp.len() >= prefix.len() && cp[..prefix.len()].eq_ignore_ascii_case(prefix);
    let min_wh = if w < h { w } else { h };
    if starts(b"circle") {
        let mut r = min_wh / 2.0;
        if let Some(paren) = cp.iter().position(|&c| c == b'(') {
            let mut p = paren + 1;
            while p < cp.len() && (cp[p] == b' ' || cp[p] == b'\t') {
                p += 1;
            }
            if p < cp.len()
                && cp[p] != b')'
                && let Some((rv, end)) = strtod(cp, p)
                && rv > 0.0
            {
                r = if cp.get(end) == Some(&b'%') {
                    rv / 100.0 * (min_wh / 2.0)
                } else {
                    rv
                };
            }
        }
        cr.new_sub_path();
        cr.arc(cx, cy, r, 0.0, 2.0 * PI);
        cr.clip();
        clipped = true;
    } else if starts(b"ellipse") {
        cr.save();
        cr.translate(cx, cy);
        cr.scale(w / 2.0, h / 2.0);
        cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
        cr.restore();
        cr.clip();
        clipped = true;
    } else if starts(b"polygon") {
        let Some(paren) = cp.iter().position(|&c| c == b'(') else {
            return clipped;
        };
        let rest = &cp[paren + 1..];
        let Some(end) = rest.iter().rposition(|&c| c == b')') else {
            return clipped;
        };
        let body = &rest[..end];
        let mut first = true;
        cr.new_sub_path();
        for vert in body.split(|&c| c == b',') {
            let coords = strip(vert);
            if coords.is_empty() {
                continue;
            }
            let Some((xv, mut e1)) = strtod(coords, 0) else {
                continue;
            };
            let xpct = coords.get(e1) == Some(&b'%');
            if xpct {
                e1 += 1;
            }
            while e1 < coords.len() && (coords[e1] == b' ' || coords[e1] == b'\t') {
                e1 += 1;
            }
            let Some((yv, e2)) = strtod(coords, e1) else {
                continue;
            };
            let ypct = coords.get(e2) == Some(&b'%');
            let px = if xpct {
                b.x() + xv / 100.0 * w
            } else {
                b.x() + xv
            };
            let py = if ypct {
                b.y() + yv / 100.0 * h
            } else {
                b.y() + yv
            };
            if first {
                cr.move_to(px, py);
                first = false;
            } else {
                cr.line_to(px, py);
            }
        }
        if !first {
            cr.close_path();
            cr.clip();
            clipped = true;
        }
    } else if starts(b"inset")
        && let Some(paren) = cp.iter().position(|&c| c == b'(')
        && let Some((pv, end)) = strtod(cp, paren + 1)
    {
        let pad = if cp.get(end) == Some(&b'%') {
            pv / 100.0 * min_wh
        } else {
            pv
        };
        cr.rectangle(b.x() + pad, b.y() + pad, w - 2.0 * pad, h - 2.0 * pad);
        cr.clip();
        clipped = true;
    }
    clipped
}

fn strip(token: &[u8]) -> &[u8] {
    let start = token
        .iter()
        .position(|&c| !crate::util::is_space(c))
        .unwrap_or(token.len());
    let end = token
        .iter()
        .rposition(|&c| !crate::util::is_space(c))
        .map_or(start, |e| e + 1);
    &token[start..end.max(start)]
}

fn object_position_offset(
    st: Option<StyleRef<'_>>,
    prop: P,
    box_size: f64,
    object_size: f64,
) -> f64 {
    let delta = box_size - object_size;
    if let Some((v, unit)) = get(st, prop).and_then(ValueRef::length) {
        if unit == UNIT_PERCENT {
            return delta * v / 100.0;
        }
        return bg_size_px(v, unit, box_size);
    }
    delta * 0.5
}

fn video_of(b: BoxRef<'_>) -> Option<Video<'_>> {
    b.media()
        .and_then(|m| unsafe { Video::from_ptr(m.video()) })
}

pub fn paint_texture(cr: Cr, b: BoxRef<'_>, tex: Texture) -> bool {
    let iw = tex.width();
    let ih = tex.height();
    if iw <= 0 || ih <= 0 {
        return false;
    }
    if b.content_width() <= 0.0 || b.content_height() <= 0.0 {
        return false;
    }
    let st = style_of(b);
    let filter_kw = get(st, P::Filter)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text);
    let surface_filter = filter_kw.filter(|f| filter_has_bitmap_effect(Some(f)));
    let Some(surf) = texture_surface_cached(tex, surface_filter) else {
        return false;
    };
    let cw = b.content_width();
    let ch = b.content_height();
    let (iwf, ihf) = (f64::from(iw), f64::from(ih));
    let mut sx = cw / iwf;
    let mut sy = ch / ihf;
    let mut fit = get(st, P::ObjectFit)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes);
    if fit.is_none() && b.kind() == BoxKind::Video {
        fit = Some(b"contain");
    }
    let mut ox = 0.0;
    let mut oy = 0.0;
    if let Some(fit) = fit.filter(|f| *f != b"fill") {
        let s = match fit {
            b"contain" => cmin(sx, sy),
            b"cover" => crate::util::cmax(sx, sy),
            b"none" => 1.0,
            b"scale-down" => cmin(1.0, cmin(sx, sy)),
            _ => -1.0,
        };
        if s > 0.0 {
            sx = s;
            sy = s;
            ox = object_position_offset(st, P::ObjectPositionX, cw, iwf * s);
            oy = object_position_offset(st, P::ObjectPositionY, ch, ihf * s);
        }
    }
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let cx = b.x() + m.left + bd.left + p.left;
    let cy = b.y() + m.top + bd.top + p.top;
    paint_texture_drop_shadows(cr, surf, b, &TextureFit { sx, sy, ox, oy }, filter_kw);
    apply_box_content_clip(cr, b);
    cr.rectangle(cx, cy, cw, ch);
    cr.clip();
    cr.translate(cx + ox, cy + oy);
    cr.scale(sx, sy);
    cr.set_source_surface(surf, 0.0, 0.0);
    if video_of(b).is_some_and(|v| v.playing() && v.frame_texture() == Some(tex)) {
        cr.set_source_filter(cairo::FILTER_FAST);
    }
    if crate::decor::style_pixelated(st) {
        cr.set_source_filter(cairo::FILTER_NEAREST);
    }
    cr.paint();
    true
}

fn paint_failed_image(cr: Cr, b: BoxRef<'_>) {
    let w = b.content_width();
    let h = b.content_height();
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.rectangle(b.x(), b.y(), w, h);
    cr.fill_preserve();
    cr.set_source_rgb(0.78, 0.78, 0.78);
    cr.set_line_width(1.0);
    cr.stroke();
    let s = cmin(10.0, cmin(w, h) - 2.0);
    if s < 4.0 {
        return;
    }
    let mut x = b.x() + 3.0;
    let mut y = b.y() + 3.0;
    if x + s > b.x() + w {
        x = b.x() + crate::util::cmax(0.0, w - s - 1.0);
    }
    if y + s > b.y() + h {
        y = b.y() + crate::util::cmax(0.0, h - s - 1.0);
    }
    cr.set_source_rgb(0.82, 0.0, 0.0);
    cr.set_line_width(2.0);
    cr.move_to(x, y);
    cr.line_to(x + s, y + s);
    cr.move_to(x + s, y);
    cr.line_to(x, y + s);
    cr.stroke();
}

fn content_origin(b: BoxRef<'_>) -> (f64, f64) {
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    (
        b.x() + m.left + bd.left + p.left,
        b.y() + m.top + bd.top + p.top,
    )
}

pub fn paint_svg(cr: Cr, b: BoxRef<'_>) {
    if b.dom_ptr().is_null() {
        return;
    }
    let w = b.content_width();
    let h = b.content_height();
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let (x, y) = content_origin(b);
    cr.save();
    cr.translate(x, y);
    engine::svg_render_node(cr, b, w, h);
    cr.restore();
}

pub fn paint_math(cr: Cr, b: BoxRef<'_>) {
    if b.dom_ptr().is_null() {
        return;
    }
    let s = style_of(b);
    let fpx = length_or(get(s, P::FontSize), 16.0);
    let c = get(s, P::Color)
        .and_then(ValueRef::color)
        .map_or(Rgba::new(0.0, 0.0, 0.0, 1.0), Rgba::from_bytes);
    let (ox, oy) = content_origin(b);
    engine::math_paint(cr, b, ox, oy, fpx, c);
}

fn box_node(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

pub fn paint_image(cr: Cr, b: BoxRef<'_>) {
    let dom = box_node(b);
    if dom.and_then(Node::element_name) == Some(b"canvas") {
        return;
    }
    let mut img = None;
    if dom.is_some() {
        img = engine::js_image_for_node(b);
    }
    if img.is_none() {
        img = b
            .media()
            .and_then(|m| unsafe { ImageRef::from_ptr(m.image()) });
    }
    cr.save();
    let tex = img
        .filter(|i| i.loaded())
        .and_then(|i| unsafe { Texture::from_raw(i.texture()) });
    if let Some(tex) = tex {
        paint_texture(cr, b, tex);
    } else if img.is_some_and(|i| i.failed()) {
        if b.content_width() > 24.0 || b.content_height() > 24.0 {
            paint_failed_image(cr, b);
        }
    } else {
        if b.content_width() <= 24.0 && b.content_height() <= 24.0 {
            cr.restore();
            return;
        }
        let s = style_of(b);
        let bg = rgba_anim(
            Some(b),
            ANIM_TARGET_BG_COLOR,
            get(s, P::BackgroundColor),
            Rgba::default(),
        );
        let has_bg = bg.a > 0.0;
        let (cx, cy) = content_origin(b);
        if !has_bg {
            cr.set_source_rgb(0.92, 0.92, 0.92);
            cr.rectangle(cx, cy, b.content_width(), b.content_height());
            cr.fill_preserve();
            cr.set_source_rgb(0.6, 0.6, 0.6);
            cr.set_line_width(1.0);
            cr.stroke();
        }
        let alt = dom.and_then(|d| d.attr(c"alt")).filter(|a| !a.is_empty());
        if let Some(alt) = alt.filter(|_| b.content_width() > 24.0 && b.content_height() > 16.0) {
            let layout = text::create_layout();
            layout.set_text(alt);
            layout.set_width(((b.content_width() - 8.0) * pango::SCALE_F) as i32);
            layout.set_ellipsize(pango::ELLIPSIZE_END);
            let (_, ph) = layout.pixel_size();
            cr.set_source_rgb(0.3, 0.3, 0.3);
            cr.move_to(cx + 4.0, cy + (b.content_height() - f64::from(ph)) / 2.0);
            layout.show(cr);
        }
    }
    cr.restore();
}

fn paint_video_caption(cr: Cr, b: BoxRef<'_>, cue: &[u8]) {
    let lines: Vec<&[u8]> = cue.split(|&c| c == b'\n').collect();
    if lines.is_empty() {
        return;
    }
    let fs = ((b.content_height() * 0.07 + 0.5) as i32).clamp(11, 26);
    let fd = FontDescription::from_string(c"sans");
    fd.set_absolute_size(f64::from(text::pango_font_size(f64::from(fs) * 4.0 / 3.0)));
    fd.set_weight(pango::WEIGHT_MEDIUM);
    let pad = 3.0;
    let gap = 1.0;
    let mut total = 0.0;
    let mut lays = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let layout = text::create_layout();
        layout.set_font_description(fd.as_ref());
        let text = if line.is_empty() { &b" "[..] } else { line };
        let c =
            CString::new(text.split(|&c| c == 0).next().unwrap_or_default()).unwrap_or_default();
        layout.set_text(&c);
        let (lw, lh) = layout.pixel_size();
        total += f64::from(lh) + 2.0 * pad + if i > 0 { gap } else { 0.0 };
        lays.push((layout, lw, lh));
    }
    let mut y = b.y() + b.content_height() - b.content_height() * 0.05 - total;
    if y < b.y() + 2.0 {
        y = b.y() + 2.0;
    }
    cr.save();
    for (layout, lw, lh) in &lays {
        let bw = f64::from(*lw) + 2.0 * pad;
        let mut bx = b.x() + (b.content_width() - bw) / 2.0;
        if bx < b.x() {
            bx = b.x();
        }
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.6);
        cr.rectangle(bx, y, bw, f64::from(*lh) + 2.0 * pad);
        cr.fill();
        cr.move_to(bx + pad, y + pad);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        layout.show(cr);
        y += f64::from(*lh) + 2.0 * pad + gap;
    }
    cr.restore();
}

fn paint_video_note_rects(cr: Cr, b: BoxRef<'_>, v: Video<'_>, fit_mode: i32) {
    let (dx0, dy0) = cr.user_to_device(b.x(), b.y());
    let (dx1, dy1) = cr.user_to_device(b.x() + b.content_width(), b.y() + b.content_height());
    v.note_paint_rect(dx0, dy0, dx1 - dx0, dy1 - dy0, fit_mode);
    let (cx0, cy0, cx1, cy1) = cr.clip_extents();
    let (cx0, cy0) = cr.user_to_device(cx0, cy0);
    let (cx1, cy1) = cr.user_to_device(cx1, cy1);
    v.note_paint_clip(
        cmin(cx0, cx1),
        cmin(cy0, cy1),
        (cx1 - cx0).abs(),
        (cy1 - cy0).abs(),
    );
}

fn paint_audio_control(cr: Cr, b: BoxRef<'_>) {
    let x = b.x();
    let y = b.y();
    let w = b.content_width();
    let h = b.content_height();
    if w.is_nan() || w <= 0.0 || h.is_nan() || h <= 0.0 {
        return;
    }
    cr.save();
    rounded_rect_path(cr, x, y, w, h, CornerRadii::uniform(4.0));
    cr.set_source_rgb(0.96, 0.97, 0.98);
    cr.fill_preserve();
    cr.set_source_rgb(0.55, 0.58, 0.62);
    cr.set_line_width(1.0);
    cr.stroke();

    let cy = y + h / 2.0;
    let play_x = x + 13.0;
    let play_r = (h * 0.28).clamp(5.0, 9.0);
    cr.arc(play_x, cy, play_r, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.20, 0.23, 0.26);
    cr.fill();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(play_x - play_r * 0.28, cy - play_r * 0.45);
    cr.line_to(play_x + play_r * 0.45, cy);
    cr.line_to(play_x - play_r * 0.28, cy + play_r * 0.45);
    cr.close_path();
    cr.fill();

    let mut dtext = String::new();
    if let Some(dur) = box_node(b)
        .and_then(|d| d.attr(c"data-durationhint"))
        .filter(|d| !d.is_empty())
        && let Some((sec_d, _)) = strtod(dur.to_bytes(), 0)
        && sec_d >= 0.0
    {
        let sec = (sec_d + 0.5) as i32;
        dtext = format!("{}:{:02}", sec / 60, sec % 60);
    }
    let mut text_w = 0.0;
    if !dtext.is_empty() {
        let layout = text::create_layout();
        let fd = FontDescription::from_string(c"sans");
        fd.set_absolute_size(f64::from(text::pango_font_size(12.0)));
        layout.set_font_description(fd.as_ref());
        layout.set_text_bytes(dtext.as_bytes());
        let (tw, th) = layout.pixel_size();
        text_w = f64::from(tw) + 10.0;
        cr.move_to(x + w - f64::from(tw) - 8.0, y + (h - f64::from(th)) / 2.0);
        cr.set_source_rgb(0.18, 0.20, 0.23);
        layout.show(cr);
    }
    let tx0 = x + 31.0;
    let tx1 = x + w - if dtext.is_empty() { 12.0 } else { text_w + 8.0 };
    if tx1 > tx0 + 12.0 {
        cr.set_source_rgb(0.72, 0.74, 0.77);
        cr.set_line_width(3.0);
        cr.move_to(tx0, cy);
        cr.line_to(tx1, cy);
        cr.stroke();
        cr.arc(tx0, cy, 3.5, 0.0, 2.0 * PI);
        cr.set_source_rgb(0.20, 0.23, 0.26);
        cr.fill();
    }
    cr.restore();
}

fn debug_composite(v: Video<'_>, punched: bool, has_tex: bool, b: BoxRef<'_>) {
    if std::env::var_os("NS_DBG_COMPOSITE").is_none() {
        return;
    }
    if !crate::state::composite_log_due(engine::monotonic_time()) {
        return;
    }
    let line = format!(
        "[hole] punched={} opened={} tex={} box={:.0},{:.0} {:.0}x{:.0}
",
        i32::from(punched),
        i32::from(v.video_opened()),
        i32::from(has_tex),
        b.x(),
        b.y(),
        b.content_width(),
        b.content_height()
    );
    southstar_glib::stderr_write(line.as_bytes());
}

pub fn paint_video(cr: Cr, b: BoxRef<'_>) {
    if let Some(media) = b.media()
        && media.video_audio_src().is_some()
        && media.video_src().is_none()
    {
        paint_audio_control(cr, b);
        return;
    }
    let v = video_of(b);
    let tex = v.and_then(|v| v.frame_texture().or_else(|| v.poster_texture()));
    if let Some(v) = v {
        let fit = match get(style_of(b), P::ObjectFit) {
            Some(fv) if fv.kind() == Kind::Keyword => fv.keyword_text().map(CStr::to_bytes),
            _ => Some(&b"contain"[..]),
        };
        let fit_mode = match fit {
            Some(b"fill") => 0,
            Some(b"cover") => 2,
            Some(b"none") => 3,
            Some(b"scale-down") => 4,
            _ => 1,
        };
        if engine::layers_mode() == 0 {
            paint_video_note_rects(cr, b, v, fit_mode);
        }
    }
    let loaded = |raw: *mut c_void| {
        unsafe { ImageRef::from_ptr(raw) }.is_some_and(|i| i.loaded() && !i.texture().is_null())
    };
    let mut bg_painted = b.media().is_some_and(|m| loaded(m.bg_image()));
    if !bg_painted && let Some(images) = b.media().and_then(|m| m.bg_layer_images()) {
        bg_painted = images.into_iter().any(loaded);
    }
    let punched = engine::video_helper_composited(v);
    if let Some(v) = v {
        debug_composite(v, punched, tex.is_some(), b);
    }
    cr.save();
    if punched {
        engine::layers_note_video(cr);
        engine::video_hole_record(cr, b.x(), b.y(), b.content_width(), b.content_height());
        cr.set_operator(cairo::OPERATOR_CLEAR);
        cr.rectangle(b.x(), b.y(), b.content_width(), b.content_height());
        cr.fill();
    } else if let Some(tex) = tex {
        paint_texture(cr, b, tex);
    } else if !bg_painted {
        let dom = box_node(b);
        let ambient = dom.is_some_and(|d| {
            d.attr(c"autoplay").is_some()
                && d.attr(c"muted").is_some()
                && d.attr(c"controls").is_none()
        });
        if !ambient {
            cr.set_source_rgb(0.10, 0.10, 0.10);
            cr.rectangle(b.x(), b.y(), b.content_width(), b.content_height());
            cr.fill();
        }
    }
    cr.restore();
    if let Some(cue) = engine::video_active_cue_text(v).filter(|c| !c.is_empty())
        && b.content_width() > 24.0
        && b.content_height() > 24.0
    {
        paint_video_caption(cr, b, cue.to_bytes());
    }
}
