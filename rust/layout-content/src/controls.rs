//! Southstar — form-control sizing and chrome, the inline attributes emitted for controls and styled runs, and the meter and progress states.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, Style, inline_kind};
use southstar_layout_geom::{edges_from_style, length_resolve};
pub use southstar_layout_geom::{keyword_is, length_or};
use southstar_style::{Display, Kind, PropId, StyleRef, StyleTable, ValueRef, display_of};

use crate::ffi;

const UNIT_PX: u32 = 0;
const UNIT_EM: u32 = 1;
const UNIT_REM: u32 = 2;
const UNIT_PERCENT: u32 = 3;
const UNIT_NUMBER: u32 = 4;
const UNIT_VW: u32 = 5;
const UNIT_VH: u32 = 6;
const UNIT_VMIN: u32 = 7;
const UNIT_VMAX: u32 = 8;
const UNIT_CQW: u32 = 9;
const UNIT_CQH: u32 = 10;
const UNIT_CQMIN: u32 = 11;
const UNIT_CQMAX: u32 = 12;
const UNIT_EX: u32 = 13;
const UNIT_CH: u32 = 14;
const UNIT_CAP: u32 = 15;
const UNIT_IC: u32 = 16;
const UNIT_LH: u32 = 17;
const UNIT_RLH: u32 = 18;
const UNIT_REX: u32 = 19;
const UNIT_RCH: u32 = 20;
const UNIT_RCAP: u32 = 21;
const UNIT_RIC: u32 = 22;

const BOX_NORMAL: u8 = 0;
const OUTER_INLINE: u8 = 0;
const OUTER_BLOCK: u8 = 1;
const INNER_FLOW: u8 = 0;
const INNER_TABLE: u8 = 2;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_TABLE_CAPTION: u8 = 8;

const OBJECT_REPLACEMENT: &[u8] = "\u{FFFC}".as_bytes();

const BORDER_STYLES: [PropId; 4] = [
    PropId::BorderTopStyle,
    PropId::BorderRightStyle,
    PropId::BorderBottomStyle,
    PropId::BorderLeftStyle,
];

const BORDER_WIDTHS: [PropId; 4] = [
    PropId::BorderTopWidth,
    PropId::BorderRightWidth,
    PropId::BorderBottomWidth,
    PropId::BorderLeftWidth,
];

const BORDER_RADII: [PropId; 5] = [
    PropId::BorderRadius,
    PropId::BorderTopLeftRadius,
    PropId::BorderTopRightRadius,
    PropId::BorderBottomRightRadius,
    PropId::BorderBottomLeftRadius,
];

pub fn get(s: Option<StyleRef<'_>>, prop: PropId) -> Option<ValueRef<'_>> {
    s.and_then(|s| s.get(prop))
}

fn font_size_or_16(s: StyleRef<'_>) -> f64 {
    length_or(s.get(PropId::FontSize), 16.0)
}

pub fn append_inline_spacer(out: &mut Vec<u8>, attrs: &mut Vec<InlineAttr>, width: f64) {
    if width.is_nan() || width <= 0.0 {
        return;
    }
    let start = out.len();
    out.extend_from_slice(OBJECT_REPLACEMENT);
    let mut spacer = InlineAttr::new(inline_kind::SPACER, start, 3);
    spacer.box_w = width;
    attrs.push(spacer);
}

pub fn pad_spaces(s: Option<StyleRef<'_>>, right: bool) -> c_int {
    let Some(s) = s else {
        return 0;
    };
    let mut fs = font_size_or_16(s);
    if fs.is_nan() || fs <= 0.0 {
        fs = 16.0;
    }
    let prop = if right {
        PropId::PaddingRight
    } else {
        PropId::PaddingLeft
    };
    let pad = length_resolve(s.get(prop), fs * 20.0, 0.0);
    let space = fs * 0.25;
    if space.is_nan() || space <= 0.0 || pad <= space {
        return 0;
    }
    (((pad - space) / space + 0.5) as c_int).clamp(0, 12)
}

pub fn emit_attr(attrs: &mut Vec<InlineAttr>, kind: c_uint, start: usize, end: usize) {
    if end > start {
        attrs.push(InlineAttr::new(kind, start, end - start));
    }
}

pub fn emit_attr_styled(
    attrs: &mut Vec<InlineAttr>,
    kind: c_uint,
    start: usize,
    end: usize,
    s: *const Style,
) {
    if end > start {
        attrs.push(InlineAttr::new(kind, start, end - start).with_style(s));
    }
}

pub fn run_at_line_start(out: &[u8]) -> bool {
    let mut i = out.len();
    while i > 0 {
        if out[i - 1] == b' ' {
            i -= 1;
            continue;
        }
        if i >= 2 && out[i - 2] == 0xc2 && out[i - 1] == 0xa0 {
            i -= 2;
            continue;
        }
        break;
    }
    if i == 0 {
        return true;
    }
    if i >= 3 && out[i - 3] == 0xe2 && out[i - 2] == 0x80 && matches!(out[i - 1], 0xa8 | 0xa9) {
        return true;
    }
    out[i - 1] == b'\n'
}

fn container_or_viewport() -> (f64, f64) {
    let (vw, vh) = ffi::viewport();
    let (cw, ch) = ffi::container();
    (
        if cw > 0.0 { cw } else { vw },
        if ch > 0.0 { ch } else { vh },
    )
}

pub fn dim_px_basis(v: Option<ValueRef<'_>>, font_size: f64, basis: f64) -> f64 {
    let Some(v) = v else {
        return 0.0;
    };
    if let Some((pct, px)) = v.calc() {
        let mut out = px;
        if basis > 0.0 {
            out += pct * basis / 100.0;
        }
        return if out > 0.0 { out } else { 0.0 };
    }
    let Some((len, unit)) = v.length() else {
        return 0.0;
    };
    match unit {
        UNIT_PX | UNIT_NUMBER => len,
        UNIT_EM => len * font_size,
        UNIT_REM => len * 16.0,
        UNIT_PERCENT => {
            if basis > 0.0 {
                len * basis / 100.0
            } else {
                0.0
            }
        }
        UNIT_VW => len * ffi::viewport().0 / 100.0,
        UNIT_VH => len * ffi::viewport().1 / 100.0,
        UNIT_VMIN => {
            let (vw, vh) = ffi::viewport();
            len * c_min(vw, vh) / 100.0
        }
        UNIT_VMAX => {
            let (vw, vh) = ffi::viewport();
            len * c_max(vw, vh) / 100.0
        }
        UNIT_CQW => len * container_or_viewport().0 / 100.0,
        UNIT_CQH => len * container_or_viewport().1 / 100.0,
        UNIT_CQMIN => {
            let (cw, ch) = container_or_viewport();
            len * c_min(cw, ch) / 100.0
        }
        UNIT_CQMAX => {
            let (cw, ch) = container_or_viewport();
            len * c_max(cw, ch) / 100.0
        }
        UNIT_EX | UNIT_CH => len * font_size * 0.5,
        UNIT_CAP => len * font_size * 0.7,
        UNIT_IC => len * font_size,
        UNIT_LH => len * font_size * 1.5,
        UNIT_RLH => len * 24.0,
        UNIT_REX | UNIT_RCH => len * 8.0,
        UNIT_RCAP => len * 11.2,
        UNIT_RIC => len * 16.0,
        _ => 0.0,
    }
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn dim_px_clamped(
    s: Option<StyleRef<'_>>,
    props: [PropId; 3],
    font_size: f64,
    basis: f64,
) -> f64 {
    let Some(s) = s else {
        return 0.0;
    };
    let mut out = dim_px_basis(s.get(props[0]), font_size, basis);
    let mn = dim_px_basis(s.get(props[1]), font_size, basis);
    let mx = dim_px_basis(s.get(props[2]), font_size, basis);
    if mn > 0.0 && out > 0.0 && out < mn {
        out = mn;
    }
    if mx > 0.0 && out > mx {
        out = mx;
    }
    out
}

pub const WIDTH_PROPS: [PropId; 3] = [PropId::Width, PropId::MinWidth, PropId::MaxWidth];
const HEIGHT_PROPS: [PropId; 3] = [PropId::Height, PropId::MinHeight, PropId::MaxHeight];

fn is_border_box(dom: Option<Node<'_>>, s: Option<StyleRef<'_>>) -> bool {
    let sizing = get(s, PropId::BoxSizing);
    if keyword_is(sizing, c"border-box") {
        return true;
    }
    if keyword_is(sizing, c"content-box") {
        return false;
    }
    let Some(name) = dom.and_then(Node::name).map(CStr::to_bytes) else {
        return false;
    };
    if name == b"button" || name == b"select" {
        return true;
    }
    if name != b"input" {
        return false;
    }
    let Some(ty) = dom
        .and_then(|d| d.attr(c"type"))
        .map(CStr::to_bytes)
        .filter(|t| !t.is_empty())
    else {
        return false;
    };
    [
        &b"submit"[..],
        b"reset",
        b"button",
        b"checkbox",
        b"radio",
        b"color",
        b"search",
    ]
    .iter()
    .any(|t| ty.eq_ignore_ascii_case(t))
}

pub fn css_extra(dom: Option<Node<'_>>, s: Option<StyleRef<'_>>, vertical: bool) -> f64 {
    let Some(st) = s else {
        return 0.0;
    };
    if is_border_box(dom, s) {
        return 0.0;
    }
    let (_, p, b) = edges_from_style(Some(st), 0.0);
    if vertical {
        p.top + p.bottom + b.top + b.bottom
    } else {
        p.left + p.right + b.left + b.right
    }
}

pub fn inline_attr_control_width(
    r: &InlineAttr,
    s: Option<StyleRef<'_>>,
    dom: Option<Node<'_>>,
    b: Option<BoxRef<'_>>,
) -> f64 {
    let Some(st) = s else {
        return if r.box_w > 0.0 { r.box_w } else { 0.0 };
    };
    let fs = font_size_or_16(st);
    let mut w = dim_px_clamped(s, WIDTH_PROPS, fs, b.map_or(0.0, BoxRef::content_width));
    if w > 0.0 {
        w += css_extra(dom, s, false);
    }
    if w > 0.0 { w } else { r.box_w }
}

fn style_none_or_hidden(v: Option<ValueRef<'_>>) -> bool {
    keyword_is(v, c"none") || keyword_is(v, c"hidden")
}

fn has_visible_control_box(s: StyleRef<'_>) -> bool {
    if s.get(PropId::BackgroundColor)
        .and_then(ValueRef::color)
        .is_some_and(|c| c[3] > 0)
    {
        return true;
    }
    if s.get(PropId::BoxShadow)
        .and_then(ValueRef::shadows)
        .is_some_and(|sh| sh.n > 0)
    {
        return true;
    }
    if s.get(PropId::BackgroundImage)
        .is_some_and(|v| matches!(v.kind(), Kind::Url | Kind::Gradient))
    {
        return true;
    }
    if BORDER_RADII.iter().any(|&p| length_or(s.get(p), 0.0) > 0.0) {
        return true;
    }
    BORDER_WIDTHS
        .iter()
        .zip(BORDER_STYLES)
        .any(|(&w, st)| length_or(s.get(w), 0.0) > 0.0 && !style_none_or_hidden(s.get(st)))
}

fn strips_chrome(s: StyleRef<'_>) -> bool {
    BORDER_STYLES
        .iter()
        .all(|&p| style_none_or_hidden(s.get(p)))
}

pub fn is_block_level(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.outer == OUTER_BLOCK
        && d.inner != INNER_RUBY
        && (d.internal == INTERNAL_NONE || d.internal == INTERNAL_TABLE_CAPTION)
}

pub fn is_atomic_inline_container(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
        && d.inner != INNER_TABLE
}

fn prefers_css_chrome(kind: c_uint, dom: Option<Node<'_>>, s: Option<StyleRef<'_>>) -> bool {
    let (Some(dom), Some(s)) = (dom, s) else {
        return false;
    };
    if keyword_is(s.get(PropId::Appearance), c"none")
        || strips_chrome(s)
        || has_visible_control_box(s)
    {
        return true;
    }
    if dom.attr(c"class").is_none() {
        return false;
    }
    let d = display_of(Some(s));
    if is_block_level(d) || is_atomic_inline_container(d) {
        return matches!(
            kind,
            inline_kind::INPUT_FIELD | inline_kind::INPUT_FIELD_FOCUSED | inline_kind::BUTTON
        );
    }
    false
}

pub fn atoi(s: Option<&CStr>, fallback: c_int) -> c_int {
    let Some(s) = s.map(CStr::to_bytes) else {
        return fallback;
    };
    let mut i = s
        .iter()
        .take_while(|&&c| matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .count();
    let negative = s.get(i) == Some(&b'-');
    if matches!(s.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let mut v: i64 = 0;
    while let Some(d) = s.get(i).filter(|c| c.is_ascii_digit()) {
        v = v.wrapping_mul(10).wrapping_add(i64::from(d - b'0'));
        i += 1;
    }
    (if negative { v.wrapping_neg() } else { v }) as c_int
}

pub fn emit_form_attr_sized(
    attrs: &mut Vec<InlineAttr>,
    kind: c_uint,
    start: usize,
    end: usize,
    dom: Option<Node<'_>>,
    styles: StyleTable,
) {
    if end <= start {
        return;
    }
    let (mut bw, mut bh) = (0.0, 0.0);
    let mut s = None;
    let mut fs = 16.0;
    let mut bg_src: *const c_char = ptr::null();
    let mut bg_image = ptr::null_mut();
    if let Some(d) = dom.filter(|_| !styles.as_ptr().is_null()) {
        s = styles.get(d);
        if let Some(st) = s {
            fs = font_size_or_16(st);
            bw = dim_px_clamped(s, WIDTH_PROPS, fs, 0.0);
            bh = dim_px_clamped(s, HEIGHT_PROPS, fs, 0.0);
            if bw > 0.0 {
                bw += css_extra(dom, s, false);
            }
            if bh > 0.0 {
                bh += css_extra(dom, s, true);
            }
            if let Some(url) = st.get(PropId::BackgroundImage).and_then(ValueRef::url) {
                bg_src = url.as_ptr();
                bg_image = ffi::peek_image(url);
            }
        }
    }
    if let Some(d) = dom.filter(|d| d.name() == Some(c"textarea")) {
        if bw <= 0.0 {
            let mut c = atoi(d.attr(c"cols"), 20);
            if c <= 0 {
                c = 20;
            }
            bw = f64::from(c) * (fs * 0.5) + 8.0;
        }
        if bh <= 0.0 {
            let mut r = atoi(d.attr(c"rows"), 2);
            if r <= 0 {
                r = 2;
            }
            let line_h = fs * 1.3;
            bh = f64::from(r) * line_h + 6.0;
        }
    }
    let mut a = InlineAttr::new(kind, start, end - start)
        .with_dom(Node::ptr_or_null(dom).cast())
        .with_style(s.map_or(ptr::null(), StyleRef::as_ptr))
        .with_bg_image(bg_src, bg_image);
    a.box_w = bw;
    a.box_h = bh;
    a.native_chrome = southstar_glib::boolean(!prefers_css_chrome(kind, dom, s));
    attrs.push(a);
}

pub fn emit_control_text_style(
    attrs: &mut Vec<InlineAttr>,
    s: Option<StyleRef<'_>>,
    field: (usize, usize),
    value: (usize, usize),
    is_placeholder: bool,
    disabled: bool,
) {
    let (field_start, field_end) = field;
    let (val_start, val_end) = value;
    if field_end > field_start {
        let ifs = s.map_or(0.0, |s| length_or(s.get(PropId::FontSize), 0.0));
        if ifs > 0.0 {
            let mut a =
                InlineAttr::new(inline_kind::FONT_SIZE, field_start, field_end - field_start);
            a.font_size_px = ifs;
            attrs.push(a);
        }
        if let Some(fam) = get(s, PropId::FontFamily).and_then(ValueRef::keyword_text) {
            attrs.push(
                InlineAttr::new(
                    inline_kind::FONT_FAMILY,
                    field_start,
                    field_end - field_start,
                )
                .with_family(fam.as_ptr()),
            );
        }
    }
    if val_end <= val_start {
        return;
    }
    let phs = if is_placeholder {
        s.and_then(StyleRef::placeholder)
    } else {
        None
    };
    let mut rgba = [0u8, 0, 0, 255];
    let mut have_color = false;
    if is_placeholder {
        rgba = get(phs, PropId::Color)
            .and_then(ValueRef::color)
            .unwrap_or([0x75, 0x75, 0x75, 255]);
        have_color = true;
    } else if let Some(c) = get(s, PropId::Color).and_then(ValueRef::color) {
        rgba = c;
        have_color = true;
    }
    if disabled {
        rgba = [0x82, 0x82, 0x82, 255];
        have_color = true;
    }
    if let Some(ph) = phs {
        if let Some((op, _)) = ph.get(PropId::Opacity).and_then(ValueRef::length) {
            rgba[3] = (f64::from(rgba[3]) * op.clamp(0.0, 1.0)).round() as u8;
        }
        let vis = ph.get(PropId::Visibility);
        if keyword_is(vis, c"hidden") || keyword_is(vis, c"collapse") {
            rgba[3] = 0;
        }
    }
    if have_color {
        emit_color(attrs, val_start, val_end, rgba);
    }
    if let Some(ph) = phs {
        let fst = ph.get(PropId::FontStyle);
        if fst.is_some_and(ValueRef::font_slanted) {
            emit_attr(attrs, inline_kind::ITALIC, val_start, val_end);
        }
        let phw = ph
            .get(PropId::FontWeight)
            .map_or(-1, |v| v.font_weight_or(-1));
        if phw > 0 {
            let mut a = InlineAttr::new(inline_kind::FONT_WEIGHT, val_start, val_end - val_start);
            a.font_weight = phw;
            attrs.push(a);
        }
    }
}

pub fn emit_font_size(attrs: &mut Vec<InlineAttr>, start: usize, end: usize, font_size_px: f64) {
    if end > start {
        let mut a = InlineAttr::new(inline_kind::FONT_SIZE, start, end - start);
        a.font_size_px = font_size_px;
        attrs.push(a);
    }
}

pub fn emit_font_weight(attrs: &mut Vec<InlineAttr>, start: usize, end: usize, font_weight: c_int) {
    if end > start && font_weight > 0 {
        let mut a = InlineAttr::new(inline_kind::FONT_WEIGHT, start, end - start);
        a.font_weight = font_weight;
        attrs.push(a);
    }
}

pub fn emit_font_stretch(
    attrs: &mut Vec<InlineAttr>,
    start: usize,
    end: usize,
    font_stretch: c_int,
) {
    if end > start {
        let mut a = InlineAttr::new(inline_kind::FONT_STRETCH, start, end - start);
        a.font_stretch = font_stretch;
        attrs.push(a);
    }
}

pub fn emit_font_features(
    attrs: &mut Vec<InlineAttr>,
    start: usize,
    end: usize,
    kerning: c_int,
    ligatures: *const c_char,
    features: *const c_char,
) {
    if end <= start || (kerning < 0 && ligatures.is_null() && features.is_null()) {
        return;
    }
    let mut a = InlineAttr::new(inline_kind::FONT_FEATURES, start, end - start).with_font_settings(
        ligatures,
        features,
        ptr::null(),
    );
    a.font_kerning = kerning;
    attrs.push(a);
}

pub fn emit_font_variations(
    attrs: &mut Vec<InlineAttr>,
    start: usize,
    end: usize,
    variations: *const c_char,
) {
    if end > start && !variations.is_null() {
        attrs.push(
            InlineAttr::new(inline_kind::FONT_VARIATIONS, start, end - start).with_font_settings(
                ptr::null(),
                ptr::null(),
                variations,
            ),
        );
    }
}

pub fn emit_color(attrs: &mut Vec<InlineAttr>, start: usize, end: usize, rgba: [u8; 4]) {
    if end > start {
        let mut a = InlineAttr::new(inline_kind::COLOR, start, end - start);
        [a.r, a.g, a.b, a.a] = rgba;
        attrs.push(a);
    }
}

pub fn emit_font_family(
    attrs: &mut Vec<InlineAttr>,
    start: usize,
    end: usize,
    family: *const c_char,
) {
    if end > start && !family.is_null() {
        attrs.push(
            InlineAttr::new(inline_kind::FONT_FAMILY, start, end - start).with_family(family),
        );
    }
}

pub fn font_kerning(s: Option<StyleRef<'_>>) -> c_int {
    match s
        .and_then(|s| s.keyword_of(PropId::FontKerning))
        .map(CStr::to_bytes)
    {
        Some(b"none") => 0,
        Some(b"normal" | b"auto") => 1,
        _ => -1,
    }
}

pub enum FontKeyword {
    Ligatures,
    Features,
    Variations,
}

pub fn font_keyword(s: Option<StyleRef<'_>>, which: FontKeyword) -> Option<&CStr> {
    let prop = match which {
        FontKeyword::Ligatures => PropId::FontVariantLigatures,
        FontKeyword::Features => PropId::FontFeatureSettings,
        FontKeyword::Variations => PropId::FontVariationSettings,
    };
    get(s, prop).and_then(ValueRef::keyword_text)
}

fn float_attr(n: Node<'_>, name: &CStr) -> Option<f64> {
    southstar_html_util::parse_float(n.attr(name)?.to_bytes())
}

pub fn progress_fraction(n: Option<Node<'_>>) -> f64 {
    let attr = |name: &CStr| n.and_then(|n| float_attr(n, name));
    let max = attr(c"max").filter(|&v| v > 0.0).unwrap_or(1.0);
    if n.and_then(|n| n.attr(c"value")).is_none() {
        return -1.0;
    }
    let mut value = attr(c"value").filter(|&v| v > 0.0).unwrap_or(0.0);
    if value > max {
        value = max;
    }
    if max > 0.0 { value / max } else { 0.0 }
}

pub fn meter_state(n: Option<Node<'_>>) -> (f64, c_int) {
    let attr = |name: &CStr| n.and_then(|n| float_attr(n, name));
    let min = attr(c"min").unwrap_or(0.0);
    let candidate_max = attr(c"max").unwrap_or(1.0);
    let max = if candidate_max >= min {
        candidate_max
    } else {
        min
    };
    let clamp = |v: f64, lo: f64, hi: f64| {
        let v = if v < lo { lo } else { v };
        if v > hi { hi } else { v }
    };
    let value = clamp(attr(c"value").unwrap_or(0.0), min, max);
    let low = clamp(attr(c"low").unwrap_or(min), min, max);
    let high = clamp(attr(c"high").unwrap_or(max), low, max);
    let midpoint = min + (max - min) / 2.0;
    let optimum = clamp(attr(c"optimum").unwrap_or(midpoint), min, max);
    let frac = if max > min {
        (value - min) / (max - min)
    } else {
        1.0
    };
    let quality = if optimum >= low && optimum <= high {
        if value >= low && value <= high { 0 } else { 1 }
    } else if optimum < low {
        if value < low {
            0
        } else if value <= high {
            1
        } else {
            2
        }
    } else if value > high {
        0
    } else if value >= low {
        1
    } else {
        2
    };
    (frac, quality)
}
