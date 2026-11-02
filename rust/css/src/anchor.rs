//! Southstar — CSS Anchor Positioning values: anchor() and anchor-size() expressions with their calc() simplification and evaluation, position-area, and the anchor-name, anchor-scope, position-anchor, position-try and position-visibility values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css_syntax::{Component, Kind};

use crate::content::ident_valid;

const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    Inset,
    Size,
    Margin,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MathKind {
    Min,
    Max,
    Clamp,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Inside,
    Outside,
    Left,
    Right,
    Top,
    Bottom,
    Start,
    End,
    SelfStart,
    SelfEnd,
    Center,
    Percent(f64, bool),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SizeKeyword {
    Width,
    Height,
    Block,
    Inline,
    SelfBlock,
    SelfInline,
}

#[derive(Clone, Debug)]
pub struct AnchorFn {
    pub name: Option<String>,
    pub side: Side,
    pub fallback: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct AnchorSizeFn {
    pub name: Option<String>,
    pub size: Option<SizeKeyword>,
    pub fallback: Option<Expr>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64),
    Dim(f64, String),
    Anchor(Box<AnchorFn>),
    AnchorSize(Box<AnchorSizeFn>),
    Sum(Vec<Expr>),
    Product(f64, Box<Expr>),
    Math(MathKind, Vec<Expr>),
}

const SIDES: [(&str, Side); 11] = [
    ("inside", Side::Inside),
    ("outside", Side::Outside),
    ("left", Side::Left),
    ("right", Side::Right),
    ("top", Side::Top),
    ("bottom", Side::Bottom),
    ("start", Side::Start),
    ("end", Side::End),
    ("self-start", Side::SelfStart),
    ("self-end", Side::SelfEnd),
    ("center", Side::Center),
];

const SIZES: [(&str, SizeKeyword); 6] = [
    ("width", SizeKeyword::Width),
    ("height", SizeKeyword::Height),
    ("block", SizeKeyword::Block),
    ("inline", SizeKeyword::Inline),
    ("self-block", SizeKeyword::SelfBlock),
    ("self-inline", SizeKeyword::SelfInline),
];

const LENGTH_UNITS: [&str; 46] = [
    "px", "em", "rem", "ex", "rex", "ch", "rch", "ic", "ric", "cap", "rcap", "lh", "rlh", "vw",
    "vh", "vi", "vb", "vmin", "vmax", "svw", "svh", "svi", "svb", "svmin", "svmax", "lvw", "lvh",
    "lvi", "lvb", "lvmin", "lvmax", "dvw", "dvh", "dvi", "dvb", "dvmin", "dvmax", "cqw", "cqh",
    "cqi", "cqb", "cqmin", "cqmax", "cm", "mm", "q",
];

const ABSOLUTE_UNITS: [&str; 3] = ["in", "pt", "pc"];

pub fn is_dashed_ident(text: &[u8]) -> bool {
    text.len() > 2 && text.starts_with(b"--") && ident_valid(text)
}

fn text_of<'a>(src: &'a [u8], c: &Component) -> &'a [u8] {
    &src[c.start..c.end.min(src.len())]
}

fn value_of<'a>(src: &'a [u8], c: &Component) -> &'a [u8] {
    c.value.clone().map_or(&[][..], |r| &src[r])
}

fn significant(list: &[Component]) -> Vec<&Component> {
    list.iter().filter(|c| c.kind != Kind::Whitespace).collect()
}

fn split_commas(list: &[Component]) -> Vec<&[Component]> {
    list.split(|c| c.kind == Kind::Comma).collect()
}

fn is_length_unit(unit: &str) -> bool {
    LENGTH_UNITS.contains(&unit) || ABSOLUTE_UNITS.contains(&unit)
}

struct Parser<'s> {
    src: &'s [u8],
    ctx: Context,
    depth: usize,
}

impl<'s> Parser<'s> {
    fn function(&mut self, c: &Component) -> Option<Expr> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let name = value_of(self.src, c).to_ascii_lowercase();
        let children = c.children.as_deref().unwrap_or(&[]);
        let out = match name.as_slice() {
            b"calc" => self.sum(children),
            b"min" | b"max" | b"clamp" => {
                let kind = match name.as_slice() {
                    b"min" => MathKind::Min,
                    b"max" => MathKind::Max,
                    _ => MathKind::Clamp,
                };
                let parts = split_commas(children);
                if kind == MathKind::Clamp && parts.len() != 3 {
                    None
                } else {
                    let args: Option<Vec<Expr>> = parts.iter().map(|p| self.sum(p)).collect();
                    args.filter(|a| !a.is_empty() && a.iter().all(|e| !is_number(e)))
                        .map(|a| Expr::Math(kind, a))
                }
            }
            b"anchor" if self.ctx == Context::Inset => {
                self.anchor(children).map(|f| Expr::Anchor(Box::new(f)))
            }
            b"anchor-size" => self
                .anchor_size(children)
                .map(|f| Expr::AnchorSize(Box::new(f))),
            _ => None,
        };
        self.depth -= 1;
        out
    }

    fn fallback(&mut self, part: &[Component]) -> Option<Expr> {
        let e = self.sum(part)?;
        match e {
            Expr::Number(0.0) => Some(Expr::Dim(0.0, "px".into())),
            Expr::Number(_) => None,
            e => Some(e),
        }
    }

    fn name_and_rest<'c>(
        &self,
        head: &'c [Component],
    ) -> (Option<String>, Vec<&'c Component>, bool) {
        let mut name = None;
        let mut rest = Vec::new();
        let mut ok = true;
        for c in significant(head) {
            let t = text_of(self.src, c);
            if c.kind == Kind::Ident && t.starts_with(b"--") {
                if name.is_some() || !is_dashed_ident(t) {
                    ok = false;
                }
                name = Some(String::from_utf8_lossy(t).into_owned());
            } else {
                rest.push(c);
            }
        }
        (name, rest, ok)
    }

    fn anchor(&mut self, children: &[Component]) -> Option<AnchorFn> {
        let parts = split_commas(children);
        if parts.len() > 2 {
            return None;
        }
        let (name, rest, ok) = self.name_and_rest(parts[0]);
        if !ok || rest.len() != 1 {
            return None;
        }
        let side = self.side(rest[0])?;
        let fallback = match parts.get(1) {
            Some(p) => Some(self.fallback(p)?),
            None => None,
        };
        Some(AnchorFn {
            name,
            side,
            fallback,
        })
    }

    fn side(&mut self, c: &Component) -> Option<Side> {
        match c.kind {
            Kind::Ident => {
                let t = text_of(self.src, c).to_ascii_lowercase();
                SIDES
                    .iter()
                    .find(|(k, _)| k.as_bytes() == t.as_slice())
                    .map(|(_, s)| *s)
            }
            Kind::Percentage => Some(Side::Percent(c.number, false)),
            Kind::Function => {
                let e = self.function(c);
                match e.map(simplify)? {
                    Expr::Dim(v, unit) if unit == "%" => Some(Side::Percent(v, true)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn anchor_size(&mut self, children: &[Component]) -> Option<AnchorSizeFn> {
        let parts = split_commas(children);
        if parts.len() > 2 {
            return None;
        }
        let (name, rest, ok) = self.name_and_rest(parts[0]);
        if !ok {
            return None;
        }
        let mut size = None;
        let mut fallback_part: Option<&[Component]> = parts.get(1).copied();
        match rest.as_slice() {
            [] => {
                if parts.len() == 2 && name.is_none() {
                    return None;
                }
            }
            [one] if one.kind == Kind::Ident => {
                let t = text_of(self.src, one).to_ascii_lowercase();
                size = Some(
                    SIZES
                        .iter()
                        .find(|(k, _)| k.as_bytes() == t.as_slice())
                        .map(|(_, s)| *s)?,
                );
            }
            _ if name.is_none() && parts.len() == 1 => fallback_part = Some(parts[0]),
            _ => return None,
        }
        let fallback = match fallback_part {
            Some(p) => Some(self.fallback(p)?),
            None => None,
        };
        Some(AnchorSizeFn {
            name,
            size,
            fallback,
        })
    }

    fn sum(&mut self, list: &[Component]) -> Option<Expr> {
        let items = significant(list);
        if items.is_empty() {
            return None;
        }
        let mut terms = Vec::new();
        let mut i = 0;
        let mut sign = 1.0;
        loop {
            let start = i;
            while i < items.len() && !is_additive(items[i]) {
                i += 1;
            }
            let product = self.product(&items[start..i])?;
            terms.push(if sign < 0.0 {
                Expr::Product(-1.0, Box::new(product))
            } else {
                product
            });
            if i >= items.len() {
                break;
            }
            sign = if items[i].delimiter == b'-' {
                -1.0
            } else {
                1.0
            };
            let before = list
                .iter()
                .position(|c| core::ptr::eq(c, items[i]))
                .filter(|&p| p > 0 && list[p - 1].kind == Kind::Whitespace);
            let after = list
                .iter()
                .position(|c| core::ptr::eq(c, items[i]))
                .filter(|&p| list.get(p + 1).is_some_and(|c| c.kind == Kind::Whitespace));
            if before.is_none() || after.is_none() {
                return None;
            }
            i += 1;
        }
        if terms.len() > 1 && terms.iter().any(is_number) && !terms.iter().all(is_number) {
            return None;
        }
        Some(if terms.len() == 1 {
            terms.pop().expect("one term")
        } else {
            Expr::Sum(terms)
        })
    }

    fn product(&mut self, items: &[&Component]) -> Option<Expr> {
        if items.is_empty() {
            return None;
        }
        let mut coefficient = 1.0;
        let mut factor: Option<Expr> = None;
        let mut divide = false;
        for (k, c) in items.iter().enumerate() {
            if k % 2 == 1 {
                if c.kind != Kind::Delim || !matches!(c.delimiter, b'*' | b'/') {
                    return None;
                }
                divide = c.delimiter == b'/';
                continue;
            }
            let value = self.value(c)?;
            if divide {
                let Expr::Number(n) = value else { return None };
                if n == 0.0 {
                    return None;
                }
                coefficient /= n;
                continue;
            }
            match value {
                Expr::Number(n) => coefficient *= n,
                other => {
                    if factor.is_some() {
                        return None;
                    }
                    factor = Some(other);
                }
            }
        }
        if items.len().is_multiple_of(2) {
            return None;
        }
        Some(match factor {
            None => Expr::Number(coefficient),
            Some(f) if coefficient == 1.0 => f,
            Some(f) => Expr::Product(coefficient, Box::new(f)),
        })
    }

    fn value(&mut self, c: &Component) -> Option<Expr> {
        match c.kind {
            Kind::Number => Some(Expr::Number(c.number)),
            Kind::Percentage => Some(Expr::Dim(c.number, "%".into())),
            Kind::Dimension => {
                let unit = String::from_utf8_lossy(value_of(self.src, c)).to_ascii_lowercase();
                is_length_unit(&unit).then_some(Expr::Dim(c.number, unit))
            }
            Kind::Function => self.function(c),
            Kind::Block if c.delimiter == b')' => {
                let children = c.children.as_deref().unwrap_or(&[]);
                self.sum(children)
            }
            _ => None,
        }
    }
}

fn is_additive(c: &Component) -> bool {
    c.kind == Kind::Delim && matches!(c.delimiter, b'+' | b'-')
}

fn is_number(e: &Expr) -> bool {
    match e {
        Expr::Number(_) => true,
        Expr::Product(_, inner) => is_number(inner),
        Expr::Sum(terms) => terms.iter().all(is_number),
        Expr::Math(_, args) => args.iter().all(is_number),
        _ => false,
    }
}

fn has_anchor_fn(e: &Expr) -> bool {
    match e {
        Expr::Anchor(_) | Expr::AnchorSize(_) => true,
        Expr::Product(_, inner) => has_anchor_fn(inner),
        Expr::Sum(terms) | Expr::Math(_, terms) => terms.iter().any(has_anchor_fn),
        _ => false,
    }
}

fn unit_rank(e: &Expr) -> (u8, String) {
    match e {
        Expr::Number(_) => (0, String::new()),
        Expr::Dim(_, u) if u == "%" => (1, String::new()),
        Expr::Dim(_, u) => (2, u.clone()),
        _ => (3, String::new()),
    }
}

pub fn simplify(e: Expr) -> Expr {
    match e {
        Expr::Sum(terms) => {
            let mut flat: Vec<Expr> = Vec::new();
            for t in terms.into_iter().map(simplify) {
                match t {
                    Expr::Sum(inner) => flat.extend(inner),
                    t => flat.push(t),
                }
            }
            let mut merged: Vec<Expr> = Vec::new();
            for t in flat {
                let slot = match &t {
                    Expr::Dim(_, u) => merged
                        .iter()
                        .position(|m| matches!(m, Expr::Dim(_, mu) if mu == u)),
                    Expr::Number(_) => merged.iter().position(|m| matches!(m, Expr::Number(_))),
                    _ => None,
                };
                match (slot, t) {
                    (Some(k), Expr::Dim(v, _)) => {
                        if let Expr::Dim(mv, _) = &mut merged[k] {
                            *mv += v;
                        }
                    }
                    (Some(k), Expr::Number(v)) => {
                        if let Expr::Number(mv) = &mut merged[k] {
                            *mv += v;
                        }
                    }
                    (_, t) => merged.push(t),
                }
            }
            merged.sort_by(|a, b| {
                let (ra, ua) = unit_rank(a);
                let (rb, ub) = unit_rank(b);
                ra.cmp(&rb).then_with(|| {
                    if ra == 2 {
                        ua.cmp(&ub)
                    } else {
                        core::cmp::Ordering::Equal
                    }
                })
            });
            if merged.len() == 1 {
                merged.pop().expect("one term")
            } else {
                Expr::Sum(merged)
            }
        }
        Expr::Product(c, inner) => match simplify(*inner) {
            Expr::Dim(v, u) => Expr::Dim(c * v, u),
            Expr::Number(v) => Expr::Number(c * v),
            Expr::Product(c2, inner) => {
                if c * c2 == 1.0 {
                    *inner
                } else {
                    Expr::Product(c * c2, inner)
                }
            }
            Expr::Sum(terms)
                if terms
                    .iter()
                    .all(|t| matches!(t, Expr::Dim(..) | Expr::Number(_))) =>
            {
                simplify(Expr::Sum(
                    terms
                        .into_iter()
                        .map(|t| Expr::Product(c, Box::new(t)))
                        .collect(),
                ))
            }
            other if c == 1.0 => other,
            other => Expr::Product(c, Box::new(other)),
        },
        Expr::Math(kind, args) => {
            let args: Vec<Expr> = args.into_iter().map(simplify).collect();
            let unit = match args.first() {
                Some(Expr::Dim(_, u)) => Some(u.clone()),
                _ => None,
            };
            if let Some(unit) = unit
                && args
                    .iter()
                    .all(|a| matches!(a, Expr::Dim(_, u) if *u == unit))
            {
                let vals: Vec<f64> = args
                    .iter()
                    .map(|a| match a {
                        Expr::Dim(v, _) => *v,
                        _ => 0.0,
                    })
                    .collect();
                let v = match kind {
                    MathKind::Min => vals.iter().copied().fold(f64::INFINITY, f64::min),
                    MathKind::Max => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    MathKind::Clamp => vals[1].min(vals[2]).max(vals[0]),
                };
                return Expr::Dim(v, unit);
            }
            Expr::Math(kind, args)
        }
        Expr::Anchor(mut f) => {
            f.fallback = f.fallback.map(simplify);
            Expr::Anchor(f)
        }
        Expr::AnchorSize(mut f) => {
            f.fallback = f.fallback.map(simplify);
            Expr::AnchorSize(f)
        }
        e => e,
    }
}

fn number_text(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    southstar_numfmt::g(v)
}

fn side_text(side: Side) -> String {
    match side {
        Side::Percent(v, false) => format!("{}%", number_text(v)),
        Side::Percent(v, true) => format!("calc({}%)", number_text(v)),
        s => SIDES
            .iter()
            .find(|(_, k)| *k == s)
            .map_or_else(String::new, |(n, _)| (*n).to_owned()),
    }
}

fn fn_text(e: &Expr) -> String {
    match e {
        Expr::Anchor(f) => {
            let mut out = String::from("anchor(");
            if let Some(n) = &f.name {
                out.push_str(n);
                out.push(' ');
            }
            out.push_str(&side_text(f.side));
            if let Some(fb) = &f.fallback {
                out.push_str(", ");
                out.push_str(&serialize(fb));
            }
            out.push(')');
            out
        }
        Expr::AnchorSize(f) => {
            let mut parts = Vec::new();
            if let Some(n) = &f.name {
                parts.push(n.clone());
            }
            if let Some(s) = f.size {
                let name = SIZES.iter().find(|(_, k)| *k == s).map_or("", |(n, _)| n);
                parts.push(name.to_owned());
            }
            let mut out = String::from("anchor-size(");
            out.push_str(&parts.join(" "));
            if let Some(fb) = &f.fallback {
                if !parts.is_empty() {
                    out.push_str(", ");
                }
                out.push_str(&serialize(fb));
            }
            out.push(')');
            out
        }
        _ => String::new(),
    }
}

fn negated(e: &Expr) -> Option<Expr> {
    match e {
        Expr::Dim(v, u) if *v < 0.0 => Some(Expr::Dim(-v, u.clone())),
        Expr::Number(v) if *v < 0.0 => Some(Expr::Number(-v)),
        Expr::Product(c, inner) if *c < 0.0 => Some(if *c == -1.0 {
            (**inner).clone()
        } else {
            Expr::Product(-c, inner.clone())
        }),
        _ => None,
    }
}

fn inner_text(e: &Expr) -> String {
    match e {
        Expr::Number(v) => number_text(*v),
        Expr::Dim(v, u) => format!("{}{u}", number_text(*v)),
        Expr::Anchor(_) | Expr::AnchorSize(_) => fn_text(e),
        Expr::Sum(terms) => {
            let mut out = String::new();
            for (k, t) in terms.iter().enumerate() {
                if k == 0 {
                    out.push_str(&inner_text(t));
                } else if let Some(n) = negated(t) {
                    out.push_str(" - ");
                    out.push_str(&inner_text(&n));
                } else {
                    out.push_str(" + ");
                    out.push_str(&inner_text(t));
                }
            }
            out
        }
        Expr::Product(c, inner) => {
            let body = match **inner {
                Expr::Sum(_) => format!("({})", inner_text(inner)),
                _ => inner_text(inner),
            };
            if *c == -1.0 {
                format!("-1 * {body}")
            } else {
                format!("{} * {body}", number_text(*c))
            }
        }
        Expr::Math(kind, args) => {
            let name = match kind {
                MathKind::Min => "min",
                MathKind::Max => "max",
                MathKind::Clamp => "clamp",
            };
            let args: Vec<String> = args.iter().map(inner_text).collect();
            format!("{name}({})", args.join(", "))
        }
    }
}

pub fn serialize(e: &Expr) -> String {
    match e {
        Expr::Dim(..)
        | Expr::Number(_)
        | Expr::Anchor(_)
        | Expr::AnchorSize(_)
        | Expr::Math(..) => inner_text(e),
        _ => format!("calc({})", inner_text(e)),
    }
}

pub fn mentions_anchor_fn(text: &[u8]) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.windows(7).any(|w| w == b"anchor(") || lower.windows(12).any(|w| w == b"anchor-size(")
}

pub fn parse(text: &[u8], ctx: Context) -> Option<Expr> {
    let (list, _) = southstar_css_syntax::parse(text);
    let items = significant(&list);
    let mut parser = Parser {
        src: text,
        ctx,
        depth: 0,
    };
    let e = match items.as_slice() {
        [one] if one.kind == Kind::Function => parser.function(one)?,
        _ => return None,
    };
    let e = simplify(e);
    (has_anchor_fn(&e) && !is_number(&e)).then_some(e)
}

pub fn parse_length(text: &[u8], ctx: Context) -> Option<Expr> {
    let (list, _) = southstar_css_syntax::parse(text);
    let items = significant(&list);
    let mut parser = Parser {
        src: text,
        ctx,
        depth: 0,
    };
    let e = match items.as_slice() {
        [one] => parser.value(one)?,
        _ => return None,
    };
    let e = match simplify(e) {
        Expr::Number(0.0) => Expr::Dim(0.0, "px".into()),
        e => e,
    };
    (!is_number(&e)).then_some(e)
}

pub fn is_position_try_name(text: &[u8]) -> bool {
    let (list, _) = southstar_css_syntax::parse(text);
    matches!(significant(&list).as_slice(), [one] if one.kind == Kind::Ident && one.start == 0 && is_dashed_ident(text.trim_ascii()))
}

pub fn position_try_allows(name: &[u8]) -> bool {
    const SIDES: [&str; 10] = [
        "top",
        "right",
        "bottom",
        "left",
        "block",
        "inline",
        "block-start",
        "block-end",
        "inline-start",
        "inline-end",
    ];
    let name = String::from_utf8_lossy(name);
    let name = name.as_ref();
    if let Some(rest) = name.strip_prefix("margin-") {
        return SIDES.contains(&rest);
    }
    if let Some(rest) = name.strip_prefix("inset-") {
        return SIDES[4..].contains(&rest);
    }
    let size = name
        .strip_prefix("min-")
        .or_else(|| name.strip_prefix("max-"))
        .unwrap_or(name);
    matches!(
        name,
        "margin"
            | "inset"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "place-self"
            | "align-self"
            | "justify-self"
            | "position-anchor"
            | "position-area"
    ) || matches!(size, "width" | "height" | "block-size" | "inline-size")
}

pub type PositionTryRule = (Vec<u8>, Vec<(crate::prop::Prop, Vec<u8>)>);

thread_local! {
    static POSITION_TRIES: core::cell::RefCell<std::collections::HashMap<usize, Vec<PositionTryRule>>> =
        core::cell::RefCell::new(std::collections::HashMap::new());
}

thread_local! {
    static IMPLICIT_ANCHORS: core::cell::RefCell<Vec<(southstar_dom::WeakNode, southstar_dom::WeakNode)>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

pub fn set_implicit_anchor(
    element: southstar_dom::Node<'static>,
    source: Option<southstar_dom::Node<'static>>,
) {
    let _ = IMPLICIT_ANCHORS.try_with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|(el, src)| {
            el.get()
                .is_some_and(|e| !core::ptr::eq(e.as_ptr(), element.as_ptr()))
                && src.get().is_some()
        });
        if let Some(source) = source {
            list.push((element.downgrade(), source.downgrade()));
        }
    });
}

pub fn implicit_anchor(element: southstar_dom::Node<'_>) -> Option<southstar_dom::Node<'static>> {
    IMPLICIT_ANCHORS
        .try_with(|list| {
            list.borrow()
                .iter()
                .find(|(el, _)| {
                    el.get()
                        .is_some_and(|e| core::ptr::eq(e.as_ptr(), element.as_ptr()))
                })
                .and_then(|(_, src)| src.get())
        })
        .ok()
        .flatten()
}

pub const USED_FALLBACK: u8 = 1;
pub const ZEROED_INSETS: [u8; 4] = [2, 4, 8, 16];

thread_local! {
    static PLACEMENT_FLAGS: core::cell::RefCell<Vec<(southstar_dom::WeakNode, u8)>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

pub fn set_placement_flags(element: southstar_dom::Node<'_>, flags: u8) {
    let _ = PLACEMENT_FLAGS.try_with(|list| {
        let mut list = list.borrow_mut();
        list.retain(|(w, _)| {
            w.get()
                .is_some_and(|e| !core::ptr::eq(e.as_ptr(), element.as_ptr()))
        });
        if flags != 0 {
            list.push((element.downgrade(), flags));
        }
    });
}

pub fn placement_flags(element: southstar_dom::Node<'_>) -> u8 {
    PLACEMENT_FLAGS
        .try_with(|list| {
            list.borrow()
                .iter()
                .find(|(w, _)| {
                    w.get()
                        .is_some_and(|e| core::ptr::eq(e.as_ptr(), element.as_ptr()))
                })
                .map_or(0, |(_, f)| *f)
        })
        .unwrap_or(0)
}

pub fn register_position_tries(doc: usize, rules: Vec<PositionTryRule>) {
    let _ = POSITION_TRIES.try_with(|map| {
        let mut map = map.borrow_mut();
        if rules.is_empty() {
            map.remove(&doc);
        } else {
            map.insert(doc, rules);
        }
    });
}

pub fn position_try_rule(doc: usize, name: &[u8]) -> Option<Vec<(crate::prop::Prop, Vec<u8>)>> {
    POSITION_TRIES
        .try_with(|map| {
            map.borrow()
                .get(&doc)
                .and_then(|rules| rules.iter().rev().find(|(n, _)| n == name))
                .map(|(_, decls)| decls.clone())
        })
        .ok()
        .flatten()
}

pub fn canonical(text: &[u8], ctx: Context) -> Option<String> {
    parse(text, ctx).map(|e| serialize(&e))
}

pub trait Resolver {
    fn anchor(&self, f: &AnchorFn) -> Option<f64>;
    fn anchor_size(&self, f: &AnchorSizeFn) -> Option<f64>;
    fn length(&self, value: f64, unit: &str) -> f64;
}

pub fn evaluate(e: &Expr, r: &dyn Resolver) -> Option<f64> {
    Some(match e {
        Expr::Number(v) => *v,
        Expr::Dim(v, u) => r.length(*v, u),
        Expr::Anchor(f) => match r.anchor(f) {
            Some(v) => v,
            None => evaluate(f.fallback.as_ref()?, r)?,
        },
        Expr::AnchorSize(f) => match r.anchor_size(f) {
            Some(v) => v,
            None => evaluate(f.fallback.as_ref()?, r)?,
        },
        Expr::Sum(terms) => {
            let mut total = 0.0;
            for t in terms {
                total += evaluate(t, r)?;
            }
            total
        }
        Expr::Product(c, inner) => c * evaluate(inner, r)?,
        Expr::Math(kind, args) => {
            let vals: Option<Vec<f64>> = args.iter().map(|a| evaluate(a, r)).collect();
            let vals = vals?;
            match kind {
                MathKind::Min => vals.iter().copied().fold(f64::INFINITY, f64::min),
                MathKind::Max => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                MathKind::Clamp => vals[1].min(vals[2]).max(vals[0]),
            }
        }
    })
}

fn words(text: &[u8]) -> Vec<&[u8]> {
    text.split(|c| c.is_ascii_whitespace())
        .filter(|w| !w.is_empty())
        .collect()
}

fn comma_items(text: &[u8]) -> Option<Vec<&[u8]>> {
    let items: Vec<&[u8]> = text.split(|&c| c == b',').map(<[u8]>::trim_ascii).collect();
    items.iter().all(|i| !i.is_empty()).then_some(items)
}

fn dashed_list(text: &[u8]) -> Option<String> {
    let items = comma_items(text)?;
    let names: Option<Vec<String>> = items
        .iter()
        .map(|i| is_dashed_ident(i).then(|| String::from_utf8_lossy(i).into_owned()))
        .collect();
    names.map(|n| n.join(", "))
}

fn one_of(text: &[u8], choices: &[&str]) -> Option<String> {
    let lower = text.trim_ascii().to_ascii_lowercase();
    choices
        .iter()
        .find(|c| c.as_bytes() == lower.as_slice())
        .map(|c| (*c).to_owned())
}

pub fn anchor_name(text: &[u8]) -> Option<String> {
    one_of(text, &["none"]).or_else(|| dashed_list(text))
}

pub fn anchor_scope(text: &[u8]) -> Option<String> {
    one_of(text, &["none", "all"]).or_else(|| dashed_list(text))
}

pub fn position_anchor(text: &[u8]) -> Option<String> {
    let t = text.trim_ascii();
    one_of(t, &["normal", "none", "auto", "match-parent"])
        .or_else(|| is_dashed_ident(t).then(|| String::from_utf8_lossy(t).into_owned()))
}

pub fn position_try_order(text: &[u8]) -> Option<String> {
    one_of(
        text,
        &[
            "normal",
            "most-width",
            "most-height",
            "most-block-size",
            "most-inline-size",
        ],
    )
}

pub fn position_visibility(text: &[u8]) -> Option<String> {
    let t = text.trim_ascii();
    if let Some(always) = one_of(t, &["always"]) {
        return Some(always);
    }
    const FLAGS: [&str; 3] = ["anchor-valid", "anchor-visible", "no-overflow"];
    let mut seen = [false; 3];
    let list = words(t);
    if list.is_empty() {
        return None;
    }
    for w in list {
        let lower = w.to_ascii_lowercase();
        let k = FLAGS
            .iter()
            .position(|f| f.as_bytes() == lower.as_slice())?;
        if seen[k] {
            return None;
        }
        seen[k] = true;
    }
    Some(
        FLAGS
            .iter()
            .zip(seen)
            .filter(|(_, s)| *s)
            .map(|(f, _)| *f)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

const TACTICS: [&str; 5] = [
    "flip-block",
    "flip-inline",
    "flip-start",
    "flip-x",
    "flip-y",
];

fn try_fallback_item(text: &[u8]) -> Option<String> {
    if let Some(area) = position_area_specified(text)
        && area != "none"
    {
        return Some(area);
    }
    let mut name: Option<String> = None;
    let mut tactics: Vec<&str> = Vec::new();
    let list = words(text);
    for (k, w) in list.iter().enumerate() {
        if w.starts_with(b"--") {
            if name.is_some() || !is_dashed_ident(w) || (k != 0 && k != list.len() - 1) {
                return None;
            }
            name = Some(String::from_utf8_lossy(w).into_owned());
            continue;
        }
        let lower = w.to_ascii_lowercase();
        let t = TACTICS.iter().find(|t| t.as_bytes() == lower.as_slice())?;
        if tactics.contains(t) {
            return None;
        }
        tactics.push(t);
    }
    let mut parts: Vec<String> = name.into_iter().collect();
    parts.extend(tactics.iter().map(|t| (*t).to_owned()));
    (!parts.is_empty()).then(|| parts.join(" "))
}

pub fn position_try_fallbacks(text: &[u8]) -> Option<String> {
    if let Some(none) = one_of(text, &["none"]) {
        return Some(none);
    }
    let items = comma_items(text)?;
    let parsed: Option<Vec<String>> = items.iter().map(|i| try_fallback_item(i)).collect();
    parsed.map(|p| p.join(", "))
}

pub fn position_try_split(text: &[u8]) -> Option<(String, String)> {
    let t = text.trim_ascii();
    let first_end = t
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(t.len());
    if let Some(order) = position_try_order(&t[..first_end])
        && first_end < t.len()
    {
        let fallbacks = position_try_fallbacks(&t[first_end..])?;
        return Some((order, fallbacks));
    }
    position_try_fallbacks(t).map(|f| ("normal".to_owned(), f))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AreaAxis {
    X,
    Y,
    Inline,
    Block,
    SelfInline,
    SelfBlock,
    Either,
    SelfEither,
    Both,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AreaSpan {
    Start,
    End,
    SpanStart,
    SpanEnd,
    Center,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AreaKeyword {
    pub axis: AreaAxis,
    pub span: AreaSpan,
    pub self_relative: bool,
}

fn area_keyword(word: &[u8]) -> Option<(AreaKeyword, String)> {
    let lower = String::from_utf8_lossy(word).to_ascii_lowercase();
    let mut kw = lower.as_str();
    if kw == "center" || kw == "span-all" {
        let span = if kw == "center" {
            AreaSpan::Center
        } else {
            AreaSpan::All
        };
        return Some((
            AreaKeyword {
                axis: AreaAxis::Both,
                span,
                self_relative: false,
            },
            lower,
        ));
    }
    let spanning = kw.starts_with("span-");
    if spanning {
        kw = &kw[5..];
    }
    let physical = match kw {
        "left" => Some((AreaAxis::X, true)),
        "right" => Some((AreaAxis::X, false)),
        "top" => Some((AreaAxis::Y, true)),
        "bottom" => Some((AreaAxis::Y, false)),
        _ => None,
    };
    let (axis, start, self_relative) = if let Some((axis, start)) = physical {
        (axis, start, false)
    } else {
        let self_relative = kw.starts_with("self-");
        let rest = if self_relative { &kw[5..] } else { kw };
        let (axis_word, edge) = match rest.rsplit_once('-') {
            Some((a, e)) if e == "start" || e == "end" => (a, e),
            None if rest == "start" || rest == "end" => ("", rest),
            _ => return None,
        };
        let axis = match (axis_word, self_relative) {
            ("x", _) => AreaAxis::X,
            ("y", _) => AreaAxis::Y,
            ("inline", false) => AreaAxis::Inline,
            ("block", false) => AreaAxis::Block,
            ("inline", true) => AreaAxis::SelfInline,
            ("block", true) => AreaAxis::SelfBlock,
            ("", false) => AreaAxis::Either,
            ("", true) => AreaAxis::SelfEither,
            _ => return None,
        };
        (axis, edge == "start", self_relative)
    };
    let span = match (spanning, start) {
        (false, true) => AreaSpan::Start,
        (false, false) => AreaSpan::End,
        (true, true) => AreaSpan::SpanStart,
        (true, false) => AreaSpan::SpanEnd,
    };
    Some((
        AreaKeyword {
            axis,
            span,
            self_relative,
        },
        lower,
    ))
}

fn first_slot(axis: AreaAxis) -> Option<bool> {
    match axis {
        AreaAxis::X | AreaAxis::Block | AreaAxis::SelfBlock => Some(true),
        AreaAxis::Y | AreaAxis::Inline | AreaAxis::SelfInline => Some(false),
        _ => None,
    }
}

fn pair_compatible(a: AreaAxis, b: AreaAxis) -> bool {
    use AreaAxis as A;
    matches!(
        (a, b),
        (A::X, A::Y)
            | (A::Y, A::X)
            | (A::Block, A::Inline)
            | (A::Inline, A::Block)
            | (A::SelfBlock, A::SelfInline)
            | (A::SelfInline, A::SelfBlock)
            | (A::Either, A::Either)
            | (A::SelfEither, A::SelfEither)
    ) || a == A::Both
        || b == A::Both
}

#[derive(Clone, Debug)]
pub struct Area {
    pub first: (AreaKeyword, String),
    pub second: (AreaKeyword, String),
    pub single: bool,
}

pub fn position_area(text: &[u8]) -> Option<Area> {
    let list = words(text);
    match list.as_slice() {
        [one] => {
            let k = area_keyword(one)?;
            let other = match k.0.axis {
                AreaAxis::Both | AreaAxis::Either | AreaAxis::SelfEither => k.clone(),
                _ => area_keyword(b"span-all")?,
            };
            let first_is_k = first_slot(k.0.axis) != Some(false);
            Some(if first_is_k {
                Area {
                    first: k,
                    second: other,
                    single: true,
                }
            } else {
                Area {
                    first: other,
                    second: k,
                    single: true,
                }
            })
        }
        [a, b] => {
            let a = area_keyword(a)?;
            let b = area_keyword(b)?;
            if !pair_compatible(a.0.axis, b.0.axis) {
                return None;
            }
            if a.0.axis == AreaAxis::Both && matches!(b.0.axis, AreaAxis::Both) {
                return Some(Area {
                    first: a,
                    second: b,
                    single: false,
                });
            }
            let swap = first_slot(a.0.axis) == Some(false) || first_slot(b.0.axis) == Some(true);
            Some(if swap {
                Area {
                    first: b,
                    second: a,
                    single: false,
                }
            } else {
                Area {
                    first: a,
                    second: b,
                    single: false,
                }
            })
        }
        _ => None,
    }
}

fn explicit(axis: AreaAxis) -> bool {
    first_slot(axis).is_some()
}

pub fn position_area_specified(text: &[u8]) -> Option<String> {
    if let Some(none) = one_of(text, &["none"]) {
        return Some(none);
    }
    let area = position_area(text)?;
    let (a, b) = (&area.first, &area.second);
    if area.single || a.1 == b.1 {
        return Some(if explicit(b.0.axis) && !explicit(a.0.axis) {
            b.1.clone()
        } else {
            a.1.clone()
        });
    }
    if a.0.span == AreaSpan::All && explicit(b.0.axis) {
        return Some(b.1.clone());
    }
    if b.0.span == AreaSpan::All && explicit(a.0.axis) {
        return Some(a.1.clone());
    }
    Some(format!("{} {}", a.1, b.1))
}

fn short_logical(word: &str) -> String {
    word.replace("block-", "").replace("inline-", "")
}

pub fn position_area_computed(text: &[u8]) -> Option<String> {
    if let Some(none) = one_of(text, &["none"]) {
        return Some(none);
    }
    let area = position_area(text)?;
    let (a, b) = (&area.first, &area.second);
    if a.1 == b.1 {
        return Some(a.1.clone());
    }
    if b.0.span == AreaSpan::All && explicit(a.0.axis) {
        return Some(a.1.clone());
    }
    if a.0.span == AreaSpan::All && explicit(b.0.axis) {
        return Some(b.1.clone());
    }
    let expand = |k: &(AreaKeyword, String), block: bool| -> String {
        let word = &k.1;
        let (prefix, rest) = match word.strip_prefix("span-") {
            Some(r) => ("span-", r),
            None => ("", word.as_str()),
        };
        let (self_part, edge) = match rest.strip_prefix("self-") {
            Some(r) => ("self-", r),
            None => ("", rest),
        };
        let axis = if block { "block-" } else { "inline-" };
        format!("{prefix}{self_part}{axis}{edge}")
    };
    if b.0.span == AreaSpan::All && matches!(a.0.axis, AreaAxis::Either | AreaAxis::SelfEither) {
        return Some(expand(a, true));
    }
    if a.0.span == AreaSpan::All && matches!(b.0.axis, AreaAxis::Either | AreaAxis::SelfEither) {
        return Some(expand(b, false));
    }
    let logical = |k: &(AreaKeyword, String)| {
        matches!(
            k.0.axis,
            AreaAxis::Block | AreaAxis::Inline | AreaAxis::SelfBlock | AreaAxis::SelfInline
        )
    };
    let word = |k: &(AreaKeyword, String)| {
        if logical(k) {
            short_logical(&k.1)
        } else {
            k.1.clone()
        }
    };
    let (wa, wb) = (word(a), word(b));
    if wa == wb {
        return Some(wa);
    }
    Some(format!("{wa} {wb}"))
}
