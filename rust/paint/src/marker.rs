//! Southstar — list item markers: ordinals with start, value and reversed, the counter styles, ::marker content, and painting outside markers as text, glyph shapes or images.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::CStr;
use std::collections::HashMap;
use std::ffi::CString;

use southstar_dom::{Kind as NodeKind, Node, NsNode};
use southstar_image::ImageRef;
use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId as P, StyleRef, display_of};

use crate::ffi::cairo::Cr;
use crate::ffi::engine::{self, Texture};
use crate::util::{
    Rgba, get, is_space, keyword_is, length_or, rgba_of, skip_spaces, style_of, truncate,
};

type OrdinalTable = HashMap<*const NsNode, i32>;

struct Ordinals {
    scope: u32,
    tables: Option<(OrdinalTable, OrdinalTable)>,
}

impl Ordinals {
    fn new() -> Ordinals {
        Ordinals {
            scope: 0,
            tables: None,
        }
    }
}

thread_local! {
    static ORDINALS: RefCell<Ordinals> = RefCell::new(Ordinals::new());
}

pub fn ordinals_begin() {
    ORDINALS.with(|o| {
        let mut o = o.borrow_mut();
        o.scope += 1;
        if o.scope == 1 {
            o.tables = Some((HashMap::new(), HashMap::new()));
        }
    });
}

pub fn ordinals_end() {
    ORDINALS.with(|o| {
        let mut o = o.borrow_mut();
        if o.scope == 0 {
            return;
        }
        o.scope -= 1;
        if o.scope == 0 {
            o.tables = None;
        }
    });
}

fn is_li(n: Node<'_>) -> bool {
    n.element_name() == Some(b"li")
}

fn children(parent: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(parent.first_child(), |c| c.next_sibling())
}

fn list_item_count(parent: Node<'_>) -> i32 {
    children(parent).filter(|&c| is_li(c)).count() as i32
}

fn list_start(parent: Node<'_>) -> (i32, bool) {
    let start_attr = parent.attr(c"start");
    let start = start_attr.map_or(1, |s| engine::parse_int(s, 1, -1_000_000, 1_000_000));
    let reversed = parent.attr(c"reversed").is_some();
    let current = if reversed && start_attr.is_none() {
        list_item_count(parent)
    } else {
        start
    };
    (current, reversed)
}

fn item_ordinal(item: Node<'_>, current: i32) -> i32 {
    item.attr(c"value").map_or(current, |v| {
        engine::parse_int(v, current, -1_000_000, 1_000_000)
    })
}

fn number_children(parent: Node<'_>, ordinals: &mut HashMap<*const NsNode, i32>) -> i32 {
    let (mut current, reversed) = list_start(parent);
    for p in children(parent).filter(|&c| is_li(c)) {
        let ordinal = item_ordinal(p, current);
        ordinals.insert(p.as_ptr(), ordinal);
        current = ordinal + if reversed { -1 } else { 1 };
    }
    current
}

fn list_item_ordinal(li: Option<Node<'_>>) -> i32 {
    let Some(li) = li else {
        return 1;
    };
    let Some(parent) = li.parent().filter(|p| p.name().is_some()) else {
        return 1;
    };
    let cached = ORDINALS.with(|o| {
        let mut o = o.borrow_mut();
        let (ordinals, next) = o.tables.as_mut()?;
        let next_value = match next.get(&parent.as_ptr()) {
            Some(&n) => n,
            None => {
                let n = number_children(parent, ordinals);
                next.insert(parent.as_ptr(), n);
                n
            }
        };
        Some(ordinals.get(&li.as_ptr()).copied().unwrap_or(next_value))
    });
    if let Some(ordinal) = cached {
        return ordinal;
    }
    let (mut current, reversed) = list_start(parent);
    for p in children(parent).filter(|&c| is_li(c)) {
        let ordinal = item_ordinal(p, current);
        if p == li {
            return ordinal;
        }
        current = ordinal + if reversed { -1 } else { 1 };
    }
    current
}

fn roman_numeral(n: i32, upper: bool) -> Vec<u8> {
    const VALS: [i32; 13] = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
    const UPPER: [&str; 13] = [
        "M", "CM", "D", "CD", "C", "XC", "L", "XL", "X", "IX", "V", "IV", "I",
    ];
    const LOWER: [&str; 13] = [
        "m", "cm", "d", "cd", "c", "xc", "l", "xl", "x", "ix", "v", "iv", "i",
    ];
    if !(1..=3999).contains(&n) {
        return n.to_string().into_bytes();
    }
    let mut n = n;
    let mut s = String::new();
    for (i, &v) in VALS.iter().enumerate() {
        while n >= v {
            s.push_str(if upper { UPPER[i] } else { LOWER[i] });
            n -= v;
        }
    }
    s.into_bytes()
}

fn alpha_label(n: i32, upper: bool) -> Vec<u8> {
    if n < 1 {
        return b"?".to_vec();
    }
    let mut n = n;
    let mut buf = Vec::new();
    while n > 0 && buf.len() < 15 {
        n -= 1;
        buf.push((if upper { b'A' } else { b'a' }) + (n % 26) as u8);
        n /= 26;
    }
    buf.reverse();
    buf
}

fn greek_label(n: i32) -> Vec<u8> {
    const GREEK: [u32; 24] = [
        0x3B1, 0x3B2, 0x3B3, 0x3B4, 0x3B5, 0x3B6, 0x3B7, 0x3B8, 0x3B9, 0x3BA, 0x3BB, 0x3BC, 0x3BD,
        0x3BE, 0x3BF, 0x3C0, 0x3C1, 0x3C3, 0x3C4, 0x3C5, 0x3C6, 0x3C7, 0x3C8, 0x3C9,
    ];
    if n < 1 {
        return n.to_string().into_bytes();
    }
    let mut v = n;
    let mut chars = Vec::new();
    while v > 0 && chars.len() < 16 {
        v -= 1;
        chars.push(GREEK[(v % 24) as usize]);
        v /= 24;
    }
    let s: String = chars
        .iter()
        .rev()
        .filter_map(|&c| char::from_u32(c))
        .collect();
    truncate(s.into_bytes(), 32)
}

fn format_ordered_label(kind: Option<&[u8]>, n: i32) -> Vec<u8> {
    let label = match kind {
        Some(b"lower-greek") => greek_label(n),
        Some(b"upper-alpha" | b"upper-latin") => alpha_label(n, true),
        Some(b"lower-alpha" | b"lower-latin") => alpha_label(n, false),
        Some(b"upper-roman") => roman_numeral(n, true),
        Some(b"lower-roman") => roman_numeral(n, false),
        Some(b"decimal-leading-zero") => format!("{n:02}").into_bytes(),
        _ => n.to_string().into_bytes(),
    };
    truncate(label, 32)
}

fn ordered_marker_kind(style_kw: Option<&CStr>) -> bool {
    matches!(
        style_kw.map(CStr::to_bytes),
        Some(
            b"decimal"
                | b"decimal-leading-zero"
                | b"upper-alpha"
                | b"lower-alpha"
                | b"upper-latin"
                | b"lower-latin"
                | b"upper-roman"
                | b"lower-roman"
                | b"lower-greek"
        )
    )
}

fn marker_is_ordered(li: Option<Node<'_>>, style_kw: Option<&CStr>) -> bool {
    match style_kw.map(CStr::to_bytes) {
        None => li
            .and_then(|l| l.parent())
            .and_then(|p| p.name())
            .is_some_and(|n| n.to_bytes() == b"ol"),
        Some(kw) => {
            kw != b"disc"
                && kw != b"circle"
                && kw != b"square"
                && kw != b"none"
                && !kw.starts_with(b"disclosure-")
        }
    }
}

fn ordered_kind_from_type_attr(type_attr: Option<&CStr>) -> Option<&'static [u8]> {
    let first = *type_attr?.to_bytes().first()?;
    Some(match first {
        b'A' => b"upper-alpha",
        b'a' => b"lower-alpha",
        b'I' => b"upper-roman",
        b'i' => b"lower-roman",
        _ => b"decimal",
    })
}

fn marker_default_kind<'a>(li: Option<Node<'a>>, style_kw: Option<&'a CStr>) -> Option<&'a [u8]> {
    if ordered_marker_kind(style_kw) {
        return style_kw.map(CStr::to_bytes);
    }
    let parent = li.and_then(|l| l.parent());
    if let Some(parent) = parent.filter(|p| p.name().is_some_and(|n| n.to_bytes() == b"ol")) {
        return ordered_kind_from_type_attr(parent.attr(c"type"));
    }
    style_kw.map(CStr::to_bytes)
}

fn marker_counter_text(body: &[u8], li: Option<Node<'_>>, style_kw: Option<&CStr>) -> Vec<u8> {
    let mut p = skip_spaces(body, 0);
    let name_s = p;
    while p < body.len() && !matches!(body[p], b',' | b')') && !is_space(body[p]) {
        p += 1;
    }
    if p - name_s != 9 || !body[name_s..p].eq_ignore_ascii_case(b"list-item") {
        return Vec::new();
    }
    p = skip_spaces(body, p);
    let mut style = None;
    if body.get(p) == Some(&b',') {
        p = skip_spaces(body, p + 1);
        let style_s = p;
        while p < body.len() && body[p] != b')' && !is_space(body[p]) {
            p += 1;
        }
        if p != style_s {
            style = Some(body[style_s..p].to_ascii_lowercase());
        }
    }
    let kind = match &style {
        Some(s) => Some(s.as_slice()),
        None => marker_default_kind(li, style_kw),
    };
    format_ordered_label(kind, list_item_ordinal(li))
}

fn marker_resolve_content_text(
    raw: &[u8],
    li: Option<Node<'_>>,
    style_kw: Option<&CStr>,
) -> Vec<u8> {
    let has_func = raw.contains(&b'(');
    let has_string = raw.contains(&b'"') || raw.contains(&b'\'');
    if !has_func && !has_string {
        return raw.to_vec();
    }
    let mut out = Vec::new();
    let mut p = 0;
    while p < raw.len() {
        p = skip_spaces(raw, p);
        if p >= raw.len() {
            break;
        }
        if raw[p] == b'"' || raw[p] == b'\'' {
            let q = raw[p];
            p += 1;
            let start = p;
            while p < raw.len() && raw[p] != q {
                if raw[p] == b'\\' && p + 1 < raw.len() {
                    p += 2;
                    continue;
                }
                p += 1;
            }
            out.extend_from_slice(&engine::css_unescape(&raw[start..p.min(raw.len())]));
            if p < raw.len() && raw[p] == q {
                p += 1;
            }
        } else if raw[p..].starts_with(b"attr(") {
            p = skip_spaces(raw, p + 5);
            let start = p;
            while p < raw.len() && raw[p] != b')' && raw[p] != b',' && !is_space(raw[p]) {
                p += 1;
            }
            if let Some(li) = li.filter(|_| p != start)
                && let Ok(name) = CString::new(&raw[start..p])
                && let Some(val) = li.attr(&name)
            {
                out.extend_from_slice(val.to_bytes());
            }
            while p < raw.len() && raw[p] != b')' {
                p += 1;
            }
            if p < raw.len() {
                p += 1;
            }
        } else if raw[p..].starts_with(b"counter(") {
            p += 8;
            let body_s = p;
            let mut depth = 1;
            while p < raw.len() && depth > 0 {
                if raw[p] == b'(' {
                    depth += 1;
                } else if raw[p] == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                p += 1;
            }
            let body = &raw[body_s..p];
            if p < raw.len() && raw[p] == b')' {
                p += 1;
            }
            out.extend_from_slice(&marker_counter_text(body, li, style_kw));
        } else {
            let start = p;
            while p < raw.len() && !is_space(raw[p]) && raw[p] != b'"' && raw[p] != b'\'' {
                p += 1;
            }
            out.extend_from_slice(&raw[start..p]);
        }
    }
    out
}

enum Custom {
    Text(Vec<u8>),
    Suppressed,
    Default,
}

fn marker_custom_text(
    li: Option<Node<'_>>,
    li_style: Option<StyleRef<'_>>,
    style_kw: Option<&CStr>,
) -> Custom {
    let Some(ms) = li_style.and_then(StyleRef::marker) else {
        return Custom::Default;
    };
    if display_of(Some(ms)).is_none() {
        return Custom::Suppressed;
    }
    let Some(raw) = ms
        .get(P::Content)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(|v| v.keyword_text())
    else {
        return Custom::Default;
    };
    match raw.to_bytes() {
        b"normal" => Custom::Default,
        b"none" | b"no-open-quote" | b"no-close-quote" => Custom::Suppressed,
        b"open-quote" => Custom::Text("\u{201c}".as_bytes().to_vec()),
        b"close-quote" => Custom::Text("\u{201d}".as_bytes().to_vec()),
        raw => Custom::Text(marker_resolve_content_text(raw, li, style_kw)),
    }
}

fn li_generates_marker(li: Option<Node<'_>>, li_style: Option<StyleRef<'_>>) -> bool {
    let Some(li) = li else {
        return false;
    };
    if li.kind() != NodeKind::Element {
        return false;
    }
    let Some(name) = li.name() else {
        return false;
    };
    if li.parent().and_then(|p| p.name()).is_none() {
        return false;
    }
    let display = get(li_style, P::Display)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(|v| v.keyword_text());
    match display {
        None => name.to_bytes() == b"li",
        Some(d) => d.to_bytes().windows(9).any(|w| w == b"list-item"),
    }
}

fn disclosure_glyph(style_kw: Option<&CStr>, rtl: bool) -> Option<&'static str> {
    let kw = style_kw?.to_bytes().strip_prefix(b"disclosure-")?;
    if kw == b"open" {
        return Some("\u{25be}");
    }
    Some(if rtl { "\u{25c2}" } else { "\u{25b8}" })
}

pub fn li_is_inside(li_style: Option<StyleRef<'_>>) -> bool {
    get(li_style, P::ListStylePosition)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(|v| v.keyword_text())
        .is_some_and(|k| k.to_bytes() == b"inside")
}

fn list_style_type(style: Option<StyleRef<'_>>) -> Option<&CStr> {
    get(style, P::ListStyleType)
        .filter(|v| v.kind() == Kind::Keyword)
        .and_then(|v| v.keyword_text())
}

pub fn li_marker_text(
    li: Option<Node<'_>>,
    li_style: Option<StyleRef<'_>>,
    out_sz: usize,
) -> Option<Vec<u8>> {
    if !li_generates_marker(li, li_style) || out_sz < 8 {
        return None;
    }
    let style_kw = list_style_type(li_style);
    match marker_custom_text(li, li_style, style_kw) {
        Custom::Text(text) => return Some(truncate(text, out_sz)),
        Custom::Suppressed => return Some(Vec::new()),
        Custom::Default => {}
    }
    if style_kw.is_some_and(|k| k.to_bytes() == b"none") {
        return Some(Vec::new());
    }
    if marker_is_ordered(li, style_kw) {
        let n = list_item_ordinal(li);
        let mut label = format_ordered_label(marker_default_kind(li, style_kw), n);
        label.extend_from_slice(b". ");
        return Some(truncate(label, out_sz));
    }
    let rtl = keyword_is(get(li_style, P::Direction), c"rtl");
    let glyph = match (
        disclosure_glyph(style_kw, rtl),
        style_kw.map(CStr::to_bytes),
    ) {
        (Some(g), _) => g,
        (None, Some(b"square")) => "\u{25aa}",
        (None, Some(b"circle")) => "\u{25cb}",
        (None, _) => "\u{2022}",
    };
    let mut text = glyph.as_bytes().to_vec();
    text.push(b' ');
    Some(truncate(text, out_sz))
}

fn paint_marker_label(
    cr: Cr,
    label: &[u8],
    edge_x: f64,
    rtl: bool,
    baseline_y: f64,
    font_size: f64,
) {
    if label.is_empty() {
        return;
    }
    let Ok(label) = CString::new(label) else {
        return;
    };
    cr.set_font_size(font_size);
    let ext = cr.text_extents(&label);
    let x = if rtl {
        edge_x + font_size * 0.35 - ext.x_bearing
    } else {
        edge_x - font_size * 0.35 - ext.width - ext.x_bearing
    };
    cr.move_to(x, baseline_y);
    cr.show_text(&label);
}

fn marker_image(b: BoxRef<'_>, s: Option<StyleRef<'_>>) -> Option<Texture> {
    let media = b.media()?;
    let image = unsafe { ImageRef::from_ptr(media.marker_image()) }?;
    if get(s, P::ListStyleImage).is_none_or(|v| v.kind() != Kind::Url) {
        return None;
    }
    let texture = unsafe { Texture::from_raw(image.texture()) }?;
    (image.loaded() && texture.width() > 0 && texture.height() > 0).then_some(texture)
}

pub fn paint_marker(cr: Cr, b: BoxRef<'_>) {
    let s = style_of(b);
    let dom = unsafe { Node::from_ptr(b.dom_ptr().cast()) };
    if !li_generates_marker(dom, s) || li_is_inside(s) {
        return;
    }
    let style_kw = list_style_type(s);
    let custom = match marker_custom_text(dom, s, style_kw) {
        Custom::Suppressed => return,
        Custom::Text(t) => Some(t),
        Custom::Default => None,
    };
    let marker_img = if custom.is_none() {
        marker_image(b, s)
    } else {
        None
    };
    if style_kw.is_some_and(|k| k.to_bytes() == b"none") && custom.is_none() && marker_img.is_none()
    {
        return;
    }
    let ms = s.and_then(StyleRef::marker);
    let mut font_size = length_or(get(s, P::FontSize), 16.0);
    if let Some(mfs) = get(ms, P::FontSize) {
        font_size = mfs.length_or(font_size);
    }
    let margin = b.margin();
    let padding = b.padding();
    let cy = b.y() + margin.top + padding.top + font_size * 0.7;
    let rtl = engine::node_dir_is_rtl(dom);
    let content_x = b.x() + margin.left + padding.left;
    let edge_x = if rtl {
        content_x + b.content_width()
    } else {
        content_x
    };
    let cx = if rtl {
        edge_x + font_size * 0.8
    } else {
        edge_x - font_size * 0.8
    };

    if let Some(texture) = marker_img {
        let iw = texture.width();
        let ih = texture.height();
        let mut dw = f64::from(iw);
        let mut dh = f64::from(ih);
        let cap = font_size;
        if dh > cap {
            dw *= cap / dh;
            dh = cap;
        }
        if let Some(surf) = crate::media::texture_surface_cached(texture, None) {
            let dx = if rtl {
                edge_x + font_size * 0.35
            } else {
                edge_x - font_size * 0.35 - dw
            };
            let dy = cy - dh + font_size * 0.15;
            cr.save();
            cr.translate(dx, dy);
            cr.scale(dw / f64::from(iw), dh / f64::from(ih));
            cr.set_source_surface(surf, 0.0, 0.0);
            cr.paint();
            cr.restore();
            return;
        }
    }
    let cval = get(ms, P::Color).or_else(|| get(s, P::Color));
    rgba_of(cval, Rgba::new(0.1, 0.1, 0.1, 1.0)).set_source(cr);

    if let Some(custom) = custom {
        paint_marker_label(cr, &custom, edge_x, rtl, cy, font_size);
    } else if marker_is_ordered(dom, style_kw) {
        let n = list_item_ordinal(dom);
        let mut label = format_ordered_label(marker_default_kind(dom, style_kw), n);
        label.push(b'.');
        let label = truncate(label, 40);
        paint_marker_label(cr, &label, edge_x, rtl, cy, font_size);
    } else if disclosure_glyph(style_kw, rtl).is_some() {
        let open = style_kw.is_some_and(|k| k.to_bytes() == b"disclosure-open");
        let sz = font_size * 0.3;
        let ty = cy - font_size * 0.32;
        let dir = if rtl { -1.0 } else { 1.0 };
        cr.new_path();
        if open {
            cr.move_to(cx - sz, ty - sz * 0.5);
            cr.line_to(cx + sz, ty - sz * 0.5);
            cr.line_to(cx, ty + sz * 0.7);
        } else {
            cr.move_to(cx - dir * sz * 0.5, ty - sz);
            cr.line_to(cx - dir * sz * 0.5, ty + sz);
            cr.line_to(cx + dir * sz * 0.7, ty);
        }
        cr.close_path();
        cr.fill();
    } else if style_kw.is_some_and(|k| k.to_bytes() == b"square") {
        let sz = font_size * 0.32;
        cr.new_path();
        cr.rectangle(cx - sz / 2.0, cy - font_size * 0.32 - sz / 2.0, sz, sz);
        cr.fill();
    } else if style_kw.is_some_and(|k| k.to_bytes() == b"circle") {
        cr.new_sub_path();
        cr.arc(
            cx,
            cy - font_size * 0.32,
            font_size * 0.18,
            0.0,
            2.0 * core::f64::consts::PI,
        );
        cr.set_line_width(1.0);
        cr.stroke();
    } else {
        cr.new_sub_path();
        cr.arc(
            cx,
            cy - font_size * 0.32,
            font_size * 0.18,
            0.0,
            2.0 * core::f64::consts::PI,
        );
        cr.fill();
    }
}
