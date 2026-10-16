//! Southstar — parsing a declaration's value for one property into the value css.c stores: keywords, lengths, calc(), colours, sizes, rects, URLs and the structured values of the other sections, with layer lists for the background and mask properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use southstar_css_color::Color;

use crate::animation::{self, Entry, Longhand};
use crate::border_image;
use crate::calc::{self, Calc, Parsed};
use crate::clip_path;
use crate::color;
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
use crate::scan::{is_gspace, is_ws, split_ws_limit, starts_with_ci, strip, strtol10, trim_range};
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
    Color(Color),
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

fn lower_outside_strings(t: &[u8]) -> Vec<u8> {
    let mut quote = 0u8;
    t.iter()
        .map(|&c| {
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
                c
            } else {
                if c == b'"' || c == b'\'' {
                    quote = c;
                }
                c.to_ascii_lowercase()
            }
        })
        .collect()
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

pub(crate) fn word_is_one_of(w: &[u8], choices: &str) -> bool {
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
        return (found.0 >= 0.0).then_some(found);
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
            | Prop::MaskMode
            | Prop::MaskRepeat
            | Prop::MaskPositionX
            | Prop::MaskPositionY
            | Prop::MaskSize
            | Prop::MaskOrigin
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
            | Prop::ColumnHeight
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
            | Prop::ScrollPaddingTop
            | Prop::ScrollPaddingRight
            | Prop::ScrollPaddingBottom
            | Prop::ScrollPaddingLeft
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
            | Prop::ColumnHeight
            | Prop::ColumnRuleWidth
    )
}

fn bare_number_is_length(prop: Prop) -> bool {
    !matches!(
        prop,
        Prop::Opacity | Prop::FlexGrow | Prop::FlexShrink | Prop::LineHeight
    )
}

fn is_scroll_margin(prop: Prop) -> bool {
    matches!(
        prop,
        Prop::ScrollMarginTop
            | Prop::ScrollMarginRight
            | Prop::ScrollMarginBottom
            | Prop::ScrollMarginLeft
    )
}

fn numeric_valid(prop: Prop, body: &Body) -> bool {
    match body {
        Body::Length(_, PERCENT) if is_scroll_margin(prop) => false,
        Body::Calc(c) if is_scroll_margin(prop) && c.pct != 0.0 => false,
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

#[allow(clippy::result_large_err)]
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

const BLEND_MODES: &str = "normal multiply screen overlay darken lighten color-dodge color-burn hard-light soft-light difference exclusion hue saturation color luminosity";

fn blend_mode_list(t: &[u8]) -> Option<Value> {
    let mut out = Vec::new();
    for part in t.split(|&c| c == b',') {
        let Body::Keyword(kw) = keyword_choice(strip(part), BLEND_MODES)?.body else {
            return None;
        };
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(&kw);
    }
    Some(Value::keyword(out))
}

fn color_value(t: &[u8]) -> Option<Value> {
    match color::parse_value_or_current(t)? {
        Some(color) => Some(Value::of(Body::Color(color))),
        None => Some(Value::keyword(lower(t))),
    }
}

fn color_prop(prop: Prop, t: &[u8]) -> Option<Value> {
    if let Some(value) = color_value(t) {
        return Some(value);
    }
    if prop == Prop::CaretColor
        && let [first, second] = split_ws_limit(t, 3)[..]
        && eq(second, b"auto")
    {
        return color_prop(prop, first);
    }
    let kw = lower(t);
    let ok = matches!(kw.as_slice(), b"currentcolor" | b"inherit" | b"transparent")
        || (prop == Prop::OutlineColor && kw == b"invert");
    ok.then(|| Value::keyword(kw))
}

fn paint_prop(t: &[u8]) -> Option<Value> {
    if let Some(value) = color_value(t) {
        return Some(value);
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

fn folds_relative_units(t: &[u8]) -> bool {
    const UNITS: [&[u8]; 9] = [
        b"ex", b"ch", b"vw", b"vh", b"vmin", b"vmax", b"lh", b"cap", b"cq",
    ];
    let lower = t.to_ascii_lowercase();
    t.contains(&b'(')
        && lower.windows(2).enumerate().any(|(i, _)| {
            lower[i].is_ascii_digit() && UNITS.iter().any(|unit| lower[i + 1..].starts_with(unit))
        })
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
            | Prop::FlexBasis
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
        let mut value = Value::from_parsed(parsed);
        if folds_relative_units(t) {
            value.specified = crate::math::math_canonical(&c_text(t)).or_else(|| Some(lower(t)));
        }
        Some(value)
    } else {
        length(t).map(|(num, unit)| Value::length(num, unit))
    };
    if v.as_ref().is_some_and(|v| {
        matches!(v.body, Body::Length(..) | Body::Calc(_)) && !numeric_valid(prop, &v.body)
    }) {
        v = None;
    }
    let mut v = v?;
    if let Body::Length(_, unit) = &mut v.body
        && *unit == NUMBER
        && bare_number_is_length(prop)
    {
        *unit = PX;
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
    let horizontal = matches!(
        prop,
        Prop::BackgroundPositionX | Prop::ObjectPositionX | Prop::MaskPositionX
    );
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
        [first, second] => {
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
        _ => return None,
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
    if let Some((plain, NUMBER)) = length(t)
        && plain != 0.0
    {
        return None;
    }
    if let Some(Parsed::Length(plain, NUMBER)) = calc_value(t)
        && plain != 0.0
    {
        return None;
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

fn font_style(t: &[u8]) -> Option<Value> {
    if let Some(v) = keyword_choice(t, "normal italic oblique") {
        return Some(v);
    }
    let [kw, angle] = split_ws_limit(t, 3)[..] else {
        return None;
    };
    if !eq(kw, b"oblique") {
        return None;
    }
    let deg = font::oblique_angle(angle)?;
    Some(Value::keyword(if deg == 0.0 {
        b"normal".to_vec()
    } else {
        format!("oblique {}deg", southstar_numfmt::general(deg, 6)).into_bytes()
    }))
}

fn shape_outside(t: &[u8]) -> Option<Value> {
    let stripped = strip(t);
    if starts_with_ci(stripped, b"url(")
        || stripped.contains(&b'(') && !clip_path::is_shape_text(stripped)
    {
        return image_prop(t);
    }
    if split_ws_limit(stripped, 3)
        .iter()
        .any(|w| word_is_one_of(w, "fill-box stroke-box view-box"))
    {
        return None;
    }
    clip_path::canonical(t, b"margin-box").map(Value::keyword)
}

const ALIGNMENT_BASELINES: &str =
    "baseline text-bottom alphabetic ideographic middle central mathematical text-top hanging";

fn shift_text(t: &[u8]) -> Option<Vec<u8>> {
    if word_is_one_of(t, "sub super top center bottom") {
        return Some(lower(t));
    }
    let numeric =
        calc_value(t).is_some() || length(t).is_some_and(|(n, unit)| unit != NUMBER || n == 0.0);
    numeric.then(|| lower(t))
}

fn baseline_shift(t: &[u8]) -> Option<Value> {
    let text = shift_text(strip(t))?;
    Some(Value::keyword(if text == b"0" {
        b"0px".to_vec()
    } else {
        text
    }))
}

fn vertical_align(t: &[u8]) -> Option<Value> {
    let mut source: Option<Vec<u8>> = None;
    let mut align: Option<Vec<u8>> = None;
    let mut shift: Option<Vec<u8>> = None;
    for word in split_ws_limit(strip(t), 5) {
        if word_is_one_of(word, "first last") && source.is_none() {
            source = Some(lower(word));
        } else if word_is_one_of(word, ALIGNMENT_BASELINES)
            && !eq(word, b"hanging")
            && align.is_none()
        {
            align = Some(lower(word));
        } else if shift.is_none()
            && let Some(text) = shift_text(word)
        {
            shift = Some(text);
        } else {
            return None;
        }
    }
    let zero = |s: &[u8]| !s.contains(&b'(') && length(s).is_some_and(|(n, _)| n == 0.0);
    let shift = shift.filter(|s| !zero(s));
    let align = align.filter(|a| a != b"baseline" || (source.is_none() && shift.is_none()));
    let parts: Vec<Vec<u8>> = [source, align, shift].into_iter().flatten().collect();
    let text = if parts.is_empty() {
        b"baseline".to_vec()
    } else {
        parts.join(&b' ')
    };
    Some(Value::keyword(text))
}

fn contain(t: &[u8]) -> Option<Value> {
    if let Some(v) = keyword_choice(t, "none strict content") {
        return Some(v);
    }
    const ORDER: [&[u8]; 5] = [b"size", b"inline-size", b"layout", b"style", b"paint"];
    let mut seen = [false; 5];
    for word in split_ws_limit(t, 6) {
        let i = ORDER.iter().position(|k| eq(word, k))?;
        if seen[i] || (i < 2 && (seen[0] || seen[1])) {
            return None;
        }
        seen[i] = true;
    }
    let words: Vec<&[u8]> = ORDER
        .iter()
        .zip(seen)
        .filter_map(|(k, on)| on.then_some(*k))
        .collect();
    (!words.is_empty()).then(|| Value::keyword(words.join(&b' ')))
}

fn will_change(t: &[u8]) -> Option<Value> {
    if eq(strip(t), b"auto") {
        return Some(Value::keyword(b"auto".to_vec()));
    }
    let mut out: Vec<u8> = Vec::new();
    for part in t.split(|&c| c == b',') {
        let word = strip(part);
        let reserved = word_is_one_of(
            word,
            "will-change none all auto initial inherit unset revert revert-layer revert-rule default",
        );
        if word.is_empty() || reserved || !text_is_ident(word) {
            return None;
        }
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        out.extend_from_slice(word);
    }
    Some(Value::keyword(out))
}

fn text_transform(t: &[u8]) -> Option<Value> {
    if let Some(v) = keyword_choice(t, "none math-auto") {
        return Some(v);
    }
    let mut case: Option<Vec<u8>> = None;
    let (mut full_width, mut full_size_kana) = (false, false);
    for word in split_ws_limit(t, 4) {
        if word_is_one_of(word, "capitalize uppercase lowercase") && case.is_none() {
            case = Some(lower(word));
        } else if eq(word, b"full-width") && !full_width {
            full_width = true;
        } else if eq(word, b"full-size-kana") && !full_size_kana {
            full_size_kana = true;
        } else {
            return None;
        }
    }
    let mut out = case.unwrap_or_default();
    for (set, word) in [
        (full_width, &b"full-width"[..]),
        (full_size_kana, b"full-size-kana"),
    ] {
        if set {
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend_from_slice(word);
        }
    }
    (!out.is_empty()).then(|| Value::keyword(out))
}

fn font_palette(t: &[u8]) -> Option<Value> {
    if t.starts_with(b"--")
        && t[2..]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80)
    {
        return Some(Value::keyword(t.to_vec()));
    }
    keyword_choice(t, "normal light dark")
}

fn font_stretch(t: &[u8]) -> Option<Value> {
    let kw = lower(t);
    if font::is_stretch_keyword(&kw) {
        return Some(Value::keyword(kw));
    }
    if let Some(parsed) = calc_value(t) {
        return match parsed {
            Parsed::Length(num, PERCENT) => Some(Value::length(num.max(0.0), PERCENT)),
            Parsed::Calc(c) if c.px == 0.0 && c.em == 0.0 && c.rem == 0.0 && c.func == 0 => {
                Some(Value::length(c.pct.max(0.0), PERCENT))
            }
            _ => None,
        };
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

const FONT_VARIANT_SLOTS: [&str; 22] = [
    "common-ligatures no-common-ligatures",
    "discretionary-ligatures no-discretionary-ligatures",
    "historical-ligatures no-historical-ligatures",
    "contextual no-contextual",
    "small-caps all-small-caps petite-caps all-petite-caps unicase titling-caps",
    "stylistic(",
    "historical-forms",
    "styleset(",
    "character-variant(",
    "swash(",
    "ornaments(",
    "annotation(",
    "lining-nums oldstyle-nums",
    "proportional-nums tabular-nums",
    "diagonal-fractions stacked-fractions",
    "ordinal",
    "slashed-zero",
    "jis78 jis83 jis90 jis04 simplified traditional",
    "full-width proportional-width",
    "ruby",
    "sub super",
    "text emoji unicode",
];

fn font_variant_slot(token: &[u8]) -> Option<usize> {
    if let Some(open) = token.iter().position(|&c| c == b'(') {
        let inner = token.strip_suffix(b")")?.get(open + 1..)?;
        if inner.is_empty()
            || !inner
                .iter()
                .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
        {
            return None;
        }
        let head = &token[..=open];
        return FONT_VARIANT_SLOTS
            .iter()
            .position(|s| eq(s.as_bytes(), head));
    }
    FONT_VARIANT_SLOTS
        .iter()
        .position(|s| !s.ends_with('(') && word_is_one_of(token, s))
}

fn font_variant_longhand(t: &[u8], slots: core::ops::RangeInclusive<usize>) -> Option<Value> {
    let words = split_gspace_words(t);
    if let [w] = words.as_slice()
        && eq(w, b"normal")
    {
        return Some(Value::keyword(b"normal".to_vec()));
    }
    let mut chosen: [Option<Vec<u8>>; 22] = Default::default();
    for w in &words {
        let i = font_variant_slot(w).filter(|i| slots.contains(i))?;
        if chosen[i].is_some() {
            return None;
        }
        chosen[i] = Some(lower(w));
    }
    if words.is_empty() {
        return None;
    }
    let canonical: Vec<Vec<u8>> = chosen.into_iter().flatten().collect();
    Some(Value::keyword(canonical.join(&b' ')))
}

fn font_language_override(t: &[u8]) -> Option<Value> {
    if eq(t, b"normal") {
        return Some(Value::keyword(b"normal".to_vec()));
    }
    let quote = *t.first()?;
    if !matches!(quote, b'"' | b'\'') || t.len() < 2 || t[t.len() - 1] != quote {
        return None;
    }
    let mut inner = &t[1..t.len() - 1];
    if !(1..=4).contains(&inner.len()) || !inner.iter().all(|c| (b' '..=b'~').contains(c)) {
        return None;
    }
    if inner.contains(&quote) || inner.contains(&b'\\') {
        return None;
    }
    while let Some(rest) = inner.strip_suffix(b" ") {
        inner = rest;
    }
    let mut out = vec![b'"'];
    out.extend_from_slice(inner);
    out.push(b'"');
    Some(Value::keyword(out))
}

fn hanging_punctuation(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    if let [w] = words.as_slice()
        && eq(w, b"none")
    {
        return Some(Value::keyword(b"none".to_vec()));
    }
    let (mut first, mut last, mut end) = (false, false, false);
    for w in &words {
        let seen = if eq(w, b"first") {
            &mut first
        } else if eq(w, b"last") {
            &mut last
        } else if eq(w, b"force-end") || eq(w, b"allow-end") {
            &mut end
        } else {
            return None;
        };
        if *seen {
            return None;
        }
        *seen = true;
    }
    (!words.is_empty()).then(|| Value::keyword(lower(&words.join(&b' '))))
}

fn text_autospace(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    if let [w] = words.as_slice()
        && word_is_one_of(w, "normal auto no-autospace")
    {
        return Some(Value::keyword(lower(w)));
    }
    const ORDER: [&[u8]; 5] = [
        b"ideograph-alpha",
        b"ideograph-numeric",
        b"punctuation",
        b"insert",
        b"replace",
    ];
    let mut seen = [false; 5];
    for w in &words {
        let i = ORDER.iter().position(|o| eq(o, w))?;
        if seen[i] {
            return None;
        }
        seen[i] = true;
    }
    if !(seen[0] || seen[1] || seen[2]) || (seen[3] && seen[4]) {
        return None;
    }
    let canonical: Vec<&[u8]> = ORDER
        .iter()
        .zip(seen)
        .filter_map(|(o, on)| on.then_some(*o))
        .collect();
    Some(Value::keyword(canonical.join(&b' ')))
}

fn font_variant(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    if let [w] = words.as_slice()
        && (eq(w, b"normal") || eq(w, b"none"))
    {
        return Some(Value::keyword(lower(w)));
    }
    let mut slots: [Option<Vec<u8>>; 22] = Default::default();
    for w in &words {
        let i = font_variant_slot(w)?;
        if slots[i].is_some() {
            return None;
        }
        slots[i] = Some(w.to_vec());
    }
    if words.is_empty() {
        return None;
    }
    let canonical: Vec<Vec<u8>> = slots.into_iter().flatten().collect();
    Some(Value::keyword(canonical.join(&b' ')))
}

fn white_space(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    let collapse_of = |w: &[u8]| {
        ["collapse", "preserve", "preserve-breaks", "break-spaces"]
            .iter()
            .position(|k| eq(k.as_bytes(), w))
    };
    let wrap_of = |w: &[u8]| ["wrap", "nowrap"].iter().position(|k| eq(k.as_bytes(), w));
    let (collapse, nowrap) = match words.as_slice() {
        [w] if word_is_one_of(w, "normal nowrap pre pre-wrap pre-line break-spaces") => {
            return Some(Value::keyword(lower(w)));
        }
        [w] => (collapse_of(w)?, false),
        [a, b] => match (collapse_of(a), wrap_of(b), collapse_of(b), wrap_of(a)) {
            (Some(c), Some(w), _, _) | (_, _, Some(c), Some(w)) => (c, w == 1),
            _ => return None,
        },
        _ => return None,
    };
    let legacy: &[u8] = match (collapse, nowrap) {
        (0, false) => b"normal",
        (0, true) => b"nowrap",
        (1, false) => b"pre-wrap",
        (1, true) => b"pre",
        (2, false) => b"pre-line",
        (2, true) => b"preserve-breaks nowrap",
        (_, false) => b"break-spaces",
        _ => return None,
    };
    Some(Value::keyword(legacy.to_vec()))
}

fn font_synthesis(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    if words.len() == 1 && eq(words[0], b"none") {
        return Some(Value::keyword(b"none".to_vec()));
    }
    const ORDER: [&[u8]; 5] = [
        b"weight",
        b"style",
        b"oblique-only",
        b"small-caps",
        b"position",
    ];
    let mut seen = [false; 5];
    for w in &words {
        let i = ORDER.iter().position(|o| eq(o, w))?;
        if seen[i] {
            return None;
        }
        seen[i] = true;
    }
    if words.is_empty() || (seen[1] && seen[2]) {
        return None;
    }
    let canonical: Vec<&[u8]> = ORDER
        .iter()
        .zip(seen)
        .filter_map(|(o, on)| on.then_some(*o))
        .collect();
    Some(Value::keyword(canonical.join(&b' ')))
}

fn font_size_adjust(t: &[u8]) -> Option<Value> {
    let words = split_gspace_words(t);
    const BASES: [&[u8]; 5] = [
        b"ex-height",
        b"cap-height",
        b"ch-width",
        b"ic-width",
        b"ic-height",
    ];
    let amount = |w: &[u8]| {
        eq(w, b"from-font")
            || (w.first().is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                && core::str::from_utf8(w)
                    .ok()
                    .and_then(|s| s.parse::<f64>().ok())
                    .is_some_and(|v| v >= 0.0))
    };
    let canonical = match words.as_slice() {
        [w] if eq(w, b"none") => b"none".to_vec(),
        [w] if amount(w) => lower(w),
        [basis, w] if amount(w) => {
            let basis = BASES.iter().find(|b| eq(b, basis))?;
            if *basis == b"ex-height" {
                lower(w)
            } else {
                [*basis, &lower(w)[..]].join(&b' ')
            }
        }
        _ => return None,
    };
    Some(Value::keyword(canonical))
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
    Some(Value::keyword(southstar_numfmt::g(num).into_bytes()))
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
        P::FontStyle => font_style(t),
        P::FontPalette => font_palette(t),
        P::FontVariant => font_variant(t),
        P::TextAlign => keyword_choice(t, "start end left right center justify match-parent"),
        P::TextTransform => text_transform(t),
        P::Contain => contain(t),
        P::VerticalAlign => vertical_align(t),
        P::AlignmentBaseline => keyword_choice(t, ALIGNMENT_BASELINES),
        P::BaselineShift => baseline_shift(t),
        P::WillChange => will_change(t),
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
        | P::WebkitTextFillColor
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
        | P::ColumnHeight
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
        | P::MaskPositionX
        | P::MaskPositionY
        | P::ObjectPositionX
        | P::ObjectPositionY => position_axis(prop, t),
        P::BackgroundSize | P::MaskSize => background_size(t),
        P::BackgroundRepeat | P::MaskRepeat => background_repeat(t),
        P::MaskMode => keyword_choice(t, "alpha luminance match-source"),
        P::MaskOrigin => keyword_choice(
            t,
            "border-box padding-box content-box fill-box stroke-box view-box",
        ),
        P::MaskType => keyword_choice(t, "luminance alpha"),
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
        P::BorderImageSource | P::MaskBorderSource => {
            image_reference(t).or_else(|| keyword_in(t, &[b"none"]))
        }
        P::BorderImageSlice | P::MaskBorderSlice => keyword_text(border_image::slice_canonical(t)),
        P::BorderImageWidth | P::MaskBorderWidth => {
            keyword_text(border_image::quad_canonical(t, true, true))
        }
        P::BorderImageOutset | P::MaskBorderOutset => {
            keyword_text(border_image::quad_canonical(t, false, false))
        }
        P::BorderImageRepeat | P::MaskBorderRepeat => {
            keyword_text(border_image::repeat_canonical(t))
        }
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
        P::FontSynthesis => font_synthesis(t),
        P::FontSizeAdjust => font_size_adjust(t),
        P::FontVariantCaps => font_variant_longhand(t, 4..=4),
        P::FontVariantNumeric => font_variant_longhand(t, 12..=16),
        P::FontVariantEastAsian => font_variant_longhand(t, 17..=19),
        P::FontVariantPosition => font_variant_longhand(t, 20..=20),
        P::FontVariantEmoji => font_variant_longhand(t, 21..=21),
        P::FontOpticalSizing => keyword_in(t, &[b"auto", b"none"]),
        P::FontLanguageOverride => font_language_override(t),
        P::HangingPunctuation => hanging_punctuation(t),
        P::TextAutospace => text_autospace(t),
        P::OverscrollBehaviorX | P::OverscrollBehaviorY => {
            keyword_choice(t, "auto contain none chain")
        }
        P::Interactivity => keyword_choice(t, "auto inert"),
        P::BackgroundBlendMode => blend_mode_list(t),
        P::FontVariantLigatures => {
            if eq(t.trim_ascii(), b"none") {
                Some(Value::keyword(b"none".to_vec()))
            } else {
                font_variant_longhand(t, 0..=3)
            }
        }
        P::FontFeatureSettings => keyword_text(font::feature_settings_canonical(t, true)),
        P::FontVariationSettings => settings_prop(t, font::variation_settings_valid),
        P::TabSize => tab_size(t),
        P::BorderSpacing => border_spacing(t),
        P::WhiteSpace => white_space(t),
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
        P::ClipPath => {
            let raw = contains(t, b"var(") || starts_with_ci(strip(t), b"url(");
            if raw {
                clip_path::valid(t).then(|| Value::keyword(lower_outside_strings(t)))
            } else {
                clip_path::canonical(t, b"border-box").map(Value::keyword)
            }
        }
        P::ShapeOutside => shape_outside(t),
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
