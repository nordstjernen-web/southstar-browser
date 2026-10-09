//! Southstar — CSS grid values: track lists with repeat(), minmax() and fit-content() parsed into the ns_css_tracks css.c stores, grid-template-areas, grid lines and placements, the grid-template and grid shorthands, and their canonical specified spelling.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::calc::{self, Parsed};
use crate::container;
use crate::lex::{ident_serialize, read_ident, read_string};
use crate::math;
use crate::scan::{
    is_ws, scan_until, skip_ws, split_ws_limit, starts_with_ci, strtol10, trim_range,
};
use crate::transform::is_math_fn_start;
use crate::units::{
    self, CAP, CH, CQH, CQMAX, CQMIN, CQW, EM, EX, IC, LH, NUMBER, PERCENT, PX, REM, RLH,
};

pub(crate) const TRACKS_MAX: usize = 24;
pub(crate) const LINE_NAME_MAX: usize = 24;
pub(crate) const LINE_NAMES_MAX: usize = 32;
pub(crate) const AREAS_MAX: usize = 32;
const TRACK_MAX_DEPTH: i32 = 32;

pub(crate) const TRACK_PX: u32 = 0;
pub(crate) const TRACK_PERCENT: u32 = 1;
pub(crate) const TRACK_FR: u32 = 2;
pub(crate) const TRACK_AUTO: u32 = 3;
pub(crate) const TRACK_MIN_CONTENT: u32 = 4;
pub(crate) const TRACK_MAX_CONTENT: u32 = 5;

pub(crate) const AUTO_REPEAT_NONE: u32 = 0;
pub(crate) const AUTO_REPEAT_FIT: u32 = 1;
pub(crate) const AUTO_REPEAT_FILL: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Track {
    pub kind: u32,
    pub v: f64,
    pub em: f64,
    pub rem: f64,
    pub pct: f64,
    pub min_kind: u32,
    pub min_v: f64,
    pub min_em: f64,
    pub min_rem: f64,
    pub min_pct: f64,
    pub has_min: i32,
    pub fit_content: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct LineName {
    pub name: [u8; LINE_NAME_MAX],
    pub line: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Tracks {
    pub n: i32,
    pub tracks: [Track; TRACKS_MAX],
    pub auto_repeat: u32,
    pub auto_repeat_start: i32,
    pub auto_repeat_count: i32,
    pub auto_repeat_names_start: i32,
    pub auto_repeat_names_end: i32,
    pub subgrid: i32,
    pub n_line_names: i32,
    pub line_names: [LineName; LINE_NAMES_MAX],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<Track>() == 88
        && core::mem::size_of::<LineName>() == 28
        && core::mem::offset_of!(Tracks, auto_repeat) == 2120
        && core::mem::offset_of!(Tracks, line_names) == 2148
        && core::mem::size_of::<Tracks>() == 3048
);

impl Default for Tracks {
    fn default() -> Tracks {
        Tracks {
            n: 0,
            tracks: [Track::default(); TRACKS_MAX],
            auto_repeat: AUTO_REPEAT_NONE,
            auto_repeat_start: 0,
            auto_repeat_count: 0,
            auto_repeat_names_start: 0,
            auto_repeat_names_end: 0,
            subgrid: 0,
            n_line_names: 0,
            line_names: [LineName::default(); LINE_NAMES_MAX],
        }
    }
}

pub(crate) struct AreaRect {
    pub name: Vec<u8>,
    pub r0: i32,
    pub r1: i32,
    pub c0: i32,
    pub c1: i32,
}

pub(crate) struct Areas {
    pub rows: i32,
    pub cols: i32,
    pub rects: Vec<AreaRect>,
}

fn c_text(bytes: &[u8]) -> CString {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..len]).unwrap_or_default()
}

fn track_token(tok: &[u8]) -> Option<Track> {
    if tok.is_empty() {
        return None;
    }
    let mut out = Track::default();
    for (keyword, kind) in [
        (&b"auto"[..], TRACK_AUTO),
        (b"min-content", TRACK_MIN_CONTENT),
        (b"max-content", TRACK_MAX_CONTENT),
    ] {
        if tok.eq_ignore_ascii_case(keyword) {
            out.kind = kind;
            return Some(out);
        }
    }
    let text = c_text(tok);
    let (v, end) = crate::ffi::strtod(&text, 0);
    if end == 0 {
        return None;
    }
    if text.to_bytes()[end..].eq_ignore_ascii_case(b"fr") {
        if v < 0.0 {
            return None;
        }
        out.kind = TRACK_FR;
        out.v = v;
        return Some(out);
    }
    let (v, unit) = units::parse_length(&text)?;
    if v < 0.0 {
        return None;
    }
    out.kind = TRACK_PX;
    match unit {
        PERCENT => {
            out.kind = TRACK_PERCENT;
            out.v = v;
        }
        NUMBER | PX => out.v = v,
        EM | IC => out.em = v,
        REM => out.rem = v,
        LH | RLH => out.v = v * 19.2,
        EX | CH => out.v = v * 8.0,
        CAP => out.v = v * 11.2,
        CQW | CQH | CQMIN | CQMAX => out.v = container::unit_resolve(v, unit),
        _ => out.v = units::viewport_resolve(v, unit),
    }
    Some(out)
}

fn math_track(text: &[u8]) -> Option<Track> {
    let mut out = Track::default();
    match calc::parse_calc(&c_text(text))? {
        Parsed::Length(v, PX | NUMBER) => {
            out.kind = TRACK_PX;
            out.v = v;
        }
        Parsed::Calc(c) => {
            if c.pct != 0.0 && c.px == 0.0 && c.em == 0.0 && c.rem == 0.0 {
                out.kind = TRACK_PERCENT;
                out.v = c.pct;
            } else {
                out.kind = TRACK_PX;
                out.v = c.px;
                out.em = c.em;
                out.rem = c.rem;
                out.pct = c.pct;
            }
        }
        Parsed::Length(..) => return None,
    }
    Some(out)
}

fn trim_ws(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_ws(c)).unwrap_or(s.len());
    let end = s.iter().rposition(|&c| !is_ws(c)).map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

fn minmax_track(body: &[u8], depth: i32) -> Option<Track> {
    let start = skip_ws(body, 0, body.len());
    let (comma, term) = scan_until(body, start, body.len(), b",");
    if term != b',' {
        return None;
    }
    let first = trim_ws(&body[start..comma]);
    let second = trim_ws(&body[comma + 1..]);
    let min = one_track(first, depth)?;
    let max = one_track(second, depth)?;
    if min.kind == TRACK_FR {
        return (max.kind == TRACK_FR).then_some(max);
    }
    let mut out = max;
    out.min_kind = min.kind;
    out.min_v = min.v;
    out.min_em = min.em;
    out.min_rem = min.rem;
    out.min_pct = min.pct;
    out.has_min = 1;
    Some(out)
}

fn one_track(text: &[u8], depth: i32) -> Option<Track> {
    if depth >= TRACK_MAX_DEPTH {
        return None;
    }
    let text = trim_ws(text);
    let len = text.len();
    if len == 0 {
        return None;
    }
    let closed = text[len - 1] == b')';
    if len > 7 && starts_with_ci(text, b"minmax(") && closed {
        return minmax_track(&text[7..len - 1], depth + 1);
    }
    if len > 12 && starts_with_ci(text, b"fit-content(") && closed {
        let mut out = one_track(&text[12..len - 1], depth + 1)?;
        out.min_kind = TRACK_AUTO;
        out.min_v = 0.0;
        out.has_min = 1;
        out.fit_content = 1;
        return Some(out);
    }
    if closed
        && [&b"min("[..], b"max(", b"clamp(", b"calc("]
            .iter()
            .any(|name| starts_with_ci(text, name))
    {
        return math_track(text);
    }
    track_token(text)
}

fn line_name_is_custom_ident(name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut i = 0;
    if name[0] == b'-' {
        i = 1;
        if name.len() == 1 {
            return false;
        }
    }
    if name[i].is_ascii_digit() {
        return false;
    }
    if !name[i..]
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80)
    {
        return false;
    }
    ![
        &b"span"[..],
        b"auto",
        b"initial",
        b"inherit",
        b"unset",
        b"revert",
        b"revert-layer",
        b"default",
    ]
    .iter()
    .any(|reserved| name.eq_ignore_ascii_case(reserved))
}

fn add_names(tk: &mut Tracks, names: &[u8], line: i32) -> bool {
    if names.contains(&b',') {
        return false;
    }
    let mut q = 0;
    while q < names.len() {
        q = skip_ws(names, q, names.len());
        let start = q;
        while q < names.len() && !is_ws(names[q]) {
            q += 1;
        }
        let name = &names[start..q];
        if !name.is_empty() && !line_name_is_custom_ident(name) {
            return false;
        }
        if !name.is_empty()
            && name.len() < LINE_NAME_MAX
            && (tk.n_line_names as usize) < LINE_NAMES_MAX
        {
            let slot = &mut tk.line_names[tk.n_line_names as usize];
            tk.n_line_names += 1;
            slot.name[..name.len()].copy_from_slice(name);
            slot.name[name.len()] = 0;
            slot.line = line;
        }
    }
    true
}

fn track_is_fixed_size(t: &Track) -> bool {
    let main = t.kind == TRACK_PX || t.kind == TRACK_PERCENT;
    if t.has_min == 0 {
        return main;
    }
    main || t.min_kind == TRACK_PX || t.min_kind == TRACK_PERCENT
}

fn append_repeat_body(tk: &mut Tracks, body: &[u8], repeats: i64) -> Option<i32> {
    let mut tokens: Vec<&[u8]> = Vec::new();
    let end = body.len();
    let mut p = 0;
    while p < end && tokens.len() < 32 {
        p = skip_ws(body, p, end);
        if p >= end {
            break;
        }
        let start = p;
        if body[p] == b'[' {
            while p < end && body[p] != b']' {
                p += 1;
            }
            if p < end {
                p += 1;
            }
        } else {
            p = scan_until(body, p, end, b" \t\n\r\x0c,[").0;
        }
        tokens.push(&body[start..p]);
        if p < end && body[p] == b',' {
            p += 1;
        }
    }
    let per = tokens.iter().filter(|t| t.first() != Some(&b'[')).count() as i32;
    if per == 0 {
        return None;
    }
    for _ in 0..repeats {
        let mut last_names = false;
        for tok in &tokens {
            if tok.first() == Some(&b'[') {
                if last_names || tok.len() < 2 {
                    return None;
                }
                let line = tk.n + 1;
                if !add_names(tk, &tok[1..tok.len() - 1], line) {
                    return None;
                }
                last_names = true;
                continue;
            }
            last_names = false;
            if tk.n as usize >= TRACKS_MAX {
                return Some(per);
            }
            let track = one_track(tok, 0)?;
            tk.tracks[tk.n as usize] = track;
            tk.n += 1;
        }
    }
    Some(per)
}

pub(crate) fn parse_tracks(text: &[u8]) -> Option<Tracks> {
    let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
    if text.is_empty() {
        return None;
    }
    let mut tk = Tracks::default();
    let end = text.len();
    let mut p = skip_ws(text, 0, end);
    if starts_with_ci(&text[p..], b"subgrid")
        && (p + 7 >= end || is_ws(text[p + 7]) || text[p + 7] == b'[')
    {
        tk.subgrid = 1;
        return Some(tk);
    }
    let mut last_was_names = false;
    while p < end && (tk.n as usize) < TRACKS_MAX {
        p = skip_ws(text, p, end);
        if p >= end {
            break;
        }
        if text[p] == b'[' {
            p += 1;
            let names_start = p;
            while p < end && text[p] != b']' {
                p += 1;
            }
            let names = &text[names_start..p];
            if p >= end || last_was_names || names.contains(&b',') {
                return None;
            }
            last_was_names = true;
            p += 1;
            let line = tk.n + 1;
            if !add_names(&mut tk, names, line) {
                return None;
            }
            continue;
        }
        last_was_names = false;
        if starts_with_ci(&text[p..], b"repeat(") {
            p = skip_ws(text, p + 7, end);
            let count_start = p;
            while p < end && text[p] != b',' && text[p] != b')' {
                p += 1;
            }
            let mut count_end = p;
            while count_end > count_start && is_ws(text[count_end - 1]) {
                count_end -= 1;
            }
            let count = &text[count_start..count_end];
            if p < end && text[p] == b',' {
                p += 1;
            }
            let body_start = p;
            let mut depth = 1;
            while p < end && depth > 0 {
                if text[p] == b'(' {
                    depth += 1;
                } else if text[p] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                p += 1;
            }
            let body = &text[body_start..p];
            if p < end && text[p] == b')' {
                p += 1;
            }
            let auto_repeat = if count.eq_ignore_ascii_case(b"auto-fit") {
                AUTO_REPEAT_FIT
            } else if count.eq_ignore_ascii_case(b"auto-fill") {
                AUTO_REPEAT_FILL
            } else {
                AUTO_REPEAT_NONE
            };
            if auto_repeat != AUTO_REPEAT_NONE {
                if tk.auto_repeat != AUTO_REPEAT_NONE {
                    return None;
                }
                tk.auto_repeat = auto_repeat;
                tk.auto_repeat_start = tk.n;
                tk.auto_repeat_names_start = tk.n_line_names;
                let mut count = append_repeat_body(&mut tk, body, 1)?;
                tk.auto_repeat_names_end = tk.n_line_names;
                for k in tk.auto_repeat_start..tk.n {
                    if !track_is_fixed_size(&tk.tracks[k as usize]) {
                        return None;
                    }
                }
                if count > tk.n - tk.auto_repeat_start {
                    count = tk.n - tk.auto_repeat_start;
                }
                tk.auto_repeat_count = count;
                continue;
            }
            let (mut n, _) = strtol10(count);
            if n <= 0 {
                continue;
            }
            if n > TRACKS_MAX as i64 {
                n = TRACKS_MAX as i64;
            }
            append_repeat_body(&mut tk, body, n)?;
            continue;
        }
        let token_start = skip_ws(text, p, end);
        if token_start >= end {
            break;
        }
        let token_end = scan_until(text, token_start, end, b" \t\n\r\x0c,").0;
        let after = skip_ws(text, token_end, end);
        if after < end && text[after] == b',' {
            return None;
        }
        let track = one_track(&text[token_start..token_end], 0)?;
        tk.tracks[tk.n as usize] = track;
        tk.n += 1;
        let mut next = skip_ws(text, token_end, end);
        if next < end && text[next] == b',' {
            next += 1;
        }
        if next <= p {
            next = p + 1;
        }
        p = next;
    }
    if tk.n == 0 {
        return None;
    }
    if tk.auto_repeat != AUTO_REPEAT_NONE {
        let start = tk.auto_repeat_start;
        let stop = start + tk.auto_repeat_count;
        for k in 0..tk.n {
            if k >= start && k < stop {
                continue;
            }
            if !track_is_fixed_size(&tk.tracks[k as usize]) {
                return None;
            }
        }
    }
    Some(tk)
}

pub(crate) fn parse_areas(text: &[u8]) -> Option<Areas> {
    let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
    if text.is_empty() {
        return None;
    }
    let mut grid: Vec<Vec<Vec<u8>>> = Vec::new();
    let mut cols: i32 = -1;
    let mut p = 0;
    while p < text.len() && grid.len() < TRACKS_MAX {
        p = skip_ws(text, p, text.len());
        if p >= text.len() {
            break;
        }
        if text[p] != b'"' && text[p] != b'\'' {
            return None;
        }
        let row_start = p;
        let row = read_string(text, &mut p, text.len());
        if p == row_start {
            return None;
        }
        let row = &row[..row.iter().position(|&c| c == 0).unwrap_or(row.len())];
        let cells: Vec<Vec<u8>> = row
            .split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n'))
            .filter(|cell| !cell.is_empty())
            .take(TRACKS_MAX)
            .map(<[u8]>::to_vec)
            .collect();
        let count = cells.len() as i32;
        if cols < 0 {
            cols = count;
        } else if count != cols {
            return None;
        }
        grid.push(cells);
    }
    let rows = grid.len();
    if rows == 0 || cols <= 0 {
        return None;
    }
    let cols = cols as usize;
    let mut used = vec![vec![false; cols]; rows];
    let mut rects = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            if used[r][c] {
                continue;
            }
            let name = &grid[r][c];
            if name == b"." {
                used[r][c] = true;
                continue;
            }
            let mut c1 = c;
            while c1 + 1 < cols && &grid[r][c1 + 1] == name {
                c1 += 1;
            }
            let mut r1 = r;
            while r1 + 1 < rows && (c..=c1).all(|k| &grid[r1 + 1][k] == name) {
                r1 += 1;
            }
            if rects.len() < AREAS_MAX {
                rects.push(AreaRect {
                    name: name.clone(),
                    r0: r as i32,
                    r1: r1 as i32,
                    c0: c as i32,
                    c1: c1 as i32,
                });
            }
            for row in used.iter_mut().take(r1 + 1).skip(r) {
                for cell in row.iter_mut().take(c1 + 1).skip(c) {
                    *cell = true;
                }
            }
        }
    }
    Some(Areas {
        rows: rows as i32,
        cols: cols as i32,
        rects,
    })
}

fn custom_ident_canonical(tok: &[u8]) -> Option<Vec<u8>> {
    let first = *tok.first()?;
    let second = tok.get(1).copied().unwrap_or(0);
    if first.is_ascii_digit() || first == b'+' || first == b'.' {
        return None;
    }
    if first == b'-' && (second.is_ascii_digit() || second == b'.') {
        return None;
    }
    let mut p = 0;
    let decoded = read_ident(tok, &mut p, tok.len());
    let escaped = tok.contains(&b'\\');
    if p != tok.len()
        || decoded.is_empty()
        || decoded[0] == 0
        || (decoded[0] == b'-' && decoded.get(1).copied().unwrap_or(0) == 0 && !escaped)
    {
        return None;
    }
    let decoded_text = &decoded[..decoded
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(decoded.len())];
    if [
        &b"auto"[..],
        b"span",
        b"inherit",
        b"initial",
        b"unset",
        b"revert",
        b"revert-layer",
        b"default",
    ]
    .iter()
    .any(|reserved| decoded_text.eq_ignore_ascii_case(reserved))
    {
        return None;
    }
    if !escaped {
        return Some(tok.to_vec());
    }
    Some(ident_serialize(&decoded))
}

fn integer_canonical(tok: &[u8]) -> Option<(Vec<u8>, bool, i64)> {
    let digits = match tok.first() {
        Some(b'+' | b'-') => &tok[1..],
        _ => tok,
    };
    if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) {
        let (value, _) = strtol10(tok);
        return Some((value.to_string().into_bytes(), true, value));
    }
    if !is_math_fn_start(tok) {
        return None;
    }
    if !matches!(
        calc::parse_calc(&c_text(tok)),
        Some(Parsed::Length(_, NUMBER))
    ) {
        return None;
    }
    let canon = math::math_canonical(&c_text(tok)).unwrap_or_else(|| tok.to_vec());
    Some((canon, false, 0))
}

fn line_tokens(text: &[u8], max: usize) -> Option<Vec<&[u8]>> {
    let mut out = Vec::new();
    let end = text.len();
    let mut p = 0;
    while p < end {
        p = skip_ws(text, p, end);
        if p >= end {
            break;
        }
        if out.len() == max {
            return None;
        }
        let start = p;
        let mut depth = 0;
        while p < end && (depth > 0 || !is_ws(text[p])) {
            if text[p] == b'\\' && p + 1 < end {
                p += 1;
                if text[p].is_ascii_hexdigit() {
                    let mut k = 0;
                    while k < 6 && p < end && text[p].is_ascii_hexdigit() {
                        p += 1;
                        k += 1;
                    }
                    if p < end && is_ws(text[p]) {
                        p += 1;
                    }
                } else {
                    p += 1;
                }
                continue;
            }
            if text[p] == b'(' {
                depth += 1;
            } else if text[p] == b')' && depth > 0 {
                depth -= 1;
            }
            p += 1;
        }
        out.push(&text[start..p]);
    }
    Some(out)
}

pub(crate) fn line_canonical(text: &[u8]) -> Option<(Vec<u8>, bool)> {
    let tok = line_tokens(text, 3)?;
    let n = tok.len();
    if !(1..=3).contains(&n) {
        return None;
    }
    if n == 1 && tok[0].eq_ignore_ascii_case(b"auto") {
        return Some((b"auto".to_vec(), false));
    }
    let (mut span_at, mut int_at, mut ident_at) = (None, None, None);
    let mut int_text: Option<Vec<u8>> = None;
    let mut ident: Option<Vec<u8>> = None;
    let mut literal = false;
    let mut int_value = 0i64;
    for (i, t) in tok.iter().enumerate() {
        if t.eq_ignore_ascii_case(b"span") {
            if span_at.is_some() {
                return None;
            }
            span_at = Some(i);
        } else if let Some((canon, tok_literal, value)) = integer_canonical(t) {
            if int_at.is_some() {
                return None;
            }
            int_at = Some(i);
            literal = tok_literal;
            int_value = value;
            int_text = Some(canon);
        } else if let Some(canon) = custom_ident_canonical(t) {
            if ident_at.is_some() {
                return None;
            }
            ident_at = Some(i);
            ident = Some(canon);
        } else {
            return None;
        }
    }
    if int_at.is_none() && ident_at.is_none() {
        return None;
    }
    if let Some(span_at) = span_at {
        if span_at != 0 && span_at != n - 1 {
            return None;
        }
        if literal && int_value < 1 {
            return None;
        }
        let mut out = b"span".to_vec();
        if let Some(int_text) = &int_text {
            if !(literal && int_value == 1 && ident.is_some()) {
                out.push(b' ');
                out.extend_from_slice(int_text);
            }
        }
        if let Some(ident) = &ident {
            out.push(b' ');
            out.extend_from_slice(ident);
        }
        return Some((out, false));
    }
    if literal && int_value == 0 {
        return None;
    }
    let ident_only = int_text.is_none();
    let text = match (int_text, ident) {
        (Some(int_text), Some(ident)) => {
            let mut out = int_text;
            out.push(b' ');
            out.extend_from_slice(&ident);
            out
        }
        (Some(int_text), None) => int_text,
        (None, ident) => ident.unwrap_or_default(),
    };
    Some((text, ident_only))
}

fn placement_parts(text: &[u8], max_parts: usize) -> Option<Vec<(Vec<u8>, bool)>> {
    let end = text.len();
    let mut parts = Vec::new();
    let mut scan = 0;
    loop {
        let (slash_at, term) = scan_until(text, scan, end, b"/");
        let slash = (term == b'/').then_some(slash_at);
        if parts.len() >= max_parts {
            return None;
        }
        let part = trim_range(text, scan, slash.unwrap_or(end));
        if part.is_empty() {
            return None;
        }
        parts.push(line_canonical(part)?);
        match slash {
            Some(slash) => scan = slash + 1,
            None => break,
        }
    }
    Some(parts)
}

pub(crate) fn placement_expand(text: &[u8], area: bool) -> Option<Vec<(Vec<u8>, bool)>> {
    let max = if area { 4 } else { 2 };
    let mut parts = placement_parts(text, max)?;
    let fill = |parts: &mut Vec<(Vec<u8>, bool)>, index: usize, from: usize| {
        if index < parts.len() {
            return;
        }
        let (text, ident_only) = &parts[from];
        let value = if *ident_only {
            text.clone()
        } else {
            b"auto".to_vec()
        };
        let ident_only = *ident_only;
        parts.push((value, ident_only));
    };
    fill(&mut parts, 1, 0);
    if area {
        fill(&mut parts, 2, 0);
        fill(&mut parts, 3, 1);
    }
    Some(parts)
}

fn line_is_default_for(full: &[(Vec<u8>, bool)], index: usize, from: usize) -> bool {
    let expected: &[u8] = if full[from].1 { &full[from].0 } else { b"auto" };
    full[index].0 == expected
}

pub(crate) fn placement_canonical(text: &[u8], area: bool) -> Option<Vec<u8>> {
    let full = placement_expand(text, area)?;
    let mut keep = full.len();
    if area {
        if line_is_default_for(&full, 3, 1) {
            keep = 3;
            if line_is_default_for(&full, 2, 0) {
                keep = 2;
                if line_is_default_for(&full, 1, 0) {
                    keep = 1;
                }
            }
        }
    } else if line_is_default_for(&full, 1, 0) {
        keep = 1;
    }
    let parts: Vec<&[u8]> = full[..keep]
        .iter()
        .map(|(text, _)| text.as_slice())
        .collect();
    Some(parts.join(&b" / "[..]))
}

#[derive(Clone, Copy, PartialEq)]
enum TokKind {
    Other,
    String,
    Names,
    Slash,
}

struct GridTok<'a> {
    kind: TokKind,
    text: &'a [u8],
}

fn tokens(text: &[u8]) -> Option<Vec<GridTok<'_>>> {
    let end = text.len();
    let mut out = Vec::new();
    let mut p = 0;
    loop {
        p = skip_ws(text, p, end);
        if p >= end {
            return Some(out);
        }
        let start = p;
        let kind = match text[p] {
            quote @ (b'"' | b'\'') => {
                p += 1;
                while p < end && text[p] != quote {
                    if text[p] == b'\\' && p + 1 < end {
                        p += 1;
                    }
                    p += 1;
                }
                if p >= end {
                    return None;
                }
                p += 1;
                TokKind::String
            }
            b'[' => {
                while p < end && text[p] != b']' {
                    p += 1;
                }
                if p >= end {
                    return None;
                }
                p += 1;
                TokKind::Names
            }
            b'/' => {
                p += 1;
                TokKind::Slash
            }
            _ => {
                let mut depth = 0i32;
                while p < end {
                    let c = text[p];
                    if c == b'(' {
                        depth += 1;
                    } else if c == b')' {
                        depth -= 1;
                        if depth < 0 {
                            break;
                        }
                    } else if depth == 0
                        && (is_ws(c) || c == b'/' || c == b'[' || c == b'"' || c == b'\'')
                    {
                        break;
                    }
                    p += 1;
                }
                if depth != 0 {
                    return None;
                }
                TokKind::Other
            }
        };
        out.push(GridTok {
            kind,
            text: &text[start..p],
        });
    }
}

fn names_append(acc: &mut Vec<u8>, tok: &[u8]) -> bool {
    if tok.len() < 2 {
        return true;
    }
    let inner = &tok[1..tok.len() - 1];
    let mut q = 0;
    while q < inner.len() {
        q = skip_ws(inner, q, inner.len());
        let start = q;
        while q < inner.len() && !is_ws(inner[q]) {
            q += 1;
        }
        let name = &inner[start..q];
        if name.is_empty() {
            break;
        }
        if !line_name_is_custom_ident(name) {
            return false;
        }
        if !acc.is_empty() {
            acc.push(b' ');
        }
        acc.extend_from_slice(name);
    }
    true
}

fn track_token_canonical(tok: &[u8]) -> Vec<u8> {
    if let Some((v, NUMBER)) = units::parse_length(&c_text(tok)) {
        if v == 0.0 {
            return b"0px".to_vec();
        }
    }
    for keyword in [
        &b"auto"[..],
        b"min-content",
        b"max-content",
        b"none",
        b"subgrid",
    ] {
        if tok.eq_ignore_ascii_case(keyword) {
            return keyword.to_vec();
        }
    }
    tok.to_vec()
}

fn parts_append(out: &mut Vec<u8>, part: &[u8]) {
    if !out.is_empty() {
        out.push(b' ');
    }
    out.extend_from_slice(part);
}

fn track_tokens_canonical(toks: &[GridTok<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for t in toks {
        if t.kind == TokKind::Names {
            let mut names = Vec::new();
            if names_append(&mut names, t.text) && !names.is_empty() {
                if !out.is_empty() {
                    out.push(b' ');
                }
                out.push(b'[');
                out.extend_from_slice(&names);
                out.push(b']');
            }
            continue;
        }
        parts_append(&mut out, &track_token_canonical(t.text));
    }
    out
}

fn name_group_append(out: &mut Vec<u8>, tok: &[u8]) -> bool {
    let mut names = Vec::new();
    if !names_append(&mut names, tok) {
        return false;
    }
    out.extend_from_slice(b" [");
    out.extend_from_slice(&names);
    out.push(b']');
    true
}

fn name_repeat_append(out: &mut Vec<u8>, tok: &[u8], auto_fill: &mut bool) -> bool {
    let len = tok.len();
    if len < 9 || !starts_with_ci(tok, b"repeat(") || tok[len - 1] != b')' {
        return false;
    }
    let body = &tok[7..len - 1];
    let Some(comma) = body.iter().position(|&c| c == b',') else {
        return false;
    };
    let count = trim_range(body, 0, comma);
    let names = &body[comma + 1..];
    let mut group = b"repeat(".to_vec();
    let ok = if count.eq_ignore_ascii_case(b"auto-fill") {
        let ok = !*auto_fill;
        *auto_fill = true;
        group.extend_from_slice(b"auto-fill,");
        ok
    } else {
        let (n, end) = strtol10(count);
        let ok = !count.is_empty() && count[0].is_ascii_digit() && end == count.len() && n >= 1;
        group.extend_from_slice(format!("{n},").as_bytes());
        ok
    };
    if !ok {
        return false;
    }
    let Some(toks) = tokens(names) else {
        return false;
    };
    if toks.is_empty() {
        return false;
    }
    for t in &toks {
        if t.kind != TokKind::Names || !name_group_append(&mut group, t.text) {
            return false;
        }
    }
    out.push(b' ');
    out.extend_from_slice(&group);
    out.push(b')');
    true
}

fn subgrid_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let toks = tokens(text)?;
    let first = toks.first()?;
    if first.kind != TokKind::Other || !first.text.eq_ignore_ascii_case(b"subgrid") {
        return None;
    }
    let mut auto_fill = false;
    let mut out = b"subgrid".to_vec();
    for t in &toks[1..] {
        let ok = if t.kind == TokKind::Names {
            name_group_append(&mut out, t.text)
        } else {
            t.kind == TokKind::Other && name_repeat_append(&mut out, t.text, &mut auto_fill)
        };
        if !ok {
            return None;
        }
    }
    Some(out)
}

pub(crate) fn track_text_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let text = &text[skip_ws(text, 0, text.len())..];
    if starts_with_ci(text, b"subgrid") && (text.len() == 7 || is_ws(text[7]) || text[7] == b'[') {
        return subgrid_canonical(text);
    }
    let toks = tokens(text)?;
    Some(track_tokens_canonical(&toks))
}

#[derive(PartialEq)]
enum TrackValue {
    None,
    Tracks { subgrid: bool, auto_repeat: u32 },
    Other,
}

const CSS_WIDE: &[&[u8]] = &[
    b"inherit",
    b"initial",
    b"unset",
    b"revert",
    b"revert-layer",
    b"revert-rule",
];

fn track_property_value(text: &[u8], auto_tracks: bool) -> Option<TrackValue> {
    let t = trim_ws(text);
    let lower = t.to_ascii_lowercase();
    if CSS_WIDE.contains(&lower.as_slice()) {
        return Some(TrackValue::Other);
    }
    let mut parsed = parse_tracks(t);
    if parsed.is_some_and(|tk| tk.subgrid != 0 && auto_tracks) {
        parsed = None;
    }
    if let Some(tk) = parsed {
        let specified = track_text_canonical(t);
        if !(tk.subgrid != 0 && specified.is_none()) {
            return Some(TrackValue::Tracks {
                subgrid: tk.subgrid != 0,
                auto_repeat: tk.auto_repeat,
            });
        }
    }
    (lower == b"none").then_some(TrackValue::None)
}

fn track_list_canonical(
    toks: &[GridTok<'_>],
    auto_tracks: bool,
    explicit_only: bool,
) -> Option<Vec<u8>> {
    if toks.is_empty() {
        return None;
    }
    let mut raw = Vec::new();
    for t in toks {
        if t.kind == TokKind::String || t.kind == TokKind::Slash {
            return None;
        }
        parts_append(&mut raw, t.text);
    }
    let ok = match track_property_value(&raw, auto_tracks)? {
        TrackValue::None => !explicit_only,
        TrackValue::Tracks {
            subgrid,
            auto_repeat,
        } => !explicit_only || (!subgrid && auto_repeat == AUTO_REPEAT_NONE),
        TrackValue::Other => false,
    };
    ok.then(|| track_tokens_canonical(toks))
}

fn area_string_canonical(tok: &[u8]) -> Vec<u8> {
    let mut p = 0;
    let body = read_string(tok, &mut p, tok.len());
    let body = &body[..body.iter().position(|&c| c == 0).unwrap_or(body.len())];
    let cells: Vec<&[u8]> = body
        .split(|&c| matches!(c, b' ' | b'\t' | b'\r' | b'\n' | 0x0c))
        .filter(|cell| !cell.is_empty())
        .collect();
    let mut out = b"\"".to_vec();
    out.extend_from_slice(&cells.join(&b' '));
    out.push(b'"');
    out
}

fn track_size_valid(tok: &[u8]) -> bool {
    !starts_with_ci(tok, b"repeat(") && one_track(tok, 0).is_some()
}

fn flush_names(names: &mut Vec<u8>, rows: &mut Vec<u8>, canon: &mut Vec<u8>) {
    if names.is_empty() {
        return;
    }
    let mut group = b"[".to_vec();
    group.extend_from_slice(names);
    group.push(b']');
    parts_append(rows, &group);
    parts_append(canon, &group);
    names.clear();
}

fn template_areas_form(
    t: &[GridTok<'_>],
    rows: &mut Vec<u8>,
    canon: &mut Vec<u8>,
    areas: &mut Vec<u8>,
) -> bool {
    let n = t.len();
    let mut names = Vec::new();
    let mut i = 0;
    while i < n {
        if t[i].kind == TokKind::Names {
            let ok = names_append(&mut names, t[i].text);
            i += 1;
            if !ok {
                return false;
            }
        }
        if i >= n || t[i].kind != TokKind::String {
            return false;
        }
        let row = area_string_canonical(t[i].text);
        i += 1;
        let mut size = None;
        if i < n && t[i].kind == TokKind::Other {
            if !track_size_valid(t[i].text) {
                return false;
            }
            size = Some(track_token_canonical(t[i].text));
            i += 1;
        }
        flush_names(&mut names, rows, canon);
        parts_append(canon, &row);
        parts_append(areas, &row);
        parts_append(rows, size.as_deref().unwrap_or(b"auto"));
        if let Some(size) = &size {
            if size != b"auto" {
                parts_append(canon, size);
            }
        }
        if i < n && t[i].kind == TokKind::Names {
            let ok = names_append(&mut names, t[i].text);
            i += 1;
            if !ok {
                return false;
            }
        }
    }
    flush_names(&mut names, rows, canon);
    parse_areas(areas).is_some()
}

fn slash_index(t: &[GridTok<'_>]) -> Result<Option<usize>, ()> {
    let mut at = None;
    for (i, tok) in t.iter().enumerate() {
        if tok.kind != TokKind::Slash {
            continue;
        }
        if at.is_some() {
            return Err(());
        }
        at = Some(i);
    }
    Ok(at)
}

pub(crate) struct Template {
    pub rows: Vec<u8>,
    pub cols: Vec<u8>,
    pub areas: Vec<u8>,
    pub canon: Vec<u8>,
}

fn template_tokens_parse(t: &[GridTok<'_>]) -> Option<Template> {
    let n = t.len();
    if n == 1 && t[0].kind == TokKind::Other && t[0].text.eq_ignore_ascii_case(b"none") {
        return Some(Template {
            rows: b"none".to_vec(),
            cols: b"none".to_vec(),
            areas: b"none".to_vec(),
            canon: b"none".to_vec(),
        });
    }
    let slash = slash_index(t).ok()?;
    let has_string = t.iter().any(|tok| tok.kind == TokKind::String);
    if !has_string {
        let slash = slash?;
        let rows = track_list_canonical(&t[..slash], false, false)?;
        let cols = track_list_canonical(&t[slash + 1..], false, false)?;
        let canon = if rows == b"none" && cols == b"none" {
            b"none".to_vec()
        } else {
            let mut canon = rows.clone();
            canon.extend_from_slice(b" / ");
            canon.extend_from_slice(&cols);
            canon
        };
        return Some(Template {
            rows,
            cols,
            areas: b"none".to_vec(),
            canon,
        });
    }
    let row_end = slash.unwrap_or(n);
    let cols = match slash {
        Some(slash) => Some(track_list_canonical(&t[slash + 1..], false, true)?),
        None => None,
    };
    let mut rows = Vec::new();
    let mut canon = Vec::new();
    let mut areas = Vec::new();
    if !template_areas_form(&t[..row_end], &mut rows, &mut canon, &mut areas) {
        return None;
    }
    if let Some(cols) = &cols {
        canon.extend_from_slice(b" / ");
        canon.extend_from_slice(cols);
    }
    Some(Template {
        rows,
        cols: cols.unwrap_or_else(|| b"none".to_vec()),
        areas,
        canon,
    })
}

pub(crate) fn template_parse(text: &[u8]) -> Option<Template> {
    let toks = tokens(text)?;
    template_tokens_parse(&toks)
}

fn auto_flow_prefix(t: &[GridTok<'_>]) -> (usize, bool) {
    let mut dense = false;
    let mut flow = false;
    let mut i = 0;
    while i < t.len() && i < 2 && t[i].kind == TokKind::Other {
        if !flow && t[i].text.eq_ignore_ascii_case(b"auto-flow") {
            flow = true;
        } else if !dense && t[i].text.eq_ignore_ascii_case(b"dense") {
            dense = true;
        } else {
            break;
        }
        i += 1;
    }
    (if flow { i } else { 0 }, dense)
}

fn auto_tracks_canonical(t: &[GridTok<'_>]) -> Option<Vec<u8>> {
    if t.is_empty() {
        return Some(b"auto".to_vec());
    }
    if t.iter()
        .any(|tok| tok.kind != TokKind::Other || !track_size_valid(tok.text))
    {
        return None;
    }
    track_list_canonical(t, true, true)
}

pub(crate) struct Shorthand {
    pub values: [Vec<u8>; 6],
    pub canon: Vec<u8>,
}

pub(crate) fn shorthand_parse(text: &[u8]) -> Option<Shorthand> {
    let t = tokens(text)?;
    let n = t.len();
    let slash = slash_index(&t);
    let (left_flow, left_dense) = match slash {
        Ok(Some(slash)) if slash > 0 => auto_flow_prefix(&t[..slash]),
        _ => (0, false),
    };
    let (right_flow, right_dense) = match slash {
        Ok(Some(slash)) => auto_flow_prefix(&t[slash + 1..]),
        _ => (0, false),
    };
    let owned = |text: &[u8]| text.to_vec();
    match slash {
        Err(()) => None,
        Ok(None) => template_shorthand(&t),
        Ok(Some(_)) if left_flow == 0 && right_flow == 0 => template_shorthand(&t),
        Ok(Some(slash)) if left_flow > 0 && right_flow == 0 => {
            let auto_rows = auto_tracks_canonical(&t[left_flow..slash])?;
            let cols = track_list_canonical(&t[slash + 1..], false, false)?;
            let mut canon = b"auto-flow".to_vec();
            if left_dense {
                canon.extend_from_slice(b" dense");
            }
            if slash > left_flow {
                canon.push(b' ');
                canon.extend_from_slice(&auto_rows);
            }
            canon.extend_from_slice(b" / ");
            canon.extend_from_slice(&cols);
            Some(Shorthand {
                values: [
                    owned(b"none"),
                    cols,
                    owned(b"none"),
                    owned(if left_dense { b"row dense" } else { b"row" }),
                    auto_rows,
                    owned(b"auto"),
                ],
                canon,
            })
        }
        Ok(Some(slash)) if right_flow > 0 && left_flow == 0 => {
            let rows = track_list_canonical(&t[..slash], false, false)?;
            let rest = n - slash - 1 - right_flow;
            let auto_cols = auto_tracks_canonical(&t[slash + 1 + right_flow..])?;
            let mut canon = rows.clone();
            canon.extend_from_slice(b" / auto-flow");
            if right_dense {
                canon.extend_from_slice(b" dense");
            }
            if rest > 0 {
                canon.push(b' ');
                canon.extend_from_slice(&auto_cols);
            }
            Some(Shorthand {
                values: [
                    rows,
                    owned(b"none"),
                    owned(b"none"),
                    owned(if right_dense {
                        b"column dense"
                    } else {
                        b"column"
                    }),
                    owned(b"auto"),
                    auto_cols,
                ],
                canon,
            })
        }
        Ok(Some(_)) => None,
    }
}

fn template_shorthand(t: &[GridTok<'_>]) -> Option<Shorthand> {
    let template = template_tokens_parse(t)?;
    Some(Shorthand {
        values: [
            template.rows,
            template.cols,
            template.areas,
            b"row".to_vec(),
            b"auto".to_vec(),
            b"auto".to_vec(),
        ],
        canon: template.canon,
    })
}

pub(crate) fn auto_flow_canonical(text: &[u8]) -> Option<Vec<u8>> {
    let tok = split_ws_limit(text, 3);
    if !(1..=2).contains(&tok.len()) {
        return None;
    }
    let (mut row, mut column, mut dense) = (false, false, false);
    for t in tok {
        if t.eq_ignore_ascii_case(b"row") && !row && !column {
            row = true;
        } else if t.eq_ignore_ascii_case(b"column") && !row && !column {
            column = true;
        } else if t.eq_ignore_ascii_case(b"dense") && !dense {
            dense = true;
        } else {
            return None;
        }
    }
    Some(
        match (column, dense) {
            (true, true) => &b"column dense"[..],
            (true, false) => b"column",
            (false, true) => b"dense",
            (false, false) => b"row",
        }
        .to_vec(),
    )
}

pub(crate) fn template_compose(rows: &[u8], cols: &[u8], areas: &[u8]) -> Vec<u8> {
    if areas == b"none" {
        if rows == b"none" && cols == b"none" {
            return b"none".to_vec();
        }
        let mut out = rows.to_vec();
        out.extend_from_slice(b" / ");
        out.extend_from_slice(cols);
        return out;
    }
    let (Some(area_toks), Some(row_toks), Some(col_toks)) =
        (tokens(areas), tokens(rows), tokens(cols))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut row = 0;
    for t in &row_toks {
        if t.kind == TokKind::Names {
            parts_append(&mut out, t.text);
            continue;
        }
        if t.kind != TokKind::Other || row >= area_toks.len() || !track_size_valid(t.text) {
            return Vec::new();
        }
        parts_append(&mut out, area_toks[row].text);
        row += 1;
        if t.text != b"auto" {
            parts_append(&mut out, t.text);
        }
    }
    if row != area_toks.len() {
        return Vec::new();
    }
    if cols != b"none" {
        let Some(explicit_cols) = track_list_canonical(&col_toks, false, true) else {
            return Vec::new();
        };
        out.extend_from_slice(b" / ");
        out.extend_from_slice(&explicit_cols);
    }
    out
}

pub(crate) fn grid_compose(v: &[&[u8]; 6]) -> Vec<u8> {
    let [rows, cols, areas, flow, auto_rows, auto_cols] = *v;
    let dense = flow.windows(5).any(|w| w == b"dense");
    let column = flow.windows(6).any(|w| w == b"column");
    if auto_rows == b"auto" && auto_cols == b"auto" && flow == b"row" {
        return template_compose(rows, cols, areas);
    }
    if areas != b"none" {
        return Vec::new();
    }
    if !column && auto_cols == b"auto" && rows == b"none" {
        let mut out = b"auto-flow".to_vec();
        if dense {
            out.extend_from_slice(b" dense");
        }
        if auto_rows != b"auto" {
            out.push(b' ');
            out.extend_from_slice(auto_rows);
        }
        out.extend_from_slice(b" / ");
        out.extend_from_slice(cols);
        return out;
    }
    if column && auto_rows == b"auto" && cols == b"none" {
        let mut out = rows.to_vec();
        out.extend_from_slice(b" / auto-flow");
        if dense {
            out.extend_from_slice(b" dense");
        }
        if auto_cols != b"auto" {
            out.push(b' ');
            out.extend_from_slice(auto_cols);
        }
        return out;
    }
    Vec::new()
}

pub(crate) fn placement_compose(values: &[&[u8]], area: bool) -> Vec<u8> {
    let n = if area { 4 } else { 2 };
    if values.len() < n || values[..n].iter().any(|v| v.is_empty()) {
        return Vec::new();
    }
    let text = values[..n].join(&b" / "[..]);
    placement_canonical(&text, area).unwrap_or_default()
}
