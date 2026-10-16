//! Southstar — sizing image, video and svg boxes: used sizes from width, height, aspect-ratio and the natural size, auto and cyclic percentage widths, and measuring the atomic inlines of a run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxKind, BoxRef, Edges, Style};
use southstar_layout_float::{float_side_of, style_is_grid_container};
use southstar_style::{Display, Kind, PropId, ValueRef, display_of};

use crate::ffi::{
    aspect_ratio_number, box_is_abs_placeholder, container_w, containing_block_definite_height,
    dom_of, edges_into_box, edges_of, flex_direction_of, height_keyword_stretches,
    image_dimension_attr, image_of, layout_box, length_resolve, resolve_height_with_basis,
    resolve_used_height, style_is_absolute_or_fixed, style_is_flex_container, style_of,
    svg_intrinsic_size, value_is_percent, viewport_w,
};
use crate::intrinsic::{measure_natural_width, min_width_of, size_keyword_is_intrinsic};

const MEDIA_SRC_ATTR: &core::ffi::CStr = c"data-nd-media-src";
const MEDIA_STREAM_ATTR: &core::ffi::CStr = c"data-nd-media-stream";

const BOX_NORMAL: u8 = 0;
const OUTER_INLINE: u8 = 0;
const INNER_FLOW: u8 = 0;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;

pub fn is_replaced(b: BoxRef<'_>) -> bool {
    matches!(b.kind(), BoxKind::Image | BoxKind::Video | BoxKind::Svg)
}

pub fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

pub fn style_value(b: BoxRef<'_>, prop: PropId) -> Option<ValueRef<'_>> {
    style_of(b).and_then(|s| s.get(prop))
}

pub fn is_border_box(b: BoxRef<'_>) -> bool {
    style_value(b, PropId::BoxSizing).is_some_and(|v| v.is_keyword(c"border-box"))
}

fn display_is_atomic_inline(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
}

fn node_has_media_metadata(n: Option<Node<'_>>) -> bool {
    n.is_some_and(|n| {
        n.is_element() && (n.attr(MEDIA_SRC_ATTR).is_some() || n.attr(MEDIA_STREAM_ATTR).is_some())
    })
}

fn flex_row_direction(dir: &[u8]) -> bool {
    dir == b"row" || dir == b"row-reverse"
}

pub fn layout_image(b: BoxRef<'_>, parent_content_width: f64) {
    edges_into_box(b, parent_content_width);
    let wv = style_value(b, PropId::Width);
    let hv = style_value(b, PropId::Height);
    let mxw = style_value(b, PropId::MaxWidth);
    let mxh = style_value(b, PropId::MaxHeight);
    let mnw = style_value(b, PropId::MinWidth);
    let mnh = style_value(b, PropId::MinHeight);

    let media = b.media();
    let declared_size = media.is_some_and(|m| m.declared_image_size());
    let placeholder_size = media.is_some_and(|m| m.placeholder_image_size());
    let flex_row_item = b.parent().is_some_and(|p| {
        let dir = flex_direction_of(p.style());
        style_is_flex_container(p.style()) && flex_row_direction(dir)
    });
    let mut pct_width_base = parent_content_width;
    if flex_row_item && let Some(p) = b.parent().filter(|p| p.content_width() > 0.0) {
        pct_width_base = p.content_width();
    }
    let (margin, padding, border) = (b.margin(), b.padding(), b.border());
    let mut w = -1.0;
    let mut h = -1.0;
    if is_length_or_calc(wv) {
        w = length_resolve(wv, pct_width_base, -1.0);
    } else if height_keyword_stretches(wv) {
        w = parent_content_width
            - margin.left
            - margin.right
            - padding.left
            - padding.right
            - border.left
            - border.right;
        if w < 0.0 {
            w = 0.0;
        }
    }
    if is_length_or_calc(hv) {
        if value_is_percent(hv) {
            let cb_h = containing_block_definite_height(b);
            if cb_h >= 0.0
                && let Some(hv) = hv
            {
                h = match hv.calc() {
                    Some((pct, px)) => pct / 100.0 * cb_h + px,
                    None => hv.length().map_or(0.0, |(v, _)| v) * cb_h / 100.0,
                };
            }
        } else {
            h = resolve_used_height(b, hv, parent_content_width, -1.0);
        }
    } else if height_keyword_stretches(hv) {
        let cb_h = containing_block_definite_height(b);
        if cb_h >= 0.0 {
            h = cb_h
                - margin.top
                - margin.bottom
                - padding.top
                - padding.bottom
                - border.top
                - border.bottom;
            if h < 0.0 {
                h = 0.0;
            }
        }
    }

    let img = media.and_then(image_of);
    let density = media
        .map(|m| m.image_density())
        .filter(|&d| d > 0.0)
        .unwrap_or(1.0);
    let loaded = img.filter(|i| i.loaded());
    let mut nat_w = loaded
        .map(|i| i.natural_size().0)
        .filter(|&n| n > 0)
        .map_or(-1.0, |n| f64::from(n) / density);
    let mut nat_h = loaded
        .map(|i| i.natural_size().1)
        .filter(|&n| n > 0)
        .map_or(-1.0, |n| f64::from(n) / density);
    if declared_size {
        if nat_w < 0.0 && b.content_width() > 0.0 {
            nat_w = b.content_width();
        }
        if nat_h < 0.0 && b.content_height() > 0.0 {
            nat_h = b.content_height();
        }
    }
    let mut intrinsic_ratio = if nat_w > 0.0 && nat_h > 0.0 {
        nat_w / nat_h
    } else {
        -1.0
    };
    if b.kind() == BoxKind::Svg && dom_of(b).is_some() {
        let svg_size = svg_intrinsic_size(dom_of(b));
        if svg_size.has_ratio() && svg_size.ratio > 0.0 {
            intrinsic_ratio = svg_size.ratio;
        }
    }
    let (specified_ratio, ratio_with_auto) = match style_of(b) {
        Some(s) => aspect_ratio_number(s.get(PropId::AspectRatio)),
        None => (-1.0, false),
    };
    let ratio_overrides = specified_ratio > 0.0 && (!ratio_with_auto || intrinsic_ratio <= 0.0);
    if ratio_overrides {
        intrinsic_ratio = specified_ratio;
    }
    if let Some(m) = media {
        m.set_size_independent_of_image(
            (w >= 0.0 && h >= 0.0) || declared_size || placeholder_size,
        );
    }

    let metadata_video = b.kind() == BoxKind::Video && node_has_media_metadata(dom_of(b));
    let video_without_metadata = b.kind() == BoxKind::Video
        && !metadata_video
        && dom_of(b).is_some_and(|d| d.element_name() == Some(b"video".as_slice()));
    if nat_w < 0.0 && b.content_width() > 0.0 {
        nat_w = b.content_width();
    }
    if nat_h < 0.0 && b.content_height() > 0.0 {
        nat_h = b.content_height();
    }

    let ratio_only = media.is_some_and(|m| m.intrinsic_ratio_only());
    let w_specified = w >= 0.0;
    let h_specified = h >= 0.0;
    if w < 0.0 && h < 0.0 {
        if ratio_only && intrinsic_ratio > 0.0 {
            let cb_h = containing_block_definite_height(b);
            if parent_content_width > 0.0 && cb_h >= 0.0 {
                w = parent_content_width;
                h = cb_h;
            } else if cb_h >= 0.0 {
                h = cb_h;
                w = h * intrinsic_ratio;
            } else if parent_content_width > 0.0 {
                w = parent_content_width;
                h = w / intrinsic_ratio;
            } else {
                w = 0.0;
                h = 0.0;
            }
        } else if nat_w > 0.0 && nat_h > 0.0 {
            w = nat_w;
            h = if ratio_overrides && !ratio_with_auto {
                w / intrinsic_ratio
            } else {
                nat_h
            };
        } else {
            w = 0.0;
            h = 0.0;
        }
    } else if w < 0.0 {
        w = if intrinsic_ratio > 0.0 {
            h * intrinsic_ratio
        } else if video_without_metadata {
            300.0
        } else {
            h
        };
    } else if h < 0.0 {
        h = if intrinsic_ratio > 0.0 {
            w / intrinsic_ratio
        } else if video_without_metadata {
            150.0
        } else {
            w
        };
    }
    if metadata_video && h <= 0.0 && w > 0.0 && nat_w > 0.0 && nat_h > 0.0 {
        h = w * (nat_h / nat_w);
    }
    if metadata_video && w <= 0.0 && h > 0.0 && nat_w > 0.0 && nat_h > 0.0 {
        w = h * (nat_w / nat_h);
    }

    let mut max_w = length_resolve(mxw, pct_width_base, -1.0);
    let max_h = resolve_used_height(b, mxh, parent_content_width, -1.0);
    let mut min_w = length_resolve(mnw, pct_width_base, -1.0);
    let min_h = resolve_used_height(b, mnh, parent_content_width, -1.0);
    let keyword_w = if intrinsic_ratio > 0.0 && h_specified {
        h * intrinsic_ratio
    } else {
        nat_w
    };
    if max_w < 0.0 && size_keyword_is_intrinsic(mxw) && keyword_w >= 0.0 {
        max_w = keyword_w;
    }
    if min_w < 0.0 && size_keyword_is_intrinsic(mnw) && keyword_w >= 0.0 {
        min_w = keyword_w;
    }

    if max_w >= 0.0 && w > max_w {
        if h > 0.0 && w > 0.0 && !h_specified {
            h *= max_w / w;
        }
        w = max_w;
    }
    if max_h >= 0.0 && h > max_h {
        if w > 0.0 && h > 0.0 && !w_specified {
            w *= max_h / h;
        }
        h = max_h;
    }
    if min_w >= 0.0 && w < min_w {
        if h > 0.0 && w > 0.0 && !h_specified && intrinsic_ratio > 0.0 {
            h = min_w / intrinsic_ratio;
        }
        w = min_w;
    }
    if min_h >= 0.0 && h < min_h {
        if w > 0.0 && h > 0.0 && !w_specified && intrinsic_ratio > 0.0 {
            w = min_h * intrinsic_ratio;
        }
        h = min_h;
    }

    b.set_content_width(w);
    b.set_content_height(h);
}

fn inline_atomic_needs_layout(ab: BoxRef<'_>) -> bool {
    !box_is_abs_placeholder(ab)
}

pub fn inline_atomic_measure_basis(b: BoxRef<'_>) -> f64 {
    let wv = style_value(b, PropId::Width);
    if value_is_percent(wv) && !is_replaced(b) {
        let content = measure_natural_width(b, b.style());
        if content >= 0.0 {
            return content;
        }
    }
    let mut basis = container_w();
    if basis.is_nan() || basis <= 0.0 {
        let mut p = b.parent();
        while let Some(anc) = p {
            if anc.content_width() > 0.0 {
                basis = anc.content_width();
                break;
            }
            p = anc.parent();
        }
    }
    if basis.is_nan() || basis <= 0.0 {
        basis = viewport_w();
    }
    if basis.is_nan() || basis <= 0.0 {
        basis = 1000.0;
    }
    basis.clamp(32.0, 1600.0)
}

fn box_inline_size_is_definite(b: BoxRef<'_>) -> bool {
    let mut cur = Some(b);
    while let Some(bx) = cur {
        cur = bx.parent();
        if bx.kind() == BoxKind::TableCell {
            return false;
        }
        let Some(s) = style_of(bx) else {
            continue;
        };
        let wv = s.get(PropId::Width);
        if is_length_or_calc(wv) && !value_is_percent(wv) {
            return true;
        }
        if style_is_absolute_or_fixed(s.as_ptr())
            || float_side_of(Some(s)) >= 0
            || display_is_atomic_inline(display_of(Some(s)))
        {
            return false;
        }
        if let Some(p) = bx.parent()
            && (style_is_flex_container(p.style()) || style_is_grid_container(style_of(p)))
        {
            return false;
        }
    }
    true
}

fn replaced_height_for_width(b: BoxRef<'_>) -> f64 {
    let hv = style_value(b, PropId::Height);
    if !is_length_or_calc(hv) {
        return -1.0;
    }
    let mut h = if !value_is_percent(hv) {
        length_resolve(hv, 0.0, -1.0)
    } else {
        let cb_h = containing_block_definite_height(b);
        if cb_h >= 0.0 {
            resolve_height_with_basis(hv, 0.0, cb_h, -1.0)
        } else {
            -1.0
        }
    };
    if h > 0.0 && is_border_box(b) {
        let (_, pd, bd) = edges_of(b.style(), 0.0);
        h -= pd.top + pd.bottom + bd.top + bd.bottom;
        if h < 0.0 {
            h = 0.0;
        }
    }
    h
}

fn replaced_containing_box(b: BoxRef<'_>) -> Option<BoxRef<'_>> {
    let mut p = b.parent();
    while let Some(anc) = p {
        if anc.kind() != BoxKind::Inline {
            return Some(anc);
        }
        p = anc.parent();
    }
    None
}

fn replaced_containing_block_is_definite(b: BoxRef<'_>) -> bool {
    if b.parent()
        .is_some_and(|p| style_is_grid_container(style_of(p)))
    {
        return false;
    }
    replaced_containing_box(b).is_some_and(box_inline_size_is_definite)
}

fn ratio_only_replaced_width(b: BoxRef<'_>, max_content: bool) -> f64 {
    let h = replaced_height_for_width(b);
    if h > 0.0 {
        let size = svg_intrinsic_size(dom_of(b));
        if size.has_ratio() && size.ratio > 0.0 {
            return h * size.ratio;
        }
    }
    if max_content
        && replaced_containing_block_is_definite(b)
        && let Some(cb) = replaced_containing_box(b)
        && cb.content_width() > 0.0
    {
        return cb.content_width();
    }
    0.0
}

fn loaded_image_auto_width(b: BoxRef<'_>, natural: (i32, i32), density: f64) -> f64 {
    let h = replaced_height_for_width(b);
    if h > 0.0 {
        return h * f64::from(natural.0) / f64::from(natural.1);
    }
    let density = if density > 0.0 { density } else { 1.0 };
    f64::from(natural.0) / density
}

fn svg_auto_width(b: BoxRef<'_>) -> f64 {
    let size = svg_intrinsic_size(dom_of(b));
    let h = replaced_height_for_width(b);
    if h > 0.0 && size.has_ratio() && size.ratio > 0.0 {
        return h * size.ratio;
    }
    if size.has_width() && size.width > 0.0 {
        size.width
    } else {
        300.0
    }
}

fn width_attribute_px(dom: Option<Node<'_>>) -> f64 {
    let Some(attr) = dom.and_then(|d| d.attr(c"width")) else {
        return 0.0;
    };
    if attr.to_bytes().contains(&b'%') {
        return 0.0;
    }
    image_dimension_attr(dom, c"width")
}

fn replaced_auto_width(b: BoxRef<'_>) -> f64 {
    let media = b.media();
    if media.is_some_and(|m| m.intrinsic_ratio_only()) {
        let w = ratio_only_replaced_width(b, false);
        return if w > 0.0 { w } else { 300.0 };
    }
    if let Some(m) = media
        && let Some(img) = image_of(m)
        && img.loaded()
    {
        let natural = img.natural_size();
        if natural.0 > 0 && natural.1 > 0 {
            return loaded_image_auto_width(b, natural, m.image_density());
        }
    }
    if b.kind() == BoxKind::Svg && dom_of(b).is_some() {
        return svg_auto_width(b);
    }
    let attr = width_attribute_px(dom_of(b));
    if attr > 0.0 {
        return attr;
    }
    if b.kind() == BoxKind::Video {
        return 300.0;
    }
    if media.is_some_and(|m| m.placeholder_image_size()) {
        200.0
    } else {
        0.0
    }
}

pub fn replaced_width_is_cyclic(b: BoxRef<'_>) -> bool {
    if !is_replaced(b) {
        return false;
    }
    let wv = style_value(b, PropId::Width);
    if value_is_percent(wv) {
        return !replaced_containing_block_is_definite(b);
    }
    !is_length_or_calc(wv) && b.media().is_some_and(|m| m.intrinsic_ratio_only())
}

pub fn replaced_width_is_percent(b: BoxRef<'_>) -> bool {
    is_replaced(b) && style_of(b).is_some_and(|s| value_is_percent(s.get(PropId::Width)))
}

pub fn replaced_intrinsic_contribution(b: BoxRef<'_>, max_content: bool) -> f64 {
    let wv = style_value(b, PropId::Width);
    let mut w = if value_is_percent(wv) {
        if max_content {
            replaced_auto_width(b)
        } else {
            0.0
        }
    } else {
        ratio_only_replaced_width(b, max_content)
    };
    let mxw = style_value(b, PropId::MaxWidth);
    let mnw = style_value(b, PropId::MinWidth);
    let max_w = if value_is_percent(mxw) {
        -1.0
    } else {
        length_resolve(mxw, 0.0, -1.0)
    };
    let min_w = if value_is_percent(mnw) {
        -1.0
    } else {
        length_resolve(mnw, 0.0, -1.0)
    };
    if max_w >= 0.0 && w > max_w {
        w = max_w;
    }
    if min_w >= 0.0 && w < min_w {
        w = min_w;
    }
    w
}

pub struct AtomicGeometry<'a> {
    b: BoxRef<'a>,
    width: f64,
    height: f64,
    margin: Edges,
    padding: Edges,
    border: Edges,
}

pub type SavedGeometry<'a> = Vec<AtomicGeometry<'a>>;

fn is_form_control(b: BoxRef<'_>) -> bool {
    dom_of(b)
        .and_then(|n| n.element_name())
        .is_some_and(|name| {
            matches!(
                name,
                b"input" | b"button" | b"select" | b"textarea" | b"meter" | b"progress"
            )
        })
}

fn shrinks_to_min_content(ab: BoxRef<'_>) -> bool {
    ab.kind() == BoxKind::Block
        && style_value(ab, PropId::Width).is_none_or(|v| v.is_keyword(c"auto"))
        && !is_form_control(ab)
        && !box_is_abs_placeholder(ab)
}

fn geometry(ab: BoxRef<'_>) -> AtomicGeometry<'_> {
    AtomicGeometry {
        b: ab,
        width: ab.content_width(),
        height: ab.content_height(),
        margin: ab.margin(),
        padding: ab.padding(),
        border: ab.border(),
    }
}

pub fn measure_inline_atomics_begin<'a>(
    b: BoxRef<'a>,
    parent_style: *const Style,
    max_content: bool,
) -> Option<Box<SavedGeometry<'a>>> {
    b.inline_atomics()?;
    let mut saved: Option<Box<SavedGeometry<'a>>> = None;
    for ab in b.inline_atomic_boxes() {
        let cyclic =
            replaced_width_is_cyclic(ab) || (!max_content && replaced_width_is_percent(ab));
        if cyclic {
            saved.get_or_insert_default().push(geometry(ab));
        }
        if inline_atomic_needs_layout(ab) {
            layout_box(ab, inline_atomic_measure_basis(ab), parent_style);
        }
        if !max_content && !cyclic && shrinks_to_min_content(ab) {
            let min = min_width_of(Some(ab), parent_style);
            if min >= 0.0 && min < ab.content_width() {
                saved.get_or_insert_default().push(geometry(ab));
                ab.set_content_width(min);
            }
        }
        if cyclic {
            let w = replaced_intrinsic_contribution(ab, max_content);
            let ratio = if ab.content_width() > 0.0 && ab.content_height() > 0.0 {
                ab.content_width() / ab.content_height()
            } else {
                0.0
            };
            ab.set_content_width(w);
            if ratio > 0.0 {
                ab.set_content_height(w / ratio);
            }
        }
    }
    saved
}

pub fn measure_inline_atomics_end(saved: SavedGeometry<'_>) {
    for g in saved {
        let b = g.b;
        b.set_content_width(g.width);
        b.set_content_height(g.height);
        b.set_margin(g.margin);
        b.set_padding(g.padding);
        b.set_border(g.border);
    }
}
