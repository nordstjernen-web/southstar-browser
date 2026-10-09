//! Southstar — expanding one declaration into the longhand declarations css.c stores: `all`, the border, outline, background, mask, position, grid, gap, place, columns, text-decoration, font, flex, list-style, border-radius, inset, margin and padding shorthands, and the legacy and logical aliases.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::animation::{self, Longhand};
use crate::border_image;
use crate::color::parse_color;
use crate::ffi;
use crate::font;
use crate::gradient::text_starts_gradient;
use crate::grid;
use crate::image::{image_value_canonical, text_starts_image_set};
use crate::inline;
use crate::math::math_canonical;
use crate::position;
use crate::prop::Prop;
use crate::property::{
    self, Body, Value, bg_clip_canonical, bg_repeat_canonical, bg_repeat_token, bg_token_is_box,
    c_text, calc_value, contains, eq, length, lower, mask_box_keyword, mask_composite_keyword,
    wide_keyword,
};
use crate::scan::{scan_until, split_ws_limit, split_ws_paren, starts_with_ci, strip, trim_range};
use crate::text::{add_leading_zeros, split_top_level_commas};
use crate::units::{EM, NUMBER, PERCENT, PX};

pub(crate) enum Slot {
    Own(Value),
    Dup(usize),
}

pub(crate) struct Decl {
    pub prop: Prop,
    pub slot: Slot,
}

#[derive(Default)]
struct Out(Vec<Decl>);

impl Out {
    fn push(&mut self, prop: Prop, value: Value) -> usize {
        self.0.push(Decl {
            prop,
            slot: Slot::Own(value),
        });
        self.0.len() - 1
    }

    fn emit(&mut self, prop: Prop, text: &[u8]) -> bool {
        match parse(prop, text) {
            Some(value) => {
                self.push(prop, value);
                true
            }
            None => false,
        }
    }

    fn shared(&mut self, props: &[Prop], value: Value) {
        let first = self.push(props[0], value);
        for &prop in &props[1..] {
            self.0.push(Decl {
                prop,
                slot: Slot::Dup(first),
            });
        }
    }

    fn quad(&mut self, props: [Prop; 4], vals: &[&[u8]]) {
        let top = vals[0];
        let right = vals.get(1).copied().unwrap_or(top);
        let bottom = vals.get(2).copied().unwrap_or(top);
        let left = vals.get(3).copied().unwrap_or(right);
        for (prop, text) in props.into_iter().zip([top, right, bottom, left]) {
            self.emit(prop, text);
        }
    }
}

const BORDER_WIDTHS: [Prop; 4] = [
    Prop::BorderTopWidth,
    Prop::BorderRightWidth,
    Prop::BorderBottomWidth,
    Prop::BorderLeftWidth,
];
const BORDER_COLORS: [Prop; 4] = [
    Prop::BorderTopColor,
    Prop::BorderRightColor,
    Prop::BorderBottomColor,
    Prop::BorderLeftColor,
];
const BORDER_STYLES: [Prop; 4] = [
    Prop::BorderTopStyle,
    Prop::BorderRightStyle,
    Prop::BorderBottomStyle,
    Prop::BorderLeftStyle,
];

struct Side {
    width: Prop,
    color: Prop,
    style: Prop,
}

const fn side(edge: usize) -> Side {
    Side {
        width: BORDER_WIDTHS[edge],
        color: BORDER_COLORS[edge],
        style: BORDER_STYLES[edge],
    }
}

const BORDER_SIDES: [(&[u8], Side); 8] = [
    (b"border-top", side(0)),
    (b"border-right", side(1)),
    (b"border-bottom", side(2)),
    (b"border-left", side(3)),
    (b"border-inline-start", side(3)),
    (b"border-inline-end", side(1)),
    (b"border-block-start", side(0)),
    (b"border-block-end", side(2)),
];

const BORDER_PAIRS: [(&[u8], Prop, Prop); 6] = [
    (
        b"border-block-width",
        Prop::BorderTopWidth,
        Prop::BorderBottomWidth,
    ),
    (
        b"border-inline-width",
        Prop::BorderLeftWidth,
        Prop::BorderRightWidth,
    ),
    (
        b"border-block-style",
        Prop::BorderTopStyle,
        Prop::BorderBottomStyle,
    ),
    (
        b"border-inline-style",
        Prop::BorderLeftStyle,
        Prop::BorderRightStyle,
    ),
    (
        b"border-block-color",
        Prop::BorderTopColor,
        Prop::BorderBottomColor,
    ),
    (
        b"border-inline-color",
        Prop::BorderLeftColor,
        Prop::BorderRightColor,
    ),
];

const ALIASES: [(&[u8], Prop); 41] = [
    (b"grid-row-gap", Prop::RowGap),
    (b"grid-column-gap", Prop::ColumnGap),
    (b"-webkit-user-select", Prop::UserSelect),
    (b"-moz-user-select", Prop::UserSelect),
    (b"inline-size", Prop::Width),
    (b"block-size", Prop::Height),
    (b"min-inline-size", Prop::MinWidth),
    (b"max-inline-size", Prop::MaxWidth),
    (b"min-block-size", Prop::MinHeight),
    (b"max-block-size", Prop::MaxHeight),
    (b"margin-inline-start", Prop::MarginLeft),
    (b"margin-inline-end", Prop::MarginRight),
    (b"margin-block-start", Prop::MarginTop),
    (b"margin-block-end", Prop::MarginBottom),
    (b"padding-inline-start", Prop::PaddingLeft),
    (b"padding-inline-end", Prop::PaddingRight),
    (b"padding-block-start", Prop::PaddingTop),
    (b"padding-block-end", Prop::PaddingBottom),
    (b"inset-inline-start", Prop::Left),
    (b"inset-inline-end", Prop::Right),
    (b"inset-block-start", Prop::Top),
    (b"inset-block-end", Prop::Bottom),
    (b"border-inline-start-width", Prop::BorderLeftWidth),
    (b"border-inline-end-width", Prop::BorderRightWidth),
    (b"border-block-start-width", Prop::BorderTopWidth),
    (b"border-block-end-width", Prop::BorderBottomWidth),
    (b"border-inline-start-style", Prop::BorderLeftStyle),
    (b"border-inline-end-style", Prop::BorderRightStyle),
    (b"border-block-start-style", Prop::BorderTopStyle),
    (b"border-block-end-style", Prop::BorderBottomStyle),
    (b"border-inline-start-color", Prop::BorderLeftColor),
    (b"border-inline-end-color", Prop::BorderRightColor),
    (b"border-block-start-color", Prop::BorderTopColor),
    (b"border-block-end-color", Prop::BorderBottomColor),
    (b"border-start-start-radius", Prop::BorderTopLeftRadius),
    (b"border-start-end-radius", Prop::BorderTopRightRadius),
    (b"border-end-start-radius", Prop::BorderBottomLeftRadius),
    (b"border-end-end-radius", Prop::BorderBottomRightRadius),
    (b"page-break-before", Prop::BreakBefore),
    (b"page-break-after", Prop::BreakAfter),
    (b"page-break-inside", Prop::BreakInside),
];

pub(crate) const ANIMATION_LONGHANDS: [(Prop, Longhand); 11] = [
    (Prop::AnimationName, Longhand::AnimationName),
    (Prop::AnimationDuration, Longhand::AnimationDuration),
    (Prop::AnimationDelay, Longhand::AnimationDelay),
    (
        Prop::AnimationTimingFunction,
        Longhand::AnimationTimingFunction,
    ),
    (
        Prop::AnimationIterationCount,
        Longhand::AnimationIterationCount,
    ),
    (Prop::AnimationDirection, Longhand::AnimationDirection),
    (Prop::AnimationFillMode, Longhand::AnimationFillMode),
    (Prop::AnimationPlayState, Longhand::AnimationPlayState),
    (Prop::AnimationTimeline, Longhand::AnimationTimeline),
    (Prop::AnimationRangeStart, Longhand::AnimationRangeStart),
    (Prop::AnimationRangeEnd, Longhand::AnimationRangeEnd),
];

pub(crate) const TRANSITION_LONGHANDS: [(Prop, Longhand); 5] = [
    (Prop::TransitionProperty, Longhand::TransitionProperty),
    (Prop::TransitionDuration, Longhand::TransitionDuration),
    (Prop::TransitionDelay, Longhand::TransitionDelay),
    (
        Prop::TransitionTimingFunction,
        Longhand::TransitionTimingFunction,
    ),
    (Prop::TransitionBehavior, Longhand::TransitionBehavior),
];

fn parse(prop: Prop, text: &[u8]) -> Option<Value> {
    property::parse(prop, text)
}

fn split_ws(text: &[u8]) -> Vec<&[u8]> {
    split_ws_limit(text, 4)
}

fn join(parts: &[&[u8]], sep: &[u8]) -> Vec<u8> {
    parts.join(sep)
}

fn is_color(tok: &[u8]) -> bool {
    parse_color(&c_text(tok)).is_some()
}

fn is_color_keyword(tok: &[u8]) -> bool {
    eq(tok, b"currentcolor") || eq(tok, b"transparent")
}

fn is_width_keyword(tok: &[u8]) -> bool {
    eq(tok, b"thin") || eq(tok, b"medium") || eq(tok, b"thick")
}

fn chain(values: Vec<Value>) -> Option<Value> {
    let mut rev = values.into_iter().rev();
    let mut head = rev.next()?;
    for mut value in rev {
        value.next_layer = Some(Box::new(head));
        head = value;
    }
    Some(head)
}

pub(crate) fn expand(name: &[u8], text: &[u8]) -> Vec<Decl> {
    let mut out = Out::default();
    match name {
        b"all" => all(&mut out, text),
        b"border-image" | b"-webkit-border-image" => border_image_shorthand(&mut out, text),
        b"border" => border(&mut out, text, None),
        b"border-block" | b"border-inline" => {
            border_axis(&mut out, text, name == b"border-block");
        }
        b"overflow" => {
            if !overflow(&mut out, text) {
                generic(&mut out, name, text);
            }
        }
        b"mask" | b"-webkit-mask" => mask(&mut out, text),
        b"background" => background(&mut out, text),
        b"background-position" => background_position(&mut out, text),
        b"object-position" => object_position(&mut out, text),
        b"grid-template" | b"grid" => grid_template(&mut out, text, name == b"grid"),
        b"gap" | b"grid-gap" => gap(&mut out, text),
        b"grid-area" | b"grid-column" | b"grid-row" => {
            if grid_placement(&mut out, name, text) {
                generic(&mut out, name, text);
            }
        }
        b"place-items" | b"place-self" | b"place-content" => place(&mut out, name, text),
        b"columns" => columns(&mut out, text),
        b"outline" => rule(
            &mut out,
            text,
            Prop::OutlineWidth,
            Prop::OutlineStyle,
            Prop::OutlineColor,
        ),
        b"column-rule" => rule(
            &mut out,
            text,
            Prop::ColumnRuleWidth,
            Prop::ColumnRuleStyle,
            Prop::ColumnRuleColor,
        ),
        b"text-decoration" | b"text-decoration-line" => {
            text_decoration(&mut out, text, name == b"text-decoration-line");
        }
        b"font" => font_shorthand(&mut out, text),
        b"flex" => flex(&mut out, text),
        b"flex-flow" => flex_flow(&mut out, text),
        b"list-style" => list_style(&mut out, text),
        b"border-radius" => border_radius(&mut out, text),
        b"margin-block" => logical_pair(&mut out, text, Prop::MarginTop, Prop::MarginBottom),
        b"margin-inline" => logical_pair(&mut out, text, Prop::MarginLeft, Prop::MarginRight),
        b"padding-block" => logical_pair(&mut out, text, Prop::PaddingTop, Prop::PaddingBottom),
        b"padding-inline" => logical_pair(&mut out, text, Prop::PaddingLeft, Prop::PaddingRight),
        b"inset-block" => logical_pair(&mut out, text, Prop::Top, Prop::Bottom),
        b"inset-inline" => logical_pair(&mut out, text, Prop::Left, Prop::Right),
        b"inset" => box_quad(
            &mut out,
            text,
            [Prop::Top, Prop::Right, Prop::Bottom, Prop::Left],
        ),
        b"text-wrap" | b"text-wrap-mode" => text_wrap(&mut out, text),
        b"margin" => box_quad(
            &mut out,
            text,
            [
                Prop::MarginTop,
                Prop::MarginRight,
                Prop::MarginBottom,
                Prop::MarginLeft,
            ],
        ),
        b"padding" => box_quad(
            &mut out,
            text,
            [
                Prop::PaddingTop,
                Prop::PaddingRight,
                Prop::PaddingBottom,
                Prop::PaddingLeft,
            ],
        ),
        b"scroll-margin" => box_quad(
            &mut out,
            text,
            [
                Prop::ScrollMarginTop,
                Prop::ScrollMarginRight,
                Prop::ScrollMarginBottom,
                Prop::ScrollMarginLeft,
            ],
        ),
        b"scroll-padding" => box_quad(
            &mut out,
            text,
            [
                Prop::ScrollPaddingTop,
                Prop::ScrollPaddingRight,
                Prop::ScrollPaddingBottom,
                Prop::ScrollPaddingLeft,
            ],
        ),
        b"border-width" => box_quad(&mut out, text, BORDER_WIDTHS),
        b"border-color" => box_quad(&mut out, text, BORDER_COLORS),
        b"border-style" => box_quad(&mut out, text, BORDER_STYLES),
        b"container" => container(&mut out, text),
        b"animation-range" => animation_range(&mut out, text),
        _ => {
            if let Some((_, side)) = BORDER_SIDES.iter().find(|(n, _)| *n == name) {
                border(&mut out, text, Some(side));
            } else if let Some(&(_, a, b)) = BORDER_PAIRS.iter().find(|(n, ..)| *n == name) {
                border_pair(&mut out, text, a, b);
            } else if let Some(&(alias, prop)) = ALIASES.iter().find(|(n, _)| *n == name) {
                let page = alias.starts_with(b"page-break-") && eq(text, b"always");
                out.emit(prop, if page { b"page" } else { text });
            } else {
                generic(&mut out, name, text);
            }
        }
    }
    out.0
}

fn all(out: &mut Out, text: &[u8]) {
    let Some(wide) = wide_keyword(text) else {
        return;
    };
    let props: Vec<Prop> = Prop::ALL
        .iter()
        .copied()
        .filter(|p| !matches!(p, Prop::Direction | Prop::UnicodeBidi))
        .collect();
    out.shared(&props, wide);
}

fn generic(out: &mut Out, name: &[u8], text: &[u8]) {
    let Some(prop) = ffi::prop_named(name) else {
        return;
    };
    let Some(value) = parse(prop, text) else {
        return;
    };
    let index = out.push(prop, value);
    match prop {
        Prop::Animation => anim_longhands(out, index, &ANIMATION_LONGHANDS),
        Prop::Transition => anim_longhands(out, index, &TRANSITION_LONGHANDS),
        _ => {}
    }
}

fn anim_longhands(out: &mut Out, index: usize, longhands: &[(Prop, Longhand)]) {
    let Slot::Own(Value {
        body: Body::Anim(list),
        ..
    }) = &out.0[index].slot
    else {
        for &(prop, _) in longhands {
            out.0.push(Decl {
                prop,
                slot: Slot::Dup(index),
            });
        }
        return;
    };
    let texts: Vec<Option<Vec<u8>>> = longhands
        .iter()
        .map(|&(prop, lh)| {
            let single = matches!(
                prop,
                Prop::AnimationTimeline | Prop::AnimationRangeStart | Prop::AnimationRangeEnd
            );
            let count = if single {
                list.len().min(1)
            } else {
                list.len()
            };
            let parts: Vec<Vec<u8>> = list[..count]
                .iter()
                .map(|e| animation::entry_longhand_text(e, lh).unwrap_or_default())
                .collect();
            (!list.is_empty()).then(|| parts.join(&b", "[..]))
        })
        .collect();
    for (&(prop, _), text) in longhands.iter().zip(texts) {
        let value = text
            .and_then(|t| parse(prop, &t))
            .unwrap_or_else(|| Value::keyword(&b"initial"[..]));
        out.push(prop, value);
    }
}

fn border_image_initial(out: &mut Out) {
    out.emit(Prop::BorderImageSource, b"none");
    out.emit(Prop::BorderImageSlice, b"100%");
    out.emit(Prop::BorderImageWidth, b"1");
    out.emit(Prop::BorderImageOutset, b"0");
    out.emit(Prop::BorderImageRepeat, b"stretch");
}

fn append_word(target: &mut Vec<u8>, word: &[u8]) {
    if !target.is_empty() {
        target.push(b' ');
    }
    target.extend_from_slice(word);
}

fn border_image_shorthand(out: &mut Out, text: &[u8]) {
    if let Some(wide) = wide_keyword(text) {
        out.shared(
            &[
                Prop::BorderImageSource,
                Prop::BorderImageSlice,
                Prop::BorderImageWidth,
                Prop::BorderImageOutset,
                Prop::BorderImageRepeat,
            ],
            wide,
        );
        return;
    }
    let toks = border_image::shorthand_tokens(text);
    let (mut slice, mut width, mut outset, mut repeat) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut source: Option<&[u8]> = None;
    let (mut slash, mut repeats) = (0, 0);
    let mut slice_closed = false;
    let mut ok = !toks.is_empty();
    for tok in &toks {
        let tok = tok.as_slice();
        if tok == b"/" {
            if slice_closed || slash >= 2 || (slash == 0 && slice.is_empty()) {
                ok = false;
                break;
            }
            slash += 1;
            continue;
        }
        if slash > 0 {
            if let Some(comp) = border_image::length_serialize(tok, slash == 1, slash == 1) {
                append_word(if slash == 1 { &mut width } else { &mut outset }, &comp);
                continue;
            }
            if if slash == 1 {
                width.is_empty()
            } else {
                outset.is_empty()
            } {
                ok = false;
                break;
            }
            slice_closed = true;
            slash = 0;
        }
        if border_image::tile_keyword(tok) {
            if repeats >= 2 {
                ok = false;
                break;
            }
            append_word(&mut repeat, tok);
            repeats += 1;
            continue;
        }
        if wide_keyword(tok).is_some() {
            ok = false;
            break;
        }
        if parse(Prop::BorderImageSource, tok).is_some() {
            if source.is_some() {
                ok = false;
                break;
            }
            source = Some(tok);
            continue;
        }
        if slice_closed {
            ok = false;
            break;
        }
        append_word(&mut slice, tok);
    }
    if ok && !slice.is_empty() {
        ok = parse(Prop::BorderImageSlice, &slice).is_some();
    }
    if ok && (!width.is_empty() || !outset.is_empty()) {
        if !width.is_empty() && parse(Prop::BorderImageWidth, &width).is_none() {
            ok = false;
        }
        if !outset.is_empty() && parse(Prop::BorderImageOutset, &outset).is_none() {
            ok = false;
        }
    }
    if !ok {
        return;
    }
    let or = |text: &Vec<u8>, initial: &'static [u8]| -> Vec<u8> {
        if text.is_empty() {
            initial.to_vec()
        } else {
            text.clone()
        }
    };
    out.emit(Prop::BorderImageSource, source.unwrap_or(b"none"));
    out.emit(Prop::BorderImageSlice, &or(&slice, b"100%"));
    out.emit(Prop::BorderImageWidth, &or(&width, b"1"));
    out.emit(Prop::BorderImageOutset, &or(&outset, b"0"));
    out.emit(Prop::BorderImageRepeat, &or(&repeat, b"stretch"));
}

fn border_shorthand_valid(text: &[u8], style_prop: Prop) -> bool {
    if wide_keyword(text).is_some() {
        return true;
    }
    let tokens = split_ws_limit(text, 5);
    let mut ok = (1..=3).contains(&tokens.len());
    let (mut saw_color, mut saw_width, mut saw_style) = (false, false, false);
    for tok in tokens {
        if !ok {
            break;
        }
        if is_color(tok) || is_color_keyword(tok) {
            ok = !saw_color;
            saw_color = true;
        } else if is_width_keyword(tok) {
            ok = !saw_width;
            saw_width = true;
        } else if let Some((num, unit)) = length(tok) {
            ok = !saw_width && num >= 0.0 && unit != PERCENT && (unit != NUMBER || num == 0.0);
            saw_width = true;
        } else if calc_value(tok).is_some() {
            ok = !saw_width;
            saw_width = true;
        } else {
            ok = !saw_style && parse(style_prop, tok).is_some();
            saw_style = true;
        }
    }
    ok
}

const BORDER_PARTS: [[Prop; 4]; 3] = [BORDER_COLORS, BORDER_WIDTHS, BORDER_STYLES];
const BORDER_INITIALS: [&[u8]; 3] = [b"currentcolor", b"medium", b"none"];

fn border_part(out: &mut Out, side: Option<&Side>, part: usize, text: &[u8]) {
    match side {
        Some(side) => {
            out.emit([side.color, side.width, side.style][part], text);
        }
        None => out.quad(BORDER_PARTS[part], &[text, text, text, text]),
    }
}

fn border(out: &mut Out, text: &[u8], side: Option<&Side>) {
    if !contains(text, b"var(") && !border_shorthand_valid(text, Prop::BorderTopStyle) {
        return;
    }
    let tokens = split_ws(text);
    if tokens.is_empty() {
        return;
    }
    let mut seen = [false; 3];
    for &tok in &tokens {
        let part = if is_color(tok) || is_color_keyword(tok) {
            0
        } else if length(tok).is_some() || is_width_keyword(tok) {
            1
        } else {
            2
        };
        seen[part] = true;
        border_part(out, side, part, tok);
    }
    for part in 0..3 {
        if !seen[part] {
            border_part(out, side, part, BORDER_INITIALS[part]);
        }
    }
    if side.is_none() {
        border_image_initial(out);
    }
}

fn border_axis(out: &mut Out, text: &[u8], block: bool) {
    let edges = if block { [0, 2] } else { [3, 1] };
    for tok in split_ws(text) {
        let props = if is_color(tok) || is_color_keyword(tok) {
            BORDER_COLORS
        } else if length(tok).is_some() {
            BORDER_WIDTHS
        } else {
            BORDER_STYLES
        };
        for edge in edges {
            out.emit(props[edge], tok);
        }
    }
}

fn border_pair(out: &mut Out, text: &[u8], a: Prop, b: Prop) {
    let tokens = split_ws(text);
    if let Some(&first) = tokens.first() {
        out.emit(a, first);
        out.emit(b, tokens.get(1).copied().unwrap_or(first));
    }
}

fn overflow(out: &mut Out, text: &[u8]) -> bool {
    let tokens = split_ws_limit(text, 3);
    let n = tokens.len();
    if !(n == 2 || (n == 1 && !tokens[0].contains(&b'('))) {
        return false;
    }
    out.emit(Prop::OverflowX, tokens[0]);
    out.emit(Prop::OverflowY, tokens[n - 1]);
    true
}

fn mask(out: &mut Out, text: &[u8]) {
    let props = [Prop::MaskImage, Prop::MaskClip, Prop::MaskComposite];
    if let Some(wide) = wide_keyword(text) {
        out.shared(&props, wide);
        return;
    }
    let layers = split_top_level_commas(text);
    if layers.is_empty() {
        return;
    }
    let mut chains: [Vec<Value>; 3] = Default::default();
    for layer in &layers {
        if !mask_layer(layer, &mut chains) {
            return;
        }
    }
    for (prop, values) in props.into_iter().zip(chains) {
        if let Some(head) = chain(values) {
            out.push(prop, head);
        }
    }
}

fn mask_layer(layer: &[u8], chains: &mut [Vec<Value>; 3]) -> bool {
    let toks = split_ws_limit(layer, 24);
    let mut image: Option<&[u8]> = None;
    let mut boxes: Vec<&'static [u8]> = Vec::new();
    let mut op: Option<&'static [u8]> = None;
    for &tok in &toks {
        let found_box = mask_box_keyword(tok);
        let comp = mask_composite_keyword(tok);
        if let (Some(b), true) = (found_box, boxes.len() < 2) {
            boxes.push(b);
        } else if let (Some(c), None) = (comp, op) {
            op = Some(c);
        } else if image.is_none() && (tok.contains(&b'(') || eq(tok, b"none")) {
            image = Some(tok);
        }
    }
    let Some(iv) = parse(Prop::MaskImage, image.unwrap_or(b"none")) else {
        return false;
    };
    let clip = boxes.last().copied().unwrap_or(b"border-box");
    chains[0].push(iv);
    chains[1].push(Value::keyword(clip));
    chains[2].push(Value::keyword(op.unwrap_or(b"add")));
    true
}

#[derive(Default)]
struct BgLayer {
    image: Option<Vec<u8>>,
    pos_x: Option<Vec<u8>>,
    pos_y: Option<Vec<u8>>,
    size: Option<Vec<u8>>,
    repeat: Option<Vec<u8>>,
    attachment: Option<Vec<u8>>,
    origin: Option<Vec<u8>>,
    clip: Option<Vec<u8>>,
}

impl BgLayer {
    fn field(&self, f: usize) -> Option<&[u8]> {
        [
            &self.image,
            &self.pos_x,
            &self.pos_y,
            &self.size,
            &self.repeat,
            &self.attachment,
            &self.origin,
            &self.clip,
        ][f]
            .as_deref()
    }
}

fn bg_token_is_length_like(tok: &[u8], nonnegative: bool) -> bool {
    match length(tok) {
        Some((num, unit)) => (unit != NUMBER || num == 0.0) && (!nonnegative || num >= 0.0),
        None => calc_value(tok).is_some(),
    }
}

fn bg_token_is_position(tok: &[u8]) -> bool {
    [&b"left"[..], b"center", b"right", b"top", b"bottom"]
        .iter()
        .any(|k| eq(tok, k))
        || bg_token_is_length_like(tok, false)
}

fn bg_token_is_image(tok: &[u8]) -> bool {
    eq(tok, b"none")
        || starts_with_ci(tok, b"url(")
        || text_starts_gradient(tok)
        || text_starts_image_set(tok)
}

fn bg_layer_tokens(text: &[u8]) -> Vec<&[u8]> {
    let mut toks = Vec::new();
    for t in split_ws_limit(text, 24) {
        let slash = if t.contains(&b'(') {
            None
        } else {
            t.iter().position(|&c| c == b'/')
        };
        match slash {
            None => toks.push(t),
            Some(s) => {
                if s > 0 {
                    toks.push(&t[..s]);
                }
                toks.push(&b"/"[..]);
                if s + 1 < t.len() {
                    toks.push(&t[s + 1..]);
                }
            }
        }
    }
    toks
}

fn bg_layer_parse(
    text: &[u8],
    final_layer: bool,
    out: &mut BgLayer,
    color: &mut Option<Vec<u8>>,
) -> bool {
    let toks = bg_layer_tokens(text);
    let mut ok = !toks.is_empty();
    let (mut repeat_a, mut repeat_b): (&[u8], Option<&[u8]>) = (b"", None);
    let (mut n_repeat, mut n_box, mut n_pos) = (0, 0, 0);
    let mut pos_closed = false;
    let mut pos: Option<Vec<u8>> = None;
    let mut size: Option<Vec<u8>> = None;
    let mut clip_only: Option<Vec<u8>> = None;
    let mut i = 0;
    while ok && i < toks.len() {
        let tok = toks[i];
        if tok == b"/" {
            if pos.is_none() || pos_closed || size.is_some() {
                ok = false;
                break;
            }
            pos_closed = true;
            let size = size.insert(Vec::new());
            let mut n_size = 0;
            while i + 1 < toks.len() {
                let nx = toks[i + 1];
                if n_size == 0 && (eq(nx, b"cover") || eq(nx, b"contain")) {
                    size.extend_from_slice(&lower(nx));
                    i += 1;
                    n_size = 2;
                    break;
                }
                if n_size < 2 && (eq(nx, b"auto") || bg_token_is_length_like(nx, true)) {
                    append_word(size, nx);
                    i += 1;
                    n_size += 1;
                    continue;
                }
                break;
            }
            if n_size == 0 {
                ok = false;
            }
            i += 1;
            continue;
        }
        if bg_token_is_position(tok) {
            if pos_closed || n_pos >= 4 {
                ok = false;
                break;
            }
            append_word(pos.get_or_insert_with(Vec::new), tok);
            n_pos += 1;
            i += 1;
            continue;
        }
        if pos.is_some() {
            pos_closed = true;
        }
        if bg_token_is_image(tok) {
            if out.image.is_some() {
                ok = false;
                break;
            }
            out.image = Some(tok.to_vec());
        } else if bg_repeat_token(tok, true) {
            let axis = starts_with_ci(tok, b"repeat-");
            if n_repeat >= 2 || (n_repeat == 1 && (axis || starts_with_ci(repeat_a, b"repeat-"))) {
                ok = false;
                break;
            }
            if n_repeat == 0 {
                repeat_a = tok;
            } else {
                repeat_b = Some(tok);
            }
            n_repeat += 1;
        } else if eq(tok, b"scroll") || eq(tok, b"fixed") || eq(tok, b"local") {
            if out.attachment.is_some() {
                ok = false;
                break;
            }
            out.attachment = Some(lower(tok));
        } else if bg_token_is_box(tok) {
            if n_box >= 2 {
                ok = false;
                break;
            }
            if n_box == 0 {
                out.origin = Some(lower(tok));
            }
            out.clip = Some(lower(tok));
            n_box += 1;
        } else if eq(tok, b"text") || eq(tok, b"border-area") {
            if n_box >= 2 || clip_only.as_deref().is_some_and(|c| c.contains(&b' ')) {
                ok = false;
                break;
            }
            let mut joined = clip_only.take().unwrap_or_default();
            append_word(&mut joined, &lower(tok));
            clip_only = bg_clip_canonical(&joined);
            if clip_only.is_none() {
                ok = false;
                break;
            }
        } else if is_color(tok) || eq(tok, b"currentcolor") {
            if !final_layer || color.is_some() {
                ok = false;
                break;
            }
            *color = Some(tok.to_vec());
        } else {
            ok = false;
        }
        i += 1;
    }
    if ok {
        if let Some(pos) = &pos {
            match position::canonical_ex(pos, true, true) {
                Some(canon) => {
                    let (x, y) = position_split_specified(&canon);
                    out.pos_x = Some(x);
                    out.pos_y = Some(y);
                }
                None => ok = false,
            }
        }
    }
    if ok {
        if let Some(size) = &size {
            if parse(Prop::BackgroundSize, size).is_some() {
                out.size = Some(add_leading_zeros(size));
            } else {
                ok = false;
            }
        }
    }
    if ok && n_repeat > 0 {
        out.repeat = Some(bg_repeat_canonical(repeat_a, repeat_b));
    }
    if ok {
        if let Some(clip) = clip_only {
            out.clip = Some(clip);
            if out.origin.is_none() {
                out.origin = Some(b"border-box".to_vec());
            }
        }
    }
    if ok {
        if let Some(img) = &out.image {
            match image_value_canonical(img) {
                Some(canon) => out.image = Some(canon),
                None => ok = eq(img, b"none"),
            }
        }
    }
    ok
}

fn background(out: &mut Out, text: &[u8]) {
    const FIELDS: [(Prop, &[u8]); 8] = [
        (Prop::BackgroundImage, b"none"),
        (Prop::BackgroundPositionX, b"0%"),
        (Prop::BackgroundPositionY, b"0%"),
        (Prop::BackgroundSize, b"auto"),
        (Prop::BackgroundRepeat, b"repeat"),
        (Prop::BackgroundAttachment, b"scroll"),
        (Prop::BackgroundOrigin, b"padding-box"),
        (Prop::BackgroundClip, b"border-box"),
    ];
    if let Some(wide) = wide_keyword(text) {
        let mut props: Vec<Prop> = FIELDS.iter().map(|&(prop, _)| prop).collect();
        props.push(Prop::BackgroundColor);
        out.shared(&props, wide);
        return;
    }
    let layers = split_top_level_commas(text);
    if layers.is_empty() {
        return;
    }
    let mut parsed: Vec<BgLayer> = layers.iter().map(|_| BgLayer::default()).collect();
    let mut color = None;
    for (i, layer) in layers.iter().enumerate() {
        if !bg_layer_parse(layer, i + 1 == layers.len(), &mut parsed[i], &mut color) {
            return;
        }
    }
    for (f, &(prop, initial)) in FIELDS.iter().enumerate() {
        let mut values = Vec::new();
        for layer in &parsed {
            let text = layer.field(f).unwrap_or(initial);
            let Some(mut value) = parse(prop, text) else {
                return;
            };
            value.specified = Some(text.to_vec());
            values.push(value);
        }
        if let Some(head) = chain(values) {
            out.push(prop, head);
        }
    }
    out.emit(
        Prop::BackgroundColor,
        color.as_deref().unwrap_or(b"transparent"),
    );
}

pub(crate) fn position_split_specified(canon: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let tok = split_ws_limit(canon, 4);
    match tok.len() {
        4 => (join(&tok[..2], b" "), join(&tok[2..], b" ")),
        3 => {
            if position::is_keyword(tok[1]) {
                (tok[0].to_vec(), join(&tok[1..], b" "))
            } else {
                (join(&tok[..2], b" "), tok[2].to_vec())
            }
        }
        2 => (tok[0].to_vec(), tok[1].to_vec()),
        1 => (tok[0].to_vec(), b"center".to_vec()),
        _ => (b"center".to_vec(), b"center".to_vec()),
    }
}

fn position_wide(out: &mut Out, text: &[u8], x: Prop, y: Prop) -> bool {
    let Some(wide) = wide_keyword(text) else {
        return false;
    };
    out.shared(&[x, y], wide);
    true
}

fn background_position(out: &mut Out, text: &[u8]) {
    if position_wide(
        out,
        text,
        Prop::BackgroundPositionX,
        Prop::BackgroundPositionY,
    ) {
        return;
    }
    let (mut xs_chain, mut ys_chain) = (Vec::new(), Vec::new());
    let end = text.len();
    let mut p = 0;
    while p < end {
        let (seg, term) = scan_until(text, p, end, b",");
        let layer = trim_range(text, p, seg);
        p = if term == b',' { seg + 1 } else { seg };
        let Some(canon) = position::canonical_ex(layer, true, true) else {
            xs_chain.clear();
            ys_chain.clear();
            break;
        };
        let (xs, ys) = position::split(layer);
        let (sx, sy) = position_split_specified(&canon);
        if let Some(mut v) = parse(Prop::BackgroundPositionX, &xs) {
            v.specified = Some(sx);
            xs_chain.push(v);
        }
        if let Some(mut v) = parse(Prop::BackgroundPositionY, &ys) {
            v.specified = Some(sy);
            ys_chain.push(v);
        }
    }
    if let Some(head) = chain(xs_chain) {
        out.push(Prop::BackgroundPositionX, head);
    }
    if let Some(head) = chain(ys_chain) {
        out.push(Prop::BackgroundPositionY, head);
    }
}

fn object_position(out: &mut Out, text: &[u8]) {
    if position_wide(out, text, Prop::ObjectPositionX, Prop::ObjectPositionY) {
        return;
    }
    if position::canonical_ex(text, true, false).is_none() {
        return;
    }
    let (xs, ys) = position::split(text);
    out.emit(Prop::ObjectPositionX, &xs);
    out.emit(Prop::ObjectPositionY, &ys);
}

fn grid_template(out: &mut Out, text: &[u8], full: bool) {
    const PROPS: [Prop; 6] = [
        Prop::GridTemplateRows,
        Prop::GridTemplateColumns,
        Prop::GridTemplateAreas,
        Prop::GridAutoFlow,
        Prop::GridAutoRows,
        Prop::GridAutoColumns,
    ];
    let n = if full { 6 } else { 3 };
    let parts: Vec<Vec<u8>> = if wide_keyword(text).is_some() {
        vec![text.to_vec(); n]
    } else if full {
        match grid::shorthand_parse(text) {
            Some(shorthand) => shorthand.values.to_vec(),
            None => return,
        }
    } else {
        match grid::template_parse(text) {
            Some(t) => vec![t.rows, t.cols, t.areas],
            None => return,
        }
    };
    for (prop, part) in PROPS.into_iter().zip(&parts) {
        out.emit(prop, part);
    }
}

fn gap(out: &mut Out, text: &[u8]) {
    let tokens = split_ws(text);
    if let Some(&row) = tokens.first() {
        out.emit(Prop::RowGap, row);
        out.emit(Prop::ColumnGap, tokens.get(1).copied().unwrap_or(row));
    }
}

fn grid_placement(out: &mut Out, name: &[u8], text: &[u8]) -> bool {
    let area = name[5] == b'a';
    let props: &[Prop] = if area {
        &[
            Prop::GridRowStart,
            Prop::GridColumnStart,
            Prop::GridRowEnd,
            Prop::GridColumnEnd,
        ]
    } else if name[5] == b'c' {
        &[Prop::GridColumnStart, Prop::GridColumnEnd]
    } else {
        &[Prop::GridRowStart, Prop::GridRowEnd]
    };
    let parts: Vec<Vec<u8>> = if wide_keyword(text).is_some() {
        vec![text.to_vec(); props.len()]
    } else {
        grid::placement_expand(text, area)
            .map(|parts| parts.into_iter().map(|(part, _)| part).collect())
            .unwrap_or_default()
    };
    for (&prop, part) in props.iter().zip(&parts) {
        out.emit(prop, part);
    }
    area && !parts.is_empty()
}

fn place(out: &mut Out, name: &[u8], text: &[u8]) {
    let (ap, jp) = match name {
        b"place-items" => (Prop::AlignItems, Prop::JustifyItems),
        b"place-self" => (Prop::AlignSelf, Prop::JustifySelf),
        _ => (Prop::AlignContent, Prop::JustifyContent),
    };
    const SPLITS: [[usize; 2]; 5] = [[0, 0], [1, 0], [2, 1], [1, 2], [2, 0]];
    let tokens = split_ws(text);
    let n = tokens.len();
    if n == 0 {
        return;
    }
    for k in SPLITS[n] {
        if k == 0 {
            break;
        }
        let first = join(&tokens[..k], b" ");
        let second = if k >= n {
            first.clone()
        } else {
            join(&tokens[k..], b" ")
        };
        let Some(av) = parse(ap, &first) else {
            continue;
        };
        if let Some(jv) = parse(jp, &second) {
            out.push(ap, av);
            out.push(jp, jv);
            return;
        }
    }
}

fn columns(out: &mut Out, text: &[u8]) {
    for tok in split_ws(text) {
        if let Some((_, unit)) = length(tok) {
            let prop = if unit == NUMBER {
                Prop::ColumnCount
            } else {
                Prop::ColumnWidth
            };
            out.emit(prop, tok);
        }
    }
}

fn rule(out: &mut Out, text: &[u8], width: Prop, style: Prop, color: Prop) {
    if !contains(text, b"var(") && !border_shorthand_valid(text, style) {
        return;
    }
    let tokens = split_ws(text);
    let (mut saw_c, mut saw_w, mut saw_s) = (false, false, false);
    for &tok in &tokens {
        if is_color(tok) || is_color_keyword(tok) {
            saw_c |= out.emit(color, tok);
        } else if length(tok).is_some() || is_width_keyword(tok) {
            saw_w |= out.emit(width, tok);
        } else {
            saw_s |= out.emit(style, tok);
        }
    }
    let wide = tokens.len() == 1 && wide_keyword(tokens[0]).is_some();
    if tokens.is_empty() || wide {
        return;
    }
    for (prop, initial, seen) in [
        (color, &b"currentcolor"[..], saw_c),
        (width, b"medium", saw_w),
        (style, b"none", saw_s),
    ] {
        if !seen {
            out.emit(prop, initial);
        }
    }
}

fn text_decoration(out: &mut Out, text: &[u8], line_only: bool) {
    let mut lines = Vec::new();
    for tk in split_ws(text) {
        if [&b"underline"[..], b"overline", b"line-through", b"none"]
            .iter()
            .any(|k| eq(tk, k))
        {
            append_word(&mut lines, &lower(tk));
        } else if line_only {
            continue;
        } else if [&b"solid"[..], b"double", b"dotted", b"dashed", b"wavy"]
            .iter()
            .any(|k| eq(tk, k))
        {
            out.push(Prop::TextDecorationStyle, Value::keyword(lower(tk)));
        } else if let Some(rgba) = parse_color(&c_text(tk)) {
            out.push(Prop::TextDecorationColor, Value::of(Body::Color(rgba)));
        }
    }
    if !lines.is_empty() {
        out.push(Prop::TextDecoration, Value::keyword(lines));
    }
}

fn font_size_value(size: &[u8]) -> Option<Value> {
    let kw = font::size_keyword_px(size);
    if kw > 0.0 {
        Some(Value::length(kw, PX))
    } else if eq(size, b"larger") {
        Some(Value::length(1.2, EM))
    } else if eq(size, b"smaller") {
        Some(Value::length(0.833333333333, EM))
    } else {
        parse(Prop::FontSize, size)
    }
}

fn font_shorthand(out: &mut Out, text: &[u8]) {
    if let Some(wide) = wide_keyword(text) {
        out.shared(
            &[
                Prop::FontStyle,
                Prop::FontVariant,
                Prop::FontWeight,
                Prop::FontStretch,
                Prop::FontKerning,
                Prop::FontVariantLigatures,
                Prop::FontFeatureSettings,
                Prop::FontVariationSettings,
                Prop::FontSize,
                Prop::LineHeight,
                Prop::FontFamily,
            ],
            wide,
        );
        return;
    }
    let text = strip(text);
    let system = lower(text);
    if [
        &b"caption"[..],
        b"icon",
        b"menu",
        b"message-box",
        b"small-caption",
        b"status-bar",
    ]
    .contains(&system.as_slice())
    {
        for (prop, value) in [
            (Prop::FontStyle, &b"normal"[..]),
            (Prop::FontVariant, b"normal"),
            (Prop::FontWeight, b"normal"),
            (Prop::FontStretch, b"normal"),
            (Prop::LineHeight, b"normal"),
            (Prop::FontFamily, b"system-ui"),
        ] {
            out.push(prop, Value::keyword(value));
        }
        out.push(Prop::FontSize, Value::length(13.3333, PX));
        return;
    }
    if font::shorthand_canonical(text).is_none() {
        return;
    }
    let tokens = split_ws_paren(text, 24);
    let n = tokens.len();
    let size_idx = tokens
        .iter()
        .position(|tok| font::shorthand_is_size_token(tok));
    let mark = out.0.len();
    for prop in [
        Prop::FontStyle,
        Prop::FontVariant,
        Prop::FontWeight,
        Prop::FontStretch,
        Prop::LineHeight,
    ] {
        out.push(prop, Value::keyword(&b"normal"[..]));
    }
    for &t in &tokens[..size_idx.unwrap_or(0)] {
        let found: Option<(Prop, &[u8])> = if eq(t, b"italic") || eq(t, b"oblique") {
            Some((Prop::FontStyle, b"italic"))
        } else if eq(t, b"bold") || eq(t, b"bolder") || eq(t, b"lighter") {
            Some((Prop::FontWeight, t))
        } else if t.first().is_some_and(u8::is_ascii_digit) {
            length(t)
                .filter(|&(num, unit)| unit == NUMBER && (1.0..=1000.0).contains(&num))
                .map(|_| (Prop::FontWeight, t))
        } else if eq(t, b"small-caps") {
            Some((Prop::FontVariant, b"small-caps"))
        } else if font::is_stretch_keyword(t) {
            Some((Prop::FontStretch, t))
        } else {
            None
        };
        if let Some((prop, kw)) = found {
            out.push(prop, Value::keyword(lower(kw)));
        }
    }
    let mut size_ok = size_idx.is_some();
    let mut family = None;
    if let Some(si) = size_idx {
        let size_tok = tokens[si];
        let slash = font::shorthand_slash(size_tok);
        let size_only = slash.map_or(size_tok, |s| &size_tok[..s]);
        let mut lh_text: Option<&[u8]> = None;
        let mut family_start = si + 1;
        if let Some(s) = slash {
            if s + 1 < size_tok.len() {
                lh_text = Some(&size_tok[s + 1..]);
            } else if family_start < n {
                lh_text = Some(tokens[family_start]);
                family_start += 1;
            }
        } else if family_start < n && tokens[family_start].first() == Some(&b'/') {
            if tokens[family_start].len() > 1 {
                lh_text = Some(&tokens[family_start][1..]);
            } else if family_start + 1 < n {
                family_start += 1;
                lh_text = Some(tokens[family_start]);
            }
            family_start += 1;
        }
        match font_size_value(size_only) {
            Some(v) => {
                out.push(Prop::FontSize, v);
            }
            None => size_ok = false,
        }
        if let Some(lh) = lh_text.filter(|lh| !eq(lh, b"normal")) {
            if !out.emit(Prop::LineHeight, lh) {
                size_ok = false;
            }
        }
        if family_start < n {
            family = Some(join(&tokens[family_start..], b" "));
        }
    }
    let mut family_ok = false;
    if let Some(family) = family {
        for (prop, value) in [
            (Prop::FontKerning, &b"auto"[..]),
            (Prop::FontVariantLigatures, b"normal"),
            (Prop::FontFeatureSettings, b"normal"),
            (Prop::FontVariationSettings, b"normal"),
        ] {
            out.push(prop, Value::keyword(value));
        }
        if let Some(canon) = font::family_canonical(&family) {
            out.push(Prop::FontFamily, Value::keyword(canon));
            family_ok = true;
        }
    }
    if !family_ok || !size_ok {
        out.0.truncate(mark);
    }
}

fn flex(out: &mut Out, text: &[u8]) {
    if let Some(wide) = wide_keyword(text) {
        out.shared(&[Prop::FlexGrow, Prop::FlexShrink, Prop::FlexBasis], wide);
        return;
    }
    let (mut grow, mut shrink) = (0.0, 1.0);
    let mut basis: Option<Vec<u8>> = None;
    let mut keyword_set = false;
    let mut numerics = 0;
    for t in split_ws(text) {
        if eq(t, b"none") {
            grow = 0.0;
            shrink = 0.0;
            keyword_set = true;
            basis = Some(b"auto".to_vec());
            break;
        }
        if eq(t, b"auto") {
            if numerics == 0 {
                grow = 1.0;
                shrink = 1.0;
            }
            basis = Some(b"auto".to_vec());
            continue;
        }
        if eq(t, b"initial") {
            grow = 0.0;
            shrink = 1.0;
            keyword_set = true;
            basis = Some(b"auto".to_vec());
            continue;
        }
        if [&b"calc("[..], b"min(", b"max(", b"clamp("]
            .iter()
            .any(|f| starts_with_ci(t, f))
        {
            basis = Some(t.to_vec());
            continue;
        }
        match length(t) {
            Some((_, unit)) if unit != NUMBER => basis = Some(t.to_vec()),
            Some((num, _)) => {
                match numerics {
                    0 => grow = num,
                    1 => shrink = num,
                    2 => basis = Some(ffi::format_double(c"%g", num)),
                    _ => {}
                }
                numerics += 1;
            }
            None => {}
        }
    }
    if numerics >= 1 && basis.is_none() {
        basis = Some(b"0%".to_vec());
    }
    if numerics == 0 && basis.is_some() && !keyword_set {
        grow = 1.0;
    }
    out.emit(Prop::FlexGrow, &ffi::format_double(c"%g", grow));
    out.emit(Prop::FlexShrink, &ffi::format_double(c"%g", shrink));
    if let Some(basis) = basis {
        out.emit(Prop::FlexBasis, &basis);
    }
}

fn flex_flow(out: &mut Out, text: &[u8]) {
    for t in split_ws(text) {
        if [&b"row"[..], b"row-reverse", b"column", b"column-reverse"]
            .iter()
            .any(|k| eq(t, k))
        {
            out.emit(Prop::FlexDirection, t);
        } else if [&b"wrap"[..], b"nowrap", b"wrap-reverse"]
            .iter()
            .any(|k| eq(t, k))
        {
            out.emit(Prop::FlexWrap, t);
        }
    }
}

fn list_style(out: &mut Out, text: &[u8]) {
    let Some([kind, position, image]) = inline::list_style_split(text) else {
        return;
    };
    out.emit(Prop::ListStyleType, &kind);
    out.emit(Prop::ListStylePosition, &position);
    out.emit(Prop::ListStyleImage, &image);
}

fn quad_text_collapse(v: [&[u8]; 4]) -> Vec<u8> {
    if v[0] == v[1] && v[1] == v[2] && v[2] == v[3] {
        v[0].to_vec()
    } else if v[0] == v[2] && v[1] == v[3] {
        join(&v[..2], b" ")
    } else if v[1] == v[3] {
        join(&v[..3], b" ")
    } else {
        join(&v, b" ")
    }
}

fn border_radius_half_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(text, 5);
    if !(1..=4).contains(&tokens.len()) {
        return None;
    }
    let mut vals = Vec::with_capacity(tokens.len());
    for tok in tokens {
        if let Some((num, unit)) = length(tok) {
            if !(num >= 0.0 && (unit != NUMBER || num == 0.0)) {
                return None;
            }
            vals.push(add_leading_zeros(tok));
        } else {
            calc_value(tok)?;
            vals.push(math_canonical(&c_text(tok)).unwrap_or_else(|| add_leading_zeros(tok)));
        }
    }
    let n = vals.len();
    let at = |i: usize, or: usize| -> &[u8] { &vals[if n > i { i } else { or }] };
    let left = if n >= 4 {
        3
    } else if n >= 2 {
        1
    } else {
        0
    };
    Some(quad_text_collapse([
        at(0, 0),
        at(1, 0),
        at(2, 0),
        at(left, 0),
    ]))
}

pub(crate) fn border_radius_canonical(value: &[u8]) -> Option<Vec<u8>> {
    if contains(value, b"var(") {
        return None;
    }
    let slash = value.iter().position(|&c| c == b'/');
    if slash.is_some_and(|s| value[s + 1..].contains(&b'/')) {
        return None;
    }
    let first = slash.map_or(value, |s| &value[..s]);
    let h = border_radius_half_canonical(strip(first))?;
    let v = match slash {
        Some(s) => Some(border_radius_half_canonical(strip(&value[s + 1..]))?),
        None => None,
    };
    Some(match v {
        Some(v) if v != h => [&h[..], b" / ", &v[..]].concat(),
        _ => h,
    })
}

fn border_radius(out: &mut Out, text: &[u8]) {
    if !contains(text, b"var(")
        && wide_keyword(text).is_none()
        && border_radius_canonical(text).is_none()
    {
        return;
    }
    let slash = text.iter().position(|&c| c == b'/');
    let tokens = split_ws(slash.map_or(text, |s| &text[..s]));
    let vtokens = slash.map_or_else(Vec::new, |s| split_ws(&text[s + 1..]));
    let n = tokens.len();
    if n == 0 {
        return;
    }
    let pick = |toks: &[&'_ [u8]], i: usize| -> Option<usize> {
        let n = toks.len();
        match i {
            _ if n == 0 => None,
            3 if n < 4 => Some(if n >= 2 { 1 } else { 0 }),
            _ if i < n => Some(i),
            _ => Some(0),
        }
    };
    const CORNERS: [Prop; 4] = [
        Prop::BorderTopLeftRadius,
        Prop::BorderTopRightRadius,
        Prop::BorderBottomRightRadius,
        Prop::BorderBottomLeftRadius,
    ];
    for (i, corner) in CORNERS.into_iter().enumerate() {
        let h = pick(&tokens, i).map_or(&b""[..], |k| tokens[k]);
        let text = match pick(&vtokens, i) {
            Some(k) => join(&[h, vtokens[k]], b" "),
            None => h.to_vec(),
        };
        out.emit(corner, &text);
    }
    out.emit(Prop::BorderRadius, tokens[0]);
}

fn logical_pair(out: &mut Out, text: &[u8], a: Prop, b: Prop) {
    border_pair(out, text, a, b);
}

fn box_quad(out: &mut Out, text: &[u8], props: [Prop; 4]) {
    let tokens = split_ws(text);
    if !tokens.is_empty() {
        out.quad(props, &tokens);
    }
}

fn text_wrap(out: &mut Out, text: &[u8]) {
    let kw = lower(text);
    let mapped: &[u8] = match strip(&kw) {
        b"nowrap" => b"nowrap",
        b"wrap" | b"balance" | b"pretty" | b"stable" => b"normal",
        _ => return,
    };
    out.emit(Prop::WhiteSpace, mapped);
}

fn container(out: &mut Out, text: &[u8]) {
    let slash = text.iter().position(|&c| c == b'/');
    let name_part = strip(slash.map_or(text, |s| &text[..s]));
    let type_part = slash.map(|s| strip(&text[s + 1..]));
    let type_ok = match type_part {
        Some(t) => !t.is_empty() && parse(Prop::ContainerType, t).is_some(),
        None => true,
    };
    let Some(name) = type_ok
        .then(|| parse(Prop::ContainerName, name_part))
        .flatten()
    else {
        return;
    };
    match type_part {
        None => {
            if let Some(normal) = parse(Prop::ContainerType, b"normal") {
                out.push(Prop::ContainerType, normal);
            }
            out.push(Prop::ContainerName, name);
        }
        Some(t) => {
            out.push(Prop::ContainerName, name);
            if !t.is_empty() {
                out.emit(Prop::ContainerType, t);
            }
        }
    }
}

fn animation_range(out: &mut Out, text: &[u8]) {
    if let Some((start, end)) = animation::range_shorthand_expand(text) {
        out.emit(Prop::AnimationRangeStart, &start);
        out.emit(Prop::AnimationRangeEnd, &end);
    }
}
