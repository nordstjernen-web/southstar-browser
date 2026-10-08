//! Southstar — SVG presentation state: paints, strokes, fonts and how attributes, style declarations and the cascade set them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::borrow::Cow;
use std::ffi::CString;

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_style::{Prop, StyleRef, UNIT_PERCENT, Value, parse_color};

use crate::Ctx;
use crate::ffi::{
    CAP_BUTT, CAP_ROUND, CAP_SQUARE, FILL_EVEN_ODD, FILL_WINDING, JOIN_BEVEL, JOIN_MITER,
    JOIN_ROUND,
};
use crate::parse::{self, eq_ci, is_ws};

const DECL_MAX: usize = 256;

pub static COLOR: Prop = Prop::new(c"color");
pub static FILL: Prop = Prop::new(c"fill");
pub static STROKE: Prop = Prop::new(c"stroke");
pub static FILL_OPACITY: Prop = Prop::new(c"fill-opacity");
pub static STROKE_OPACITY: Prop = Prop::new(c"stroke-opacity");
pub static STROKE_WIDTH: Prop = Prop::new(c"stroke-width");
pub static STROKE_MITERLIMIT: Prop = Prop::new(c"stroke-miterlimit");
pub static STROKE_DASHOFFSET: Prop = Prop::new(c"stroke-dashoffset");
pub static STROKE_LINECAP: Prop = Prop::new(c"stroke-linecap");
pub static STROKE_LINEJOIN: Prop = Prop::new(c"stroke-linejoin");
pub static FILL_RULE: Prop = Prop::new(c"fill-rule");
pub static CLIP_RULE: Prop = Prop::new(c"clip-rule");
pub static TEXT_ANCHOR: Prop = Prop::new(c"text-anchor");
pub static PAINT_ORDER: Prop = Prop::new(c"paint-order");
pub static STROKE_DASHARRAY: Prop = Prop::new(c"stroke-dasharray");
pub static VECTOR_EFFECT: Prop = Prop::new(c"vector-effect");
pub static VISIBILITY: Prop = Prop::new(c"visibility");
pub static FONT_SIZE: Prop = Prop::new(c"font-size");
pub static FONT_WEIGHT: Prop = Prop::new(c"font-weight");
pub static FONT_STYLE: Prop = Prop::new(c"font-style");
pub static FONT_FAMILY: Prop = Prop::new(c"font-family");
pub static STOP_COLOR: Prop = Prop::new(c"stop-color");
pub static STOP_OPACITY: Prop = Prop::new(c"stop-opacity");
pub static DISPLAY: Prop = Prop::new(c"display");
pub static OPACITY: Prop = Prop::new(c"opacity");

pub fn cmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn cmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    if x > hi {
        hi
    } else if x < lo {
        lo
    } else {
        x
    }
}

pub fn diag_basis(w: f64, h: f64) -> f64 {
    (w * w + h * h).sqrt() / core::f64::consts::SQRT_2
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PaintKind {
    None,
    Color,
    Ref,
}

#[derive(Clone)]
pub struct Paint {
    pub kind: PaintKind,
    pub rgba: [f64; 4],
    pub reference: Option<Vec<u8>>,
    pub have_fallback: bool,
    pub fallback: [f64; 4],
}

impl Paint {
    fn new(kind: PaintKind) -> Paint {
        Paint {
            kind,
            rgba: [0.0; 4],
            reference: None,
            have_fallback: false,
            fallback: [0.0; 4],
        }
    }

    pub fn set_color(&mut self, rgba: [f64; 4]) {
        self.reference = None;
        self.kind = PaintKind::Color;
        self.rgba = rgba;
    }

    fn set_none(&mut self) {
        self.reference = None;
        self.kind = PaintKind::None;
    }
}

pub fn unit_rgba(c: [u8; 4]) -> [f64; 4] {
    c.map(|v| f64::from(v) / 255.0)
}

fn suffix(s: &CStr, pos: usize) -> &CStr {
    CStr::from_bytes_with_nul(&s.to_bytes_with_nul()[pos..]).unwrap_or(c"")
}

fn skip_ws(b: &[u8], mut pos: usize) -> usize {
    while pos < b.len() && is_ws(b[pos]) {
        pos += 1;
    }
    pos
}

fn starts_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub fn parse_paint(text: &CStr, color: [f64; 4], out: &mut Paint) -> bool {
    let b = text.to_bytes();
    let start = skip_ws(b, 0);
    if start == b.len() {
        return false;
    }
    let t = &b[start..];
    if starts_ci(t, b"url(") {
        let Some((id, rest)) = parse::url_id(t) else {
            return false;
        };
        out.reference = Some(id);
        out.kind = PaintKind::Ref;
        out.have_fallback = false;
        let rest = skip_ws(b, start + rest);
        if rest < b.len() {
            let r = &b[rest..];
            if starts_ci(r, b"none") {
                out.have_fallback = true;
                out.fallback[3] = 0.0;
            } else if starts_ci(r, b"currentcolor") {
                out.have_fallback = true;
                out.fallback = color;
            } else if let Some(c) = parse_color(suffix(text, rest)) {
                out.have_fallback = true;
                out.fallback = unit_rgba(c);
            }
        }
        return true;
    }
    if eq_ci(t, b"none") {
        out.set_none();
        return true;
    }
    if eq_ci(t, b"currentcolor") {
        out.set_color(color);
        return true;
    }
    if let Some(c) = parse_color(suffix(text, start)) {
        out.set_color(unit_rgba(c));
        return true;
    }
    false
}

#[derive(Clone)]
pub struct State {
    pub fill: Paint,
    pub stroke: Paint,
    pub fill_opacity: f64,
    pub stroke_opacity: f64,
    pub stroke_width: f64,
    pub dash_offset: f64,
    pub miter_limit: f64,
    pub line_cap: i32,
    pub line_join: i32,
    pub fill_rule: i32,
    pub clip_rule: i32,
    pub dashes: Option<Vec<f64>>,
    pub color: [f64; 4],
    pub font_family: Option<CString>,
    pub font_size: f64,
    pub font_weight: i32,
    pub font_italic: bool,
    pub text_anchor: i32,
    pub hidden: bool,
    pub stroke_first: bool,
    pub non_scaling_stroke: bool,
}

impl Default for State {
    fn default() -> State {
        let mut fill = Paint::new(PaintKind::Color);
        fill.rgba[3] = 1.0;
        State {
            fill,
            stroke: Paint::new(PaintKind::None),
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            dash_offset: 0.0,
            miter_limit: 4.0,
            line_cap: CAP_BUTT,
            line_join: JOIN_MITER,
            fill_rule: FILL_WINDING,
            clip_rule: FILL_WINDING,
            dashes: None,
            color: [0.0, 0.0, 0.0, 1.0],
            font_family: None,
            font_size: 16.0,
            font_weight: 400,
            font_italic: false,
            text_anchor: 0,
            hidden: false,
            stroke_first: false,
            non_scaling_stroke: false,
        }
    }
}

impl State {
    pub fn inherited(inherited: Option<StyleRef<'_>>) -> State {
        let mut st = State::default();
        let Some(s) = inherited else {
            return st;
        };
        if let Some(Value::Color(c)) = s.value(&COLOR).map(|v| v.get()) {
            st.color = unit_rgba(c);
        }
        if let Some(v) = s.value(&FONT_SIZE) {
            st.font_size = cmax(1.0, v.length_or(16.0));
        }
        st
    }
}

fn cap_of(s: &[u8]) -> i32 {
    if eq_ci(s, b"round") {
        CAP_ROUND
    } else if eq_ci(s, b"square") {
        CAP_SQUARE
    } else {
        CAP_BUTT
    }
}

fn join_of(s: &[u8]) -> i32 {
    if eq_ci(s, b"round") {
        JOIN_ROUND
    } else if eq_ci(s, b"bevel") {
        JOIN_BEVEL
    } else {
        JOIN_MITER
    }
}

fn anchor_of(s: &[u8]) -> i32 {
    if eq_ci(s, b"middle") {
        1
    } else if eq_ci(s, b"end") {
        2
    } else {
        0
    }
}

fn rule_of(s: &[u8]) -> i32 {
    if eq_ci(s, b"evenodd") {
        FILL_EVEN_ODD
    } else {
        FILL_WINDING
    }
}

fn hidden_of(s: &[u8]) -> bool {
    eq_ci(s, b"hidden") || eq_ci(s, b"collapse")
}

fn style_decl(n: Node<'_>, prop: &CStr) -> Option<CString> {
    let style = n.attr(c"style")?.to_bytes();
    let prop = prop.to_bytes();
    let mut p = 0;
    let at = |i: usize| style.get(i).copied().unwrap_or(0);
    let mut found: Option<(usize, usize)> = None;
    while at(p) != 0 {
        while at(p) != 0 && (is_ws(at(p)) || at(p) == b';') {
            p += 1;
        }
        let name = p;
        while at(p) != 0 && at(p) != b':' && at(p) != b';' {
            p += 1;
        }
        if at(p) != b':' {
            while at(p) != 0 && at(p) != b';' {
                p += 1;
            }
            continue;
        }
        let mut name_end = p;
        while name_end > name && is_ws(style[name_end - 1]) {
            name_end -= 1;
        }
        p += 1;
        while at(p) != 0 && is_ws(at(p)) {
            p += 1;
        }
        let val = p;
        while at(p) != 0 && at(p) != b';' {
            p += 1;
        }
        let mut val_end = p;
        while val_end > val && is_ws(style[val_end - 1]) {
            val_end -= 1;
        }
        if style[name..name_end].eq_ignore_ascii_case(prop) {
            found = Some((val, val_end));
        }
    }
    let (val, end) = found?;
    let len = end - val;
    if len == 0 || len >= DECL_MAX {
        return None;
    }
    CString::new(&style[val..end]).ok()
}

pub fn css_number(s: StyleRef<'_>, prop: &Prop, basis: f64, fallback: f64) -> f64 {
    let Some(v) = s.value(prop) else {
        return fallback;
    };
    match v.get() {
        Value::Length(len, UNIT_PERCENT) => len / 100.0 * basis,
        Value::Length(len, _) => len,
        _ => v.length_or(fallback),
    }
}

impl<'a> Ctx<'a> {
    pub fn style(&self, n: Node<'a>) -> Option<StyleRef<'a>> {
        self.styles.get(n)
    }

    pub fn prop(&self, n: Node<'a>, name: &CStr) -> Option<Cow<'a, CStr>> {
        let v = match style_decl(n, name) {
            Some(decl) => Cow::Owned(decl),
            None => Cow::Borrowed(n.attr(name)?),
        };
        if !v.to_bytes().windows(4).any(|w| w == b"var(") {
            return Some(v);
        }
        let Some(st) = self.style(n) else {
            return Some(v);
        };
        let Some(resolved) = st.resolve_vars(&v) else {
            return Some(v);
        };
        if resolved.to_bytes().len() >= DECL_MAX {
            return Some(v);
        }
        Some(Cow::Owned(resolved.to_owned()))
    }

    pub fn apply_node(&self, st: &mut State, n: Node<'a>) {
        let s = self.style(n);
        let basis = diag_basis(self.vw, self.vh);
        let inherited_font_size = st.font_size;
        let mut font_size_from_attr = false;

        match s.and_then(|s| s.value(&COLOR)).map(|v| v.get()) {
            Some(Value::Color(c)) => st.color = unit_rgba(c),
            _ => {
                if let Some(c) = self.prop(n, c"color").and_then(|c| parse_color(&c)) {
                    st.color = unit_rgba(c);
                }
            }
        }

        let fs = |st: &State| st.font_size;
        if let Some(v) = self.prop(n, c"fill") {
            parse_paint(&v, st.color, &mut st.fill);
        }
        if let Some(v) = self.prop(n, c"stroke") {
            parse_paint(&v, st.color, &mut st.stroke);
        }
        if let Some(v) = self.prop(n, c"fill-opacity") {
            st.fill_opacity = clamp(parse::length(Some(&v), 1.0, fs(st), 1.0), 0.0, 1.0);
        }
        if let Some(v) = self.prop(n, c"stroke-opacity") {
            st.stroke_opacity = clamp(parse::length(Some(&v), 1.0, fs(st), 1.0), 0.0, 1.0);
        }
        if let Some(v) = self.prop(n, c"stroke-width") {
            st.stroke_width = cmax(0.0, parse::length(Some(&v), basis, fs(st), 1.0));
        }
        if let Some(v) = self.prop(n, c"stroke-linecap") {
            st.line_cap = cap_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"stroke-linejoin") {
            st.line_join = join_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"stroke-miterlimit") {
            st.miter_limit = cmax(1.0, parse::length(Some(&v), 1.0, fs(st), 4.0));
        }
        if let Some(v) = self.prop(n, c"stroke-dashoffset") {
            st.dash_offset = parse::length(Some(&v), basis, fs(st), 0.0);
        }
        if let Some(v) = self.prop(n, c"stroke-dasharray") {
            st.dashes = parse::dashes(&v);
        }
        if let Some(v) = self.prop(n, c"fill-rule") {
            st.fill_rule = rule_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"clip-rule") {
            st.clip_rule = rule_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"text-anchor") {
            st.text_anchor = anchor_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"font-size") {
            st.font_size = cmax(
                0.0,
                parse::length(Some(&v), inherited_font_size, inherited_font_size, 16.0),
            );
            font_size_from_attr = true;
        }
        if let Some(v) = self.prop(n, c"font-family") {
            st.font_family = Some(v.into_owned());
        }
        if let Some(v) = self.prop(n, c"font-weight") {
            let v = v.to_bytes();
            if eq_ci(v, b"bold") {
                st.font_weight = 700;
            } else if eq_ci(v, b"normal") {
                st.font_weight = 400;
            } else {
                let w = parse::atoi(v);
                if (1..=1000).contains(&w) {
                    st.font_weight = w as i32;
                }
            }
        }
        if let Some(v) = self.prop(n, c"font-style") {
            st.font_italic = !eq_ci(v.to_bytes(), b"normal");
        }
        if let Some(v) = self.prop(n, c"paint-order") {
            st.stroke_first = starts_ci(v.to_bytes(), b"stroke");
        }
        if let Some(v) = self.prop(n, c"visibility") {
            st.hidden = hidden_of(v.to_bytes());
        }
        if let Some(v) = self.prop(n, c"vector-effect") {
            st.non_scaling_stroke = eq_ci(v.to_bytes(), b"non-scaling-stroke");
        }

        let Some(s) = s else {
            return;
        };
        apply_css_paint(st, s, &FILL, true);
        apply_css_paint(st, s, &STROKE, false);
        if s.value(&FILL_OPACITY).is_some() {
            st.fill_opacity = clamp(css_number(s, &FILL_OPACITY, 1.0, 1.0), 0.0, 1.0);
        }
        if s.value(&STROKE_OPACITY).is_some() {
            st.stroke_opacity = clamp(css_number(s, &STROKE_OPACITY, 1.0, 1.0), 0.0, 1.0);
        }
        if s.value(&STROKE_WIDTH).is_some() {
            st.stroke_width = cmax(0.0, css_number(s, &STROKE_WIDTH, basis, 1.0));
        }
        if s.value(&STROKE_MITERLIMIT).is_some() {
            st.miter_limit = cmax(1.0, css_number(s, &STROKE_MITERLIMIT, 1.0, 4.0));
        }
        if s.value(&STROKE_DASHOFFSET).is_some() {
            st.dash_offset = css_number(s, &STROKE_DASHOFFSET, basis, 0.0);
        }
        if let Some(kw) = s.keyword(&STROKE_LINECAP) {
            st.line_cap = cap_of(kw.to_bytes());
        }
        if let Some(kw) = s.keyword(&STROKE_LINEJOIN) {
            st.line_join = join_of(kw.to_bytes());
        }
        if let Some(kw) = s.keyword(&FILL_RULE) {
            st.fill_rule = rule_of(kw.to_bytes());
        }
        if let Some(kw) = s.keyword(&CLIP_RULE) {
            st.clip_rule = rule_of(kw.to_bytes());
        }
        if let Some(kw) = s.keyword(&TEXT_ANCHOR) {
            st.text_anchor = anchor_of(kw.to_bytes());
        }
        if let Some(kw) = s.keyword(&PAINT_ORDER) {
            st.stroke_first = starts_ci(kw.to_bytes(), b"stroke");
        }
        if let Some(kw) = s.keyword(&STROKE_DASHARRAY) {
            st.dashes = parse::dashes(kw);
        }
        if let Some(kw) = s.keyword(&VECTOR_EFFECT) {
            st.non_scaling_stroke = eq_ci(kw.to_bytes(), b"non-scaling-stroke");
        }
        if let Some(kw) = s.keyword(&VISIBILITY) {
            st.hidden = hidden_of(kw.to_bytes());
        }
        if s.value(&FONT_SIZE).is_some() {
            let css_font_size = cmax(
                0.0,
                css_number(s, &FONT_SIZE, inherited_font_size, inherited_font_size),
            );
            if !font_size_from_attr || (css_font_size - inherited_font_size).abs() > 1e-6 {
                st.font_size = css_font_size;
            }
        }
        if let Some(v) = s.value(&FONT_WEIGHT) {
            st.font_weight = v.font_weight_or(st.font_weight);
        }
        if let Some(kw) = s.keyword(&FONT_STYLE) {
            st.font_italic = !eq_ci(kw.to_bytes(), b"normal");
        }
        if let Some(Value::Keyword(Some(family))) = s.value(&FONT_FAMILY).map(|v| v.get()) {
            st.font_family = Some(family.to_owned());
        }
    }
}

fn apply_css_paint(st: &mut State, s: StyleRef<'_>, prop: &Prop, fill: bool) {
    let Some(v) = s.value(prop) else {
        return;
    };
    let color = st.color;
    let out = if fill { &mut st.fill } else { &mut st.stroke };
    match v.get() {
        Value::Color(c) => out.set_color(unit_rgba(c)),
        Value::Keyword(Some(kw)) => {
            parse_paint(kw, color, out);
        }
        _ => {}
    }
}
