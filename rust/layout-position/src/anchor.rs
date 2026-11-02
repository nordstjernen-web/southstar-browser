//! Southstar — anchor positioning: finding a positioned box's anchors, resolving anchor() and anchor-size(), position-area cells and anchor-center, and the position-try fallback candidates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::collections::HashMap;

use southstar_css::anchor::{
    self as a, AnchorFn, AnchorSizeFn, AreaAxis, AreaKeyword, AreaSpan, Context, Expr, Resolver,
    Side, SizeKeyword,
};
use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

use crate::ffi::{Area, viewport_h, viewport_w};
use crate::static_pos::{BoxMap, NodeOrder};

const TOP: usize = 0;
const RIGHT: usize = 1;
const BOTTOM: usize = 2;
const LEFT: usize = 3;

const INSETS: [PropId; 4] = [PropId::Top, PropId::Right, PropId::Bottom, PropId::Left];
const MARGINS: [PropId; 4] = [
    PropId::MarginTop,
    PropId::MarginRight,
    PropId::MarginBottom,
    PropId::MarginLeft,
];

const ALIGNMENTS: [&CStr; 12] = [
    c"normal",
    c"start",
    c"end",
    c"center",
    c"stretch",
    c"self-start",
    c"self-end",
    c"flex-start",
    c"flex-end",
    c"left",
    c"right",
    c"anchor-center",
];

#[derive(Clone)]
struct Inputs {
    inset: [Vec<u8>; 4],
    margin: [Vec<u8>; 4],
    size: [Vec<u8>; 2],
    size_changed: [bool; 2],
    area: Vec<u8>,
    justify: Vec<u8>,
    align: Vec<u8>,
    anchor: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct Plan {
    pub inset: [Option<f64>; 4],
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub area: Area,
    pub justify: Option<&'static CStr>,
    pub align: Option<&'static CStr>,
    pub margin: [Option<f64>; 4],
    pub centre: [Option<f64>; 2],
    pub zeroed: [bool; 4],
}

impl Plan {
    pub fn flags(&self, fallback: bool) -> u8 {
        let mut flags = if fallback { a::USED_FALLBACK } else { 0 };
        for (k, zeroed) in self.zeroed.iter().enumerate() {
            if *zeroed {
                flags |= a::ZEROED_INSETS[k];
            }
        }
        flags
    }

    pub fn imcb(&self) -> Area {
        let x0 = self.area.x + self.inset[LEFT].unwrap_or(0.0);
        let x1 = self.area.x + self.area.w - self.inset[RIGHT].unwrap_or(0.0);
        let y0 = self.area.y + self.inset[TOP].unwrap_or(0.0);
        let y1 = self.area.y + self.area.h - self.inset[BOTTOM].unwrap_or(0.0);
        Area {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0.0),
            h: (y1 - y0).max(0.0),
        }
    }
}

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

pub struct Anchors<'a> {
    names: Option<HashMap<Vec<u8>, Vec<Node<'a>>>>,
}

impl<'a> Anchors<'a> {
    pub fn new() -> Anchors<'a> {
        Anchors { names: None }
    }

    fn names(&mut self, root: Node<'a>, styles: StyleTable) -> &HashMap<Vec<u8>, Vec<Node<'a>>> {
        self.names.get_or_insert_with(|| {
            let mut names: HashMap<Vec<u8>, Vec<Node<'a>>> = HashMap::new();
            let mut stack = vec![root];
            while let Some(n) = stack.pop() {
                if n.is_element()
                    && let Some(kw) = styles.get(n).and_then(|s| s.keyword_of(PropId::AnchorName))
                    && kw.to_bytes() != b"none"
                {
                    for name in kw.to_bytes().split(|&c| c == b',') {
                        names.entry(name.trim_ascii().to_vec()).or_default().push(n);
                    }
                }
                let mut kids = Vec::new();
                let mut c = n.first_child();
                while let Some(child) = c {
                    kids.push(child);
                    c = child.next_sibling();
                }
                stack.extend(kids.into_iter().rev());
            }
            names
        })
    }
}

pub struct Env<'e, 'a> {
    pub root: Node<'a>,
    pub target: Node<'a>,
    pub pseudo: bool,
    pub cb_node: Option<Node<'a>>,
    pub styles: StyleTable<'e>,
    pub map: &'e BoxMap<'a>,
    pub order: &'e NodeOrder,
    pub anchors: &'e mut Anchors<'a>,
    pub cb: Area,
    pub font_px: f64,
}

fn until_nul(mut v: Vec<u8>) -> Vec<u8> {
    if let Some(p) = v.iter().position(|&c| c == 0) {
        v.truncate(p);
    }
    v
}

fn text(style: StyleRef<'_>, prop: PropId, initial: &[u8]) -> Vec<u8> {
    style
        .get(prop)
        .map(|v| until_nul(southstar_css::value_serialize(v.data())))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| initial.to_vec())
}

fn is_auto(t: &[u8]) -> bool {
    t.trim_ascii().eq_ignore_ascii_case(b"auto")
}

fn mentions(t: &[u8]) -> bool {
    a::mentions_anchor_fn(t)
}

pub fn uses_anchoring(style: Option<StyleRef<'_>>) -> bool {
    let Some(s) = style else { return false };
    let kw = |p: PropId| s.keyword_of(p).map(CStr::to_bytes);
    let anchored_value = |p: PropId| {
        s.get(p)
            .and_then(ValueRef::keyword_text)
            .is_some_and(|k| mentions(k.to_bytes()))
    };
    kw(PropId::PositionArea).is_some_and(|k| k != b"none")
        || kw(PropId::PositionTryFallbacks).is_some_and(|k| k != b"none")
        || kw(PropId::JustifySelf).is_some_and(|k| k.ends_with(b"anchor-center"))
        || kw(PropId::AlignSelf).is_some_and(|k| k.ends_with(b"anchor-center"))
        || INSETS.into_iter().any(anchored_value)
        || [PropId::Width, PropId::Height]
            .into_iter()
            .chain(MARGINS)
            .any(anchored_value)
}

fn inputs_of(style: StyleRef<'_>) -> Inputs {
    Inputs {
        inset: INSETS.map(|p| text(style, p, b"auto")),
        margin: MARGINS.map(|p| text(style, p, b"0px")),
        size: [
            text(style, PropId::Width, b"auto"),
            text(style, PropId::Height, b"auto"),
        ],
        size_changed: [false; 2],
        area: text(style, PropId::PositionArea, b"none"),
        justify: text(style, PropId::JustifySelf, b"normal"),
        align: text(style, PropId::AlignSelf, b"normal"),
        anchor: text(style, PropId::PositionAnchor, b"normal"),
    }
}

impl<'a> Env<'_, 'a> {
    fn border_rect(&self, n: Node<'_>) -> Option<Rect> {
        let b: BoxRef<'_> = self.map.get(n)?;
        let (m, p, e) = (b.margin(), b.padding(), b.border());
        Some(Rect {
            x: b.x() + m.left,
            y: b.y() + m.top,
            w: b.content_width() + p.left + p.right + e.left + e.right,
            h: b.content_height() + p.top + p.bottom + e.top + e.bottom,
        })
    }

    fn contains(ancestor: Node<'_>, n: Node<'_>) -> bool {
        southstar_dom::ancestors_and_self(n).any(|p| core::ptr::eq(p.as_ptr(), ancestor.as_ptr()))
    }

    fn scoped_out(&self, candidate: Node<'_>, name: &[u8]) -> bool {
        for p in southstar_dom::ancestors_and_self(candidate) {
            let Some(scope) = self
                .styles
                .get(p)
                .and_then(|s| s.keyword_of(PropId::AnchorScope))
            else {
                continue;
            };
            let scope = scope.to_bytes();
            let hit = scope == b"all"
                || scope
                    .split(|&c| c == b',')
                    .any(|item| item.trim_ascii() == name);
            if hit {
                return !Self::contains(p, self.target);
            }
        }
        false
    }

    fn acceptable(&self, candidate: Node<'_>) -> bool {
        if !self.pseudo && Self::contains(self.target, candidate) {
            return false;
        }
        if let Some(cb) = self.cb_node
            && (core::ptr::eq(cb.as_ptr(), candidate.as_ptr()) || !Self::contains(cb, candidate))
        {
            return false;
        }
        let positioned = self
            .styles
            .get(candidate)
            .and_then(|s| s.keyword_of(PropId::Position))
            .is_some_and(|k| k == c"absolute" || k == c"fixed");
        if positioned && !self.order.precedes(candidate, self.target) {
            return false;
        }
        true
    }

    fn named(&mut self, name: &[u8]) -> Option<Rect> {
        let (root, styles) = (self.root, self.styles);
        let list = self.anchors.names(root, styles).get(name)?.clone();
        list.into_iter()
            .rev()
            .filter(|c| self.acceptable(*c) && !self.scoped_out(*c, name))
            .find_map(|c| self.border_rect(c))
    }

    fn default_anchor(&mut self, inputs: &Inputs) -> Option<Rect> {
        let anchor = inputs.anchor.trim_ascii();
        if anchor.starts_with(b"--") {
            return self.named(anchor);
        }
        if anchor == b"none" {
            return None;
        }
        if self.pseudo {
            return self.border_rect(self.target);
        }
        let source = a::implicit_anchor(self.target)?;
        if Self::contains(self.target, source) {
            return None;
        }
        self.border_rect(source)
    }

    fn rect_for(&mut self, name: Option<&str>, default: Option<Rect>) -> Option<Rect> {
        match name {
            Some(n) => self.named(n.as_bytes()),
            None => default,
        }
    }
}

struct Values<'r> {
    rects: &'r HashMap<String, Option<Rect>>,
    default: Option<Rect>,
    area: Area,
    side: usize,
    vertical: bool,
    font_px: f64,
}

impl Values<'_> {
    fn rect(&self, name: &Option<String>) -> Option<Rect> {
        match name {
            Some(n) => self.rects.get(n).copied().flatten(),
            None => self.default,
        }
    }
}

impl Resolver for Values<'_> {
    fn anchor(&self, f: &AnchorFn) -> Option<f64> {
        let r = self.rect(&f.name)?;
        let (start, size) = if self.vertical {
            (r.y, r.h)
        } else {
            (r.x, r.w)
        };
        let end = start + size;
        let at_start = self.side == TOP || self.side == LEFT;
        let coordinate = match f.side {
            Side::Top | Side::Bottom if !self.vertical => return None,
            Side::Left | Side::Right if self.vertical => return None,
            Side::Top | Side::Left | Side::Start | Side::SelfStart => start,
            Side::Bottom | Side::Right | Side::End | Side::SelfEnd => end,
            Side::Inside => {
                if at_start {
                    start
                } else {
                    end
                }
            }
            Side::Outside => {
                if at_start {
                    end
                } else {
                    start
                }
            }
            Side::Center => start + size / 2.0,
            Side::Percent(p, _) => start + size * p / 100.0,
        };
        let (origin, extent) = if self.vertical {
            (self.area.y, self.area.h)
        } else {
            (self.area.x, self.area.w)
        };
        Some(if at_start {
            coordinate - origin
        } else {
            origin + extent - coordinate
        })
    }

    fn anchor_size(&self, f: &AnchorSizeFn) -> Option<f64> {
        let r = self.rect(&f.name)?;
        Some(match f.size {
            Some(SizeKeyword::Width | SizeKeyword::Inline | SizeKeyword::SelfInline) => r.w,
            Some(SizeKeyword::Height | SizeKeyword::Block | SizeKeyword::SelfBlock) => r.h,
            None if self.vertical => r.h,
            None => r.w,
        })
    }

    fn length(&self, value: f64, unit: &str) -> f64 {
        let basis = if self.vertical {
            self.area.h
        } else {
            self.area.w
        };
        let (vw, vh) = (viewport_w(), viewport_h());
        let em = self.font_px;
        value
            * match unit {
                "%" => basis / 100.0,
                "em" => em,
                "rem" | "rlh" => 16.0,
                "ex" | "ch" | "rex" | "rch" | "cap" | "rcap" => em / 2.0,
                "ic" | "ric" => em,
                "lh" => em * 1.2,
                "vw" | "svw" | "lvw" | "dvw" | "vi" | "svi" | "lvi" | "dvi" | "cqw" | "cqi" => {
                    vw / 100.0
                }
                "vh" | "svh" | "lvh" | "dvh" | "vb" | "svb" | "lvb" | "dvb" | "cqh" | "cqb" => {
                    vh / 100.0
                }
                "vmin" | "svmin" | "lvmin" | "dvmin" | "cqmin" => vw.min(vh) / 100.0,
                "vmax" | "svmax" | "lvmax" | "dvmax" | "cqmax" => vw.max(vh) / 100.0,
                "cm" => 96.0 / 2.54,
                "mm" => 96.0 / 25.4,
                "q" => 96.0 / 101.6,
                "in" => 96.0,
                "pt" => 96.0 / 72.0,
                "pc" => 16.0,
                _ => 1.0,
            }
    }
}

fn collect_names(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Anchor(f) => {
            out.extend(f.name.clone());
            if let Some(fb) = &f.fallback {
                collect_names(fb, out);
            }
        }
        Expr::AnchorSize(f) => {
            out.extend(f.name.clone());
            if let Some(fb) = &f.fallback {
                collect_names(fb, out);
            }
        }
        Expr::Sum(t) | Expr::Math(_, t) => t.iter().for_each(|x| collect_names(x, out)),
        Expr::Product(_, x) => collect_names(x, out),
        _ => {}
    }
}

fn parse_any(t: &[u8], ctx: Context) -> Option<Expr> {
    if mentions(t) {
        a::parse(t, ctx)
    } else {
        a::parse_length(t, ctx)
    }
}

fn area_keywords(t: &[u8]) -> Option<a::Area> {
    if t.trim_ascii() == b"none" {
        return None;
    }
    a::position_area(t)
}

fn first_slot_vertical(area: &a::Area) -> bool {
    match area.first.0.axis {
        AreaAxis::X => false,
        AreaAxis::Block | AreaAxis::SelfBlock | AreaAxis::Either | AreaAxis::SelfEither => true,
        AreaAxis::Y => true,
        AreaAxis::Inline | AreaAxis::SelfInline => false,
        AreaAxis::Both => !matches!(area.second.0.axis, AreaAxis::Y),
    }
}

fn track(kw: AreaKeyword, c0: f64, c1: f64, a0: f64, a1: f64) -> (f64, f64, &'static CStr) {
    let (lo, hi, align) = match kw.span {
        AreaSpan::Start => (c0, a0, c"end"),
        AreaSpan::End => (a1, c1, c"start"),
        AreaSpan::Center => (a0, a1, c"center"),
        AreaSpan::SpanStart => (c0, a1, c"end"),
        AreaSpan::SpanEnd => (a0, c1, c"start"),
        AreaSpan::All => (c0, c1, c"anchor-center"),
    };
    (lo, hi.max(lo), align)
}

fn area_cell(area: &a::Area, cb: Area, anchor: Rect) -> (Area, &'static CStr, &'static CStr) {
    let vertical_first = first_slot_vertical(area);
    let (v_kw, h_kw) = if vertical_first {
        (area.first.0, area.second.0)
    } else {
        (area.second.0, area.first.0)
    };
    let (x0, x1, justify) = track(h_kw, cb.x, cb.x + cb.w, anchor.x, anchor.x + anchor.w);
    let (y0, y1, align) = track(v_kw, cb.y, cb.y + cb.h, anchor.y, anchor.y + anchor.h);
    (
        Area {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        },
        justify,
        align,
    )
}

fn alignment(t: &[u8]) -> Option<&'static CStr> {
    let t = t.trim_ascii();
    let t = t
        .strip_prefix(b"unsafe ")
        .or_else(|| t.strip_prefix(b"safe "))
        .unwrap_or(t);
    ALIGNMENTS.iter().copied().find(|k| k.to_bytes() == t)
}

fn resolve(env: &mut Env<'_, '_>, inputs: &Inputs, base_margin: &[Vec<u8>; 4]) -> Plan {
    let default = env.default_anchor(inputs);
    let mut names = Vec::new();
    let mut exprs: Vec<Option<Expr>> = Vec::new();
    for t in &inputs.inset {
        let e = if is_auto(t) {
            None
        } else {
            parse_any(t, Context::Inset)
        };
        if let Some(e) = &e {
            collect_names(e, &mut names);
        }
        exprs.push(e);
    }
    let sizes: Vec<Option<Expr>> = inputs
        .size
        .iter()
        .zip(inputs.size_changed)
        .map(|(t, changed)| {
            (mentions(t) || (changed && !is_auto(t)))
                .then(|| parse_any(t, Context::Size))
                .flatten()
        })
        .collect();
    for e in sizes.iter().flatten() {
        collect_names(e, &mut names);
    }
    let margins: Vec<Option<Expr>> = inputs
        .margin
        .iter()
        .zip(base_margin)
        .map(|(t, base)| {
            (mentions(t) || (t != base && !is_auto(t)))
                .then(|| parse_any(t, Context::Margin))
                .flatten()
        })
        .collect();
    for e in margins.iter().flatten() {
        collect_names(e, &mut names);
    }
    let mut rects = HashMap::new();
    for n in names {
        if let std::collections::hash_map::Entry::Vacant(slot) = rects.entry(n) {
            let r = env.rect_for(Some(slot.key().as_str()), default);
            slot.insert(r);
        }
    }
    let mut area = env.cb;
    let mut justify = alignment(&inputs.justify);
    let mut align = alignment(&inputs.align);
    let normal = |k: Option<&CStr>| k.is_none_or(|k| k == c"normal");
    if let (Some(keywords), Some(anchor)) = (area_keywords(&inputs.area), default) {
        let (cell, j, al) = area_cell(&keywords, env.cb, anchor);
        area = cell;
        if normal(justify) {
            justify = Some(j);
        }
        if normal(align) {
            align = Some(al);
        }
    }
    let values = |side: usize, area: Area| Values {
        rects: &rects,
        default,
        area,
        side,
        vertical: side == TOP || side == BOTTOM,
        font_px: env.font_px,
    };
    let mut inset = [None; 4];
    for (side, e) in exprs.iter().enumerate() {
        inset[side] = e.as_ref().and_then(|e| a::evaluate(e, &values(side, area)));
    }
    let declared = inset.map(|v| v.is_some());
    if area_keywords(&inputs.area).is_some() && default.is_some() {
        for v in &mut inset {
            v.get_or_insert(0.0);
        }
    }
    let mut centre = [None; 2];
    if let Some(anchor) = default {
        if justify == Some(c"anchor-center") {
            centre[0] = Some(anchor.x + anchor.w / 2.0);
            inset[LEFT].get_or_insert(0.0);
            inset[RIGHT].get_or_insert(0.0);
        }
        if align == Some(c"anchor-center") {
            centre[1] = Some(anchor.y + anchor.h / 2.0);
            inset[TOP].get_or_insert(0.0);
            inset[BOTTOM].get_or_insert(0.0);
        }
    }
    let to_centre = |k: Option<&'static CStr>| {
        if k == Some(c"anchor-center") {
            Some(c"center")
        } else {
            k
        }
    };
    let width = sizes[0]
        .as_ref()
        .and_then(|e| a::evaluate(e, &values(LEFT, area)));
    let height = sizes[1]
        .as_ref()
        .and_then(|e| a::evaluate(e, &values(TOP, area)));
    let margin_basis = Area {
        h: env.cb.w,
        ..env.cb
    };
    let mut margin = [None; 4];
    for (side, e) in margins.iter().enumerate() {
        margin[side] = e
            .as_ref()
            .and_then(|e| a::evaluate(e, &values(side, margin_basis)));
    }
    Plan {
        inset,
        width,
        height,
        area,
        justify: to_centre(justify),
        align: to_centre(align),
        margin,
        centre,
        zeroed: core::array::from_fn(|k| !declared[k] && inset[k].is_some()),
    }
}

fn map_sides(e: &mut Expr, side: &dyn Fn(Side) -> Side, size: &dyn Fn(SizeKeyword) -> SizeKeyword) {
    match e {
        Expr::Anchor(f) => {
            f.side = side(f.side);
            if let Some(fb) = &mut f.fallback {
                map_sides(fb, side, size);
            }
        }
        Expr::AnchorSize(f) => {
            f.size = f.size.map(size);
            if let Some(fb) = &mut f.fallback {
                map_sides(fb, side, size);
            }
        }
        Expr::Sum(t) | Expr::Math(_, t) => t.iter_mut().for_each(|x| map_sides(x, side, size)),
        Expr::Product(_, x) => map_sides(x, side, size),
        _ => {}
    }
}

fn remap(
    t: &mut Vec<u8>,
    ctx: Context,
    side: &dyn Fn(Side) -> Side,
    size: &dyn Fn(SizeKeyword) -> SizeKeyword,
) {
    if !mentions(t) {
        return;
    }
    if let Some(mut e) = a::parse(t, ctx) {
        map_sides(&mut e, side, size);
        *t = a::serialize(&e).into_bytes();
    }
}

fn flip_side_axis(s: Side, vertical: bool) -> Side {
    match s {
        Side::Top if vertical => Side::Bottom,
        Side::Bottom if vertical => Side::Top,
        Side::Left if !vertical => Side::Right,
        Side::Right if !vertical => Side::Left,
        Side::Start => Side::End,
        Side::End => Side::Start,
        Side::SelfStart => Side::SelfEnd,
        Side::SelfEnd => Side::SelfStart,
        Side::Percent(p, calc) => Side::Percent(100.0 - p, calc),
        s => s,
    }
}

fn transpose_side(s: Side) -> Side {
    match s {
        Side::Top => Side::Left,
        Side::Left => Side::Top,
        Side::Bottom => Side::Right,
        Side::Right => Side::Bottom,
        s => s,
    }
}

fn transpose_size(s: SizeKeyword) -> SizeKeyword {
    match s {
        SizeKeyword::Width => SizeKeyword::Height,
        SizeKeyword::Height => SizeKeyword::Width,
        SizeKeyword::Block => SizeKeyword::Inline,
        SizeKeyword::Inline => SizeKeyword::Block,
        SizeKeyword::SelfBlock => SizeKeyword::SelfInline,
        SizeKeyword::SelfInline => SizeKeyword::SelfBlock,
    }
}

fn swap_words(word: &str, pairs: &[(&str, &str)]) -> String {
    let mut out = word.to_owned();
    for (x, y) in pairs {
        if out.contains(x) {
            return out.replace(x, y);
        }
        if out.contains(y) {
            out = out.replace(y, x);
            return out;
        }
    }
    out
}

fn flip_area(t: &[u8], vertical: bool) -> Vec<u8> {
    let Some(area) = area_keywords(t) else {
        return t.to_vec();
    };
    let first_vertical = first_slot_vertical(&area);
    let flip = |w: &str| swap_words(w, &[("top", "bottom"), ("left", "right"), ("start", "end")]);
    let (mut f, mut s) = (area.first.1.clone(), area.second.1.clone());
    if first_vertical == vertical {
        f = flip(&f);
    } else {
        s = flip(&s);
    }
    let text = format!("{f} {s}");
    a::position_area_computed(text.as_bytes()).map_or_else(|| t.to_vec(), String::into_bytes)
}

fn transpose_area(t: &[u8]) -> Vec<u8> {
    let Some(area) = area_keywords(t) else {
        return t.to_vec();
    };
    let swap = |w: &str| {
        swap_words(
            w,
            &[
                ("left", "top"),
                ("right", "bottom"),
                ("x-", "y-"),
                ("block", "inline"),
            ],
        )
    };
    let text = format!("{} {}", swap(&area.second.1), swap(&area.first.1));
    a::position_area_computed(text.as_bytes()).map_or_else(|| t.to_vec(), String::into_bytes)
}

fn flip_alignment(t: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(t);
    swap_words(&s, &[("left", "right"), ("start", "end")]).into_bytes()
}

fn flip_block(i: &mut Inputs) {
    i.inset.swap(TOP, BOTTOM);
    i.margin.swap(TOP, BOTTOM);
    let side = |s| flip_side_axis(s, true);
    let size = |s| s;
    remap(&mut i.inset[TOP], Context::Inset, &side, &size);
    remap(&mut i.inset[BOTTOM], Context::Inset, &side, &size);
    i.area = flip_area(&i.area, true);
    i.align = flip_alignment(&i.align);
}

fn flip_inline(i: &mut Inputs) {
    i.inset.swap(LEFT, RIGHT);
    i.margin.swap(LEFT, RIGHT);
    let side = |s| flip_side_axis(s, false);
    let size = |s| s;
    remap(&mut i.inset[LEFT], Context::Inset, &side, &size);
    remap(&mut i.inset[RIGHT], Context::Inset, &side, &size);
    i.area = flip_area(&i.area, false);
    i.justify = flip_alignment(&i.justify);
}

fn flip_start(i: &mut Inputs) {
    i.inset.swap(TOP, LEFT);
    i.inset.swap(BOTTOM, RIGHT);
    i.margin.swap(TOP, LEFT);
    i.margin.swap(BOTTOM, RIGHT);
    i.size.swap(0, 1);
    i.size_changed = [true; 2];
    for k in 0..4 {
        remap(
            &mut i.inset[k],
            Context::Inset,
            &transpose_side,
            &transpose_size,
        );
    }
    for k in 0..2 {
        remap(
            &mut i.size[k],
            Context::Size,
            &transpose_side,
            &transpose_size,
        );
    }
    i.area = transpose_area(&i.area);
    core::mem::swap(&mut i.justify, &mut i.align);
}

fn initial_text(prop: PropId) -> &'static [u8] {
    match prop {
        PropId::MarginTop | PropId::MarginRight | PropId::MarginBottom | PropId::MarginLeft => {
            b"0px"
        }
        PropId::PositionArea => b"none",
        PropId::JustifySelf | PropId::AlignSelf | PropId::PositionAnchor => b"normal",
        _ => b"auto",
    }
}

fn apply_rule(i: &mut Inputs, decls: &[(PropId, Vec<u8>)]) {
    for (prop, value) in decls {
        let v = match value.trim_ascii().to_ascii_lowercase().as_slice() {
            b"revert" | b"revert-layer" => continue,
            b"initial" | b"inherit" | b"unset" => initial_text(*prop).to_vec(),
            _ => value.clone(),
        };
        if let Some(k) = INSETS.iter().position(|p| p == prop) {
            i.inset[k] = v;
        } else if let Some(k) = MARGINS.iter().position(|p| p == prop) {
            i.margin[k] = v;
        } else {
            match prop {
                PropId::Width => {
                    i.size[0] = v;
                    i.size_changed[0] = true;
                }
                PropId::Height => {
                    i.size[1] = v;
                    i.size_changed[1] = true;
                }
                PropId::PositionArea => i.area = v,
                PropId::JustifySelf => i.justify = v,
                PropId::AlignSelf => i.align = v,
                PropId::PositionAnchor => i.anchor = v,
                _ => {}
            }
        }
    }
}

fn fallback_inputs(base: &Inputs, item: &[u8], doc: usize) -> Option<Inputs> {
    let mut inputs = base.clone();
    if let Some(area) = a::position_area_computed(item)
        && area != "none"
    {
        inputs.area = area.into_bytes();
        return Some(inputs);
    }
    for word in item
        .split(u8::is_ascii_whitespace)
        .filter(|w| !w.is_empty())
    {
        match word {
            b"flip-block" | b"flip-y" => flip_block(&mut inputs),
            b"flip-inline" | b"flip-x" => flip_inline(&mut inputs),
            b"flip-start" => flip_start(&mut inputs),
            name => apply_rule(&mut inputs, &a::position_try_rule(doc, name)?),
        }
    }
    Some(inputs)
}

pub fn candidates(env: &mut Env<'_, '_>, style: StyleRef<'_>, doc: usize) -> Vec<Plan> {
    let base = inputs_of(style);
    let mut plans = vec![resolve(env, &base, &base.margin)];
    let fallbacks = text(style, PropId::PositionTryFallbacks, b"none");
    if fallbacks.trim_ascii() != b"none" {
        for item in fallbacks.split(|&c| c == b',') {
            if let Some(inputs) = fallback_inputs(&base, item.trim_ascii(), doc) {
                plans.push(resolve(env, &inputs, &base.margin));
            }
        }
    }
    let order = text(style, PropId::PositionTryOrder, b"normal");
    let key: Option<fn(&Plan) -> f64> = match order.as_slice() {
        b"most-width" | b"most-inline-size" => Some(|p| p.imcb().w),
        b"most-height" | b"most-block-size" => Some(|p| p.imcb().h),
        _ => None,
    };
    if let Some(key) = key {
        plans.sort_by(|x, y| key(y).total_cmp(&key(x)));
    }
    plans
}

pub fn font_px(style: Option<StyleRef<'_>>) -> f64 {
    style
        .and_then(|s| s.get(PropId::FontSize))
        .and_then(ValueRef::length)
        .map_or(16.0, |(v, _)| v)
}
