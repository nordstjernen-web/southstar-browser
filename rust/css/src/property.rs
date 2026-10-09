//! Southstar — parsing a declaration's value for one property into the value css.c stores: keywords, lengths, calc(), colours, sizes, rects, URLs and the structured values of the other sections, with layer lists for the background and mask properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::animation::{self, Entry, Longhand};
use crate::border_image;
use crate::calc::{self, Calc, Parsed};
use crate::color::parse_color;
use crate::container;
use crate::content::content_canonical;
use crate::counter::{self, CounterProp};
use crate::display;
use crate::ffi;
use crate::font;
use crate::gradient::{self, Gradient};
use crate::grid::{self, Areas, Tracks};
use crate::image;
use crate::prop::Prop;
use crate::scan::{is_gspace, is_ws, split_ws_limit, strip, strtol10, trim_range};
use crate::shadow::{self, ShadowList};
use crate::time;
use crate::transform::{self, Individual, Transform};
use crate::units::{self, CAP, CH, EM, EX, IC, NUMBER, PERCENT, PX, Unit};

#[derive(Clone, Copy, Default)]
pub(crate) struct Size {
    pub w: f64,
    pub h: f64,
    pub w_unit: Unit,
    pub h_unit: Unit,
    pub w_auto: bool,
    pub h_auto: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Rect {
    pub v: [f64; 4],
    pub unit: [Unit; 4],
    pub is_auto: [bool; 4],
}

pub(crate) enum Body {
    Keyword(Vec<u8>),
    Length(f64, Unit),
    Calc(Calc),
    Color([u8; 4]),
    Size(Size),
    Rect(Rect),
    Url(Vec<u8>),
    Shadow(Box<ShadowList>),
    Gradient(Box<Gradient>),
    Tracks(Box<Tracks>),
    Areas(Areas),
    Transform(Box<Transform>),
    Anim(Vec<Entry>),
}

pub(crate) struct Value {
    pub body: Body,
    pub image_set_text: Option<Vec<u8>>,
    pub specified: Option<Vec<u8>>,
    pub next_layer: Option<Box<Value>>,
}

impl Value {
    pub(crate) fn of(body: Body) -> Value {
        Value {
            body,
            image_set_text: None,
            specified: None,
            next_layer: None,
        }
    }

    pub(crate) fn keyword(text: impl Into<Vec<u8>>) -> Value {
        Value::of(Body::Keyword(text.into()))
    }

    pub(crate) fn length(v: f64, unit: Unit) -> Value {
        Value::of(Body::Length(v, unit))
    }

    fn from_parsed(parsed: Parsed) -> Value {
        match parsed {
            Parsed::Length(v, unit) => Value::length(v, unit),
            Parsed::Calc(c) => Value::of(Body::Calc(c)),
        }
    }
}

pub(crate) fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

pub(crate) fn lower(t: &[u8]) -> Vec<u8> {
    t.to_ascii_lowercase()
}

pub(crate) fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn length(t: &[u8]) -> Option<(f64, Unit)> {
    units::parse_length(&c_text(t))
}

pub(crate) fn calc_value(t: &[u8]) -> Option<Parsed> {
    calc::parse_calc(&c_text(t))
}

fn split_ws(t: &[u8]) -> Vec<&[u8]> {
    split_ws_limit(t, 4)
}

const CSS_WIDE: [&[u8]; 6] = [
    b"inherit",
    b"initial",
    b"unset",
    b"revert",
    b"revert-layer",
    b"revert-rule",
];

pub(crate) fn wide_keyword(t: &[u8]) -> Option<Value> {
    let start = t.iter().position(|&c| !is_ws(c)).unwrap_or(t.len());
    let kw = lower(trim_range(t, start, t.len()));
    CSS_WIDE
        .contains(&kw.as_slice())
        .then(|| Value::keyword(kw))
}

pub(crate) fn keyword_choice(text: &[u8], choices: &str) -> Option<Value> {
    let kw = lower(text);
    choices
        .split(' ')
        .filter(|c| !c.is_empty())
        .any(|c| c.as_bytes() == kw.as_slice())
        .then(|| Value::keyword(kw))
}

fn word_is_one_of(w: &[u8], choices: &str) -> bool {
    choices
        .split(' ')
        .filter(|c| !c.is_empty())
        .any(|c| eq(c.as_bytes(), w))
}

fn legacy_em_normalize(val: &mut f64, unit: &mut Unit) {
    match *unit {
        EX | CH => {
            *val *= 0.5;
            *unit = EM;
        }
        CAP => {
            *val *= 0.7;
            *unit = EM;
        }
        IC => *unit = EM,
        _ => {}
    }
}

fn bg_size_component(tok: &[u8]) -> Option<(f64, Unit)> {
    if let Some(found) = length(tok) {
        return Some(found);
    }
    match calc_value(tok)? {
        Parsed::Length(v, unit) => Some((v, unit)),
        Parsed::Calc(c) if c.pct != 0.0 && c.px == 0.0 && c.em == 0.0 && c.rem == 0.0 => {
            Some((c.pct, PERCENT))
        }
        Parsed::Calc(c) => Some((c.px + (c.em + c.rem) * 16.0, PX)),
    }
}

fn is_integer_token(s: &[u8]) -> bool {
    let digits = match s.first() {
        Some(b'+' | b'-') => &s[1..],
        _ => s,
    };
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

fn integer_property(prop: Prop, t: &[u8]) -> Option<Value> {
    let kw: Option<&[u8]> = match prop {
        Prop::ZIndex | Prop::ColumnCount => Some(b"auto"),
        Prop::MaxLines => Some(b"none"),
        Prop::HyphenateLimitLines => Some(b"no-limit"),
        _ => None,
    };
    if let Some(kw) = kw.filter(|kw| eq(t, kw)) {
        return Some(Value::keyword(kw));
    }
    if is_integer_token(t) {
        return Some(Value::length(strtol10(t).0 as f64, NUMBER));
    }
    match calc_value(t)? {
        Parsed::Length(v, NUMBER) => Some(Value::length(v.round(), NUMBER)),
        _ => None,
    }
}

fn has_top_level_comma(t: &[u8]) -> bool {
    let mut depth = 0u32;
    for &c in t {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth = depth.saturating_sub(1);
        } else if c == b',' && depth == 0 {
            return true;
        }
    }
    false
}

pub(crate) fn mask_box_keyword(t: &[u8]) -> Option<&'static [u8]> {
    const BOXES: [&[u8]; 7] = [
        b"border-box",
        b"padding-box",
        b"content-box",
        b"fill-box",
        b"stroke-box",
        b"view-box",
        b"no-clip",
    ];
    const LEGACY: [(&[u8], &[u8]); 3] = [
        (b"border", b"border-box"),
        (b"padding", b"padding-box"),
        (b"content", b"content-box"),
    ];
    BOXES
        .iter()
        .find(|b| eq(t, b))
        .copied()
        .or_else(|| LEGACY.iter().find(|(l, _)| eq(t, l)).map(|(_, b)| *b))
}

pub(crate) fn mask_composite_keyword(t: &[u8]) -> Option<&'static [u8]> {
    const OPS: [(&[u8], &[u8]); 8] = [
        (b"add", b"add"),
        (b"subtract", b"subtract"),
        (b"intersect", b"intersect"),
        (b"exclude", b"exclude"),
        (b"source-over", b"add"),
        (b"source-in", b"intersect"),
        (b"source-out", b"subtract"),
        (b"xor", b"exclude"),
    ];
    OPS.iter().find(|(name, _)| eq(t, name)).map(|(_, op)| *op)
}

fn bg_layered(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::BackgroundImage
            | Prop::BackgroundRepeat
            | Prop::BackgroundSize
            | Prop::BackgroundPositionX
            | Prop::BackgroundPositionY
            | Prop::BackgroundClip
            | Prop::BackgroundOrigin
            | Prop::BackgroundAttachment
            | Prop::MaskImage
            | Prop::MaskClip
            | Prop::MaskComposite
    )
}

fn split_gspace_words(text: &[u8]) -> Vec<&[u8]> {
    text.split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn scroll_snap_type(text: &[u8]) -> Option<Value> {
    let words = split_gspace_words(text);
    if words.len() == 1 && eq(words[0], b"none") {
        return Some(Value::keyword(b"none".to_vec()));
    }
    if !(1..=2).contains(&words.len()) || !word_is_one_of(words[0], "x y block inline both") {
        return None;
    }
    let strictness: &[u8] = words.get(1).copied().unwrap_or(b"proximity");
    if !word_is_one_of(strictness, "mandatory proximity") {
        return None;
    }
    let mut out = lower(words[0]);
    out.push(b' ');
    out.extend_from_slice(&lower(strictness));
    Some(Value::keyword(out))
}

fn scroll_snap_align(text: &[u8]) -> Option<Value> {
    let words = split_gspace_words(text);
    if !(1..=2).contains(&words.len())
        || !words
            .iter()
            .all(|w| word_is_one_of(w, "none start end center"))
    {
        return None;
    }
    let mut out = lower(words[0]);
    out.push(b' ');
    out.extend_from_slice(&lower(words[words.len() - 1]));
    Some(Value::keyword(out))
}

fn alignment_is_position_keyword(kw: &[u8]) -> bool {
    [
        &b"center"[..],
        b"start",
        b"end",
        b"self-start",
        b"self-end",
        b"flex-start",
        b"flex-end",
        b"left",
        b"right",
    ]
    .contains(&kw)
}

fn alignment_keyword(text: &[u8], choices: &str) -> Option<Value> {
    let mut kw = strip(&lower(text)).to_vec();
    for c in kw.iter_mut() {
        if is_gspace(*c) {
            *c = b' ';
        }
    }
    let tokens = split_ws(&kw);
    match tokens.as_slice() {
        [only] => keyword_choice(only, choices),
        [first, second] => {
            let full: Option<Vec<u8>> = if *second == b"baseline" && choices.contains("baseline") {
                if *first == b"first" {
                    return keyword_choice(b"baseline", choices);
                }
                (*first == b"last").then(|| b"last baseline".to_vec())
            } else if (*first == b"safe" || *first == b"unsafe")
                && alignment_is_position_keyword(second)
            {
                keyword_choice(second, choices).map(|_| [*first, b" ", *second].concat())
            } else if choices.contains("legacy") {
                let pos: Option<&[u8]> = if *first == b"legacy" {
                    Some(second)
                } else if *second == b"legacy" {
                    Some(first)
                } else {
                    None
                };
                pos.filter(|p| matches!(*p, b"left" | b"center" | b"right"))
                    .map(|p| [&b"legacy "[..], p].concat())
            } else {
                None
            };
            full.map(Value::keyword)
        }
        _ => None,
    }
}

fn accepts_auto(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::MarginTop
            | Prop::MarginRight
            | Prop::MarginBottom
            | Prop::MarginLeft
            | Prop::Width
            | Prop::Height
            | Prop::MinWidth
            | Prop::MinHeight
            | Prop::Top
            | Prop::Right
            | Prop::Bottom
            | Prop::Left
            | Prop::FlexBasis
            | Prop::ColumnWidth
    )
}

fn accepts_normal(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::LineHeight
            | Prop::LetterSpacing
            | Prop::WordSpacing
            | Prop::Gap
            | Prop::RowGap
            | Prop::ColumnGap
    )
}

fn requires_nonnegative(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::FontSize
            | Prop::PaddingTop
            | Prop::PaddingRight
            | Prop::PaddingBottom
            | Prop::PaddingLeft
            | Prop::BorderTopWidth
            | Prop::BorderRightWidth
            | Prop::BorderBottomWidth
            | Prop::BorderLeftWidth
            | Prop::Width
            | Prop::Height
            | Prop::MaxWidth
            | Prop::MaxHeight
            | Prop::MinWidth
            | Prop::MinHeight
            | Prop::BorderRadius
            | Prop::BorderTopLeftRadius
            | Prop::BorderTopRightRadius
            | Prop::BorderBottomRightRadius
            | Prop::BorderBottomLeftRadius
            | Prop::Gap
            | Prop::RowGap
            | Prop::ColumnGap
            | Prop::FlexGrow
            | Prop::FlexShrink
            | Prop::FlexBasis
            | Prop::LineHeight
            | Prop::OutlineWidth
            | Prop::ColumnWidth
            | Prop::ColumnRuleWidth
    )
}

fn bare_number_is_length(prop: Prop) -> bool {
    !matches!(
        prop,
        Prop::Opacity | Prop::FlexGrow | Prop::FlexShrink | Prop::LineHeight
    )
}

fn numeric_valid(prop: Prop, body: &Body) -> bool {
    match body {
        Body::Length(num, unit) => {
            let (num, unit) = (*num, *unit);
            if prop == Prop::Opacity {
                return unit == NUMBER || unit == PERCENT;
            }
            if prop == Prop::FlexGrow || prop == Prop::FlexShrink {
                return unit == NUMBER && num >= 0.0;
            }
            if unit == NUMBER && prop != Prop::LineHeight && num != 0.0 {
                return false;
            }
            !(requires_nonnegative(prop) && num < 0.0)
        }
        Body::Calc(c) => {
            if prop == Prop::FlexGrow || prop == Prop::FlexShrink {
                return false;
            }
            if prop == Prop::Opacity {
                return c.px == 0.0 && c.em == 0.0 && c.rem == 0.0;
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn text_is_ident(t: &[u8]) -> bool {
    !t.is_empty()
        && !t[0].is_ascii_digit()
        && t.iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn is_border_width(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::BorderTopWidth
            | Prop::BorderRightWidth
            | Prop::BorderBottomWidth
            | Prop::BorderLeftWidth
            | Prop::OutlineWidth
            | Prop::ColumnRuleWidth
    )
}

fn record_specified_keyword(v: &mut Value, prop: Prop, t: &[u8]) {
    match v.body {
        Body::Color(_) => {
            if text_is_ident(t) {
                v.specified = Some(lower(t));
            }
        }
        Body::Length(..)
            if is_border_width(prop) && (eq(t, b"thin") || eq(t, b"medium") || eq(t, b"thick")) =>
        {
            v.specified = Some(lower(t));
        }
        _ => {}
    }
}

fn layer_list(prop: Prop, t: &[u8]) -> Option<Value> {
    let mut layers = Vec::new();
    let mut depth = 0u32;
    let mut seg = 0;
    for p in 0..=t.len() {
        let c = t.get(p).copied().unwrap_or(0);
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth = depth.saturating_sub(1);
        }
        if (c == b',' && depth == 0) || p == t.len() {
            layers.push(parse(prop, &t[seg..p])?);
            seg = p + 1;
        }
    }
    let mut head: Option<Value> = None;
    while let Some(mut layer) = layers.pop() {
        layer.next_layer = head.map(Box::new);
        head = Some(layer);
    }
    head
}

fn image_reference(t: &[u8]) -> Option<Value> {
    if let Some(gr) = gradient::parse_value(&c_text(t)) {
        return Some(Value::of(Body::Gradient(Box::new(gr))));
    }
    let start = t.iter().position(|&c| !is_ws(c)).unwrap_or(t.len());
    let p = &t[start..];
    if let Some(url) = image::pick_image_set_url(p) {
        return Some(Value::of(Body::Url(url)));
    }
    if !p.get(..4).is_some_and(|s| eq(s, b"url(")) {
        return None;
    }
    let mut u = 4;
    while u < p.len() && is_ws(p[u]) {
        u += 1;
    }
    let quote = p.get(u).copied().filter(|&c| c == b'"' || c == b'\'');
    if quote.is_some() {
        u += 1;
    }
    let rest = &p[u..];
    let end = match quote {
        Some(q) => image::quoted_end(rest, q)?,
        None => rest
            .iter()
            .position(|&c| c == b')' || is_ws(c))
            .unwrap_or(rest.len()),
    };
    if end == 0 {
        return None;
    }
    Some(Value::of(Body::Url(image::unescape_url(&rest[..end]))))
}

pub(crate) fn bg_repeat_token(tok: &[u8], allow_axis: bool) -> bool {
    [&b"repeat"[..], b"no-repeat", b"space", b"round"]
        .iter()
        .any(|k| eq(tok, k))
        || (allow_axis && (eq(tok, b"repeat-x") || eq(tok, b"repeat-y")))
}

pub(crate) fn bg_repeat_canonical(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    let la = lower(a);
    let Some(b) = b else {
        return la;
    };
    let lb = lower(b);
    if la == lb {
        la
    } else if la == b"repeat" && lb == b"no-repeat" {
        b"repeat-x".to_vec()
    } else if la == b"no-repeat" && lb == b"repeat" {
        b"repeat-y".to_vec()
    } else {
        [la.as_slice(), b" ", lb.as_slice()].concat()
    }
}

pub(crate) fn bg_token_is_box(tok: &[u8]) -> bool {
    eq(tok, b"border-box") || eq(tok, b"padding-box") || eq(tok, b"content-box")
}

pub(crate) fn bg_clip_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let tok = split_ws_limit(text, 3);
    match tok.as_slice() {
        [only] if bg_token_is_box(only) || eq(only, b"text") || eq(only, b"border-area") => {
            Some(lower(only))
        }
        [a, b]
            if (eq(a, b"border-area") && eq(b, b"text"))
                || (eq(a, b"text") && eq(b, b"border-area")) =>
        {
            Some(b"border-area text".to_vec())
        }
        _ => None,
    }
}

fn radius_corner(t: &[u8]) -> Result<Option<Vec<u8>>, Option<Value>> {
    let pair = split_ws_limit(t, 3);
    match pair.as_slice() {
        [one] => match length(one) {
            Some((num, _)) if num < 0.0 => Err(None),
            _ => Ok(None),
        },
        [a, b] => {
            let mut parsed = [(0.0, NUMBER); 2];
            for (slot, tok) in parsed.iter_mut().zip([a, b]) {
                match length(tok) {
                    Some((num, unit)) if num >= 0.0 && (unit != NUMBER || num == 0.0) => {
                        *slot = (num, if unit == NUMBER { PX } else { unit });
                    }
                    _ => return Err(None),
                }
            }
            if parsed[0].0 == parsed[1].0 && parsed[0].1 == parsed[1].1 {
                return Ok(Some(a.to_vec()));
            }
            Err(Some(Value::of(Body::Size(Size {
                w: parsed[0].0,
                h: parsed[1].0,
                w_unit: parsed[0].1,
                h_unit: parsed[1].1,
                w_auto: false,
                h_auto: false,
            }))))
        }
        _ => Err(None),
    }
}

fn clip_rect(t: &[u8]) -> Option<Value> {
    if eq(t, b"auto") {
        return Some(Value::keyword(b"auto".to_vec()));
    }
    let open = t.iter().position(|&c| c == b'(')?;
    let close = t.iter().rposition(|&c| c == b')')?;
    if close < open {
        return None;
    }
    let inner = &t[open + 1..close];
    let mut rect = Rect {
        v: [0.0; 4],
        unit: [PX; 4],
        is_auto: [true; 4],
    };
    let mut idx = 0;
    for part in inner.split(|&c| matches!(c, b',' | b' ' | b'\t')) {
        if idx >= 4 {
            break;
        }
        if part.is_empty() {
            continue;
        }
        if eq(part, b"auto") {
            rect.is_auto[idx] = true;
        } else if let Some((v, unit)) = length(part) {
            rect.v[idx] = v;
            rect.unit[idx] = unit;
            rect.is_auto[idx] = false;
        }
        idx += 1;
    }
    (idx == 4).then(|| Value::of(Body::Rect(rect)))
}

fn color_prop(prop: Prop, t: &[u8]) -> Option<Value> {
    if let Some(rgba) = parse_color(&c_text(t)) {
        return Some(Value::of(Body::Color(rgba)));
    }
    let kw = lower(t);
    let ok = matches!(kw.as_slice(), b"currentcolor" | b"inherit" | b"transparent")
        || (prop == Prop::OutlineColor && kw == b"invert");
    ok.then(|| Value::keyword(kw))
}

fn paint_prop(t: &[u8]) -> Option<Value> {
    if let Some(rgba) = parse_color(&c_text(t)) {
        return Some(Value::of(Body::Color(rgba)));
    }
    let kw = lower(t);
    let ok = matches!(
        kw.as_slice(),
        b"none" | b"currentcolor" | b"transparent" | b"context-fill" | b"context-stroke"
    ) || kw.starts_with(b"url(");
    ok.then(|| Value::keyword(kw))
}

fn paint_order(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if kw.is_empty() {
        return None;
    }
    if kw != b"normal" {
        let mut seen = 0;
        for part in split_gspace_words(&kw) {
            if !matches!(part, b"fill" | b"stroke" | b"markers") {
                return None;
            }
            seen += 1;
            if seen > 3 {
                return None;
            }
        }
        if seen == 0 {
            return None;
        }
    }
    Some(Value::keyword(kw))
}

fn dasharray(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if kw.is_empty() {
        return None;
    }
    if kw != b"none" {
        let mut seen = 0;
        for part in kw.split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b',')) {
            if part.is_empty() {
                continue;
            }
            let text = c_text(part);
            let (d, mut end) = ffi::strtod(&text, 0);
            while end < part.len() && b"%epxtcmnihra".contains(&part[end]) {
                end += 1;
            }
            if end != part.len() || d < 0.0 {
                return None;
            }
            seen += 1;
        }
        if seen == 0 {
            return None;
        }
    }
    Some(Value::keyword(kw))
}

fn length_prop(prop: Prop, t: &[u8]) -> Option<Value> {
    if prop == Prop::FontSize {
        if eq(t, b"larger") || eq(t, b"smaller") {
            return Some(Value::length(
                if eq(t, b"larger") {
                    1.2
                } else {
                    0.833333333333
                },
                EM,
            ));
        }
        let fs = font::size_keyword_px(t);
        if fs > 0.0 {
            return Some(Value::length(fs, PX));
        }
    }
    if is_border_width(prop) {
        let bw = if eq(t, b"thin") {
            1.0
        } else if eq(t, b"medium") {
            3.0
        } else if eq(t, b"thick") {
            5.0
        } else {
            -1.0
        };
        if bw >= 0.0 {
            return Some(Value::length(bw, PX));
        }
    }
    let sizing = matches!(
        prop,
        Prop::Width
            | Prop::Height
            | Prop::MinWidth
            | Prop::MaxWidth
            | Prop::MinHeight
            | Prop::MaxHeight
    );
    let max_sizing = matches!(prop, Prop::MaxWidth | Prop::MaxHeight);
    let keyword = (accepts_auto(prop) && eq(t, b"auto"))
        || (accepts_normal(prop) && eq(t, b"normal"))
        || (max_sizing && eq(t, b"none"))
        || (prop == Prop::FlexBasis && eq(t, b"content"))
        || (sizing
            && [
                &b"min-content"[..],
                b"max-content",
                b"fit-content",
                b"stretch",
                b"-webkit-fill-available",
                b"-moz-available",
            ]
            .iter()
            .any(|k| eq(t, k)));
    let mut v = if keyword {
        Some(Value::keyword(lower(t)))
    } else if let Some(parsed) = calc_value(t) {
        Some(Value::from_parsed(parsed))
    } else {
        length(t).map(|(num, unit)| Value::length(num, unit))
    };
    if v.as_ref().is_some_and(|v| {
        matches!(v.body, Body::Length(..) | Body::Calc(_)) && !numeric_valid(prop, &v.body)
    }) {
        v = None;
    }
    let mut v = v?;
    if let Body::Length(_, unit) = &mut v.body {
        if *unit == NUMBER && bare_number_is_length(prop) {
            *unit = PX;
        }
    }
    if prop == Prop::Opacity {
        match &mut v.body {
            Body::Length(num, unit) if *unit == PERCENT => {
                *num /= 100.0;
                *unit = NUMBER;
            }
            Body::Calc(c) if c.px == 0.0 && c.em == 0.0 && c.rem == 0.0 => {
                v = Value::length(c.pct / 100.0, NUMBER);
            }
            _ => {}
        }
    }
    Some(v)
}

fn position_axis(prop: Prop, t: &[u8]) -> Option<Value> {
    if let Some(parsed) = calc_value(t) {
        return Some(Value::from_parsed(parsed));
    }
    let kw = lower(t);
    let horizontal = matches!(prop, Prop::BackgroundPositionX | Prop::ObjectPositionX);
    let pct = match (horizontal, kw.as_slice()) {
        (true, b"left") | (false, b"top") => Some(0.0),
        (_, b"center") => Some(50.0),
        (true, b"right") | (false, b"bottom") => Some(100.0),
        _ => None,
    };
    if let Some(pct) = pct {
        return Some(Value::length(pct, PERCENT));
    }
    length(t).map(|(num, unit)| Value::length(num, unit))
}

fn background_size(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if matches!(kw.as_slice(), b"cover" | b"contain" | b"auto") {
        return Some(Value::keyword(kw));
    }
    let tokens = split_ws(t);
    let mut size = Size {
        w: 0.0,
        h: 0.0,
        w_unit: PX,
        h_unit: PX,
        w_auto: false,
        h_auto: true,
    };
    match tokens.as_slice() {
        [only] => {
            if eq(only, b"auto") {
                size.w_auto = true;
                size.h_auto = true;
            } else {
                (size.w, size.w_unit) = bg_size_component(only)?;
            }
        }
        [first, second, ..] => {
            if eq(first, b"auto") {
                size.w_auto = true;
            } else {
                (size.w, size.w_unit) = bg_size_component(first)?;
            }
            if eq(second, b"auto") {
                size.h_auto = true;
            } else {
                (size.h, size.h_unit) = bg_size_component(second)?;
                size.h_auto = false;
            }
        }
        [] => return None,
    }
    legacy_em_normalize(&mut size.w, &mut size.w_unit);
    legacy_em_normalize(&mut size.h, &mut size.h_unit);
    Some(Value::of(Body::Size(size)))
}

fn background_repeat(t: &[u8]) -> Option<Value> {
    let pair = split_ws_limit(t, 3);
    let kw = match pair.as_slice() {
        [one] if bg_repeat_token(one, true) => lower(one),
        [a, b] if bg_repeat_token(a, false) && bg_repeat_token(b, false) => {
            bg_repeat_canonical(a, Some(b))
        }
        _ => return None,
    };
    Some(Value::keyword(kw))
}

fn image_prop(t: &[u8]) -> Option<Value> {
    let mut v = image_reference(t);
    if v.is_none() && gradient::text_starts_gradient(t) {
        return None;
    }
    let mut iset_computed = None;
    if image::text_starts_image_set(t) {
        let computed = image::image_set_canonical(t, true)?;
        match &mut v {
            Some(v) => v.image_set_text = Some(computed),
            None => iset_computed = Some(computed),
        }
    }
    if v.is_none() && iset_computed.is_none() && text_is_ident(t) && !eq(t, b"none") {
        return None;
    }
    Some(v.unwrap_or_else(|| Value::keyword(iset_computed.unwrap_or_else(|| lower(t)))))
}

fn transform_prop(t: &[u8]) -> Option<Value> {
    if !eq(t, b"none") && !contains(t, b"var(") {
        transform::list_canonical(t)?;
    }
    if let Some(tf) = transform::parse_transform(t) {
        return Some(Value::of(Body::Transform(Box::new(tf))));
    }
    (strip(&lower(t)) == b"none").then(|| Value::keyword(b"none".to_vec()))
}

fn transform_value(tf: Option<Transform>) -> Option<Value> {
    tf.map(|tf| Value::of(Body::Transform(Box::new(tf))))
}

fn individual_prop(prop: Prop, t: &[u8]) -> Option<Value> {
    let kind = match prop {
        Prop::Translate => Individual::Translate,
        Prop::Rotate => Individual::Rotate,
        _ => Individual::Scale,
    };
    let canon = transform::individual_canonical(t, kind);
    if canon.is_none() && !contains(t, b"var(") {
        return None;
    }
    let v = match kind {
        Individual::Translate => transform_value(transform::parse_translate_prop(t)),
        Individual::Rotate => {
            transform_value(transform::parse_rotate_prop(canon.as_deref().unwrap_or(t)))
        }
        Individual::Scale => transform_value(transform::parse_scale_prop(t)),
    };
    v.or_else(|| {
        if eq(t, b"none") {
            keyword_choice(t, "none")
        } else {
            None
        }
    })
}

fn perspective(t: &[u8]) -> Option<Value> {
    if let Some((plain, NUMBER)) = length(t) {
        if plain != 0.0 {
            return None;
        }
    }
    let (ok, r) = calc::resolve_to_px_pct(t, false);
    if ok && r.px > 0.0 && r.pct == 0.0 {
        return Some(Value::length(r.px, PX));
    }
    if eq(t, b"none") {
        return keyword_choice(t, "none");
    }
    None
}

fn aspect_ratio(t: &[u8]) -> Option<Value> {
    let text = c_text(t);
    let s = text.to_bytes();
    let end = s.len();
    let mut rp = 0;
    let (mut with_auto, mut has_ratio) = (false, false);
    let (mut a, mut b) = (1.0, 1.0);
    loop {
        while rp < end && is_ws(s[rp]) {
            rp += 1;
        }
        if rp >= end {
            break;
        }
        if s[rp..].len() >= 4 && eq(&s[rp..rp + 4], b"auto") && (rp + 4 == end || is_ws(s[rp + 4]))
        {
            if with_auto {
                return None;
            }
            with_auto = true;
            rp += 4;
            continue;
        }
        if has_ratio {
            return None;
        }
        let (va, ea) = ffi::strtod(&text, rp);
        if ea == rp || va < 0.0 || !va.is_finite() || (ea < end && !is_ws(s[ea]) && s[ea] != b'/') {
            return None;
        }
        a = va;
        rp = ea;
        while rp < end && is_ws(s[rp]) {
            rp += 1;
        }
        if rp < end && s[rp] == b'/' {
            rp += 1;
            while rp < end && is_ws(s[rp]) {
                rp += 1;
            }
            let (vb, eb) = ffi::strtod(&text, rp);
            if eb == rp || vb < 0.0 || !vb.is_finite() || (eb < end && !is_ws(s[eb])) {
                return None;
            }
            b = vb;
            rp = eb;
        }
        has_ratio = true;
    }
    if !with_auto && !has_ratio {
        return None;
    }
    if !has_ratio {
        return Some(Value::keyword(b"auto".to_vec()));
    }
    Some(Value::of(Body::Size(Size {
        w: a,
        h: b,
        w_unit: NUMBER,
        h_unit: NUMBER,
        w_auto: with_auto,
        h_auto: false,
    })))
}

fn font_stretch(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if font::is_stretch_keyword(&kw) {
        return Some(Value::keyword(kw));
    }
    match length(t) {
        Some((num, PERCENT)) if num > 0.0 => Some(Value::length(num, PERCENT)),
        _ => None,
    }
}

fn settings_prop(t: &[u8], valid: fn(&[u8]) -> bool) -> Option<Value> {
    let kw = lower(t);
    if kw == b"normal" {
        return Some(Value::keyword(kw));
    }
    valid(t).then(|| Value::keyword(t.to_vec()))
}

fn tab_size(t: &[u8]) -> Option<Value> {
    if let Some(parsed) = calc_value(t) {
        let (num, unit) = match parsed {
            Parsed::Calc(c) if c.pct == 0.0 => (c.px + (c.em + c.rem) * 16.0, PX),
            Parsed::Length(num, unit) => (num, unit),
            Parsed::Calc(_) => return None,
        };
        if unit == PERCENT {
            return None;
        }
        return Some(Value::length(if num < 0.0 { 0.0 } else { num }, unit));
    }
    match length(t) {
        Some((len, unit)) if unit != PERCENT && len >= 0.0 => Some(Value::length(len, unit)),
        _ => None,
    }
}

fn border_spacing(t: &[u8]) -> Option<Value> {
    let tokens = split_ws(t);
    let (mut w, mut wu) = length(tokens.first()?)?;
    let (mut h, mut hu) = match tokens.get(1) {
        Some(tok) => length(tok)?,
        None => (w, wu),
    };
    if !(w >= 0.0 && h >= 0.0) {
        return None;
    }
    legacy_em_normalize(&mut w, &mut wu);
    legacy_em_normalize(&mut h, &mut hu);
    Some(Value::of(Body::Size(Size {
        w,
        h,
        w_unit: wu,
        h_unit: hu,
        w_auto: false,
        h_auto: false,
    })))
}

fn keyword_in(t: &[u8], words: &[&[u8]]) -> Option<Value> {
    let kw = lower(t);
    words.contains(&kw.as_slice()).then(|| Value::keyword(kw))
}

const CURSORS: [&[u8]; 36] = [
    b"auto",
    b"default",
    b"none",
    b"context-menu",
    b"help",
    b"pointer",
    b"progress",
    b"wait",
    b"cell",
    b"crosshair",
    b"text",
    b"vertical-text",
    b"alias",
    b"copy",
    b"move",
    b"no-drop",
    b"not-allowed",
    b"grab",
    b"grabbing",
    b"e-resize",
    b"n-resize",
    b"ne-resize",
    b"nw-resize",
    b"s-resize",
    b"se-resize",
    b"sw-resize",
    b"w-resize",
    b"ew-resize",
    b"ns-resize",
    b"nesw-resize",
    b"nwse-resize",
    b"col-resize",
    b"row-resize",
    b"all-scroll",
    b"zoom-in",
    b"zoom-out",
];

fn font_weight(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if matches!(kw.as_slice(), b"normal" | b"bold" | b"bolder" | b"lighter")
        || contains(&kw, b"var(")
        || contains(&kw, b"attr(")
        || contains(&kw, b"env(")
        || kw.contains(&b'"')
        || kw.contains(&b'\'')
    {
        return Some(Value::keyword(kw));
    }
    let num = match calc_value(t) {
        Some(Parsed::Length(v, NUMBER)) => Some(v),
        Some(_) => None,
        None => {
            let text = c_text(t);
            let (d, mut end) = ffi::strtod(&text, 0);
            let s = text.to_bytes();
            while end < s.len() && is_ws(s[end]) {
                end += 1;
            }
            (end != 0 && end == s.len()).then_some(d)
        }
    };
    let num = num.filter(|n| n.is_finite() && (1.0..=1000.0).contains(n))?;
    Some(Value::keyword(ffi::format_double(c"%g", num)))
}

fn keyword_text(text: Option<Vec<u8>>) -> Option<Value> {
    text.map(Value::keyword)
}

fn longhand(prop: Prop) -> Option<Longhand> {
    let lh = match prop {
        Prop::AnimationName => Longhand::AnimationName,
        Prop::AnimationTimingFunction => Longhand::AnimationTimingFunction,
        Prop::AnimationIterationCount => Longhand::AnimationIterationCount,
        Prop::AnimationDirection => Longhand::AnimationDirection,
        Prop::AnimationFillMode => Longhand::AnimationFillMode,
        Prop::AnimationPlayState => Longhand::AnimationPlayState,
        Prop::TransitionProperty => Longhand::TransitionProperty,
        Prop::TransitionTimingFunction => Longhand::TransitionTimingFunction,
        Prop::TransitionBehavior => Longhand::TransitionBehavior,
        Prop::AnimationTimeline => Longhand::AnimationTimeline,
        Prop::AnimationRangeStart => Longhand::AnimationRangeStart,
        Prop::AnimationRangeEnd => Longhand::AnimationRangeEnd,
        Prop::AnimationComposition => Longhand::AnimationComposition,
        _ => return None,
    };
    Some(lh)
}

fn by_prop(prop: Prop, t: &[u8]) -> Option<Value> {
    use Prop as P;
    match prop {
        P::Display => keyword_text(display::normalize(t)),
        P::Position => {
            let kw = lower(t);
            let kw: &[u8] = if kw == b"-webkit-sticky" {
                b"sticky"
            } else {
                &kw
            };
            keyword_choice(kw, "static relative absolute fixed sticky")
        }
        P::Overflow | P::OverflowX | P::OverflowY => {
            let kw = lower(t);
            let kw: &[u8] = if kw == b"overlay" { b"auto" } else { &kw };
            keyword_choice(kw, "visible hidden clip scroll auto")
        }
        P::BoxSizing => keyword_choice(t, "content-box border-box"),
        P::Visibility => keyword_choice(t, "visible hidden collapse"),
        P::PointerEvents => keyword_choice(
            t,
            "auto none visiblepainted visiblefill visiblestroke visible painted fill stroke bounding-box all",
        ),
        P::FlexDirection => keyword_choice(t, "row row-reverse column column-reverse"),
        P::FlexWrap => keyword_choice(t, "nowrap wrap wrap-reverse"),
        P::Float => keyword_choice(t, "none left right"),
        P::Clear => keyword_choice(t, "none left right both"),
        P::BorderTopStyle
        | P::BorderRightStyle
        | P::BorderBottomStyle
        | P::BorderLeftStyle
        | P::ColumnRuleStyle => keyword_choice(
            t,
            "none hidden dotted dashed solid double groove ridge inset outset",
        ),
        P::OutlineStyle => keyword_choice(
            t,
            "auto none hidden dotted dashed solid double groove ridge inset outset",
        ),
        P::BackgroundClip => keyword_text(bg_clip_canonical(t)),
        P::BackgroundOrigin => keyword_choice(t, "border-box padding-box content-box"),
        P::BackgroundAttachment => keyword_choice(t, "scroll fixed local"),
        P::ScrollbarWidth => keyword_choice(t, "auto thin none"),
        P::ImageRendering => keyword_choice(t, "auto smooth high-quality crisp-edges pixelated"),
        P::OverflowWrap => keyword_choice(t, "normal break-word anywhere"),
        P::WordBreak => {
            keyword_choice(t, "normal break-all keep-all break-word auto-phrase manual")
        }
        P::Hyphens => keyword_choice(t, "none manual auto"),
        P::TextOverflow => keyword_choice(t, "clip ellipsis"),
        P::WebkitBoxOrient => keyword_choice(t, "horizontal vertical inline-axis block-axis"),
        P::MaskClip => mask_box_keyword(t).map(Value::keyword),
        P::MaskComposite => mask_composite_keyword(t).map(Value::keyword),
        P::TextDecorationStyle => keyword_choice(t, "solid double dotted dashed wavy"),
        P::ListStylePosition => keyword_choice(t, "outside inside"),
        P::UserSelect => keyword_choice(t, "auto text none contain all"),
        P::ObjectFit => keyword_choice(t, "fill contain cover none scale-down"),
        P::Appearance => keyword_choice(
            t,
            "none auto base-select menulist-button textfield button searchfield checkbox radio menulist listbox textarea",
        ),
        P::TableLayout => keyword_choice(t, "auto fixed"),
        P::CaptionSide => keyword_choice(t, "top bottom block-start block-end"),
        P::BorderCollapse => keyword_choice(t, "separate collapse"),
        P::ContainerType => keyword_choice(t, "normal size inline-size"),
        P::Direction => keyword_choice(t, "ltr rtl"),
        P::UnicodeBidi => keyword_choice(
            t,
            "normal embed isolate bidi-override isolate-override plaintext",
        ),
        P::FontStyle => keyword_choice(t, "normal italic oblique"),
        P::FontVariant => keyword_choice(t, "normal small-caps"),
        P::TextAlign => keyword_choice(t, "start end left right center justify match-parent"),
        P::TextTransform => keyword_choice(t, "none capitalize uppercase lowercase"),
        P::JustifyContent => alignment_keyword(
            t,
            "normal stretch center start end flex-start flex-end left right space-between space-around space-evenly",
        ),
        P::AlignItems => alignment_keyword(
            t,
            "normal stretch center start end self-start self-end flex-start flex-end baseline",
        ),
        P::AlignSelf => alignment_keyword(
            t,
            "auto normal stretch center start end self-start self-end flex-start flex-end baseline",
        ),
        P::AlignContent => alignment_keyword(
            t,
            "normal stretch center start end flex-start flex-end baseline space-between space-around space-evenly",
        ),
        P::JustifyItems => alignment_keyword(
            t,
            "normal stretch center start end self-start self-end flex-start flex-end left right baseline legacy",
        ),
        P::JustifySelf => alignment_keyword(
            t,
            "auto normal stretch center start end self-start self-end flex-start flex-end left right baseline",
        ),
        P::MixBlendMode => keyword_choice(
            t,
            "normal multiply screen overlay darken lighten color-dodge color-burn hard-light soft-light difference exclusion hue saturation color luminosity",
        ),
        P::TransformStyle => keyword_choice(t, "flat preserve-3d"),
        P::BackfaceVisibility => keyword_choice(t, "visible hidden"),
        P::Clip => clip_rect(t),
        P::Color
        | P::BackgroundColor
        | P::BorderTopColor
        | P::BorderRightColor
        | P::BorderBottomColor
        | P::BorderLeftColor
        | P::OutlineColor
        | P::TextDecorationColor
        | P::ColumnRuleColor
        | P::CaretColor
        | P::StopColor
        | P::AccentColor => color_prop(prop, t),
        P::Fill | P::Stroke => paint_prop(t),
        P::FillRule | P::ClipRule => keyword_choice(t, "nonzero evenodd"),
        P::StrokeLinecap => keyword_choice(t, "butt round square"),
        P::StrokeLinejoin => keyword_choice(t, "miter round bevel miter-clip arcs"),
        P::TextAnchor => keyword_choice(t, "start middle end"),
        P::DominantBaseline => keyword_choice(
            t,
            "auto text-bottom alphabetic ideographic middle central mathematical hanging text-top",
        ),
        P::VectorEffect => keyword_choice(
            t,
            "none non-scaling-stroke non-scaling-size non-rotation fixed-position",
        ),
        P::ShapeRendering => keyword_choice(t, "auto optimizespeed crispedges geometricprecision"),
        P::PaintOrder => paint_order(t),
        P::StrokeDasharray => dasharray(t),
        P::FontSize
        | P::MarginTop
        | P::MarginRight
        | P::MarginBottom
        | P::MarginLeft
        | P::PaddingTop
        | P::PaddingRight
        | P::PaddingBottom
        | P::PaddingLeft
        | P::BorderTopWidth
        | P::BorderRightWidth
        | P::BorderBottomWidth
        | P::BorderLeftWidth
        | P::Width
        | P::Height
        | P::MaxWidth
        | P::MaxHeight
        | P::MinWidth
        | P::MinHeight
        | P::LetterSpacing
        | P::WordSpacing
        | P::TextIndent
        | P::Opacity
        | P::FillOpacity
        | P::StrokeOpacity
        | P::StopOpacity
        | P::StrokeMiterlimit
        | P::StrokeWidth
        | P::StrokeDashoffset
        | P::SvgX
        | P::SvgY
        | P::Cx
        | P::Cy
        | P::R
        | P::Rx
        | P::Ry
        | P::BorderRadius
        | P::BorderTopLeftRadius
        | P::BorderTopRightRadius
        | P::BorderBottomRightRadius
        | P::BorderBottomLeftRadius
        | P::Gap
        | P::RowGap
        | P::ColumnGap
        | P::FlexGrow
        | P::FlexShrink
        | P::FlexBasis
        | P::LineHeight
        | P::OutlineWidth
        | P::OutlineOffset
        | P::Top
        | P::Right
        | P::Bottom
        | P::Left
        | P::ColumnWidth
        | P::ScrollPaddingTop
        | P::ScrollPaddingRight
        | P::ScrollPaddingBottom
        | P::ScrollPaddingLeft
        | P::ScrollMarginTop
        | P::ScrollMarginRight
        | P::ScrollMarginBottom
        | P::ScrollMarginLeft
        | P::ColumnRuleWidth => length_prop(prop, t),
        P::Order
        | P::ZIndex
        | P::ColumnCount
        | P::Orphans
        | P::Widows
        | P::MaxLines
        | P::HyphenateLimitLines => integer_property(prop, t),
        P::LineClamp => {
            if eq(t, b"none") {
                return keyword_choice(t, "none");
            }
            integer_property(prop, t)
                .filter(|v| matches!(v.body, Body::Length(n, _) if n >= 1.0 || n.is_nan()))
        }
        P::ColumnSpan => (eq(t, b"none") || eq(t, b"all")).then(|| Value::keyword(lower(t))),
        P::TransitionDelay | P::TransitionDuration | P::AnimationDelay => {
            time::property_valid(t).then(|| Value::keyword(t.to_vec()))
        }
        P::AnimationDuration => keyword_text(animation::duration_canonical(t)),
        P::BoxShadow | P::TextShadow => {
            if eq(t, b"none") {
                return keyword_choice(t, "none");
            }
            let is_text = prop == P::TextShadow;
            if shadow::specified_canonical(t, is_text).is_none() && !contains(t, b"var(") {
                return None;
            }
            shadow::parse_list(t).map(|mut list| {
                list.is_text = i32::from(is_text);
                Value::of(Body::Shadow(Box::new(list)))
            })
        }
        P::GridTemplateColumns | P::GridTemplateRows | P::GridAutoRows | P::GridAutoColumns => {
            let auto_tracks = matches!(prop, P::GridAutoRows | P::GridAutoColumns);
            let tracks = grid::parse_tracks(t).filter(|tk| !(tk.subgrid != 0 && auto_tracks));
            let v = tracks.and_then(|tk| {
                let specified = grid::track_text_canonical(t);
                if tk.subgrid != 0 && specified.is_none() {
                    return None;
                }
                let mut v = Value::of(Body::Tracks(Box::new(tk)));
                v.specified = specified;
                Some(v)
            });
            v.or_else(|| keyword_choice(t, "none"))
        }
        P::GridTemplateAreas => grid::parse_areas(t)
            .map(|areas| Value::of(Body::Areas(areas)))
            .or_else(|| keyword_choice(t, "none")),
        P::BackgroundPositionX
        | P::BackgroundPositionY
        | P::ObjectPositionX
        | P::ObjectPositionY => position_axis(prop, t),
        P::BackgroundSize => background_size(t),
        P::BackgroundRepeat => background_repeat(t),
        P::Content => keyword_text(if contains(t, b"var(") {
            Some(t.to_vec())
        } else {
            content_canonical(t)
        }),
        P::CounterReset | P::CounterIncrement | P::CounterSet => {
            let which = match prop {
                P::CounterIncrement => CounterProp::Increment,
                P::CounterReset => CounterProp::Reset,
                _ => CounterProp::Set,
            };
            keyword_text(counter::list_canonical(t, which))
        }
        P::ListStyleType => keyword_text(counter::list_style_type_canonical(t)),
        P::OverflowClipMargin => keyword_text(display::overflow_clip_margin_canonical(t)),
        P::Quotes => Some(Value::keyword(strip(t).to_vec())),
        P::BorderImageSource => image_reference(t).or_else(|| keyword_in(t, &[b"none"])),
        P::BorderImageSlice => keyword_text(border_image::slice_canonical(t)),
        P::BorderImageWidth => keyword_text(border_image::quad_canonical(t, true, true)),
        P::BorderImageOutset => keyword_text(border_image::quad_canonical(t, false, false)),
        P::BorderImageRepeat => keyword_text(border_image::repeat_canonical(t)),
        P::MaskImage | P::ListStyleImage | P::BackgroundImage => image_prop(t),
        P::Transform => transform_prop(t),
        P::TransformBox => keyword_choice(t, "content-box border-box fill-box stroke-box view-box"),
        P::TransformOrigin | P::PerspectiveOrigin => {
            if transform::origin_canonical(t, prop == P::PerspectiveOrigin).is_none()
                && !contains(t, b"var(")
            {
                return None;
            }
            transform_value(transform::parse_transform_origin(t))
        }
        P::Translate | P::Rotate | P::Scale => individual_prop(prop, t),
        P::Perspective => perspective(t),
        P::Transition => {
            animation::shorthand_parse(t, false).map(|list| Value::of(Body::Anim(list)))
        }
        P::Animation => animation::shorthand_parse(t, true).map(|list| Value::of(Body::Anim(list))),
        P::AspectRatio => aspect_ratio(t),
        P::ContainerName => keyword_text(container::name_canonical(t)),
        P::FontStretch => font_stretch(t),
        P::FontKerning => keyword_in(t, &[b"auto", b"normal", b"none"]),
        P::FontVariantLigatures => {
            let kw = lower(t);
            font::ligatures_valid(&kw).then(|| Value::keyword(kw))
        }
        P::FontFeatureSettings => settings_prop(t, font::feature_settings_valid),
        P::FontVariationSettings => settings_prop(t, font::variation_settings_valid),
        P::TabSize => tab_size(t),
        P::BorderSpacing => border_spacing(t),
        P::WhiteSpace => keyword_in(
            t,
            &[
                b"normal",
                b"nowrap",
                b"pre",
                b"pre-wrap",
                b"pre-line",
                b"break-spaces",
            ],
        ),
        P::BreakBefore | P::BreakAfter => keyword_choice(
            t,
            "auto avoid avoid-page avoid-column avoid-region page column region left right recto verso always all",
        ),
        P::BreakInside => keyword_choice(t, "auto avoid avoid-page avoid-column avoid-region"),
        P::ScrollSnapType => scroll_snap_type(t),
        P::ScrollSnapAlign => scroll_snap_align(t),
        P::ScrollSnapStop => keyword_choice(t, "normal always"),
        P::ContentVisibility => keyword_in(t, &[b"visible", b"auto", b"hidden"]),
        P::WritingMode => keyword_in(
            t,
            &[
                b"horizontal-tb",
                b"vertical-rl",
                b"vertical-lr",
                b"sideways-rl",
                b"sideways-lr",
                b"tb",
                b"tb-rl",
            ],
        ),
        P::TextOrientation => {
            let kw = lower(t);
            let kw = if kw == b"sideways-right" {
                b"sideways".to_vec()
            } else {
                kw
            };
            matches!(
                kw.as_slice(),
                b"mixed" | b"upright" | b"sideways" | b"use-glyph-orientation"
            )
            .then(|| Value::keyword(kw))
        }
        P::Cursor => {
            if t.contains(&b'(') {
                return Some(Value::keyword(lower(t)));
            }
            keyword_in(t, &CURSORS)
        }
        P::FontWeight => font_weight(t),
        P::FontFamily => keyword_text(font::family_canonical(t)),
        P::GridAutoFlow => keyword_text(grid::auto_flow_canonical(t)),
        P::GridRow | P::GridColumn | P::GridArea => {
            keyword_text(grid::placement_canonical(t, prop == P::GridArea))
        }
        P::GridRowStart | P::GridRowEnd | P::GridColumnStart | P::GridColumnEnd => {
            keyword_text(grid::line_canonical(t).map(|(text, _)| text))
        }
        _ => match longhand(prop) {
            Some(lh) => keyword_text(animation::longhand_canonical(lh, t)),
            None => Some(Value::keyword(lower(t))),
        },
    }
}

pub(crate) fn parse_for(prop: Option<Prop>, text: &[u8]) -> Option<Value> {
    match prop {
        Some(prop) => parse(prop, text),
        None => {
            let start = text.iter().position(|&c| !is_ws(c)).unwrap_or(text.len());
            let t = trim_range(text, start, text.len());
            wide_keyword(t).or_else(|| Some(Value::keyword(lower(t))))
        }
    }
}

pub(crate) fn parse(prop: Prop, text: &[u8]) -> Option<Value> {
    let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
    let start = text.iter().position(|&c| !is_ws(c)).unwrap_or(text.len());
    let mut t = trim_range(text, start, text.len());
    if let Some(v) = wide_keyword(t) {
        return Some(v);
    }
    if bg_layered(prop) && has_top_level_comma(t) {
        return layer_list(prop, t);
    }
    let collapsed;
    if matches!(
        prop,
        Prop::BorderTopLeftRadius
            | Prop::BorderTopRightRadius
            | Prop::BorderBottomRightRadius
            | Prop::BorderBottomLeftRadius
    ) {
        match radius_corner(t) {
            Ok(Some(first)) => {
                collapsed = first;
                t = &collapsed;
            }
            Ok(None) => {}
            Err(v) => return v,
        }
    }
    let mut v = by_prop(prop, t)?;
    if v.next_layer.is_none() {
        record_specified_keyword(&mut v, prop, t);
    }
    Some(v)
}
