//! Southstar — CSS container queries: the query containers laid out for a page, the stack of ancestor containers while styles cascade, container-relative units, and @container conditions parsed, serialized and evaluated against the nearest matching container.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use std::ffi::{CStr, CString};

use crate::calc;
use crate::ffi::{self, Container};
use crate::scan::{
    is_ws, match_paren_quoted, skip_ws, split_ws_limit, split_ws_paren, starts_with_ci, strip,
};
use crate::units::{self, CQH, CQMAX, CQMIN, CQW, NUMBER, Unit};

pub(crate) const TYPE_INLINE: i32 = 1;
pub(crate) const TYPE_SIZE: i32 = 2;

const MAX_NESTING: i32 = 32;

thread_local! {
    static STACK: RefCell<Vec<Container>> = const { RefCell::new(Vec::new()) };
    static FEATURES_USED: Cell<bool> = const { Cell::new(false) };
    static UNIT_DIMS: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
}

pub(crate) fn set_unit_dims(inline_px: f64, block_px: f64) {
    UNIT_DIMS.with(|dims| dims.set((inline_px, block_px)));
}

pub(crate) fn unit_dims() -> (f64, f64) {
    UNIT_DIMS.with(Cell::get)
}

pub(crate) fn features_begin() {
    FEATURES_USED.with(|used| used.set(false));
}

pub(crate) fn note_features() {
    FEATURES_USED.with(|used| used.set(true));
}

pub(crate) fn features_used() -> bool {
    FEATURES_USED.with(Cell::get)
}

pub(crate) fn stack_reset() {
    STACK.with(|stack| stack.borrow_mut().clear());
}

pub(crate) fn stack_push(container: Container) {
    STACK.with(|stack| stack.borrow_mut().push(container));
}

pub(crate) fn stack_pop() {
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

pub(crate) fn with_stack<R>(f: impl FnOnce(&[Container]) -> R) -> R {
    STACK.with(|stack| f(&stack.borrow()))
}

fn names_contain(names: Option<&CStr>, name: &[u8]) -> bool {
    let Some(names) = names else {
        return false;
    };
    names
        .to_bytes()
        .split(|&c| c == b' ' || c == b'\t')
        .any(|token| token == name)
}

fn select_container(name: Option<&[u8]>) -> Option<Container> {
    with_stack(|stack| {
        stack
            .iter()
            .rev()
            .find(|c| name.is_none_or(|name| names_contain(c.names(), name)))
            .copied()
    })
}

fn select_axis(block_axis: bool) -> Option<Container> {
    with_stack(|stack| {
        stack
            .iter()
            .rev()
            .find(|c| !block_axis || c.kind == TYPE_SIZE)
            .copied()
    })
}

pub(crate) fn unit_resolve(v: f64, unit: Unit) -> f64 {
    if !ffi::container_map().is_null() {
        note_features();
    }
    let (viewport_w, viewport_h) = ffi::viewport();
    let inline_size = match select_axis(false) {
        Some(c) if c.width > 0.0 => c.width,
        _ => viewport_w,
    };
    let block_size = match select_axis(true) {
        Some(c) if c.height > 0.0 => c.height,
        _ => viewport_h,
    };
    match unit {
        CQW => v * inline_size / 100.0,
        CQH => v * block_size / 100.0,
        CQMIN => {
            v * (if inline_size < block_size {
                inline_size
            } else {
                block_size
            }) / 100.0
        }
        CQMAX => {
            v * (if inline_size > block_size {
                inline_size
            } else {
                block_size
            }) / 100.0
        }
        _ => v,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tri {
    False,
    True,
    Unknown,
}

impl Tri {
    fn of(value: bool) -> Tri {
        if value { Tri::True } else { Tri::False }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Op {
    None,
    Lt,
    Le,
    Eq,
    Gt,
    Ge,
}

impl Op {
    fn parse(text: &[u8]) -> Op {
        match text {
            b"<" => Op::Lt,
            b"<=" => Op::Le,
            b"=" => Op::Eq,
            b">" => Op::Gt,
            b">=" => Op::Ge,
            _ => Op::None,
        }
    }

    fn text(self) -> &'static [u8] {
        match self {
            Op::Lt => b"<",
            Op::Le => b"<=",
            Op::Eq => b"=",
            Op::Gt => b">",
            Op::Ge => b">=",
            Op::None => b"",
        }
    }

    fn flipped(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
            _ => Op::Eq,
        }
    }
}

struct Feature {
    name: Vec<u8>,
    val1: Option<Vec<u8>>,
    val2: Option<Vec<u8>>,
    op1: Op,
    op2: Op,
    is_min: bool,
    is_max: bool,
}

impl Feature {
    fn named(name: Vec<u8>) -> Feature {
        Feature {
            name,
            val1: None,
            val2: None,
            op1: Op::None,
            op2: Op::None,
            is_min: false,
            is_max: false,
        }
    }
}

enum QueryNode {
    Feature(Feature),
    General(Vec<u8>),
    Not(Box<QueryNode>),
    Group(Box<QueryNode>),
    And(Vec<QueryNode>),
    Or(Vec<QueryNode>),
}

fn word_at(s: &[u8], p: usize, end: usize, word: &[u8]) -> bool {
    if end - p < word.len() || !starts_with_ci(&s[p..end], word) {
        return false;
    }
    let after = p + word.len();
    after == end || is_ws(s[after]) || s[after] == b'('
}

const FEATURE_NAMES: &[&[u8]] = &[
    b"width",
    b"height",
    b"inline-size",
    b"block-size",
    b"aspect-ratio",
    b"orientation",
];

fn feature_name_known(name: &[u8]) -> bool {
    FEATURE_NAMES.contains(&name)
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn ratio_value(v: &[u8]) -> Option<f64> {
    let whole_number = |part: &[u8]| -> Option<f64> {
        let text = c_text(strip(part));
        let (value, end) = ffi::strtod(&text, 0);
        (end == text.to_bytes().len()).then_some(value)
    };
    let (a, b) = match v.iter().position(|&c| c == b'/') {
        Some(slash) => {
            let a = whole_number(&v[..slash]);
            let b = whole_number(&v[slash + 1..]);
            match (a, b) {
                (Some(a), Some(b)) if a >= 0.0 && b >= 0.0 => (a, b),
                _ => return None,
            }
        }
        None => match whole_number(v) {
            Some(a) if a >= 0.0 => (a, 1.0),
            _ => return None,
        },
    };
    Some(if b > 0.0 {
        a / b
    } else if a > 0.0 {
        f64::MAX
    } else {
        0.0
    })
}

fn substitute_tree_counting(text: &[u8], index: i32, count: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut p = 0;
    while p < text.len() {
        let rest = &text[p..];
        let function = if starts_with_ci(rest, b"sibling-index(") {
            Some(index)
        } else if starts_with_ci(rest, b"sibling-count(") {
            Some(count)
        } else {
            None
        };
        if let Some(value) = function {
            let close = skip_ws(text, p + 14, text.len());
            if close < text.len() && text[close] == b')' {
                out.extend_from_slice(value.to_string().as_bytes());
                p = close + 1;
                continue;
            }
        }
        out.push(text[p]);
        p += 1;
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn is_math_function(text: &[u8]) -> bool {
    [&b"calc("[..], b"min(", b"max(", b"clamp("]
        .iter()
        .any(|name| starts_with_ci(text, name))
}

fn value_is_length(v: &[u8]) -> bool {
    if is_math_function(v) {
        let text = if contains(v, b"sibling-") {
            substitute_tree_counting(v, 1, 1)
        } else {
            v.to_vec()
        };
        return calc::parse_calc(&c_text(&text)).is_some();
    }
    match units::parse_length(&c_text(v)) {
        Some((n, unit)) => unit != NUMBER || n == 0.0,
        None => false,
    }
}

fn feature_value_valid(name: &[u8], value: &[u8], range: bool) -> bool {
    match name {
        b"orientation" => {
            !range
                && (value.eq_ignore_ascii_case(b"portrait")
                    || value.eq_ignore_ascii_case(b"landscape"))
        }
        b"aspect-ratio" => ratio_value(value).is_some(),
        _ => value_is_length(value),
    }
}

fn spacify(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 8);
    let mut p = 0;
    while p < s.len() {
        let c = s[p];
        if c == b'<' || c == b'>' || c == b'=' {
            out.push(b' ');
            while p < s.len() && matches!(s[p], b'<' | b'>' | b'=') {
                out.push(s[p]);
                p += 1;
            }
            out.push(b' ');
        } else if is_ws(c) {
            out.push(b' ');
            p += 1;
        } else {
            out.push(c);
            p += 1;
        }
    }
    out
}

fn parse_feature(text: &[u8]) -> Option<QueryNode> {
    let f = strip(text);
    if let Some(colon) = f.iter().position(|&c| c == b':') {
        let name = strip(&f[..colon]).to_ascii_lowercase();
        let value = strip(&f[colon + 1..]).to_vec();
        let is_min = name.starts_with(b"min-");
        let is_max = name.starts_with(b"max-");
        let base = if is_min || is_max {
            &name[4..]
        } else {
            &name[..]
        };
        if feature_name_known(base)
            && !value.is_empty()
            && !((is_min || is_max) && base == b"orientation")
            && feature_value_valid(base, &value, false)
        {
            let mut feature = Feature::named(base.to_vec());
            feature.is_min = is_min;
            feature.is_max = is_max;
            feature.val1 = Some(value);
            return Some(QueryNode::Feature(feature));
        }
        return None;
    }
    let normalized = spacify(f);
    let parts: Vec<&[u8]> = split_ws_paren(&normalized, 12)
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    match parts.len() {
        1 => {
            let name = parts[0].to_ascii_lowercase();
            feature_name_known(&name).then(|| QueryNode::Feature(Feature::named(name)))
        }
        3 | 5 => {
            let mut name_idx = None;
            for (i, part) in parts.iter().enumerate() {
                let lower = part.to_ascii_lowercase();
                if feature_name_known(&lower) && lower != b"orientation" {
                    name_idx = Some(i);
                }
            }
            let mut feature = Feature::named(Vec::new());
            match (parts.len(), name_idx) {
                (3, Some(idx @ (0 | 2))) => {
                    feature.name = parts[idx].to_ascii_lowercase();
                    let value = parts[if idx == 0 { 2 } else { 0 }];
                    let op = Op::parse(parts[1]);
                    if op == Op::None || !feature_value_valid(&feature.name, value, true) {
                        return None;
                    }
                    if idx == 0 {
                        feature.op2 = op;
                        feature.val2 = Some(value.to_vec());
                    } else {
                        feature.op1 = op;
                        feature.val1 = Some(value.to_vec());
                    }
                }
                (5, Some(2)) => {
                    feature.name = parts[2].to_ascii_lowercase();
                    let op1 = Op::parse(parts[1]);
                    let op2 = Op::parse(parts[3]);
                    let ascending =
                        matches!(op1, Op::Lt | Op::Le) && matches!(op2, Op::Lt | Op::Le);
                    let descending =
                        matches!(op1, Op::Gt | Op::Ge) && matches!(op2, Op::Gt | Op::Ge);
                    if !(ascending || descending)
                        || !feature_value_valid(&feature.name, parts[0], true)
                        || !feature_value_valid(&feature.name, parts[4], true)
                    {
                        return None;
                    }
                    feature.op1 = op1;
                    feature.val1 = Some(parts[0].to_vec());
                    feature.op2 = op2;
                    feature.val2 = Some(parts[4].to_vec());
                }
                _ => return None,
            }
            Some(QueryNode::Feature(feature))
        }
        _ => None,
    }
}

fn parse_in_parens(
    s: &[u8],
    pos: &mut usize,
    end: usize,
    ok: &mut bool,
    depth: i32,
) -> Option<QueryNode> {
    let mut p = skip_ws(s, *pos, end);
    if p < end && s[p] == b'(' {
        let Some(close) = match_paren_quoted(s, p, end) else {
            *ok = false;
            return None;
        };
        let inner = skip_ws(s, p + 1, end);
        let mut inner_end = close;
        while inner_end > inner && is_ws(s[inner_end - 1]) {
            inner_end -= 1;
        }
        *pos = close + 1;
        if inner >= inner_end {
            *ok = false;
            return None;
        }
        if s[inner] == b'(' || word_at(s, inner, inner_end, b"not") {
            let mut sub_ok = true;
            if let Some(query) = parse_query(s, inner, inner_end, &mut sub_ok, depth + 1)
                && sub_ok
            {
                return Some(QueryNode::Group(Box::new(query)));
            }
        }
        let text = &s[inner..inner_end];
        return Some(parse_feature(text).unwrap_or_else(|| {
            let mut general = Vec::with_capacity(text.len() + 2);
            general.push(b'(');
            general.extend_from_slice(text);
            general.push(b')');
            QueryNode::General(general)
        }));
    }
    let start = p;
    while p < end && (s[p].is_ascii_alphanumeric() || s[p] == b'-' || s[p] == b'_') {
        p += 1;
    }
    if p > start && p < end && s[p] == b'(' {
        let Some(close) = match_paren_quoted(s, p, end) else {
            *ok = false;
            return None;
        };
        *pos = close + 1;
        return Some(QueryNode::General(s[start..=close].to_vec()));
    }
    *ok = false;
    None
}

fn parse_query(s: &[u8], p: usize, end: usize, ok: &mut bool, depth: i32) -> Option<QueryNode> {
    if depth > MAX_NESTING {
        *ok = false;
        return None;
    }
    let mut p = skip_ws(s, p, end);
    if word_at(s, p, end, b"not") {
        p += 3;
        let Some(child) = parse_in_parens(s, &mut p, end, ok, depth) else {
            *ok = false;
            return None;
        };
        if skip_ws(s, p, end) < end {
            *ok = false;
            return None;
        }
        return Some(QueryNode::Not(Box::new(child)));
    }
    let Some(first) = parse_in_parens(s, &mut p, end, ok, depth) else {
        *ok = false;
        return None;
    };
    let mut list: Option<(bool, Vec<QueryNode>)> = None;
    let mut first = Some(first);
    loop {
        p = skip_ws(s, p, end);
        if p >= end {
            break;
        }
        let is_and = if word_at(s, p, end, b"and") {
            p += 3;
            true
        } else if word_at(s, p, end, b"or") {
            p += 2;
            false
        } else {
            *ok = false;
            break;
        };
        match &mut list {
            None => list = Some((is_and, first.take().into_iter().collect())),
            Some((kind, _)) if *kind != is_and => {
                *ok = false;
                break;
            }
            Some(_) => {}
        }
        let Some(next) = parse_in_parens(s, &mut p, end, ok, depth) else {
            *ok = false;
            break;
        };
        if let Some((_, children)) = &mut list {
            children.push(next);
        }
    }
    if !*ok {
        return None;
    }
    match list {
        Some((true, children)) => Some(QueryNode::And(children)),
        Some((false, children)) => Some(QueryNode::Or(children)),
        None => first,
    }
}

const RESERVED_NAMES: &[&[u8]] = &[
    b"none",
    b"and",
    b"or",
    b"not",
    b"inherit",
    b"initial",
    b"unset",
    b"revert",
    b"revert-layer",
    b"default",
];

fn container_name_valid(name: &[u8]) -> bool {
    if name.is_empty()
        || RESERVED_NAMES
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
        || name[0].is_ascii_digit()
        || (name[0] == b'-' && name.len() > 1 && name[1].is_ascii_digit())
    {
        return false;
    }
    let mut i = 0;
    while i < name.len() {
        let c = name[i];
        if c == b'\\' {
            i += 2;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80) {
            return false;
        }
        i += 1;
    }
    true
}

struct Condition {
    name: Option<Vec<u8>>,
    query: Option<QueryNode>,
}

fn parse_condition(cond: &[u8]) -> Option<Condition> {
    let end = cond.len();
    let mut p = skip_ws(cond, 0, end);
    let mut name = None;
    if p < end && cond[p] != b'(' && !word_at(cond, p, end, b"not") {
        let start = p;
        while p < end && !is_ws(cond[p]) && cond[p] != b'(' {
            if cond[p] == b'\\' && p + 1 < end {
                p += 1;
            }
            p += 1;
        }
        if !container_name_valid(&cond[start..p]) {
            return None;
        }
        name = Some(cond[start..p].to_vec());
        p = skip_ws(cond, p, end);
        if p >= end {
            return Some(Condition { name, query: None });
        }
    }
    if p >= end {
        return None;
    }
    let mut ok = true;
    let query = parse_query(cond, p, end, &mut ok, 0)?;
    ok.then_some(Condition {
        name,
        query: Some(query),
    })
}

fn split_commas(text: &[u8]) -> Vec<&[u8]> {
    let end = text.len();
    let mut parts = Vec::new();
    let mut seg = 0;
    let mut q = 0;
    loop {
        if q < end
            && text[q] == b'('
            && let Some(close) = match_paren_quoted(text, q, end)
        {
            q = close + 1;
            continue;
        }
        if q >= end || text[q] == b',' {
            parts.push(strip(&text[seg..q.min(end)]));
            if q >= end {
                break;
            }
            seg = q + 1;
        }
        q += 1;
    }
    parts
}

fn serialize(node: &QueryNode, out: &mut Vec<u8>) {
    match node {
        QueryNode::Feature(f) => {
            out.push(b'(');
            if f.op1 == Op::None && f.op2 == Op::None {
                if f.is_min {
                    out.extend_from_slice(b"min-");
                }
                if f.is_max {
                    out.extend_from_slice(b"max-");
                }
                out.extend_from_slice(&f.name);
                if let Some(v) = &f.val1 {
                    out.extend_from_slice(b": ");
                    out.extend_from_slice(v);
                }
            } else {
                if f.op1 != Op::None {
                    out.extend_from_slice(f.val1.as_deref().unwrap_or_default());
                    out.push(b' ');
                    out.extend_from_slice(f.op1.text());
                    out.push(b' ');
                }
                out.extend_from_slice(&f.name);
                if f.op2 != Op::None {
                    out.push(b' ');
                    out.extend_from_slice(f.op2.text());
                    out.push(b' ');
                    out.extend_from_slice(f.val2.as_deref().unwrap_or_default());
                }
            }
            out.push(b')');
        }
        QueryNode::General(text) => out.extend_from_slice(text),
        QueryNode::Not(child) => {
            out.extend_from_slice(b"not ");
            serialize(child, out);
        }
        QueryNode::Group(child) => {
            out.push(b'(');
            serialize(child, out);
            out.push(b')');
        }
        QueryNode::And(children) | QueryNode::Or(children) => {
            let joiner: &[u8] = if matches!(node, QueryNode::And(_)) {
                b" and "
            } else {
                b" or "
            };
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(joiner);
                }
                serialize(child, out);
            }
        }
    }
}

pub(crate) fn name_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(text, 16);
    if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case(b"none") {
        return Some(b"none".to_vec());
    }
    if tokens.is_empty() || !tokens.iter().all(|token| container_name_valid(token)) {
        return None;
    }
    Some(tokens.join(&b' '))
}

pub(crate) fn shorthand_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let slash = text.iter().position(|&c| c == b'/');
    let name_part = strip(&text[..slash.unwrap_or(text.len())]);
    let name = name_canonical(name_part)?;
    let Some(slash) = slash else {
        return Some(name);
    };
    let type_part = strip(&text[slash + 1..]);
    let kind = [&b"normal"[..], b"size", b"inline-size"]
        .into_iter()
        .find(|kind| type_part.eq_ignore_ascii_case(kind))?;
    if kind == b"normal" {
        return Some(name);
    }
    let mut out = name;
    out.extend_from_slice(b" / ");
    out.extend_from_slice(kind);
    Some(out)
}

pub(crate) fn condition_canonical(cond: &[u8]) -> Option<Vec<u8>> {
    let parts = split_commas(cond);
    if parts.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let condition = parse_condition(part)?;
        if i > 0 {
            out.extend_from_slice(b", ");
        }
        if let Some(name) = &condition.name {
            out.extend_from_slice(name);
        }
        if condition.name.is_some() && condition.query.is_some() {
            out.push(b' ');
        }
        if let Some(query) = &condition.query {
            serialize(query, &mut out);
        }
    }
    Some(out)
}

fn length_resolve(v: &[u8], pct_basis: f64, c: &Container) -> f64 {
    let text = if contains(v, b"sibling-") {
        substitute_tree_counting(v, c.sibling_index, c.sibling_count)
    } else {
        v.to_vec()
    };
    let (ok, resolved) = calc::resolve_to_px_pct(&text, false);
    if !ok {
        return 0.0;
    }
    resolved.px + resolved.pct / 100.0 * pct_basis
}

fn compare(actual: f64, op: Op, v: f64) -> Tri {
    match op {
        Op::Lt => Tri::of(actual < v),
        Op::Le => Tri::of(actual <= v),
        Op::Eq => Tri::of((actual - v).abs() < 0.001),
        Op::Gt => Tri::of(actual > v),
        Op::Ge => Tri::of(actual >= v),
        Op::None => Tri::Unknown,
    }
}

fn feature_value(value: Option<&[u8]>, ratio: bool, basis: f64, c: &Container) -> f64 {
    let value = value.unwrap_or_default();
    if ratio {
        ratio_value(value).unwrap_or(0.0)
    } else {
        length_resolve(value, basis, c)
    }
}

fn eval_feature(f: &Feature, c: &Container) -> Tri {
    let name = f.name.as_slice();
    let vertical = c.vertical != 0;
    let horiz = name == b"width"
        || name
            == if vertical {
                &b"block-size"[..]
            } else {
                b"inline-size"
            };
    let vert = name == b"height"
        || name
            == if vertical {
                &b"inline-size"[..]
            } else {
                b"block-size"
            };
    let block_axis =
        name == if vertical { &b"width"[..] } else { b"height" } || name == b"block-size";
    let needs_block = block_axis || name == b"aspect-ratio" || name == b"orientation";
    if needs_block && c.kind != TYPE_SIZE {
        return Tri::False;
    }
    let actual = if horiz {
        c.width
    } else if vert {
        c.height
    } else if name == b"aspect-ratio" {
        if c.height > 0.0 {
            c.width / c.height
        } else {
            0.0
        }
    } else {
        let portrait = c.height >= c.width;
        if f.op1 == Op::None && f.op2 == Op::None && f.val1.is_none() {
            return Tri::True;
        }
        let Some(value) = &f.val1 else {
            return Tri::Unknown;
        };
        return Tri::of(portrait == value.eq_ignore_ascii_case(b"portrait"));
    };
    let ratio = name == b"aspect-ratio";
    if f.op1 == Op::None && f.op2 == Op::None {
        if f.val1.is_none() {
            return Tri::of(actual > 0.0);
        }
        let v = feature_value(
            f.val1.as_deref(),
            ratio,
            if horiz { c.width } else { c.height },
            c,
        );
        if f.is_min {
            return Tri::of(actual >= v);
        }
        if f.is_max {
            return Tri::of(actual <= v);
        }
        return Tri::of((actual - v).abs() < 0.001);
    }
    let basis = if horiz { c.width } else { c.height };
    if f.op1 != Op::None {
        let v = feature_value(f.val1.as_deref(), ratio, basis, c);
        if compare(actual, f.op1.flipped(), v) != Tri::True {
            return Tri::False;
        }
    }
    if f.op2 != Op::None {
        let v = feature_value(f.val2.as_deref(), ratio, basis, c);
        if compare(actual, f.op2, v) != Tri::True {
            return Tri::False;
        }
    }
    Tri::True
}

fn eval(node: &QueryNode, c: &Container) -> Tri {
    match node {
        QueryNode::Feature(f) => eval_feature(f, c),
        QueryNode::General(_) => Tri::Unknown,
        QueryNode::Group(child) => eval(child, c),
        QueryNode::Not(child) => match eval(child, c) {
            Tri::Unknown => Tri::Unknown,
            Tri::True => Tri::False,
            Tri::False => Tri::True,
        },
        QueryNode::And(children) | QueryNode::Or(children) => {
            let (mut any_true, mut any_false) = (false, false);
            for child in children {
                match eval(child, c) {
                    Tri::Unknown => return Tri::Unknown,
                    Tri::True => any_true = true,
                    Tri::False => any_false = true,
                }
            }
            if matches!(node, QueryNode::And(_)) {
                Tri::of(!any_false)
            } else {
                Tri::of(any_true)
            }
        }
    }
}

pub(crate) struct Query {
    terms: Vec<Vec<Condition>>,
}

impl Query {
    pub(crate) fn compile(cond: &[u8]) -> Query {
        let mut terms = Vec::new();
        let mut p = 0;
        while p < cond.len() {
            let sep = cond[p..].iter().position(|&c| c == 0x1f).map(|i| p + i);
            let part = &cond[p..sep.unwrap_or(cond.len())];
            terms.push(
                split_commas(part)
                    .into_iter()
                    .filter_map(parse_condition)
                    .collect(),
            );
            match sep {
                Some(sep) => p = sep + 1,
                None => break,
            }
        }
        Query { terms }
    }

    pub(crate) fn matches(&self) -> bool {
        self.terms.iter().all(|term| {
            term.iter().any(|alt| {
                select_container(alt.name.as_deref()).is_some_and(|c| {
                    alt.query
                        .as_ref()
                        .is_none_or(|query| eval(query, &c) == Tri::True)
                })
            })
        })
    }
}
