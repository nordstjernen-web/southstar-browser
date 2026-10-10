//! Southstar — Media Queries Level 4 parser, evaluator and serializer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::sync::Mutex;

const MAX_DEPTH: i32 = 32;
const VIEWPORT_STACK: usize = 16;

struct State {
    stack: Vec<(f64, f64)>,
    device_w: f64,
    device_h: f64,
    dppx: f64,
    print: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    stack: Vec::new(),
    device_w: 1920.0,
    device_h: 1080.0,
    dppx: 1.0,
    print: false,
});

fn state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    f(&mut STATE.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn set_device_size(w: f64, h: f64) {
    state(|s| {
        if w > 0.0 {
            s.device_w = w;
        }
        if h > 0.0 {
            s.device_h = h;
        }
    });
}

pub fn set_device_pixel_ratio(dppx: f64) {
    let changed = state(|s| {
        if !(dppx > 0.0 && dppx.is_finite()) || dppx == s.dppx {
            return false;
        }
        s.dppx = dppx;
        true
    });
    if changed {
        ffi::stylesheet_cache_drop();
    }
}

pub fn device_pixel_ratio() -> f64 {
    state(|s| s.dppx)
}

pub fn viewport_push(w: f64, h: f64) {
    state(|s| {
        if s.stack.len() < VIEWPORT_STACK {
            s.stack.push((
                if w >= 0.0 { w } else { 0.0 },
                if h >= 0.0 { h } else { 0.0 },
            ));
        }
    });
}

pub fn viewport_pop() {
    state(|s| {
        s.stack.pop();
    });
}

pub fn viewport_w() -> f64 {
    state(|s| s.stack.last().map(|v| v.0)).unwrap_or_else(ffi::css_viewport_w)
}

pub fn viewport_h() -> f64 {
    state(|s| s.stack.last().map(|v| v.1)).unwrap_or_else(ffi::css_viewport_h)
}

pub fn set_print_media(printing: bool) {
    let changed = state(|s| {
        if s.print == printing {
            return false;
        }
        s.print = printing;
        true
    });
    if changed {
        ffi::stylesheet_cache_drop();
    }
}

pub fn print_media() -> bool {
    state(|s| s.print)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tri {
    No,
    Yes,
    Dunno,
}

fn tri_not(v: Tri) -> Tri {
    match v {
        Tri::Yes => Tri::No,
        Tri::No => Tri::Yes,
        Tri::Dunno => Tri::Dunno,
    }
}

fn tri_and(a: Tri, b: Tri) -> Tri {
    if a == Tri::No || b == Tri::No {
        Tri::No
    } else if a == Tri::Yes && b == Tri::Yes {
        Tri::Yes
    } else {
        Tri::Dunno
    }
}

fn tri_or(a: Tri, b: Tri) -> Tri {
    if a == Tri::Yes || b == Tri::Yes {
        Tri::Yes
    } else if a == Tri::No && b == Tri::No {
        Tri::No
    } else {
        Tri::Dunno
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeatureType {
    Length,
    Ratio,
    Resolution,
    Integer,
    Number,
    Discrete,
}

struct FeatureDef {
    name: &'static str,
    kind: FeatureType,
    keywords: &'static [&'static str],
}

const ORIENTATION: &[&str] = &["portrait", "landscape"];
const SCAN: &[&str] = &["interlace", "progressive"];
const OVERFLOW_BLOCK: &[&str] = &["none", "scroll", "paged"];
const OVERFLOW_INLINE: &[&str] = &["none", "scroll"];
const UPDATE: &[&str] = &["none", "slow", "fast"];
const HOVER: &[&str] = &["none", "hover"];
const POINTER: &[&str] = &["none", "coarse", "fine"];
const SCHEME: &[&str] = &["light", "dark"];
const MOTION: &[&str] = &["no-preference", "reduce"];
const CONTRAST: &[&str] = &["no-preference", "less", "more", "custom"];
const DATA: &[&str] = &["no-preference", "reduce"];
const FORCED: &[&str] = &["none", "active"];
const INVERTED: &[&str] = &["none", "inverted"];
const GAMUT: &[&str] = &["srgb", "p3", "rec2020"];
const SCRIPTING: &[&str] = &["none", "initial-only", "enabled"];
const DISPLAY_MODE: &[&str] = &[
    "fullscreen",
    "standalone",
    "minimal-ui",
    "browser",
    "picture-in-picture",
    "window-controls-overlay",
];
const DYNAMIC_RANGE: &[&str] = &["standard", "high"];

const fn feature(name: &'static str, kind: FeatureType) -> FeatureDef {
    FeatureDef {
        name,
        kind,
        keywords: &[],
    }
}

const fn discrete(name: &'static str, keywords: &'static [&'static str]) -> FeatureDef {
    FeatureDef {
        name,
        kind: FeatureType::Discrete,
        keywords,
    }
}

const FEATURES: [FeatureDef; 35] = [
    feature("width", FeatureType::Length),
    feature("height", FeatureType::Length),
    feature("device-width", FeatureType::Length),
    feature("device-height", FeatureType::Length),
    feature("inline-size", FeatureType::Length),
    feature("block-size", FeatureType::Length),
    feature("aspect-ratio", FeatureType::Ratio),
    feature("device-aspect-ratio", FeatureType::Ratio),
    feature("resolution", FeatureType::Resolution),
    feature("color", FeatureType::Integer),
    feature("color-index", FeatureType::Integer),
    feature("monochrome", FeatureType::Integer),
    feature("grid", FeatureType::Integer),
    feature("-webkit-device-pixel-ratio", FeatureType::Number),
    discrete("orientation", ORIENTATION),
    discrete("scan", SCAN),
    discrete("overflow-block", OVERFLOW_BLOCK),
    discrete("overflow-inline", OVERFLOW_INLINE),
    discrete("update", UPDATE),
    discrete("hover", HOVER),
    discrete("any-hover", HOVER),
    discrete("pointer", POINTER),
    discrete("any-pointer", POINTER),
    discrete("prefers-color-scheme", SCHEME),
    discrete("prefers-reduced-motion", MOTION),
    discrete("prefers-reduced-transparency", MOTION),
    discrete("prefers-contrast", CONTRAST),
    discrete("prefers-reduced-data", DATA),
    discrete("forced-colors", FORCED),
    discrete("inverted-colors", INVERTED),
    discrete("color-gamut", GAMUT),
    discrete("scripting", SCRIPTING),
    discrete("display-mode", DISPLAY_MODE),
    discrete("dynamic-range", DYNAMIC_RANGE),
    discrete("video-dynamic-range", DYNAMIC_RANGE),
];

fn caseless(a: &[u8], b: &str) -> bool {
    a.eq_ignore_ascii_case(b.as_bytes())
}

fn feature_lookup(name: &[u8]) -> Option<&'static FeatureDef> {
    FEATURES.iter().find(|f| caseless(name, f.name))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn flipped(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
            Op::Eq => Op::Eq,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }
}

#[derive(Clone, Default)]
struct Value {
    num: f64,
    denom: f64,
    ident: &'static str,
}

enum Node {
    Not(Box<Node>),
    And(Vec<Node>),
    Or(Vec<Node>),
    Feature(FeatureNode),
    Enclosed(Vec<u8>),
}

struct FeatureNode {
    feature: &'static FeatureDef,
    minmax: u8,
    nops: u8,
    plain: bool,
    op1: Op,
    op2: Op,
    v1: Value,
    v2: Value,
}

impl FeatureNode {
    fn new(feature: &'static FeatureDef) -> FeatureNode {
        FeatureNode {
            feature,
            minmax: 0,
            nops: 0,
            plain: false,
            op1: Op::Eq,
            op2: Op::Eq,
            v1: Value::default(),
            v2: Value::default(),
        }
    }
}

#[derive(Default)]
struct Query {
    cond: Option<Node>,
    kind: Option<Vec<u8>>,
    negated: bool,
    only: bool,
    valid: bool,
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

struct Text<'a> {
    bytes: &'a [u8],
}

impl Text<'_> {
    fn at(&self, i: usize) -> u8 {
        self.bytes.get(i).copied().unwrap_or(0)
    }

    fn skip_ws(&self, mut p: usize, end: usize) -> usize {
        while p < end && is_ws(self.at(p)) {
            p += 1;
        }
        p
    }

    fn trim_end(&self, s: usize, mut e: usize) -> usize {
        while e > s && is_ws(self.at(e - 1)) {
            e -= 1;
        }
        e
    }

    fn read_ident(&self, mut p: usize, end: usize, buflen: usize) -> Option<(usize, Vec<u8>)> {
        let s = p;
        while p < end && (self.at(p).is_ascii_alphanumeric() || matches!(self.at(p), b'-' | b'_')) {
            p += 1;
        }
        if p == s || p - s >= buflen {
            return None;
        }
        Some((p, self.bytes[s..p].to_ascii_lowercase()))
    }

    fn kw_at(&self, p: usize, end: usize, kw: &str) -> bool {
        let n = kw.len();
        if end < p || end - p < n {
            return false;
        }
        if !self.bytes[p..p + n].eq_ignore_ascii_case(kw.as_bytes()) {
            return false;
        }
        let after = p + n;
        after == end || is_ws(self.at(after)) || self.at(after) == b'('
    }

    fn close_paren(&self, mut p: usize, end: usize) -> usize {
        let mut depth = 1;
        while p < end {
            match self.at(p) {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return p;
                    }
                }
                _ => {}
            }
            p += 1;
        }
        end
    }

    fn any_value_ok(&self, p: usize, end: usize) -> bool {
        let (mut paren, mut bracket, mut brace) = (0i32, 0i32, 0i32);
        for &c in &self.bytes[p..end] {
            let counter = match c {
                b'(' => {
                    paren += 1;
                    continue;
                }
                b')' => &mut paren,
                b'[' => {
                    bracket += 1;
                    continue;
                }
                b']' => &mut bracket,
                b'{' => {
                    brace += 1;
                    continue;
                }
                b'}' => &mut brace,
                b';' | b'!' => return false,
                _ => continue,
            };
            *counter -= 1;
            if *counter < 0 {
                return false;
            }
        }
        true
    }

    fn strtod(&self, p: usize) -> (f64, usize) {
        let (value, consumed) = ffi::ascii_strtod(self.bytes.get(p..).unwrap_or_default());
        (value, p + consumed)
    }

    fn parse_number(&self, s: usize, e: usize) -> Option<(f64, usize)> {
        let (v, num_end) = self.strtod(s);
        (num_end != s && num_end <= e).then_some((v, num_end))
    }
}

type UnitFn = fn(f64, &[u8]) -> f64;

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn length_unit_px(v: f64, unit: &[u8]) -> f64 {
    let is = |u: &str| caseless(unit, u);
    if unit.is_empty() || is("px") {
        v
    } else if is("em") || is("rem") {
        v * 16.0
    } else if is("ex") || is("ch") {
        v * 8.0
    } else if is("cm") {
        v * 96.0 / 2.54
    } else if is("mm") {
        v * 96.0 / 25.4
    } else if is("q") {
        v * 96.0 / 101.6
    } else if is("in") {
        v * 96.0
    } else if is("pt") {
        v * 96.0 / 72.0
    } else if is("pc") {
        v * 16.0
    } else if is("vw") {
        v * viewport_w() / 100.0
    } else if is("vh") {
        v * viewport_h() / 100.0
    } else if is("vmin") {
        v * c_min(viewport_w(), viewport_h()) / 100.0
    } else if is("vmax") {
        v * c_max(viewport_w(), viewport_h()) / 100.0
    } else {
        f64::NAN
    }
}

fn resolution_unit_dppx(v: f64, unit: &[u8]) -> f64 {
    let is = |u: &str| caseless(unit, u);
    if is("dppx") || is("x") {
        v
    } else if is("dpi") {
        v / 96.0
    } else if is("dpcm") {
        v * 2.54 / 96.0
    } else {
        f64::NAN
    }
}

fn starts_calc(t: &Text<'_>, p: usize) -> bool {
    t.bytes
        .get(p..p + 5)
        .is_some_and(|s| s.eq_ignore_ascii_case(b"calc("))
}

fn calc_primary(
    t: &Text<'_>,
    pp: &mut usize,
    end: usize,
    f: UnitFn,
    depth: i32,
) -> Option<(f64, bool)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut p = t.skip_ws(*pp, end);
    if p < end && (t.at(p) == b'(' || starts_calc(t, p)) {
        p += if t.at(p) == b'(' { 1 } else { 5 };
        let out = calc_sum(t, &mut p, end, f, depth + 1)?;
        p = t.skip_ws(p, end);
        if p < end && t.at(p) == b')' {
            p += 1;
        }
        *pp = p;
        return Some((out, false));
    }
    let (v, num_end) = t.strtod(p);
    if num_end == p || num_end > end {
        return None;
    }
    let mut unit = Vec::new();
    let mut q = num_end;
    while q < end && (t.at(q).is_ascii_alphabetic() || t.at(q) == b'%') && unit.len() < 7 {
        unit.push(t.at(q));
        q += 1;
    }
    let result = if unit.is_empty() {
        (v, true)
    } else {
        let c = f(v, &unit);
        if c.is_nan() {
            return None;
        }
        (c, false)
    };
    *pp = q;
    Some(result)
}

fn calc_product(
    t: &Text<'_>,
    pp: &mut usize,
    end: usize,
    f: UnitFn,
    depth: i32,
) -> Option<(f64, bool)> {
    let (mut out, mut is_number) = calc_primary(t, pp, end, f, depth)?;
    loop {
        let mut p = t.skip_ws(*pp, end);
        if p >= end || (t.at(p) != b'*' && t.at(p) != b'/') {
            return Some((out, is_number));
        }
        let op = t.at(p);
        p += 1;
        let (rhs, rhs_num) = calc_primary(t, &mut p, end, f, depth)?;
        if op == b'*' {
            if !is_number && !rhs_num {
                return None;
            }
            out *= rhs;
            is_number = is_number && rhs_num;
        } else {
            if !rhs_num || rhs == 0.0 {
                return None;
            }
            out /= rhs;
        }
        *pp = p;
    }
}

fn calc_sum(t: &Text<'_>, pp: &mut usize, end: usize, f: UnitFn, depth: i32) -> Option<f64> {
    if depth > MAX_DEPTH {
        return None;
    }
    let (mut out, is_number) = calc_product(t, pp, end, f, depth)?;
    loop {
        let mut p = t.skip_ws(*pp, end);
        if p >= end || (t.at(p) != b'+' && t.at(p) != b'-') {
            return Some(out);
        }
        if !is_ws(t.at(p.wrapping_sub(1))) {
            return None;
        }
        let op = t.at(p);
        p += 1;
        if p >= end || !is_ws(t.at(p)) {
            return None;
        }
        let (rhs, rhs_num) = calc_product(t, &mut p, end, f, depth)?;
        if rhs_num != is_number {
            return None;
        }
        out += if op == b'+' { rhs } else { -rhs };
        *pp = p;
    }
}

fn parse_calc(t: &Text<'_>, s: usize, end: usize, f: UnitFn) -> Option<f64> {
    if !starts_calc(t, s) {
        return None;
    }
    let mut p = s + 5;
    let out = calc_sum(t, &mut p, end, f, 0)?;
    p = t.skip_ws(p, end);
    if p < end && t.at(p) == b')' {
        p += 1;
    }
    (t.skip_ws(p, end) == end).then_some(out)
}

fn read_unit(t: &Text<'_>, mut p: usize, e: usize) -> (usize, Vec<u8>) {
    let mut unit = Vec::new();
    while p < e && t.at(p).is_ascii_alphabetic() && unit.len() < 7 {
        unit.push(t.at(p).to_ascii_lowercase());
        p += 1;
    }
    (p, unit)
}

fn value_parse(t: &Text<'_>, s: usize, e: usize, f: &FeatureDef) -> Option<Value> {
    let s = t.skip_ws(s, e);
    let e = t.trim_end(s, e);
    if s >= e {
        return None;
    }
    let mut out = Value {
        num: 0.0,
        denom: 1.0,
        ident: "",
    };
    match f.kind {
        FeatureType::Discrete => {
            let (p, ident) = t.read_ident(s, e, 24)?;
            if t.skip_ws(p, e) != e {
                return None;
            }
            out.ident = f.keywords.iter().find(|kw| kw.as_bytes() == ident)?;
            Some(out)
        }
        FeatureType::Integer => {
            let mut p = s;
            if matches!(t.at(p), b'+' | b'-') {
                p += 1;
            }
            if p >= e || !t.bytes[p..e].iter().all(u8::is_ascii_digit) {
                return None;
            }
            out.num = t.strtod(s).0;
            if caseless(f.name.as_bytes(), "grid") && out.num != 0.0 && out.num != 1.0 {
                return None;
            }
            Some(out)
        }
        FeatureType::Number => {
            let (v, p) = t.parse_number(s, e)?;
            if t.skip_ws(p, e) != e {
                return None;
            }
            out.num = v;
            Some(out)
        }
        FeatureType::Ratio => {
            let (a, p) = t.parse_number(s, e)?;
            if a < 0.0 {
                return None;
            }
            let mut b = 1.0;
            let p = t.skip_ws(p, e);
            if p < e {
                if t.at(p) != b'/' {
                    return None;
                }
                let p = t.skip_ws(p + 1, e);
                let (value, q) = t.parse_number(p, e)?;
                if value < 0.0 || t.skip_ws(q, e) != e {
                    return None;
                }
                b = value;
            }
            out.num = a;
            out.denom = b;
            Some(out)
        }
        FeatureType::Resolution => {
            if starts_calc(t, s) {
                out.num = parse_calc(t, s, e, resolution_unit_dppx)?;
                return Some(out);
            }
            let (v, p) = t.parse_number(s, e)?;
            if v < 0.0 {
                return None;
            }
            let (p, unit) = read_unit(t, p, e);
            if p != e || unit.is_empty() {
                return None;
            }
            let dppx = resolution_unit_dppx(v, &unit);
            if dppx.is_nan() {
                return None;
            }
            out.num = dppx;
            Some(out)
        }
        FeatureType::Length => {
            if starts_calc(t, s) {
                out.num = parse_calc(t, s, e, length_unit_px)?;
                return Some(out);
            }
            let (v, p) = t.parse_number(s, e)?;
            let (p, unit) = read_unit(t, p, e);
            if p != e {
                return None;
            }
            if unit.is_empty() {
                if v != 0.0 {
                    return None;
                }
                out.num = 0.0;
                return Some(out);
            }
            let px = length_unit_px(v, &unit);
            if px.is_nan() {
                return None;
            }
            out.num = px;
            Some(out)
        }
    }
}

fn find_op(t: &Text<'_>, mut p: usize, end: usize) -> Option<(usize, Op, usize)> {
    let mut depth = 0;
    while p < end {
        let c = t.at(p);
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            if depth > 0 {
                depth -= 1;
            }
        } else if depth == 0 && matches!(c, b'<' | b'>' | b'=') {
            let eq_next = p + 1 < end && t.at(p + 1) == b'=';
            let (op, len) = match c {
                b'<' if eq_next => (Op::Le, 2),
                b'<' => (Op::Lt, 1),
                b'>' if eq_next => (Op::Ge, 2),
                b'>' => (Op::Gt, 1),
                _ => (Op::Eq, 1),
            };
            return Some((p, op, len));
        }
        p += 1;
    }
    None
}

fn side_is_feature(t: &Text<'_>, s: usize, e: usize) -> Option<&'static FeatureDef> {
    let s = t.skip_ws(s, e);
    let e = t.trim_end(s, e);
    let (p, ident) = t.read_ident(s, e, 40)?;
    if p != e {
        return None;
    }
    let f = feature_lookup(&ident)?;
    if f.kind == FeatureType::Discrete || caseless(f.name.as_bytes(), "grid") {
        return None;
    }
    Some(f)
}

fn parse_feature(t: &Text<'_>, s: usize, e: usize) -> Option<FeatureNode> {
    let s = t.skip_ws(s, e);
    let e = t.trim_end(s, e);
    if s >= e {
        return None;
    }
    let mut colon = None;
    let mut depth = 0;
    for p in s..e {
        match t.at(p) {
            b'(' => depth += 1,
            b')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b':' if depth == 0 => {
                colon = Some(p);
                break;
            }
            _ => {}
        }
    }
    if let Some(colon) = colon {
        let (ne, name) = t.read_ident(s, colon, 40)?;
        if t.skip_ws(ne, colon) != colon {
            return None;
        }
        let (minmax, base) = if name.starts_with(b"min-") {
            (1, name[4..].to_vec())
        } else if name.starts_with(b"max-") {
            (2, name[4..].to_vec())
        } else if name.starts_with(b"-webkit-min-") || name.starts_with(b"-webkit-max-") {
            let mut base = b"-webkit-".to_vec();
            base.extend_from_slice(&name[12..]);
            (if name[9] == b'i' { 1 } else { 2 }, base)
        } else {
            (0, name)
        };
        let f = feature_lookup(&base)?;
        if minmax != 0 && (f.kind == FeatureType::Discrete || caseless(f.name.as_bytes(), "grid")) {
            return None;
        }
        let v = value_parse(t, colon + 1, e, f)?;
        let mut node = FeatureNode::new(f);
        node.minmax = minmax;
        node.plain = true;
        node.nops = 1;
        node.op1 = match minmax {
            1 => Op::Ge,
            2 => Op::Le,
            _ => Op::Eq,
        };
        node.v1 = v;
        return Some(node);
    }
    if let Some((p1, op1, len1)) = find_op(t, s, e) {
        let after1 = p1 + len1;
        if let Some((p2, op2, len2)) = find_op(t, after1, e) {
            let f = side_is_feature(t, after1, p2)?;
            let lt = |op: Op| matches!(op, Op::Lt | Op::Le);
            let gt = |op: Op| matches!(op, Op::Gt | Op::Ge);
            if !((lt(op1) && lt(op2)) || (gt(op1) && gt(op2))) {
                return None;
            }
            let va = value_parse(t, s, p1, f)?;
            let vb = value_parse(t, p2 + len2, e, f)?;
            let mut node = FeatureNode::new(f);
            node.nops = 2;
            node.op1 = op1;
            node.op2 = op2;
            node.v1 = va;
            node.v2 = vb;
            return Some(node);
        }
        if let Some(f) = side_is_feature(t, s, p1) {
            let v = value_parse(t, after1, e, f)?;
            let mut node = FeatureNode::new(f);
            node.nops = 1;
            node.op1 = op1;
            node.v1 = v;
            return Some(node);
        }
        let f = side_is_feature(t, after1, e)?;
        let v = value_parse(t, s, p1, f)?;
        let mut node = FeatureNode::new(f);
        node.nops = 1;
        node.op1 = op1.flipped();
        node.v1 = v;
        return Some(node);
    }
    let (p, ident) = t.read_ident(s, e, 40)?;
    if t.skip_ws(p, e) != e {
        return None;
    }
    Some(FeatureNode::new(feature_lookup(&ident)?))
}

fn parse_in_parens(t: &Text<'_>, pp: &mut usize, end: usize, depth: i32) -> Option<Node> {
    if depth > MAX_DEPTH {
        return None;
    }
    let p = t.skip_ws(*pp, end);
    let after = |close: usize| if close < end { close + 1 } else { end };
    if p < end && t.at(p) == b'(' {
        let inner = p + 1;
        let close = t.close_paren(inner, end);
        let mut cp = inner;
        if let Some(cond) = parse_condition(t, &mut cp, close, true, depth + 1)
            && t.skip_ws(cp, close) == close
        {
            *pp = after(close);
            return Some(Node::And(vec![cond]));
        }
        if let Some(feature) = parse_feature(t, inner, close) {
            *pp = after(close);
            return Some(Node::Feature(feature));
        }
        if !t.any_value_ok(inner, close) {
            return None;
        }
        let text = t.bytes[inner..close].trim_ascii().to_vec();
        *pp = after(close);
        return Some(Node::Enclosed(text));
    }
    let (q, _) = t.read_ident(p, end, 40)?;
    if q < end && t.at(q) == b'(' {
        let inner = q + 1;
        let close = t.close_paren(inner, end);
        if !t.any_value_ok(inner, close) {
            return None;
        }
        let text = t.bytes[p..after(close)].to_vec();
        *pp = after(close);
        return Some(Node::Enclosed(text));
    }
    None
}

fn parse_condition(
    t: &Text<'_>,
    pp: &mut usize,
    end: usize,
    allow_or: bool,
    depth: i32,
) -> Option<Node> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut p = t.skip_ws(*pp, end);
    if t.kw_at(p, end, "not") {
        let mut q = p + 3;
        let child = parse_in_parens(t, &mut q, end, depth + 1)?;
        *pp = q;
        return Some(Node::Not(Box::new(child)));
    }
    let first = parse_in_parens(t, &mut p, end, depth + 1)?;
    let q = t.skip_ws(p, end);
    if q >= end {
        *pp = p;
        return Some(first);
    }
    let use_and = if t.kw_at(q, end, "and") {
        true
    } else if allow_or && t.kw_at(q, end, "or") {
        false
    } else {
        *pp = p;
        return Some(first);
    };
    let mut kids = vec![first];
    loop {
        let mut q = t.skip_ws(p, end);
        if q >= end {
            break;
        }
        if use_and && t.kw_at(q, end, "and") {
            q += 3;
        } else if !use_and && t.kw_at(q, end, "or") {
            q += 2;
        } else {
            break;
        }
        kids.push(parse_in_parens(t, &mut q, end, depth + 1)?);
        p = q;
    }
    *pp = p;
    Some(if use_and {
        Node::And(kids)
    } else {
        Node::Or(kids)
    })
}

fn type_reserved(ident: &[u8]) -> bool {
    matches!(ident, b"not" | b"only" | b"and" | b"or" | b"layer")
}

fn parse_query(t: &Text<'_>, s: usize, e: usize) -> Query {
    let mut out = Query::default();
    let s = t.skip_ws(s, e);
    let e = t.trim_end(s, e);
    if s >= e {
        return out;
    }
    let mut p = s;
    if let Some(cond) = parse_condition(t, &mut p, e, true, 0)
        && t.skip_ws(p, e) == e
    {
        out.cond = Some(cond);
        out.valid = true;
        return out;
    }
    let mut p = s;
    let Some((q, ident)) = t.read_ident(p, e, 64) else {
        return out;
    };
    let (mut negated, mut only) = (false, false);
    if ident == b"not" {
        negated = true;
        p = q;
    } else if ident == b"only" {
        only = true;
        p = q;
    }
    p = t.skip_ws(p, e);
    let Some((q, kind)) = t.read_ident(p, e, 64) else {
        return out;
    };
    if type_reserved(&kind) {
        return out;
    }
    let mut p = t.skip_ws(q, e);
    let mut tail = None;
    if p < e {
        if !t.kw_at(p, e, "and") {
            return out;
        }
        p += 3;
        match parse_condition(t, &mut p, e, false, 0) {
            Some(cond) if t.skip_ws(p, e) == e => tail = Some(cond),
            _ => return out,
        }
    }
    out.kind = Some(kind);
    out.cond = tail;
    out.negated = negated;
    out.only = only;
    out.valid = true;
    out
}

fn discrete_current(f: &FeatureDef) -> Option<&'static str> {
    Some(match f.name {
        "orientation" => {
            if viewport_w() >= viewport_h() {
                "landscape"
            } else {
                "portrait"
            }
        }
        "hover" | "any-hover" => "hover",
        "pointer" | "any-pointer" => "fine",
        "update" => "fast",
        "overflow-block" | "overflow-inline" => "scroll",
        "scripting" => "enabled",
        "display-mode" => "browser",
        "forced-colors" | "inverted-colors" => "none",
        "color-gamut" => "srgb",
        "dynamic-range" | "video-dynamic-range" => "standard",
        "prefers-color-scheme" => {
            if ffi::prefers_dark() {
                "dark"
            } else {
                "light"
            }
        }
        "prefers-reduced-motion" => {
            if ffi::prefers_reduced_motion() {
                "reduce"
            } else {
                "no-preference"
            }
        }
        "prefers-reduced-transparency" | "prefers-contrast" | "prefers-reduced-data" => {
            "no-preference"
        }
        _ => return None,
    })
}

fn feature_current(f: &FeatureDef) -> Option<(f64, f64)> {
    let device = || state(|s| (s.device_w, s.device_h));
    Some(match f.name {
        "width" | "inline-size" => (viewport_w(), 1.0),
        "height" | "block-size" => (viewport_h(), 1.0),
        "device-width" => (device().0, 1.0),
        "device-height" => (device().1, 1.0),
        "aspect-ratio" => (viewport_w(), viewport_h()),
        "device-aspect-ratio" => device(),
        "resolution" | "-webkit-device-pixel-ratio" => (device_pixel_ratio(), 1.0),
        "color" => (8.0, 1.0),
        "monochrome" | "color-index" | "grid" => (0.0, 1.0),
        _ => return None,
    })
}

fn compare(a: f64, ad: f64, op: Op, b: f64, bd: f64) -> Tri {
    let lhs = a * bd;
    let rhs = b * ad;
    let yes = match op {
        Op::Eq => lhs == rhs,
        Op::Lt => lhs < rhs,
        Op::Le => lhs <= rhs,
        Op::Gt => lhs > rhs,
        Op::Ge => lhs >= rhs,
    };
    if yes { Tri::Yes } else { Tri::No }
}

fn eval_feature(n: &FeatureNode) -> Tri {
    let f = n.feature;
    if f.kind == FeatureType::Discrete {
        let Some(cur) = discrete_current(f) else {
            return Tri::No;
        };
        if n.nops == 0 {
            return if cur == "none" || cur == "no-preference" {
                Tri::No
            } else {
                Tri::Yes
            };
        }
        return if cur == n.v1.ident { Tri::Yes } else { Tri::No };
    }
    let Some((cur, curd)) = feature_current(f) else {
        return Tri::Dunno;
    };
    match n.nops {
        0 => {
            if n.minmax != 0 {
                Tri::Dunno
            } else if f.kind == FeatureType::Ratio {
                if cur > 0.0 && curd > 0.0 {
                    Tri::Yes
                } else {
                    Tri::No
                }
            } else if cur != 0.0 {
                Tri::Yes
            } else {
                Tri::No
            }
        }
        2 => tri_and(
            compare(cur, curd, n.op1.flipped(), n.v1.num, n.v1.denom),
            compare(cur, curd, n.op2, n.v2.num, n.v2.denom),
        ),
        _ => compare(cur, curd, n.op1, n.v1.num, n.v1.denom),
    }
}

fn eval_node(n: &Node, depth: i32) -> Tri {
    if depth > MAX_DEPTH {
        return Tri::Dunno;
    }
    match n {
        Node::Enclosed(_) => Tri::Dunno,
        Node::Feature(feature) => eval_feature(feature),
        Node::Not(child) => tri_not(eval_node(child, depth + 1)),
        Node::And(kids) => kids
            .iter()
            .fold(Tri::Yes, |r, kid| tri_and(r, eval_node(kid, depth + 1))),
        Node::Or(kids) => kids
            .iter()
            .fold(Tri::No, |r, kid| tri_or(r, eval_node(kid, depth + 1))),
    }
}

fn eval_type(kind: Option<&[u8]>) -> Tri {
    let Some(kind) = kind else {
        return Tri::Yes;
    };
    let current = if print_media() { "print" } else { "screen" };
    if caseless(kind, "all") || caseless(kind, current) {
        Tri::Yes
    } else {
        Tri::No
    }
}

fn eval_query(q: &Query) -> Tri {
    if !q.valid {
        return Tri::No;
    }
    let mut r = eval_type(q.kind.as_deref());
    if let Some(cond) = &q.cond {
        r = tri_and(r, eval_node(cond, 0));
    }
    if q.negated {
        r = tri_not(r);
    }
    r
}

fn split_next(t: &Text<'_>, p: usize, end: usize) -> (usize, usize) {
    let (mut paren, mut bracket) = (0, 0);
    for q in p..end {
        match t.at(q) {
            b'(' => paren += 1,
            b')' => {
                if paren > 0 {
                    paren -= 1;
                }
            }
            b'[' => bracket += 1,
            b']' => {
                if bracket > 0 {
                    bracket -= 1;
                }
            }
            b',' if paren == 0 && bracket == 0 => return (q, q + 1),
            _ => {}
        }
    }
    (end, end)
}

fn strip_comments(query: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(query.len());
    let mut quote = 0u8;
    let mut p = 0;
    while p < query.len() {
        let c = query[p];
        if quote != 0 {
            if c == b'\\' && p + 1 < query.len() {
                out.push(c);
                p += 1;
            } else if c == quote {
                quote = 0;
            }
            out.push(query[p]);
            p += 1;
            continue;
        }
        if c == b'/' && query.get(p + 1) == Some(&b'*') {
            out.push(b' ');
            match query[p + 2..].windows(2).position(|w| w == b"*/") {
                Some(close) => p += 2 + close + 2,
                None => break,
            }
            continue;
        }
        if c == b'"' || c == b'\'' {
            quote = c;
        }
        out.push(c);
        p += 1;
    }
    out
}

fn query_list_matches(query: &[u8]) -> bool {
    let t = Text { bytes: query };
    let end = query.len();
    let mut p = t.skip_ws(0, end);
    if p == end {
        return true;
    }
    while p < end {
        let (seg_end, next) = split_next(&t, p, end);
        if eval_query(&parse_query(&t, p, seg_end)) == Tri::Yes {
            return true;
        }
        p = next;
    }
    false
}

pub fn query_matches(query: &[u8]) -> bool {
    if !query.windows(2).any(|w| w == b"/*") {
        return query_list_matches(query);
    }
    query_list_matches(&strip_comments(query))
}

fn serialize_value(out: &mut Vec<u8>, v: &Value, kind: FeatureType) {
    match kind {
        FeatureType::Discrete => out.extend_from_slice(v.ident.as_bytes()),
        FeatureType::Integer | FeatureType::Number => out.extend_from_slice(&ffi::format_g(v.num)),
        FeatureType::Ratio => {
            out.extend_from_slice(&ffi::format_g(v.num));
            out.extend_from_slice(b" / ");
            out.extend_from_slice(&ffi::format_g(v.denom));
        }
        FeatureType::Resolution => {
            out.extend_from_slice(&ffi::format_g(v.num));
            out.extend_from_slice(b"dppx");
        }
        FeatureType::Length => {
            out.extend_from_slice(&ffi::format_g(v.num));
            out.extend_from_slice(b"px");
        }
    }
}

fn serialize_node(out: &mut Vec<u8>, n: &Node, depth: i32) {
    if depth > MAX_DEPTH {
        return;
    }
    match n {
        Node::Enclosed(text) => {
            if text.first() == Some(&b'(') || text.contains(&b'(') {
                out.extend_from_slice(text);
            } else {
                out.push(b'(');
                out.extend_from_slice(text);
                out.push(b')');
            }
        }
        Node::Feature(f) => {
            out.push(b'(');
            let def = f.feature;
            if f.nops == 0 {
                out.extend_from_slice(def.name.as_bytes());
            } else if f.plain {
                let mut name = def.name;
                if f.minmax != 0
                    && let Some(rest) = name.strip_prefix("-webkit-")
                {
                    out.extend_from_slice(b"-webkit-");
                    name = rest;
                }
                match f.minmax {
                    1 => out.extend_from_slice(b"min-"),
                    2 => out.extend_from_slice(b"max-"),
                    _ => {}
                }
                out.extend_from_slice(name.as_bytes());
                out.extend_from_slice(b": ");
                serialize_value(out, &f.v1, def.kind);
            } else if f.nops == 2 {
                serialize_value(out, &f.v1, def.kind);
                out.extend_from_slice(
                    format!(" {} {} {} ", f.op1.text(), def.name, f.op2.text()).as_bytes(),
                );
                serialize_value(out, &f.v2, def.kind);
            } else {
                out.extend_from_slice(def.name.as_bytes());
                out.extend_from_slice(format!(" {} ", f.op1.text()).as_bytes());
                serialize_value(out, &f.v1, def.kind);
            }
            out.push(b')');
        }
        Node::Not(child) => {
            out.extend_from_slice(b"not ");
            serialize_node(out, child, depth + 1);
        }
        Node::And(kids) | Node::Or(kids) => {
            if kids.len() == 1 && depth > 0 {
                out.push(b'(');
                serialize_node(out, &kids[0], depth + 1);
                out.push(b')');
                return;
            }
            let joiner: &[u8] = if matches!(n, Node::And(_)) {
                b" and "
            } else {
                b" or "
            };
            for (i, kid) in kids.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(joiner);
                }
                serialize_node(out, kid, depth + 1);
            }
        }
    }
}

fn serialize_query(out: &mut Vec<u8>, q: &Query) {
    if !q.valid {
        out.extend_from_slice(b"not all");
        return;
    }
    if let Some(kind) = &q.kind {
        if q.negated {
            out.extend_from_slice(b"not ");
        } else if q.only {
            out.extend_from_slice(b"only ");
        }
        out.extend_from_slice(&kind.to_ascii_lowercase());
        if let Some(cond) = &q.cond {
            out.extend_from_slice(b" and ");
            serialize_node(out, cond, 1);
        }
        return;
    }
    if let Some(cond) = &q.cond {
        serialize_node(out, cond, 0);
    }
}

pub fn list_serialize(query: &[u8]) -> Vec<u8> {
    let t = Text { bytes: query };
    let end = query.len();
    let mut p = t.skip_ws(0, end);
    let mut out = Vec::new();
    let mut first = true;
    while p < end {
        let (seg_end, next) = split_next(&t, p, end);
        let q = parse_query(&t, p, seg_end);
        if !first {
            out.extend_from_slice(b", ");
        }
        serialize_query(&mut out, &q);
        first = false;
        p = next;
    }
    out
}
