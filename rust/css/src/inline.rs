//! Southstar — the inline style text behind the CSSOM: one property read from a style attribute with its shorthand rebuilt, one property set, and the whole block serialized with complete shorthands collapsed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::animation::{self, Longhand};
use crate::container;
use crate::content;
use crate::counter;
use crate::declarations::{self, Collected};
use crate::ffi::{self, SheetDecl};
use crate::font;
use crate::gradient::text_starts_gradient;
use crate::grid;
use crate::image::{self, text_starts_image_set};
use crate::position;
use crate::prop::Prop;
use crate::property::{self, Body, Value, bg_token_is_box, contains, eq, lower};
use crate::scan::{
    is_ident, is_ws, match_close_paren, scan_until, skip_to_block_end, skip_ws_comments,
    split_ws_limit, split_ws_paren, starts_with_ci, strip, strip_important, trim_range,
};
use crate::shorthand::{self, ANIMATION_LONGHANDS, TRANSITION_LONGHANDS};
use crate::text::{add_leading_zeros, normalize_negative_zero, split_top_level_commas};
use crate::values;

const DECL_SHEETS_MAX: usize = 4096;
const GET_MEMO: usize = 32;
const SERIALIZE_MEMO: usize = 16;

const QUADS: [(&[u8], [Prop; 4]); 5] = [
    (
        b"margin",
        [
            Prop::MarginTop,
            Prop::MarginRight,
            Prop::MarginBottom,
            Prop::MarginLeft,
        ],
    ),
    (
        b"padding",
        [
            Prop::PaddingTop,
            Prop::PaddingRight,
            Prop::PaddingBottom,
            Prop::PaddingLeft,
        ],
    ),
    (
        b"border-width",
        [
            Prop::BorderTopWidth,
            Prop::BorderRightWidth,
            Prop::BorderBottomWidth,
            Prop::BorderLeftWidth,
        ],
    ),
    (
        b"border-color",
        [
            Prop::BorderTopColor,
            Prop::BorderRightColor,
            Prop::BorderBottomColor,
            Prop::BorderLeftColor,
        ],
    ),
    (
        b"border-style",
        [
            Prop::BorderTopStyle,
            Prop::BorderRightStyle,
            Prop::BorderBottomStyle,
            Prop::BorderLeftStyle,
        ],
    ),
];

const BACKGROUND_GET: [&[u8]; 8] = [
    b"background-image",
    b"background-position",
    b"background-size",
    b"background-repeat",
    b"background-attachment",
    b"background-origin",
    b"background-clip",
    b"background-color",
];

const BACKGROUND_MEMBERS: [&[u8]; 9] = [
    b"background-image",
    b"background-position-x",
    b"background-position-y",
    b"background-size",
    b"background-repeat",
    b"background-attachment",
    b"background-origin",
    b"background-clip",
    b"background-color",
];

const OUTLINE_MEMBERS: [&[u8]; 3] = [b"outline-color", b"outline-style", b"outline-width"];

const LIST_MEMBERS: [&[u8]; 3] = [
    b"list-style-position",
    b"list-style-type",
    b"list-style-image",
];

const GRID_MEMBERS: [&[u8]; 6] = [
    b"grid-template-rows",
    b"grid-template-columns",
    b"grid-template-areas",
    b"grid-auto-flow",
    b"grid-auto-rows",
    b"grid-auto-columns",
];

const CSS_WIDE: [&[u8]; 6] = [
    b"inherit",
    b"initial",
    b"revert",
    b"revert-layer",
    b"revert-rule",
    b"unset",
];

#[derive(Default)]
struct DeclSheets {
    sheets: HashMap<Vec<u8>, Rc<Vec<SheetDecl>>>,
    viewport: (f64, f64),
}

struct GetSlot {
    style: Vec<u8>,
    prop: Vec<u8>,
    value: Option<Vec<u8>>,
}

#[derive(Default)]
struct GetMemo {
    slots: Vec<Option<GetSlot>>,
    next: u32,
    viewport: (f64, f64),
}

#[derive(Default)]
struct SerializeMemo {
    slots: Vec<Option<(Vec<u8>, Vec<u8>)>>,
    next: u32,
    viewport: (f64, f64),
}

thread_local! {
    static DECL_SHEETS: RefCell<DeclSheets> = RefCell::default();
    static GET: RefCell<GetMemo> = RefCell::default();
    static SERIALIZE: RefCell<SerializeMemo> = RefCell::default();
}

struct Declarations<'a> {
    s: &'a [u8],
    p: usize,
}

fn declarations(s: &[u8]) -> Declarations<'_> {
    Declarations { s, p: 0 }
}

impl<'a> Iterator for Declarations<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let s = self.s;
        let end = s.len();
        loop {
            let mut p = skip_ws_comments(s, self.p, end);
            while p < end && s[p] == b';' {
                p = skip_ws_comments(s, p + 1, end);
            }
            if p >= end {
                self.p = end;
                return None;
            }
            if s[p] == b'@' {
                self.p = skip_at_rule(s, p, end);
                continue;
            }
            let (kend, term) = scan_until(s, p, end, b":;");
            let key = trim_range(s, p, kend);
            if term != b':' {
                self.p = if term == b';' { kend + 1 } else { kend };
                continue;
            }
            let vstart = skip_ws_comments(s, kend + 1, end);
            let (vend, term) = scan_until(s, vstart, end, b";}");
            self.p = if term == b';' { vend + 1 } else { vend };
            return Some((key, trim_range(s, vstart, vend)));
        }
    }
}

fn skip_at_rule(s: &[u8], p: usize, end: usize) -> usize {
    let (stop, term) = scan_until(s, p, end, b"{;}");
    match term {
        b'{' => skip_to_block_end(s, stop, end),
        b';' => stop + 1,
        _ if stop > p => stop,
        _ => p + 1,
    }
}

fn is_custom(name: &[u8]) -> bool {
    name.starts_with(b"--")
}

fn css_wide(value: &[u8]) -> bool {
    CSS_WIDE.contains(&value)
}

fn keyword_text(value: Option<Value>) -> Option<Vec<u8>> {
    match value?.body {
        Body::Keyword(text) => Some(text),
        _ => None,
    }
}

fn with_important(mut text: Vec<u8>, important: bool) -> Vec<u8> {
    if important {
        text.extend_from_slice(b" !important");
    }
    text
}

fn push_separated(out: &mut Vec<u8>, sep: &[u8], part: &[u8]) {
    if !out.is_empty() {
        out.extend_from_slice(sep);
    }
    out.extend_from_slice(part);
}

fn serialize_urls(value: &[u8]) -> Vec<u8> {
    let end = value.len();
    let mut out = Vec::with_capacity(end);
    let mut p = 0;
    while p < end {
        if end - p >= 4
            && value[p..p + 4].eq_ignore_ascii_case(b"url(")
            && (p == 0 || !is_ident(value[p - 1]))
            && let Some(close) = match_close_paren(value, p + 4, end)
        {
            let mut start = p + 4;
            while start < close && is_ws(value[start]) {
                start += 1;
            }
            let mut stop = close;
            while stop > start && is_ws(value[stop - 1]) {
                stop -= 1;
            }
            if stop > start
                && ((value[start] == b'"' && value[stop - 1] == b'"')
                    || (value[start] == b'\'' && value[stop - 1] == b'\''))
            {
                start += 1;
                stop -= 1;
            }
            out.extend_from_slice(b"url(\"");
            for q in start..stop {
                if value[q] == b'"' && (q == start || value[q - 1] != b'\\') {
                    out.push(b'\\');
                }
                out.push(value[q]);
            }
            out.extend_from_slice(b"\")");
            p = close + 1;
            continue;
        }
        out.push(value[p]);
        p += 1;
    }
    out
}

fn place_canonical(prop: &[u8], value: Vec<u8>) -> Vec<u8> {
    let (align, justify) = match prop {
        b"place-items" => (Prop::AlignItems, Prop::JustifyItems),
        b"place-self" => (Prop::AlignSelf, Prop::JustifySelf),
        _ => (Prop::AlignContent, Prop::JustifyContent),
    };
    const SPLITS: [[usize; 2]; 5] = [[0, 0], [1, 0], [2, 1], [1, 2], [2, 0]];
    let tokens = split_ws_limit(&value, 4);
    let n = tokens.len();
    if !(1..=4).contains(&n) {
        return value;
    }
    let pair = |at: usize| [tokens[at], tokens[at + 1]].join(&b' ');
    for k in SPLITS[n] {
        if k == 0 {
            break;
        }
        let first = if k == 2 { pair(0) } else { tokens[0].to_vec() };
        let second = if k >= n {
            first.clone()
        } else if n - k == 2 {
            pair(k)
        } else {
            tokens[k].to_vec()
        };
        let Some(a) = property::parse(align, &first) else {
            continue;
        };
        let j = property::parse(justify, &second);
        if let (
            Body::Keyword(a),
            Some(Value {
                body: Body::Keyword(j),
                ..
            }),
        ) = (a.body, j)
        {
            return if a == j { a } else { [a, j].join(&b' ') };
        }
    }
    value
}

fn positions_canonical(value: &[u8], background: bool) -> Option<Vec<u8>> {
    let end = value.len();
    let mut out = Vec::new();
    let mut p = 0;
    while p < end {
        let (seg_end, term) = scan_until(value, p, end, b",");
        let layer = trim_range(value, p, seg_end);
        let canon = position::canonical_ex(layer, true, background)?;
        push_separated(&mut out, b", ", &canon);
        p = if term == b',' { seg_end + 1 } else { seg_end };
    }
    Some(out)
}

fn value_canonical(prop: &[u8], mut value: Vec<u8>) -> Vec<u8> {
    if let Some(
        id @ (Prop::BorderImageSlice
        | Prop::BorderImageWidth
        | Prop::BorderImageOutset
        | Prop::BorderImageRepeat),
    ) = ffi::prop_named(prop)
    {
        return keyword_text(property::parse(id, &value)).unwrap_or(value);
    }
    if prop == b"unicode-range" {
        return image::unicode_range_canonical(&value).unwrap_or(value);
    }
    if matches!(prop, b"place-self" | b"place-items" | b"place-content") {
        value = place_canonical(prop, value);
    }
    if prop.starts_with(b"grid-row") || prop.starts_with(b"grid-column") || prop == b"grid-area" {
        let shorthand = matches!(prop, b"grid-row" | b"grid-column" | b"grid-area");
        let canon = if shorthand {
            grid::placement_canonical(&value, prop[5] == b'a')
        } else {
            grid::line_canonical(&value).map(|(canon, _)| canon)
        };
        if let Some(canon) = canon {
            return canon;
        }
    }
    if prop == b"grid-template" || prop == b"grid" {
        let canon = if prop.len() == 4 {
            grid::shorthand_parse(&value).map(|parsed| parsed.canon)
        } else {
            grid::template_parse(&value).map(|parsed| parsed.canon)
        };
        if let Some(canon) = canon {
            return canon;
        }
    }
    if prop == b"grid-auto-flow"
        && let Some(canon) = grid::auto_flow_canonical(&value)
    {
        return canon;
    }
    value = add_leading_zeros(&value);
    value = normalize_negative_zero(&value);
    value = serialize_urls(&value);
    let canon = match prop {
        b"content" => content::content_canonical(&value),
        b"font-family" => font::family_canonical(&value),
        b"font" => font::shorthand_canonical(&value),
        b"object-position" | b"background-position" => {
            positions_canonical(&value, prop == b"background-position")
        }
        b"container" => container::shorthand_canonical(&value),
        b"background-image" | b"mask-image" | b"list-style-image" | b"border-image-source" => {
            image::image_value_canonical(&value)
        }
        _ => None,
    };
    canon.unwrap_or(value)
}

pub(crate) fn list_style_split(text: &[u8]) -> Option<[Vec<u8>; 3]> {
    let tokens = split_ws_paren(text, 8);
    if !(1..=3).contains(&tokens.len()) {
        return None;
    }
    let (mut kind, mut position, mut image) = (None, None, None);
    let mut nones = 0;
    for tok in tokens {
        if position.is_none() && (eq(tok, b"inside") || eq(tok, b"outside")) {
            position = Some(lower(tok));
            continue;
        }
        if eq(tok, b"none") {
            nones += 1;
            continue;
        }
        if image.is_none()
            && (starts_with_ci(tok, b"url(")
                || text_starts_gradient(tok)
                || text_starts_image_set(tok))
        {
            property::parse(Prop::ListStyleImage, tok)?;
            image = Some(value_canonical(b"list-style-image", tok.to_vec()));
            continue;
        }
        if kind.is_none()
            && let Some(canon) = counter::list_style_type_canonical(tok)
        {
            kind = Some(canon);
            continue;
        }
        return None;
    }
    let invalid = nones > 2
        || (nones == 2 && (kind.is_some() || image.is_some()))
        || (nones == 1 && kind.is_some() && image.is_some());
    if invalid {
        return None;
    }
    if nones == 2 || (nones == 1 && kind.is_none() && image.is_none()) {
        kind = Some(b"none".to_vec());
        image = Some(b"none".to_vec());
    } else if nones == 1 && kind.is_none() {
        kind = Some(b"none".to_vec());
    } else if nones == 1 && image.is_none() {
        image = Some(b"none".to_vec());
    }
    Some([
        kind.unwrap_or_else(|| b"disc".to_vec()),
        position.unwrap_or_else(|| b"outside".to_vec()),
        image.unwrap_or_else(|| b"none".to_vec()),
    ])
}

fn declaration_sheet(name: &[u8], value: &[u8]) -> Rc<Vec<SheetDecl>> {
    let viewport = ffi::viewport();
    let key = [b"*{", name, b":", value, b"}"].concat();
    let cached = DECL_SHEETS.with_borrow_mut(|cache| {
        if cache.viewport != viewport || cache.sheets.len() >= DECL_SHEETS_MAX {
            cache.sheets.clear();
            cache.viewport = viewport;
        }
        cache.sheets.get(&key).cloned()
    });
    if let Some(decls) = cached {
        return decls;
    }
    let decls = Rc::new(ffi::sheet_declarations(&key, |_| true));
    DECL_SHEETS.with_borrow_mut(|cache| cache.sheets.insert(key, Rc::clone(&decls)));
    decls
}

fn expanded_value(name: &[u8], value: &[u8], prop: Prop) -> Option<(Vec<u8>, bool)> {
    if eq(name, b"list-style")
        && matches!(
            prop,
            Prop::ListStyleType | Prop::ListStylePosition | Prop::ListStyleImage
        )
    {
        let (plain, important) = strip_important(value);
        let [kind, position, image] = list_style_split(plain)?;
        let part = match prop {
            Prop::ListStyleType => kind,
            Prop::ListStylePosition => position,
            _ => image,
        };
        return Some((part, important));
    }
    declaration_sheet(name, value)
        .iter()
        .rfind(|decl| decl.prop == Some(prop))
        .map(|decl| (decl.text.clone(), decl.important))
}

fn all_covered(name: &[u8]) -> bool {
    !name.starts_with(b"-")
        && !eq(name, b"direction")
        && !eq(name, b"unicode-bidi")
        && declarations::named_property_supported(name)
}

fn under_prefix(name: &[u8], prefix: Option<&[u8]>) -> bool {
    let Some(prefix) = prefix else {
        return true;
    };
    eq(name, prefix)
        || (name.len() > prefix.len()
            && name[..prefix.len()].eq_ignore_ascii_case(prefix)
            && name[prefix.len()] == b'-')
}

fn all_value_for(style: &[u8], prefix: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut all: Option<Vec<u8>> = None;
    let mut all_important = false;
    for (name, value) in declarations(style) {
        let (value, important) = strip_important(value);
        let value = strip(value);
        if eq(name, b"all")
            && declarations::named_declaration_valid(b"all", value)
            && (all.is_none() || important || !all_important)
        {
            all = Some(value.to_vec());
            all_important = important;
        } else if all.is_some()
            && all_covered(name)
            && under_prefix(name, prefix)
            && declarations::named_declaration_valid(name, value)
            && (important || !all_important)
        {
            all = None;
            all_important = false;
        }
    }
    all
}

fn quad_ids(prop: &[u8]) -> Option<[Prop; 4]> {
    QUADS
        .iter()
        .find(|(name, _)| *name == prop)
        .map(|(_, ids)| *ids)
}

fn quad_value(style: &[u8], prop: &[u8]) -> Option<(Vec<u8>, bool)> {
    let ids = quad_ids(prop)?;
    let wrapped = [b"*{", style, b"}"].concat();
    let mut values: [Option<Vec<u8>>; 4] = Default::default();
    let mut priorities = [false; 4];
    let decls = ffi::sheet_declarations(&wrapped, |p| p.is_some_and(|p| ids.contains(&p)));
    for decl in decls {
        for side in 0..4 {
            if decl.prop != Some(ids[side]) || (priorities[side] && !decl.important) {
                continue;
            }
            values[side] = Some(decl.text.clone());
            priorities[side] = decl.important;
        }
    }
    let [Some(top), Some(right), Some(bottom), Some(left)] = &values else {
        return None;
    };
    if priorities.iter().any(|&p| p != priorities[0]) {
        return None;
    }
    let all_same = top == right && right == bottom && bottom == left;
    if !all_same && values.iter().flatten().any(|v| css_wide(v)) {
        return None;
    }
    let shown = if all_same {
        1
    } else if top == bottom && right == left {
        2
    } else if right == left {
        3
    } else {
        4
    };
    let parts = [top.as_slice(), right, bottom, left];
    Some((parts[..shown].join(&b' '), priorities[0]))
}

fn overflow_value(style: &[u8]) -> Option<(Vec<u8>, bool)> {
    let wrapped = [b"*{", style, b"}"].concat();
    let mut values: [Option<Vec<u8>>; 2] = Default::default();
    let mut priorities = [false; 2];
    let decls = ffi::sheet_declarations(&wrapped, |p| {
        matches!(p, Some(Prop::Overflow | Prop::OverflowX | Prop::OverflowY))
    });
    for decl in decls {
        let indices: &[usize] = match decl.prop {
            Some(Prop::Overflow) => &[0, 1],
            Some(Prop::OverflowX) => &[0],
            _ => &[1],
        };
        for &index in indices {
            if priorities[index] && !decl.important {
                continue;
            }
            values[index] = Some(decl.text.clone());
            priorities[index] = decl.important;
        }
    }
    let [Some(x), Some(y)] = values else {
        return None;
    };
    if priorities[0] != priorities[1] || (x != y && (css_wide(&x) || css_wide(&y))) {
        return None;
    }
    let text = if x == y { x } else { [x, y].join(&b' ') };
    Some((text, priorities[0]))
}

fn anim_shorthand_value(style: &[u8], is_animation: bool) -> Option<Vec<u8>> {
    let longhands: &[(Prop, Longhand)] = if is_animation {
        &ANIMATION_LONGHANDS[..8]
    } else {
        &TRANSITION_LONGHANDS
    };
    let mut keywords: Vec<(Longhand, Vec<u8>)> = Vec::new();
    let mut any = false;
    for &(prop, lh) in longhands {
        let Some(mut text) = get(style, prop.name()) else {
            continue;
        };
        if let Some(bang) = text.windows(11).position(|w| w == b" !important") {
            text.truncate(bang);
        }
        let value = property::parse(prop, &text);
        any |= value.is_some();
        if let Some(keyword) = keyword_text(value) {
            keywords.push((lh, keyword));
        }
    }
    if !any {
        return None;
    }
    let (list, mismatch) = animation::lists(
        |lh| {
            keywords
                .iter()
                .find(|(of, _)| *of == lh)
                .map(|(_, text)| text.as_slice())
        },
        is_animation,
    );
    (!mismatch).then(|| animation::shorthand_serialize(&list, is_animation))
}

fn background_value(style: &[u8]) -> Option<(Vec<u8>, bool)> {
    let mut values: Vec<Vec<u8>> = Vec::with_capacity(BACKGROUND_GET.len());
    let mut important = false;
    for (i, name) in BACKGROUND_GET.iter().enumerate() {
        let value = get(style, name).filter(|v| !v.is_empty())?;
        let (value, imp) = strip_important(&value);
        if i == 0 {
            important = imp;
        } else if imp != important {
            return None;
        }
        values.push(value.to_vec());
    }
    let text = if css_wide(&values[0]) && values.iter().all(|v| *v == values[0]) {
        values[0].clone()
    } else {
        let parts: [&[u8]; 7] = core::array::from_fn(|i| values[i].as_slice());
        background_shorthand_serialize(parts, Some(&values[7]))?
    };
    Some((with_important(text, important), important))
}

fn grid_value(style: &[u8], full: bool) -> Option<Vec<u8>> {
    let n = if full { 6 } else { 3 };
    let mut values: Vec<Vec<u8>> = Vec::with_capacity(n);
    let (mut present, mut important) = (0, 0);
    for name in &GRID_MEMBERS[..n] {
        let value = get(style, name).unwrap_or_default();
        if value.is_empty() {
            values.push(value);
            continue;
        }
        present += 1;
        let (plain, imp) = strip_important(&value);
        if imp {
            important += 1;
        }
        values.push(strip(plain).to_vec());
    }
    if present == 0 {
        return None;
    }
    if present != n || (important != 0 && important != n) {
        return Some(Vec::new());
    }
    let wide = values.iter().filter(|v| css_wide(v)).count();
    let text = if wide == n {
        if values.iter().all(|v| *v == values[0]) {
            values[0].clone()
        } else {
            Vec::new()
        }
    } else if wide == 0 {
        if full {
            let parts: [&[u8]; 6] = core::array::from_fn(|i| values[i].as_slice());
            grid::grid_compose(&parts)
        } else {
            grid::template_compose(&values[0], &values[1], &values[2])
        }
    } else {
        Vec::new()
    };
    if important != 0 && !text.is_empty() {
        return Some(with_important(text, true));
    }
    Some(text)
}

fn memo_get(style: &[u8], prop: &[u8]) -> Option<Option<Vec<u8>>> {
    let viewport = ffi::viewport();
    GET.with_borrow_mut(|memo| {
        if memo.viewport != viewport {
            memo.slots.clear();
            memo.viewport = viewport;
            return None;
        }
        memo.slots
            .iter()
            .flatten()
            .find(|slot| slot.prop == prop && slot.style == style)
            .map(|slot| slot.value.clone())
    })
}

fn memo_keep(style: &[u8], prop: &[u8], value: Option<Vec<u8>>) -> Option<Vec<u8>> {
    GET.with_borrow_mut(|memo| {
        let slot = memo.next as usize % GET_MEMO;
        memo.next = memo.next.wrapping_add(1);
        if memo.slots.len() < GET_MEMO {
            memo.slots.resize_with(GET_MEMO, || None);
        }
        memo.slots[slot] = Some(GetSlot {
            style: style.to_vec(),
            prop: prop.to_vec(),
            value: value.clone(),
        });
    });
    value
}

pub(crate) fn get(style: &[u8], prop: &[u8]) -> Option<Vec<u8>> {
    if let Some(hit) = memo_get(style, prop) {
        return hit;
    }
    let keep = |value: Option<Vec<u8>>| memo_keep(style, prop, value);
    if eq(prop, b"all") {
        return keep(all_value_for(style, None));
    }
    if quad_ids(prop).is_some() {
        return keep(quad_value(style, prop).map(|(text, _)| text));
    }
    if eq(prop, b"overflow") {
        return keep(overflow_value(style).map(|(text, _)| text));
    }
    if eq(prop, b"font")
        && let Some(all) = all_value_for(style, Some(b"font"))
    {
        return keep(Some(all));
    }
    if eq(prop, b"animation") || eq(prop, b"transition") {
        return keep(anim_shorthand_value(style, prop[0] == b'a'));
    }
    if eq(prop, b"list-style") {
        let kind = get(style, b"list-style-type");
        let position = get(style, b"list-style-position");
        let image = get(style, b"list-style-image");
        let text = match (kind, position, image) {
            (Some(kind), Some(position), Some(image))
                if css_wide(&kind) && kind == position && position == image =>
            {
                Some(kind)
            }
            (Some(kind), Some(position), Some(image)) => Some(counter::list_style_serialize(
                Some(&kind),
                Some(&position),
                Some(&image),
            )),
            _ => None,
        };
        return keep(text);
    }
    if eq(prop, b"animation-range") {
        let start = get(style, b"animation-range-start");
        let end = get(style, b"animation-range-end");
        let text = match (start, end) {
            (Some(start), Some(end)) => Some(animation::range_serialize(&start, &end)),
            _ => None,
        };
        return keep(text);
    }
    if eq(prop, b"background")
        && let Some((text, _)) = background_value(style)
    {
        return keep(Some(text));
    }
    if eq(prop, b"background-position") {
        let xs = get(style, b"background-position-x");
        let ys = get(style, b"background-position-y");
        if let (Some(xs), Some(ys)) = (xs, ys)
            && let Some(text) = background_position_zip(&xs, &ys)
        {
            return keep(Some(text));
        }
    }
    if (eq(prop, b"grid") || eq(prop, b"grid-template"))
        && !contains(style, b"var(")
        && let Some(text) = grid_value(style, prop.len() == 4)
    {
        return keep(Some(text));
    }
    let id = ffi::prop_named(prop);
    if id.is_none()
        && declarations::named_property_supported(prop)
        && let Some(all) = all_value_for(style, None)
    {
        return keep(Some(all));
    }
    let custom = is_custom(prop);
    let mut winner: Option<Vec<u8>> = None;
    let mut winner_important = false;
    for (key, value) in declarations(style) {
        let matched = key.len() == prop.len() && if custom { key == prop } else { eq(key, prop) };
        let candidate = if matched {
            Some((value.to_vec(), strip_important(value).1))
        } else {
            id.and_then(|id| expanded_value(key, value, id))
        };
        let Some((candidate, important)) = candidate else {
            continue;
        };
        if winner.is_none() || important || !winner_important {
            winner = Some(with_important(candidate, important && !matched));
            winner_important = important;
        }
    }
    keep(winner.map(|winner| value_canonical(prop, winner)))
}

struct InlineDecl {
    name: Vec<u8>,
    value: Vec<u8>,
    important: bool,
}

fn find<'a>(decls: &'a [InlineDecl], name: &[u8]) -> Option<&'a InlineDecl> {
    let custom = is_custom(name);
    decls.iter().find(|decl| {
        if custom {
            decl.name == name
        } else {
            eq(&decl.name, name)
        }
    })
}

fn serialize_memo_hit(style: &[u8]) -> Option<Vec<u8>> {
    let viewport = ffi::viewport();
    SERIALIZE.with_borrow_mut(|memo| {
        if memo.viewport != viewport {
            memo.slots.clear();
            memo.viewport = viewport;
            return None;
        }
        memo.slots
            .iter()
            .flatten()
            .find(|(input, _)| input == style)
            .map(|(_, out)| out.clone())
    })
}

fn serialize_memo_keep(style: &[u8], out: Vec<u8>) -> Vec<u8> {
    SERIALIZE.with_borrow_mut(|memo| {
        let slot = memo.next as usize % SERIALIZE_MEMO;
        memo.next = memo.next.wrapping_add(1);
        if memo.slots.len() < SERIALIZE_MEMO {
            memo.slots.resize_with(SERIALIZE_MEMO, || None);
        }
        memo.slots[slot] = Some((style.to_vec(), out.clone()));
    });
    out
}

fn parsed_declarations(style: &[u8]) -> Vec<InlineDecl> {
    let mut decls: Vec<InlineDecl> = Vec::new();
    for (name, value) in declarations(style) {
        let custom = is_custom(name);
        let name = match lower(name) {
            _ if custom => name.to_vec(),
            lowered if lowered == b"-webkit-line-clamp" => b"line-clamp".to_vec(),
            lowered => lowered,
        };
        let (value, important) = strip_important(value);
        let value = strip(value);
        if name.is_empty()
            || value.is_empty()
            || !declarations::named_property_supported(&name)
            || !declarations::named_declaration_valid(&name, value)
        {
            continue;
        }
        let mut value = value_canonical(&name, value.to_vec());
        if !custom && let Some(canonical) = values::specified_canonical(Some(&name), &value) {
            value = canonical;
        }
        let existing = {
            let custom = is_custom(&name);
            decls.iter_mut().find(|decl| {
                if custom {
                    decl.name == name
                } else {
                    eq(&decl.name, &name)
                }
            })
        };
        match existing {
            Some(decl) => {
                if important || !decl.important {
                    decl.value = value;
                    decl.important = important;
                }
            }
            None => decls.push(InlineDecl {
                name,
                value,
                important,
            }),
        }
    }
    decls
}

fn drop_covered_by_all(decls: &mut Vec<InlineDecl>) {
    let Some(all_index) = decls.iter().rposition(|decl| decl.name == b"all") else {
        return;
    };
    let all_value = decls[all_index].value.clone();
    let all_important = decls[all_index].important;
    for i in (0..decls.len()).rev() {
        if i == all_index || !all_covered(&decls[i].name) {
            continue;
        }
        let decl = &decls[i];
        let overridden = i < all_index && (all_important || !decl.important);
        let redundant = i > all_index && decl.important == all_important && decl.value == all_value;
        if overridden || redundant {
            decls.remove(i);
        }
    }
}

fn same_importance(parts: &[Option<&InlineDecl>]) -> Option<bool> {
    let first = parts.first().copied().flatten()?;
    parts
        .iter()
        .all(|part| part.is_some_and(|part| part.important == first.important))
        .then_some(first.important)
}

fn emit(out: &mut Vec<u8>, name: &[u8], value: &[u8], important: bool) {
    if !out.is_empty() {
        out.push(b' ');
    }
    out.extend_from_slice(name);
    out.extend_from_slice(b": ");
    out.extend_from_slice(value);
    if important {
        out.extend_from_slice(b" !important");
    }
    out.push(b';');
}

pub(crate) fn serialize(style: &[u8]) -> Vec<u8> {
    if let Some(hit) = serialize_memo_hit(style) {
        return hit;
    }
    let mut decls = parsed_declarations(style);
    drop_covered_by_all(&mut decls);
    let quads: Vec<Option<(Vec<u8>, bool)>> = QUADS
        .iter()
        .map(|(quad, ids)| {
            let mut sides = [false; 4];
            let mut complete = false;
            for decl in &decls {
                if decl.name == *quad {
                    complete = true;
                    break;
                }
                let id = ffi::prop_named(&decl.name);
                for side in 0..4 {
                    if id == Some(ids[side]) {
                        sides[side] = true;
                    }
                }
            }
            (complete || sides.iter().all(|&side| side))
                .then(|| quad_value(style, quad))
                .flatten()
        })
        .collect();
    let mut quad_emitted = [false; 5];
    let overflow_complete = decls.iter().any(|decl| decl.name == b"overflow")
        || ([Prop::OverflowX, Prop::OverflowY].iter()).all(|axis| {
            decls
                .iter()
                .any(|decl| ffi::prop_named(&decl.name) == Some(*axis))
        });
    let overflow = overflow_complete.then(|| overflow_value(style)).flatten();
    let mut overflow_emitted = false;
    let outline_parts = OUTLINE_MEMBERS.map(|name| find(&decls, name));
    let outline = same_importance(&outline_parts).map(|important| {
        let parts = outline_parts.map(|part| part.map_or(&[][..], |part| part.value.as_slice()));
        (parts.join(&b' '), important)
    });
    let mut outline_emitted = false;
    let list_parts = LIST_MEMBERS.map(|name| find(&decls, name));
    let list = same_importance(&list_parts).map(|important| {
        let parts = list_parts.map(|part| part.map_or(&[][..], |part| part.value.as_slice()));
        let shown = if parts[2] == b"none" { 2 } else { 3 };
        (parts[..shown].join(&b' '), important)
    });
    let mut list_emitted = false;
    let background_parts = BACKGROUND_MEMBERS.map(|name| find(&decls, name));
    let background = same_importance(&background_parts).and_then(|important| {
        background_value(style).map(|(text, _)| (strip_important(&text).0.to_vec(), important))
    });
    let mut background_emitted = false;
    let mut out = Vec::new();
    for decl in &decls {
        let name = decl.name.as_slice();
        if let Some((text, important)) = background
            .as_ref()
            .filter(|_| BACKGROUND_MEMBERS.contains(&name))
        {
            if !background_emitted {
                emit(&mut out, b"background", text, *important);
                background_emitted = true;
            }
            continue;
        }
        let mut collapsed = false;
        for (q, (quad, ids)) in QUADS.iter().enumerate() {
            let Some((text, important)) = &quads[q] else {
                continue;
            };
            let id = ffi::prop_named(name);
            if name != *quad && !ids.iter().any(|&side| id == Some(side)) {
                continue;
            }
            if !quad_emitted[q] {
                emit(&mut out, quad, text, *important);
                quad_emitted[q] = true;
            }
            collapsed = true;
            break;
        }
        let id = ffi::prop_named(name);
        let overflow_member =
            name == b"overflow" || matches!(id, Some(Prop::OverflowX | Prop::OverflowY));
        if let (false, Some((text, important)), true) = (collapsed, &overflow, overflow_member) {
            if !overflow_emitted {
                emit(&mut out, b"overflow", text, *important);
                overflow_emitted = true;
            }
            collapsed = true;
        }
        if let (false, Some((text, important)), true) =
            (collapsed, &outline, OUTLINE_MEMBERS.contains(&name))
        {
            if !outline_emitted {
                emit(&mut out, b"outline", text, *important);
                outline_emitted = true;
            }
            collapsed = true;
        }
        if let (false, Some((text, important)), true) =
            (collapsed, &list, LIST_MEMBERS.contains(&name))
        {
            if !list_emitted {
                emit(&mut out, b"list-style", text, *important);
                list_emitted = true;
            }
            collapsed = true;
        }
        if !collapsed {
            emit(&mut out, name, &decl.value, decl.important);
        }
    }
    serialize_memo_keep(style, out)
}

fn shorthand_follows(style: &[u8], prop: &[u8], id: Prop) -> bool {
    let mut seen = false;
    for (key, value) in declarations(style) {
        if eq(key, prop) {
            seen = true;
            continue;
        }
        if !seen {
            continue;
        }
        let key_id = ffi::prop_named(key);
        if (key_id.is_none() || key_id == Some(Prop::BorderRadius))
            && expanded_value(key, value, id).is_some()
        {
            return true;
        }
    }
    false
}

fn member_ids(prop: &[u8]) -> Vec<Prop> {
    match prop {
        b"animation" => ANIMATION_LONGHANDS.iter().map(|&(p, _)| p).collect(),
        b"transition" => TRANSITION_LONGHANDS.iter().map(|&(p, _)| p).collect(),
        b"animation-range" => vec![Prop::AnimationRangeStart, Prop::AnimationRangeEnd],
        b"list-style" => vec![
            Prop::ListStyleType,
            Prop::ListStylePosition,
            Prop::ListStyleImage,
        ],
        _ => Vec::new(),
    }
}

fn members_expanded(prop: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    if prop == b"list-style" {
        let [kind, position, image] = list_style_split(value)?;
        return Some(
            [
                b"list-style-type: ".as_slice(),
                &kind,
                b"; list-style-position: ",
                &position,
                b"; list-style-image: ",
                &image,
            ]
            .concat(),
        );
    }
    let text = [prop, b": ", value, b";"].concat();
    let mut collected = Collected {
        declarations: Vec::new(),
    };
    declarations::parse_block(&text, 0, &mut collected);
    let mut out = Vec::new();
    for (name, text, _) in &collected.declarations {
        let decls = shorthand::expand(name, text);
        for (i, decl) in decls.iter().enumerate() {
            if decl.prop == Prop::Animation || decl.prop == Prop::Transition {
                continue;
            }
            if !out.is_empty() {
                out.extend_from_slice(b"; ");
            }
            out.extend_from_slice(decl.prop.name());
            out.extend_from_slice(b": ");
            out.extend_from_slice(&values::serialize(shorthand::slot_value(&decls, i), false));
        }
    }
    (!out.is_empty()).then_some(out)
}

pub(crate) fn set(style: Option<&[u8]>, prop: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let value = value.filter(|v| !v.is_empty());
    let members = member_ids(prop);
    if style.is_none_or(<[u8]>::is_empty) {
        let Some(value) = value else {
            return Vec::new();
        };
        if !eq(prop, b"all") && quad_ids(prop).is_none() && members.is_empty() {
            return [prop, b": ", value].concat();
        }
    }
    let style = style.unwrap_or_default();
    let set_all = eq(prop, b"all");
    let append_after_all = !set_all && all_covered(prop) && all_value_for(style, None).is_some();
    let mut expanded = None;
    if let (false, Some(value)) = (members.is_empty(), value) {
        match members_expanded(prop, value) {
            Some(text) => expanded = Some(text),
            None => return style.to_vec(),
        }
    }
    let append_after_shorthand =
        ffi::prop_named(prop).is_some_and(|id| shorthand_follows(style, prop, id));
    let custom = is_custom(prop);
    let mut out = Vec::new();
    let mut found = false;
    for (key, old_value) in declarations(style) {
        let matched = key.len() == prop.len() && if custom { key == prop } else { eq(key, prop) };
        let key_id = ffi::prop_named(key);
        let member = key_id.is_some_and(|id| members.contains(&id));
        let remove_for_all = set_all && all_covered(key);
        if set_all && (matched || remove_for_all) {
            found = true;
            continue;
        }
        if (append_after_all || append_after_shorthand || !members.is_empty())
            && (matched || member)
        {
            found = true;
            continue;
        }
        if matched || remove_for_all {
            match value {
                Some(value) if !found => {
                    push_separated(&mut out, b"; ", &[key, b": ", value].concat());
                }
                _ => {}
            }
            found = true;
        } else {
            push_separated(&mut out, b"; ", &[key, b": ", old_value].concat());
        }
    }
    if let (true, Some(value)) = (
        set_all || append_after_all || append_after_shorthand || !members.is_empty() || !found,
        value,
    ) {
        match expanded {
            Some(expanded) => push_separated(&mut out, b"; ", &expanded),
            None => push_separated(&mut out, b"; ", &[prop, b": ", value].concat()),
        }
    }
    out
}

pub(crate) fn background_position_zip(xs: &[u8], ys: &[u8]) -> Option<Vec<u8>> {
    let (x, x_important) = strip_important(xs);
    let (y, y_important) = strip_important(ys);
    if x_important != y_important {
        return None;
    }
    let xl = split_top_level_commas(x);
    let yl = split_top_level_commas(y);
    if xl.len() == 1 && yl.len() == 1 && css_wide(&xl[0]) && xl[0] == yl[0] {
        return Some(with_important(xl[0].clone(), x_important));
    }
    if xl.len() != yl.len() {
        return None;
    }
    let mut out = Vec::new();
    for (x, y) in xl.iter().zip(&yl) {
        push_separated(&mut out, b", ", &[x.as_slice(), b" ", y].concat());
    }
    Some(with_important(out, x_important))
}

#[allow(clippy::needless_range_loop)]
fn background_shorthand_serialize(texts: [&[u8]; 7], color: Option<&[u8]>) -> Option<Vec<u8>> {
    let lists = texts.map(split_top_level_commas);
    let n = lists[0].len();
    if n == 0 || lists.iter().any(|list| list.len() != n) {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..n {
        let [image, position, size, repeat, attachment, origin, clip] =
            core::array::from_fn(|k| lists[k][i].as_slice());
        let mut layer = Vec::new();
        if !eq(image, b"none") {
            layer.extend_from_slice(image);
        }
        let size_set = !eq(size, b"auto") && !eq(size, b"auto auto");
        if size_set || position != b"0% 0%" {
            push_separated(&mut layer, b" ", position);
            if size_set {
                layer.extend_from_slice(b" / ");
                layer.extend_from_slice(size);
                if !size.contains(&b' ') && !eq(size, b"cover") && !eq(size, b"contain") {
                    layer.extend_from_slice(b" auto");
                }
            }
        }
        if !eq(repeat, b"repeat") {
            push_separated(&mut layer, b" ", repeat);
        }
        if !eq(attachment, b"scroll") {
            push_separated(&mut layer, b" ", attachment);
        }
        if !bg_token_is_box(clip) {
            let boxes = if eq(origin, b"border-box") {
                clip.to_vec()
            } else {
                [origin, b" ", clip].concat()
            };
            push_separated(&mut layer, b" ", &boxes);
        } else if eq(origin, clip) {
            push_separated(&mut layer, b" ", origin);
        } else if !(eq(origin, b"padding-box") && eq(clip, b"border-box")) {
            push_separated(&mut layer, b" ", &[origin, b" ", clip].concat());
        }
        if let Some(color) = color.filter(|c| i + 1 == n && !eq(c, b"transparent")) {
            push_separated(&mut layer, b" ", color);
        }
        if layer.is_empty() {
            layer.extend_from_slice(b"none");
        }
        push_separated(&mut out, b", ", &layer);
    }
    Some(out)
}
