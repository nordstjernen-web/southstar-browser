//! Southstar — CSS counters: counter-reset and counter-increment applied in tree order, the values each generating element sees, and counter() formatting.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{c_int, c_long};
use std::collections::HashMap;

use southstar_dom::{MAX_DEPTH, Node, children};
use southstar_style::{PropId, StyleRef, StyleTable, ValueRef};

type Counters = HashMap<Vec<u8>, c_int>;

thread_local! {
    static SNAPSHOTS: RefCell<Option<HashMap<usize, Counters>>> = const { RefCell::new(None) };
}

pub fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn strtol(s: &[u8], at: usize) -> Option<(c_int, usize)> {
    let mut p = at;
    let negative = s.get(p) == Some(&b'-');
    if negative {
        p += 1;
    }
    let digits_start = p;
    let mut v: i128 = 0;
    while let Some(&d) = s.get(p).filter(|c| c.is_ascii_digit()) {
        v = (v * 10 + i128::from(d - b'0')).min(i128::from(c_long::MAX) + 1);
        p += 1;
    }
    if p == digits_start {
        return None;
    }
    let v = if negative { -v } else { v };
    let v = v.clamp(i128::from(c_long::MIN), i128::from(c_long::MAX)) as i64;
    Some((v as c_int, p))
}

fn apply_decl(counters: &mut Counters, decl: &[u8], increment: bool) {
    let at = |i: usize| decl.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while at(p) != 0 {
        while at(p) != 0 && is_space(at(p)) {
            p += 1;
        }
        if at(p) == 0 {
            break;
        }
        let name_start = p;
        while at(p) != 0 && !is_space(at(p)) {
            p += 1;
        }
        let name = &decl[name_start..p];
        if name == b"none" {
            break;
        }
        while at(p) != 0 && is_space(at(p)) {
            p += 1;
        }
        let mut val = c_int::from(increment);
        if (at(p) == b'-' || at(p).is_ascii_digit())
            && let Some((v, end)) = strtol(decl, p)
        {
            val = v;
            p = end;
        }
        let cur = if increment {
            counters.get(name).copied().unwrap_or(0)
        } else {
            0
        };
        counters.insert(name.to_vec(), cur.wrapping_add(val));
    }
}

fn apply_style(current: &mut Counters, s: Option<StyleRef<'_>>) {
    let Some(s) = s else {
        return;
    };
    if let Some(reset) = s.get(PropId::CounterReset).and_then(ValueRef::keyword_text) {
        apply_decl(current, reset.to_bytes(), false);
    }
    if let Some(inc) = s
        .get(PropId::CounterIncrement)
        .and_then(ValueRef::keyword_text)
    {
        apply_decl(current, inc.to_bytes(), true);
    }
}

fn content_uses_counter(ps: Option<StyleRef<'_>>) -> bool {
    ps.and_then(|ps| ps.get(PropId::Content))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes().windows(7).any(|w| w == b"counter"))
}

fn walk(
    n: Node<'_>,
    styles: StyleTable,
    current: &mut Counters,
    snapshots: &mut HashMap<usize, Counters>,
    depth: i32,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    let mut s = None;
    if n.is_element() && !styles.as_ptr().is_null() {
        s = styles.get(n);
        apply_style(current, s);
        if let Some(st) = s {
            apply_style(current, st.before());
        }
        let need = s.is_some_and(|st| {
            content_uses_counter(st.before()) || content_uses_counter(st.after())
        });
        if need {
            snapshots.insert(n.as_ptr() as usize, current.clone());
        }
    }
    for c in children(n) {
        walk(c, styles, current, snapshots, depth + 1);
    }
    if let Some(st) = s {
        apply_style(current, st.after());
    }
}

pub fn build(root: Option<Node<'_>>, styles: StyleTable) {
    let mut snapshots = HashMap::new();
    if let Some(root) = root {
        walk(root, styles, &mut HashMap::new(), &mut snapshots, 0);
    }
    SNAPSHOTS.set(Some(snapshots));
}

pub fn clear() {
    SNAPSHOTS.set(None);
}

fn value_for(host: Option<Node<'_>>, name: &[u8]) -> c_int {
    let Some(host) = host else {
        return 0;
    };
    SNAPSHOTS.with_borrow(|snaps| {
        snaps
            .as_ref()
            .and_then(|snaps| snaps.get(&(host.as_ptr() as usize)))
            .and_then(|snap| snap.get(name))
            .copied()
            .unwrap_or(0)
    })
}

fn roman(mut v: c_int, upper: bool) -> Vec<u8> {
    const NUMERALS: [(c_int, &[u8]); 13] = [
        (1000, b"m"),
        (900, b"cm"),
        (500, b"d"),
        (400, b"cd"),
        (100, b"c"),
        (90, b"xc"),
        (50, b"l"),
        (40, b"xl"),
        (10, b"x"),
        (9, b"ix"),
        (5, b"v"),
        (4, b"iv"),
        (1, b"i"),
    ];
    let mut out = Vec::new();
    for (n, s) in NUMERALS {
        while v >= n {
            out.extend_from_slice(s);
            v -= n;
        }
    }
    if upper {
        out.make_ascii_uppercase();
    }
    out
}

fn alphabetic(mut v: c_int, base: u8) -> Vec<u8> {
    let mut digits = Vec::new();
    while v > 0 && digits.len() < 16 {
        v -= 1;
        digits.push(base + (v % 26) as u8);
        v /= 26;
    }
    digits.reverse();
    digits
}

fn greek(v: c_int) -> Vec<u8> {
    const LETTERS: [char; 24] = [
        'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ', 'ν', 'ξ', 'ο', 'π', 'ρ', 'σ',
        'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
    ];
    let mut letters = Vec::new();
    let mut w = v;
    while w > 0 && letters.len() < 16 {
        w -= 1;
        letters.push(LETTERS[(w % 24) as usize]);
        w /= 24;
    }
    letters.iter().rev().collect::<String>().into_bytes()
}

pub fn format(v: c_int, style: Option<&[u8]>) -> Vec<u8> {
    let decimal = || v.to_string().into_bytes();
    match style {
        None | Some(b"decimal") => decimal(),
        Some(b"decimal-leading-zero") => format!("{v:02}").into_bytes(),
        Some(s @ (b"lower-roman" | b"upper-roman")) => {
            if !(1..=3999).contains(&v) {
                decimal()
            } else {
                roman(v, s[0] == b'u')
            }
        }
        Some(s @ (b"lower-alpha" | b"lower-latin" | b"upper-alpha" | b"upper-latin")) => {
            if v < 1 {
                decimal()
            } else {
                alphabetic(v, if s[0] == b'u' { b'A' } else { b'a' })
            }
        }
        Some(b"lower-greek") => {
            if v < 1 {
                decimal()
            } else {
                greek(v)
            }
        }
        _ => decimal(),
    }
}

pub fn substitute(body: &[u8], host: Option<Node<'_>>) -> Vec<u8> {
    let at = |i: usize| body.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while at(p) != 0 && is_space(at(p)) {
        p += 1;
    }
    let name_start = p;
    while at(p) != 0 && at(p) != b',' && at(p) != b')' && !is_space(at(p)) {
        p += 1;
    }
    if p == name_start {
        return Vec::new();
    }
    let name = &body[name_start..p];
    let mut style = None;
    while at(p) == b',' || is_space(at(p)) {
        if at(p) == b',' {
            p += 1;
        }
        while at(p) != 0 && is_space(at(p)) {
            p += 1;
        }
        let c = at(p);
        if c == b'"' || c == b'\'' {
            p += 1;
            while at(p) != 0 && at(p) != c {
                p += 1;
            }
            if at(p) != 0 {
                p += 1;
            }
        } else if c.is_ascii_alphabetic() {
            let vs = p;
            while at(p) != 0 && at(p) != b',' && at(p) != b')' && !is_space(at(p)) {
                p += 1;
            }
            if style.is_none() {
                style = Some(&body[vs..p]);
            }
        } else {
            break;
        }
    }
    format(value_for(host, name), style)
}
