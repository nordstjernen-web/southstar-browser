//! Southstar — the 2D context's fillText, strokeText and measureText, laid out with ns-pango in the context's font, alignment, baseline and direction.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::cairo::Context;
use crate::ffi::pango::{FontDescription, Layout, PANGO_SCALE};
use crate::ffi::state::CanvasState;

type Result = core::result::Result<Value, Value>;

const DEFAULT_FONT: &[u8] = b"10px sans-serif";

pub(crate) fn font_parts(css: &[u8]) -> (Vec<u8>, f64) {
    let src = if css.is_empty() { DEFAULT_FONT } else { css };
    let mut size_px = 10.0;
    let mut found_size = false;
    let mut rest: Vec<u8> = Vec::new();
    for token in src
        .split(|c| c.is_ascii_whitespace())
        .filter(|t| !t.is_empty())
    {
        if !found_size && token.len() >= 3 && token[0].is_ascii_digit() {
            let (mut v, used) = crate::ffi::strtod_prefix(token);
            if used > 0 {
                let unit = &token[used..];
                let px = unit.len() >= 2 && unit[..2].eq_ignore_ascii_case(b"px");
                let pt = unit.len() >= 2 && unit[..2].eq_ignore_ascii_case(b"pt");
                if px || pt {
                    if pt {
                        v = v * 96.0 / 72.0;
                    }
                    size_px = v;
                    found_size = true;
                    continue;
                }
                if used == token.len() {
                    size_px = v;
                    found_size = true;
                    continue;
                }
            }
        }
        if !rest.is_empty() {
            rest.push(b' ');
        }
        rest.extend_from_slice(token);
    }
    if rest.is_empty() {
        rest.extend_from_slice(b"sans-serif");
    }
    if size_px <= 0.0 {
        size_px = 10.0;
    }
    (rest, size_px)
}

fn font_description(st: &CanvasState) -> FontDescription {
    let css = crate::ffi::state_font(st);
    let (families, size_px) = font_parts(css);
    FontDescription::new(&families, size_px)
}

fn attribute(scope: &mut Scope<'_>, this: &Value, name: &str) -> Option<Vec<u8>> {
    let v = crate::hidden::get(scope, this, name).ok()?;
    if !v.is_string() {
        return None;
    }
    scope.to_bytes(&v).ok()
}

fn text_arg(scope: &mut Scope<'_>, v: &Value) -> core::result::Result<Vec<u8>, Value> {
    let mut bytes = scope.to_bytes(v)?;
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    Ok(bytes)
}

fn number(scope: &mut Scope<'_>, args: &[Value], i: usize) -> f64 {
    args.get(i)
        .map_or(0.0, |v| scope.to_number(v).unwrap_or(f64::NAN))
}

fn baseline_shift(baseline: Option<&[u8]>, baseline_offset: f64, logical_bottom: f64) -> f64 {
    match baseline {
        None => -baseline_offset,
        Some(b"top") => 0.0,
        Some(b"hanging") => -baseline_offset * 0.2,
        Some(b"middle") => -baseline_offset * 0.5,
        Some(b"ideographic") => -logical_bottom,
        Some(_) => -baseline_offset,
    }
}

fn align_shift(align: &[u8], rtl: bool, width: f64) -> f64 {
    match align {
        b"center" => -width / 2.0,
        b"right" => -width,
        b"end" if !rtl => -width,
        b"start" if rtl => -width,
        _ => 0.0,
    }
}

struct Placement {
    x: f64,
    y: f64,
    max_width: f64,
}

fn paint(
    scope: &mut Scope<'_>,
    this: &Value,
    st: &CanvasState,
    text: &[u8],
    at: Placement,
    stroke: bool,
) {
    let Some(cr) = (unsafe { Context::from_raw(st.cr) }) else {
        return;
    };
    let desc = font_description(st);
    let layout = Layout::new(cr, &desc, text);
    let extents = layout.extents();
    let baseline_offset = f64::from(extents.baseline) / PANGO_SCALE;
    let logical_bottom = f64::from(extents.logical.y + extents.logical.height) / PANGO_SCALE;
    let baseline = attribute(scope, this, "textBaseline");
    let dy = baseline_shift(baseline.as_deref(), baseline_offset, logical_bottom);
    let rtl = attribute(scope, this, "direction").as_deref() == Some(b"rtl");
    let align = attribute(scope, this, "textAlign").unwrap_or_else(|| b"start".to_vec());
    let width = f64::from(extents.logical.width) / PANGO_SCALE;
    let dx = align_shift(&align, rtl, width);
    let xscale = if at.max_width > 0.0 && width > at.max_width {
        at.max_width / width
    } else {
        1.0
    };
    cr.save();
    crate::style::apply_composite(scope, this, cr);
    cr.translate(at.x + dx * xscale, at.y + dy);
    if xscale < 1.0 {
        cr.scale(xscale, 1.0);
    }
    if stroke {
        crate::draw::set_stroke_source(scope, this, st);
        cr.set_line_width(st.line_width);
        cr.move_to(0.0, 0.0);
        layout.path(cr);
        cr.stroke();
    } else {
        crate::draw::set_fill_source(scope, this, st);
        cr.move_to(0.0, 0.0);
        layout.show(cr);
    }
    cr.restore();
}

fn draw_text(scope: &mut Scope<'_>, this: &Value, args: &[Value], stroke: bool) -> Result {
    if args.len() < 3 {
        return Ok(Value::undefined());
    }
    let Some(st) = crate::draw::state(scope, this) else {
        return Ok(Value::undefined());
    };
    crate::style::sync(scope, this, st);
    let text = text_arg(scope, &args[0])?;
    let at = Placement {
        x: number(scope, args, 1),
        y: number(scope, args, 2),
        max_width: if args.len() >= 4 {
            number(scope, args, 3)
        } else {
            0.0
        },
    };
    paint(scope, this, st, &text, at, stroke);
    crate::ffi::mark_mutated(scope);
    Ok(Value::undefined())
}

pub(crate) fn fill_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    draw_text(scope, this, args, false)
}

pub(crate) fn stroke_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    draw_text(scope, this, args, true)
}

fn pack(width: f64, ascent: f64, descent: f64, font_ascent: f64, font_descent: f64) -> [f64; 10] {
    [
        width,
        0.0,
        width,
        ascent,
        descent,
        font_ascent,
        font_descent,
        font_ascent * 0.8,
        0.0,
        -font_descent,
    ]
}

fn metrics(st: &CanvasState, text: &[u8]) -> [f64; 10] {
    let Some(cr) = (unsafe { Context::from_raw(st.cr) }) else {
        return pack(0.0, 0.0, 0.0, 0.0, 0.0);
    };
    let desc = font_description(st);
    let layout = Layout::new(cr, &desc, text);
    let extents = layout.extents();
    let baseline_y = f64::from(extents.baseline);
    let ink = extents.ink;
    let width = f64::from(extents.logical.width) / PANGO_SCALE;
    let ascent = ((baseline_y - f64::from(ink.y)) / PANGO_SCALE).max(0.0);
    let descent = ((f64::from(ink.y + ink.height) - baseline_y) / PANGO_SCALE).max(0.0);
    let (font_ascent, font_descent) = layout
        .font_ascent_descent(&desc)
        .map_or((0.0, 0.0), |(a, d)| {
            (f64::from(a) / PANGO_SCALE, f64::from(d) / PANGO_SCALE)
        });
    pack(width, ascent, descent, font_ascent, font_descent)
}

pub(crate) fn measure_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result {
    let mut values = pack(0.0, 0.0, 0.0, 0.0, 0.0);
    if let Some(first) = args.first() {
        let st = crate::draw::state(scope, this);
        let text = text_arg(scope, first)?;
        if let Some(st) = st {
            crate::style::sync(scope, this, st);
            values = metrics(st, &text);
        }
    }
    Ok(crate::ffi::new_textmetrics(scope, this, &values))
}
