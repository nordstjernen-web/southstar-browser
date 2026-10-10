//! Southstar — CSS masks made of gradient layers: the alpha group a box is painted through, with each layer's mask-clip box and mask-composite operator.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::{Gradient, Kind, PropId as P, StyleRef, ValueRef, layer, layer_count};

use crate::decor::gradient_pattern;
use crate::ffi::cairo::{self, Cr, Pattern};
use crate::radii::{border_box_size, box_border_radii, rounded_rect_path};
use crate::util::{cmax, get, keyword, style_of};

fn mask_gradient_pattern(gr: &Gradient, border: (f64, f64, f64, f64)) -> Option<Pattern> {
    if gr.conic != 0 || gr.n_stops < 1 {
        return None;
    }
    let (bx, by, bw, bh) = border;
    let cx = bx + gr.center_x * bw + gr.center_x_px;
    let cy = by + gr.center_y * bh + gr.center_y_px;
    Some(gradient_pattern(gr, border, cx, cy, false))
}

fn layer_gradient(v: Option<ValueRef<'_>>) -> Option<&Gradient> {
    v.and_then(ValueRef::gradient).filter(|g| g.conic == 0)
}

fn mask_layer_is_none(v: ValueRef<'_>) -> bool {
    keyword(Some(v)).is_some_and(|k| k.to_bytes() == b"none")
}

pub fn mask_layers_paintable(s: Option<StyleRef<'_>>) -> bool {
    let Some(mask) = get(s, P::MaskImage) else {
        return false;
    };
    let mut any = false;
    for l in mask.layers() {
        if layer_gradient(Some(l)).is_some() {
            any = true;
        } else if !mask_layer_is_none(l) {
            return false;
        }
    }
    any
}

fn mask_composite_operator(v: Option<ValueRef<'_>>) -> i32 {
    let op = v
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(|k| k.to_bytes());
    match op {
        Some(b"subtract") => cairo::OPERATOR_OUT,
        Some(b"intersect") => cairo::OPERATOR_IN,
        Some(b"exclude") => cairo::OPERATOR_XOR,
        _ => cairo::OPERATOR_OVER,
    }
}

fn mask_clip_box_path(cr: Cr, b: BoxRef<'_>, clip: Option<ValueRef<'_>>) {
    let kw = clip
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(|k| k.to_bytes());
    let m = b.margin();
    let x = b.x() + m.left;
    let y = b.y() + m.top;
    let (w, h) = border_box_size(b);
    let radii = box_border_radii(Some(b));
    let (mut t, mut r, mut bo, mut l) = (0.0, 0.0, 0.0, 0.0);
    if matches!(kw, Some(b"padding-box" | b"content-box")) {
        let bd = b.border();
        t = bd.top;
        r = bd.right;
        bo = bd.bottom;
        l = bd.left;
    }
    if kw == Some(b"content-box") {
        let p = b.padding();
        t += p.top;
        r += p.right;
        bo += p.bottom;
        l += p.left;
    }
    if kw == Some(b"no-clip") {
        cr.rectangle(x - 1e5, y - 1e5, w + 2e5, h + 2e5);
        return;
    }
    rounded_rect_path(
        cr,
        x + l,
        y + t,
        cmax(0.0, w - l - r),
        cmax(0.0, h - t - bo),
        radii.inset(t, r, bo, l),
    );
}

pub fn mask_layers_pattern(cr: Cr, b: BoxRef<'_>) -> Pattern {
    let s = style_of(b);
    let mask = get(s, P::MaskImage);
    let n = layer_count(mask);
    let m = b.margin();
    let (bw, bh) = border_box_size(b);
    let border = (b.x() + m.left, b.y() + m.top, bw, bh);
    cr.push_group_alpha();
    for i in (0..n).rev() {
        let lv = layer(mask, i);
        let grad = layer_gradient(lv).and_then(|g| mask_gradient_pattern(g, border));
        cr.save();
        cr.new_path();
        mask_clip_box_path(cr, b, layer(get(s, P::MaskClip), i));
        cr.clip();
        cr.set_operator(if i == n - 1 {
            cairo::OPERATOR_OVER
        } else {
            mask_composite_operator(layer(get(s, P::MaskComposite), i))
        });
        match &grad {
            Some(g) => cr.set_source(g),
            None => cr.set_source_rgba(0.0, 0.0, 0.0, 0.0),
        }
        cr.paint();
        cr.restore();
        drop(grad);
    }
    cr.pop_group()
}
