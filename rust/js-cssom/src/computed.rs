//! Southstar — the resolved values getComputedStyle reports: shorthands, used sizes, insets and transforms from layout, animation lists and pseudo-element styles.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId, StyleRef, ValueRef, display_of};

use crate::ffi::{self, AnimList, Js, fmt_g};

const UNIT_PX: u32 = 0;
const UNIT_EM: u32 = 1;
const UNIT_REM: u32 = 2;
const UNIT_PERCENT: u32 = 3;
const UNIT_NUMBER: u32 = 4;

const DISPLAY_BOX_NORMAL: u8 = 0;
const DISPLAY_BOX_CONTENTS: u8 = 2;
const DISPLAY_INNER_FLEX: u8 = 3;
const DISPLAY_INNER_GRID: u8 = 4;

const TARGET_NONE: i32 = 0;
const TARGET_ALL: i32 = 1;
const TARGET_OPACITY: i32 = 2;
const TARGET_TRANSFORM: i32 = 3;
const TARGET_COLOR: i32 = 4;
const TARGET_BG_COLOR: i32 = 5;
const TARGET_OTHER: i32 = 6;

pub(crate) const COMPUTED_PROPS: &[&str] = &[
    "-webkit-text-fill-color",
    "accent-color",
    "border-collapse",
    "border-spacing",
    "caption-side",
    "caret-color",
    "clip-rule",
    "color",
    "color-interpolation",
    "color-interpolation-filters",
    "color-scheme",
    "cursor",
    "direction",
    "dominant-baseline",
    "empty-cells",
    "fill",
    "fill-opacity",
    "fill-rule",
    "font-family",
    "font-feature-settings",
    "font-kerning",
    "font-language-override",
    "font-optical-sizing",
    "font-size",
    "font-style",
    "font-variant-alternates",
    "font-variant-caps",
    "font-variant-east-asian",
    "font-variant-emoji",
    "font-variant-ligatures",
    "font-variant-numeric",
    "font-variant-position",
    "font-variation-settings",
    "font-weight",
    "font-width",
    "image-rendering",
    "letter-spacing",
    "line-height",
    "list-style-image",
    "list-style-position",
    "list-style-type",
    "math-depth",
    "math-shift",
    "math-style",
    "orphans",
    "overflow-wrap",
    "paint-order",
    "pointer-events",
    "quotes",
    "scrollbar-color",
    "shape-rendering",
    "stroke",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "stroke-width",
    "tab-size",
    "text-align",
    "text-anchor",
    "text-decoration-line",
    "text-decoration-skip-ink",
    "text-indent",
    "text-justify",
    "text-rendering",
    "text-shadow",
    "text-transform",
    "text-underline-offset",
    "text-underline-position",
    "text-wrap-mode",
    "text-wrap-style",
    "visibility",
    "white-space-collapse",
    "widows",
    "word-break",
    "word-spacing",
    "writing-mode",
    "align-content",
    "align-items",
    "align-self",
    "anchor-name",
    "anchor-scope",
    "animation-composition",
    "animation-delay",
    "animation-direction",
    "animation-duration",
    "animation-fill-mode",
    "animation-iteration-count",
    "animation-name",
    "animation-play-state",
    "animation-timeline",
    "animation-timing-function",
    "appearance",
    "aspect-ratio",
    "backdrop-filter",
    "background-attachment",
    "background-blend-mode",
    "background-clip",
    "background-color",
    "background-image",
    "background-origin",
    "background-position-x",
    "background-position-y",
    "background-repeat",
    "background-size",
    "block-size",
    "border-block-end-color",
    "border-block-end-style",
    "border-block-end-width",
    "border-block-start-color",
    "border-block-start-style",
    "border-block-start-width",
    "border-bottom-color",
    "border-bottom-left-radius",
    "border-bottom-right-radius",
    "border-bottom-style",
    "border-bottom-width",
    "border-end-end-radius",
    "border-end-start-radius",
    "border-image-outset",
    "border-image-repeat",
    "border-image-slice",
    "border-image-source",
    "border-image-width",
    "border-inline-end-color",
    "border-inline-end-style",
    "border-inline-end-width",
    "border-inline-start-color",
    "border-inline-start-style",
    "border-inline-start-width",
    "border-left-color",
    "border-left-style",
    "border-left-width",
    "border-right-color",
    "border-right-style",
    "border-right-width",
    "border-start-end-radius",
    "border-start-start-radius",
    "border-top-color",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-top-style",
    "border-top-width",
    "bottom",
    "box-shadow",
    "box-sizing",
    "break-after",
    "break-before",
    "break-inside",
    "clear",
    "clip",
    "clip-path",
    "column-count",
    "column-gap",
    "column-height",
    "column-span",
    "column-width",
    "contain",
    "container-name",
    "container-type",
    "content",
    "content-visibility",
    "corner-bottom-left-shape",
    "corner-bottom-right-shape",
    "corner-end-end-shape",
    "corner-end-start-shape",
    "corner-start-end-shape",
    "corner-start-start-shape",
    "corner-top-left-shape",
    "corner-top-right-shape",
    "counter-increment",
    "counter-reset",
    "counter-set",
    "cx",
    "cy",
    "display",
    "filter",
    "flex-basis",
    "flex-direction",
    "flex-grow",
    "flex-shrink",
    "flex-wrap",
    "float",
    "flood-color",
    "flood-opacity",
    "grid-auto-columns",
    "grid-auto-flow",
    "grid-auto-rows",
    "grid-column-end",
    "grid-column-start",
    "grid-row-end",
    "grid-row-start",
    "grid-template-areas",
    "grid-template-columns",
    "grid-template-rows",
    "height",
    "inline-size",
    "inset-block-end",
    "inset-block-start",
    "inset-inline-end",
    "inset-inline-start",
    "isolation",
    "justify-content",
    "justify-items",
    "justify-self",
    "left",
    "margin-block-end",
    "margin-block-start",
    "margin-bottom",
    "margin-inline-end",
    "margin-inline-start",
    "margin-left",
    "margin-right",
    "margin-top",
    "mask-clip",
    "mask-composite",
    "mask-image",
    "mask-mode",
    "mask-origin",
    "mask-position",
    "mask-repeat",
    "mask-size",
    "mask-type",
    "max-block-size",
    "max-height",
    "max-inline-size",
    "max-width",
    "min-block-size",
    "min-height",
    "min-inline-size",
    "min-width",
    "mix-blend-mode",
    "object-fit",
    "object-position",
    "opacity",
    "order",
    "outline-color",
    "outline-offset",
    "outline-style",
    "outline-width",
    "overflow-block",
    "overflow-clip-margin-block-end",
    "overflow-clip-margin-block-start",
    "overflow-clip-margin-bottom",
    "overflow-clip-margin-inline-end",
    "overflow-clip-margin-inline-start",
    "overflow-clip-margin-left",
    "overflow-clip-margin-right",
    "overflow-clip-margin-top",
    "overflow-inline",
    "overflow-x",
    "overflow-y",
    "padding-block-end",
    "padding-block-start",
    "padding-bottom",
    "padding-inline-end",
    "padding-inline-start",
    "padding-left",
    "padding-right",
    "padding-top",
    "perspective",
    "perspective-origin",
    "position",
    "position-anchor",
    "position-area",
    "position-try-fallbacks",
    "position-try-order",
    "position-visibility",
    "r",
    "resize",
    "right",
    "rotate",
    "row-gap",
    "rx",
    "ry",
    "scale",
    "scroll-behavior",
    "scroll-margin-block-end",
    "scroll-margin-block-start",
    "scroll-margin-bottom",
    "scroll-margin-inline-end",
    "scroll-margin-inline-start",
    "scroll-margin-left",
    "scroll-margin-right",
    "scroll-margin-top",
    "scroll-padding-block-end",
    "scroll-padding-block-start",
    "scroll-padding-bottom",
    "scroll-padding-inline-end",
    "scroll-padding-inline-start",
    "scroll-padding-left",
    "scroll-padding-right",
    "scroll-padding-top",
    "scroll-snap-align",
    "scroll-snap-stop",
    "scroll-snap-type",
    "scroll-timeline-axis",
    "scroll-timeline-name",
    "scrollbar-gutter",
    "scrollbar-width",
    "shape-image-threshold",
    "shape-margin",
    "shape-outside",
    "stop-color",
    "stop-opacity",
    "table-layout",
    "text-decoration-color",
    "text-decoration-style",
    "text-decoration-thickness",
    "text-overflow",
    "timeline-scope",
    "top",
    "touch-action",
    "transform",
    "transform-box",
    "transform-origin",
    "transform-style",
    "transition-behavior",
    "transition-delay",
    "transition-duration",
    "transition-property",
    "transition-timing-function",
    "translate",
    "unicode-bidi",
    "user-select",
    "vector-effect",
    "vertical-align",
    "view-timeline-axis",
    "view-timeline-inset",
    "view-timeline-name",
    "view-transition-name",
    "white-space-trim",
    "width",
    "will-change",
    "x",
    "y",
    "z-index",
];

fn box_style(b: BoxRef<'_>) -> Option<StyleRef<'static>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

fn keyword_of(v: Option<ValueRef<'_>>) -> Option<&str> {
    v.and_then(ValueRef::keyword_text)
        .and_then(|k| k.to_str().ok())
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|v| !v.is_empty())
}

fn box_shorthand(js: Js, n: Node<'_>, sides: [&str; 4]) -> Option<String> {
    let [t, r, b, l] = sides.map(|side| lookup(js, n, side));
    let (t, r, b, l) = (t?, r?, b?, l?);
    Some(if t == r && r == b && b == l {
        t
    } else if t == b && l == r {
        format!("{t} {r}")
    } else if l == r {
        format!("{t} {r} {b}")
    } else {
        format!("{t} {r} {b} {l}")
    })
}

fn box_edge_px(b: BoxRef<'_>, name: &str) -> Option<String> {
    let (edges, side) = if let Some(side) = name.strip_prefix("margin-") {
        (b.margin(), side)
    } else if let Some(side) = name.strip_prefix("padding-") {
        (b.padding(), side)
    } else if name.starts_with("border-") && name.ends_with("-width") {
        let side = ["top", "right", "bottom", "left"]
            .into_iter()
            .find(|side| name.starts_with(&format!("border-{side}-")))?;
        (b.border(), side)
    } else {
        return None;
    };
    let v = match side {
        "top" => edges.top,
        "right" => edges.right,
        "bottom" => edges.bottom,
        "left" => edges.left,
        _ => return None,
    };
    Some(format!("{}px", fmt_g(v)))
}

fn transform_matrix(s: Option<StyleRef<'_>>, b: Option<BoxRef<'_>>) -> String {
    let Some(t) = s
        .and_then(|s| s.get(PropId::Transform))
        .and_then(ValueRef::transform)
        .filter(|t| t.n_ops != 0)
    else {
        return "none".into();
    };
    let (bw, bh) = b.map_or((0.0, 0.0), |b| {
        let (p, br) = (b.padding(), b.border());
        (
            b.content_width() + p.left + p.right + br.left + br.right,
            b.content_height() + p.top + p.bottom + br.top + br.bottom,
        )
    });
    let m = ffi::transform_matrix(t, bw, bh).m;
    let affine = southstar_mat4::Mat4 { m }.is_affine2d();
    if affine {
        return format!(
            "matrix({}, {}, {}, {}, {}, {})",
            fmt_g(m[0]),
            fmt_g(m[4]),
            fmt_g(m[1]),
            fmt_g(m[5]),
            fmt_g(m[3]),
            fmt_g(m[7])
        );
    }
    const ORDER: [usize; 16] = [0, 4, 8, 12, 1, 5, 9, 13, 2, 6, 10, 14, 3, 7, 11, 15];
    let parts: Vec<String> = ORDER.iter().map(|&k| fmt_g(m[k])).collect();
    format!("matrix3d({})", parts.join(", "))
}

fn font_px(js: Js, n: Node<'_>) -> f64 {
    let v = lookup(js, n, "font-size").map_or(16.0, |fs| ffi::ascii_strtod(&fs).0);
    if v > 0.0 { v } else { 16.0 }
}

fn position_keyword(b: BoxRef<'_>) -> &'static str {
    let style = box_style(b);
    keyword_of(style.and_then(|s| s.get(PropId::Position))).unwrap_or("static")
}

fn inset_value_auto(v: Option<ValueRef<'_>>) -> bool {
    match v {
        None => true,
        Some(v) => v.kind() == Kind::Keyword && keyword_of(Some(v)) == Some("auto"),
    }
}

fn inset_resolve_px(v: Option<ValueRef<'_>>, basis: f64, font_px: f64) -> Option<f64> {
    let v = v?;
    if let Some((n, unit)) = v.length() {
        return match unit {
            UNIT_PX => Some(n),
            UNIT_EM => Some(n * font_px),
            UNIT_REM => Some(n * 16.0),
            UNIT_PERCENT => Some(n / 100.0 * basis),
            _ => None,
        };
    }
    let [pct, px, em, rem] = v.calc_terms()?;
    Some(px + em * font_px + rem * 16.0 + pct / 100.0 * basis)
}

fn style_has_transform(s: StyleRef<'_>) -> bool {
    let transformed = [
        PropId::Transform,
        PropId::Translate,
        PropId::Rotate,
        PropId::Scale,
    ]
    .into_iter()
    .any(|p| {
        s.get(p)
            .and_then(ValueRef::transform)
            .is_some_and(|t| t.n_ops > 0)
    });
    transformed
        || s.get(PropId::Perspective)
            .and_then(ValueRef::length)
            .is_some_and(|(v, _)| v > 0.0)
}

fn style_scroll_container(s: StyleRef<'_>) -> bool {
    [PropId::Overflow, PropId::OverflowX, PropId::OverflowY]
        .into_iter()
        .any(|p| keyword_of(s.get(p)).is_some_and(|k| k != "visible" && k != "clip"))
}

fn logical_start(style: Option<StyleRef<'_>>, vertical: bool) -> &'static str {
    let writing_mode = style
        .and_then(|s| s.get(PropId::WritingMode))
        .filter(|v| v.kind() == Kind::Keyword)
        .map_or(Some("horizontal-tb"), |v| keyword_of(Some(v)));
    let rtl = keyword_of(style.and_then(|s| s.get(PropId::Direction))) == Some("rtl");
    let writing_mode = writing_mode.unwrap_or("");
    if writing_mode.starts_with("vertical-lr") {
        return if vertical {
            if rtl { "bottom" } else { "top" }
        } else {
            "left"
        };
    }
    if writing_mode.starts_with("vertical-rl") {
        return if vertical {
            if rtl { "bottom" } else { "top" }
        } else {
            "right"
        };
    }
    if vertical {
        "top"
    } else if rtl {
        "right"
    } else {
        "left"
    }
}

fn containing_block(js: Js, n: Node<'_>, b: BoxRef<'static>, pos: &str) -> Option<BoxRef<'static>> {
    if pos == "sticky" {
        if ffi::has_style_table(js) && ffi::layout_root(js).is_some() {
            for p in southstar_dom::ancestors(n) {
                if !p.is_element() {
                    continue;
                }
                let stop = match p.parent() {
                    None => true,
                    Some(pp) => {
                        pp.kind() == southstar_dom::Kind::Document
                            || p.element_name() == Some(b"body")
                    }
                };
                if stop {
                    break;
                }
                if ffi::style_of(js, p).is_some_and(style_scroll_container) {
                    return ffi::find_box(js, p);
                }
            }
        }
        return b.parent();
    }
    if pos == "relative" {
        return b.parent();
    }
    let fixed = pos == "fixed";
    if !ffi::has_style_table(js) || ffi::layout_root(js).is_none() {
        return None;
    }
    for p in southstar_dom::ancestors(n) {
        if !p.is_element() {
            continue;
        }
        let Some(ps) = ffi::style_of(js, p) else {
            continue;
        };
        let mut is_cb = style_has_transform(ps);
        if !fixed && !is_cb {
            is_cb = keyword_of(ps.get(PropId::Position)).is_some_and(|k| k != "static");
        }
        if is_cb {
            return ffi::find_box(js, p);
        }
    }
    None
}

fn px(v: f64) -> String {
    format!("{}px", fmt_g(v))
}

fn inset_px(js: Js, n: Node<'_>, b: BoxRef<'static>, name: &str) -> Option<String> {
    let style = box_style(b)?;
    let pos = position_keyword(b);
    let rel = pos == "relative";
    let sticky = pos == "sticky";
    let abs_pos = pos == "absolute" || pos == "fixed";
    if !rel && !sticky && !abs_pos {
        return None;
    }
    let opp_name = match name {
        "top" => "bottom",
        "bottom" => "top",
        "left" => "right",
        "right" => "left",
        _ => return None,
    };
    let vertical = name == "top" || name == "bottom";
    let cb = containing_block(js, n, b, pos);
    let (cb_w, cb_h, cb_x, cb_y) = match cb {
        Some(cb) => {
            let (m, p, br) = (cb.margin(), cb.padding(), cb.border());
            if abs_pos {
                (
                    cb.content_width() + p.left + p.right,
                    cb.content_height() + p.top + p.bottom,
                    cb.x() + m.left + br.left,
                    cb.y() + m.top + br.top,
                )
            } else {
                (
                    cb.content_width(),
                    cb.content_height(),
                    cb.x() + p.left,
                    cb.y() + p.top,
                )
            }
        }
        None => {
            let (w, h) = ffi::viewport();
            (w, h, 0.0, 0.0)
        }
    };
    let basis = if vertical { cb_h } else { cb_w };
    let font = font_px(js, n);
    let v = ffi::prop_id(name).and_then(|id| style.value_at(id));
    let ov = ffi::prop_id(opp_name).and_then(|id| style.value_at(id));

    if !inset_value_auto(v) {
        return inset_resolve_px(v, basis, font).map(px);
    }
    if sticky {
        return Some("auto".into());
    }
    if rel {
        if inset_value_auto(ov) {
            return Some("0px".into());
        }
        return inset_resolve_px(ov, basis, font).map(|r| px(-r + 0.0));
    }
    let (m, p, br) = (b.margin(), b.padding(), b.border());
    let outer_w = b.content_width() + p.left + p.right + br.left + br.right + m.left + m.right;
    let outer_h = b.content_height() + p.top + p.bottom + br.top + br.bottom + m.top + m.bottom;
    let outer = if vertical { outer_h } else { outer_w };
    if !inset_value_auto(ov) {
        let r = inset_resolve_px(ov, basis, font)?;
        return Some(px((if vertical { cb_h } else { cb_w }) - r - outer));
    }
    let start_off = if vertical { b.y() - cb_y } else { b.x() - cb_x };
    let cb_style = cb.and_then(box_style);
    let start = logical_start(cb_style, vertical);
    let cb_vertical = cb_style
        .and_then(|s| s.get(PropId::WritingMode))
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(|v| keyword_of(Some(v)))
        .is_some_and(|k| k.starts_with("vertical"));
    if cb_vertical {
        let used = if name == start {
            start_off
        } else {
            basis - start_off
        };
        return Some(px(used));
    }
    let physical_start = name == "left" || name == "top";
    let used = if physical_start {
        start_off
    } else {
        basis - start_off - outer
    };
    Some(px(used))
}

fn line_height_px(js: Js, n: Node<'_>, s: Option<StyleRef<'_>>) -> Option<String> {
    let (v, unit) = s.and_then(|s| s.get(PropId::LineHeight))?.length()?;
    if ffi::input_is_one_line_text(n) {
        let (normal, css) = ffi::line_heights(s);
        if css < normal {
            return Some(px(normal));
        }
    }
    let fs = font_px(js, n);
    match unit {
        UNIT_NUMBER | UNIT_EM => Some(px(v * fs)),
        UNIT_PERCENT => Some(px(v / 100.0 * fs)),
        UNIT_REM => Some(px(v * 16.0)),
        _ => None,
    }
}

fn target_property_name(target: i32) -> &'static str {
    match target {
        TARGET_OPACITY => "opacity",
        TARGET_TRANSFORM => "transform",
        TARGET_COLOR => "color",
        TARGET_BG_COLOR => "background-color",
        TARGET_ALL => "all",
        _ => "all",
    }
}

fn anim_longhand(list: &AnimList, sub: &str) -> Option<String> {
    let entries = list.entries();
    if entries.is_empty() {
        return None;
    }
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match sub {
            "duration" => {
                if e.duration_auto != 0 {
                    out.push_str("auto");
                } else {
                    out.push_str(&format!("{}s", fmt_g(e.duration_ms / 1000.0)));
                }
            }
            "delay" => out.push_str(&format!("{}s", fmt_g(e.delay_ms / 1000.0))),
            "property" => {
                let name = ffi::anim_entry_name(e);
                if e.target == TARGET_OTHER && name.is_some() {
                    out.push_str(&name.unwrap_or_default());
                } else if e.target == TARGET_NONE {
                    out.push_str("none");
                } else {
                    out.push_str(target_property_name(e.target));
                }
            }
            "name" => match ffi::anim_entry_name(e).filter(|n| !n.is_empty()) {
                Some(name) => out.push_str(&name),
                None => out.push_str("none"),
            },
            "timing" => out.push_str(&ffi::anim_entry_timing(e)),
            "iteration" => {
                if e.iterations.is_finite() {
                    out.push_str(&fmt_g(e.iterations));
                } else {
                    out.push_str("infinite");
                }
            }
            "play-state" => out.push_str(if e.paused != 0 { "paused" } else { "running" }),
            "direction" => out.push_str(match e.direction {
                1 => "reverse",
                2 => "alternate",
                3 => "alternate-reverse",
                _ => "normal",
            }),
            "fill" => out.push_str(match e.fill {
                1 => "forwards",
                2 => "backwards",
                3 => "both",
                _ => "none",
            }),
            _ => {}
        }
    }
    Some(out)
}

fn ascii_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b')
}

fn range_list(js: Js, n: Node<'_>, text: &str) -> String {
    let font = font_px(js, n);
    let mut out = String::new();
    for (i, raw) in text.split(',').enumerate() {
        let item = ascii_strip(raw);
        if i > 0 {
            out.push_str(", ");
        }
        let sp = item.rfind(' ');
        let lp = sp.map_or(item, |sp| &item[sp + 1..]);
        let (_, used) = ffi::ascii_strtod(lp);
        let rest = &lp[used..];
        let is_calc = ["calc(", "min(", "max(", "clamp("]
            .iter()
            .any(|p| lp.starts_with(p));
        let mut converted = None;
        if is_calc
            || (used > 0
                && !rest.is_empty()
                && rest != "px"
                && rest != "%"
                && rest.as_bytes()[0].is_ascii_alphabetic())
        {
            converted = ffi::with_declarations(&format!("left: {lp}"), |decls| {
                let lv = decls
                    .first()
                    .and_then(|d| unsafe { ValueRef::from_ptr(d.value) })?;
                let plain_len = lv.length().is_some_and(|(_, u)| u != UNIT_PERCENT);
                let calc_len = lv.calc_terms().is_some_and(|c| c[0] == 0.0);
                (plain_len || calc_len).then(|| px(ffi::dimension_px(lv, font, 0.0)))
            });
        }
        match converted {
            Some(converted) => {
                if let Some(sp) = sp {
                    out.push_str(&item[..sp]);
                    out.push(' ');
                }
                out.push_str(&converted);
            }
            None => out.push_str(item),
        }
    }
    out
}

fn anim_key(sub: &str, is_anim: bool) -> Option<(&'static str, PropId)> {
    Some(match (sub, is_anim) {
        ("duration", true) => ("duration", PropId::AnimationDuration),
        ("duration", false) => ("duration", PropId::TransitionDuration),
        ("delay", true) => ("delay", PropId::AnimationDelay),
        ("delay", false) => ("delay", PropId::TransitionDelay),
        ("property", false) => ("property", PropId::TransitionProperty),
        ("name", true) => ("name", PropId::AnimationName),
        ("timing-function", true) => ("timing", PropId::AnimationTimingFunction),
        ("timing-function", false) => ("timing", PropId::TransitionTimingFunction),
        ("iteration-count", true) => ("iteration", PropId::AnimationIterationCount),
        ("direction", true) => ("direction", PropId::AnimationDirection),
        ("fill-mode", true) => ("fill", PropId::AnimationFillMode),
        ("play-state", true) => ("play-state", PropId::AnimationPlayState),
        ("timeline", true) => ("timeline", PropId::AnimationTimeline),
        ("range-start", true) => ("range-start", PropId::AnimationRangeStart),
        ("range-end", true) => ("range-end", PropId::AnimationRangeEnd),
        ("behavior", false) => ("behavior", PropId::TransitionBehavior),
        _ => return None,
    })
}

fn anim_keyword_value(
    js: Js,
    n: Node<'_>,
    s: StyleRef<'_>,
    key: &str,
    keyword: &str,
    is_anim: bool,
) -> Option<String> {
    if key == "duration" || key == "delay" {
        if keyword.contains("auto") {
            let tl = s.get(PropId::AnimationTimeline);
            let auto_timeline = match tl {
                None => true,
                Some(tl) => tl.kind() == Kind::Keyword && keyword_of(Some(tl)) == Some("auto"),
            };
            if !auto_timeline {
                return Some(keyword.into());
            }
            let parts: Vec<String> = keyword
                .split(',')
                .map(|part| {
                    let item = ascii_strip(part);
                    if item == "auto" {
                        "0s".into()
                    } else {
                        ffi::time_computed(item).unwrap_or_else(|| item.into())
                    }
                })
                .collect();
            return Some(parts.join(", "));
        }
        if let Some(r) = ffi::time_computed(keyword) {
            return Some(r);
        }
    }
    if key == "range-start" || key == "range-end" {
        return Some(range_list(js, n, keyword));
    }
    if key == "timing" {
        let list = ffi::anim_effective(s, is_anim);
        if list.entries().is_empty() {
            let parts: Vec<String> = keyword
                .split(',')
                .take(8)
                .map(ffi::timing_text_canonical)
                .collect();
            return Some(parts.join(", "));
        }
        if let Some(r) = anim_longhand(&list, key) {
            return Some(r);
        }
    }
    Some(keyword.into())
}

fn anim_lookup(js: Js, n: Node<'_>, name: &str) -> Option<String> {
    let is_anim = name.starts_with('a');
    if name == "animation" || name == "transition" {
        let s = ffi::style_of(js, n);
        let (list, mismatch) = ffi::anim_lists(s, is_anim);
        return Some(if mismatch {
            String::new()
        } else if list.entries().is_empty() {
            (if is_anim { "none" } else { "all" }).into()
        } else {
            list.shorthand_serialize(is_anim)
        });
    }
    if name == "animation-range" {
        let st = anim_lookup(js, n, "animation-range-start");
        let en = anim_lookup(js, n, "animation-range-end");
        return ffi::animation_range_serialize(st.as_deref(), en.as_deref());
    }
    let dash = if is_anim { 9 } else { 10 };
    if name.as_bytes().get(dash) != Some(&b'-') {
        return None;
    }
    let (key, lhp) = anim_key(&name[dash + 1..], is_anim)?;
    if ffi::has_style_table(js) {
        let s = ffi::style_of(js, n);
        let lv = s.and_then(|s| s.get(lhp));
        if let (Some(s), Some(keyword)) = (s, keyword_of(lv)) {
            return anim_keyword_value(js, n, s, key, keyword, is_anim);
        }
        if let Some(s) = s {
            let list = ffi::anim_effective(s, is_anim);
            if let Some(r) = anim_longhand(&list, key) {
                return Some(r);
            }
        }
    }
    let fallback = match key {
        "duration" | "delay" => "0s",
        "property" => "all",
        "name" => "none",
        "timing" => "ease",
        "iteration" => "1",
        "direction" => "normal",
        "fill" => "none",
        "play-state" => "running",
        "timeline" => "auto",
        "range-start" | "range-end" => "normal",
        "behavior" => "normal",
        _ => return None,
    };
    Some(fallback.into())
}

fn radius_corner_axes(value: Option<&str>) -> (String, String) {
    let value = value.filter(|v| !v.is_empty()).unwrap_or("0px");
    let mut parts = value.splitn(2, ' ');
    let horizontal = parts.next().unwrap_or("0px").to_string();
    let vertical = parts
        .next()
        .map_or_else(|| horizontal.clone(), str::to_string);
    (horizontal, vertical)
}

fn radius_axis_list(c: &[String; 4]) -> String {
    if c[0] == c[1] && c[1] == c[2] && c[2] == c[3] {
        c[0].clone()
    } else if c[0] == c[2] && c[1] == c[3] {
        format!("{} {}", c[0], c[1])
    } else if c[1] == c[3] {
        format!("{} {} {}", c[0], c[1], c[2])
    } else {
        format!("{} {} {} {}", c[0], c[1], c[2], c[3])
    }
}

fn radius_shorthand(js: Js, n: Node<'_>) -> String {
    let corners = [
        "border-top-left-radius",
        "border-top-right-radius",
        "border-bottom-right-radius",
        "border-bottom-left-radius",
    ];
    let axes = corners.map(|c| radius_corner_axes(lookup(js, n, c).as_deref()));
    let h = radius_axis_list(&axes.clone().map(|a| a.0));
    let v = radius_axis_list(&axes.map(|a| a.1));
    if h == v { h } else { format!("{h} / {v}") }
}

fn font_shorthand(js: Js, n: Node<'_>) -> String {
    let get = |name| lookup(js, n, name);
    let (fstyle, fvariant, fweight, fstretch) = (
        get("font-style"),
        get("font-variant"),
        get("font-weight"),
        get("font-stretch"),
    );
    let (fsize, flh, ffamily) = (get("font-size"), get("line-height"), get("font-family"));
    let mut out = String::new();
    let push = |out: &mut String, part: &str| {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    };
    if let Some(s) = non_empty(&fstyle).filter(|s| *s != "normal") {
        push(&mut out, s);
    }
    if fvariant.as_deref() == Some("small-caps") {
        push(&mut out, "small-caps");
    }
    if let Some(w) = non_empty(&fweight).filter(|w| *w != "normal" && *w != "400") {
        push(&mut out, w);
    }
    if let Some(s) = non_empty(&fstretch).filter(|s| *s != "normal" && *s != "100%") {
        push(&mut out, s);
    }
    push(&mut out, non_empty(&fsize).unwrap_or("16px"));
    if let Some(lh) = non_empty(&flh).filter(|lh| *lh != "normal") {
        out.push_str(" / ");
        out.push_str(lh);
    }
    out.push(' ');
    out.push_str(non_empty(&ffamily).unwrap_or("serif"));
    out
}

fn shorthand(js: Js, n: Node<'_>, name: &str) -> Option<Option<String>> {
    let sides = |prefix: &str, suffix: &str| {
        ["top", "right", "bottom", "left"].map(|side| format!("{prefix}{side}{suffix}"))
    };
    let four =
        |names: [String; 4]| box_shorthand(js, n, [&names[0], &names[1], &names[2], &names[3]]);
    Some(match name {
        "font" => Some(font_shorthand(js, n)),
        "margin" => four(sides("margin-", "")),
        "padding" => four(sides("padding-", "")),
        "border-width" => four(sides("border-", "-width")),
        "border-color" => four(sides("border-", "-color")),
        "border-style" => four(sides("border-", "-style")),
        "inset" => box_shorthand(js, n, ["top", "right", "bottom", "left"]),
        "border-image" => {
            let part = |name, fallback: &'static str| {
                lookup(js, n, name)
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| fallback.into())
            };
            Some(format!(
                "{} {} / {} / {} {}",
                part("border-image-source", "none"),
                part("border-image-slice", "100%"),
                part("border-image-width", "1"),
                part("border-image-outset", "0"),
                part("border-image-repeat", "stretch")
            ))
        }
        "border-radius" => Some(radius_shorthand(js, n)),
        "object-position" | "background-position" => {
            let is_bg = name.starts_with('b');
            let x = lookup(
                js,
                n,
                if is_bg {
                    "background-position-x"
                } else {
                    "object-position-x"
                },
            );
            let y = lookup(
                js,
                n,
                if is_bg {
                    "background-position-y"
                } else {
                    "object-position-y"
                },
            );
            let (x, y) = (
                non_empty(&x).unwrap_or("50%"),
                non_empty(&y).unwrap_or("50%"),
            );
            Some(ffi::background_position_join(x, y).unwrap_or_else(|| format!("{x} {y}")))
        }
        "gap" | "grid-gap" => {
            let (row, col) = (lookup(js, n, "row-gap"), lookup(js, n, "column-gap"));
            let (r, c) = (
                non_empty(&row).unwrap_or("normal"),
                non_empty(&col).unwrap_or("normal"),
            );
            Some(if r == c { r.into() } else { format!("{r} {c}") })
        }
        "grid-area" | "grid-row" | "grid-column" => {
            let parts = [
                "grid-row-start",
                "grid-column-start",
                "grid-row-end",
                "grid-column-end",
            ];
            let area = name.as_bytes()[5] == b'a';
            let column = name.as_bytes()[5] == b'c';
            let mut values: [Option<String>; 4] = Default::default();
            for (i, slot) in values.iter_mut().take(if area { 4 } else { 2 }).enumerate() {
                let part = if area { i } else { i * 2 + usize::from(column) };
                *slot = lookup(js, n, parts[part]);
            }
            ffi::grid_placement_compose(&values, area)
        }
        "grid-template" | "grid" => {
            let parts = [
                "grid-template-rows",
                "grid-template-columns",
                "grid-template-areas",
                "grid-auto-flow",
                "grid-auto-rows",
                "grid-auto-columns",
            ];
            ffi::flush_style(js);
            let Some(grid_style) = ffi::style_of(js, n) else {
                return Some(Some(String::new()));
            };
            let full = name == "grid";
            let mut values: [Option<String>; 6] = Default::default();
            for (i, slot) in values.iter_mut().take(if full { 6 } else { 3 }).enumerate() {
                let tracks = match i {
                    0 => grid_style.get(PropId::GridTemplateRows),
                    1 => grid_style.get(PropId::GridTemplateColumns),
                    _ => None,
                };
                *slot = match tracks.filter(|t| is_tracks(*t)) {
                    Some(t) => ffi::serialize(t),
                    None => lookup(js, n, parts[i]),
                };
            }
            ffi::grid_shorthand_compose(&values, full)
        }
        _ => return None,
    })
}

const KIND_TRACKS: u32 = 7;

fn is_tracks(v: ValueRef<'_>) -> bool {
    unsafe { *v.as_ptr().cast::<u32>() == KIND_TRACKS }
}

fn str_of(c: Option<&core::ffi::CStr>) -> Option<&str> {
    c.and_then(|c| c.to_str().ok())
}

pub(crate) fn lookup(js: Js, n: Node<'_>, name: &str) -> Option<String> {
    let name = if name == "css-float" || name == "cssFloat" {
        "float"
    } else {
        name
    };
    if let Some(value) = shorthand(js, n, name) {
        return value;
    }

    ffi::flush_layout(js);
    let style_attr = str_of(n.attr(c"style"));
    let has_style_attr = style_attr.is_some_and(|s| !s.is_empty());
    let lbox = ffi::find_box(js, n);
    let computed = ffi::style_of(js, n).or_else(|| lbox.and_then(box_style));

    if let Some(b) = lbox
        && (name == "grid-template-columns" || name == "grid-template-rows")
        && let Some(tracks) = ffi::grid_resolved_tracks(b, name == "grid-template-columns")
    {
        return Some(tracks);
    }
    let tracks_id = ffi::prop_id(name);
    if let Some(c) = computed
        && let Some(id) = tracks_id
        && [
            PropId::GridTemplateColumns,
            PropId::GridTemplateRows,
            PropId::GridAutoColumns,
            PropId::GridAutoRows,
        ]
        .iter()
        .any(|&p| p as usize == id)
    {
        let mut root = n;
        while let Some(p) = root.parent().filter(|p| p.is_element()) {
            root = p;
        }
        let root_style = ffi::style_of(js, root);
        if let Some(tracks) = ffi::tracks_computed_serialize(c, root_style, id) {
            return Some(tracks);
        }
    }

    if (name == "width" || name == "height")
        && let Some(b) = lbox
    {
        let is_width = name == "width";
        let mut v = if is_width {
            b.content_width()
        } else {
            b.content_height()
        };
        let border_box = computed
            .and_then(|c| c.keyword_of(PropId::BoxSizing))
            .is_some_and(|k| k.to_bytes() == b"border-box");
        if border_box {
            let (p, br) = (b.padding(), b.border());
            v += if is_width {
                p.left + p.right + br.left + br.right
            } else {
                p.top + p.bottom + br.top + br.bottom
            };
        }
        if v < 0.0 {
            v = 0.0;
        }
        return Some(px(v));
    }

    if let Some(b) = lbox
        && let Some(edge) = box_edge_px(b, name)
    {
        return Some(edge);
    }

    if name == "transform" {
        return Some(transform_matrix(computed, lbox));
    }

    if name == "direction" {
        if let Some(v) = computed.and_then(|c| c.get(PropId::Direction)) {
            return ffi::serialize(v);
        }
        return Some(ffi::node_dir(n));
    }

    if let Some(b) = lbox
        && matches!(name, "top" | "right" | "bottom" | "left")
        && let Some(inset) = inset_px(js, n, b, name)
    {
        return Some(inset);
    }

    if name == "line-height"
        && let Some(lh) = line_height_px(js, n, computed)
    {
        return Some(lh);
    }

    if name == "font-weight"
        && let Some(w) = computed.and_then(|c| c.get(PropId::FontWeight))
    {
        return Some(w.font_weight_or(400).to_string());
    }

    if name.len() > 2 && name.starts_with("--") {
        if ffi::has_style_table(js)
            && let Some(s) = ffi::style_of(js, n)
            && s.has_vars()
        {
            let key = ffi::c_string(name.as_bytes());
            if let Some(v) = s.var(&key) {
                return (!v.to_bytes().eq_ignore_ascii_case(b"initial"))
                    .then(|| v.to_string_lossy().into_owned());
            }
        }
        if has_style_attr && let Some(v) = ffi::inline_style_get(n.attr(c"style"), name) {
            return Some(v);
        }
        return None;
    }

    if (name.starts_with("transition") || name.starts_with("animation"))
        && let Some(v) = anim_lookup(js, n, name)
    {
        return Some(v);
    }
    if name == "overflow" {
        let (x, y) = (lookup(js, n, "overflow-x"), lookup(js, n, "overflow-y"));
        if let (Some(x), Some(y)) = (x, y) {
            return Some(if x == y { x } else { format!("{x} {y}") });
        }
    }
    if name == "overflow-clip-margin"
        && let Some(kw) =
            keyword_of(ffi::style_of(js, n).and_then(|s| s.get(PropId::OverflowClipMargin)))
    {
        return Some(range_list(js, n, kw));
    }
    if name == "list-style" {
        let kind = lookup(js, n, "list-style-type");
        let pos = lookup(js, n, "list-style-position");
        let img = lookup(js, n, "list-style-image");
        if let (Some(kind), Some(pos), Some(img)) = (kind, pos, img)
            && let Some(r) = ffi::list_style_serialize(&kind, &pos, &img)
        {
            return Some(r);
        }
    }

    let pid = ffi::prop_id(name);
    let is = |p: PropId| pid == Some(p as usize);
    if (is(PropId::Overflow) || is(PropId::OverflowX) || is(PropId::OverflowY))
        && let Some(c) = computed
    {
        let x = ffi::overflow_keyword(c, PropId::OverflowX as usize);
        let y = ffi::overflow_keyword(c, PropId::OverflowY as usize);
        if is(PropId::OverflowX) {
            return Some(x);
        }
        if is(PropId::OverflowY) {
            return Some(y);
        }
        return Some(if x == y { x } else { format!("{x} {y}") });
    }
    if let Some(id) = pid.filter(|_| is(PropId::MinWidth) || is(PropId::MinHeight)) {
        let minimum = computed.and_then(|c| c.value_at(id));
        let is_auto = match minimum {
            None => true,
            Some(m) => m.kind() == Kind::Keyword && keyword_of(Some(m)) == Some("auto"),
        };
        if is_auto {
            if ffi::has_style_table(js) {
                for ancestor in southstar_dom::ancestors_and_self(n) {
                    if display_of(ffi::style_of(js, ancestor)).is_none() {
                        return Some("0px".into());
                    }
                }
            }
            let mut preserve = false;
            if has_style_attr
                && let Some(aspect) = ffi::inline_style_get(n.attr(c"style"), "aspect-ratio")
                && !aspect.is_empty()
                && !aspect.eq_ignore_ascii_case("auto")
            {
                preserve = true;
            }
            if !preserve && let Some(ratio) = computed.and_then(|c| c.get(PropId::AspectRatio)) {
                preserve =
                    !(ratio.kind() == Kind::Keyword && keyword_of(Some(ratio)) == Some("auto"));
            }
            if !preserve
                && ffi::has_style_table(js)
                && let Some(parent) = n.parent()
            {
                let pd = display_of(ffi::style_of(js, parent));
                preserve = pd.box_ == DISPLAY_BOX_NORMAL
                    && pd.internal == 0
                    && (pd.inner == DISPLAY_INNER_FLEX || pd.inner == DISPLAY_INNER_GRID);
            }
            return Some((if preserve { "auto" } else { "0px" }).into());
        }
    }
    if is(PropId::Opacity)
        && ffi::has_style_table(js)
        && let Some((o, UNIT_NUMBER)) = ffi::style_of(js, n)
            .and_then(|s| s.get(PropId::Opacity))
            .and_then(ValueRef::length)
    {
        let o = if o.is_nan() || o < 0.0 {
            0.0
        } else {
            o.min(1.0)
        };
        return Some(fmt_g(o));
    }
    if let Some(id) =
        pid.filter(|_| is(PropId::Scale) || is(PropId::Rotate) || is(PropId::Translate))
    {
        if let Some(v) = ffi::style_of(js, n).and_then(|s| s.value_at(id))
            && let Some(r) = ffi::individual_transform_serialize(v, id)
        {
            return Some(r);
        }
        return Some("none".into());
    }
    let canonical = match pid {
        Some(id) => ffi::prop_name(id).unwrap_or(name),
        None => name,
    };
    if let Some(id) = pid
        && ffi::has_style_table(js)
    {
        let s = ffi::style_of(js, n);
        if let Some(v) = s.and_then(|s| s.value_at(id)) {
            return ffi::serialize(v);
        }
        if s.is_some()
            && let Some(initial) = ffi::initial_value(canonical)
        {
            return Some(initial.into());
        }
    }

    if has_style_attr && let Some(v) = ffi::inline_style_get(n.attr(c"style"), name) {
        return Some(v);
    }

    if let (true, Some(id), Some(style)) = (has_style_attr, pid, style_attr) {
        let found = ffi::with_sheet_declarations(&format!("* {{ {style} }}"), |rules| {
            rules.iter().find_map(|decls| {
                decls
                    .iter()
                    .find(|d| d.prop as usize == id && !d.value.is_null())
                    .map(|d| ffi::serialize_raw(d.value))
            })
        });
        if let Some(Some(result)) = found {
            return Some(result);
        }
    }
    ffi::initial_value(canonical).map(str::to_string)
}

fn pseudo_substyle<'a>(s: StyleRef<'a>, pseudo: &str) -> Option<StyleRef<'a>> {
    match pseudo {
        "before" => s.before().or_else(|| s.hidden_before()),
        "after" => s.after().or_else(|| s.hidden_after()),
        "marker" => s.marker(),
        "first-line" => s.first_line(),
        "first-letter" => s.first_letter(),
        "placeholder" => s.placeholder(),
        "selection" => s.selection(),
        "backdrop" => s.backdrop(),
        "file-selector-button" => s.file_selector_button(),
        _ => None,
    }
}

pub(crate) fn lookup_pseudo(js: Js, n: Node<'_>, pseudo: &str, name: &str) -> Option<String> {
    ffi::flush_layout(js);
    let base = ffi::style_of(js, n);
    let ps = base.and_then(|b| pseudo_substyle(b, pseudo));
    let Some(ps) = ps else {
        if pseudo == "before" || pseudo == "after" {
            if name == "display"
                && let Some(d) = base.and_then(|b| b.get(PropId::Display))
                && ffi::serialize(d).is_some_and(|d| d.contains("flex") || d.contains("grid"))
            {
                return Some("block".into());
            }
            return ffi::initial_value(name).map(str::to_string);
        }
        return None;
    };
    let name = if name == "css-float" || name == "cssFloat" {
        "float"
    } else {
        name
    };
    if name.len() > 2 && name.starts_with("--") {
        let key = ffi::c_string(name.as_bytes());
        return ps
            .var(&key)
            .filter(|v| !v.to_bytes().eq_ignore_ascii_case(b"initial"))
            .map(|v| v.to_string_lossy().into_owned());
    }
    if name == "font-weight"
        && let Some(w) = ps.get(PropId::FontWeight)
    {
        return Some(w.font_weight_or(400).to_string());
    }
    let pid = ffi::prop_id(name);
    let sizing = pid == Some(PropId::Width as usize) || pid == Some(PropId::Height as usize);
    if sizing && display_of(Some(ps)).box_ == DISPLAY_BOX_CONTENTS {
        return Some("auto".into());
    }
    if sizing
        && let Some(id) = pid
        && let Some((v, UNIT_PERCENT)) = ps.value_at(id).and_then(ValueRef::length)
    {
        let found = southstar_dom::ancestors_and_self(n).find_map(|a| ffi::find_box(js, a));
        if let Some(b) = found {
            let basis = if id == PropId::Width as usize {
                b.content_width()
            } else {
                b.content_height()
            };
            return Some(px(v * basis / 100.0));
        }
    }
    if let Some(id) = pid
        && let Some(v) = ps.value_at(id)
    {
        let value = ffi::serialize(v);
        if id == PropId::Content as usize
            && let Some(text) = &value
            && !text.is_empty()
            && !text.starts_with('\'')
            && !text.starts_with('"')
            && text != "none"
            && text != "normal"
        {
            return Some(format!("\"{text}\""));
        }
        return value;
    }
    ffi::initial_value(name).map(str::to_string)
}
