//! Southstar — presentational hints: the declarations an element's legacy HTML attributes (bgcolor, width and height, align, border, cellpadding, font color, face and size, body margins, table rules and frame, ...) and SVG presentation attributes stand for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{FLAG_SVG_NS, Kind, Node, attrs, controls};

use crate::color;
use crate::ffi;
use crate::scan::{strip, utf8_char};

const QUIRKS: u32 = 1 << 5;
const INT_HALF_MAX: i32 = i32::MAX / 2;
const INT_HALF_MIN: i32 = i32::MIN / 2;

const RULES_GROUPS: i32 = 2;
const RULES_ROWS: i32 = 3;
const RULES_COLS: i32 = 4;
const RULES_ALL: i32 = 5;

const SVG_PRESENTATION_ATTRS: [&[u8]; 18] = [
    b"fill",
    b"fill-opacity",
    b"fill-rule",
    b"clip-rule",
    b"stroke",
    b"stroke-width",
    b"stroke-opacity",
    b"stroke-linecap",
    b"stroke-linejoin",
    b"stroke-miterlimit",
    b"stroke-dasharray",
    b"stroke-dashoffset",
    b"paint-order",
    b"vector-effect",
    b"text-anchor",
    b"stop-color",
    b"stop-opacity",
    b"visibility",
];

const GENERIC_FAMILIES: [&[u8]; 13] = [
    b"serif",
    b"sans-serif",
    b"monospace",
    b"cursive",
    b"fantasy",
    b"system-ui",
    b"ui-serif",
    b"ui-sans-serif",
    b"ui-monospace",
    b"ui-rounded",
    b"math",
    b"emoji",
    b"fangsong",
];

fn is_html_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn skip_html_space(s: &[u8]) -> &[u8] {
    &s[s.iter().take_while(|&&c| is_html_space(c)).count()..]
}

fn eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn attr<'a>(el: Node<'a>, name: &CStr) -> Option<&'a CStr> {
    attrs::get(el, name)
}

fn attr_bytes<'a>(el: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    attr(el, name).map(CStr::to_bytes)
}

fn non_empty<'a>(el: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    attr_bytes(el, name).filter(|v| !v.is_empty())
}

fn named(node: Option<Node<'_>>, tag: &[u8]) -> bool {
    node.and_then(Node::element_name) == Some(tag)
}

fn parse_int(value: Option<&CStr>, default: i32, min: i32, max: i32) -> i32 {
    controls::parse_int(value, default, min, max)
}

fn legacy_color(input: &[u8]) -> (u8, u8, u8) {
    let mut s = Vec::with_capacity(input.len());
    let mut p = 0;
    while p < input.len() {
        let (c, next) = utf8_char(input, p);
        if c > 0xffff {
            s.extend_from_slice(b"00");
        } else {
            s.extend_from_slice(&input[p..next]);
        }
        p = next;
    }
    let mut hex = Vec::new();
    let mut p = 0;
    let mut i = 0;
    while p < s.len() && i < 128 {
        let (c, next) = utf8_char(&s, p);
        if !(i == 0 && c == u32::from(b'#')) {
            let digit = u8::try_from(c).ok().filter(u8::is_ascii_hexdigit);
            hex.push(digit.unwrap_or(b'0'));
        }
        p = next;
        i += 1;
    }
    if hex.is_empty() {
        hex.push(b'0');
    }
    while hex.len() % 3 != 0 {
        hex.push(b'0');
    }
    let comp = hex.len() / 3;
    let (c0, c1, c2) = (&hex[..comp], &hex[comp..2 * comp], &hex[2 * comp..]);
    let (mut off, mut len) = (0, comp);
    if len > 8 {
        off = len - 8;
        len = 8;
    }
    while len > 2 && c0[off] == b'0' && c1[off] == b'0' && c2[off] == b'0' {
        off += 1;
        len -= 1;
    }
    let len = len.min(2);
    let channel = |c: &[u8]| {
        c[off..off + len].iter().fold(0u32, |v, &d| {
            v * 16 + char::from(d).to_digit(16).unwrap_or(0)
        }) as u8
    };
    (channel(c0), channel(c1), channel(c2))
}

fn attr_color(value: &[u8]) -> Option<[u8; 4]> {
    let start = value.iter().position(|&c| !is_html_space(c))?;
    let end = value.iter().rposition(|&c| !is_html_space(c))? + 1;
    let stripped = &value[start..end];
    let text = CString::new(stripped).unwrap_or_default();
    let mut channels: color::Channels = [None; 4];
    if color::parse_into(&text, &mut channels) {
        return Some(channels.map(|c| c.unwrap_or(0)));
    }
    let (r, g, b) = legacy_color(stripped);
    Some([r, g, b, 255])
}

fn rgba(out: &mut Vec<u8>, prop: &str, [r, g, b, a]: [u8; 4]) {
    out.extend_from_slice(format!("{prop}: rgba({r},{g},{b},").as_bytes());
    out.extend_from_slice(&ffi::format_double(c"%g", f64::from(a) / 255.0));
    out.extend_from_slice(b");");
}

fn html_dimension(value: Option<&[u8]>, ignore_zero: bool) -> Option<(f64, bool)> {
    let s = skip_html_space(value?);
    if !s.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut p = 0;
    let mut v = 0.0;
    while p < s.len() && s[p].is_ascii_digit() {
        v = v * 10.0 + f64::from(s[p] - b'0');
        p += 1;
    }
    if p < s.len() && s[p] == b'.' {
        p += 1;
        let mut divisor = 1.0;
        while p < s.len() && s[p].is_ascii_digit() {
            divisor *= 10.0;
            v += f64::from(s[p] - b'0') / divisor;
            p += 1;
        }
    }
    if ignore_zero && v == 0.0 {
        return None;
    }
    Some((v, s.get(p) == Some(&b'%')))
}

fn length_g(v: f64) -> Vec<u8> {
    ffi::format_double(c"%.10g", v)
}

fn append_dimension(out: &mut Vec<u8>, value: Option<&[u8]>, ignore_zero: bool, props: &[&str]) {
    let Some((v, percent)) = html_dimension(value, ignore_zero) else {
        return;
    };
    let unit: &[u8] = if percent { b"%" } else { b"px" };
    for prop in props {
        out.extend_from_slice(prop.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(&length_g(v));
        out.extend_from_slice(unit);
        out.push(b';');
    }
}

fn cell_nowrap_quirk(cell: Node<'_>) -> bool {
    cell.root().flags() & QUIRKS != 0
        && html_dimension(attr_bytes(cell, c"width"), true).is_some_and(|(_, pct)| !pct)
}

fn table_rules_kind(rules: Option<&[u8]>) -> i32 {
    let Some(rules) = rules else {
        return 0;
    };
    [&b"none"[..], b"groups", b"rows", b"cols", b"all"]
        .iter()
        .position(|name| eq(rules, name))
        .map_or(0, |i| i as i32 + 1)
}

fn table_frame_border_style(frame: Option<&[u8]>) -> Option<&'static str> {
    const FRAMES: [(&[u8], &str); 9] = [
        (b"void", "hidden"),
        (b"above", "outset hidden hidden hidden"),
        (b"below", "hidden hidden outset hidden"),
        (b"hsides", "outset hidden outset hidden"),
        (b"lhs", "hidden hidden hidden outset"),
        (b"rhs", "hidden outset hidden hidden"),
        (b"vsides", "hidden outset"),
        (b"box", "outset"),
        (b"border", "outset"),
    ];
    let frame = frame?;
    FRAMES
        .iter()
        .find(|(name, _)| eq(frame, name))
        .map(|&(_, style)| style)
}

fn table_of_part(el: Node<'_>) -> Option<Node<'_>> {
    let some = Some(el);
    let mut p = el.parent();
    let is_cell = named(some, b"td") || named(some, b"th");
    if is_cell {
        if !named(p, b"tr") {
            return None;
        }
        p = p.and_then(Node::parent);
    }
    if (is_cell || named(some, b"tr"))
        && (named(p, b"thead") || named(p, b"tbody") || named(p, b"tfoot"))
    {
        p = p.and_then(Node::parent);
    }
    p.filter(|&p| named(Some(p), b"table"))
}

fn img_dimension_source(img: Node<'_>) -> Node<'_> {
    let picture = img.parent();
    if !named(picture, b"picture") {
        return img;
    }
    let mut child = picture.and_then(Node::first_child);
    while let Some(c) = child.filter(|&c| c != img) {
        child = c.next_sibling();
        if !named(Some(c), b"source") || non_empty(c, c"srcset").is_none() {
            continue;
        }
        if non_empty(c, c"media").is_some_and(|media| !ffi::media_query_matches(media)) {
            continue;
        }
        if attr(c, c"type").is_some_and(|ty| !ty.is_empty() && !ffi::image_supports_mime(ty)) {
            continue;
        }
        return if attr(c, c"width").is_some() || attr(c, c"height").is_some() {
            c
        } else {
            img
        };
    }
    img
}

fn legacy_font_size_keyword(value: Option<&[u8]>) -> Option<&'static str> {
    const KEYWORDS: [&str; 7] = [
        "x-small",
        "small",
        "medium",
        "large",
        "x-large",
        "xx-large",
        "xxx-large",
    ];
    let mut s = skip_html_space(value?);
    let sign = match s.first() {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => 0,
    };
    if sign != 0 {
        s = &s[1..];
    }
    if !s.first().is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0;
    for &c in s.iter().take_while(|c| c.is_ascii_digit()) {
        if value >= 100 {
            break;
        }
        value = value * 10 + i32::from(c - b'0');
    }
    if sign != 0 {
        value = 3 + sign * value;
    }
    Some(KEYWORDS[value.clamp(1, 7) as usize - 1])
}

fn is_svg_presentation_attr(name: &[u8]) -> bool {
    SVG_PRESENTATION_ATTRS.contains(&name)
}

fn append_svg_presentation_hints(out: &mut Vec<u8>, el: Node<'_>) {
    for a in el.attrs() {
        let (Some(name), Some(value)) = (a.name(), a.value()) else {
            continue;
        };
        let name = name.to_bytes();
        if !is_svg_presentation_attr(name) {
            continue;
        }
        let value = strip(value.to_bytes());
        if value.is_empty() || value.iter().any(|c| b";{}!\\".contains(c)) {
            continue;
        }
        let text = CString::new(value).unwrap_or_default();
        let (_, end) = ffi::strtod(&text, 0);
        let unitless_length = end != 0
            && end == value.len()
            && (name == b"stroke-width" || name == b"stroke-dashoffset");
        out.extend_from_slice(name);
        out.extend_from_slice(b": ");
        out.extend_from_slice(value);
        if unitless_length {
            out.extend_from_slice(b"px");
        }
        out.push(b';');
    }
}

pub(crate) fn is_presentational_attr(name: &[u8]) -> bool {
    let Some(&first) = name.first() else {
        return false;
    };
    if is_svg_presentation_attr(name) {
        return true;
    }
    let names: &[&[u8]] = match first.to_ascii_lowercase() {
        b'a' => &[b"align"],
        b'b' => &[b"bgcolor", b"bordercolor", b"background", b"border"],
        b'c' => &[b"color", b"cellspacing", b"cellpadding"],
        b'f' => &[b"face", b"frame", b"frameborder"],
        b'h' => &[b"height", b"hspace"],
        b'l' => &[b"leftmargin"],
        b'm' => &[b"marginheight", b"marginwidth"],
        b'n' => &[b"nowrap", b"noshade"],
        b'r' => &[b"rules"],
        b's' => &[b"size"],
        b't' => &[b"text", b"topmargin", b"type"],
        b'v' => &[b"valign", b"vspace"],
        b'w' => &[b"width", b"wrap"],
        _ => &[],
    };
    names.iter().any(|n| eq(name, n))
}

fn lower(value: &[u8]) -> Vec<u8> {
    value.to_ascii_lowercase()
}

fn push(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
}

fn list_style_hints(out: &mut Vec<u8>, el: Node<'_>, tag: &[u8]) {
    let ty = attr_bytes(el, c"type");
    if tag == b"ol" || tag == b"li" {
        let style = match ty {
            Some(b"1") => Some("decimal"),
            Some(b"a") => Some("lower-alpha"),
            Some(b"A") => Some("upper-alpha"),
            Some(b"i") => Some("lower-roman"),
            Some(b"I") => Some("upper-roman"),
            _ => None,
        };
        if let Some(style) = style {
            push(out, &format!("list-style-type: {style};"));
        }
    }
    if tag == b"ul" || tag == b"li" {
        let style = ty.and_then(|t| {
            ["disc", "circle", "square", "none"]
                .into_iter()
                .find(|s| eq(t, s.as_bytes()))
        });
        if let Some(style) = style {
            push(out, &format!("list-style-type: {style};"));
        }
    }
}

fn background_hint(out: &mut Vec<u8>, background: &[u8]) {
    out.extend_from_slice(b"background-image: url(\"");
    for &c in background {
        if c == b'"' || c == b'\\' {
            out.push(b'\\');
        }
        if matches!(c, b'\n' | b'\r' | 0x0c) {
            continue;
        }
        out.push(c);
    }
    out.extend_from_slice(b"\");");
}

fn body_hints(out: &mut Vec<u8>, el: Node<'_>) {
    let mut doc = Some(el);
    while let Some(node) = doc.filter(|node| node.kind() != Kind::Document) {
        doc = node.parent();
    }
    let container = doc
        .and_then(Node::parent)
        .filter(|&c| named(Some(c), b"iframe") || named(Some(c), b"frame"));
    let margins = [
        ("margin-top", "margin-bottom", c"marginheight", c"topmargin"),
        ("margin-left", "margin-right", c"marginwidth", c"leftmargin"),
    ];
    for (start, end, name, alt) in margins {
        let v = attr(el, name)
            .or_else(|| attr(el, alt))
            .or_else(|| container.and_then(|c| attr(c, name)));
        let px = if v.is_some() {
            parse_int(v, -1, -1, INT_HALF_MAX)
        } else {
            -1
        };
        if px >= 0 {
            push(out, &format!("{start}: {px}px; {end}: {px}px;"));
        }
    }
    if let Some(color) = non_empty(el, c"text").and_then(attr_color) {
        rgba(out, "color", color);
    }
}

fn font_hints(out: &mut Vec<u8>, el: Node<'_>) {
    if let Some(color) = non_empty(el, c"color").and_then(attr_color) {
        rgba(out, "color", color);
    }
    if let Some(face) = non_empty(el, c"face") {
        if GENERIC_FAMILIES.iter().any(|g| eq(face, g)) {
            out.extend_from_slice(b"font-family: ");
            out.extend_from_slice(face);
            out.push(b';');
        } else {
            out.extend_from_slice(b"font-family: \"");
            for &c in face {
                if c == b'\\' || c == b'"' {
                    out.push(b'\\');
                    out.push(c);
                } else if c < 0x20 || c == 0x7f {
                    push(out, &format!("\\{c:X} "));
                } else {
                    out.push(c);
                }
            }
            out.extend_from_slice(b"\";");
        }
    }
    if let Some(size) = legacy_font_size_keyword(attr_bytes(el, c"size")) {
        push(out, &format!("font-size: {size};"));
    }
}

fn table_hints(out: &mut Vec<u8>, el: Node<'_>) {
    if table_rules_kind(attr_bytes(el, c"rules")) != 0 {
        out.extend_from_slice(b"border-style: hidden;border-collapse: collapse;");
    }
    if let Some(border) = attr(el, c"border") {
        let w = parse_int(Some(border), -1, -1, INT_HALF_MAX);
        push(
            out,
            &format!("border-width: {}px;", if w < 0 { 1 } else { w }),
        );
        if w != 0 {
            out.extend_from_slice(b"border-style: outset;");
        }
    }
    if let Some(style) = table_frame_border_style(attr_bytes(el, c"frame")) {
        push(out, &format!("border-style: {style};"));
    }
    if let Some(color) = non_empty(el, c"bordercolor").and_then(attr_color) {
        rgba(out, "border-color", color);
    }
    let spacing = parse_int(attr(el, c"cellspacing"), -1, -1, INT_HALF_MAX);
    if spacing >= 0 {
        push(out, &format!("border-spacing: {spacing}px;"));
    }
}

fn cell_hints(out: &mut Vec<u8>, el: Node<'_>, part_table: Option<Node<'_>>, part_rules: i32) {
    let mut table = el.parent();
    while let Some(t) = table {
        if t.is_element() && t.name().is_some_and(|n| eq(n.to_bytes(), b"table")) {
            break;
        }
        table = t.parent();
    }
    let padding = parse_int(
        table.and_then(|t| attr(t, c"cellpadding")),
        -1,
        -1,
        INT_HALF_MAX,
    );
    if padding >= 0 {
        push(out, &format!("padding: {padding}px;"));
    }
    let border = part_table.and_then(|t| attr(t, c"border"));
    if border.is_some() && parse_int(border, -1, -1, INT_HALF_MAX) != 0 {
        out.extend_from_slice(b"border-width: 1px; border-style: inset;");
    }
    let color = part_table.and_then(|t| non_empty(t, c"bordercolor"));
    if let Some(color) = color.filter(|_| border.is_some() || part_rules != 0) {
        if let Some(color) = attr_color(color) {
            rgba(out, "border-color", color);
        }
    }
    match part_rules {
        RULES_COLS => out.extend_from_slice(
            b"border-width: 1px;border-block-style: none;border-inline-style: solid;",
        ),
        RULES_ALL => out.extend_from_slice(b"border-width: 1px; border-style: solid;"),
        RULES_ROWS => out.extend_from_slice(
            b"border-width: 1px;border-block-style: solid;border-inline-style: none;",
        ),
        0 => {}
        _ => out.extend_from_slice(b"border-width: 1px; border-style: none;"),
    }
    if attr(el, c"nowrap").is_some() && !cell_nowrap_quirk(el) {
        out.extend_from_slice(b"white-space: nowrap;");
    }
}

fn table_part_align_hints(out: &mut Vec<u8>, el: Node<'_>) {
    if let Some(align) = non_empty(el, c"align").map(lower) {
        match align.as_slice() {
            b"middle" | b"absmiddle" => out.extend_from_slice(b"text-align: center;"),
            b"left" | b"center" | b"right" | b"justify" => {
                out.extend_from_slice(b"text-align: ");
                out.extend_from_slice(&align);
                out.push(b';');
            }
            _ => {}
        }
    }
    if let Some(valign) = non_empty(el, c"valign").map(lower) {
        if matches!(
            valign.as_slice(),
            b"top" | b"middle" | b"bottom" | b"baseline"
        ) {
            out.extend_from_slice(b"vertical-align: ");
            out.extend_from_slice(&valign);
            out.push(b';');
        }
    }
}

fn block_align_hints(out: &mut Vec<u8>, el: Node<'_>, is_table: bool) {
    let Some(align) = non_empty(el, c"align").map(lower) else {
        return;
    };
    match align.as_slice() {
        b"left" | b"right" if is_table => {
            out.extend_from_slice(b"float: ");
            out.extend_from_slice(&align);
            out.push(b';');
        }
        b"center" if is_table => {
            out.extend_from_slice(b"margin-left: auto; margin-right: auto;");
        }
        b"left" | b"center" | b"right" | b"justify" => {
            out.extend_from_slice(b"text-align: ");
            out.extend_from_slice(&align);
            out.push(b';');
        }
        _ => {}
    }
}

fn img_align_hints(out: &mut Vec<u8>, el: Node<'_>) {
    let Some(align) = non_empty(el, c"align").map(lower) else {
        return;
    };
    match align.as_slice() {
        b"left" | b"right" => {
            out.extend_from_slice(b"float: ");
            out.extend_from_slice(&align);
            out.push(b';');
        }
        b"top" | b"bottom" => {
            out.extend_from_slice(b"vertical-align: ");
            out.extend_from_slice(&align);
            out.push(b';');
        }
        b"middle" | b"center" | b"absmiddle" => {
            out.extend_from_slice(b"vertical-align: middle;");
        }
        _ => {}
    }
}

fn hr_hints(out: &mut Vec<u8>, el: Node<'_>) {
    if let Some(align) = non_empty(el, c"align").map(lower) {
        match align.as_slice() {
            b"center" => out.extend_from_slice(b"margin-left: auto; margin-right: auto;"),
            b"left" => out.extend_from_slice(b"margin-left: 0; margin-right: auto;"),
            b"right" => out.extend_from_slice(b"margin-left: auto; margin-right: 0;"),
            _ => {}
        }
    }
    let color = non_empty(el, c"color");
    if let Some(rgba_color) = color.and_then(attr_color) {
        rgba(out, "color", rgba_color);
        rgba(out, "background-color", rgba_color);
    }
    if non_empty(el, c"size").is_some() {
        let v = parse_int(attr(el, c"size"), 0, 0, 1000);
        if v > 0 {
            push(out, &format!("height: {v}px;"));
        }
    }
    if attr(el, c"noshade").is_some() && color.is_none() {
        out.extend_from_slice(b"background-color: #808080;");
    }
}

pub(crate) fn presentational_hints(el: Node<'_>) -> Option<Vec<u8>> {
    let tag = el.element_name()?;
    let any = matches!(
        tag,
        b"td" | b"th" | b"body" | b"tr" | b"thead" | b"tbody" | b"tfoot" | b"colgroup"
    ) || (tag == b"img" && named(el.parent(), b"picture"))
        || el.attrs().any(|a| {
            a.name()
                .is_some_and(|n| is_presentational_attr(n.to_bytes()))
        });
    if !any {
        return None;
    }
    let mut out = Vec::new();
    let is_table = tag == b"table";
    let is_cell = tag == b"td" || tag == b"th";
    let is_row = tag == b"tr";
    let is_table_part =
        is_cell || is_row || matches!(tag, b"thead" | b"tbody" | b"tfoot" | b"col" | b"colgroup");
    let is_img = tag == b"img";
    let is_hr = tag == b"hr";
    let is_body = tag == b"body";
    let is_iframe = tag == b"iframe";
    let is_video = tag == b"video";
    let is_image_input =
        tag == b"input" && attr_bytes(el, c"type").is_some_and(|t| eq(t, b"image"));
    let is_embedded = is_img
        || is_image_input
        || is_iframe
        || is_video
        || matches!(tag, b"object" | b"embed" | b"marquee");

    list_style_hints(&mut out, el, tag);
    if let Some(background) = non_empty(el, c"background") {
        if is_body || is_table || is_table_part {
            background_hint(&mut out, background);
        }
    }
    if let Some(color) = non_empty(el, c"bgcolor").and_then(attr_color) {
        rgba(&mut out, "background-color", color);
    }
    if is_body {
        body_hints(&mut out, el);
    }
    if tag == b"font" {
        font_hints(&mut out, el);
    }

    let dim_source = if is_img { img_dimension_source(el) } else { el };
    let width = attr_bytes(dim_source, c"width");
    if width.is_some() && (is_embedded || is_hr || matches!(tag, b"col" | b"colgroup" | b"pre")) {
        append_dimension(&mut out, width, false, &["width"]);
    } else if width.is_some() && (is_table || is_cell) {
        append_dimension(&mut out, width, true, &["width"]);
    }
    let height = attr_bytes(dim_source, c"height");
    if height.is_some() && (is_embedded || is_table || is_row) {
        append_dimension(&mut out, height, false, &["height"]);
    } else if height.is_some() && is_cell {
        append_dimension(&mut out, height, true, &["height"]);
    }
    if is_iframe {
        let frameborder = attr(el, c"frameborder");
        if frameborder.is_some() && parse_int(frameborder, 0, INT_HALF_MIN, INT_HALF_MAX) == 0 {
            out.extend_from_slice(b"border-width: 0;");
        }
    }
    if is_img || is_video || is_image_input {
        let w = html_dimension(width, false).filter(|&(_, pct)| !pct);
        let h = html_dimension(height, false).filter(|&(_, pct)| !pct);
        if let (Some((w, _)), Some((h, _))) = (w, h) {
            out.extend_from_slice(b"aspect-ratio: auto ");
            out.extend_from_slice(&length_g(w));
            out.extend_from_slice(b" / ");
            out.extend_from_slice(&length_g(h));
            out.push(b';');
        }
    }
    if is_embedded && !is_iframe && !is_video {
        append_dimension(
            &mut out,
            attr_bytes(el, c"hspace"),
            false,
            &["margin-left", "margin-right"],
        );
        append_dimension(
            &mut out,
            attr_bytes(el, c"vspace"),
            false,
            &["margin-top", "margin-bottom"],
        );
    }
    if tag == b"canvas" && width.is_some() && height.is_some() {
        let cw = parse_int(attr(el, c"width"), 0, 0, i32::MAX);
        let ch = parse_int(attr(el, c"height"), 0, 0, i32::MAX);
        if cw > 0 && ch > 0 {
            push(&mut out, &format!("aspect-ratio: auto {cw} / {ch};"));
        }
    }
    if is_table {
        table_hints(&mut out, el);
    }
    let part_table = if is_table_part {
        table_of_part(el)
    } else {
        None
    };
    let part_rules = part_table.map_or(0, |t| table_rules_kind(attr_bytes(t, c"rules")));
    if is_cell {
        cell_hints(&mut out, el, part_table, part_rules);
    } else if (part_rules == RULES_GROUPS && tag != b"tr" && tag != b"colgroup")
        || (part_rules == RULES_ROWS && is_row)
    {
        out.extend_from_slice(b"border-block-width: 1px;border-block-style: solid;");
    } else if part_rules == RULES_GROUPS && tag == b"colgroup" {
        out.extend_from_slice(b"border-inline-width: 1px;border-inline-style: solid;");
    }
    if is_table_part {
        table_part_align_hints(&mut out, el);
    }
    if is_table
        || matches!(
            tag,
            b"p" | b"div" | b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6"
        )
    {
        block_align_hints(&mut out, el, is_table);
    }
    if is_img {
        img_align_hints(&mut out, el);
    }
    if is_img || is_image_input || tag == b"object" {
        let v = parse_int(attr(el, c"border"), 0, 0, INT_HALF_MAX);
        if v > 0 {
            push(&mut out, &format!("border: {v}px solid;"));
        }
    }
    if tag == b"legend" {
        let align = attr_bytes(el, c"align")
            .filter(|a| [&b"left"[..], b"center", b"right"].iter().any(|w| eq(a, w)));
        if let Some(align) = align {
            out.extend_from_slice(b"justify-self: ");
            out.extend_from_slice(&lower(align));
            out.push(b';');
        }
    }
    if is_hr {
        hr_hints(&mut out, el);
    }
    if tag == b"textarea" && attr_bytes(el, c"wrap").is_some_and(|w| eq(w, b"off")) {
        out.extend_from_slice(b"white-space: pre;");
    }
    if el.flags() & FLAG_SVG_NS != 0 {
        append_svg_presentation_hints(&mut out, el);
    }
    (!out.is_empty()).then_some(out)
}
