//! Southstar — calc-size(): a sizing basis and a calculation linear in `size`, parsed into a calc value with its canonical text, interpolated, and serialized.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::calc::{Calc, Parsed};
use crate::math::math_canonical;
use crate::property::{Body, Value, c_text, calc_value, eq, length, lower};
use crate::scan::{match_close_paren, starts_with_ci, strip};
use crate::text::split_top_level_commas;
use crate::units::{EM, PERCENT, PX, REM, Unit, VH, VMAX, VMIN, VW};

pub const FN_CALC_SIZE: u8 = 32;

pub const BASIS_ANY: u8 = 0;
pub const BASIS_AUTO: u8 = 1;
pub const BASIS_MIN_CONTENT: u8 = 2;
pub const BASIS_MAX_CONTENT: u8 = 3;
pub const BASIS_FIT_CONTENT: u8 = 4;
pub const BASIS_STRETCH: u8 = 5;
pub const BASIS_PERCENT: u8 = 6;

const BASES: [(&[u8], u8); 6] = [
    (b"any", BASIS_ANY),
    (b"auto", BASIS_AUTO),
    (b"min-content", BASIS_MIN_CONTENT),
    (b"max-content", BASIS_MAX_CONTENT),
    (b"fit-content", BASIS_FIT_CONTENT),
    (b"stretch", BASIS_STRETCH),
];

const PROBE: f64 = 1000.0;

#[derive(Clone, Copy)]
pub(crate) struct CalcSize {
    pub basis: u8,
    pub size: f64,
    pub linear: Calc,
}

impl CalcSize {
    fn to_calc(self) -> Calc {
        let mut calc = self.linear;
        calc.func = FN_CALC_SIZE;
        calc.n_args = 0;
        calc.args[0].px = self.size;
        calc.args[0].pct = f64::from(self.basis);
        calc
    }
}

pub(crate) fn of_calc(c: &Calc) -> Option<CalcSize> {
    (c.func == FN_CALC_SIZE).then(|| {
        let mut linear = *c;
        linear.func = 0;
        linear.args = Default::default();
        CalcSize {
            basis: c.args[0].pct as u8,
            size: c.args[0].px,
            linear,
        }
    })
}

fn scaled_sum(a: &Calc, k: f64, b: &Calc) -> Calc {
    Calc {
        pct: a.pct * k + b.pct,
        px: a.px * k + b.px,
        em: a.em * k + b.em,
        rem: a.rem * k + b.rem,
        vw: a.vw * k + b.vw,
        vh: a.vh * k + b.vh,
        vmin: a.vmin * k + b.vmin,
        vmax: a.vmax * k + b.vmax,
        parsed_vw: if b.parsed_vw != 0.0 {
            b.parsed_vw
        } else {
            a.parsed_vw
        },
        parsed_vh: if b.parsed_vh != 0.0 {
            b.parsed_vh
        } else {
            a.parsed_vh
        },
        ..Calc::default()
    }
}

fn unit_calc(v: f64, unit: Unit) -> Option<Calc> {
    let mut c = Calc::default();
    match unit {
        PX => c.px = v,
        PERCENT => c.pct = v,
        EM => c.em = v,
        REM => c.rem = v,
        VW | VH | VMIN | VMAX => {
            let (w, h) = crate::ffi::viewport();
            let coeff = match unit {
                VW => &mut c.vw,
                VH => &mut c.vh,
                VMIN => &mut c.vmin,
                _ => &mut c.vmax,
            };
            *coeff = v;
            let basis = match unit {
                VW => w,
                VH => h,
                VMIN => w.min(h),
                _ => w.max(h),
            };
            c.px = v * basis / 100.0;
            c.parsed_vw = w;
            c.parsed_vh = h;
        }
        _ => return None,
    }
    Some(c)
}

fn linear_of(parsed: Parsed) -> Option<Calc> {
    match parsed {
        Parsed::Length(v, unit) => unit_calc(v, unit),
        Parsed::Calc(c) if c.func == 0 => Some(c),
        Parsed::Calc(_) => None,
    }
}

fn length_percentage(t: &[u8]) -> Option<Calc> {
    if t.contains(&b'(') {
        return linear_of(calc_value(t)?);
    }
    let (v, unit) = length(t)?;
    linear_of(Parsed::Length(v, unit))
}

fn is_ident_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

fn mentions_size(expr: &[u8]) -> bool {
    substitute(expr, b"") != expr
}

fn substitute(expr: &[u8], with: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(expr.len() + 16);
    let mut i = 0;
    while i < expr.len() {
        let word = expr.len() - i >= 4 && expr[i..i + 4].eq_ignore_ascii_case(b"size");
        let before = i == 0 || !is_ident_byte(expr[i - 1]);
        let after = i + 4 >= expr.len() || !is_ident_byte(expr[i + 4]);
        if word && before && after {
            out.extend_from_slice(with);
            i += 4;
        } else {
            out.push(expr[i]);
            i += 1;
        }
    }
    out
}

fn evaluate(expr: &[u8], size: f64) -> Option<Calc> {
    let value = format!("{}px", southstar_numfmt::g(size));
    let text = [
        b"calc(".as_slice(),
        &substitute(expr, value.as_bytes()),
        b")",
    ]
    .concat();
    linear_of(calc_value(&text)?)
}

fn same_terms(a: &Calc, b: &Calc) -> bool {
    a.pct == b.pct
        && a.em == b.em
        && a.rem == b.rem
        && a.vw == b.vw
        && a.vh == b.vh
        && a.vmin == b.vmin
        && a.vmax == b.vmax
}

fn arguments(t: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let t = strip(t);
    if !starts_with_ci(t, b"calc-size(") {
        return None;
    }
    let open = 10;
    let close = match_close_paren(t, open, t.len())?;
    if close + 1 != t.len() {
        return None;
    }
    let args = split_top_level_commas(&t[open..close]);
    match &args[..] {
        [basis, expr] if !basis.is_empty() && !expr.is_empty() => {
            Some((basis.clone(), expr.clone()))
        }
        _ => None,
    }
}

struct Basis {
    kind: u8,
    nested: Option<CalcSize>,
    keyword: bool,
    text: Vec<u8>,
}

fn basis(t: &[u8]) -> Option<Basis> {
    if let Some(&(name, kind)) = BASES.iter().find(|(name, _)| eq(t, name)) {
        return Some(Basis {
            kind,
            nested: None,
            keyword: true,
            text: name.to_vec(),
        });
    }
    if starts_with_ci(t, b"calc-size(") {
        let (inner, text) = parse_inner(t)?;
        return Some(Basis {
            kind: inner.basis,
            nested: Some(inner),
            keyword: false,
            text,
        });
    }
    let linear = length_percentage(t)?;
    let text = if t.contains(&b'(') {
        math_canonical(&c_text(t)).unwrap_or_else(|| lower(t))
    } else {
        lower(t)
    };
    let kind = if linear.pct != 0.0 {
        BASIS_PERCENT
    } else {
        BASIS_ANY
    };
    Some(Basis {
        kind,
        nested: Some(CalcSize {
            basis: kind,
            size: 0.0,
            linear,
        }),
        keyword: false,
        text,
    })
}

fn is_zero(c: &Calc) -> bool {
    c.px == 0.0 && same_terms(c, &Calc::default())
}

fn expression_text(expr: &[u8], size: f64, constant: &Calc) -> Vec<u8> {
    let zero = substitute(expr, b"0px");
    let canonical = math_canonical(&c_text(&[b"calc(".as_slice(), &zero, b")"].concat()));
    let constant_text = match canonical {
        Some(text) if !is_zero(constant) => {
            let inner = text
                .strip_prefix(b"calc(")
                .and_then(|t| t.strip_suffix(b")"))
                .map(<[u8]>::to_vec);
            inner.unwrap_or(text)
        }
        _ if !is_zero(constant) => lower(&zero),
        _ => Vec::new(),
    };
    let size_text = if size == 0.0 {
        Vec::new()
    } else if size == 1.0 {
        b"size".to_vec()
    } else {
        format!("{} * size", southstar_numfmt::g(size)).into_bytes()
    };
    match (constant_text.is_empty(), size_text.is_empty()) {
        (true, true) => b"0px".to_vec(),
        (true, false) => size_text,
        (false, true) => constant_text,
        (false, false) if size == 1.0 => [constant_text, b" + ".to_vec(), size_text].concat(),
        (false, false) => [constant_text, b" + (".to_vec(), size_text, b")".to_vec()].concat(),
    }
}

fn parse_inner(t: &[u8]) -> Option<(CalcSize, Vec<u8>)> {
    let (basis_text, expr) = arguments(t)?;
    let basis = basis(strip(&basis_text))?;
    let expr = strip(&expr);
    if starts_with_ci(expr, b"calc-size(") || crate::property::contains(&lower(expr), b"calc-size(")
    {
        return None;
    }
    let uses_size = mentions_size(expr);
    if uses_size && basis.keyword && basis.kind == BASIS_ANY {
        return None;
    }
    let at_zero = evaluate(expr, 0.0)?;
    let at_probe = evaluate(expr, PROBE)?;
    if !same_terms(&at_zero, &at_probe) {
        return None;
    }
    let size = (at_probe.px - at_zero.px) / PROBE;
    let text = [
        b"calc-size(".as_slice(),
        &basis.text,
        b", ",
        &expression_text(expr, size, &at_zero),
        b")",
    ]
    .concat();
    let value = match basis.nested {
        Some(inner) if inner.basis == BASIS_PERCENT && inner.size == 0.0 => CalcSize {
            basis: BASIS_PERCENT,
            size,
            linear: scaled_sum(&inner.linear, size, &at_zero),
        },
        Some(inner) => CalcSize {
            basis: inner.basis,
            size: size * inner.size,
            linear: scaled_sum(&inner.linear, size, &at_zero),
        },
        None => CalcSize {
            basis: basis.kind,
            size,
            linear: at_zero,
        },
    };
    Some((value, text))
}

pub(crate) fn parse(t: &[u8], allow_auto: bool) -> Option<Value> {
    let (value, text) = parse_inner(t)?;
    if !allow_auto && value.basis == BASIS_AUTO {
        return None;
    }
    let mut v = Value::of(Body::Calc(value.to_calc()));
    v.specified = Some(text);
    Some(v)
}

pub(crate) fn of_body(body: &Body) -> Option<CalcSize> {
    match body {
        Body::Calc(c) if c.func == FN_CALC_SIZE => of_calc(c),
        Body::Calc(c) if c.func == 0 => Some(CalcSize {
            basis: BASIS_ANY,
            size: 0.0,
            linear: *c,
        }),
        Body::Length(v, unit) => unit_calc(*v, *unit).map(|linear| CalcSize {
            basis: BASIS_ANY,
            size: 0.0,
            linear,
        }),
        _ => None,
    }
}

pub(crate) fn is_calc_size(body: &Body) -> bool {
    matches!(body, Body::Calc(c) if c.func == FN_CALC_SIZE)
}

pub(crate) fn operand(body: &Body) -> Option<CalcSize> {
    match body {
        Body::Keyword(keyword) => keyword_basis(keyword),
        _ => of_body(body),
    }
}

pub(crate) fn keyword_basis(keyword: &[u8]) -> Option<CalcSize> {
    let &(_, basis) = BASES[1..].iter().find(|(name, _)| eq(keyword, name))?;
    Some(CalcSize {
        basis,
        size: 1.0,
        linear: Calc::default(),
    })
}

pub(crate) fn interpolate(a: CalcSize, b: CalcSize, t: f64) -> Option<Body> {
    let basis = match (a.basis, b.basis) {
        (x, y) if x == y => x,
        (BASIS_ANY, y) => y,
        (x, BASIS_ANY) => x,
        _ => return None,
    };
    let size_of = |c: &CalcSize| {
        if c.basis == BASIS_ANY && basis == BASIS_PERCENT {
            0.0
        } else {
            c.size
        }
    };
    let lerp = |x: f64, y: f64| x + (y - x) * t;
    let (la, lb) = (&a.linear, &b.linear);
    let linear = Calc {
        pct: lerp(la.pct, lb.pct),
        px: lerp(la.px, lb.px),
        em: lerp(la.em, lb.em),
        rem: lerp(la.rem, lb.rem),
        vw: lerp(la.vw, lb.vw),
        vh: lerp(la.vh, lb.vh),
        vmin: lerp(la.vmin, lb.vmin),
        vmax: lerp(la.vmax, lb.vmax),
        parsed_vw: la.parsed_vw.max(lb.parsed_vw),
        parsed_vh: la.parsed_vh.max(lb.parsed_vh),
        ..Calc::default()
    };
    Some(Body::Calc(
        CalcSize {
            basis,
            size: lerp(size_of(&a), size_of(&b)),
            linear,
        }
        .to_calc(),
    ))
}

pub(crate) fn computed_text(c: &Calc) -> Option<Vec<u8>> {
    let cs = of_calc(c)?;
    let basis = BASES
        .iter()
        .find(|(_, kind)| *kind == cs.basis)
        .map_or(&b"any"[..], |(name, _)| name);
    let mut terms: Vec<String> = Vec::new();
    for (v, unit) in [(cs.linear.pct, "%"), (cs.linear.px, "px")] {
        if v != 0.0 {
            terms.push(format!("{}{unit}", southstar_numfmt::g(v)));
        }
    }
    if cs.size != 0.0 && cs.basis != BASIS_PERCENT {
        terms.push(if cs.size == 1.0 {
            "size".to_owned()
        } else if terms.is_empty() {
            format!("{} * size", southstar_numfmt::g(cs.size))
        } else {
            format!("({} * size)", southstar_numfmt::g(cs.size))
        });
    }
    let expr = if terms.is_empty() {
        "0px".to_owned()
    } else {
        terms.join(" + ")
    };
    Some(format!("calc-size({}, {expr})", String::from_utf8_lossy(basis)).into_bytes())
}
