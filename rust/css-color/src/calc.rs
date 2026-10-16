//! Southstar — calc() and the CSS math functions inside colour channels, with relative-colour channel keywords.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::{E, PI};

use southstar_css_syntax::{Component, Kind};

use crate::number::calc_number;
use crate::tokens::Tokens;

const MAX_DEPTH: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    Number,
    Percent,
    Angle,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Node {
    Value(f64, Unit),
    Channel(String),
    Sum(Vec<Node>),
    Product(Vec<Node>),
    Negate(Box<Node>),
    Invert(Box<Node>),
    Func(String, Vec<Node>),
}

pub(crate) const MATH_FUNCTIONS: &[&str] = &[
    "calc", "min", "max", "clamp", "abs", "sign", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "pow", "sqrt", "hypot", "log", "exp", "round", "mod", "rem",
];

pub(crate) fn is_math_function(name: &str) -> bool {
    MATH_FUNCTIONS.contains(&name)
}

pub(crate) fn angle_degrees(value: f64, unit: &str) -> Option<f64> {
    Some(match unit {
        "deg" => value,
        "grad" => value * 0.9,
        "rad" => value.to_degrees(),
        "turn" => value * 360.0,
        _ => return None,
    })
}

pub(crate) struct Parser<'c> {
    pub channels: &'c [&'c str],
}

impl Parser<'_> {
    pub fn function(&self, t: &Tokens<'_>, c: &Component) -> Option<Node> {
        self.function_at(t, c, 0)
    }

    fn function_at(&self, t: &Tokens<'_>, c: &Component, depth: u32) -> Option<Node> {
        if depth > MAX_DEPTH {
            return None;
        }
        let name = t.name(c);
        let args = t.children(c);
        if name == "calc" {
            let mut inner = args;
            let node = self.sum(&mut inner, depth + 1)?;
            return inner.at_end().then_some(node);
        }
        if !is_math_function(&name) {
            return None;
        }
        let mut nodes = Vec::new();
        for mut arg in args.split_commas() {
            if name == "round"
                && nodes.is_empty()
                && let Some(c) = arg.peek()
                && c.kind == Kind::Ident
                && ["nearest", "up", "down", "to-zero"].contains(&arg.name(c).as_str())
            {
                nodes.push(Node::Channel(arg.name(c)));
                arg.pos += 1;
                if !arg.at_end() {
                    return None;
                }
                continue;
            }
            let node = self.sum(&mut arg, depth + 1)?;
            if !arg.at_end() {
                return None;
            }
            nodes.push(node);
        }
        let count = nodes.len();
        let arity_ok = match name.as_str() {
            "min" | "max" | "hypot" => count >= 1,
            "clamp" => count == 3,
            "atan2" | "pow" | "mod" | "rem" => count == 2,
            "log" => count == 1 || count == 2,
            "round" => (1..=3).contains(&count),
            _ => count == 1,
        };
        arity_ok.then_some(Node::Func(name, nodes))
    }

    fn sum(&self, t: &mut Tokens<'_>, depth: u32) -> Option<Node> {
        let mut terms = vec![self.product(t, depth)?];
        loop {
            let save = t.pos;
            let had_space = t.skip_ws();
            let Some(c) = t.items.get(t.pos) else {
                return Some(collapse(Node::Sum(terms)));
            };
            if c.kind != Kind::Delim || !matches!(c.delimiter, b'+' | b'-') {
                t.pos = save;
                return Some(collapse(Node::Sum(terms)));
            }
            t.pos += 1;
            let space_after = t
                .items
                .get(t.pos)
                .is_some_and(|c| c.kind == Kind::Whitespace);
            if !had_space || !space_after {
                return None;
            }
            let rhs = self.product(t, depth)?;
            terms.push(if c.delimiter == b'-' {
                Node::Negate(Box::new(rhs))
            } else {
                rhs
            });
        }
    }

    fn product(&self, t: &mut Tokens<'_>, depth: u32) -> Option<Node> {
        let mut factors = vec![self.value(t, depth)?];
        loop {
            let save = t.pos;
            if t.eat_delim(b'*') {
                factors.push(self.value(t, depth)?);
            } else if t.eat_delim(b'/') {
                factors.push(Node::Invert(Box::new(self.value(t, depth)?)));
            } else {
                t.pos = save;
                return Some(collapse(Node::Product(factors)));
            }
        }
    }

    fn value(&self, t: &mut Tokens<'_>, depth: u32) -> Option<Node> {
        let c = t.next()?;
        match c.kind {
            Kind::Number => Some(Node::Value(c.number, Unit::Number)),
            Kind::Percentage => Some(Node::Value(c.number, Unit::Percent)),
            Kind::Dimension => {
                angle_degrees(c.number, &t.unit(c)).map(|deg| Node::Value(deg, Unit::Angle))
            }
            Kind::Ident => {
                let name = t.name(c);
                Some(match name.as_str() {
                    "e" => Node::Value(E, Unit::Number),
                    "pi" => Node::Value(PI, Unit::Number),
                    "infinity" => Node::Value(f64::INFINITY, Unit::Number),
                    "-infinity" => Node::Value(f64::NEG_INFINITY, Unit::Number),
                    "nan" => Node::Value(f64::NAN, Unit::Number),
                    _ if self.channels.contains(&name.as_str()) => Node::Channel(name),
                    _ => return None,
                })
            }
            Kind::Function => self.function_at(t, c, depth + 1),
            Kind::Block if c.delimiter == b')' => {
                let mut inner = t.children(c);
                let node = self.sum(&mut inner, depth + 1)?;
                inner.at_end().then_some(node)
            }
            _ => None,
        }
    }
}

fn collapse(node: Node) -> Node {
    match node {
        Node::Sum(mut v) | Node::Product(mut v) if v.len() == 1 => v.remove(0),
        other => other,
    }
}

pub(crate) fn has_percent(node: &Node) -> bool {
    match node {
        Node::Value(_, unit) => *unit == Unit::Percent,
        Node::Channel(_) => false,
        Node::Sum(nodes) | Node::Product(nodes) | Node::Func(_, nodes) => {
            nodes.iter().any(has_percent)
        }
        Node::Negate(x) | Node::Invert(x) => has_percent(x),
    }
}

pub(crate) fn eval(node: &Node, channel: &dyn Fn(&str) -> Option<f64>) -> Option<(f64, Unit)> {
    use Unit::*;
    Some(match node {
        Node::Value(v, u) => (*v, *u),
        Node::Channel(name) => (channel(name)?, Number),
        Node::Negate(x) => {
            let (v, u) = eval(x, channel)?;
            (-v, u)
        }
        Node::Invert(x) => {
            let (v, u) = eval(x, channel)?;
            if u != Number {
                return None;
            }
            (1.0 / v, Number)
        }
        Node::Sum(terms) => {
            let mut total = 0.0;
            let mut unit = None;
            for term in terms {
                let (v, u) = eval(term, channel)?;
                if unit.is_some_and(|known| known != u) {
                    return None;
                }
                unit = Some(u);
                total += v;
            }
            (total, unit?)
        }
        Node::Product(factors) => {
            let mut total = 1.0;
            let mut unit = Number;
            for factor in factors {
                let (v, u) = eval(factor, channel)?;
                if u != Number {
                    if unit != Number {
                        return None;
                    }
                    unit = u;
                }
                total *= v;
            }
            (total, unit)
        }
        Node::Func(name, args) => eval_function(name, args, channel)?,
    })
}

fn same_unit(values: &[(f64, Unit)]) -> Option<Unit> {
    let unit = values.first()?.1;
    values.iter().all(|v| v.1 == unit).then_some(unit)
}

fn radians(v: (f64, Unit)) -> Option<f64> {
    match v.1 {
        Unit::Number => Some(v.0),
        Unit::Angle => Some(v.0.to_radians()),
        Unit::Percent => None,
    }
}

fn eval_function(
    name: &str,
    args: &[Node],
    channel: &dyn Fn(&str) -> Option<f64>,
) -> Option<(f64, Unit)> {
    use Unit::*;
    if name == "round" {
        return eval_round(args, channel);
    }
    let values = args
        .iter()
        .map(|a| eval(a, channel))
        .collect::<Option<Vec<_>>>()?;
    let first = values[0];
    let number = |v: (f64, Unit)| (v.1 == Number).then_some(v.0);
    Some(match name {
        "min" => (
            values.iter().map(|v| v.0).fold(f64::INFINITY, f64::min),
            same_unit(&values)?,
        ),
        "max" => (
            values.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max),
            same_unit(&values)?,
        ),
        "clamp" => {
            let unit = same_unit(&values)?;
            (values[1].0.min(values[2].0).max(values[0].0), unit)
        }
        "abs" => (first.0.abs(), first.1),
        "sign" => (
            if first.0 == 0.0 || first.0.is_nan() {
                first.0
            } else {
                first.0.signum()
            },
            Number,
        ),
        "sin" => (radians(first)?.sin(), Number),
        "cos" => (radians(first)?.cos(), Number),
        "tan" => (radians(first)?.tan(), Number),
        "asin" => (number(first)?.asin().to_degrees(), Angle),
        "acos" => (number(first)?.acos().to_degrees(), Angle),
        "atan" => (number(first)?.atan().to_degrees(), Angle),
        "atan2" => {
            same_unit(&values)?;
            (first.0.atan2(values[1].0).to_degrees(), Angle)
        }
        "pow" => (number(first)?.powf(number(values[1])?), Number),
        "sqrt" => (number(first)?.sqrt(), Number),
        "hypot" => (
            values.iter().map(|v| v.0 * v.0).sum::<f64>().sqrt(),
            same_unit(&values)?,
        ),
        "log" => {
            let x = number(first)?;
            match values.get(1) {
                Some(&base) => (x.ln() / number(base)?.ln(), Number),
                None => (x.ln(), Number),
            }
        }
        "exp" => (number(first)?.exp(), Number),
        "mod" => {
            let unit = same_unit(&values)?;
            let b = values[1].0;
            let r = first.0 % b;
            (
                if r != 0.0 && (r < 0.0) != (b < 0.0) {
                    r + b
                } else {
                    r
                },
                unit,
            )
        }
        "rem" => (first.0 % values[1].0, same_unit(&values)?),
        _ => return None,
    })
}

fn eval_round(args: &[Node], channel: &dyn Fn(&str) -> Option<f64>) -> Option<(f64, Unit)> {
    let (strategy, rest) = match args.first() {
        Some(Node::Channel(s)) if ["nearest", "up", "down", "to-zero"].contains(&s.as_str()) => {
            (s.as_str(), &args[1..])
        }
        _ => ("nearest", args),
    };
    let (value, unit) = eval(rest.first()?, channel)?;
    let step = match rest.get(1) {
        Some(node) => {
            let (s, u) = eval(node, channel)?;
            if u != unit {
                return None;
            }
            s
        }
        None if unit == Unit::Number => 1.0,
        None => return None,
    };
    let q = value / step;
    let rounded = match strategy {
        "up" => q.ceil(),
        "down" => q.floor(),
        "to-zero" => q.trunc(),
        _ => (q + 0.5).floor(),
    };
    Some((rounded * step, unit))
}

pub(crate) fn simplify(node: &Node) -> Node {
    match node {
        Node::Value(..) | Node::Channel(_) => node.clone(),
        Node::Negate(x) => match simplify(x) {
            Node::Value(v, u) => Node::Value(-v, u),
            Node::Negate(inner) => *inner,
            other => Node::Negate(Box::new(other)),
        },
        Node::Invert(x) => match simplify(x) {
            Node::Value(v, Unit::Number) => Node::Value(1.0 / v, Unit::Number),
            other => Node::Invert(Box::new(other)),
        },
        Node::Sum(terms) => simplify_sum(terms),
        Node::Product(factors) => simplify_product(factors),
        Node::Func(name, args) => {
            let args: Vec<Node> = args.iter().map(simplify).collect();
            let all_values = args.iter().all(|a| matches!(a, Node::Value(..)))
                || (name == "round" && args.iter().skip(1).all(|a| matches!(a, Node::Value(..))));
            if all_values && let Some((v, u)) = eval_function(name, &args, &|_| None) {
                return Node::Value(v, u);
            }
            Node::Func(name.clone(), args)
        }
    }
}

fn simplify_sum(terms: &[Node]) -> Node {
    let mut flat = Vec::new();
    for term in terms.iter().map(simplify) {
        match term {
            Node::Sum(inner) => flat.extend(inner),
            other => flat.push(other),
        }
    }
    let mut values: Vec<(f64, Unit)> = Vec::new();
    let mut rest = Vec::new();
    for term in flat {
        match term {
            Node::Value(v, u) => match values.iter_mut().find(|(_, known)| *known == u) {
                Some(slot) => slot.0 += v,
                None => values.push((v, u)),
            },
            other => rest.push(other),
        }
    }
    let mut out: Vec<Node> = values.into_iter().map(|(v, u)| Node::Value(v, u)).collect();
    out.extend(rest);
    collapse(Node::Sum(out))
}

fn simplify_product(factors: &[Node]) -> Node {
    let mut flat = Vec::new();
    for factor in factors.iter().map(simplify) {
        match factor {
            Node::Product(inner) => flat.extend(inner),
            other => flat.push(other),
        }
    }
    let mut coefficient = 1.0;
    let mut unit = Unit::Number;
    let mut rest = Vec::new();
    for factor in flat {
        match factor {
            Node::Value(v, u) if u == Unit::Number || unit == Unit::Number => {
                coefficient *= v;
                if u != Unit::Number {
                    unit = u;
                }
            }
            other => rest.push(other),
        }
    }
    if rest.is_empty() {
        return Node::Value(coefficient, unit);
    }
    let only_inverts = rest.iter().all(|f| matches!(f, Node::Invert(_)));
    let mut out = Vec::new();
    if coefficient != 1.0 || unit != Unit::Number || only_inverts {
        out.push(Node::Value(coefficient, unit));
    }
    out.extend(rest);
    collapse(Node::Product(out))
}

pub(crate) fn serialize_root(node: &Node) -> String {
    match node {
        Node::Func(name, _) if name != "calc" => serialize(node, false),
        _ => format!("calc({})", serialize(node, false)),
    }
}

fn value_text(v: f64, u: Unit) -> String {
    let text = calc_number(v);
    match u {
        Unit::Number => text,
        Unit::Percent => format!("{text}%"),
        Unit::Angle => format!("{text}deg"),
    }
}

fn serialize(node: &Node, nested: bool) -> String {
    let wrap = |s: String| if nested { format!("({s})") } else { s };
    match node {
        Node::Value(v, u) => value_text(*v, *u),
        Node::Channel(name) => name.clone(),
        Node::Negate(x) => wrap(format!("-1 * {}", serialize(x, true))),
        Node::Invert(x) => wrap(format!("1 / {}", serialize(x, true))),
        Node::Sum(terms) => {
            let mut s = String::new();
            for (i, term) in terms.iter().enumerate() {
                if i == 0 {
                    s.push_str(&serialize(term, true));
                    continue;
                }
                match term {
                    Node::Value(v, u) if *v < 0.0 => {
                        s.push_str(" - ");
                        s.push_str(&value_text(-v, *u));
                    }
                    Node::Negate(x) => {
                        s.push_str(" - ");
                        s.push_str(&serialize(x, true));
                    }
                    other => {
                        s.push_str(" + ");
                        s.push_str(&serialize(other, true));
                    }
                }
            }
            wrap(s)
        }
        Node::Product(factors) => {
            let mut s = String::new();
            for (i, factor) in factors.iter().enumerate() {
                match factor {
                    Node::Invert(x) if i > 0 => {
                        s.push_str(" / ");
                        s.push_str(&serialize(x, true));
                    }
                    other => {
                        if i > 0 {
                            s.push_str(" * ");
                        }
                        s.push_str(&serialize(other, true));
                    }
                }
            }
            wrap(s)
        }
        Node::Func(name, args) => {
            let inner: Vec<String> = args.iter().map(|a| serialize(a, false)).collect();
            if name == "calc" {
                return inner.join(", ");
            }
            format!("{name}({})", inner.join(", "))
        }
    }
}
