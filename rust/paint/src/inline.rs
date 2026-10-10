//! Southstar — inline text: building the Pango layout of a run from its style and inline attributes, painting it with decorations, shadows, selection, find highlights, carets, form controls and inline boxes, and the geometry queries hit testing and script make of it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::{FRAC_PI_2, PI};
use core::ffi::CStr;

use southstar_dom::{Kind as NodeKind, Node};
use southstar_layout::{BoxRef, InlineAttr, inline_kind as k};
use southstar_style::{Kind, PropId as P, StyleRef, ValueRef};

use crate::blur::box_blur_a8;
use crate::decor::{
    ANIM_TARGET_COLOR, paint_inline_css_chrome, rgba_anim, snap_device_x, snap_device_y,
    style_has_inline_box_paint, style_side_visible,
};
use crate::ffi::cairo::{self, Context, Cr, Matrix, Pattern, Surface};
use crate::ffi::engine::{self, FontMetrics};
use crate::ffi::pango::{self, AttrList, Attribute, Layout, Line, Rectangle};
use crate::radii::{CornerRadii, rounded_rect_path};
use crate::state;
use crate::text::{
    apply_css_line_spacing, apply_font_features, apply_i18n, apply_inline_font,
    apply_nowrap_align_width, apply_text_align, create_layout, font_features_attr, font_metrics,
    font_variations_attr, inline_y_offset_for_layout, is_nowrap, pango_font_size,
    start_align_overflow, stretch_from_css, weight_from_css, wrap_mode_for,
};
use crate::util::{
    Rgba, UNIT_CAP, UNIT_CH, UNIT_CQH, UNIT_CQMAX, UNIT_CQMIN, UNIT_CQW, UNIT_EM, UNIT_EX, UNIT_IC,
    UNIT_LH, UNIT_NUMBER, UNIT_PERCENT, UNIT_PX, UNIT_RCAP, UNIT_RCH, UNIT_REM, UNIT_REX, UNIT_RIC,
    UNIT_RLH, UNIT_VH, UNIT_VMAX, UNIT_VMIN, UNIT_VW, cmax, cmin, get, inherited_style, keyword_is,
    length_or, rgba_of, style_keyword, style_of,
};

const S: f64 = pango::SCALE_F;

fn c16(v: u8) -> u16 {
    u16::from(v) * 0x101
}

fn attr_style(r: &InlineAttr) -> Option<StyleRef<'_>> {
    unsafe { StyleRef::from_ptr(r.style()) }
}

fn attr_node(r: &InlineAttr) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(r.dom_ptr().cast()) }
}

fn box_node(b: BoxRef<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(b.dom_ptr().cast()) }
}

fn node_named(n: Option<Node<'_>>, name: &[u8]) -> bool {
    n.and_then(Node::name)
        .is_some_and(|nm| nm.to_bytes() == name)
}

fn decoration_insert_around_atomics(
    attrs: &AttrList,
    a: Option<Attribute>,
    text: &[u8],
    start: usize,
    len: usize,
) {
    const PLACEHOLDER: &[u8] = b"\xef\xbf\xbc";
    let Some(a) = a else {
        return;
    };
    let end = cmin_usize(start + len, text.len());
    let mut seg = start;
    let mut p = start;
    while p + 3 <= end {
        if &text[p..p + 3] != PLACEHOLDER {
            p += 1;
            continue;
        }
        if p > seg {
            attrs.insert_range(Some(a.copy()), seg, p - seg);
        }
        p += 3;
        seg = p;
    }
    if seg == start && end.wrapping_sub(start) == len {
        attrs.insert_range(Some(a), start, len);
        return;
    }
    if end > seg {
        attrs.insert_range(Some(a.copy()), seg, end - seg);
    }
}

fn cmin_usize(a: usize, b: usize) -> usize {
    if a < b { a } else { b }
}

fn find_ci_substring(hay: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || start >= hay.len() {
        return None;
    }
    let case_sensitive = state::search_case_sensitive();
    let mut i = start;
    while i + needle.len() <= hay.len() {
        let window = &hay[i..i + needle.len()];
        let hit = if case_sensitive {
            window == needle
        } else {
            window.eq_ignore_ascii_case(needle)
        };
        if hit {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn apply_first_line_attrs(attrs: &AttrList, fl: StyleRef<'_>, start: u32, end: u32) {
    if end <= start {
        return;
    }
    let (start_u, len) = (start as usize, (end - start) as usize);
    if let Some((v, UNIT_PX)) = fl.get(P::FontSize).and_then(ValueRef::length) {
        attrs.insert_range(
            Some(Attribute::size_absolute(pango_font_size(v))),
            start_u,
            len,
        );
    }
    if let Some(c) = fl.get(P::Color).and_then(ValueRef::color) {
        attrs.insert_range(
            Some(Attribute::foreground(c16(c[0]), c16(c[1]), c16(c[2]))),
            start_u,
            len,
        );
        if c[3] < 255 {
            let alpha = if c[3] != 0 { c16(c[3]) } else { 1 };
            attrs.insert_range(Some(Attribute::foreground_alpha(alpha)), start_u, len);
        }
    }
    if let Some(c) = fl
        .get(P::BackgroundColor)
        .and_then(ValueRef::color)
        .filter(|c| c[3] > 0)
    {
        attrs.insert_range(
            Some(Attribute::background(c16(c[0]), c16(c[1]), c16(c[2]))),
            start_u,
            len,
        );
        if c[3] < 255 {
            attrs.insert_range(Some(Attribute::background_alpha(c16(c[3]))), start_u, len);
        }
    }
    let fw = engine::font_weight_number(fl.get(P::FontWeight), -1);
    if fw > 0 {
        attrs.insert_range(Some(Attribute::weight(weight_from_css(fw))), start_u, len);
    }
    if let Some(stretch) = fl.get(P::FontStretch) {
        attrs.insert_range(
            Some(Attribute::stretch(stretch_from_css(
                engine::font_stretch_rank(Some(stretch)),
            ))),
            start_u,
            len,
        );
    }
    if keyword_is(fl.get(P::FontStyle), c"italic") || keyword_is(fl.get(P::FontStyle), c"oblique") {
        attrs.insert_range(Some(Attribute::style(pango::STYLE_ITALIC)), start_u, len);
    }
    if let Some(ff) = fl
        .get(P::FontFamily)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
    {
        let pf = engine::font_family_for_pango(Some(ff));
        attrs.insert_range(Some(Attribute::family(&pf)), start_u, len);
    }
    if keyword_is(fl.get(P::FontVariant), c"small-caps") {
        attrs.insert_range(
            Some(Attribute::variant(pango::VARIANT_SMALL_CAPS)),
            start_u,
            len,
        );
    }
    apply_font_features(attrs, fl, start, end);
    attrs.insert_range(
        font_variations_attr(fl.keyword_of(P::FontVariationSettings)),
        start_u,
        len,
    );
    let td = fl.get(P::TextDecoration);
    let invisible = fl
        .get(P::TextDecorationColor)
        .and_then(ValueRef::color)
        .is_some_and(|c| c[3] == 0);
    if let Some(kw) = td
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes)
    {
        let has = |needle: &[u8]| kw.windows(needle.len()).any(|w| w == needle);
        if !invisible && has(b"underline") && !has(b"none") {
            attrs.insert_range(
                Some(Attribute::underline(pango::UNDERLINE_SINGLE)),
                start_u,
                len,
            );
        }
    }
}

fn decoration_value<'a>(
    r: Option<&'a InlineAttr>,
    s: Option<StyleRef<'a>>,
    prop: P,
) -> Option<ValueRef<'a>> {
    r.and_then(|r| attr_style(r)?.get(prop))
        .or_else(|| get(s, prop))
}

fn underline_dash_style<'a>(r: &'a InlineAttr, s: Option<StyleRef<'a>>) -> Option<&'a [u8]> {
    let dv = decoration_value(Some(r), s, P::TextDecorationStyle)?;
    let kw = dv
        .keyword_text()
        .filter(|_| dv.kind() == Kind::Keyword)?
        .to_bytes();
    matches!(kw, b"dotted" | b"dashed").then_some(kw)
}

fn decoration_color_of<'a>(r: &'a InlineAttr, s: Option<StyleRef<'a>>) -> Option<[u8; 4]> {
    decoration_value(Some(r), s, P::TextDecorationColor).and_then(ValueRef::color)
}

fn decoration_style_of<'a>(r: &'a InlineAttr, s: Option<StyleRef<'a>>) -> Option<&'a [u8]> {
    decoration_value(Some(r), s, P::TextDecorationStyle)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(ValueRef::keyword_text)
        .map(CStr::to_bytes)
}

fn is_decoration(kind: u32) -> bool {
    matches!(kind, k::UNDERLINE | k::STRIKETHROUGH | k::OVERLINE)
}

fn paint_inline_dashed_decorations(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    text_x: f64,
    y_origin: f64,
    s: Option<StyleRef<'_>>,
    base: Rgba,
) {
    if !b.has_attrs() {
        return;
    }
    let attrs = b.attrs();
    if !attrs
        .iter()
        .any(|r| is_decoration(r.kind) && underline_dash_style(r, s).is_some())
    {
        return;
    }
    let mut iter = layout.iter();
    loop {
        if let Some(line) = iter.line() {
            let base_y = y_origin + f64::from(iter.baseline()) / S;
            let line_start = line.start_index();
            let line_end = line.start_index() + line.length();
            for r in attrs {
                if !is_decoration(r.kind) {
                    continue;
                }
                let Some(kw) = underline_dash_style(r, s) else {
                    continue;
                };
                let dotted = kw == b"dotted";
                let rstart = r.start as i32;
                let rend = (r.start + r.len) as i32;
                let seg0 = if rstart > line_start {
                    rstart
                } else {
                    line_start
                };
                let seg1 = if rend < line_end { rend } else { line_end };
                if seg0 >= seg1 {
                    continue;
                }
                let xa = line.index_to_x(seg0, false);
                let xb = line.index_to_x(seg1, false);
                let x0 = text_x + f64::from(xa.min(xb)) / S;
                let x1 = text_x + f64::from(xa.max(xb)) / S;
                let em = if r.font_size_px > 0.0 {
                    r.font_size_px
                } else {
                    16.0
                };
                let mut thick = em / 16.0;
                if thick < 1.0 {
                    thick = 1.0;
                }
                let uy = if r.kind == k::STRIKETHROUGH {
                    base_y - em * 0.28
                } else if r.kind == k::OVERLINE {
                    base_y - em * 0.78
                } else {
                    base_y + thick * 1.5
                };
                cr.save();
                match attr_style(r)
                    .and_then(|st| st.get(P::TextDecorationColor))
                    .and_then(ValueRef::color)
                {
                    Some(c) => Rgba::from_bytes(c).set_source(cr),
                    None => base.set_source(cr),
                }
                cr.set_line_width(thick);
                let on = if dotted { thick } else { thick * 3.0 };
                let off = if dotted { thick * 1.6 } else { thick * 2.5 };
                cr.set_dash(&[on, off]);
                cr.set_line_cap(if dotted {
                    cairo::LINE_CAP_ROUND
                } else {
                    cairo::LINE_CAP_BUTT
                });
                cr.move_to(x0, uy);
                cr.line_to(x1, uy);
                cr.stroke();
                cr.restore();
            }
        }
        if !iter.next_line() {
            break;
        }
    }
}

fn paint_text_shadow_layer(cr: Cr, layout: &Layout, x: f64, y: f64, sh: &southstar_style::Shadow) {
    let (lw, lh) = layout.pixel_size();
    if lw <= 0 || lh <= 0 {
        return;
    }
    let mut blur = (sh.blur + 0.5) as i32;
    if blur < 0 {
        blur = 0;
    }
    let ds = (f64::from(blur) / 3.0).clamp(1.0, 4.0);
    let mut blur_s = (f64::from(blur) / ds + 0.5) as i32;
    if blur > 0 && blur_s < 1 {
        blur_s = 1;
    }
    let pad = blur_s * 3 + 2;
    let mw = (f64::from(lw) / ds).ceil() as i32 + 2 * pad;
    let mh = (f64::from(lh) / ds).ceil() as i32 + 2 * pad;
    let color = Rgba::new(
        f64::from(sh.r) / 255.0,
        f64::from(sh.g) / 255.0,
        f64::from(sh.b) / 255.0,
        f64::from(sh.a) / 255.0,
    );
    if mw > 4096 || mh > 4096 {
        cr.save();
        color.set_source(cr);
        cr.move_to(x + sh.x, y + sh.y);
        layout.show(cr);
        cr.restore();
        return;
    }
    let Some(mask) = Surface::image(cairo::FORMAT_A8, mw, mh) else {
        return;
    };
    {
        let ctx = Context::new(mask.as_ref());
        let mcr = ctx.cr();
        mcr.scale(1.0 / ds, 1.0 / ds);
        mcr.move_to(f64::from(pad) * ds, f64::from(pad) * ds);
        layout.show(mcr);
    }
    let mut ms = mask.as_ref();
    ms.flush();
    if blur > 0 {
        let stride = ms.stride();
        let data = ms.data_mut();
        box_blur_a8(data, mw, mh, stride, blur_s);
        box_blur_a8(data, mw, mh, stride, blur_s);
        ms.mark_dirty();
    }
    let ox = x + sh.x - f64::from(pad) * ds;
    let oy = y + sh.y - f64::from(pad) * ds;
    cr.save();
    color.set_source(cr);
    if ds == 1.0 {
        cr.mask_surface(mask.as_ref(), ox, oy);
    } else {
        let mp = Pattern::for_surface(mask.as_ref());
        let mut m = Matrix::scaling(1.0 / ds, 1.0 / ds);
        m.translate(-ox, -oy);
        mp.set_matrix(&m);
        cr.mask(&mp);
    }
    cr.restore();
}

fn spell_underline_range(attrs: &AttrList, t: &CStr, rstart: usize, rend: usize) {
    let bytes = t.to_bytes();
    let mut p = rstart;
    while p < rend {
        if !engine::unichar_isalpha(engine::utf8_get_char(t, p)) {
            p = engine::utf8_next(bytes, p);
            continue;
        }
        let start = p;
        let mut q = p;
        let mut alpha = 0;
        while q < rend {
            let c = engine::utf8_get_char(t, q);
            let nx = engine::utf8_next(bytes, q);
            if engine::unichar_isalpha(c) {
                alpha += 1;
                q = nx;
                continue;
            }
            if (c == u32::from(b'\'') || c == 0x2019)
                && nx < rend
                && engine::unichar_isalpha(engine::utf8_get_char(t, nx))
            {
                q = nx;
                continue;
            }
            break;
        }
        let blen = q - start;
        if alpha >= 2 && !engine::spell_word_ok(&bytes[start..q]) {
            attrs.insert(
                Attribute::underline(pango::UNDERLINE_ERROR),
                start as u32,
                (start + blen) as u32,
            );
            attrs.insert(
                Attribute::underline_color(0xffff, 0x1000, 0x1000),
                start as u32,
                (start + blen) as u32,
            );
        }
        p = q;
    }
}

fn paint_spell_underlines(attrs: &AttrList, b: BoxRef<'_>) {
    let Some(text) = b.text() else {
        return;
    };
    if !engine::spell_available() {
        return;
    }
    let tlen = text.to_bytes().len();
    let mut ranges_known = false;
    for a in b.attrs() {
        if a.kind == k::INPUT_FIELD || a.kind == k::INPUT_FIELD_FOCUSED {
            ranges_known = true;
        }
        if a.kind != k::SPELLCHECK {
            continue;
        }
        let s = a.start;
        let e = cmin_usize(a.start + a.len, tlen);
        if s < e {
            spell_underline_range(attrs, text, s, e);
            ranges_known = true;
        }
    }
    if ranges_known {
        return;
    }
    let mut owner = None;
    let mut bx = Some(b);
    while let Some(cur) = bx {
        if owner.is_some() {
            break;
        }
        owner = box_node(cur);
        bx = cur.parent();
    }
    if owner.is_some_and(engine::node_spellcheck_host) {
        spell_underline_range(attrs, text, 0, tlen);
    }
}

pub fn drop_box_cache(b: BoxRef<'_>) {
    let layout = b.paint_layout();
    if !layout.is_null() {
        drop(unsafe { Layout::from_owned(layout) });
        b.set_paint_layout(core::ptr::null_mut());
    }
}

fn line_clamp(s: Option<StyleRef<'_>>) -> Option<f64> {
    get(s, P::LineClamp)
        .and_then(ValueRef::length)
        .map(|(v, _)| v)
        .filter(|v| *v >= 1.0)
}

fn base_layout(b: BoxRef<'_>, s: Option<StyleRef<'_>>) -> Option<(Layout, AttrList)> {
    let text = b.text()?;
    let layout = create_layout();
    apply_inline_font(&layout, s);
    if is_nowrap(s) && !keyword_is(get(s, P::TextOverflow), c"ellipsis") {
        layout.set_width(-1);
    } else {
        layout.set_width((b.content_width() * S) as i32);
    }
    layout.set_wrap(wrap_mode_for(s));
    if b.inline_atomics().is_none_or(<[_]>::is_empty) {
        apply_css_line_spacing(Some(&layout), s);
    }
    let ti = engine::inline_text_indent_px(b, s, b.content_width());
    if ti > 0.0 {
        layout.set_indent((ti * S) as i32);
    }
    if keyword_is(get(s, P::TextOverflow), c"ellipsis") {
        layout.set_ellipsize(pango::ELLIPSIZE_END);
    }
    if let Some(lc) = line_clamp(s) {
        layout.set_height(-(lc as i32));
        layout.set_ellipsize(pango::ELLIPSIZE_END);
    }
    layout.set_text(text);
    let attrs = AttrList::new();
    apply_i18n(Some(&layout), Some(&attrs), Some(b));
    if let Some(st) = s {
        apply_font_features(&attrs, st, 0, u32::MAX);
    }
    engine::inline_apply_atomic_shapes(&attrs, b);
    Some((layout, attrs))
}

fn px_length(s: Option<StyleRef<'_>>, prop: P) -> f64 {
    match get(s, prop).and_then(ValueRef::length) {
        Some((v, UNIT_PX)) => v,
        _ => 0.0,
    }
}

fn family_attr(family: Option<&CStr>) -> Option<Attribute> {
    let pf = engine::font_family_for_pango(Some(family?));
    Some(Attribute::family(&pf))
}

fn paint_inline_make_layout(
    b: BoxRef<'_>,
    s: Option<StyleRef<'_>>,
    highlight: Option<&CStr>,
) -> Option<Layout> {
    let (layout, attrs) = base_layout(b, s)?;
    let text = b.text()?;
    let ls_px = px_length(s, P::LetterSpacing);
    let ws_px = px_length(s, P::WordSpacing);
    if ls_px != 0.0 {
        attrs.insert(Attribute::letter_spacing((ls_px * S) as i32), 0, u32::MAX);
    }
    if ws_px != 0.0 {
        let per_space = ((ls_px + ws_px) * S) as i32;
        for (idx, _) in text
            .to_bytes()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == b' ')
        {
            attrs.insert(
                Attribute::letter_spacing(per_space),
                idx as u32,
                (idx + 1) as u32,
            );
        }
    }
    for r in b.attrs().iter().rev() {
        let mut a = None;
        match r.kind {
            k::BOLD => a = Some(Attribute::weight(pango::WEIGHT_BOLD)),
            k::FONT_WEIGHT => a = Some(Attribute::weight(weight_from_css(r.font_weight))),
            k::FONT_STRETCH => a = Some(Attribute::stretch(stretch_from_css(r.font_stretch))),
            k::FONT_FEATURES => {
                a = font_features_attr(r.font_kerning, r.font_ligatures(), r.font_features())
            }
            k::FONT_VARIATIONS => a = font_variations_attr(r.font_variations()),
            k::ITALIC => a = Some(Attribute::style(pango::STYLE_ITALIC)),
            k::MONOSPACE => a = Some(Attribute::family(c"monospace")),
            k::UNDERLINE | k::OVERLINE | k::STRIKETHROUGH => 'deco: {
                if underline_dash_style(r, s).is_some() {
                    break 'deco;
                }
                let cv = decoration_color_of(r, s);
                if cv.is_some_and(|c| c[3] == 0) {
                    break 'deco;
                }
                match r.kind {
                    k::UNDERLINE => {
                        let mut ul = pango::UNDERLINE_SINGLE;
                        match decoration_style_of(r, s) {
                            Some(b"double") => ul = pango::UNDERLINE_DOUBLE,
                            Some(b"wavy") => ul = pango::UNDERLINE_ERROR,
                            _ => {}
                        }
                        a = Some(Attribute::underline(ul));
                        if let Some(c) = cv {
                            attrs.insert_range(
                                Some(Attribute::underline_color(c16(c[0]), c16(c[1]), c16(c[2]))),
                                r.start,
                                r.len,
                            );
                        }
                    }
                    k::OVERLINE => {
                        a = Some(Attribute::overline(pango::OVERLINE_SINGLE));
                        if let Some(c) = cv {
                            attrs.insert_range(
                                Some(Attribute::overline_color(c16(c[0]), c16(c[1]), c16(c[2]))),
                                r.start,
                                r.len,
                            );
                        }
                    }
                    _ => {
                        a = Some(Attribute::strikethrough(true));
                        if let Some(c) = cv {
                            attrs.insert_range(
                                Some(Attribute::strikethrough_color(
                                    c16(c[0]),
                                    c16(c[1]),
                                    c16(c[2]),
                                )),
                                r.start,
                                r.len,
                            );
                        }
                    }
                }
            }
            k::INPUT_FIELD | k::INPUT_FIELD_FOCUSED | k::BUTTON => {
                if !node_named(attr_node(r), b"textarea") {
                    attrs.insert_range(Some(Attribute::allow_breaks(false)), r.start, r.len);
                }
            }
            k::CHECKBOX | k::CHECKBOX_CHECKED | k::RADIO | k::RADIO_CHECKED => {
                attrs.insert_range(Some(Attribute::foreground_alpha(1)), r.start, r.len);
            }
            k::FONT_SIZE => a = Some(Attribute::size_absolute(pango_font_size(r.font_size_px))),
            k::COLOR => {
                a = Some(Attribute::foreground(c16(r.r), c16(r.g), c16(r.b)));
                if r.a < 255 {
                    let alpha = if r.a != 0 { c16(r.a) } else { 1 };
                    attrs.insert_range(Some(Attribute::foreground_alpha(alpha)), r.start, r.len);
                }
            }
            k::BG_COLOR => {
                if r.a != 0 {
                    a = Some(Attribute::background(c16(r.r), c16(r.g), c16(r.b)));
                    if r.a < 255 {
                        attrs.insert_range(
                            Some(Attribute::background_alpha(c16(r.a))),
                            r.start,
                            r.len,
                        );
                    }
                }
            }
            k::FONT_FAMILY => a = family_attr(r.family()),
            k::SUPERSCRIPT => {
                attrs.insert_range(Some(Attribute::rise(4000)), r.start, r.len);
                a = Some(Attribute::scale(0.75));
            }
            k::SUBSCRIPT => {
                attrs.insert_range(Some(Attribute::rise(-3000)), r.start, r.len);
                a = Some(Attribute::scale(0.75));
            }
            k::SMALL_CAPS => a = Some(Attribute::variant(pango::VARIANT_SMALL_CAPS)),
            k::SELECTION => {
                attrs.insert_range(
                    Some(Attribute::background(0xb400, 0xd500, 0xfe00)),
                    r.start,
                    r.len,
                );
                attrs.insert_range(Some(Attribute::foreground(0, 0, 0)), r.start, r.len);
            }
            k::SPACER => a = Some(Attribute::shape((r.box_w * S) as i32)),
            _ => {}
        }
        if is_decoration(r.kind) {
            decoration_insert_around_atomics(&attrs, a, text.to_bytes(), r.start, r.len);
        } else {
            attrs.insert_range(a, r.start, r.len);
        }
    }
    if let Some(highlight) = highlight.filter(|h| !h.is_empty()) {
        let hay = text.to_bytes();
        let needle = highlight.to_bytes();
        let is_active = core::ptr::eq(b.as_ptr(), state::search_active());
        let (br, bg, bb) = (
            0xffff,
            if is_active { 0xff00 } else { 0xee00 },
            if is_active { 0x6600 } else { 0xb000 },
        );
        let mut pos = 0;
        while let Some(at) = find_ci_substring(hay, needle, pos) {
            attrs.insert_range(Some(Attribute::background(br, bg, bb)), at, needle.len());
            pos = at + if needle.is_empty() { 1 } else { needle.len() };
        }
    }
    if let Some(fl) = s.and_then(StyleRef::first_line) {
        let first = b
            .parent()
            .and_then(|p| p.first_child())
            .is_some_and(|c| c.same(b));
        if first && let Some(line0) = layout.line(0).filter(|l| l.length() > 0) {
            apply_first_line_attrs(
                &attrs,
                fl,
                line0.start_index() as u32,
                (line0.start_index() + line0.length()) as u32,
            );
        }
    }
    paint_spell_underlines(&attrs, b);
    engine::inline_layout_set_attrs(&layout, &attrs, b);
    drop(attrs);
    apply_text_align(&layout, s);
    apply_nowrap_align_width(&layout, b);
    start_align_overflow(&layout);
    if keyword_is(get(s, P::TextAlign), c"justify") {
        layout.set_justify(true);
    }
    Some(layout)
}

pub fn build_inline_layout(b: BoxRef<'_>) -> Option<Layout> {
    let s = inherited_style(b);
    let (layout, attrs) = base_layout(b, s)?;
    for r in b.attrs().iter().rev() {
        let a = match r.kind {
            k::BOLD => Some(Attribute::weight(pango::WEIGHT_BOLD)),
            k::FONT_WEIGHT => Some(Attribute::weight(weight_from_css(r.font_weight))),
            k::FONT_STRETCH => Some(Attribute::stretch(stretch_from_css(r.font_stretch))),
            k::FONT_FEATURES => {
                font_features_attr(r.font_kerning, r.font_ligatures(), r.font_features())
            }
            k::FONT_VARIATIONS => font_variations_attr(r.font_variations()),
            k::ITALIC => Some(Attribute::style(pango::STYLE_ITALIC)),
            k::MONOSPACE => Some(Attribute::family(c"monospace")),
            k::FONT_SIZE => Some(Attribute::size_absolute(pango_font_size(r.font_size_px))),
            k::FONT_FAMILY => family_attr(r.family()),
            k::SUPERSCRIPT | k::SUBSCRIPT => Some(Attribute::scale(0.75)),
            k::SMALL_CAPS => Some(Attribute::variant(pango::VARIANT_SMALL_CAPS)),
            k::SPACER => Some(Attribute::shape((r.box_w * S) as i32)),
            _ => None,
        };
        attrs.insert_range(a, r.start, r.len);
    }
    engine::inline_layout_set_attrs(&layout, &attrs, b);
    drop(attrs);
    apply_text_align(&layout, s);
    apply_nowrap_align_width(&layout, b);
    start_align_overflow(&layout);
    Some(layout)
}

#[derive(Clone, Copy)]
struct SelRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn paint_selection_rects(
    layout: &Layout,
    ox: f64,
    oy: f64,
    run: engine::SelectionRun,
) -> Vec<SelRect> {
    let mut out = Vec::new();
    let mut iter = layout.iter();
    loop {
        if let Some(line) = iter.line() {
            let line_start = line.start_index();
            let line_end = line_start + line.length();
            let rs = run.start as i32;
            let re = run.end as i32;
            let s = if rs > line_start { rs } else { line_start };
            let e = if re < line_end { re } else { line_end };
            if s < e {
                let ext = iter.line_logical_extents();
                for (x0, x1) in line.x_ranges(s, e) {
                    let (x0, x1) = if x1 < x0 { (x1, x0) } else { (x0, x1) };
                    let mut r = SelRect {
                        x: ox + f64::from(x0) / S,
                        y: oy + f64::from(ext.y) / S,
                        w: f64::from(x1 - x0) / S,
                        h: f64::from(ext.height) / S,
                    };
                    if r.w < 1.0 {
                        r.w = 1.0;
                    }
                    if r.h < 1.0 {
                        r.h = 1.0;
                    }
                    out.push(r);
                }
            }
        }
        if !iter.next_line() {
            break;
        }
    }
    out
}

fn selection_pseudo_color(b: BoxRef<'_>, prop: P) -> Option<Rgba> {
    let mut ssel = None;
    let mut p = Some(b);
    while let Some(cur) = p {
        if ssel.is_some() {
            break;
        }
        if let Some(st) = style_of(cur) {
            ssel = st.selection();
        }
        p = cur.parent();
    }
    ssel?
        .get(prop)
        .and_then(ValueRef::color)
        .map(Rgba::from_bytes)
}

fn paint_selection_background(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    ox: f64,
    oy: f64,
    run: engine::SelectionRun,
) {
    let rects = paint_selection_rects(layout, ox, oy, run);
    if rects.is_empty() {
        return;
    }
    let bg =
        selection_pseudo_color(b, P::BackgroundColor).unwrap_or(Rgba::new(0.20, 0.40, 0.85, 0.30));
    cr.save();
    bg.set_source(cr);
    for r in &rects {
        cr.rectangle(r.x, r.y, r.w, r.h);
    }
    cr.fill();
    cr.restore();
}

fn paint_selection_foreground(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    ox: f64,
    oy: f64,
    run: engine::SelectionRun,
) {
    let Some(fg) = selection_pseudo_color(b, P::Color) else {
        return;
    };
    let rects = paint_selection_rects(layout, ox, oy, run);
    if rects.is_empty() {
        return;
    }
    cr.save();
    for r in &rects {
        cr.rectangle(r.x, r.y, r.w, r.h);
    }
    cr.clip();
    fg.set_source(cr);
    cr.move_to(ox, oy);
    layout.show(cr);
    cr.restore();
}

fn paint_inline_lines_at_layout_heights(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    text_x: f64,
) -> bool {
    let Some(heights) = b.atomic_line_heights() else {
        return false;
    };
    if heights.len() < 2 || layout.line_count() as usize != heights.len() {
        return false;
    }
    let mut it = layout.iter();
    let mut line_top = b.y();
    let mut i = 0;
    loop {
        let line = it.line();
        let logical = it.line_logical_extents();
        let line_h = heights[i];
        let pango_h = f64::from(logical.height) / S;
        let baseline = f64::from(it.baseline() - logical.y) / S;
        cr.move_to(
            text_x + f64::from(logical.x) / S,
            line_top + (line_h - pango_h) / 2.0 + baseline,
        );
        if let Some(line) = line {
            line.show(cr);
        }
        line_top += line_h;
        i += 1;
        if !(i < heights.len() && it.next_line()) {
            break;
        }
    }
    true
}

fn paint_inline_line_baselines(b: BoxRef<'_>, layout: &Layout, y_origin: f64) -> Vec<f64> {
    let n = layout.line_count();
    let mut out = vec![0.0; if n > 0 { n as usize } else { 1 }];
    let heights = b.atomic_line_heights();
    let by_layout = heights.is_some_and(|h| h.len() >= 2 && h.len() == n as usize);
    let mut it = layout.iter();
    let mut line_top = b.y();
    let mut i = 0usize;
    loop {
        let logical = it.line_logical_extents();
        let baseline = it.baseline();
        if by_layout {
            let line_h = heights.map_or(0.0, |h| h[i]);
            out[i] = line_top
                + (line_h - f64::from(logical.height) / S) / 2.0
                + f64::from(baseline - logical.y) / S;
            line_top += line_h;
        } else {
            out[i] = y_origin + f64::from(baseline) / S;
        }
        i += 1;
        if !((i as i32) < n && it.next_line()) {
            break;
        }
    }
    out
}

fn paint_inline_element_fragment(cr: Cr, r: &InlineAttr, f: &Fragment) {
    let reach = (f.y1 - f.y0) + 64.0;
    let bx0 = if f.open_left { f.x0 - reach } else { f.x0 };
    let bx1 = if f.open_right { f.x1 + reach } else { f.x1 };
    cr.save();
    if f.open_left || f.open_right {
        cr.rectangle(f.x0, f.y0 - reach, f.x1 - f.x0, (f.y1 - f.y0) + 2.0 * reach);
        cr.clip();
    }
    paint_inline_css_chrome(cr, Some(r), (bx0, f.y0, bx1 - bx0, f.y1 - f.y0));
    cr.restore();
}

#[derive(Clone, Copy, Default)]
struct Fragment {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    open_left: bool,
    open_right: bool,
}

fn inline_border_px(s: StyleRef<'_>, width: P, style: P) -> f64 {
    if !style_side_visible(Some(s), width, style) {
        return 0.0;
    }
    length_or(s.get(width), 0.0)
}

fn inline_box_vertical_reach(s: Option<StyleRef<'_>>) -> Option<(f64, f64)> {
    let st = s?;
    let mut ascent = 0.0;
    let mut descent = 0.0;
    let font_size = length_or(st.get(P::FontSize), 16.0);
    if font_size > 0.0 {
        let family = st
            .get(P::FontFamily)
            .filter(|v| v.kind() == Kind::Keyword)
            .and_then(ValueRef::keyword_text)
            .unwrap_or(c"sans-serif");
        let italic = keyword_is(st.get(P::FontStyle), c"italic")
            || keyword_is(st.get(P::FontStyle), c"oblique");
        let weight = engine::font_weight_number(st.get(P::FontWeight), 400);
        let mut m = FontMetrics::default();
        font_metrics(Some(family), font_size, weight, italic, &mut m);
        let total = m.ascent_px + m.descent_px;
        if total.is_nan() || total <= 0.0 {
            return None;
        }
        ascent = m.ascent_px;
        descent = m.descent_px;
    }
    let above = ascent
        + length_or(st.get(P::PaddingTop), 0.0)
        + inline_border_px(st, P::BorderTopWidth, P::BorderTopStyle);
    let below = descent
        + length_or(st.get(P::PaddingBottom), 0.0)
        + inline_border_px(st, P::BorderBottomWidth, P::BorderBottomStyle);
    Some((above, below))
}

fn inline_line_baseline_shift(line: Line<'_>, lo: usize, hi: usize) -> i32 {
    let mut shift: [i32; 2] = [0, 0];
    let mut any = [false, false];
    for run in line.runs() {
        let run_start = run.offset as usize;
        let run_end = run_start + run.length as usize;
        if run_end <= lo || run_start >= hi {
            continue;
        }
        let kk = usize::from(run.is_spacer);
        if !any[kk] || run.y_offset.abs() < shift[kk].abs() {
            shift[kk] = run.y_offset;
        }
        any[kk] = true;
    }
    if any[0] { shift[0] } else { shift[1] }
}

fn inline_line_x_extent(line: Line<'_>, lo: usize, hi: usize) -> Option<(f64, f64)> {
    let ranges = line.x_ranges(lo as i32, hi as i32);
    let (&(a, b), rest) = ranges.split_first()?;
    let mut left = a.min(b);
    let mut right = a.max(b);
    for &(a, b) in rest {
        left = left.min(a.min(b));
        right = right.max(a.max(b));
    }
    Some((f64::from(left) / S, f64::from(right) / S))
}

struct InlineRange<'a> {
    start: usize,
    len: usize,
    box_style: Option<StyleRef<'a>>,
    raised: bool,
}

fn inline_element_is_raised(b: BoxRef<'_>, r: &InlineAttr) -> bool {
    let stop = b.dom_ptr();
    let mut n = attr_node(r);
    while let Some(node) = n {
        if core::ptr::eq(node.as_ptr().cast(), stop) {
            break;
        }
        if node.kind() == NodeKind::Element
            && let Some(name) = node.name()
            && (name.to_bytes() == b"sup" || name.to_bytes() == b"sub")
        {
            return true;
        }
        n = node.parent();
    }
    false
}

fn inline_range_fragments(
    b: BoxRef<'_>,
    layout: &Layout,
    y_origin: f64,
    range: &InlineRange<'_>,
    mut f: impl FnMut(&Fragment),
) {
    let reach = inline_box_vertical_reach(range.box_style);
    let rtl = range
        .box_style
        .is_some_and(|s| keyword_is(s.get(P::Direction), c"rtl"));
    let baselines = reach.map(|_| paint_inline_line_baselines(b, layout, y_origin));
    let end = range.start + range.len;
    let mut it = layout.iter();
    let mut line_index: i32 = -1;
    loop {
        line_index += 1;
        if let Some(line) = it.line() {
            let line_start = line.start_index() as usize;
            let line_end = line_start + line.length() as usize;
            let lo = range.start.max(line_start);
            let hi = end.min(line_end);
            if lo < hi
                && let Some((x0, x1)) = inline_line_x_extent(line, lo, hi)
            {
                let mut frag = Fragment {
                    x0,
                    x1,
                    ..Fragment::default()
                };
                if let (Some((above, below)), Some(baselines)) = (reach, &baselines) {
                    let shift = if range.raised {
                        inline_line_baseline_shift(line, lo, hi)
                    } else {
                        0
                    };
                    let baseline = baselines[line_index as usize] - f64::from(shift) / S;
                    frag.y0 = baseline - above;
                    frag.y1 = baseline + below;
                } else {
                    let lrect = it.line_logical_extents();
                    frag.y0 = y_origin + f64::from(lrect.y) / S;
                    frag.y1 = y_origin + f64::from(lrect.y + lrect.height) / S;
                }
                frag.open_left = if rtl { hi < end } else { lo > range.start };
                frag.open_right = if rtl { lo > range.start } else { hi < end };
                f(&frag);
            }
        }
        if !it.next_line() {
            break;
        }
    }
}

fn inline_box_is_hidden(s: Option<StyleRef<'_>>) -> bool {
    style_keyword(s, P::Visibility).is_some_and(|v| matches!(v.to_bytes(), b"hidden" | b"collapse"))
}

fn paint_inline_element_boxes(cr: Cr, b: BoxRef<'_>, layout: &Layout, text_x: f64, y_origin: f64) {
    for r in b.attrs().iter().rev() {
        let st = attr_style(r);
        if r.kind != k::ELEMENT
            || r.len == 0
            || !style_has_inline_box_paint(st)
            || inline_box_is_hidden(st)
        {
            continue;
        }
        let range = InlineRange {
            start: r.start,
            len: r.len,
            box_style: st,
            raised: inline_element_is_raised(b, r),
        };
        inline_range_fragments(b, layout, y_origin, &range, |f| {
            let snapped = Fragment {
                x0: snap_device_x(cr, text_x + f.x0),
                x1: snap_device_x(cr, text_x + f.x1),
                y0: snap_device_y(cr, f.y0),
                y1: snap_device_y(cr, f.y1),
                open_left: f.open_left,
                open_right: f.open_right,
            };
            paint_inline_element_fragment(cr, r, &snapped);
        });
    }
}

fn inline_control_dim_px(v: Option<ValueRef<'_>>, font_size: f64, basis: f64) -> f64 {
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
    let container = |c: f64, vp: f64| if c > 0.0 { c } else { vp };
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
        UNIT_VW => len * engine::viewport_w() / 100.0,
        UNIT_VH => len * engine::viewport_h() / 100.0,
        UNIT_VMIN => len * cmin(engine::viewport_w(), engine::viewport_h()) / 100.0,
        UNIT_VMAX => len * cmax(engine::viewport_w(), engine::viewport_h()) / 100.0,
        UNIT_CQW => len * container(engine::container_w(), engine::viewport_w()) / 100.0,
        UNIT_CQH => len * container(engine::container_h(), engine::viewport_h()) / 100.0,
        UNIT_CQMIN => {
            let cw = container(engine::container_w(), engine::viewport_w());
            let ch = container(engine::container_h(), engine::viewport_h());
            len * cmin(cw, ch) / 100.0
        }
        UNIT_CQMAX => {
            let cw = container(engine::container_w(), engine::viewport_w());
            let ch = container(engine::container_h(), engine::viewport_h());
            len * cmax(cw, ch) / 100.0
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

fn inline_control_dim_px_clamped(
    s: StyleRef<'_>,
    props: (P, P, P),
    font_size: f64,
    basis: f64,
) -> f64 {
    let mut out = inline_control_dim_px(s.get(props.0), font_size, basis);
    let mn = inline_control_dim_px(s.get(props.1), font_size, basis);
    let mx = inline_control_dim_px(s.get(props.2), font_size, basis);
    if mn > 0.0 && out > 0.0 && out < mn {
        out = mn;
    }
    if mx > 0.0 && out > mx {
        out = mx;
    }
    out
}

fn inline_control_css_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    let Some(st) = attr_style(r) else {
        return if r.box_w > 0.0 { r.box_w } else { 0.0 };
    };
    let fs = length_or(st.get(P::FontSize), 16.0);
    let mut w = inline_control_dim_px_clamped(
        st,
        (P::Width, P::MinWidth, P::MaxWidth),
        fs,
        b.content_width(),
    );
    if w > 0.0 {
        w += engine::control_css_extra_w(r, st);
    }
    if w > 0.0 { w } else { r.box_w }
}

fn inline_control_css_min_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    let Some(st) = attr_style(r) else {
        return 0.0;
    };
    let fs = length_or(st.get(P::FontSize), 16.0);
    let mut mn = inline_control_dim_px(st.get(P::MinWidth), fs, b.content_width());
    if mn > 0.0 {
        mn += engine::control_css_extra_w(r, st);
    }
    mn
}

fn is_field(kind: u32) -> bool {
    kind == k::INPUT_FIELD || kind == k::INPUT_FIELD_FOCUSED
}

fn char_span(layout: &Layout, r: &InlineAttr) -> (Rectangle, Rectangle) {
    let r0 = layout.index_to_pos(r.start as i32);
    let last = if r.len > 0 {
        r.start + r.len - 1
    } else {
        r.start
    };
    let r1 = layout.index_to_pos(last as i32);
    (r0, r1)
}

fn paint_form_controls(cr: Cr, b: BoxRef<'_>, layout: &Layout, text_x: f64, y_origin: f64) {
    let (mut opt_minx, mut opt_maxx, mut opt_miny, mut opt_maxy) = (1e9, -1e9, 1e9, -1e9);
    let mut opt_count = 0;
    let mut opt_sel: Vec<(f64, f64)> = Vec::new();
    for r in b.attrs() {
        if !is_field(r.kind) && r.kind != k::BUTTON {
            continue;
        }
        let (r0, r1) = char_span(layout, r);
        let dom = attr_node(r);
        if node_named(dom, b"option") {
            let rx0 = text_x + f64::from(r0.x) / S;
            let rx1 = text_x + f64::from(r1.x + r1.width) / S;
            let ry0 = y_origin + f64::from(r0.y) / S;
            let ry1 = y_origin + f64::from(r0.y + r0.height) / S;
            if rx0 < opt_minx {
                opt_minx = rx0;
            }
            if rx1 > opt_maxx {
                opt_maxx = rx1;
            }
            if ry0 < opt_miny {
                opt_miny = ry0;
            }
            if ry1 > opt_maxy {
                opt_maxy = ry1;
            }
            opt_count += 1;
            if dom.and_then(|d| d.attr(c"selected")).is_some() && opt_sel.len() < 64 {
                opt_sel.push((ry0, ry1));
            }
            continue;
        }
        let sized = r.box_w > 0.0 || r.box_h > 0.0;
        let bleed_x = if sized { 0.0 } else { 10.0 };
        let bleed_y = if sized { 0.0 } else { 5.0 };
        let mut x0 = text_x + f64::from(r0.x) / S - bleed_x;
        let mut y0 = y_origin + f64::from(r0.y) / S - bleed_y;
        let mut x1 = text_x + f64::from(r1.x + r1.width) / S + bleed_x;
        let mut y1 = y_origin + f64::from(r0.y + r0.height) / S + bleed_y;
        let css_w = inline_control_css_width(r, b);
        if is_field(r.kind)
            && let Some(dom) = dom
        {
            let ty = dom.attr(c"type").map(CStr::to_bytes);
            let text_like = ty.is_none_or(|t| {
                t.is_empty()
                    || [
                        &b"text"[..],
                        b"search",
                        b"email",
                        b"url",
                        b"tel",
                        b"number",
                        b"password",
                    ]
                    .iter()
                    .any(|w| t.eq_ignore_ascii_case(w))
            });
            if text_like && r.box_w <= 0.0 {
                let n = dom
                    .attr(c"size")
                    .map_or(20, |sz| engine::parse_int(sz, 20, 4, 80));
                let pctx = layout.context();
                let fd = layout
                    .font_description()
                    .unwrap_or_else(|| pctx.font_description());
                let aw = pctx.metrics(fd).map_or(0, |m| m.approximate_char_width());
                let cell = f64::from(aw) / S;
                let want_w = cell * f64::from(n) + 20.0;
                let cur_w = x1 - x0;
                if want_w > cur_w {
                    x1 = x0 + want_w;
                }
            }
        }
        let is_textarea = node_named(dom, b"textarea");
        if css_w <= 0.0 && r.kind == k::BUTTON {
            let mnw = inline_control_css_min_width(r, b);
            if mnw > 0.0 && x1 - x0 < mnw {
                let cx = (x0 + x1) / 2.0;
                x0 = cx - mnw / 2.0;
                x1 = cx + mnw / 2.0;
            }
        }
        if css_w > 0.0 {
            x0 = text_x + f64::from(r0.x) / S;
            x1 = x0 + css_w;
        } else if is_field(r.kind)
            && b.content_width() > 0.0
            && b.parent()
                .is_some_and(|p| core::ptr::eq(p.dom_ptr(), r.dom_ptr()))
        {
            let tlen = b.text().map_or(0, |t| t.to_bytes().len());
            if r.start <= 3 && r.start + r.len >= tlen {
                let fill_x1 = text_x + b.content_width();
                if fill_x1 > x1 {
                    x1 = fill_x1;
                }
            }
        }
        if r.box_h > 0.0 {
            if is_textarea {
                y1 = y0 + r.box_h;
                let text_bottom = y_origin + f64::from(r1.y + r1.height) / S + 3.0;
                if text_bottom > y1 {
                    y1 = text_bottom;
                }
            } else {
                let cy = (y0 + y1) / 2.0;
                y0 = cy - r.box_h / 2.0;
                y1 = cy + r.box_h / 2.0;
            }
        }
        if x1 < x0 {
            core::mem::swap(&mut x0, &mut x1);
        }
        let mut field_box = None;
        let mut p = Some(b);
        while let Some(cur) = p {
            if !cur.dom_ptr().is_null() {
                field_box = Some(cur);
                break;
            }
            p = cur.parent();
        }
        let block_chrome = field_box.is_some_and(|fb| core::ptr::eq(fb.dom_ptr(), r.dom_ptr()))
            && style_has_inline_box_paint(attr_style(r));
        let mut draw_native = r.native_chrome != 0 && !block_chrome;
        if r.kind == k::BUTTON
            && dom.is_some_and(|d| d.attr(c"class").is_some())
            && r.box_w <= 0.0
            && r.box_h <= 0.0
        {
            draw_native = false;
        }
        if !draw_native && !block_chrome {
            paint_inline_css_chrome(cr, Some(r), (x0, y0, x1 - x0, y1 - y0));
        }
        if draw_native {
            cr.save();
            if r.kind == k::BUTTON {
                cr.set_source_rgb(0.902, 0.902, 0.902);
            } else {
                cr.set_source_rgb(1.0, 1.0, 1.0);
            }
            cr.rectangle(x0, y0, x1 - x0, y1 - y0);
            cr.fill();
            cr.set_source_rgb(0.722, 0.722, 0.722);
            cr.set_line_width(1.0);
            cr.rectangle(x0 + 0.5, y0 + 0.5, x1 - x0 - 1.0, y1 - y0 - 1.0);
            cr.stroke();
            cr.restore();
        }
        if r.kind == k::INPUT_FIELD_FOCUSED && draw_native {
            cr.save();
            cr.set_source_rgb(0.13, 0.36, 0.80);
            cr.set_line_width(2.0);
            cr.rectangle(x0 + 0.5, y0 + 0.5, x1 - x0 - 1.0, y1 - y0 - 1.0);
            cr.stroke();
            cr.restore();
        }
    }
    if opt_count > 0 && opt_maxx > opt_minx {
        let px = opt_minx - 6.0;
        let pw = (opt_maxx - opt_minx) + 12.0;
        let py = opt_miny;
        let ph = opt_maxy - opt_miny;
        let pr = CornerRadii::uniform(3.0);
        cr.save();
        rounded_rect_path(cr, px + 0.5, py + 1.5, pw, ph, pr);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.12);
        cr.fill();
        rounded_rect_path(cr, px, py, pw, ph, pr);
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.fill();
        for &(sy0, sy1) in &opt_sel {
            cr.rectangle(px, sy0, pw, sy1 - sy0);
            cr.set_source_rgb(0.816, 0.886, 0.988);
            cr.fill();
        }
        rounded_rect_path(cr, px + 0.5, py + 0.5, pw - 1.0, ph - 1.0, pr);
        cr.set_source_rgb(0.70, 0.72, 0.75);
        cr.set_line_width(1.0);
        cr.stroke();
        cr.restore();
    }
}

fn paint_carets(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    s: Option<StyleRef<'_>>,
    text_x: f64,
    y_origin: f64,
) {
    let attrs = b.attrs();
    for r in attrs {
        if r.kind != k::CARET || !state::caret_visible() {
            continue;
        }
        if b.text().is_some_and(|t| r.start >= t.to_bytes().len()) {
            continue;
        }
        let pos = layout.index_to_pos(r.start as i32);
        let cx = text_x + f64::from(pos.x) / S;
        let cy = y_origin + f64::from(pos.y) / S;
        let mut ch = f64::from(pos.height) / S;
        if ch < 1.0 {
            ch = 14.0;
        }
        cr.save();
        let mut cstyle = s;
        for f in attrs {
            if is_field(f.kind)
                && !f.style().is_null()
                && f.start <= r.start
                && r.start <= f.start + f.len
            {
                cstyle = attr_style(f);
                break;
            }
        }
        let cc = get(cstyle, P::CaretColor).and_then(ValueRef::color);
        let tc = get(cstyle, P::Color).and_then(ValueRef::color);
        match cc.or(tc) {
            Some(c) => cr.set_source_rgb(
                f64::from(c[0]) / 255.0,
                f64::from(c[1]) / 255.0,
                f64::from(c[2]) / 255.0,
            ),
            None => cr.set_source_rgb(0.0, 0.0, 0.0),
        }
        cr.set_line_width(1.5);
        cr.move_to(cx + 0.5, cy);
        cr.line_to(cx + 0.5, cy + ch);
        cr.stroke();
        cr.restore();
    }
}

fn accent_color(s: Option<StyleRef<'_>>) -> Rgba {
    let ac = get(s, P::AccentColor).filter(|v| v.kind() == Kind::Color);
    rgba_of(ac, Rgba::new(0.13, 0.36, 0.80, 1.0))
}

fn glyph_box(layout: &Layout, r: &InlineAttr, text_x: f64, y_origin: f64) -> (f64, f64, f64, f64) {
    let (r0, r1) = char_span(layout, r);
    let mut gx0 = text_x + f64::from(r0.x) / S;
    let gy0 = y_origin + f64::from(r0.y) / S;
    let mut gx1 = text_x + f64::from(r1.x + r1.width) / S;
    let gy1 = y_origin + f64::from(r0.y + r0.height) / S;
    if gx1 < gx0 {
        core::mem::swap(&mut gx0, &mut gx1);
    }
    (gx0, gy0, gx1, gy1)
}

fn paint_check_controls(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    s: Option<StyleRef<'_>>,
    text_x: f64,
    y_origin: f64,
) {
    let font_size = length_or(get(s, P::FontSize), 16.0);
    let accent = accent_color(s);
    for r in b.attrs() {
        if !matches!(
            r.kind,
            k::CHECKBOX | k::CHECKBOX_CHECKED | k::RADIO | k::RADIO_CHECKED
        ) {
            continue;
        }
        let (gx0, gy0, gx1, gy1) = glyph_box(layout, r, text_x, y_origin);
        let mut side = font_size * 0.82;
        if r.box_w > 0.0 || r.box_h > 0.0 {
            let bw = if r.box_w > 0.0 { r.box_w } else { r.box_h };
            let bh = if r.box_h > 0.0 { r.box_h } else { r.box_w };
            side = if bw < bh { bw } else { bh };
        }
        let bx = gx0 + ((gx1 - gx0) - side) / 2.0;
        let by = gy0 + ((gy1 - gy0) - side) / 2.0;
        let radio = r.kind == k::RADIO || r.kind == k::RADIO_CHECKED;
        let checked = r.kind == k::CHECKBOX_CHECKED || r.kind == k::RADIO_CHECKED;
        cr.save();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        if radio {
            cr.new_sub_path();
            cr.arc(bx + side / 2.0, by + side / 2.0, side / 2.0, 0.0, 2.0 * PI);
        } else {
            cr.rectangle(bx, by, side, side);
        }
        cr.fill_preserve();
        cr.set_source_rgb(0.45, 0.45, 0.45);
        cr.set_line_width(1.0);
        cr.stroke();
        if checked {
            accent.set_source(cr);
            if radio {
                let rdot = side * 0.30;
                cr.new_sub_path();
                cr.arc(bx + side / 2.0, by + side / 2.0, rdot, 0.0, 2.0 * PI);
                cr.fill();
            } else {
                cr.rectangle(bx, by, side, side);
                cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(side * 0.18);
                cr.set_line_cap(cairo::LINE_CAP_ROUND);
                cr.move_to(bx + side * 0.20, by + side * 0.55);
                cr.line_to(bx + side * 0.42, by + side * 0.78);
                cr.line_to(bx + side * 0.80, by + side * 0.28);
                cr.stroke();
            }
        }
        cr.restore();
    }
}

fn paint_meters(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    s: Option<StyleRef<'_>>,
    text_x: f64,
    y_origin: f64,
) {
    let accent = accent_color(s);
    for r in b.attrs() {
        if r.kind != k::PROGRESS && r.kind != k::METER {
            continue;
        }
        let (gx0, gy0, gx1, gy1) = glyph_box(layout, r, text_x, y_origin);
        let pad_x = 2.0;
        let bx = gx0 + pad_x;
        let mut bw = gx1 - gx0 - pad_x * 2.0;
        if bw < 4.0 {
            bw = 4.0;
        }
        let mut bh = (gy1 - gy0) * 0.55;
        if bh < 6.0 {
            bh = 6.0;
        }
        let by = gy0 + ((gy1 - gy0) - bh) / 2.0;
        let radius = bh / 2.0;
        cr.save();
        cr.new_sub_path();
        cr.arc(bx + radius, by + radius, radius, PI / 2.0, 3.0 * PI / 2.0);
        cr.arc(bx + bw - radius, by + radius, radius, -PI / 2.0, PI / 2.0);
        cr.close_path();
        cr.set_source_rgb(0.88, 0.88, 0.90);
        cr.fill_preserve();
        cr.clip();
        let mut frac = r.font_size_px;
        if frac > 1.0 {
            frac = 1.0;
        }
        let mut fill = accent;
        if r.kind == k::METER && r.a != 0 {
            fill = Rgba::from_bytes([r.r, r.g, r.b, r.a]);
        }
        fill.set_source(cr);
        if r.kind == k::PROGRESS && frac < 0.0 {
            let iw = bw * 0.35;
            let ix = bx + (bw - iw) / 2.0;
            cr.rectangle(ix, by, iw, bh);
        } else {
            if frac < 0.0 {
                frac = 0.0;
            }
            cr.rectangle(bx, by, bw * frac, bh);
        }
        cr.fill();
        cr.restore();
    }
}

fn paint_inline_atomics(
    cr: Cr,
    b: BoxRef<'_>,
    layout: &Layout,
    text_x: f64,
    highlight: Option<&CStr>,
) {
    let Some(atomics) = b.inline_atomics() else {
        return;
    };
    let by_lines = b.atomic_line_heights().is_some();
    for (i, a) in atomics.iter().enumerate() {
        let Some(ab) = a.box_ref() else {
            continue;
        };
        let pos = layout.index_to_pos(a.byte_off() as i32);
        let sx = text_x + f64::from(pos.x) / S;
        let sy = if by_lines {
            ab.y() - ab.rel_dy()
        } else {
            b.y() + f64::from(pos.y) / S
        };
        b.set_atomic_owner_offset(i, sx - b.x(), sy - b.y());
        cr.save();
        cr.translate(sx + ab.rel_dx() - ab.x(), sy + ab.rel_dy() - ab.y());
        engine::paint_walk_atomic(cr, ab, highlight);
        cr.restore();
    }
}

fn debug_text(cr: Cr, b: BoxRef<'_>, color: Rgba, text_x: f64, y_origin: f64) {
    let Some((dx, dy)) = state::debug_point() else {
        return;
    };
    let Some(text) = b.text() else {
        return;
    };
    let (px0, py0) = cr.user_to_device(b.x(), b.y());
    let (px1, py1) = cr.user_to_device(b.x() + b.content_width(), b.y() + b.content_height());
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    if dx >= px0 && dx <= px1 && dy >= py0 && dy <= py1 {
        let (cx0, cy0, cx1, cy1) = cr.clip_extents();
        let bytes = text.to_bytes();
        let shown = String::from_utf8_lossy(&bytes[..bytes.len().min(30)]);
        let line = format!(
            "[paint-at] TEXT \"{shown}\" rgba({:.2},{:.2},{:.2},{:.2}) at {text_x:.0},{y_origin:.0} clip={cx0:.0},{cy0:.0}..{cx1:.0},{cy1:.0} grp={}\n",
            color.r,
            color.g,
            color.b,
            color.a,
            i32::from(cr.group_target() != cr.target())
        );
        southstar_glib::stderr_write(line.as_bytes());
    }
}

fn paint_vertical(
    cr: Cr,
    b: BoxRef<'_>,
    s: Option<StyleRef<'_>>,
    color: Rgba,
    highlight: Option<&CStr>,
) {
    if b.text_orient() == 1 {
        let Some(text) = b.text() else {
            return;
        };
        let stacked = engine::vertical_stack_text(text);
        let Some(layout) = paint_inline_make_layout(b, s, highlight) else {
            return;
        };
        layout.clear_attributes();
        layout.set_width(-1);
        layout.set_alignment(pango::ALIGN_CENTER);
        layout.set_text(&stacked);
        drop(stacked);
        cr.save();
        color.set_source(cr);
        cr.move_to(b.x(), b.y());
        layout.show(cr);
        cr.restore();
        return;
    }
    let Some(layout) = paint_inline_make_layout(b, s, highlight) else {
        return;
    };
    layout.set_width(-1);
    color.set_source(cr);
    let mut it = layout.iter();
    let mut acc = 0.0;
    loop {
        let line = it.line();
        let logical = it.line_logical_extents();
        let baseline = it.baseline();
        let line_h = f64::from(logical.height) / S;
        let ascent = f64::from(baseline - logical.y) / S;
        let col_left = if b.vertical_wm() == 2 {
            b.x() + acc
        } else {
            b.x() + b.content_width() - acc - line_h
        };
        cr.save();
        cr.translate(col_left + line_h, b.y());
        cr.rotate(FRAC_PI_2);
        cr.move_to(0.0, ascent);
        if let Some(line) = line {
            line.show(cr);
        }
        cr.restore();
        acc += line_h;
        if !it.next_line() {
            break;
        }
    }
}

pub fn paint_inline(cr: Cr, b: BoxRef<'_>, highlight: Option<&CStr>) {
    let Some(text) = b.text().filter(|t| !t.is_empty()) else {
        return;
    };
    let _ = text;
    let s = inherited_style(b);
    let color = rgba_anim(
        Some(b),
        ANIM_TARGET_COLOR,
        get(s, P::Color),
        Rgba::new(0.07, 0.07, 0.07, 1.0),
    );
    if b.vertical_wm() != 0 {
        paint_vertical(cr, b, s, color, highlight);
        return;
    }
    let mut text_x = b.x();
    let ti = engine::inline_text_indent_px(b, s, b.content_width());
    if ti < 0.0 {
        text_x += ti;
    }
    let cacheable = highlight.is_none_or(CStr::is_empty);
    let cached = if cacheable {
        unsafe { Layout::from_borrowed(b.paint_layout()) }
    } else {
        None
    };
    let layout = match cached {
        Some(l) => l,
        None => {
            let Some(l) = paint_inline_make_layout(b, s, highlight) else {
                return;
            };
            l
        }
    };
    if cacheable && b.paint_layout().is_null() {
        b.set_paint_layout(layout.new_ref());
    }
    let y_offset = inline_y_offset_for_layout(b, &layout);
    let y_origin = b.y() + y_offset;

    paint_inline_element_boxes(cr, b, &layout, text_x, y_origin);
    if b.has_attrs() {
        paint_form_controls(cr, b, &layout, text_x, y_origin);
    }
    if let Some(sl) = get(s, P::TextShadow).and_then(ValueRef::shadows) {
        for si in (0..sl.n.max(0) as usize).rev() {
            paint_text_shadow_layer(cr, &layout, text_x, y_origin, &sl.s[si]);
        }
    }
    debug_text(cr, b, color, text_x, y_origin);
    let sel_run = engine::selection_run(b);
    if let Some(run) = sel_run {
        paint_selection_background(cr, b, &layout, text_x, y_origin, run);
    }
    cr.save();
    color.set_source(cr);
    if !paint_inline_lines_at_layout_heights(cr, b, &layout, text_x) {
        cr.move_to(text_x, y_origin);
        layout.show(cr);
    }
    cr.restore();
    if let Some(run) = sel_run {
        paint_selection_foreground(cr, b, &layout, text_x, y_origin, run);
    }
    paint_inline_dashed_decorations(cr, b, &layout, text_x, y_origin, s, color);
    if b.has_attrs() {
        paint_carets(cr, b, &layout, s, text_x, y_origin);
        paint_check_controls(cr, b, &layout, s, text_x, y_origin);
        paint_meters(cr, b, &layout, s, text_x, y_origin);
    }
    paint_inline_atomics(cr, b, &layout, text_x, highlight);
}

pub fn sync_inline_atomic_offsets(root: BoxRef<'_>) {
    if let Some(atomics) = root.inline_atomics()
        && root.text().is_some_and(|t| !t.is_empty())
    {
        let s = inherited_style(root);
        if let Some(layout) = paint_inline_make_layout(root, s, None) {
            let mut text_x = 0.0;
            let ti = engine::inline_text_indent_px(root, s, root.content_width());
            if ti < 0.0 {
                text_x = ti;
            }
            let by_lines = root.atomic_line_heights().is_some();
            for (i, atomic) in atomics.iter().enumerate() {
                let pos = layout.index_to_pos(atomic.byte_off() as i32);
                let x = text_x + f64::from(pos.x) / S;
                let y = match atomic.box_ref() {
                    Some(ab) if by_lines => ab.y() - ab.rel_dy() - root.y(),
                    _ => f64::from(pos.y) / S,
                };
                root.set_atomic_owner_offset(i, x, y);
            }
        }
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        sync_inline_atomic_offsets(c);
        child = c.next_sibling();
    }
    if let Some(atomics) = root.inline_atomics() {
        for a in atomics {
            if let Some(ab) = a.box_ref() {
                sync_inline_atomic_offsets(ab);
            }
        }
    }
}

pub fn inline_xy_to_byte(b: BoxRef<'_>, rel_x: f64, rel_y: f64) -> Option<usize> {
    let text = b.text().filter(|t| !t.is_empty())?;
    let layout = build_inline_layout(b)?;
    let y_offset = inline_y_offset_for_layout(b, &layout);
    let mut layout_y = rel_y - y_offset;
    if layout_y < 0.0 {
        layout_y = 0.0;
    }
    let (index, trailing) = layout.xy_to_index((rel_x * S) as i32, (layout_y * S) as i32);
    let bytes = text.to_bytes();
    let tlen = bytes.len();
    let bi = if (index as usize) <= tlen {
        index as usize
    } else {
        tlen
    };
    let off = engine::utf8_offset_to_byte(bytes, bi, trailing);
    Some(if off <= tlen { off } else { tlen })
}

pub fn inline_word_range(b: BoxRef<'_>, byte: usize) -> Option<(usize, usize)> {
    let text = b.text().filter(|t| !t.is_empty())?;
    let tlen = text.to_bytes().len();
    let byte = byte.min(tlen);
    let layout = build_inline_layout(b)?;
    let attrs = layout.log_attrs();
    let ltext = layout.text()?.to_bytes();
    let n = attrs.len() as i64;
    if n <= 1 || ltext.len() != tlen {
        return None;
    }
    let mut here = engine::utf8_pointer_to_offset(ltext, byte) as i64;
    if here < 0 {
        here = 0;
    }
    if here > n - 1 {
        here = n - 1;
    }
    let (mut s, mut e) = (here as usize, here as usize);
    while s > 0 && !attrs[s].is_word_start() {
        s -= 1;
    }
    while (e as i64) < n - 1 && !attrs[e].is_word_end() {
        e += 1;
    }
    if e <= s {
        return None;
    }
    let sp = engine::utf8_offset_to_byte(ltext, 0, s as i32);
    let ep = engine::utf8_offset_to_byte(ltext, 0, e as i32);
    Some((sp, ep))
}

pub fn inline_range_extents(
    b: BoxRef<'_>,
    start: usize,
    len: usize,
    element: Option<&InlineAttr>,
) -> Option<(f64, f64, f64, f64)> {
    let text = b.text().filter(|t| !t.is_empty())?;
    if len == 0 {
        return None;
    }
    let text_len = text.to_bytes().len();
    if start >= text_len {
        return None;
    }
    let len = if start + len > text_len {
        text_len - start
    } else {
        len
    };
    let box_style = element
        .filter(|_| b.vertical_wm() == 0)
        .and_then(attr_style);
    let layout = if box_style.is_some() {
        paint_inline_make_layout(b, inherited_style(b), None)
    } else {
        build_inline_layout(b)
    }?;
    let y_origin = b.y() + inline_y_offset_for_layout(b, &layout);
    let range = InlineRange {
        start,
        len,
        box_style,
        raised: box_style.is_some() && element.is_some_and(|e| inline_element_is_raised(b, e)),
    };
    let mut u: Option<(f64, f64, f64, f64)> = None;
    inline_range_fragments(b, &layout, y_origin, &range, |f| {
        u = Some(match u {
            None => (f.x0, f.y0, f.x1, f.y1),
            Some((x0, y0, x1, y1)) => (
                if f.x0 < x0 { f.x0 } else { x0 },
                if f.y0 < y0 { f.y0 } else { y0 },
                if f.x1 > x1 { f.x1 } else { x1 },
                if f.y1 > y1 { f.y1 } else { y1 },
            ),
        });
    });
    let (x0, y0, x1, y1) = u?;
    Some((x0, y0 - b.y(), x1 - x0, y1 - y0))
}
