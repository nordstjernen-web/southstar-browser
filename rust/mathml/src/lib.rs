//! Southstar — minimalist MathML presentation layout and paint: tokens, scripts, fractions, radicals, under and over scripts, tables and fences.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::CStr;
use std::ffi::CString;

use ffi::{Canvas, Token};
use southstar_dom::{Node, children};

const MAX_DEPTH: i32 = 64;
const MAX_CELLS: i32 = 4096;
const MAX_FENCED: usize = 64;
const SCRIPT_SCALE: f64 = 0.72;

#[derive(Clone, Copy)]
struct Extent {
    width: f64,
    ascent: f64,
    descent: f64,
}

impl Extent {
    fn empty(fpx: f64) -> Extent {
        Extent {
            width: 0.0,
            ascent: fpx * 0.7,
            descent: fpx * 0.2,
        }
    }
}

fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn text_is_blank(text: Option<&CStr>) -> bool {
    text.is_none_or(|t| t.to_bytes().iter().all(u8::is_ascii_whitespace))
}

fn tag_is(node: Node, name: &str) -> bool {
    node.element_name()
        .is_some_and(|tag| !tag.is_empty() && tag.eq_ignore_ascii_case(name.as_bytes()))
}

fn is_renderable(node: Node) -> bool {
    node.is_element() || (node.is_text() && !text_is_blank(node.text()))
}

fn args(node: Node<'_>, max: usize) -> (Vec<Node<'_>>, usize) {
    let mut kept = Vec::new();
    let mut count = 0;
    for child in children(node).filter(|child| is_renderable(*child)) {
        if count < max {
            kept.push(child);
        }
        count += 1;
    }
    (kept, count)
}

fn stripped(text: &[u8]) -> CString {
    CString::new(text.trim_ascii()).unwrap_or_default()
}

fn render_token(
    canvas: Option<&Canvas>,
    text: &CStr,
    fpx: f64,
    italic: bool,
    x: f64,
    by: f64,
) -> Extent {
    let token = Token::new(text, fpx, italic);
    let ascent = f64::from(token.baseline()) / ffi::PANGO_SCALE;
    let (width, height) = token.logical_size();
    let total = f64::from(height) / ffi::PANGO_SCALE;
    if let Some(canvas) = canvas
        && !text.is_empty()
    {
        canvas.move_to(x, by - ascent);
        token.show(canvas);
    }
    Extent {
        width: f64::from(width) / ffi::PANGO_SCALE,
        ascent,
        descent: total - ascent,
    }
}

fn render_seq(
    canvas: Option<&Canvas>,
    parent: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let mut cx = x;
    let (mut max_ascent, mut max_descent) = (0.0, 0.0);
    let mut any = false;
    for child in children(parent).filter(|child| is_renderable(*child)) {
        if tag_is(child, "annotation") || tag_is(child, "annotation-xml") {
            continue;
        }
        let e = render(canvas, Some(child), fpx, depth + 1, cx, by);
        cx += e.width;
        if e.ascent > max_ascent {
            max_ascent = e.ascent;
        }
        if e.descent > max_descent {
            max_descent = e.descent;
        }
        any = true;
    }
    if !any {
        max_ascent = fpx * 0.7;
        max_descent = fpx * 0.2;
    }
    Extent {
        width: cx - x,
        ascent: max_ascent,
        descent: max_descent,
    }
}

fn mo_spacing(text: &[u8], fpx: f64) -> f64 {
    match text {
        b"" => 0.0,
        b"(" | b")" | b"[" | b"]" | b"{" | b"}" | b"," | b"!" => fpx * 0.05,
        _ => fpx * 0.18,
    }
}

fn render_script(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
    superscript: bool,
) -> Extent {
    let (args, count) = args(node, 2);
    if count < 2 {
        return render_seq(canvas, node, fpx, depth, x, by);
    }
    let base = render(canvas, Some(args[0]), fpx, depth + 1, x, by);
    let sfpx = fpx * SCRIPT_SCALE;
    let script = render(None, Some(args[1]), sfpx, depth + 1, 0.0, 0.0);
    let (ascent, descent) = if superscript {
        let shift = base.ascent * 0.5 + sfpx * 0.2;
        if canvas.is_some() {
            render(
                canvas,
                Some(args[1]),
                sfpx,
                depth + 1,
                x + base.width,
                by - shift,
            );
        }
        (max(base.ascent, shift + script.ascent), base.descent)
    } else {
        let shift = max(base.descent * 0.5, sfpx * 0.3);
        if canvas.is_some() {
            render(
                canvas,
                Some(args[1]),
                sfpx,
                depth + 1,
                x + base.width,
                by + shift,
            );
        }
        (base.ascent, max(base.descent, shift + script.descent))
    };
    Extent {
        width: base.width + script.width,
        ascent,
        descent,
    }
}

fn render_subsup(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let (args, count) = args(node, 3);
    if count < 3 {
        return render_seq(canvas, node, fpx, depth, x, by);
    }
    let base = render(canvas, Some(args[0]), fpx, depth + 1, x, by);
    let sfpx = fpx * SCRIPT_SCALE;
    let sub = render(None, Some(args[1]), sfpx, depth + 1, 0.0, 0.0);
    let sup = render(None, Some(args[2]), sfpx, depth + 1, 0.0, 0.0);
    let up = base.ascent * 0.5 + sfpx * 0.2;
    let down = max(base.descent * 0.5, sfpx * 0.3);
    if canvas.is_some() {
        render(
            canvas,
            Some(args[2]),
            sfpx,
            depth + 1,
            x + base.width,
            by - up,
        );
        render(
            canvas,
            Some(args[1]),
            sfpx,
            depth + 1,
            x + base.width,
            by + down,
        );
    }
    Extent {
        width: base.width + max(sub.width, sup.width),
        ascent: max(base.ascent, up + sup.ascent),
        descent: max(base.descent, down + sub.descent),
    }
}

fn render_frac(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let (args, count) = args(node, 2);
    if count < 2 {
        return render_seq(canvas, node, fpx, depth, x, by);
    }
    let num = render(None, Some(args[0]), fpx, depth + 1, 0.0, 0.0);
    let den = render(None, Some(args[1]), fpx, depth + 1, 0.0, 0.0);
    let pad = fpx * 0.2;
    let width = max(num.width, den.width) + 2.0 * pad;
    let axis = fpx * 0.30;
    let rule = max(1.0, fpx * 0.055);
    let gap = fpx * 0.18;
    let bar_y = by - axis;
    if let Some(c) = canvas {
        let nx = x + (width - num.width) / 2.0;
        let dx = x + (width - den.width) / 2.0;
        render(
            canvas,
            Some(args[0]),
            fpx,
            depth + 1,
            nx,
            bar_y - gap - num.descent,
        );
        render(
            canvas,
            Some(args[1]),
            fpx,
            depth + 1,
            dx,
            bar_y + gap + den.ascent,
        );
        c.save();
        c.set_line_width(rule);
        c.move_to(x + pad * 0.5, bar_y);
        c.line_to(x + width - pad * 0.5, bar_y);
        c.stroke();
        c.restore();
    }
    let low = -axis + gap + den.ascent + den.descent;
    Extent {
        width,
        ascent: axis + gap + num.ascent + num.descent,
        descent: if low < 0.0 { 0.0 } else { low },
    }
}

fn render_radical(
    canvas: Option<&Canvas>,
    fpx: f64,
    radicand: Extent,
    x: f64,
    by: f64,
) -> (f64, f64) {
    let surd = fpx * 0.6;
    let top_gap = fpx * 0.12;
    let pad = fpx * 0.12;
    if let Some(c) = canvas {
        let top = by - radicand.ascent - top_gap;
        let bottom = by + radicand.descent;
        c.save();
        c.set_line_width(max(1.0, fpx * 0.05));
        c.move_to(x, by - radicand.ascent * 0.35);
        c.line_to(x + surd * 0.45, bottom);
        c.line_to(x + surd * 0.85, top);
        c.line_to(x + surd + radicand.width + pad, top);
        c.stroke();
        c.restore();
    }
    (surd + radicand.width + pad + top_gap, surd)
}

fn radical_ascent(radicand: Extent, fpx: f64) -> f64 {
    radicand.ascent + fpx * 0.12 + max(1.0, fpx * 0.05)
}

fn render_sqrt(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let radicand = render_seq(None, node, fpx, depth, 0.0, 0.0);
    let (total, surd) = render_radical(canvas, fpx, radicand, x, by);
    if canvas.is_some() {
        render_seq(canvas, node, fpx, depth, x + surd, by);
    }
    Extent {
        width: total,
        ascent: radical_ascent(radicand, fpx),
        descent: radicand.descent,
    }
}

fn render_root(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let (args, count) = args(node, 2);
    if count < 2 {
        return render_sqrt(canvas, node, fpx, depth, x, by);
    }
    let ifpx = fpx * 0.55;
    let index = render(None, Some(args[1]), ifpx, depth + 1, 0.0, 0.0);
    let radicand = render(None, Some(args[0]), fpx, depth + 1, 0.0, 0.0);
    let ox = x + index.width * 0.7;
    let (total, surd) = render_radical(canvas, fpx, radicand, ox, by);
    if canvas.is_some() {
        render(canvas, Some(args[0]), fpx, depth + 1, ox + surd, by);
        render(
            canvas,
            Some(args[1]),
            ifpx,
            depth + 1,
            x,
            by - radicand.ascent * 0.6,
        );
    }
    Extent {
        width: (ox - x) + total,
        ascent: max(
            radical_ascent(radicand, fpx),
            radicand.ascent * 0.6 + index.ascent,
        ),
        descent: radicand.descent,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Limits {
    Over,
    Under,
    Both,
}

fn render_under_over(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
    limits: Limits,
) -> Extent {
    let (args, count) = args(node, 3);
    let need = if limits == Limits::Both { 3 } else { 2 };
    if count < need {
        return render_seq(canvas, node, fpx, depth, x, by);
    }
    let sfpx = fpx * SCRIPT_SCALE;
    let base = render(None, Some(args[0]), fpx, depth + 1, 0.0, 0.0);
    let gap = fpx * 0.12;
    let (over_node, under_node) = match limits {
        Limits::Both => (Some(args[2]), Some(args[1])),
        Limits::Over => (Some(args[1]), None),
        Limits::Under => (None, Some(args[1])),
    };
    let zero = Extent {
        width: 0.0,
        ascent: 0.0,
        descent: 0.0,
    };
    let over_extent = over_node.map_or(zero, |n| render(None, Some(n), sfpx, depth + 1, 0.0, 0.0));
    let under_extent =
        under_node.map_or(zero, |n| render(None, Some(n), sfpx, depth + 1, 0.0, 0.0));
    let width = max(base.width, max(over_extent.width, under_extent.width));
    if canvas.is_some() {
        let bx = x + (width - base.width) / 2.0;
        render(canvas, Some(args[0]), fpx, depth + 1, bx, by);
        if let Some(n) = over_node {
            let sx = x + (width - over_extent.width) / 2.0;
            render(
                canvas,
                Some(n),
                sfpx,
                depth + 1,
                sx,
                by - base.ascent - gap - over_extent.descent,
            );
        }
        if let Some(n) = under_node {
            let sx = x + (width - under_extent.width) / 2.0;
            render(
                canvas,
                Some(n),
                sfpx,
                depth + 1,
                sx,
                by + base.descent + gap + under_extent.ascent,
            );
        }
    }
    Extent {
        width,
        ascent: if over_node.is_some() {
            base.ascent + gap + over_extent.ascent + over_extent.descent
        } else {
            base.ascent
        },
        descent: if under_node.is_some() {
            base.descent + gap + under_extent.ascent + under_extent.descent
        } else {
            base.descent
        },
    }
}

fn is_row(node: Node) -> bool {
    tag_is(node, "mtr") || tag_is(node, "mlabeledtr")
}

fn render_table(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let mut columns: Vec<f64> = Vec::new();
    let mut rows: Vec<(f64, f64)> = Vec::new();
    let mut cells = 0;
    for row in children(node).filter(|r| is_row(*r)) {
        let (mut ascent, mut descent) = (fpx * 0.7, fpx * 0.2);
        for (col, cell) in children(row).filter(|c| tag_is(*c, "mtd")).enumerate() {
            cells += 1;
            if cells > MAX_CELLS {
                break;
            }
            let e = render_seq(None, cell, fpx, depth + 1, 0.0, 0.0);
            if columns.len() <= col {
                columns.resize(col + 1, 0.0);
            }
            if e.width > columns[col] {
                columns[col] = e.width;
            }
            if e.ascent > ascent {
                ascent = e.ascent;
            }
            if e.descent > descent {
                descent = e.descent;
            }
        }
        rows.push((ascent, descent));
    }
    let (column_gap, row_gap) = (fpx * 0.6, fpx * 0.35);
    let mut total_width = 0.0;
    for (i, width) in columns.iter().enumerate() {
        total_width += width;
        if i + 1 < columns.len() {
            total_width += column_gap;
        }
    }
    let mut total_height = 0.0;
    for (i, (ascent, descent)) in rows.iter().enumerate() {
        total_height += ascent + descent;
        if i + 1 < rows.len() {
            total_height += row_gap;
        }
    }
    let axis = fpx * 0.30;
    if canvas.is_some() {
        let mut cy = by - axis - total_height / 2.0;
        for (row, &(ascent, descent)) in children(node).filter(|r| is_row(*r)).zip(&rows) {
            let row_base = cy + ascent;
            let mut cx = x;
            for (cell, &cell_width) in children(row).filter(|c| tag_is(*c, "mtd")).zip(&columns) {
                let e = render_seq(None, cell, fpx, depth + 1, 0.0, 0.0);
                render_seq(
                    canvas,
                    cell,
                    fpx,
                    depth + 1,
                    cx + (cell_width - e.width) / 2.0,
                    row_base,
                );
                cx += cell_width + column_gap;
            }
            cy += ascent + descent + row_gap;
        }
    }
    let low = total_height / 2.0 - axis;
    Extent {
        width: total_width,
        ascent: axis + total_height / 2.0,
        descent: if low < 0.0 { 0.0 } else { low },
    }
}

fn fence_token(
    canvas: Option<&Canvas>,
    text: &CStr,
    fpx: f64,
    cx: &mut f64,
    by: f64,
    max_extent: &mut Extent,
) {
    let sp = mo_spacing(text.to_bytes(), fpx);
    let e = render_token(canvas, text, fpx, false, *cx + sp, by);
    *cx += e.width + 2.0 * sp;
    if e.ascent > max_extent.ascent {
        max_extent.ascent = e.ascent;
    }
    if e.descent > max_extent.descent {
        max_extent.descent = e.descent;
    }
}

fn render_fenced(
    canvas: Option<&Canvas>,
    node: Node,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let open = node.attr(c"open").unwrap_or(c"(");
    let close = node.attr(c"close").unwrap_or(c")");
    let separators = node.attr(c"separators").unwrap_or(c",").to_bytes();
    let (kids, _) = args(node, MAX_FENCED);
    let mut cx = x;
    let mut extent = Extent::empty(fpx);
    if !open.is_empty() {
        fence_token(canvas, open, fpx, &mut cx, by, &mut extent);
    }
    for (i, kid) in kids.iter().enumerate() {
        if i > 0 && !separators.is_empty() {
            let separator = separators[(i - 1).min(separators.len() - 1)];
            if !separator.is_ascii_whitespace() {
                let text = CString::new([separator]).unwrap_or_default();
                fence_token(canvas, &text, fpx, &mut cx, by, &mut extent);
            }
        }
        let e = render(canvas, Some(*kid), fpx, depth + 1, cx, by);
        cx += e.width;
        if e.ascent > extent.ascent {
            extent.ascent = e.ascent;
        }
        if e.descent > extent.descent {
            extent.descent = e.descent;
        }
    }
    if !close.is_empty() {
        fence_token(canvas, close, fpx, &mut cx, by, &mut extent);
    }
    extent.width = cx - x;
    extent
}

fn token_text(node: Node) -> CString {
    stripped(&node.collect_text())
}

fn render(
    canvas: Option<&Canvas>,
    node: Option<Node>,
    fpx: f64,
    depth: i32,
    x: f64,
    by: f64,
) -> Extent {
    let Some(node) = node else {
        return Extent::empty(fpx);
    };
    if depth > MAX_DEPTH {
        return Extent::empty(fpx);
    }
    if node.is_text() {
        let text = stripped(node.text().map_or(&[][..], CStr::to_bytes));
        return render_token(canvas, &text, fpx, false, x, by);
    }
    let is = |name| tag_is(node, name);
    if is("mi") {
        let text = token_text(node);
        let italic = !text.is_empty() && ffi::is_single_letter(&text);
        return render_token(canvas, &text, fpx, italic, x, by);
    }
    if is("mn") || is("mtext") || is("ms") {
        return render_token(canvas, &token_text(node), fpx, false, x, by);
    }
    if is("mo") {
        let text = token_text(node);
        let sp = mo_spacing(text.to_bytes(), fpx);
        let e = render_token(canvas, &text, fpx, false, x + sp, by);
        return Extent {
            width: e.width + 2.0 * sp,
            ..e
        };
    }
    if is("mphantom") {
        return render_seq(None, node, fpx, depth, 0.0, 0.0);
    }
    if is("mspace") {
        return Extent {
            width: fpx * 0.4,
            ascent: 0.0,
            descent: 0.0,
        };
    }
    if is("msup") || is("msub") {
        return render_script(canvas, node, fpx, depth, x, by, is("msup"));
    }
    if is("msubsup") {
        return render_subsup(canvas, node, fpx, depth, x, by);
    }
    if is("mfrac") {
        return render_frac(canvas, node, fpx, depth, x, by);
    }
    if is("msqrt") {
        return render_sqrt(canvas, node, fpx, depth, x, by);
    }
    if is("mroot") {
        return render_root(canvas, node, fpx, depth, x, by);
    }
    if is("mover") {
        return render_under_over(canvas, node, fpx, depth, x, by, Limits::Over);
    }
    if is("munder") {
        return render_under_over(canvas, node, fpx, depth, x, by, Limits::Under);
    }
    if is("munderover") {
        return render_under_over(canvas, node, fpx, depth, x, by, Limits::Both);
    }
    if is("mtable") {
        return render_table(canvas, node, fpx, depth, x, by);
    }
    if is("mfenced") {
        return render_fenced(canvas, node, fpx, depth, x, by);
    }
    if is("semantics") {
        let (first, _) = args(node, 1);
        return match first.first() {
            Some(first) => render(canvas, Some(*first), fpx, depth + 1, x, by),
            None => Extent::empty(fpx),
        };
    }
    render_seq(canvas, node, fpx, depth, x, by)
}

pub(crate) fn measure(math: Option<Node>, font_px: f64) -> (f64, f64, f64) {
    let e = render(None, math, font_px, 0, 0.0, 0.0);
    (e.width, e.ascent, e.descent)
}

pub(crate) fn paint(canvas: &Canvas, math: Node, x: f64, y: f64, font_px: f64, rgba: [f64; 4]) {
    let e = render(None, Some(math), font_px, 0, 0.0, 0.0);
    canvas.save();
    canvas.set_source_rgba(rgba);
    render(Some(canvas), Some(math), font_px, 0, x, y + e.ascent);
    canvas.restore();
}
