//! Southstar — resolving grid-row and grid-column placements to line numbers through numbers, named lines and template areas.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_css::{GridAreas, GridLineName, GridTracks, TRACKS_MAX};
use southstar_style::{PropId, StyleRef, ValueRef};

use crate::text::{
    parse_span, skip_spaces, span_body, span_is_count, strip, strtol, trim_spaces_end,
};

#[derive(Clone, Copy)]
pub(crate) struct Lines<'a> {
    pub tracks: Option<&'a GridTracks>,
    pub areas: Option<&'a GridAreas>,
    pub row_axis: bool,
}

pub(crate) fn line_name(ln: &GridLineName) -> &[u8] {
    let len = ln
        .name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(ln.name.len());
    &ln.name[..len]
}

fn line_names(tracks: &GridTracks) -> &[GridLineName] {
    let n = usize::try_from(tracks.n_line_names).unwrap_or(0);
    &tracks.line_names[..n.min(tracks.line_names.len())]
}

struct AreaEdges<'a> {
    name: &'a [u8],
    start: i32,
    end: i32,
}

fn areas_on_axis<'a>(gl: &Lines<'a>) -> impl Iterator<Item = AreaEdges<'a>> {
    let rects = gl.areas.map_or(&[][..], |a| {
        &a.rects[..usize::try_from(a.n_rects).unwrap_or(0).min(a.rects.len())]
    });
    let row_axis = gl.row_axis;
    rects.iter().filter_map(move |a| {
        if a.name.is_null() {
            return None;
        }
        let name = unsafe { CStr::from_ptr(a.name) }.to_bytes();
        let (start, end) = if row_axis {
            (a.r0 + 1, a.r1 + 2)
        } else {
            (a.c0 + 1, a.c1 + 2)
        };
        Some(AreaEdges { name, start, end })
    })
}

fn line_name_with_suffix(tracks: Option<&GridTracks>, name: &[u8], suffix: &[u8]) -> i32 {
    let mut best = 0;
    for ln in tracks.map_or(&[][..], line_names) {
        let n = line_name(ln);
        if n.len() != name.len() + suffix.len() || !n.starts_with(name) || !n.ends_with(suffix) {
            continue;
        }
        if best == 0 || ln.line < best {
            best = ln.line;
        }
    }
    best
}

fn area_rect_line(gl: &Lines<'_>, name: &[u8], end_side: bool) -> i32 {
    let mut best = 0;
    for a in areas_on_axis(gl) {
        if a.name != name {
            continue;
        }
        let line = if end_side { a.end } else { a.start };
        if best == 0 || line < best {
            best = line;
        }
    }
    best
}

fn area_edge_line(gl: &Lines<'_>, name: &[u8], end_side: bool) -> i32 {
    let suffix: &[u8] = if end_side { b"-end" } else { b"-start" };
    let named = line_name_with_suffix(gl.tracks, name, suffix);
    let area = area_rect_line(gl, name, end_side);
    if named == 0 {
        return area;
    }
    if area == 0 {
        return named;
    }
    named.min(area)
}

fn named_line(gl: &Lines<'_>, name: &[u8], after: i32, end_side: bool) -> i32 {
    if name.is_empty() {
        return 0;
    }
    let edge = area_edge_line(gl, name, end_side);
    if edge > 0 {
        return edge;
    }
    if let Some(tracks) = gl.tracks {
        for ln in line_names(tracks) {
            if ln.line > after && line_name(ln) == name {
                return ln.line;
            }
        }
    }
    let len = name.len();
    if gl.areas.is_some() && len > 4 {
        let (base, want_end) = if len > 6 && name.ends_with(b"-start") {
            (len - 6, false)
        } else if name.ends_with(b"-end") {
            (len - 4, true)
        } else {
            (0, false)
        };
        if base > 0 {
            for a in areas_on_axis(gl) {
                if a.name != &name[..base] {
                    continue;
                }
                let line = if want_end { a.end } else { a.start };
                if line > after {
                    return line;
                }
            }
        }
    }
    for a in areas_on_axis(gl) {
        if a.name != name {
            continue;
        }
        if a.start > after {
            return a.start;
        }
        if a.end > after {
            return a.end;
        }
    }
    0
}

pub(crate) fn resolve_line_from(
    gl: &Lines<'_>,
    s: &[u8],
    n_tracks: i32,
    after: i32,
    end_side: bool,
) -> i32 {
    let s = skip_spaces(s);
    let Some((n, rest)) = strtol(s) else {
        return named_line(gl, trim_spaces_end(s), after, end_side).max(0);
    };
    if !skip_spaces(rest).is_empty() || n == 0 {
        return 0;
    }
    let n = if n < 0 {
        i64::from(n_tracks) + 2 + n
    } else {
        n
    };
    if n < 1 || n > TRACKS_MAX as i64 + 1 {
        return 0;
    }
    n as i32
}

fn resolve_line_number(gl: &Lines<'_>, s: &[u8], n_tracks: i32) -> i32 {
    resolve_line_from(gl, s, n_tracks, 0, false)
}

fn keyword(v: Option<ValueRef<'_>>) -> Option<&[u8]> {
    v.and_then(ValueRef::keyword_text).map(CStr::to_bytes)
}

fn start_span_count(a: &[u8]) -> i32 {
    let Some(body) = span_body(strip(a)) else {
        return 1;
    };
    if span_is_count(body) {
        parse_span(body)
    } else {
        -1
    }
}

fn pos_span_pair(
    gl: &Lines<'_>,
    s: &[u8],
    slash: usize,
    n_tracks: i32,
    start: &mut i32,
    span: &mut i32,
) -> bool {
    let a = &s[..slash];
    let b = skip_spaces(&s[slash + 1..]);
    let n = resolve_line_number(gl, a, n_tracks);
    *start = if n > 0 { n - 1 } else { 0 };
    let start_span = start_span_count(a);
    if let Some(body) = span_body(b) {
        *span = parse_span(body);
        return n > 0;
    }
    let e = resolve_line_from(gl, b, n_tracks, n.max(0), true);
    if n > 0 && e > n {
        *span = e - n;
    }
    if n <= 0 && start_span > 0 && e - start_span >= 1 {
        *start = e - start_span - 1;
        *span = start_span;
        return true;
    }
    n > 0
}

fn pos_span(
    gl: &Lines<'_>,
    v: Option<ValueRef<'_>>,
    n_tracks: i32,
    start: &mut i32,
    span: &mut i32,
) -> bool {
    *start = 0;
    *span = 1;
    let Some(s) = keyword(v) else {
        return false;
    };
    if let Some(body) = span_body(s) {
        *span = parse_span(body);
        return false;
    }
    if let Some(slash) = s.iter().position(|&c| c == b'/') {
        return pos_span_pair(gl, s, slash, n_tracks, start, span);
    }
    let n = resolve_line_number(gl, s, n_tracks);
    if n > 0 {
        *start = n - 1;
        return true;
    }
    false
}

fn line_num(
    gl: &Lines<'_>,
    v: Option<ValueRef<'_>>,
    n_tracks: i32,
    after: i32,
    end_side: bool,
) -> i32 {
    match keyword(v) {
        Some(s) if span_body(s).is_none() => resolve_line_from(gl, s, n_tracks, after, end_side),
        _ => 0,
    }
}

pub(crate) fn area_parts(st: StyleRef<'_>, row_axis: bool) -> (Option<&[u8]>, Option<&[u8]>) {
    let Some(kw) = keyword(st.get(PropId::GridArea)).filter(|kw| !kw.is_empty()) else {
        return (None, None);
    };
    let (si, ei) = if row_axis { (0, 2) } else { (1, 3) };
    let mut parts = kw.split(|&c| c == b'/');
    let mut start = None;
    let mut end = None;
    for i in 0..=ei {
        let Some(part) = parts.next() else {
            break;
        };
        if i == si {
            start = Some(part);
        } else if i == ei {
            end = Some(part);
        }
    }
    (start, end)
}

fn area_axis_pos(
    gl: &Lines<'_>,
    st: StyleRef<'_>,
    n_tracks: i32,
    start: &mut i32,
    span: &mut i32,
) -> bool {
    if keyword(st.get(PropId::GridArea)).is_none() {
        return false;
    }
    let (sstr, estr) = area_parts(st, gl.row_axis);
    let mut got = false;
    let ss = sstr.map(strip);
    if let Some(ss) = ss {
        let s = resolve_line_number(gl, ss, n_tracks);
        if let Some(body) = span_body(ss) {
            *span = parse_span(body);
        }
        if s > 0 {
            *start = s - 1;
            *span = 1;
            got = true;
        }
    }
    if let (None, true, Some(ss)) = (estr, got, ss)
        && strtol(ss).is_none()
    {
        let e = resolve_line_from(gl, ss, n_tracks, *start + 1, true);
        if e > *start + 1 {
            *span = e - (*start + 1);
        }
    }
    if let Some(es) = estr.map(strip) {
        if let Some(body) = span_body(es) {
            *span = parse_span(body);
        } else if got {
            let e = resolve_line_from(gl, es, n_tracks, *start + 1, true);
            if e > *start + 1 {
                *span = e - (*start + 1);
            }
        }
    }
    got
}

pub(crate) struct AxisProps {
    pub shorthand: PropId,
    pub start: PropId,
    pub end: PropId,
}

pub(crate) const COLUMN_PROPS: AxisProps = AxisProps {
    shorthand: PropId::GridColumn,
    start: PropId::GridColumnStart,
    end: PropId::GridColumnEnd,
};

pub(crate) const ROW_PROPS: AxisProps = AxisProps {
    shorthand: PropId::GridRow,
    start: PropId::GridRowStart,
    end: PropId::GridRowEnd,
};

pub(crate) fn resolve_pos(
    gl: &Lines<'_>,
    st: StyleRef<'_>,
    props: &AxisProps,
    n_tracks: i32,
    start: &mut i32,
    span: &mut i32,
) -> bool {
    if pos_span(gl, st.get(props.shorthand), n_tracks, start, span) {
        return true;
    }
    if area_axis_pos(gl, st, n_tracks, start, span) {
        return true;
    }
    let sv = st.get(props.start);
    let ev = st.get(props.end);
    let sl = line_num(gl, sv, n_tracks, 0, false);
    let el = line_num(gl, ev, n_tracks, sl, true);
    let ev_span = keyword(ev).and_then(span_body);
    if sl > 0 {
        *start = sl - 1;
        if el > sl {
            *span = el - sl;
        } else if let Some(body) = ev_span {
            *span = parse_span(body);
        }
        return true;
    }
    let sv_span = keyword(sv).and_then(span_body);
    if el > 0 && sv_span.is_none_or(span_is_count) {
        let item_span = sv_span.map_or(1, parse_span);
        if el - item_span >= 1 {
            *start = el - item_span - 1;
            *span = item_span;
            return true;
        }
    }
    if let Some(body) = sv_span {
        *span = parse_span(body);
        return false;
    }
    if let Some(body) = ev_span {
        *span = parse_span(body);
    }
    false
}

fn abs_line(gl: &Lines<'_>, tok: Option<&[u8]>, explicit_tracks: i32, end_side: bool) -> i32 {
    let Some(tok) = tok else {
        return 0;
    };
    let tok = skip_spaces(tok);
    if tok.is_empty() || tok == b"auto" || span_body(tok).is_some() {
        return 0;
    }
    let past_end = explicit_tracks + 2;
    if let Some((n, rest)) = strtol(tok) {
        if !skip_spaces(rest).is_empty() || n == 0 {
            return 0;
        }
        let n = if n < 0 {
            i64::from(explicit_tracks) + 2 + n
        } else {
            n
        };
        if n < 1 {
            return -1;
        }
        if n > i64::from(explicit_tracks) + 1 {
            return past_end;
        }
        return n as i32;
    }
    let named = resolve_line_from(gl, tok, explicit_tracks, 0, end_side);
    if named < 1 || named > explicit_tracks + 1 {
        return past_end;
    }
    named
}

pub(crate) fn abs_axis_lines(gl: &Lines<'_>, st: StyleRef<'_>, explicit_tracks: i32) -> (i32, i32) {
    let row_axis = gl.row_axis;
    let (mut start_tok, mut end_tok) = {
        let (s, e) = area_parts(st, row_axis);
        (s.map(strip), e.map(strip))
    };
    let shorthand = if row_axis {
        PropId::GridRow
    } else {
        PropId::GridColumn
    };
    if let Some(kw) = keyword(st.get(shorthand)) {
        match kw.iter().position(|&c| c == b'/') {
            Some(slash) => {
                start_tok = Some(strip(&kw[..slash]));
                end_tok = Some(strip(&kw[slash + 1..]));
            }
            None => {
                start_tok = Some(strip(kw));
                end_tok = None;
            }
        }
    }
    let (sp, ep) = if row_axis {
        (PropId::GridRowStart, PropId::GridRowEnd)
    } else {
        (PropId::GridColumnStart, PropId::GridColumnEnd)
    };
    if let Some(kw) = keyword(st.get(sp)) {
        start_tok = Some(kw);
    }
    if let Some(kw) = keyword(st.get(ep)) {
        end_tok = Some(kw);
    }
    let mut s0 = abs_line(gl, start_tok, explicit_tracks, false);
    let mut e0 = abs_line(gl, end_tok, explicit_tracks, true);
    if s0 != 0 && e0 != 0 && s0 > e0 {
        core::mem::swap(&mut s0, &mut e0);
    }
    if s0 != 0 && e0 != 0 && s0 == e0 {
        e0 = 0;
    }
    if s0 == -1 || s0 >= explicit_tracks + 2 {
        s0 = 0;
    }
    if e0 == -1 || e0 >= explicit_tracks + 2 {
        e0 = 0;
    }
    (s0, e0)
}
