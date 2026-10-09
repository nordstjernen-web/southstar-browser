//! Southstar — CSS font values: the font-family list and the font shorthand in canonical form, their keyword and token checks, font-feature- and font-variation-settings, and resolving a family list to the font Pango should load.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CString;

use crate::calc;
use crate::ffi;
use crate::scan::{
    is_gspace, is_ws, scan_until, split_ws_limit, split_ws_paren, starts_with_ci, strip,
};
use crate::units::{self, CAP, CH, EX, IC, NUMBER};

const RANDOM_ITEM_MAX_DEPTH: i32 = 16;
const MEMO_MAX: usize = 1024;

thread_local! {
    static RANDOM_ITEM_NESTING: Cell<i32> = const { Cell::new(0) };
    static PANGO_MEMO: RefCell<PangoMemo> = RefCell::new(PangoMemo::default());
}

#[derive(Default)]
struct PangoMemo {
    map: HashMap<Vec<u8>, Vec<u8>>,
    generation: u64,
    oracle: u32,
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn token_clean(raw: &[u8]) -> Vec<u8> {
    let mut s = raw;
    while let [first, rest @ ..] = s {
        if !is_ws(*first) {
            break;
        }
        s = rest;
    }
    while let [rest @ .., last] = s {
        if !is_ws(*last) {
            break;
        }
        s = rest;
    }
    if s.len() >= 2 && (s[0] == b'"' || s[0] == b'\'') && s[s.len() - 1] == s[0] {
        s = &s[1..s.len() - 1];
    }
    let mut out = Vec::with_capacity(s.len());
    let mut pending_space = false;
    let mut i = 0;
    while i < s.len() {
        let mut c = s[i];
        if c == b'\\' && i + 1 < s.len() {
            i += 1;
            c = s[i];
        }
        i += 1;
        if is_ws(c) {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(b' ');
            pending_space = false;
        }
        out.push(c);
    }
    strip(&out).to_vec()
}

fn map_generic(token: &[u8]) -> Option<Vec<u8>> {
    let lo = token.to_ascii_lowercase();
    let mapped: &[u8] = match lo.as_slice() {
        b"system-ui" => b"system-ui",
        b"ui-sans-serif" | b"ui-rounded" | b"sans-serif" => b"sans-serif",
        b"ui-serif" | b"serif" if cfg!(windows) => b"Times New Roman",
        b"ui-serif" | b"serif" => b"serif",
        b"ui-monospace" | b"monospace" => b"monospace",
        b"cursive" | b"fantasy" | b"emoji" | b"math" | b"fangsong" => &lo,
        _ => return None,
    };
    Some(mapped.to_vec())
}

fn substitute(token: &[u8]) -> Option<&'static [u8]> {
    let lo = token.to_ascii_lowercase();
    if lo.starts_with(b"sf pro") || lo.starts_with(b"sfpro") {
        Some(b"system-ui")
    } else if matches!(lo.as_slice(), b"arial" | b"helvetica" | b"segoe ui")
        || lo.starts_with(b"roboto")
        || lo.starts_with(b"optimistic text")
    {
        Some(b"sans-serif")
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn platform_family(generic: &[u8]) -> Option<&'static [u8]> {
    const FAMILIES: [(&[u8], &[u8]); 6] = [
        (b"system-ui", b"System Font"),
        (b"sans-serif", b"Helvetica"),
        (b"serif", b"Times"),
        (b"monospace", b"Menlo"),
        (b"cursive", b"Apple Chancery"),
        (b"fantasy", b"Papyrus"),
    ];
    FAMILIES
        .iter()
        .find(|(name, family)| *name == generic && ffi::font_available(family) == Some(true))
        .map(|(_, family)| *family)
}

#[cfg(not(target_os = "macos"))]
fn platform_family(_generic: &[u8]) -> Option<&'static [u8]> {
    None
}

fn platform_family_for_generic(generic: &[u8]) -> Vec<u8> {
    if let Some(family) = platform_family(generic) {
        return family.to_vec();
    }
    if generic == b"system-ui" {
        return platform_family_for_generic(b"sans-serif");
    }
    generic.to_vec()
}

fn platform_has_system_font() -> bool {
    platform_family_for_generic(b"system-ui") != platform_family_for_generic(b"sans-serif")
}

fn family_resolve(css_family: &[u8]) -> Vec<u8> {
    let mut fallback: Option<&'static [u8]> = None;
    let end = css_family.len();
    let mut p = 0;
    while p < end {
        while p < end && css_family[p] == b',' {
            p += 1;
        }
        let start = p;
        let mut quote = 0u8;
        while p < end {
            let c = css_family[p];
            if quote != 0 {
                if c == b'\\' && p + 1 < end {
                    p += 1;
                } else if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b',' {
                break;
            }
            p += 1;
        }
        let token = token_clean(&css_family[start..p]);
        if !token.is_empty() {
            let lo = token.to_ascii_lowercase();
            let skip = matches!(
                lo.as_slice(),
                b"inherit" | b"initial" | b"unset" | b"revert" | b"revert-layer"
            ) || lo.windows(15).any(|w| w == b"linux libertine")
                || lo.starts_with(b"libertinus")
                || lo.starts_with(b"var(");
            let system_alias = lo == b"-apple-system" || lo == b"blinkmacsystemfont";
            if system_alias && platform_has_system_font() {
                return b"system-ui".to_vec();
            } else if system_alias {
                fallback.get_or_insert(b"sans-serif");
            } else if !skip {
                if let Some(mapped) = map_generic(&token) {
                    return mapped;
                }
                if ffi::font_available(&token) != Some(false) {
                    return token;
                }
                if let Some(substitute) = substitute(&token) {
                    return substitute.to_vec();
                }
                fallback.get_or_insert(b"sans-serif");
            }
        }
        if p < end && css_family[p] == b',' {
            p += 1;
        }
    }
    fallback.unwrap_or(b"sans-serif").to_vec()
}

pub(crate) fn family_for_pango(css_family: &[u8]) -> Vec<u8> {
    if css_family.is_empty() {
        return b"sans-serif".to_vec();
    }
    let generation = ffi::font_generation();
    let oracle = ffi::font_oracle_serial();
    let hit = PANGO_MEMO.with_borrow_mut(|memo| {
        if generation != memo.generation || oracle != memo.oracle || memo.map.len() >= MEMO_MAX {
            memo.map.clear();
            memo.generation = generation;
            memo.oracle = oracle;
        }
        memo.map.get(css_family).cloned()
    });
    if let Some(hit) = hit {
        return hit;
    }
    let resolved = platform_family_for_generic(&family_resolve(css_family));
    PANGO_MEMO.with_borrow_mut(|memo| {
        memo.map.insert(css_family.to_vec(), resolved.clone());
    });
    resolved
}

pub(crate) fn weight_relative(parent: i32, bolder: bool) -> i32 {
    if bolder {
        return if parent < 350 {
            400
        } else if parent < 550 {
            700
        } else {
            900
        };
    }
    if parent < 100 {
        parent
    } else if parent < 550 {
        100
    } else if parent < 750 {
        400
    } else {
        700
    }
}

pub(crate) fn weight_number(keyword: Option<&[u8]>, fallback: i32) -> i32 {
    let Some(kw) = keyword else {
        return fallback;
    };
    let base = if fallback > 0 { fallback } else { 400 };
    match kw {
        b"normal" => 400,
        b"bold" => 700,
        b"bolder" | b"lighter" => weight_relative(base, kw[0] == b'b'),
        _ if kw.first().is_some_and(u8::is_ascii_digit) => ffi::parse_int(kw, base, 1, 1000),
        _ => fallback,
    }
}

pub(crate) fn stretch_rank(keyword: Option<&[u8]>, percent: Option<f64>) -> i32 {
    if let Some(kw) = keyword {
        let rank = [
            &b"ultra-condensed"[..],
            b"extra-condensed",
            b"condensed",
            b"semi-condensed",
            b"normal",
            b"semi-expanded",
            b"expanded",
            b"extra-expanded",
            b"ultra-expanded",
        ]
        .iter()
        .position(|name| *name == kw);
        if let Some(rank) = rank {
            return rank as i32;
        }
    }
    let Some(p) = percent else {
        return 4;
    };
    [56.25, 68.75, 81.25, 93.75, 106.25, 118.75, 137.5, 175.0]
        .iter()
        .position(|&limit| p <= limit)
        .unwrap_or(8) as i32
}

pub(crate) fn relative_unit_px(unit: u32, metrics: &[f64; 4], font_px: f64) -> f64 {
    match unit {
        EX => metrics[0],
        CH => metrics[1],
        CAP => metrics[2],
        IC => metrics[3],
        _ => font_px,
    }
}

pub(crate) fn size_keyword_px(t: &[u8]) -> f64 {
    const SIZES: [(&[u8], f64); 8] = [
        (b"xx-small", 9.0),
        (b"x-small", 10.0),
        (b"small", 13.0),
        (b"medium", 16.0),
        (b"large", 18.0),
        (b"x-large", 24.0),
        (b"xx-large", 32.0),
        (b"xxx-large", 48.0),
    ];
    SIZES
        .iter()
        .find(|(name, _)| t.eq_ignore_ascii_case(name))
        .map_or(-1.0, |(_, px)| *px)
}

fn ident_valid(tok: &[u8]) -> bool {
    let len = tok.len();
    if len == 0 {
        return false;
    }
    let mut i = 0;
    if tok[0] == b'-' {
        i = 1;
        if len == 1 {
            return false;
        }
        if tok[1] == b'-' {
            i = 2;
        }
    }
    if i >= len {
        return false;
    }
    let first = tok[i];
    if !(first.is_ascii_alphabetic() || first == b'_' || first >= 0x80 || first == b'\\') {
        return false;
    }
    let mut k = i;
    while k < len {
        let c = tok[k];
        if c == b'\\' {
            k += 2;
            continue;
        }
        if !(c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80) {
            return false;
        }
        k += 1;
    }
    true
}

fn is_generic(tok: &[u8]) -> bool {
    [
        &b"serif"[..],
        b"sans-serif",
        b"cursive",
        b"fantasy",
        b"monospace",
        b"system-ui",
        b"ui-serif",
        b"ui-sans-serif",
        b"ui-monospace",
        b"ui-rounded",
        b"math",
        b"emoji",
        b"fangsong",
        b"inherit",
        b"initial",
        b"unset",
        b"revert",
        b"revert-layer",
        b"default",
    ]
    .iter()
    .any(|name| tok.eq_ignore_ascii_case(name))
}

fn is_math_prefix(tok: &[u8]) -> bool {
    [&b"calc("[..], b"min(", b"max(", b"clamp("]
        .iter()
        .any(|name| starts_with_ci(tok, name))
}

fn size_token_valid(tok: &[u8]) -> bool {
    if size_keyword_px(tok) > 0.0
        || tok.eq_ignore_ascii_case(b"larger")
        || tok.eq_ignore_ascii_case(b"smaller")
    {
        return true;
    }
    if is_math_prefix(tok) {
        return calc::parse_calc(&c_text(tok)).is_some();
    }
    units::parse_length(&c_text(tok)).is_some_and(|(v, unit)| unit != NUMBER && v >= 0.0)
}

fn line_height_token_valid(tok: &[u8]) -> bool {
    if tok.eq_ignore_ascii_case(b"normal") {
        return true;
    }
    if is_math_prefix(tok) {
        return calc::parse_calc(&c_text(tok)).is_some();
    }
    units::parse_length(&c_text(tok)).is_some_and(|(v, _)| v >= 0.0)
}

pub(crate) fn shorthand_slash(tok: &[u8]) -> Option<usize> {
    let mut depth = 0u32;
    for (i, &c) in tok.iter().enumerate() {
        if c == 0 {
            break;
        }
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth = depth.saturating_sub(1);
        } else if c == b'/' && depth == 0 {
            return Some(i);
        }
    }
    None
}

pub(crate) fn shorthand_is_size_token(tok: &[u8]) -> bool {
    let size = &tok[..shorthand_slash(tok).unwrap_or(tok.len())];
    !size.is_empty()
        && size_token_valid(size)
        && !(size[0].is_ascii_digit() && size.iter().all(|c| c.is_ascii_digit() || *c == b'.'))
}

pub(crate) fn is_stretch_keyword(s: &[u8]) -> bool {
    [
        &b"ultra-condensed"[..],
        b"extra-condensed",
        b"condensed",
        b"semi-condensed",
        b"normal",
        b"semi-expanded",
        b"expanded",
        b"extra-expanded",
        b"ultra-expanded",
    ]
    .iter()
    .any(|name| s.eq_ignore_ascii_case(name))
}

fn weight_token(t: &[u8]) -> bool {
    if !t.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let text = c_text(t);
    let (w, end) = ffi::strtod(&text, 0);
    end == text.to_bytes().len() && (1.0..=1000.0).contains(&w)
}

pub(crate) fn shorthand_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let tokens = split_ws_paren(text, 24);
    let n = tokens.len();
    let (mut style, mut variant, mut weight, mut stretch) = (None, None, None, None);
    let mut prefix = 0;
    let mut i = 0;
    while i < n && prefix < 4 {
        let t = tokens[i];
        let slot = if t.eq_ignore_ascii_case(b"normal") {
            None
        } else if t.eq_ignore_ascii_case(b"italic") || t.eq_ignore_ascii_case(b"oblique") {
            Some(&mut style)
        } else if t.eq_ignore_ascii_case(b"small-caps") {
            Some(&mut variant)
        } else if t.eq_ignore_ascii_case(b"bold")
            || t.eq_ignore_ascii_case(b"bolder")
            || t.eq_ignore_ascii_case(b"lighter")
            || weight_token(t)
        {
            Some(&mut weight)
        } else if is_stretch_keyword(t) {
            Some(&mut stretch)
        } else {
            break;
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                return None;
            }
            *slot = Some(t);
        }
        prefix += 1;
        i += 1;
    }
    if i >= n {
        return None;
    }
    let size_tok = tokens[i];
    let slash = shorthand_slash(size_tok);
    let size_only = &size_tok[..slash.unwrap_or(size_tok.len())];
    let mut fam_start = i + 1;
    let mut lh: Option<&[u8]> = None;
    if let Some(slash) = slash {
        if slash + 1 < size_tok.len() {
            lh = Some(&size_tok[slash + 1..]);
        } else if fam_start < n {
            lh = Some(tokens[fam_start]);
            fam_start += 1;
        } else {
            return None;
        }
    } else if fam_start < n && tokens[fam_start][0] == b'/' {
        if tokens[fam_start].len() > 1 {
            lh = Some(&tokens[fam_start][1..]);
        } else if fam_start + 1 < n {
            lh = Some(tokens[fam_start + 1]);
            fam_start += 1;
        }
        fam_start += 1;
    }
    if size_only.is_empty()
        || !size_token_valid(size_only)
        || lh.is_some_and(|lh| !line_height_token_valid(lh))
        || fam_start >= n
    {
        return None;
    }
    let family = family_canonical(&tokens[fam_start..].join(&b' '))?;
    let mut out = Vec::new();
    let mut push = |part: &[u8]| {
        if !out.is_empty() {
            out.push(b' ');
        }
        out.extend_from_slice(part);
    };
    if let Some(style) = style {
        push(if style.eq_ignore_ascii_case(b"italic") {
            b"italic"
        } else {
            b"oblique"
        });
    }
    if variant.is_some() {
        push(b"small-caps");
    }
    if let Some(weight) = weight {
        push(&weight.to_ascii_lowercase());
    }
    if let Some(stretch) = stretch {
        push(&stretch.to_ascii_lowercase());
    }
    if size_keyword_px(size_only) > 0.0
        || size_only.eq_ignore_ascii_case(b"larger")
        || size_only.eq_ignore_ascii_case(b"smaller")
    {
        push(&size_only.to_ascii_lowercase());
    } else {
        push(size_only);
    }
    if let Some(lh) = lh.filter(|lh| !lh.eq_ignore_ascii_case(b"normal")) {
        out.extend_from_slice(b" / ");
        out.extend_from_slice(lh);
    }
    out.push(b' ');
    out.extend_from_slice(&family);
    Some(out)
}

fn split_random_item_args(body: &[u8]) -> Vec<&[u8]> {
    let mut args = Vec::new();
    let mut depth = 0u32;
    let mut seg = 0;
    for (i, &c) in body.iter().enumerate() {
        if matches!(c, b'(' | b'{' | b'[') {
            depth += 1;
        } else if matches!(c, b')' | b'}' | b']') && depth > 0 {
            depth -= 1;
        }
        if c == b',' && depth == 0 {
            args.push(strip(&body[seg..i]));
            seg = i + 1;
        }
    }
    args.push(strip(&body[seg..]));
    args
}

fn random_item_valid(item: &[u8]) -> bool {
    if RANDOM_ITEM_NESTING.get() >= RANDOM_ITEM_MAX_DEPTH {
        return false;
    }
    let Some(open) = item.iter().position(|&c| c == b'(') else {
        return false;
    };
    if item[item.len() - 1] != b')' {
        return false;
    }
    let body = &item[open + 1..item.len() - 1];
    let body = &body[..body.iter().position(|&c| c == 0).unwrap_or(body.len())];
    let args = split_random_item_args(body);
    if args.len() < 2 {
        return false;
    }
    let tok = split_ws_limit(args[0], 3);
    if !(1..=2).contains(&tok.len()) {
        return false;
    }
    for (i, t) in tok.iter().enumerate() {
        let ok = if i == 1 && tok[0].eq_ignore_ascii_case(b"fixed") {
            let text = c_text(t);
            ffi::strtod(&text, 0).1 == text.to_bytes().len()
        } else {
            ident_valid(t) || (t.starts_with(b"--") && t.len() > 2)
        };
        if !ok {
            return false;
        }
    }
    for a in &args[1..] {
        if a.is_empty() {
            continue;
        }
        let inner = if a[0] == b'{' {
            if a[a.len() - 1] != b'}' {
                return false;
            }
            &a[1..a.len() - 1]
        } else if a.contains(&b'{') || a.contains(&b'}') {
            return false;
        } else {
            a
        };
        RANDOM_ITEM_NESTING.set(RANDOM_ITEM_NESTING.get() + 1);
        let canon = family_canonical(inner);
        RANDOM_ITEM_NESTING.set(RANDOM_ITEM_NESTING.get() - 1);
        if canon.is_none() {
            return false;
        }
    }
    true
}

fn quoted_family(out: &mut Vec<u8>, body: &[u8]) {
    let blen = body.len();
    let mut ident_like =
        blen > 0 && !is_ws(body[0]) && !is_ws(body[blen - 1]) && !body.contains(&b'\\');
    let mut w = 0;
    while w < blen && ident_like {
        let tok = w;
        while w < blen && !is_ws(body[w]) {
            w += 1;
        }
        let word = &body[tok..w];
        if !ident_valid(word) || is_generic(word) {
            ident_like = false;
        }
        if w < blen && (w + 1 >= blen || is_ws(body[w + 1])) {
            ident_like = false;
        }
        w += 1;
    }
    if ident_like {
        out.extend_from_slice(body);
        return;
    }
    out.push(b'"');
    let mut k = 0;
    while k < blen {
        let c = body[k];
        if c == b'\\' && k + 1 < blen {
            out.push(c);
            out.push(body[k + 1]);
            k += 2;
            continue;
        }
        if c == b'"' {
            out.push(b'\\');
        }
        out.push(c);
        k += 1;
    }
    out.push(b'"');
}

fn unquoted_family(out: &mut Vec<u8>, item: &[u8]) -> bool {
    if [
        &b"inherit"[..],
        b"initial",
        b"unset",
        b"revert",
        b"revert-layer",
        b"default",
    ]
    .iter()
    .any(|name| item.eq_ignore_ascii_case(name))
    {
        return false;
    }
    let qend = item.len();
    let mut q = 0;
    let mut first = true;
    while q < qend {
        while q < qend && is_ws(item[q]) {
            q += 1;
        }
        let tok = q;
        while q < qend && !is_ws(item[q]) {
            q += 1;
        }
        let word = &item[tok..q];
        if word.is_empty() {
            break;
        }
        let strict = [
            &b"serif"[..],
            b"sans-serif",
            b"cursive",
            b"fantasy",
            b"monospace",
        ]
        .iter()
        .any(|name| word.eq_ignore_ascii_case(name));
        if !ident_valid(word) || ((!first || q < qend) && strict) {
            return false;
        }
        if !first {
            out.push(b' ');
        }
        if first && q >= qend && is_generic(word) {
            out.extend_from_slice(&word.to_ascii_lowercase());
        } else {
            out.extend_from_slice(word);
        }
        first = false;
    }
    true
}

pub(crate) fn family_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let end = text.len();
    let mut out = Vec::new();
    let mut p = 0;
    loop {
        while p < end && is_ws(text[p]) {
            p += 1;
        }
        let item_start = p;
        let (item_end, term) = scan_until(text, p, end, b",");
        let mut ilen = item_end - item_start;
        while ilen > 0 && is_ws(text[item_start + ilen - 1]) {
            ilen -= 1;
        }
        if ilen == 0 {
            return None;
        }
        let item = &text[item_start..item_start + ilen];
        if !out.is_empty() {
            out.extend_from_slice(b", ");
        }
        if item[0] == b'"' || item[0] == b'\'' {
            if ilen < 2 || item[ilen - 1] != item[0] {
                return None;
            }
            quoted_family(&mut out, &item[1..ilen - 1]);
        } else if item[ilen - 1] == b')'
            && (starts_with_ci(item, b"random-item(") || starts_with_ci(item, b"-webkit-generic("))
        {
            if starts_with_ci(item, b"random-item(") && !random_item_valid(item) {
                return None;
            }
            out.extend_from_slice(item);
        } else if !unquoted_family(&mut out, item) {
            return None;
        }
        if term != b',' {
            break;
        }
        p = item_end + 1;
        if p >= end {
            return None;
        }
    }
    Some(out)
}

pub(crate) fn ligatures_valid(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    if s == b"normal" || s == b"none" {
        return true;
    }
    let mut any = false;
    for tok in s.split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c)) {
        if tok.is_empty() {
            continue;
        }
        any = true;
        if !matches!(
            tok,
            b"common-ligatures"
                | b"no-common-ligatures"
                | b"discretionary-ligatures"
                | b"no-discretionary-ligatures"
                | b"historical-ligatures"
                | b"no-historical-ligatures"
                | b"contextual"
                | b"no-contextual"
        ) {
            return false;
        }
    }
    any
}

fn feature_skip_ws(s: &[u8], mut p: usize) -> usize {
    while p < s.len() && is_gspace(s[p]) {
        p += 1;
    }
    p
}

fn feature_read_tag(s: &[u8], pos: &mut usize) -> bool {
    let mut p = feature_skip_ws(s, *pos);
    let Some(&quote) = s.get(p).filter(|&&c| c == b'"' || c == b'\'') else {
        return false;
    };
    p += 1;
    let start = p;
    while p < s.len() && s[p] != quote {
        p += 1;
    }
    if p >= s.len() || p - start != 4 || !s[start..p].iter().all(|c| (0x20..=0x7e).contains(c)) {
        return false;
    }
    *pos = p + 1;
    true
}

fn feature_read_optional_value(s: &[u8], pos: &mut usize) -> bool {
    let mut p = feature_skip_ws(s, *pos);
    if p >= s.len() || s[p] == b',' {
        *pos = p;
        return true;
    }
    if s[p].is_ascii_alphabetic() {
        let start = p;
        while p < s.len() && (s[p].is_ascii_alphabetic() || s[p] == b'-') {
            p += 1;
        }
        let kw = s[start..p].to_ascii_lowercase();
        if kw != b"on" && kw != b"off" {
            return false;
        }
        *pos = feature_skip_ws(s, p);
        return true;
    }
    if !s[p].is_ascii_digit() {
        return false;
    }
    while p < s.len() && s[p].is_ascii_digit() {
        p += 1;
    }
    *pos = feature_skip_ws(s, p);
    true
}

fn variation_read_value(s: &[u8], pos: &mut usize) -> bool {
    let p = feature_skip_ws(s, *pos);
    if p >= s.len() || s[p] == b',' {
        return false;
    }
    let text = c_text(s);
    let (v, end) = ffi::strtod(&text, p);
    if end == p || !v.is_finite() {
        return false;
    }
    *pos = feature_skip_ws(s, end);
    true
}

fn tag_list_valid(s: &[u8], read_value: fn(&[u8], &mut usize) -> bool) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut p = feature_skip_ws(s, 0);
    if p >= s.len() {
        return false;
    }
    while p < s.len() {
        if !feature_read_tag(s, &mut p) || !read_value(s, &mut p) {
            return false;
        }
        if p < s.len() && s[p] == b',' {
            p = feature_skip_ws(s, p + 1);
            if p >= s.len() {
                return false;
            }
            continue;
        }
        return p >= s.len();
    }
    false
}

pub(crate) fn feature_settings_valid(s: &[u8]) -> bool {
    tag_list_valid(s, feature_read_optional_value)
}

pub(crate) fn variation_settings_valid(s: &[u8]) -> bool {
    tag_list_valid(s, variation_read_value)
}
