//! Southstar — absolutely positioned boxes in a grid: the grid area that contains them, their static position in it, and the resolved track list getComputedStyle reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_css::{AUTO_REPEAT_NONE, GridTracks};
use southstar_layout::{BoxRef, children};
use southstar_style::{PropId, StyleRef, ValueRef, display_of};

use crate::ffi::{self, TrackEdges, style_of};
use crate::intrinsic::is_absolute_or_fixed;
use crate::lines::{Lines, abs_axis_lines, line_name};
use crate::text::{fmax, fmin};
use crate::tracks::expand_repeat_names;

pub(crate) struct Area {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn template<'a>(s: Option<StyleRef<'a>>, columns: bool) -> Option<ValueRef<'a>> {
    let prop = if columns {
        PropId::GridTemplateColumns
    } else {
        PropId::GridTemplateRows
    };
    s.and_then(|s| s.get(prop))
}

fn is_rtl(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::Direction))
        .is_some_and(|v| v.is_keyword(c"rtl"))
}

pub(crate) fn containing_block(cb: BoxRef<'_>, st: StyleRef<'_>) -> Option<Area> {
    let ct = ffi::track_edges(cb.grid_tracks(true))?;
    let rt = ffi::track_edges(cb.grid_tracks(false))?;
    let cs = style_of(cb);
    let col_lines = Lines {
        tracks: template(cs, true).and_then(ValueRef::tracks),
        areas: None,
        row_axis: false,
    };
    let row_lines = Lines {
        tracks: template(cs, false).and_then(ValueRef::tracks),
        areas: None,
        row_axis: true,
    };
    let (explicit_cols, explicit_rows) = cb.grid_explicit();
    let (cs_line, ce_line) = abs_axis_lines(&col_lines, st, explicit_cols);
    let (rs_line, re_line) = abs_axis_lines(&row_lines, st, explicit_rows);
    let (m, p, b) = (cb.margin(), cb.padding(), cb.border());
    let pad_x0 = cb.x() + m.left + b.left;
    let pad_y0 = cb.y() + m.top + b.top;
    let pad_x1 = pad_x0 + cb.content_width() + p.left + p.right;
    let pad_y1 = pad_y0 + cb.content_height() + p.top + p.bottom;
    let rtl = is_rtl(cs);
    let (mut x0, mut x1, mut y0, mut y1) = (pad_x0, pad_x1, pad_y0, pad_y1);
    if !ct.is_empty() && (cs_line != 0 || ce_line != 0) {
        let n = ct.len() as i32;
        let mut s_edge = if rtl { pad_x1 } else { pad_x0 };
        let mut e_edge = if rtl { pad_x0 } else { pad_x1 };
        if cs_line != 0 {
            let t = &ct[(cs_line.min(n) - 1) as usize];
            s_edge = if (cs_line <= n) != rtl {
                t.start
            } else {
                t.end
            };
        }
        if ce_line != 0 {
            let t = &ct[if ce_line >= 2 {
                (ce_line - 2).min(n - 1) as usize
            } else {
                0
            }];
            e_edge = if (ce_line >= 2) != rtl {
                t.end
            } else {
                t.start
            };
        }
        x0 = fmin(s_edge, e_edge);
        x1 = fmax(s_edge, e_edge);
    }
    if rs_line != 0 && !rt.is_empty() {
        let n = rt.len() as i32;
        y0 = if rs_line <= n {
            rt[(rs_line - 1) as usize].start
        } else {
            rt[rt.len() - 1].end
        };
    }
    if re_line != 0 && !rt.is_empty() {
        let n = rt.len() as i32;
        y1 = if re_line >= 2 {
            rt[(re_line - 2).min(n - 1) as usize].end
        } else {
            rt[0].start
        };
    }
    Some(Area {
        x: x0,
        y: y0,
        w: fmax(x1 - x0, 0.0),
        h: fmax(y1 - y0, 0.0),
    })
}

pub(crate) fn static_align_offset(align: Option<&CStr>, free_space: f64, flip: bool) -> f64 {
    let Some(align) = align.map(CStr::to_bytes).filter(|a| *a != b"auto") else {
        return 0.0;
    };
    let at_end = matches!(align, b"end" | b"flex-end" | b"self-end" | b"last baseline");
    let at_start = matches!(
        align,
        b"start" | b"flex-start" | b"self-start" | b"baseline"
    );
    let (near, far) = if flip {
        (free_space, 0.0)
    } else {
        (0.0, free_space)
    };
    match align {
        b"center" => free_space / 2.0,
        b"left" => near,
        b"right" => far,
        _ if at_end => far,
        _ if at_start => near,
        _ => near,
    }
}

pub(crate) fn keyword_or<'a>(
    s: Option<StyleRef<'a>>,
    prop: PropId,
    fallback: &'a CStr,
) -> &'a CStr {
    match s.and_then(|s| s.get(prop)).and_then(ValueRef::keyword_text) {
        Some(kw) => ffi::alignment_base(kw),
        None => fallback,
    }
}

pub(crate) fn self_alignment<'a>(
    item: Option<StyleRef<'a>>,
    container: Option<StyleRef<'a>>,
    self_prop: PropId,
    items_prop: PropId,
    fallback: &'a CStr,
) -> &'a CStr {
    match item.and_then(|s| s.keyword_of(self_prop)) {
        Some(kw) if kw != c"auto" => kw,
        _ => keyword_or(container, items_prop, fallback),
    }
}

pub(crate) fn static_position(
    abox: BoxRef<'_>,
    cb: BoxRef<'_>,
    area: &Area,
    static_x: bool,
    static_y: bool,
) {
    let cs = style_of(cb);
    let rtl = is_rtl(cs);
    let (m, p, b) = (abox.margin(), abox.padding(), abox.border());
    let outer_w = abox.content_width() + p.left + p.right + b.left + b.right + m.left + m.right;
    let outer_h = abox.content_height() + p.top + p.bottom + b.top + b.bottom + m.top + m.bottom;
    let st = style_of(abox);
    if static_x {
        let js = self_alignment(st, cs, PropId::JustifySelf, PropId::JustifyItems, c"normal");
        let x = area.x + static_align_offset(Some(js), area.w - outer_w, rtl);
        ffi::shift(abox, x - abox.x(), 0.0);
    }
    if static_y {
        let al = self_alignment(st, cs, PropId::AlignSelf, PropId::AlignItems, c"normal");
        let y = area.y + static_align_offset(Some(al), area.h - outer_h, false);
        ffi::shift(abox, 0.0, y - abox.y());
    }
}

fn append_line_names(s: &mut Vec<u8>, tk: Option<&GridTracks>, line: i32) {
    let Some(tk) = tk else {
        return;
    };
    let mut open = false;
    let n = usize::try_from(tk.n_line_names)
        .unwrap_or(0)
        .min(tk.line_names.len());
    for ln in &tk.line_names[..n] {
        if ln.line != line {
            continue;
        }
        s.extend_from_slice(if open {
            b" "
        } else if s.is_empty() {
            b"["
        } else {
            b" ["
        });
        s.extend_from_slice(line_name(ln));
        open = true;
    }
    if open {
        s.push(b']');
    }
}

pub(crate) fn resolved_tracks(b: BoxRef<'_>, columns: bool) -> Option<Vec<u8>> {
    let tr = ffi::track_edges(b.grid_tracks(columns))?;
    let mut tv = template(style_of(b), columns).and_then(ValueRef::tracks);
    if tv.is_some_and(|t| t.subgrid != 0) {
        let mut p = b.parent();
        while let Some(pb) = p
            && pb.style().is_null()
        {
            p = pb.parent();
        }
        if let Some(pb) = p
            && display_of(style_of(pb)).is_grid_container()
        {
            return None;
        }
        tv = None;
    }
    let expanded;
    let mut tk = None;
    if let Some(t) = tv {
        if t.auto_repeat == AUTO_REPEAT_NONE {
            tk = Some(t);
        } else if t.auto_repeat_count > 0 {
            let count = t.auto_repeat_count;
            let others = t.n - count;
            let (ec, er) = b.grid_explicit();
            let explicit_n = if columns { ec } else { er };
            if explicit_n > others && (explicit_n - others) % count == 0 {
                let mut e = *t;
                expand_repeat_names(t, (explicit_n - others) / count, &mut e);
                expanded = e;
                tk = Some(&expanded);
            }
        }
    }
    if tr.is_empty() {
        return Some(b"none".to_vec());
    }
    if tk.is_none() && tv.is_none() && children(b).all(|c| is_absolute_or_fixed(style_of(c))) {
        return Some(b"none".to_vec());
    }
    let mut s = Vec::new();
    for (i, e) in tr.iter().enumerate() {
        append_line_names(&mut s, tk, i as i32 + 1);
        if !s.is_empty() {
            s.push(b' ');
        }
        let size = e.end - e.start;
        let size = if size < 0.0 { 0.0 } else { size };
        ffi::format_g((size * 100.0).round() / 100.0, &mut s);
        s.extend_from_slice(b"px");
    }
    append_line_names(&mut s, tk, tr.len() as i32 + 1);
    Some(s)
}

pub(crate) fn track_edge(start: f64, end: f64) -> TrackEdges {
    TrackEdges { start, end }
}
