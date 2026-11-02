//! Southstar — the if() function's branches parsed and the first whose supports(), media() or style() condition holds picked, with style() comparing custom properties by declaration, by name or by range.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use std::borrow::Cow;
use std::sync::OnceLock;

use southstar_css_prop_syntax::{self as prop_syntax, SyntaxDef};

use crate::attr_fn;
use crate::declarations::find_functions;
use crate::ffi;
use crate::scan::{is_ident, is_ws, match_paren_quoted, scan_until, skip_ws_comments, strip};
use crate::supports;
use crate::vars::{Lookup, Wide, wide_kind};

const DEPTH_MAX: i32 = 32;
const RANGE_SYNTAX: &[u8] = b"<length> | <number> | <percentage> | <angle> | <time> | <resolution>";

thread_local! {
    static TAINTED: Cell<bool> = const { Cell::new(false) };
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

    fn not(self) -> Tri {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            Tri::Unknown => Tri::Unknown,
        }
    }

    fn and(self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::False, _) | (_, Tri::False) => Tri::False,
            (Tri::True, Tri::True) => Tri::True,
            _ => Tri::Unknown,
        }
    }

    fn or(self, other: Tri) -> Tri {
        match (self, other) {
            (Tri::True, _) | (_, Tri::True) => Tri::True,
            (Tri::False, Tri::False) => Tri::False,
            _ => Tri::Unknown,
        }
    }
}

type Subst<'s> = dyn FnMut(&[u8]) -> Option<Vec<u8>> + 's;

fn top_level(text: &[u8], c: u8) -> bool {
    scan_until(text, 0, text.len(), &[c]).1 == c
}

fn skip_leading_ws(text: &[u8]) -> &[u8] {
    let start = text.iter().position(|&c| !is_ws(c)).unwrap_or(text.len());
    &text[start..]
}

fn branches(args: &[u8]) -> Option<Vec<(&[u8], &[u8])>> {
    let end = args.len();
    let mut out = Vec::new();
    let mut p = 0;
    loop {
        let (semi, term) = scan_until(args, p, end, b";");
        let branch = &args[p..semi];
        if strip(branch).is_empty() {
            if term == b';' {
                return None;
            }
            break;
        }
        let (colon, colon_term) = scan_until(branch, 0, branch.len(), b":");
        if colon_term != b':' {
            return None;
        }
        let condition = strip(&branch[..colon]);
        let value = skip_leading_ws(&branch[colon + 1..]);
        if condition.is_empty() || top_level(condition, b'!') || top_level(value, b'!') {
            return None;
        }
        out.push((condition, value));
        if term != b';' {
            break;
        }
        p = semi + 1;
    }
    (!out.is_empty()).then_some(out)
}

pub(crate) fn choose<'a>(
    args: &'a [u8],
    vars: Option<&dyn Lookup>,
    subst: &mut Subst<'_>,
) -> Option<(&'a [u8], bool)> {
    let branches = branches(args)?;
    let outer = TAINTED.replace(false);
    let chosen = branches
        .into_iter()
        .find(|(condition, _)| holds(condition, vars, subst))
        .map(|(_, value)| value);
    let tainted = TAINTED.get();
    TAINTED.set(outer || tainted);
    chosen.map(|value| (value, tainted))
}

fn outside_style(condition: &[u8], subst: &mut Subst<'_>) -> Option<Vec<u8>> {
    let end = condition.len();
    let mut out = Vec::with_capacity(end);
    let mut p = 0;
    while let Some((at, _)) = find_functions(condition, p, end, &[b"style"]) {
        let close = match_paren_quoted(condition, at + 5, end)?;
        out.extend_from_slice(&subst(&condition[p..at])?);
        out.extend_from_slice(&condition[at..=close]);
        p = close + 1;
    }
    out.extend_from_slice(&subst(&condition[p..])?);
    Some(out)
}

fn holds(condition: &[u8], vars: Option<&dyn Lookup>, subst: &mut Subst<'_>) -> bool {
    let Some(text) = outside_style(condition, subst) else {
        return false;
    };
    let text = strip(&text);
    if text.eq_ignore_ascii_case(b"else") {
        return true;
    }
    let mut p = 0;
    let result = boolean(text, &mut p, 0, &mut |s, p, depth| {
        condition_term(s, p, depth, vars, subst)
    });
    result == Some(Tri::True) && skip_ws_comments(text, p, text.len()) == text.len()
}

type Term<'t> = dyn FnMut(&[u8], &mut usize, i32) -> Option<Tri> + 't;

fn word(s: &[u8], p: usize, kw: &[u8]) -> bool {
    let n = kw.len();
    s.len() - p.min(s.len()) > n
        && s[p..p + n].eq_ignore_ascii_case(kw)
        && (is_ws(s[p + n]) || s[p + n] == b'(')
}

fn boolean(s: &[u8], pp: &mut usize, depth: i32, term: &mut Term<'_>) -> Option<Tri> {
    if depth > DEPTH_MAX {
        return None;
    }
    let end = s.len();
    let mut p = skip_ws_comments(s, *pp, end);
    if word(s, p, b"not") {
        p += 3;
        let result = term(s, &mut p, depth)?.not();
        *pp = p;
        return Some(result);
    }
    let mut acc = term(s, &mut p, depth)?;
    let mut joiner: Option<bool> = None;
    loop {
        let q = skip_ws_comments(s, p, end);
        let and = if word(s, q, b"and") {
            true
        } else if word(s, q, b"or") {
            false
        } else {
            break;
        };
        if joiner.is_some_and(|j| j != and) {
            return None;
        }
        joiner = Some(and);
        p = q + if and { 3 } else { 2 };
        let rhs = term(s, &mut p, depth)?;
        acc = if and { acc.and(rhs) } else { acc.or(rhs) };
    }
    *pp = p;
    Some(acc)
}

fn parens(s: &[u8], p: usize) -> Option<(usize, usize)> {
    let close = match_paren_quoted(s, p, s.len())?;
    Some((p + 1, close))
}

fn function_name(s: &[u8], p: usize) -> Option<(usize, usize)> {
    let start = p;
    let mut q = p;
    while q < s.len() && is_ident(s[q]) {
        q += 1;
    }
    (q > start && q < s.len() && s[q] == b'(').then_some((start, q))
}

fn nested(inner: &[u8], depth: i32, term: &mut Term<'_>) -> Option<Tri> {
    let mut q = 0;
    let result = boolean(inner, &mut q, depth + 1, term)?;
    (skip_ws_comments(inner, q, inner.len()) == inner.len()).then_some(result)
}

fn condition_term(
    s: &[u8],
    pp: &mut usize,
    depth: i32,
    vars: Option<&dyn Lookup>,
    subst: &mut Subst<'_>,
) -> Option<Tri> {
    let p = skip_ws_comments(s, *pp, s.len());
    if p >= s.len() {
        return None;
    }
    if s[p] == b'(' {
        let (open, close) = parens(s, p)?;
        *pp = close + 1;
        let inner = &s[open..close];
        return Some(
            nested(inner, depth, &mut |s, p, depth| {
                condition_term(s, p, depth, vars, subst)
            })
            .unwrap_or(Tri::Unknown),
        );
    }
    let (start, open) = function_name(s, p)?;
    let (args_start, close) = parens(s, open)?;
    *pp = close + 1;
    let name = &s[start..open];
    let args = &s[args_start..close];
    Some(if name.eq_ignore_ascii_case(b"supports") {
        Tri::of(supports::condition(args, true))
    } else if name.eq_ignore_ascii_case(b"media") {
        Tri::of(media(args))
    } else if name.eq_ignore_ascii_case(b"style") {
        style_query(args, depth, vars, subst)
    } else {
        Tri::Unknown
    })
}

fn media(args: &[u8]) -> bool {
    let query = strip(args);
    let bare = query.first() == Some(&b'(')
        || word(query, 0, b"not")
        || word(query, 0, b"only")
        || [&b"all"[..], b"screen", b"print"]
            .iter()
            .any(|t| query.eq_ignore_ascii_case(t));
    if bare {
        return ffi::media_query_matches(query);
    }
    ffi::media_query_matches(&[b"(", query, b")"].concat())
}

fn style_query(args: &[u8], depth: i32, vars: Option<&dyn Lookup>, subst: &mut Subst<'_>) -> Tri {
    let query = strip(args);
    if query.is_empty() {
        return Tri::Unknown;
    }
    nested(query, depth, &mut |s, p, depth| {
        style_term(s, p, depth, vars, subst)
    })
    .unwrap_or_else(|| feature(query, vars, subst))
}

fn style_term(
    s: &[u8],
    pp: &mut usize,
    depth: i32,
    vars: Option<&dyn Lookup>,
    subst: &mut Subst<'_>,
) -> Option<Tri> {
    let p = skip_ws_comments(s, *pp, s.len());
    if p >= s.len() {
        return None;
    }
    if s[p] == b'(' {
        let (open, close) = parens(s, p)?;
        *pp = close + 1;
        let inner = strip(&s[open..close]);
        if inner.first() == Some(&b'(') || word(inner, 0, b"not") {
            return Some(
                nested(inner, depth, &mut |s, p, depth| {
                    style_term(s, p, depth, vars, subst)
                })
                .unwrap_or(Tri::Unknown),
            );
        }
        return Some(feature(inner, vars, subst));
    }
    let (_, open) = function_name(s, p)?;
    let (_, close) = parens(s, open)?;
    *pp = close + 1;
    Some(Tri::Unknown)
}

fn custom_name(text: &[u8]) -> bool {
    text.len() > 2 && text.starts_with(b"--") && text.iter().all(|&c| is_ident(c))
}

fn feature(text: &[u8], vars: Option<&dyn Lookup>, subst: &mut Subst<'_>) -> Tri {
    let Some(text) = subst(text) else {
        return Tri::False;
    };
    let text = strip(&text);
    let (colon, term) = scan_until(text, 0, text.len(), b":");
    if term == b':' {
        let name = strip(&text[..colon]);
        let value = strip(&text[colon + 1..]);
        return match vars {
            Some(vars) if custom_name(name) => Tri::of(declaration_matches(vars, name, value)),
            _ => Tri::Unknown,
        };
    }
    if text.iter().any(|&c| matches!(c, b'<' | b'>' | b'=')) {
        return range(text, vars);
    }
    match vars {
        Some(vars) if custom_name(text) => Tri::of(differs_from_initial(vars, text)),
        _ => Tri::Unknown,
    }
}

fn with_attrs<'v>(vars: &dyn Lookup, text: Cow<'v, [u8]>) -> Option<Cow<'v, [u8]>> {
    if !text.windows(5).any(|w| w == b"attr(") {
        return Some(text);
    }
    TAINTED.set(true);
    attr_fn::substitute(&text, vars.node(), 0, &mut false).map(Cow::Owned)
}

fn computed_value<'v>(vars: &'v dyn Lookup, name: &[u8]) -> Option<Cow<'v, [u8]>> {
    let value = vars.value(name)?;
    if wide_kind(&value) != Wide::None {
        return None;
    }
    with_attrs(vars, value)
}

fn initial_value(vars: &dyn Lookup, name: &[u8]) -> Option<Vec<u8>> {
    vars.registration(name)?.initial.map(<[u8]>::to_vec)
}

fn inherited_value(vars: &dyn Lookup, name: &[u8]) -> Option<Vec<u8>> {
    match vars.inherited(name) {
        Some(value) if wide_kind(&value) == Wide::None => Some(value.into_owned()),
        Some(_) => None,
        None => initial_value(vars, name),
    }
}

fn normalized(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut space = false;
    for &c in strip(text) {
        if is_ws(c) {
            space = true;
            continue;
        }
        if space {
            out.push(b' ');
            space = false;
        }
        out.push(c);
    }
    out
}

fn same(vars: &dyn Lookup, name: &[u8], actual: Option<&[u8]>, expected: Option<&[u8]>) -> bool {
    let (actual, expected) = match (actual, expected) {
        (None, None) => return true,
        (Some(a), Some(e)) => (a, e),
        _ => return false,
    };
    let registration = vars.registration(name);
    match registration.as_ref().and_then(|r| r.syntax) {
        Some(syntax) => match (vars.compute(syntax, actual), vars.compute(syntax, expected)) {
            (Some(a), Some(e)) => a == e,
            _ => false,
        },
        None => normalized(actual) == normalized(expected),
    }
}

fn declaration_matches(vars: &dyn Lookup, name: &[u8], value: &[u8]) -> bool {
    let registration = vars.registration(name);
    let registered = registration.is_some();
    let inherits = registration.is_none_or(|r| r.inherits);
    let expected = match wide_kind(value) {
        Wide::Initial => initial_value(vars, name),
        Wide::Inherit => inherited_value(vars, name),
        Wide::Unset if inherits => inherited_value(vars, name),
        Wide::Unset => initial_value(vars, name),
        Wide::Revert | Wide::RevertLayer | Wide::RevertRule => return false,
        Wide::None => match with_attrs(vars, Cow::Borrowed(value)) {
            Some(value) => Some(value.into_owned()),
            None => return false,
        },
    };
    let actual = match computed_value(vars, name) {
        Some(value) => Some(value.into_owned()),
        None if registered => initial_value(vars, name),
        None => None,
    };
    same(vars, name, actual.as_deref(), expected.as_deref())
}

fn differs_from_initial(vars: &dyn Lookup, name: &[u8]) -> bool {
    let actual = computed_value(vars, name).map(Cow::into_owned);
    if vars.registration(name).is_none() {
        return actual.is_some();
    }
    let initial = initial_value(vars, name);
    actual.is_some() && !same(vars, name, actual.as_deref(), initial.as_deref())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Percentage,
    Length,
    Angle,
    Time,
    Resolution,
}

fn range_syntax() -> Option<&'static SyntaxDef> {
    static SYNTAX: OnceLock<Option<SyntaxDef>> = OnceLock::new();
    SYNTAX
        .get_or_init(|| prop_syntax::parse(RANGE_SYNTAX))
        .as_ref()
}

fn typed(text: &[u8]) -> Option<(Kind, f64)> {
    let unit_at = text
        .iter()
        .rposition(|&c| !(c.is_ascii_alphabetic() || c == b'%'))
        .map_or(0, |i| i + 1);
    let number: f64 = std::str::from_utf8(&text[..unit_at]).ok()?.parse().ok()?;
    let unit = &text[unit_at..];
    let kind = match unit {
        b"" => Kind::Number,
        b"%" => Kind::Percentage,
        b"px" => Kind::Length,
        b"deg" => Kind::Angle,
        b"s" => Kind::Time,
        b"dppx" => Kind::Resolution,
        _ => return None,
    };
    Some((kind, number))
}

fn operand(text: &[u8], vars: Option<&dyn Lookup>) -> Option<(Kind, f64)> {
    let text = strip(text);
    let value = match vars {
        Some(vars) if custom_name(text) => computed_value(vars, text)?,
        Some(vars) => with_attrs(vars, Cow::Borrowed(text))?,
        None => Cow::Borrowed(text),
    };
    let syntax = range_syntax()?;
    let computed = match vars {
        Some(vars) => vars.compute(syntax, &value),
        None => prop_syntax::compute(syntax, &value, None),
    }?;
    typed(&computed)
}

fn comparison(s: &[u8], p: usize) -> Option<(&'static [u8], usize)> {
    let two = s.get(p..p + 2);
    for op in [&b"<="[..], b">="] {
        if two == Some(op) {
            return Some((op, 2));
        }
    }
    match s[p] {
        b'<' => Some((b"<", 1)),
        b'>' => Some((b">", 1)),
        b'=' => Some((b"=", 1)),
        _ => None,
    }
}

type Range<'t> = (Vec<&'t [u8]>, Vec<&'static [u8]>);

fn split_range(text: &[u8]) -> Option<Range<'_>> {
    let mut operands = Vec::new();
    let mut ops = Vec::new();
    let mut start = 0;
    let mut p = 0;
    while p < text.len() {
        let (at, term) = scan_until(text, p, text.len(), b"<>=");
        if term == 0 {
            break;
        }
        let (op, len) = comparison(text, at)?;
        operands.push(&text[start..at]);
        ops.push(op);
        p = at + len;
        start = p;
    }
    operands.push(&text[start..]);
    (matches!(ops.len(), 1 | 2)).then_some((operands, ops))
}

fn compare(a: (Kind, f64), op: &[u8], b: (Kind, f64)) -> Tri {
    if a.0 != b.0 {
        return Tri::Unknown;
    }
    let (x, y) = (a.1, b.1);
    let eq = (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0);
    Tri::of(match op {
        b"<" => x < y && !eq,
        b"<=" => x < y || eq,
        b">" => x > y && !eq,
        b">=" => x > y || eq,
        _ => eq,
    })
}

fn range(text: &[u8], vars: Option<&dyn Lookup>) -> Tri {
    let Some((operands, ops)) = split_range(text) else {
        return Tri::Unknown;
    };
    if ops.len() == 2 && (ops[0][0] == b'=' || ops[0][0] != ops[1][0]) {
        return Tri::Unknown;
    }
    let mut values = Vec::with_capacity(operands.len());
    for text in operands {
        match operand(text, vars) {
            Some(value) => values.push(value),
            None => return Tri::Unknown,
        }
    }
    ops.iter().enumerate().fold(Tri::True, |acc, (i, op)| {
        acc.and(compare(values[i], op, values[i + 1]))
    })
}
