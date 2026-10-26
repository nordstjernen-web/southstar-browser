//! Southstar — CSS gap decorations: the list-valued column and row rule longhands with repeat(), the rule, rule-inset and bidirectional shorthands, and their serialization from longhands.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::calc::Parsed;
use crate::prop::Prop;
use crate::property::{self, Body, c_text, calc_value, eq, length, lower, wide_keyword};
use crate::scan::{match_close_paren, split_ws_paren, starts_with_ci, strip};
use crate::text::split_top_level_commas;
use crate::units::{self, EM, NUMBER, PERCENT, PX, REM, Unit, VH, VMAX, VMIN, VW};
use crate::{math, values};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    Width,
    Style,
    Color,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Count {
    Auto,
    Times(u32),
}

struct Entry<T> {
    count: Option<Count>,
    items: Vec<T>,
}

const STYLES: [&[u8]; 10] = [
    b"none", b"hidden", b"dotted", b"dashed", b"solid", b"double", b"groove", b"ridge", b"inset",
    b"outset",
];

const DEFAULTS: [&[u8]; 3] = [b"medium", b"none", b"currentcolor"];

const COLUMN_RULE: [Prop; 3] = [
    Prop::ColumnRuleWidth,
    Prop::ColumnRuleStyle,
    Prop::ColumnRuleColor,
];

const ROW_RULE: [Prop; 3] = [Prop::RowRuleWidth, Prop::RowRuleStyle, Prop::RowRuleColor];

const COLUMN_INSETS: [Prop; 4] = [
    Prop::ColumnRuleInsetCapStart,
    Prop::ColumnRuleInsetCapEnd,
    Prop::ColumnRuleInsetJunctionStart,
    Prop::ColumnRuleInsetJunctionEnd,
];

const ROW_INSETS: [Prop; 4] = [
    Prop::RowRuleInsetCapStart,
    Prop::RowRuleInsetCapEnd,
    Prop::RowRuleInsetJunctionStart,
    Prop::RowRuleInsetJunctionEnd,
];

#[derive(Clone, Copy)]
enum Inset {
    Cap,
    Junction,
    Start,
    End,
    All,
}

impl Inset {
    fn members(self) -> &'static [usize] {
        match self {
            Inset::Cap => &[0, 1],
            Inset::Junction => &[2, 3],
            Inset::Start => &[0, 2],
            Inset::End => &[1, 3],
            Inset::All => &[0, 1, 2, 3],
        }
    }
}

#[derive(Clone, Copy)]
enum Shorthand {
    Rule(&'static [Prop; 3]),
    BothRules,
    Both(Prop, Prop),
    Inset(&'static [Prop; 4], Inset),
    BothInsets(Inset),
}

fn shorthand(name: &[u8]) -> Option<Shorthand> {
    let lower = lower(name);
    let name = lower.as_slice();
    if let Some(rest) = name.strip_prefix(b"rule-inset") {
        return inset_kind(rest).map(Shorthand::BothInsets);
    }
    for (prefix, insets) in [
        (&b"column-rule-inset"[..], &COLUMN_INSETS),
        (b"row-rule-inset", &ROW_INSETS),
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return inset_kind(rest).map(|kind| Shorthand::Inset(insets, kind));
        }
    }
    Some(match name {
        b"column-rule" => Shorthand::Rule(&COLUMN_RULE),
        b"row-rule" => Shorthand::Rule(&ROW_RULE),
        b"rule" => Shorthand::BothRules,
        b"rule-width" => Shorthand::Both(Prop::ColumnRuleWidth, Prop::RowRuleWidth),
        b"rule-style" => Shorthand::Both(Prop::ColumnRuleStyle, Prop::RowRuleStyle),
        b"rule-color" => Shorthand::Both(Prop::ColumnRuleColor, Prop::RowRuleColor),
        b"rule-break" => Shorthand::Both(Prop::ColumnRuleBreak, Prop::RowRuleBreak),
        b"rule-visibility-items" => Shorthand::Both(
            Prop::ColumnRuleVisibilityItems,
            Prop::RowRuleVisibilityItems,
        ),
        _ => return None,
    })
}

fn inset_kind(rest: &[u8]) -> Option<Inset> {
    Some(match rest {
        b"" => Inset::All,
        b"-cap" => Inset::Cap,
        b"-junction" => Inset::Junction,
        b"-start" => Inset::Start,
        b"-end" => Inset::End,
        _ => return None,
    })
}

pub(crate) fn part_of(prop: Prop) -> Option<Part> {
    match prop {
        Prop::ColumnRuleWidth | Prop::RowRuleWidth => Some(Part::Width),
        Prop::ColumnRuleStyle | Prop::RowRuleStyle => Some(Part::Style),
        Prop::ColumnRuleColor | Prop::RowRuleColor => Some(Part::Color),
        _ => None,
    }
}

fn is_inset(prop: Prop) -> bool {
    COLUMN_INSETS.contains(&prop) || ROW_INSETS.contains(&prop)
}

fn repeat_count(t: &[u8]) -> Option<Count> {
    if eq(t, b"auto") {
        return Some(Count::Auto);
    }
    let n = match calc_value(t).or_else(|| length(t).map(|(v, unit)| Parsed::Length(v, unit)))? {
        Parsed::Length(v, NUMBER) => v,
        _ => return None,
    };
    (n >= 1.0 && n.fract() == 0.0 && n <= f64::from(u32::MAX)).then_some(Count::Times(n as u32))
}

fn repeat_args(part: &[u8]) -> Option<&[u8]> {
    if !starts_with_ci(part, b"repeat(") {
        return None;
    }
    let close = match_close_paren(part, 7, part.len())?;
    (close + 1 == part.len()).then(|| &part[7..close])
}

fn parse_list<T>(text: &[u8], item: &dyn Fn(&[u8]) -> Option<T>) -> Option<Vec<Entry<T>>> {
    let mut entries = Vec::new();
    let mut autos = 0;
    for part in split_top_level_commas(text) {
        let part = strip(&part);
        if part.is_empty() {
            return None;
        }
        let Some(inner) = repeat_args(part) else {
            entries.push(Entry {
                count: None,
                items: vec![item(part)?],
            });
            continue;
        };
        let args = split_top_level_commas(inner);
        let (first, rest) = args.split_first()?;
        let count = repeat_count(strip(first))?;
        if count == Count::Auto {
            autos += 1;
        }
        let items = rest
            .iter()
            .map(|arg| Some(strip(arg)).filter(|a| !a.is_empty()).and_then(item))
            .collect::<Option<Vec<T>>>()?;
        if items.is_empty() {
            return None;
        }
        entries.push(Entry {
            count: Some(count),
            items,
        });
    }
    (autos <= 1 && !entries.is_empty()).then_some(entries)
}

fn serialize_list<T>(entries: &[Entry<T>], item: &dyn Fn(&T) -> Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        let items: Vec<Vec<u8>> = entry.items.iter().map(item).collect();
        match entry.count {
            None => out.extend_from_slice(&items.concat()),
            Some(count) => {
                out.extend_from_slice(b"repeat(");
                match count {
                    Count::Auto => out.extend_from_slice(b"auto"),
                    Count::Times(n) => out.extend_from_slice(n.to_string().as_bytes()),
                }
                for item in items {
                    out.extend_from_slice(b", ");
                    out.extend_from_slice(&item);
                }
                out.push(b')');
            }
        }
    }
    out
}

fn same_shape<A, B>(a: &[Entry<A>], b: &[Entry<B>]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.count == y.count && x.items.len() == y.items.len())
}

fn width_item(t: &[u8]) -> Option<Vec<u8>> {
    if eq(t, b"thin") || eq(t, b"medium") || eq(t, b"thick") {
        return Some(lower(t));
    }
    if t.contains(&b'(') {
        return match calc_value(t)? {
            Parsed::Length(v, unit) if unit != PERCENT && (unit != NUMBER || v == 0.0) => {
                math::math_canonical(&c_text(t))
            }
            Parsed::Calc(c) if c.pct == 0.0 => math::math_canonical(&c_text(t)),
            _ => None,
        };
    }
    let (v, unit) = length(t)?;
    if unit == PERCENT || v < 0.0 || (unit == NUMBER && v != 0.0) {
        return None;
    }
    let unit = if unit == NUMBER { PX } else { unit };
    Some(values::g_unit(v, unit))
}

fn style_item(t: &[u8]) -> Option<Vec<u8>> {
    let kw = lower(t);
    STYLES.contains(&kw.as_slice()).then_some(kw)
}

fn color_item(t: &[u8]) -> Option<Vec<u8>> {
    values::color_canonical(t)
}

fn item_of(part: Part) -> fn(&[u8]) -> Option<Vec<u8>> {
    match part {
        Part::Width => width_item,
        Part::Style => style_item,
        Part::Color => color_item,
    }
}

pub(crate) fn longhand_canonical(part: Part, t: &[u8]) -> Option<Vec<u8>> {
    let entries = parse_list(t, &item_of(part))?;
    Some(serialize_list(&entries, &|item: &Vec<u8>| item.clone()))
}

pub(crate) fn is_list(canonical: &[u8]) -> bool {
    starts_with_ci(canonical, b"repeat(") || split_top_level_commas(canonical).len() > 1
}

fn inset_value(t: &[u8]) -> Option<Vec<u8>> {
    if wide_keyword(t).is_some() {
        return None;
    }
    property::parse(Prop::ColumnRuleInsetCapStart, t)?;
    if !t.contains(&b'(')
        && let Some((v, unit)) = length(t)
    {
        return Some(values::g_unit(v, if unit == NUMBER { PX } else { unit }));
    }
    if t.contains(&b'(') {
        return math::math_canonical(&c_text(t)).or_else(|| Some(lower(t)));
    }
    Some(lower(t))
}

pub(crate) fn specified(prop: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    let id = Prop::from_name(prop)?;
    let t = strip(value);
    if wide_keyword(t).is_some() {
        return None;
    }
    if let Some(part) = part_of(id) {
        return longhand_canonical(part, t);
    }
    if is_inset(id) {
        return inset_value(t);
    }
    None
}

type RuleItem = [Option<Vec<u8>>; 3];

fn rule_item(t: &[u8]) -> Option<RuleItem> {
    let tokens = split_ws_paren(t, 4);
    if tokens.is_empty() || tokens.len() > 3 {
        return None;
    }
    let mut item: RuleItem = Default::default();
    for tok in tokens {
        let (slot, text) = if let Some(s) = style_item(tok) {
            (1, s)
        } else if let Some(w) = width_item(tok) {
            (0, w)
        } else if let Some(c) = color_item(tok) {
            (2, c)
        } else {
            return None;
        };
        if item[slot].is_some() {
            return None;
        }
        item[slot] = Some(text);
    }
    Some(item)
}

fn rule_longhands(text: &[u8]) -> Option<[Vec<u8>; 3]> {
    let entries = parse_list(text, &rule_item)?;
    Some([0, 1, 2].map(|k| {
        serialize_list(&entries, &|item: &RuleItem| {
            item[k].clone().unwrap_or_else(|| DEFAULTS[k].to_vec())
        })
    }))
}

fn inset_pair(t: &[u8]) -> Option<[Vec<u8>; 2]> {
    let tokens = split_ws_paren(t, 3);
    match tokens[..] {
        [a] => {
            let a = inset_value(a)?;
            Some([a.clone(), a])
        }
        [a, b] => Some([inset_value(a)?, inset_value(b)?]),
        _ => None,
    }
}

fn inset_longhands(kind: Inset, t: &[u8]) -> Option<Vec<Vec<u8>>> {
    match kind {
        Inset::Start | Inset::End => {
            let v = inset_value(t)?;
            Some(vec![v.clone(), v])
        }
        Inset::Cap | Inset::Junction => Some(inset_pair(t)?.to_vec()),
        Inset::All => {
            let halves: Vec<&[u8]> = t.split(|&c| c == b'/').collect();
            let (cap, junction) = match halves[..] {
                [both] => {
                    let pair = inset_pair(both)?;
                    (pair.clone(), pair)
                }
                [cap, junction] => (inset_pair(cap)?, inset_pair(junction)?),
                _ => return None,
            };
            Some([cap, junction].concat())
        }
    }
}

fn wide_for(props: &[Prop], t: &[u8]) -> Option<Vec<(Prop, Vec<u8>)>> {
    let Body::Keyword(kw) = wide_keyword(t)?.body else {
        return None;
    };
    Some(props.iter().map(|&p| (p, kw.clone())).collect())
}

fn members(sh: Shorthand) -> Vec<Prop> {
    match sh {
        Shorthand::Rule(props) => props.to_vec(),
        Shorthand::BothRules => [COLUMN_RULE, ROW_RULE].concat(),
        Shorthand::Both(a, b) => vec![a, b],
        Shorthand::Inset(props, kind) => kind.members().iter().map(|&i| props[i]).collect(),
        Shorthand::BothInsets(kind) => kind
            .members()
            .iter()
            .map(|&i| COLUMN_INSETS[i])
            .chain(kind.members().iter().map(|&i| ROW_INSETS[i]))
            .collect(),
    }
}

pub(crate) fn expand(name: &[u8], text: &[u8]) -> Option<Vec<(Prop, Vec<u8>)>> {
    let sh = shorthand(name)?;
    let props = members(sh);
    let t = strip(text);
    if let Some(wide) = wide_for(&props, t) {
        return Some(wide);
    }
    let values: Option<Vec<Vec<u8>>> = match sh {
        Shorthand::Rule(_) => rule_longhands(t).map(|v| v.to_vec()),
        Shorthand::BothRules => rule_longhands(t).map(|v| [v.clone(), v].concat()),
        Shorthand::Both(a, _) => {
            let one = match part_of(a) {
                Some(part) => longhand_canonical(part, t),
                None => property::parse(a, t).map(|_| lower(t)),
            };
            one.map(|v| vec![v.clone(), v])
        }
        Shorthand::Inset(_, kind) => inset_longhands(kind, t),
        Shorthand::BothInsets(kind) => inset_longhands(kind, t).map(|v| [v.clone(), v].concat()),
    };
    Some(match values {
        Some(values) => props.into_iter().zip(values).collect(),
        None => Vec::new(),
    })
}

fn raw_list(text: &[u8]) -> Option<Vec<Entry<Vec<u8>>>> {
    parse_list(text, &|t: &[u8]| Some(t.to_vec()))
}

fn compose_rule(parts: &[Vec<u8>]) -> Vec<u8> {
    let lists: Option<Vec<_>> = parts.iter().map(|p| raw_list(p)).collect();
    let Some(lists) = lists else {
        return Vec::new();
    };
    if !same_shape(&lists[0], &lists[1]) || !same_shape(&lists[0], &lists[2]) {
        return Vec::new();
    }
    let zipped: Vec<Entry<[&[u8]; 3]>> = (0..lists[0].len())
        .map(|e| Entry {
            count: lists[0][e].count,
            items: (0..lists[0][e].items.len())
                .map(|i| [0, 1, 2].map(|k| lists[k][e].items[i].as_slice()))
                .collect(),
        })
        .collect();
    serialize_list(&zipped, &|item: &[&[u8]; 3]| {
        let shown: Vec<&[u8]> = (0..3)
            .filter(|&k| !eq(item[k], DEFAULTS[k]))
            .map(|k| item[k])
            .collect();
        if shown.is_empty() {
            DEFAULTS[0].to_vec()
        } else {
            shown.join(&b' ')
        }
    })
}

fn compose_insets(kind: Inset, v: &[Vec<u8>]) -> Vec<u8> {
    match kind {
        Inset::Start | Inset::End => {
            if v[0] == v[1] {
                v[0].clone()
            } else {
                Vec::new()
            }
        }
        Inset::Cap | Inset::Junction => {
            if v[0] == v[1] {
                v[0].clone()
            } else {
                [&v[0][..], b" ", &v[1]].concat()
            }
        }
        Inset::All => {
            if v.iter().all(|x| *x == v[0]) {
                v[0].clone()
            } else {
                [&v[0][..], b" ", &v[1], b" / ", &v[2], b" ", &v[3]].concat()
            }
        }
    }
}

fn agree(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    if a == b { a } else { Vec::new() }
}

fn compose_values(sh: Shorthand, v: &[Vec<u8>]) -> Vec<u8> {
    match sh {
        Shorthand::Rule(_) => compose_rule(v),
        Shorthand::BothRules => agree(compose_rule(&v[..3]), compose_rule(&v[3..])),
        Shorthand::Both(..) => agree(v[0].clone(), v[1].clone()),
        Shorthand::Inset(_, kind) => compose_insets(kind, v),
        Shorthand::BothInsets(kind) => {
            let half = v.len() / 2;
            agree(
                compose_insets(kind, &v[..half]),
                compose_insets(kind, &v[half..]),
            )
        }
    }
}

pub fn shorthand_compose(
    name: &[u8],
    get: &mut dyn FnMut(&[u8]) -> Option<Vec<u8>>,
) -> Option<Option<Vec<u8>>> {
    let sh = shorthand(name)?;
    let mut values = Vec::new();
    for prop in members(sh) {
        match get(prop.name()).filter(|v| !v.is_empty()) {
            Some(v) => values.push(v),
            None => return Some(None),
        }
    }
    if values.iter().any(|v| wide_keyword(v).is_some()) {
        let same = values.iter().all(|v| eq(v, &values[0]));
        return Some(Some(if same { lower(&values[0]) } else { Vec::new() }));
    }
    Some(Some(compose_values(sh, &values)))
}

fn length_px(v: f64, unit: Unit, font_px: f64) -> Option<f64> {
    let (vw, vh) = crate::ffi::viewport::get();
    Some(match unit {
        PX | NUMBER => v,
        EM => v * font_px,
        REM => v * 16.0,
        VW => v * vw / 100.0,
        VH => v * vh / 100.0,
        VMIN => v * vw.min(vh) / 100.0,
        VMAX => v * vw.max(vh) / 100.0,
        units::EX | units::CH => v * font_px * 0.5,
        _ => return None,
    })
}

fn width_px(item: &[u8], font_px: f64) -> Option<f64> {
    if let Some(k) = [&b"thin"[..], b"medium", b"thick"]
        .iter()
        .position(|kw| eq(item, kw))
    {
        return Some([1.0, 3.0, 5.0][k]);
    }
    let px = match property::parse(Prop::ColumnRuleWidth, item)?.body {
        Body::Length(v, unit) => length_px(v, unit, font_px)?,
        Body::Calc(c) if c.func != 0 => crate::calc::math_fn_px(&c, 0.0),
        Body::Calc(c) => {
            let (vw, vh) = crate::ffi::viewport::get();
            c.px + c.em * font_px
                + c.rem * 16.0
                + (c.vw * vw + c.vh * vh + c.vmin * vw.min(vh) + c.vmax * vw.max(vh)) / 100.0
        }
        _ => return None,
    };
    Some(px.max(0.0))
}

fn color_computed(item: &[u8], current: &[u8]) -> Option<Vec<u8>> {
    match property::parse(Prop::ColumnRuleColor, item)?.body {
        Body::Color(c) => Some(c.serialize().into_bytes()),
        Body::Keyword(_) => Some(current.to_vec()),
        _ => None,
    }
}

pub fn list_computed(prop: &[u8], text: &[u8], font_px: f64, current: &[u8]) -> Option<Vec<u8>> {
    let part = part_of(Prop::from_name(prop)?)?;
    let entries = raw_list(text)?;
    Some(serialize_list(&entries, &|item: &Vec<u8>| match part {
        Part::Style => item.clone(),
        Part::Width => match width_px(item, font_px) {
            Some(px) => values::g_unit(px, PX),
            None => item.clone(),
        },
        Part::Color => color_computed(item, current).unwrap_or_else(|| item.clone()),
    }))
}
