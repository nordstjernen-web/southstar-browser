//! Southstar — border radii: resolving the corner radii of a style or box, scaling and insetting them, and tracing rounded rectangles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::{FRAC_PI_2, PI};

use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId as P, StyleRef, ValueRef};

use crate::ffi::cairo::Cr;
use crate::util::{UNIT_PERCENT, cmax, cmin, style_of};

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct CornerRadii {
    pub tl: f64,
    pub tr: f64,
    pub br: f64,
    pub bl: f64,
    pub tlv: f64,
    pub trv: f64,
    pub brv: f64,
    pub blv: f64,
}

impl CornerRadii {
    pub fn uniform(r: f64) -> CornerRadii {
        CornerRadii {
            tl: r,
            tr: r,
            br: r,
            bl: r,
            tlv: r,
            trv: r,
            brv: r,
            blv: r,
        }
    }

    pub fn is_zero(&self) -> bool {
        (self.tl <= 0.0 || self.tlv <= 0.0)
            && (self.tr <= 0.0 || self.trv <= 0.0)
            && (self.br <= 0.0 || self.brv <= 0.0)
            && (self.bl <= 0.0 || self.blv <= 0.0)
    }

    fn sums(&self) -> [f64; 4] {
        [
            self.tl + self.tr,
            self.trv + self.brv,
            self.br + self.bl,
            self.tlv + self.blv,
        ]
    }

    fn scaled(mut self, f: f64) -> CornerRadii {
        self.tl *= f;
        self.tr *= f;
        self.br *= f;
        self.bl *= f;
        self.tlv *= f;
        self.trv *= f;
        self.brv *= f;
        self.blv *= f;
        self
    }

    fn fit_factor(&self, w: f64, h: f64) -> f64 {
        let mut f = 1.0;
        let lens = [w, h, w, h];
        for (sum, len) in self.sums().into_iter().zip(lens) {
            if sum > 0.0 && len / sum < f {
                f = len / sum;
            }
        }
        f
    }

    pub fn fit(self, w: f64, h: f64) -> CornerRadii {
        let f = self.fit_factor(w, h);
        if f < 1.0 { self.scaled(f) } else { self }
    }

    pub fn fits_unscaled(&self, w: f64, h: f64) -> bool {
        let lens = [w, h, w, h];
        !self
            .sums()
            .into_iter()
            .zip(lens)
            .any(|(sum, len)| sum > 0.0 && len / sum < 1.0)
    }

    pub fn inset(self, top: f64, right: f64, bottom: f64, left: f64) -> CornerRadii {
        CornerRadii {
            tl: cmax(0.0, self.tl - left),
            tr: cmax(0.0, self.tr - right),
            br: cmax(0.0, self.br - right),
            bl: cmax(0.0, self.bl - left),
            tlv: cmax(0.0, self.tlv - top),
            trv: cmax(0.0, self.trv - top),
            brv: cmax(0.0, self.brv - bottom),
            blv: cmax(0.0, self.blv - bottom),
        }
    }
}

fn corner_radius_px(v: Option<ValueRef<'_>>, basis_w: f64, basis_h: f64) -> (f64, f64) {
    let Some(v) = v else {
        return (-1.0, -1.0);
    };
    let (rh, rv) = match v.kind() {
        Kind::Length => {
            let (r, unit) = v.length().unwrap_or_default();
            if unit == UNIT_PERCENT {
                (r * basis_w / 100.0, r * basis_h / 100.0)
            } else {
                (r, r)
            }
        }
        Kind::Size => {
            let Some(s) = v.size() else {
                return (-1.0, -1.0);
            };
            (
                if s.w_unit == UNIT_PERCENT {
                    s.w * basis_w / 100.0
                } else {
                    s.w
                },
                if s.h_unit == UNIT_PERCENT {
                    s.h * basis_h / 100.0
                } else {
                    s.h
                },
            )
        }
        Kind::Calc => {
            let (pct, px) = v.calc().unwrap_or_default();
            (px + pct * basis_w / 100.0, px + pct * basis_h / 100.0)
        }
        _ => return (-1.0, -1.0),
    };
    (
        if rh > 0.0 { rh } else { 0.0 },
        if rv > 0.0 { rv } else { 0.0 },
    )
}

pub fn style_border_radii(s: Option<StyleRef<'_>>, w: f64, h: f64) -> CornerRadii {
    let Some(s) = s else {
        return CornerRadii::default();
    };
    let (mut base_h, mut base_v) = corner_radius_px(s.get(P::BorderRadius), w, h);
    if base_h < 0.0 {
        base_h = 0.0;
    }
    if base_v < 0.0 {
        base_v = 0.0;
    }
    let corners = [
        P::BorderTopLeftRadius,
        P::BorderTopRightRadius,
        P::BorderBottomRightRadius,
        P::BorderBottomLeftRadius,
    ];
    let mut hs = [0.0; 4];
    let mut vs = [0.0; 4];
    for (i, prop) in corners.into_iter().enumerate() {
        let (rh, rv) = corner_radius_px(s.get(prop), w, h);
        hs[i] = if rh >= 0.0 { rh } else { base_h };
        vs[i] = if rv >= 0.0 { rv } else { base_v };
    }
    CornerRadii {
        tl: hs[0],
        tr: hs[1],
        br: hs[2],
        bl: hs[3],
        tlv: vs[0],
        trv: vs[1],
        brv: vs[2],
        blv: vs[3],
    }
}

pub fn border_box_size(b: BoxRef<'_>) -> (f64, f64) {
    let p = b.padding();
    let bd = b.border();
    (
        b.content_width() + p.left + p.right + bd.left + bd.right,
        b.content_height() + p.top + p.bottom + bd.top + bd.bottom,
    )
}

pub fn box_border_radii(b: Option<BoxRef<'_>>) -> CornerRadii {
    let Some(b) = b else {
        return style_border_radii(None, 0.0, 0.0);
    };
    let (w, h) = border_box_size(b);
    style_border_radii(style_of(b), w, h)
}

fn corner_arc(cr: Cr, cx: f64, cy: f64, rx: f64, ry: f64, a0: f64, a1: f64) {
    cr.save();
    cr.translate(cx, cy);
    cr.scale(rx, ry);
    cr.arc(0.0, 0.0, 1.0, a0, a1);
    cr.restore();
}

pub fn rounded_rect_path(cr: Cr, x: f64, y: f64, w: f64, h: f64, c: CornerRadii) {
    if c.is_zero() || w.is_nan() || w <= 0.0 || h.is_nan() || h <= 0.0 {
        cr.rectangle(x, y, w, h);
        return;
    }
    let c = c.fit(w, h);
    cr.new_sub_path();
    if c.tr > 0.0 && c.trv > 0.0 {
        corner_arc(cr, x + w - c.tr, y + c.trv, c.tr, c.trv, -FRAC_PI_2, 0.0);
    } else {
        cr.move_to(x + w, y);
    }
    if c.br > 0.0 && c.brv > 0.0 {
        corner_arc(cr, x + w - c.br, y + h - c.brv, c.br, c.brv, 0.0, FRAC_PI_2);
    } else {
        cr.line_to(x + w, y + h);
    }
    if c.bl > 0.0 && c.blv > 0.0 {
        corner_arc(cr, x + c.bl, y + h - c.blv, c.bl, c.blv, FRAC_PI_2, PI);
    } else {
        cr.line_to(x, y + h);
    }
    if c.tl > 0.0 && c.tlv > 0.0 {
        corner_arc(cr, x + c.tl, y + c.tlv, c.tl, c.tlv, PI, 1.5 * PI);
    } else {
        cr.line_to(x, y);
    }
    cr.close_path();
}

pub fn fill_outer_shadow(
    cr: Cr,
    outer: (f64, f64, f64, f64),
    inner: (f64, f64, f64, f64),
    radii: CornerRadii,
) {
    let (ox, oy, ow, oh) = outer;
    let (ix, iy, iw, ih) = inner;
    let x0 = cmin(ox, ix) - 1.0;
    let y0 = cmin(oy, iy) - 1.0;
    let x1 = cmax(ox + ow, ix + iw) + 1.0;
    let y1 = cmax(oy + oh, iy + ih) + 1.0;
    cr.save();
    cr.new_path();
    cr.rectangle(x0, y0, x1 - x0, y1 - y0);
    rounded_rect_path(cr, ix, iy, iw, ih, radii);
    cr.set_fill_rule(crate::ffi::cairo::FILL_RULE_EVEN_ODD);
    cr.clip();
    cr.set_fill_rule(crate::ffi::cairo::FILL_RULE_WINDING);
    rounded_rect_path(cr, ox, oy, ow, oh, radii);
    cr.fill();
    cr.restore();
}
